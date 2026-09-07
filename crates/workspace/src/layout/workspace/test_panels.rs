//! Test-only stand-ins for the feature panels the shell builds by name.
//!
//! The shell never depends on a feature crate, so tests register these blank
//! panels under the real [`oneterm_state::panel_names`] to exercise layout
//! code (`build_named_panel`, dock resets, right-dock mode switches).

use std::sync::Arc;

use gpui::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render,
    Window, div,
};
use gpui_component::dock::{BasePanelView, Panel, PanelEvent, panel_handle, register_panel};
use oneterm_state::panel_names;

/// A panel that renders nothing and reports the name it was registered under.
pub(crate) struct NamedPanel {
    name: &'static str,
    focus_handle: FocusHandle,
}

impl NamedPanel {
    pub(crate) fn new(name: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            name,
            focus_handle: cx.focus_handle(),
        }
    }

    /// A wrapped panel view for building dock layouts directly.
    pub(crate) fn view(name: &'static str, cx: &mut App) -> Arc<dyn BasePanelView> {
        panel_handle(cx.new(|cx| Self::new(name, cx)))
    }
}

impl gpui_base::dock::Panel for NamedPanel {
    fn panel_name(&self) -> &'static str {
        self.name
    }
}
impl Panel for NamedPanel {
    fn tab_name(&self, _: &App) -> Option<gpui::SharedString> {
        Some(format!("{0} title", self.name).into())
    }
}
impl EventEmitter<PanelEvent> for NamedPanel {}
impl Focusable for NamedPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
impl Render for NamedPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Register a [`NamedPanel`] for every persisted dock panel name.
pub(crate) fn register_test_panels(cx: &mut App) {
    for name in [
        panel_names::TERMINAL,
        panel_names::SFTP,
        panel_names::SESSION,
        panel_names::SSH_CLIENT,
        panel_names::AGENT,
    ] {
        register_panel(cx, name, move |_, _, cx| {
            panel_handle(cx.new(|cx| NamedPanel::new(name, cx)))
        });
    }
}
