//! Shared helpers used by the SSH connect dialogs.
//!
//! Contains:
//! - [`server_info_banner`] — read-only banner showing `ssh://…`.
//! - [`parse_user_host_port`] / [`parse_port`] — the one parser for
//!   `user[@host[:port]]` strings and port fields.
//! - [`add_ssh_terminal_to_dock`] — add a terminal panel to the DockArea center.
//! - [`SshConnectRequest`] / [`connect_ssh_session`] — the shared connect flow.
//!
//! Form field rendering lives in `oneterm_state::form_dialog::labelled_field`.

use std::cell::Cell;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gpui::{
    App, Context, FocusHandle, IntoElement, ParentElement as _, Render, SharedString, Styled,
    Window, div,
};
use gpui_component::dock::{DockPlacement, PanelView};
use gpui_component::{
    ActiveTheme, Disableable as _, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    notification::NotificationType,
};
use oneterm_core::{AppError, ConnectionCancellation, HostKeyPolicy, SshConfig};
use oneterm_settings::TerminalSettings;
use oneterm_state::commands::SshDuplicateCompletion;
use oneterm_state::{AppServices, AppState};
use oneterm_terminal::PtySize;
use oneterm_terminal_view::{PanelSpec, TerminalPanel};
use oneterm_theme::notif_ext::notify;

// ── UI helpers ───────────────────────────────────────────────────────

/// Apply one deferred initial focus without overriding later user focus changes.
pub(crate) fn defer_initial_focus_once(
    initial_focus_pending: &Cell<bool>,
    focus_handle: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) {
    if !initial_focus_pending.replace(false) {
        return;
    }

    window.defer(cx, move |window, cx| {
        focus_handle.focus(window, cx);
    });
}

/// Stateful Connect button that renders a spinner and disables itself while connecting.
pub(crate) struct ConnectButton {
    action: Rc<dyn Fn(&mut Window, &mut App) -> bool>,
    connecting: Arc<AtomicBool>,
}

impl ConnectButton {
    pub(crate) fn new(
        action: Rc<dyn Fn(&mut Window, &mut App) -> bool>,
        connecting: Arc<AtomicBool>,
    ) -> Self {
        Self { action, connecting }
    }
}

impl Render for ConnectButton {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let connecting = self.connecting.load(Ordering::Relaxed);
        let action = self.action.clone();
        Button::new("connect")
            .label(if connecting { "Connecting" } else { "Connect" })
            .primary()
            .loading(connecting)
            .disabled(connecting)
            .on_click(move |_, window, cx| {
                // The dialog closes itself once the connection attempt settles.
                let _ = action(window, cx);
            })
    }
}

/// Server info banner (read-only).
pub(crate) fn server_info_banner(info: SharedString, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .w_full()
        .px_3()
        .py_2()
        .rounded_md()
        .bg(theme.muted)
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(info)
}

// ── Parsing ──────────────────────────────────────────────────────────

/// The parts of a `user[@host[:port]]` string. Missing parts are `None`; the
/// caller decides which defaults (saved session, other form fields) fill them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UserHostPort {
    pub user: String,
    pub host: Option<String>,
    pub port: Option<u16>,
}

/// Why a `user@host:port` string or a port field was rejected. `Display` is
/// the corrective message shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UserHostPortError {
    /// `@host` — nothing before the `@`.
    EmptyUser,
    /// `user@` or `user@:22` — nothing between `@` and `:port`.
    EmptyHost,
    /// `[::1` — an opening bracket without its closing one.
    UnclosedBracket,
    /// The text after the last `:` (or in the port field) is not `1..=65535`.
    InvalidPort(String),
}

impl fmt::Display for UserHostPortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyUser => f.write_str("Username is required before the '@'."),
            Self::EmptyHost => f.write_str("Host is required after the '@'."),
            Self::UnclosedBracket => {
                f.write_str("IPv6 host must be enclosed in brackets, e.g. [::1]:22.")
            }
            Self::InvalidPort(text) => {
                write!(f, "Port {text:?} is not a number between 1 and 65535.")
            }
        }
    }
}

