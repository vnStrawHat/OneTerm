//! Shared helpers used by the SSH connect dialogs.
//!
//! Contains:
//! - [`field`] — form field renderer (label + input).
//! - [`server_info_banner`] — read-only banner showing `ssh://…`.
//! - [`parse_user_host_port`] — parse `user@host:port` strings.
//! - [`add_ssh_terminal_to_dock`] — add a terminal panel to the DockArea center.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, ClickEvent, Context, FocusHandle, IntoElement, ParentElement as _, Render, SharedString,
    Styled, Window, div,
};
use gpui_component::dock::{DockPlacement, PanelView};
use gpui_component::{
    ActiveTheme, Disableable as _, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    notification::NotificationType,
    v_flex,
};
use oneterm_core::{AppError, ConnectionCancellation, HostKeyPolicy, SshConfig};
use oneterm_state::commands::SshDuplicateCompletion;
use oneterm_state::notif_ext::notify;
use oneterm_state::{AppServices, AppState};
use oneterm_terminal::PtySize;
use oneterm_terminal_view::TerminalPanel;

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

/// Whether a form field must be filled in. Controls the required marker (`*`).
pub(crate) enum FieldRequirement {
    Required,
    Optional,
}

/// Render one form field: label (with `*` when required) + input element.
pub(crate) fn field(
    label: &'static str,
    requirement: FieldRequirement,
    input: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let danger = cx.theme().danger;
    v_flex()
        .gap_1()
        .w_full()
        .child(
            h_flex()
                .gap_1()
                .text_sm()
                .child(SharedString::from(label))
                .when(matches!(requirement, FieldRequirement::Required), |t| {
                    t.child(div().text_color(danger).child("*"))
                }),
        )
        .child(input)
}

/// Stateful Connect button that renders a spinner and disables itself while connecting.
pub(crate) struct ConnectButton {
    action: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool>,
    connecting: Arc<AtomicBool>,
}

impl ConnectButton {
    pub(crate) fn new(
        action: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool>,
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
            .on_click(move |event, window, cx| {
                let _ = action(event, window, cx);
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

/// Parse the username field input — accepts 3 forms:
/// - `username`
/// - `username@host`
/// - `username@host:port`
///
/// Returns `(username, host, port)`. Any part missing from the input keeps
/// its default.
pub(crate) fn parse_user_host_port(
    raw: &str,
    default_host: &str,
    default_port: u16,
) -> (String, String, u16) {
    match raw.split_once('@') {
        Some((user, rest)) => match rest.rsplit_once(':') {
            Some((h, p_str)) => {
                let port = p_str.parse::<u16>().unwrap_or(default_port);
                (user.to_string(), h.to_string(), port)
            }
            None => (user.to_string(), rest.to_string(), default_port),
        },
        None => (raw.to_string(), default_host.to_string(), default_port),
    }
}

/// Build a POSIX-shell command that changes the new remote shell's directory.
fn remote_cd_command(cwd: &Path) -> String {
    let value = cwd.to_string_lossy();
    let quoted = value.replace('\'', "'\"'\"'");
    format!("cd -- '{quoted}'\r")
}

// ── Dock integration ─────────────────────────────────────────────────

/// Add the SSH terminal panel to the DockArea center.
pub(crate) fn add_ssh_terminal_to_dock(
    panel: &Arc<dyn PanelView>,
    window: &mut Window,
    cx: &mut App,
) {
    let dock_area = match AppState::global(cx).read(cx).dock_area.clone() {
        Some(d) => d,
        None => {
            log::error!("AppState.dock_area not initialized — cannot add SSH terminal tab");
            return;
        }
    };

    dock_area
        .update(cx, |dock, cx| {
            dock.add_panel(panel.clone(), DockPlacement::Center, None, window, cx);
        })
        .ok();
}

/// Connect one SSH configuration on a background thread and handle host-key
/// first-use approval without weakening changed-key rejection.
pub(crate) fn connect_ssh_session(
    cfg: SshConfig,
    label: String,
    initial_cwd: Option<PathBuf>,
    completion: Option<SshDuplicateCompletion>,
    connecting: Arc<std::sync::atomic::AtomicBool>,
    window: &mut Window,
    cx: &mut App,
) -> ConnectionCancellation {
    let cancellation = cfg.cancellation.clone();
    let duplicate_config = cfg.duplicate_config();
    let factory = AppServices::session_factory(cx);

    if cancellation.is_cancelled() {
        return cancellation;
    }

    // Retain one short-lived zeroizing copy only so an unknown key can be
    // explicitly approved and retried without asking for the password again.
    let retry_cfg = cfg.clone();
    let retry_label = label.clone();
    let retry_cwd = initial_cwd.clone();
    let retry_completion = completion.clone();
    let connecting_for_task = connecting.clone();
    let task_cancellation = cancellation.clone();
    window
        .spawn(cx, async move |cx| {
            let result = cx
                .background_executor()
                .spawn(
                    async move { factory.connect_ssh(cfg, PtySize { rows: 24, cols: 80 }, 10_000) },
                )
                .await;
            if task_cancellation.is_cancelled() {
                connecting_for_task.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }

            _ = cx.update(|window, cx| match result {
                Ok(ssh_session) => {
                    connecting_for_task.store(false, std::sync::atomic::Ordering::Relaxed);
                    window.close_dialog(cx);
                    if let Some(cwd) = initial_cwd.as_deref() {
                        ssh_session.send_text(&remote_cd_command(cwd));
                    }
                    let duplicate_config =
                        oneterm_core::SessionDuplicateConfig::Ssh(duplicate_config);
                    if let Some(completion) = completion {
                        completion(ssh_session, label.clone(), duplicate_config, window, cx);
                    } else {
                        let panel: Arc<dyn PanelView> =
                            Arc::new(TerminalPanel::from_session_entity_with_duplicate_config(
                                ssh_session,
                                &label,
                                duplicate_config,
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
                        retry_label,
                        retry_cwd,
                        retry_completion,
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
    label: String,
    initial_cwd: Option<PathBuf>,
    completion: Option<SshDuplicateCompletion>,
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
    window.open_alert_dialog(cx, move |alert, _, _| {
        let cfg = cfg.clone();
        let label = label.clone();
        let initial_cwd = initial_cwd.clone();
        let completion = completion.clone();
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
                    cfg.clone(),
                    label.clone(),
                    initial_cwd.clone(),
                    completion.clone(),
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

    #[test]
    fn remote_cd_command_quotes_spaces_and_single_quotes() {
        let command = remote_cd_command(Path::new("/srv/one term/user's"));
        assert_eq!(command, "cd -- '/srv/one term/user'\"'\"'s'\r");
    }
}
