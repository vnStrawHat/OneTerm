//! Agent Mode right-dock panel — placeholder for the future Agent feature.
//!
//! OneTerm's right dock content is switched by [`oneterm_core::RightDockMode`]:
//! - `SshClient` → the combined Session + SFTP [`crate::ssh_client_panel::SshClientPanel`].
//! - `Agent` → this [`AgentPanel`].
//!
//! For now the Agent panel is an empty placeholder (a centered "coming soon"
//! message) so the mode toggle is fully wired end-to-end. The Agent feature's
//! real panels (chat, tool calls, …) will replace this render later, mirroring
//! how `SshClientPanel` composes `SessionPanel` + `SftpPanel`.
//!
//! Like `SshClientPanel`, this is registered as a raw `DockItem::Panel` (no tab bar /
//! close / zoom chrome) and lives in the `app` crate (the only omniscient crate,
//! R9) so it can later compose any feature crates the Agent mode needs.

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};

use gpui_component::{ActiveTheme as _, v_flex};
use oneterm_ui::dock::{Panel, PanelControl, PanelEvent, PanelInfo, PanelState, register_panel};

/// Panel name registered with the gpui-component `PanelRegistry`.
///
/// The feature-agnostic shell builds this panel *by name* via
/// `build_named_panel("agent_panel", ...)` — it never depends on the concrete
/// type. Saved layouts deserialize by this name too.
pub const AGENT_PANEL_NAME: &str = "agent_panel";

/// Right-dock panel for Agent Mode.
///
/// `panel_name = "agent_panel"`. Rendered raw as a `DockItem::Panel`. Today it
/// shows a placeholder; the real Agent panels will be composed here later.
pub struct AgentPanel {
    focus_handle: FocusHandle,
}

impl AgentPanel {
    /// Create a new Agent panel.
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl EventEmitter<PanelEvent> for AgentPanel {}

impl Focusable for AgentPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for AgentPanel {
    fn panel_name(&self) -> &'static str {
        AGENT_PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Agent"
    }

    fn closable(&self, _: &App) -> bool {
        // The panel itself is the whole right dock; closing is handled via the
        // dock's toggle (collapsing the right dock), not via a panel close.
        false
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        // Zoom is a TabPanel feature; `DockItem::Panel` is not subscribed to
        // zoom events by the library (see `DockArea::subscribe_item`).
        None
    }

    fn dump(&self, _cx: &App) -> PanelState {
        // Persist as `PanelInfo::Panel` so the saved layout records this as a
        // single panel rather than a tab group (mirrors `SshClientPanel::dump`).
        PanelState {
            panel_name: AGENT_PANEL_NAME.to_string(),
            children: Vec::new(),
            info: PanelInfo::panel(serde_json::Value::Null),
        }
    }
}

impl Render for AgentPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .id("agent-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(theme.background)
            .items_center()
            .justify_center()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child("Agent Mode"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Coming soon"),
            )
            .into_any_element()
    }
}

/// Initialize the Agent panel: register the `"agent_panel"` dock panel with the
/// gpui-component `PanelRegistry` so the shell can build it by name and saved
/// layouts can deserialize it. Called by the app aggregator ([`crate::init::init`]).
pub fn init(cx: &mut App) {
    register_panel(cx, AGENT_PANEL_NAME, |_, _, _, window, cx| {
        Box::new(AgentPanel::new_entity(window, cx))
    });
}