/// Parse `user`, `user@host` or `user@host:port`.
///
/// Policy (the one parser used by every SSH dialog):
/// - `user` alone → `host`/`port` are `None` and fall back to the caller's defaults.
/// - IPv6 hosts: `[2001:db8::1]:22` and `[2001:db8::1]` (bracketed, optional
///   port) or a bare address with several colons (`2001:db8::1`, no port).
///   Otherwise the text after the last `:` is the port.
/// - An invalid port (`user@host:abc`, `:0`, `:70000`) is an error, never a
///   silent default and never folded into the host name — the user is told
///   what to fix.
/// - Empty user (`@host`) and empty host (`user@`, `user@:22`) are errors.
pub(crate) fn parse_user_host_port(raw: &str) -> Result<UserHostPort, UserHostPortError> {
    let raw = raw.trim();
    let Some((user, rest)) = raw.split_once('@') else {
        return Ok(UserHostPort {
            user: raw.to_string(),
            host: None,
            port: None,
        });
    };
    if user.is_empty() {
        return Err(UserHostPortError::EmptyUser);
    }
    let (host, port) = split_host_port(rest)?;
    Ok(UserHostPort {
        user: user.to_string(),
        host: Some(host),
        port,
    })
}

/// Split `host`, `host:port`, `[v6]`, `[v6]:port` or a bare IPv6 address.
fn split_host_port(text: &str) -> Result<(String, Option<u16>), UserHostPortError> {
    if let Some(bracketed) = text.strip_prefix('[') {
        let Some((host, after)) = bracketed.split_once(']') else {
            return Err(UserHostPortError::UnclosedBracket);
        };
        if host.is_empty() {
            return Err(UserHostPortError::EmptyHost);
        }
        let port = match after.strip_prefix(':') {
            Some(port) => Some(parse_port(port)?),
            None if after.is_empty() => None,
            None => return Err(UserHostPortError::InvalidPort(after.to_string())),
        };
        return Ok((host.to_string(), port));
    }
    // Two or more colons without brackets: a bare IPv6 address, no port.
    if text.matches(':').count() >= 2 {
        return Ok((text.to_string(), None));
    }
    let (host, port) = match text.rsplit_once(':') {
        Some((host, port)) => (host, Some(parse_port(port)?)),
        None => (text, None),
    };
    if host.is_empty() {
        return Err(UserHostPortError::EmptyHost);
    }
    Ok((host.to_string(), port))
}

/// Parse a port field: `1..=65535`, surrounding whitespace ignored.
pub(crate) fn parse_port(text: &str) -> Result<u16, UserHostPortError> {
    let text = text.trim();
    match text.parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => Err(UserHostPortError::InvalidPort(text.to_string())),
    }
}

/// Build a POSIX-shell command that changes the new remote shell's directory.
fn remote_cd_command(cwd: &Path) -> String {
    let value = cwd.to_string_lossy();
    let quoted = value.replace('\'', "'\"'\"'");
    format!("cd -- '{quoted}'\r")
}

// ── Dock integration ─────────────────────────────────────────────────

/// Add the SSH terminal panel to the DockArea center. Reports (log +
/// notification) when the panel could not be placed, so a connected session
/// never disappears silently (ERR-09).
pub(crate) fn add_ssh_terminal_to_dock(
    panel: &Arc<dyn PanelView>,
    window: &mut Window,
    cx: &mut App,
) {
    let placed = AppState::global(cx)
        .read(cx)
        .dock_area
        .clone()
        .ok_or("the main workspace is not registered")
        .and_then(|dock_area| {
            dock_area
                .update(cx, |dock, cx| {
                    dock.add_panel(panel.clone(), DockPlacement::Center, None, window, cx);
                })
                .map_err(|_| "the main workspace has been released")
        });
    if let Err(reason) = placed {
        log::error!("add_ssh_terminal_to_dock: cannot add the SSH terminal tab — {reason}");
        window.push_notification(
            notify(
                NotificationType::Error,
                format!("Connected, but the terminal tab could not be opened: {reason}."),
                cx,
            ),
            cx,
        );
    }
}

/// Everything one SSH connection attempt needs besides the transport config.
///
/// Cloned for the host-key retry, so every field is cheap to clone.
#[derive(Clone)]
pub(crate) struct SshConnectRequest {
    /// Tab title / notification label.
    pub label: String,
    /// Directory the new remote shell changes into (duplicate-at-cwd).
    pub initial_cwd: Option<PathBuf>,
    /// Destination-aware placement for a duplicated session; `None` opens a
    /// new center tab.
    pub completion: Option<SshDuplicateCompletion>,
    /// Runs once the session is authenticated, before the tab opens — e.g. the
    /// quick-connect "save this session" option (CORR-54: never saved on failure).
    pub on_connected: Option<Rc<dyn Fn(&mut App)>>,
}

