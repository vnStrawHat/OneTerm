//! [`SettingsPanel`] — the General Settings dock panel.
//!
//! Wraps the gpui-component [`Settings`] widget (a sidebar + page layout) with
//! five pages: General (UI font), Key Bindings (configurable shortcuts grouped by
//! origin), Terminal (shell/font/cursor/layout/scroll/bell/security), Appearance
//! (theme mode + theme list), and About. The Terminal page reads/writes the global
//! [`TerminalSettings`] and persists changes to `terminal.json`; the Appearance
//! page drives the gpui-component [`Theme`] / [`ThemeRegistry`].

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Render,
    Window,
};
use gpui_component::dock::{Panel, PanelControl, PanelEvent};
use gpui_component::setting::{SettingPage, Settings};

use super::{about, appearance, general, key_bindings, terminal};

/// General Settings panel (font, theme, key bindings, terminal options, about).
pub struct SettingsPanel {
    focus_handle: FocusHandle,
}

impl SettingsPanel {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Re-render when the key-binding UI state changes (capture mode entered/
        // exited) so the capturing row / binding chips update live.
        cx.observe(
            &super::key_bindings::KeyBindingsState::global(cx),
            |_, _, cx| cx.notify(),
        )
        .detach();
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Build the four setting pages.
    ///
    /// Pages are rebuilt on every render (same pattern as the gpui-component
    /// `settings_story`) so the get-closures always read the latest state.
    fn pages(&self, cx: &App) -> Vec<SettingPage> {
        vec![
            general::page(),
            key_bindings::page(),
            terminal::page(),
            appearance::page(cx),
            about::page(),
        ]
    }
}

impl EventEmitter<PanelEvent> for SettingsPanel {}

impl Focusable for SettingsPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for SettingsPanel {
    fn panel_name(&self) -> &'static str {
        "settings"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Settings"
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Settings::new("oneterm-settings").pages(self.pages(cx))
    }
}
