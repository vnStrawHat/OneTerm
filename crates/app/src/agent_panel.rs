//! Agent Mode right-dock panel — composes the Agent feature's panel content.
//!
//! OneTerm's right dock content is switched by [`oneterm_core::RightDockMode`]:
//! - `SshClient` → the combined Session + SFTP [`crate::ssh_client_panel::SshClientPanel`].
//! - `Agent` → this [`AgentPanel`].
//!
//! Following the composite-panel pattern (`SshClientPanel` composes
//! `SessionPanel` + `SftpPanel`), this panel hosts the Agent feature's
//! [`oneterm_agent_ui::AgentListView`] — a right-dock "fleet view" of coding
//! agents reporting via OSC 9;7. See `docs/agent-panel-display.md`.
//!
//! Like `SshClientPanel`, this is registered as a raw `DockItem::Panel` (no tab
//! bar / close / zoom chrome) and lives in the `app` crate (the only omniscient
//! crate, R9) so it may compose feature crates.

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Window,
};

use gpui_component::dock::{
    Panel, PanelControl, PanelEvent, PanelInfo, PanelState, register_panel,
};
use gpui_component::{ActiveTheme as _, v_flex};
use oneterm_agent_ui::AgentListView;

/// Panel name registered with the gpui-component `PanelRegistry`.
///
/// The feature-agnostic shell builds this panel *by name* via
/// `build_named_panel("agent_panel", ...)` — it never depends on the concrete
/// type. Saved layouts deserialize by this name too.
pub const AGENT_PANEL_NAME: &str = "agent_panel";

/// Right-dock panel for Agent Mode.
///
/// `panel_name = "agent_panel"`. Rendered raw as a `DockItem::Panel`; hosts the
/// [`AgentListView`] which renders its own header + scrolling card column.
pub struct AgentPanel {
    focus_handle: FocusHandle,
    list: Entity<AgentListView>,
}

impl AgentPanel {
    /// Create a new Agent panel.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            list: AgentListView::new_entity(window, cx),
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl EventEmitter<PanelEvent> for AgentPanel {}

impl Focusable for AgentPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate focus to the list so keyboard input reaches it.
        self.list.read(cx).focus_handle(cx).clone()
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
        let bg = cx.theme().background;
        v_flex()
            .id("agent-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(bg)
            .child(self.list.clone())
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
