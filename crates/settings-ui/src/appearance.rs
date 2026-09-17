//! The "Theme" settings group — theme mode (Light/Dark) + theme list.
//!
//! The group lives on the General page (`US-0122` folded the separate Appearance
//! page away, since two controls did not earn a page of their own and the theme
//! is one of the first things a new user goes looking for).
//!
//! Mirrors the OneTerm ▸ Appearance / Theme menus. Switching the theme here
//! reuses the same logic as the [`SwitchTheme`] / [`SwitchThemeMode`] actions
//! (defined in [`oneterm_theme::theme`]), including the list-style override applied
//! after every theme switch.
//!
//! The theme dropdown is built from [`theme_entries`], which sections the ~40
//! registered themes into Light and Dark and puts the selected theme on the
//! first selectable row. That ordering is what makes the current theme visible
//! the moment the list opens: the kit's popup always opens at its top and
//! exposes no way to scroll it to the checked row (`US-0121` Gaps).

use gpui::{App, SharedString};
use gpui_component::{
    ActiveTheme as _, Theme, ThemeMode, ThemeRegistry,
    setting::{SettingField, SettingGroup, SettingItem},
};

use oneterm_theme::theme::apply_list_style_override;

const DEFAULT_THEME_MODE: &str = "dark";
const DEFAULT_THEME_NAME: &str = "Zed One Dark";

/// Prefix of the value a section header row carries. No theme can be named
/// this (the registry keys on file-provided names), so [`apply_theme_named`]
/// ignores a click on a header without a special case.
const SECTION_VALUE_PREFIX: &str = "\u{1}section:";

/// "Theme" group — light or dark, and which colour theme.
pub(super) fn theme_group(cx: &App) -> SettingGroup {
    SettingGroup::new()
        .title("Theme")
        .item(mode_item())
        .item(color_theme_item(cx))
}

/// The Light/Dark switch.
fn mode_item() -> SettingItem {
    SettingItem::new(
        "Mode",
        SettingField::dropdown(
            mode_options(),
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
        )
        .default_value(DEFAULT_THEME_MODE),
    )
    .description("Applies to the whole application, terminals included.")
}

fn mode_options() -> Vec<(SharedString, SharedString)> {
    vec![
        ("light".into(), "Light".into()),
        ("dark".into(), "Dark".into()),
    ]
}

/// The colour-theme picker: every theme in the registry (built-in + loaded).
fn color_theme_item(cx: &App) -> SettingItem {
    let registered: Vec<(SharedString, ThemeMode)> = ThemeRegistry::global(cx)
        .sorted_themes()
        .iter()
        .map(|theme| (theme.name.clone(), theme.mode))
        .collect();
    let options = theme_entries(&registered, cx.theme().theme_name());

    SettingItem::new(
        "Color Theme",
        SettingField::scrollable_dropdown(
            options,
            |cx: &App| cx.theme().theme_name().clone(),
            |val: SharedString, cx: &mut App| apply_theme_named(&val, cx),
        )
        .default_value(DEFAULT_THEME_NAME),
    )
    .description("Built-in and loaded themes, grouped Light / Dark.")
}

/// Apply the theme with this name, if the registry knows it. A section header's
/// sentinel value is not a theme name, so clicking a header does nothing.
fn apply_theme_named(name: &SharedString, cx: &mut App) {
    let Some(theme_config) = ThemeRegistry::global(cx).themes().get(name).cloned() else {
        return;
    };
    Theme::global_mut(cx).apply_config(&theme_config);
    apply_list_style_override(cx);
    cx.refresh_windows();
}

/// The theme dropdown's rows: a "Light" and a "Dark" section header, each
/// followed by that mode's themes.
///
/// The section holding `current` comes first and `current` leads it, so the
/// checked row is always the first selectable one and is on screen without
/// scrolling. Every other theme keeps case-insensitive name order.
fn theme_entries(
    registered: &[(SharedString, ThemeMode)],
    current: &str,
) -> Vec<(SharedString, SharedString)> {
    let current_mode = registered
        .iter()
        .find(|(name, _)| name.as_ref() == current)
        .map(|(_, mode)| *mode);
    let sections = match current_mode {
        Some(ThemeMode::Light) | None => [ThemeMode::Light, ThemeMode::Dark],
        Some(ThemeMode::Dark) => [ThemeMode::Dark, ThemeMode::Light],
    };

    let mut rows = Vec::with_capacity(registered.len() + sections.len());
    for mode in sections {
        let mut names: Vec<&SharedString> = registered
            .iter()
            .filter(|(_, theme_mode)| *theme_mode == mode)
            .map(|(name, _)| name)
            .collect();
        if names.is_empty() {
            continue;
        }
        names.sort_by_key(|name| name.to_lowercase());
        if current_mode == Some(mode) {
            names.sort_by_key(|name| name.as_ref() != current);
        }
        rows.push(section_header(mode));
        rows.extend(names.into_iter().map(|name| (name.clone(), name.clone())));
    }
    rows
}

