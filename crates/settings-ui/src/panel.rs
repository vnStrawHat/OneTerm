//! [`SettingsPanel`] — the General Settings view shown in its own window.
//!
//! Wraps the gpui-component [`Settings`] widget (a sidebar + page layout) with
//! seven pages: General (UI font), Key Bindings (configurable shortcuts grouped by
//! origin), Terminal (shell/font/cursor/layout/scroll/bell/security), SSH
//! (connection keepalive), SFTP (editor workflow), Appearance
//! (theme mode + theme list), and About. The Terminal page reads/writes the global
//! [`TerminalSettings`] and persists changes to `terminal.json`; the Appearance
//! page drives the gpui-component [`Theme`] / [`ThemeRegistry`].
//!
//! The view is hosted by [`super::window`] inside a [`gpui_component::Root`];
//! it is not a dock panel and is deliberately not registered with the
//! `PanelRegistry` (ARCH-39).

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement as _,
    Render, Styled as _, Window,
};
use gpui_component::{
    TitleBar,
    setting::{SettingPage, Settings},
    v_flex,
};

use super::{about, appearance, general, key_bindings, sftp, ssh, terminal, updates};

/// General Settings view (font, theme, key bindings, terminal options, about).
pub(crate) struct SettingsPanel {
    focus_handle: FocusHandle,
}

impl SettingsPanel {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Re-render when the key-binding UI state changes (capture mode entered/
        // exited) so the capturing row / binding chips update live.
        cx.observe(
            &super::key_bindings::KeyBindingsState::global(cx),
            |_, _, cx| cx.notify(),
        )
        .detach();
        cx.observe(&updates::UpdateUiState::global(cx), |_, _, cx| cx.notify())
            .detach();
        cx.observe(&updates::UpdateUiState::config(cx), |_, _, cx| cx.notify())
            .detach();
        // Re-render when terminal settings change so setting-dependent UI (e.g.
        // the SFTP page's Custom-mode enable/disable) updates live.
        cx.observe(
            &oneterm_settings::TerminalSettings::global(cx),
            |_, _, cx| cx.notify(),
        )
        .detach();
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub(crate) fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Build the setting pages.
    ///
    /// Pages are rebuilt on every render (same pattern as the gpui-component
    /// `settings_story`) so the get-closures always read the latest state.
    fn pages(&self, cx: &App) -> Vec<SettingPage> {
        vec![
            general::page(),
            key_bindings::page(),
            terminal::page(),
            ssh::page(),
            sftp::page(cx),
            appearance::page(cx),
            about::page(cx),
        ]
    }
}

impl Focusable for SettingsPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(TitleBar::new().child("Settings"))
            .child(Settings::new("oneterm-settings").pages(self.pages(cx)))
    }
}
