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
use gpui::{
    App, IntoElement, ParentElement as _, SharedString, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme, dock::{DockPlacement, PanelView}, h_flex, input::{Input, InputState},
    v_flex,
};

use crate::state::AppState;

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