fn section_header(mode: ThemeMode) -> (SharedString, SharedString) {
    let (key, label) = match mode {
        ThemeMode::Light => ("light", "Light themes"),
        ThemeMode::Dark => ("dark", "Dark themes"),
    };
    (
        SharedString::from(format!("{SECTION_VALUE_PREFIX}{key}")),
        SharedString::from(label),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row the dropdown opens on — the checked one. The popup itself always
    /// opens at row 0, so this is the assertion that keeps the ordering
    /// honest: it must stay at the first selectable row. `None` when the
    /// current theme is not registered (a theme file removed while selected).
    fn open_index(rows: &[(SharedString, SharedString)], current: &str) -> Option<usize> {
        rows.iter().position(|(value, _)| value.as_ref() == current)
    }

    fn registered() -> Vec<(SharedString, ThemeMode)> {
        vec![
            ("Ayu Light".into(), ThemeMode::Light),
            ("Zed One Dark".into(), ThemeMode::Dark),
            ("ayu Dark".into(), ThemeMode::Dark),
            ("Catppuccin Latte".into(), ThemeMode::Light),
        ]
    }

    fn labels(rows: &[(SharedString, SharedString)]) -> Vec<String> {
        rows.iter().map(|(_, label)| label.to_string()).collect()
    }

    #[test]
    fn the_selected_theme_leads_its_section_and_its_section_leads_the_list() {
        let rows = theme_entries(&registered(), "Zed One Dark");
        assert_eq!(
            labels(&rows),
            vec![
                "Dark themes",
                "Zed One Dark",
                "ayu Dark",
                "Light themes",
                "Ayu Light",
                "Catppuccin Latte",
            ]
        );
        // The checked row is the first selectable one, so it is on screen the
        // moment the popup opens without any scrolling.
        assert_eq!(open_index(&rows, "Zed One Dark"), Some(1));
    }

    #[test]
    fn a_light_selection_puts_the_light_section_first() {
        let rows = theme_entries(&registered(), "Catppuccin Latte");
        assert_eq!(
            labels(&rows),
            vec![
                "Light themes",
                "Catppuccin Latte",
                "Ayu Light",
                "Dark themes",
                "ayu Dark",
                "Zed One Dark",
            ]
        );
        assert_eq!(open_index(&rows, "Catppuccin Latte"), Some(1));
    }

    #[test]
    fn an_unregistered_selection_falls_back_to_plain_light_then_dark_order() {
        let rows = theme_entries(&registered(), "Deleted Theme");
        assert_eq!(
            labels(&rows),
            vec![
                "Light themes",
                "Ayu Light",
                "Catppuccin Latte",
                "Dark themes",
                "ayu Dark",
                "Zed One Dark",
            ]
        );
        assert_eq!(open_index(&rows, "Deleted Theme"), None);
    }

    #[test]
    fn a_section_header_is_not_a_selectable_theme() {
        let rows = theme_entries(&registered(), "Zed One Dark");
        let headers: Vec<&SharedString> = rows
            .iter()
            .map(|(value, _)| value)
            .filter(|value| value.starts_with(SECTION_VALUE_PREFIX))
            .collect();
        assert_eq!(headers.len(), 2);
        // No registered theme can collide with a header value, so a click on a
        // header finds nothing in the registry and changes nothing.
        for (name, _) in registered() {
            assert!(!name.starts_with(SECTION_VALUE_PREFIX));
        }
    }

    #[test]
    fn an_empty_section_contributes_no_header() {
        let only_dark: Vec<(SharedString, ThemeMode)> =
            vec![("Zed One Dark".into(), ThemeMode::Dark)];
        assert_eq!(
            labels(&theme_entries(&only_dark, "Zed One Dark")),
            vec!["Dark themes", "Zed One Dark"]
        );
    }

    #[test]
    fn the_mode_dropdown_offers_exactly_light_and_dark() {
        let keys: Vec<String> = mode_options()
            .iter()
            .map(|(key, _)| key.to_string())
            .collect();
        assert_eq!(keys, vec!["light", "dark"]);
        assert!(keys.contains(&DEFAULT_THEME_MODE.to_owned()));
    }
}
