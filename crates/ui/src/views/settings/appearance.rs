//! "Appearance" settings page — theme mode (Light/Dark) + theme list.
//!
//! Mirrors the OneTerm ▸ Appearance / Theme menus. Switching the theme here
//! reuses the same logic as the [`SwitchTheme`] / [`SwitchThemeMode`] actions
//! (defined in [`crate::theme`]), including the list-style override applied
//! after every theme switch.

use gpui::{App, SharedString};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Theme, ThemeMode, ThemeRegistry,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
};

use crate::theme::apply_list_style_override;

/// Build the "Appearance" settings page.
pub(crate) fn page(cx: &App) -> SettingPage {
    SettingPage::new("Appearance")
        .icon(Icon::new(IconName::Palette))
        .group(theme_mode_group())
        .group(theme_group(cx))
}

/// "Theme Mode" group — switch between Light and Dark.
fn theme_mode_group() -> SettingGroup {
    let options: Vec<(SharedString, SharedString)> = vec![
        ("light".into(), "Light".into()),
        ("dark".into(), "Dark".into()),
    ];

    SettingGroup::new()
        .title("Theme Mode")
        .description("Switch between light and dark appearance.")
        .item(
            SettingItem::new(
                "Mode",
                SettingField::dropdown(
                    options,
                    |cx: &App| {
                        SharedString::from(if cx.theme().mode.is_dark() {
                            "dark"
                        } else {
                            "light"
                        })
                    },
                    |val: SharedString, cx: &mut App| {
                        let mode = if val.as_ref() == "light" {
                            ThemeMode::Light
                        } else {
                            ThemeMode::Dark
                        };
                        Theme::change(mode, None, cx);
                        apply_list_style_override(cx);
                        cx.refresh_windows();
                    },
                ),
            )
            .description("Light or Dark appearance."),
        )
}

/// "Theme" group — pick a color theme from the registry (built-in + loaded).
fn theme_group(cx: &App) -> SettingGroup {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    let options: Vec<(SharedString, SharedString)> = themes
        .iter()
        .map(|t| (t.name.clone(), t.name.clone()))
        .collect();

    SettingGroup::new()
        .title("Theme")
        .description("The color theme applied to the whole application.")
        .item(
            SettingItem::new(
                "Color Theme",
                SettingField::scrollable_dropdown(
                    options,
                    |cx: &App| cx.theme().theme_name().clone(),
                    |val: SharedString, cx: &mut App| {
                        let Some(theme_config) = ThemeRegistry::global(cx)
                            .themes()
                            .get(val.as_ref())
                            .cloned()
                        else {
                            return;
                        };
                        Theme::global_mut(cx).apply_config(&theme_config);
                        apply_list_style_override(cx);
                        cx.refresh_windows();
                    },
                ),
            )
            .description("Choose from the built-in themes (Zed, iTerm2 color schemes, …)."),
        )
}
