//! Shared helpers used by the SSH connect dialogs.
//!
//! Contains:
//! - [`field`] — form field renderer (label + input).
//! - [`password_field`] — masked password field with toggle.
//! - [`server_info_banner`] — read-only banner showing `ssh://…`.
//! - [`parse_user_host_port`] — parse `user@host:port` strings.
//! - [`add_ssh_terminal_to_dock`] — add a terminal panel to the DockArea center.

use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{App, IntoElement, ParentElement as _, SharedString, Styled, Window, div};
use gpui_component::{
    ActiveTheme, WindowExt as _,
    button::ButtonVariant,
    dialog::DialogButtonProps,
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    v_flex,
};
use oneterm_ui::dock::{DockPlacement, PanelView};

use oneterm_core::{AppError, ConnectionCancellation, HostKeyPolicy, SshConfig};
use oneterm_state::AppState;
use oneterm_state::notif_ext::notify;
use oneterm_terminal::PtySize;
use oneterm_terminal_view::TerminalPanel;

// ── UI helpers ───────────────────────────────────────────────────────

/// Render one form field: label (with `*` if required) + input element.
pub(crate) fn field(
    label: &'static str,
    required: bool,
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
                .when(required, |t| t.child(div().text_color(danger).child("*"))),
        )
        .child(input)
}

/// Password field: label + masked input with mask_toggle + cleanable.
pub(crate) fn password_field(state: &gpui::Entity<InputState>, _cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .w_full()
        .child(
            h_flex()
                .gap_1()
                .text_sm()
                .child(SharedString::from("Password")),
        )
        .child(Input::new(state).mask_toggle().cleanable(true))
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
    window: &mut Window,
    cx: &mut App,
) -> ConnectionCancellation {
    let cancellation = cfg.cancellation.clone();
    let Some(factory) = oneterm_terminal::session_factory() else {
        window.push_notification(
            notify(
                NotificationType::Error,
                "Internal error: no session factory installed.",
                cx,
            ),
            cx,
        );
        return cancellation;
    };

    if cancellation.is_cancelled() {
        return cancellation;
    }

    // Retain one short-lived zeroizing copy only so an unknown key can be
    // explicitly approved and retried without asking for the password again.
    let retry_cfg = cfg.clone();
    let retry_label = label.clone();
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
                return;
            }

            _ =
                cx.update(|window, cx| match result {
                    Ok(ssh_session) => {
                        window.close_dialog(cx);
                        let panel: Arc<dyn PanelView> = Arc::new(
                            TerminalPanel::from_session_entity(ssh_session, &label, window, cx),
                        );
                        add_ssh_terminal_to_dock(&panel, window, cx);
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
                        window.close_dialog(cx);
                        open_host_key_confirmation(
                            retry_cfg,
                            retry_label,
                            host,
                            port,
                            algorithm,
                            fingerprint,
                            window,
                            cx,
                        );
                    }
                    Err(error) => {
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
                connect_ssh_session(cfg.clone(), label.clone(), window, cx);
                true
            })
    });
}
