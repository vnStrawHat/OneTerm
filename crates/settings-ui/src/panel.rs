//! [`SettingsPanel`] — the General Settings view shown in its own window.
//!
//! Wraps the gpui-component [`Settings`] widget (a sidebar + page layout).
//!
//! Pages are short by design, not by taste. The kit scrolls a sidebar sub-item
//! into view from the group heights it has measured, and it measures only what
//! has been laid out, so a group below the fold of a freshly opened page cannot
//! be scrolled to (`docs/gui-layout.md` §"Sidebar navigation", `US-0122`).
//! Page *selection*, by contrast, is exact: it swaps an index and scrolls
//! nothing. Splitting the two long pages into pages that each fit the window is
//! therefore what makes the sidebar work, and it keeps everything the kit gives
//! a page — sub-items, per-item search, and a page-level Reset All.
//!
//! The pages, in sidebar order: General; Key Bindings, Key Bindings: Terminal,
//! Key Bindings: Sessions; Terminal, Terminal Display, Mouse & Clipboard,
//! Terminal Logging, Completion; SSH; Network; About. Each owning module builds
//! its own and `docs/gui-layout.md` §Settings window is the list's home. The
//! Terminal pages read/write the global [`TerminalSettings`] and persist changes
//! to `terminal.json`; General's theme group drives the gpui-component [`Theme`]
//! / [`ThemeRegistry`].
//!
//! The view is hosted by [`super::window`] inside a [`gpui_component::Root`];
//! it is not a dock panel and is deliberately not registered with the
//! `PanelRegistry` (ARCH-39).
//!
//! **This window has no notification layer, on purpose.** `Root::render` draws
//! none of its own, so a `push_notification` from a settings control goes into a
//! layer nobody renders and is never seen. Rendering one here — exactly as
//! `OneTermWorkspace::render` does — makes the toast appear but *under* the page:
//! the kit's `SettingPage` paints its chips, switches and buttons into a scene
//! layer, and `Scene::insert_primitive` orders every primitive by the enclosing
//! layer before anything else (`reference/zed/crates/gpui/src/scene.rs:74-101`),
//! so a card drawn later still loses. Two probes settled that it is not a
//! paint-order contest a caller can win: `deferred` at `POPUP_PRIORITY + 1`, and
//! again at 10_000, both left the rows printing through the card (`US-0123`
//! Evidence, `F-R6`). A settings control that needs to say something says it
//! **in the page**, beside the control — see the Key Bindings row's notice line.

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

use super::{about, general, key_bindings, ssh, terminal, updates};

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
        // the SSH page's SFTP Custom-mode enable/disable) updates live.
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
        let mut pages = vec![general::page(cx)];
        pages.extend(key_bindings::pages());
        pages.extend(terminal::pages());
        pages.push(ssh::page(cx));
        pages.push(updates::network_page(cx));
        pages.push(about::page(cx));
        pages
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
