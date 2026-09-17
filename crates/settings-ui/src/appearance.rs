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
//! The theme picker is a [`SettingField::element`] rather than the kit's
//! `scrollable_dropdown`, for one reason: the dropdown field can only be handed
//! `(value, label)` pairs, so a section heading would have to be a selectable row
//! that does nothing. Building the menu here lets the headings be
//! [`PopupMenuItem::label`], which the kit renders disabled and excludes from
//! clicking and from keyboard navigation. `crates/settings-ui` already escapes a
//! kit field's limits this way five times over (see `terminal/font.rs`'s Line
//! Height field).
//!
//! The rows come from [`theme_rows`], which sections the ~40 registered themes
//! into Light and Dark and puts the selected theme on the first selectable row.
//! That ordering is what makes the current theme visible the moment the list
//! opens: the kit's popup always opens at its top and exposes no way to scroll it
//! to the checked row (`US-0121` Gaps).

use gpui::{
    Anchor, App, IntoElement as _, SharedString, Styled as _, Window, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, AxisExt as _, Disableable as _, Sizable as _, Theme, ThemeMode,
    ThemeRegistry,
    button::Button,
    menu::{DropdownMenu as _, PopupMenuItem},
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem},
};

use oneterm_theme::theme::apply_list_style_override;

const DEFAULT_THEME_MODE: &str = "dark";
const DEFAULT_THEME_NAME: &str = "Zed One Dark";

/// One row of the theme picker's popup.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ThemeRow {
    /// A section heading. Rendered as a `PopupMenuItem::label`, which the kit
    /// draws disabled and skips when clicking and when navigating by keyboard.
    Section(&'static str),
    /// A selectable theme, by registry name.
    Theme(SharedString),
}

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
    .description("Switching this also swaps the colour theme to the last one used in that mode.")
}

fn mode_options() -> Vec<(SharedString, SharedString)> {
    vec![
        ("light".into(), "Light".into()),
        ("dark".into(), "Dark".into()),
    ]
}

/// The colour-theme picker: every theme in the registry (built-in + loaded).
fn color_theme_item(_cx: &App) -> SettingItem {
    SettingItem::new("Color Theme", color_theme_field())
        .description("Built-in and loaded themes, grouped Light / Dark.")
        .keywords(["theme", "colour", "color", "appearance"])
}

fn color_theme_field() -> SettingField<SharedString> {
    SettingField::element(
        |options: &RenderOptions, _window: &mut Window, cx: &mut App| {
            let current = cx.theme().theme_name().clone();
            let rows = theme_rows(&registered_themes(cx), &current);

            Button::new("theme-picker")
                .when(options.layout().is_vertical(), |this| this.w_full())
                .label(current)
                .dropdown_caret(true)
                .outline()
                .disabled(options.is_disabled())
                .with_size(options.size())
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, cx| {
                    let selected = cx.theme().theme_name().clone();
                    rows.iter()
                        .fold(menu, |menu, row| match row {
                            ThemeRow::Section(title) => menu.item(PopupMenuItem::label(*title)),
                            ThemeRow::Theme(name) => menu.item(
                                PopupMenuItem::new(name.clone())
                                    .checked(name == &selected)
                                    .on_click({
                                        let name = name.clone();
                                        move |_, _, cx| apply_theme_named(&name, cx)
                                    }),
                            ),
                        })
                        .scrollable(true)
                })
                .into_any_element()
        },
    )
    .on_reset(
        |cx: &App| cx.theme().theme_name().as_ref() != DEFAULT_THEME_NAME,
        |_window, cx| apply_theme_named(&SharedString::from(DEFAULT_THEME_NAME), cx),
    )
}

/// Every registered theme's name and mode, in the registry's own order.
fn registered_themes(cx: &App) -> Vec<(SharedString, ThemeMode)> {
    ThemeRegistry::global(cx)
        .sorted_themes()
        .iter()
        .map(|theme| (theme.name.clone(), theme.mode))
        .collect()
}

/// Apply the theme with this name, if the registry knows it.
fn apply_theme_named(name: &SharedString, cx: &mut App) {
    let Some(theme_config) = ThemeRegistry::global(cx).themes().get(name).cloned() else {
        return;
    };
    Theme::global_mut(cx).apply_config(&theme_config);
    apply_list_style_override(cx);
    cx.refresh_windows();
}

/// The theme picker's rows: a "Light" and a "Dark" section, each headed by a
/// label and followed by that mode's themes.
///
/// The section holding `current` comes first and `current` leads it, so the
/// checked row is always the first selectable one and is on screen without
/// scrolling. Every other theme keeps case-insensitive name order.
fn theme_rows(registered: &[(SharedString, ThemeMode)], current: &str) -> Vec<ThemeRow> {
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
        rows.push(ThemeRow::Section(section_title(mode)));
        rows.extend(names.into_iter().map(|name| ThemeRow::Theme(name.clone())));
    }
    rows
}

fn section_title(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::Light => "Light themes",
        ThemeMode::Dark => "Dark themes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row the popup opens on — the checked one. The popup itself always opens
    /// at row 0, so this is the assertion that keeps the ordering honest: the
    /// checked row must stay the first *selectable* one.
    fn open_index(rows: &[ThemeRow], current: &str) -> Option<usize> {
        rows.iter()
            .position(|row| matches!(row, ThemeRow::Theme(name) if name.as_ref() == current))
    }

    fn registered() -> Vec<(SharedString, ThemeMode)> {
        vec![
            ("Ayu Light".into(), ThemeMode::Light),
            ("Zed One Dark".into(), ThemeMode::Dark),
            ("ayu Dark".into(), ThemeMode::Dark),
            ("Catppuccin Latte".into(), ThemeMode::Light),
        ]
    }

    fn labels(rows: &[ThemeRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                ThemeRow::Section(title) => (*title).to_owned(),
                ThemeRow::Theme(name) => name.to_string(),
            })
            .collect()
    }

    #[test]
    fn the_selected_theme_leads_its_section_and_its_section_leads_the_list() {
        let rows = theme_rows(&registered(), "Zed One Dark");
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
        // Row 0 is a section label, which the kit renders disabled and skips
        // when clicking; the checked theme is the first selectable row, so it
        // is on screen the moment the popup opens without any scrolling.
        assert_eq!(open_index(&rows, "Zed One Dark"), Some(1));
        assert!(matches!(rows[0], ThemeRow::Section(_)));
    }

    #[test]
    fn a_light_selection_puts_the_light_section_first() {
        let rows = theme_rows(&registered(), "Catppuccin Latte");
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
        let rows = theme_rows(&registered(), "Deleted Theme");
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
    fn a_section_heading_is_never_a_selectable_theme() {
        let rows = theme_rows(&registered(), "Zed One Dark");
        let sections: Vec<&ThemeRow> = rows
            .iter()
            .filter(|row| matches!(row, ThemeRow::Section(_)))
            .collect();
        assert_eq!(sections.len(), 2);
        // A section is its own row variant, so it carries no theme name and
        // cannot be clicked into `apply_theme_named` at all.
        for section in sections {
            assert!(matches!(section, ThemeRow::Section(_)));
        }
    }

    #[test]
    fn an_empty_section_contributes_no_heading() {
        let only_dark: Vec<(SharedString, ThemeMode)> =
            vec![("Zed One Dark".into(), ThemeMode::Dark)];
        assert_eq!(
            labels(&theme_rows(&only_dark, "Zed One Dark")),
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