impl SshConnectRequest {
    pub(crate) fn new(label: String) -> Self {
        Self {
            label,
            initial_cwd: None,
            completion: None,
            on_connected: None,
        }
    }
}

/// Connect one SSH configuration on a background thread and handle host-key
/// first-use approval without weakening changed-key rejection.
pub(crate) fn connect_ssh_session(
    cfg: SshConfig,
    request: SshConnectRequest,
    connecting: Arc<std::sync::atomic::AtomicBool>,
    window: &mut Window,
    cx: &mut App,
) -> ConnectionCancellation {
    let cancellation = cfg.cancellation.clone();
    let duplicate_config = cfg.duplicate_config();
    let factory = AppServices::session_factory(cx);
    // SSH sessions honour the same scrollback setting as local shells (CORR-33).
    let scrollback = TerminalSettings::global(cx).read(cx).scrollback_history;
    // The user's OSC security policy for this session (SEC-08).
    let security = oneterm_terminal_view::terminal_security_policy(cx);

    if cancellation.is_cancelled() {
        return cancellation;
    }

    // Retain one short-lived zeroizing copy only so an unknown key can be
    // explicitly approved and retried without asking for the password again.
    let retry_cfg = cfg.clone();
    let retry_request = request.clone();
    let connecting_for_task = connecting.clone();
    let task_cancellation = cancellation.clone();
    window
        .spawn(cx, async move |cx| {
            let result = cx
                .background_executor()
                .spawn(
                    async move { factory.connect_ssh(cfg, PtySize::INITIAL, scrollback, security) },
                )
                .await;
            if task_cancellation.is_cancelled() {
                connecting_for_task.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }

            _ = cx.update(|window, cx| match result {
                Ok(ssh_session) => {
                    let SshConnectRequest {
                        label,
                        initial_cwd,
                        completion,
                        on_connected,
                    } = request;
                    connecting_for_task.store(false, std::sync::atomic::Ordering::Relaxed);
                    window.close_dialog(cx);
                    if let Some(on_connected) = on_connected {
                        on_connected(cx);
                    }
                    if let Some(cwd) = initial_cwd.as_deref() {
                        ssh_session.send_text(&remote_cd_command(cwd));
                    }
                    let duplicate_config =
                        oneterm_core::SessionDuplicateConfig::Ssh(duplicate_config);
                    if let Some(completion) = completion {
                        completion(ssh_session, label.clone(), duplicate_config, window, cx);
                    } else {
                        let panel: Arc<dyn PanelView> = Arc::new(TerminalPanel::open(
                            PanelSpec::Session {
                                session: ssh_session,
                                title: label.clone(),
                                duplicate_config: Some(duplicate_config),
                            },
                            window,
                            cx,
                        ));
                        add_ssh_terminal_to_dock(&panel, window, cx);
                    }
                    window.push_notification(
                        notify(
                            NotificationType::Success,
                            format!("Connected to \"{label}\"."),
                            cx,
                        ),
                        cx,
                    );
                }
                Err(AppError::HostKeyUnknown {
                    host,
                    port,
                    algorithm,
                    fingerprint,
                }) => {
                    connecting_for_task.store(false, std::sync::atomic::Ordering::Relaxed);
                    window.close_dialog(cx);
                    open_host_key_confirmation(
                        retry_cfg,
                        retry_request,
                        host,
                        port,
                        algorithm,
                        fingerprint,
                        window,
                        cx,
                    );
                }
                Err(error) => {
                    connecting_for_task.store(false, std::sync::atomic::Ordering::Relaxed);
                    window.refresh();
                    window.push_notification(
                        notify(
                            NotificationType::Error,
                            format!("SSH connect failed: {error}"),
                            cx,
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
    cancellation
}

#[allow(clippy::too_many_arguments)]
fn open_host_key_confirmation(
    mut cfg: SshConfig,
    request: SshConnectRequest,
    host: String,
    port: u16,
    algorithm: String,
    fingerprint: String,
    window: &mut Window,
    cx: &mut App,
) {
    cfg.cancellation = ConnectionCancellation::default();
    cfg.host_key_policy = HostKeyPolicy::AcceptNewFingerprint(fingerprint.clone());
    let description = format!(
        "The server is not present in your OpenSSH known_hosts file.\n\n\
         Host: {host}:{port}\nAlgorithm: {algorithm}\n\
         SHA-256 fingerprint: {fingerprint}\n\n\
         Verify this fingerprint through a trusted channel before continuing.",
    );
    // The builder closure runs on every render of the dialog: share one copy of
    // the config (it carries the secret) instead of re-cloning it each time,
    // and clone it only when the user confirms (SEC-17).
    let cfg = Rc::new(cfg);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let cfg = Rc::clone(&cfg);
        let request = request.clone();
        alert
            .confirm()
            .title("Unknown SSH Host Key")
            .description(description.clone())
            .button_props(
                DialogButtonProps::default()
                    .ok_text("Trust and Connect")
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text("Cancel")
                    .show_cancel(true),
            )
            .on_ok(move |_, window, cx| {
                connect_ssh_session(
                    SshConfig::clone(&cfg),
                    request.clone(),
                    Arc::new(AtomicBool::new(true)),
                    window,
                    cx,
                );
                true
            })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(user: &str, host: Option<&str>, port: Option<u16>) -> UserHostPort {
        UserHostPort {
            user: user.to_string(),
            host: host.map(str::to_string),
            port,
        }
    }

    #[test]
    fn user_alone_leaves_host_and_port_to_the_caller() {
        assert_eq!(parse_user_host_port("root"), Ok(parsed("root", None, None)));
        assert_eq!(
            parse_user_host_port("  root  "),
            Ok(parsed("root", None, None))
        );
        assert_eq!(parse_user_host_port(""), Ok(parsed("", None, None)));
    }

    #[test]
    fn user_host_and_optional_port() {
        assert_eq!(
            parse_user_host_port("root@example.test"),
            Ok(parsed("root", Some("example.test"), None))
        );
        assert_eq!(
            parse_user_host_port("root@example.test:2222"),
            Ok(parsed("root", Some("example.test"), Some(2222)))
        );
        assert_eq!(
            parse_user_host_port("root@10.0.0.1:22"),
            Ok(parsed("root", Some("10.0.0.1"), Some(22)))
        );
    }

    /// TEST-19: IPv6 hosts — bracketed with and without a port, and bare.
    #[test]
    fn ipv6_hosts_are_recognised() {
        assert_eq!(
            parse_user_host_port("root@[2001:db8::1]:2200"),
            Ok(parsed("root", Some("2001:db8::1"), Some(2200)))
        );
        assert_eq!(
            parse_user_host_port("root@[::1]"),
            Ok(parsed("root", Some("::1"), None))
        );
        assert_eq!(
            parse_user_host_port("root@fe80::1"),
            Ok(parsed("root", Some("fe80::1"), None))
        );
        assert_eq!(
            parse_user_host_port("root@[::1"),
            Err(UserHostPortError::UnclosedBracket)
        );
        assert_eq!(
            parse_user_host_port("root@[]:22"),
            Err(UserHostPortError::EmptyHost)
        );
        assert_eq!(
            parse_user_host_port("root@[::1]x"),
            Err(UserHostPortError::InvalidPort("x".into()))
        );
    }

    /// TEST-19: empty user / empty host are reported, not guessed.
    #[test]
    fn empty_user_or_host_is_an_error() {
        assert_eq!(
            parse_user_host_port("@example.test"),
            Err(UserHostPortError::EmptyUser)
        );
        assert_eq!(
            parse_user_host_port("root@"),
            Err(UserHostPortError::EmptyHost)
        );
        assert_eq!(
            parse_user_host_port("root@:22"),
            Err(UserHostPortError::EmptyHost)
        );
    }

    /// TEST-19 / ARCH-34: an invalid port is never a silent default and never
    /// becomes part of the host name.
    #[test]
    fn invalid_ports_are_errors() {
        assert_eq!(
            parse_user_host_port("root@host:abc"),
            Err(UserHostPortError::InvalidPort("abc".into()))
        );
        assert_eq!(
            parse_user_host_port("root@host:0"),
            Err(UserHostPortError::InvalidPort("0".into()))
        );
        assert_eq!(
            parse_user_host_port("root@host:70000"),
            Err(UserHostPortError::InvalidPort("70000".into()))
        );
        assert_eq!(parse_port(" 22 "), Ok(22));
        assert_eq!(
            parse_port(""),
            Err(UserHostPortError::InvalidPort(String::new()))
        );
        assert!(
            UserHostPortError::InvalidPort("x".into())
                .to_string()
                .contains("65535")
        );
    }

    #[test]
    fn remote_cd_command_quotes_spaces_and_single_quotes() {
        let command = remote_cd_command(Path::new("/srv/one term/user's"));
        assert_eq!(command, "cd -- '/srv/one term/user'\"'\"'s'\r");
    }
}
