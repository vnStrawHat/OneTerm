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
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{
    TitleBar,
    group_box::GroupBoxVariant,
    setting::{SettingPage, Settings},
    v_flex,
};

use super::{about, appearance, general, key_bindings, sftp, ssh, terminal, updates};

const SETTINGS_GROUP_VARIANT: GroupBoxVariant = GroupBoxVariant::Outline;
const SETTINGS_PANEL_ROLE: Role = Role::Pane;

#[cfg(test)]
fn reset_button_visibility_probe() -> gpui_component::setting::SettingField<f64> {
    gpui_component::setting::SettingField::number_input(
        gpui_component::setting::NumberFieldOptions::default(),
        |cx| cx.global::<ResetProbe>().0,
        |value, cx| cx.global_mut::<ResetProbe>().0 = value,
    )
    .default_value(16.0)
}

#[cfg(test)]
struct ResetProbe(f64);

#[cfg(test)]
impl gpui::Global for ResetProbe {}

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
            .id("settings-panel")
            .role(SETTINGS_PANEL_ROLE)
            .aria_label("Settings")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(TitleBar::new().child("Settings"))
            .child(
                Settings::new("oneterm-settings")
                    .with_group_variant(SETTINGS_GROUP_VARIANT)
                    .pages(self.pages(cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_groups_match_the_v0_6_story_outline_variant() {
        assert_eq!(SETTINGS_GROUP_VARIANT, GroupBoxVariant::Outline);
    }

    #[test]
    fn settings_panel_focus_target_is_an_accessible_pane() {
        assert!(matches!(SETTINGS_PANEL_ROLE, Role::Pane));
    }

    struct ResetProbeView;

    impl Render for ResetProbeView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
        }
    }

    #[gpui::test]
    fn changed_field_exposes_and_executes_the_conditional_reset_contract(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui_component::setting::AnySettingField as _;

        cx.update(|cx| cx.set_global(ResetProbe(16.0)));
        let (_view, cx) = cx.add_window_view(|_window, _cx| ResetProbeView);
        cx.update(|window, cx| {
            let field = reset_button_visibility_probe();
            assert!(!field.is_resettable(cx));

            cx.global_mut::<ResetProbe>().0 = 18.0;
            assert!(field.is_resettable(cx));
            field.reset(window, cx);

            assert_eq!(cx.global::<ResetProbe>().0, 16.0);
            assert!(!field.is_resettable(cx));
        });
    }
}
