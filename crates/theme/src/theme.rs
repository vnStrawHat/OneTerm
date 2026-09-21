//! Theme registration for OneTerm.
//!
//! OneTerm embeds 24 theme JSON files (2 Zed defaults + 22 from the gpui-component
//! collection) into the binary via `ThemeRegistry::load_themes_from_str`.
//! Does not depend on the working directory.
//!
//! Wires up the `SwitchTheme` / `SwitchThemeMode` actions.
//!
//! The built-in themes (`Default Light` / `Default Dark`) are registered by
//! `gpui_component::init`. The theme menu (app_menus) lists every theme in `ThemeRegistry`.
//!
//! Active tab distinction: each theme defines its own `tab.active.background`
//! (usually = content background) and `tab_bar.background` (darker), giving the
//! "active tab merges with content" effect used by code editors, with no override needed.

use gpui::{Anchor, App, Rgba, px, rgb};
use gpui_component::{Theme, ThemeRegistry, scroll::ScrollbarMode};

use oneterm_actions::{SwitchTheme, SwitchThemeMode};

/// List of embedded JSON theme files.
///
/// Each file may contain one or more theme variants (e.g. a dark and a light variant).
/// The `&str` label is used only for log messages on load failure.
///
/// - `zed-one-dark` / `zed-one-light` — the Atom One palette (Zed editor default).
/// - 22 additional themes from the gpui-component collection (iTerm2-Color-Schemes, etc.).
const EMBEDDED_THEME_FILES: &[(&str, &str)] = &[
    // Zed defaults
    ("zed-one-dark", include_str!("../themes/zed-one-dark.json")),
    (
        "zed-one-light",
        include_str!("../themes/zed-one-light.json"),
    ),
    // gpui-component theme collection
    ("adventure", include_str!("../themes/adventure.json")),
    ("alduin", include_str!("../themes/alduin.json")),
    ("asciinema", include_str!("../themes/asciinema.json")),
    ("aurora", include_str!("../themes/aurora.json")),
    ("ayu", include_str!("../themes/ayu.json")),
    ("catppuccin", include_str!("../themes/catppuccin.json")),
    ("everforest", include_str!("../themes/everforest.json")),
    ("fahrenheit", include_str!("../themes/fahrenheit.json")),
    ("flexoki", include_str!("../themes/flexoki.json")),
    ("gruvbox", include_str!("../themes/gruvbox.json")),
    ("harper", include_str!("../themes/harper.json")),
    ("hybrid", include_str!("../themes/hybrid.json")),
    ("jellybeans", include_str!("../themes/jellybeans.json")),
    ("kibble", include_str!("../themes/kibble.json")),
    (
        "macos-classic",
        include_str!("../themes/macos-classic.json"),
    ),
    ("matrix", include_str!("../themes/matrix.json")),
    ("mellifluous", include_str!("../themes/mellifluous.json")),
    ("molokai", include_str!("../themes/molokai.json")),
    ("solarized", include_str!("../themes/solarized.json")),
    ("spaceduck", include_str!("../themes/spaceduck.json")),
    ("tokyonight", include_str!("../themes/tokyonight.json")),
    ("twilight", include_str!("../themes/twilight.json")),
];

/// Override list selection style: the selected item looks like hover (bg =
/// `list_hover`, no border).
///
/// `apply_config` / `Theme::change` reset `list_active` + `list_active_border`
/// from the theme JSON (or fallback), so this must be called again after each
/// theme switch.
pub fn apply_list_style_override(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    theme.list_active = theme.list_hover;
    theme.list_active_border = gpui::transparent_black();
    // DataTable selected-row overlay: disable both bg and border so the highlight
    // is drawn by `render_tr` (= `table_hover`, like hover, no border).
    theme.table_active = gpui::transparent_black();
    theme.table_active_border = gpui::transparent_black();
}

/// The OneTerm logo cyan (`#58c4dc`) — an identity tint that must not follow
/// the theme (title-bar icon, About page mark, empty-Space placeholder).
pub fn brand_accent() -> Rgba {
    rgb(0x58c4dc)
}

/// Initialize the theme: load embedded themes + wire the `SwitchTheme` / `SwitchThemeMode` actions.
pub fn init(cx: &mut App) {
    // Load the embedded theme JSON into the ThemeRegistry (in addition to the 2 built-in themes).
    let registry = ThemeRegistry::global_mut(cx);
    for (name, content) in EMBEDDED_THEME_FILES {
        if let Err(err) = registry.load_themes_from_str(content) {
            log::warn!("failed to load embedded theme {}: {}", name, err);
        } else {
            log::debug!("loaded embedded theme: {}", name);
        }
    }

    cx.on_action(|switch: &SwitchTheme, cx| {
        let theme_name = switch.0.clone();
        match ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Some(theme_config) => {
                Theme::global_mut(cx).apply_config(&theme_config);
                apply_list_style_override(cx);
            }
            // A stale key binding or menu entry can name a theme that is not
            // registered; say so instead of ignoring the action (ERR-10).
            None => log::warn!("SwitchTheme: theme {theme_name:?} is not registered — ignored"),
        }
        cx.refresh_windows();
    });

    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        Theme::change(mode, None, cx);
        apply_list_style_override(cx);
        cx.refresh_windows();
    });

    // Set the Zed theme as the default (light_theme/dark_theme), then apply Zed One Dark
    // (Zed editor default) as the startup theme.
    //
    // `load_themes_from_str` only puts themes into the `themes` map; it does not update
    // `default_themes` (which `init_default_themes` sets = Default Light/Dark).
    // So we must assign `Theme::light_theme` / `dark_theme` with the Zed config ourselves,
    // then call `Theme::change` to apply.
    {
        let registry = ThemeRegistry::global(cx);
        let zed_dark = registry.themes().get("Zed One Dark").cloned();
        let zed_light = registry.themes().get("Zed One Light").cloned();
        if let (Some(dark), Some(light)) = (zed_dark, zed_light) {
            let theme = Theme::global_mut(cx);
            theme.dark_theme = dark;
            theme.light_theme = light;
            // Default startup: Dark mode with Zed One Dark (the iconic Zed default).
            Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        }

        // Restore the persisted theme (from ui_config.json), if any. This overrides
        // the Zed default above so the user's last theme choice survives restart.
        let (saved_theme, saved_font) = {
            let saved = oneterm_settings::UiConfig::global(cx).read(cx);
            (saved.theme_name.clone(), saved.ui_font_size)
        };
        if let Some(name) = saved_theme.as_ref() {
            if let Some(theme_config) = ThemeRegistry::global(cx)
                .themes()
                .get(name.as_str())
                .cloned()
            {
                Theme::global_mut(cx).apply_config(&theme_config);
                apply_list_style_override(cx);
            } else {
                log::warn!("Saved theme {name:?} not found — using default");
            }
        }
        if let Some(size) = saved_font {
            Theme::global_mut(cx).font_size = px(size);
        }
    }

    // Fixed shape defaults, applied after `Theme::change` so `apply_config`
    // does not override them (the theme JSON declares no radius, so
    // `config.radius = None`):
    //
    // - radius = 4px, radius_lg = 6px: softly rounded controls.
    // - scrollbar_show = Always: scrollbars stay visible instead of the
    //   gpui-component default (`sync_scrollbar_appearance` may set Hover).
    {
        let theme = Theme::global_mut(cx);
        theme.radius = px(4.);
        theme.radius_lg = px(6.);
        theme.scrollbar_mode = ScrollbarMode::Always;
    }

    // Selected item = hover look: bg = list_hover, no border.
    // Must be called after Theme::change (apply_config resets these fields).
    apply_list_style_override(cx);

    // Notifications display in the bottom-right corner (gpui-component default is TopRight).
    // `notification` is a `#[serde(skip)]` field — not reset by `apply_config`/`change`,
    // so it only needs to be set once at init.
    Theme::global_mut(cx).notification.placement = Anchor::BottomRight;

    // Persistence of the theme choice (`ui_config.json`) is owned by
    // `oneterm_settings::UiConfig::observe_theme`, which the composition root
    // registers after this init so the startup mutations above do not write.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_theme_file_parses() {
        // TEST-21: a broken JSON in `themes/` must fail here, not as a runtime
        // warning at startup.
        for (name, content) in EMBEDDED_THEME_FILES {
            let mut registry = ThemeRegistry::default();
            registry
                .load_themes_from_str(content)
                .unwrap_or_else(|error| panic!("embedded theme {name} failed to parse: {error}"));
            assert!(
                !registry.themes().is_empty(),
                "embedded theme {name} defines no theme variant"
            );
        }
    }

    /// The theme files name a key the kit actually reads.
    ///
    /// This is a **spelling** test, and it exists because a wrong spelling is
    /// silent. `ThemeConfigColors` has no `deny_unknown_fields`, so a key the
    /// kit does not know is dropped without a word: `IN-0043` shipped 39
    /// themes carrying `"warning"` inside `colors`, which is a *syntax
    /// highlighting* key and not the colour the window draws
    /// (`schema.rs:617` renames the field to `warning.background`). Every
    /// static check passed — the JSON parsed, the contrast gate measured the
    /// value and reported it legible — and the application rendered something
    /// else entirely.
    ///
    /// So the assertion is made the only way that could have caught it: drive
    /// the kit's own `apply_config` and compare what `Theme` ends up holding
    /// against what the file says.
    #[gpui::test]
    fn warning_is_the_value_the_theme_file_names(cx: &mut gpui::TestAppContext) {
        // Two themes, one of each mode, whose `warning.background` differs from
        // every fallback the kit could substitute.
        const EXPECTED: &[(&str, u32)] = &[("Ayu Light", 0x955d0b), ("Hybrid Dark", 0x9f8f18)];

        cx.update(|cx| {
            gpui_component::init(cx);
            let registry = ThemeRegistry::global_mut(cx);
            for (_, content) in EMBEDDED_THEME_FILES {
                registry.load_themes_from_str(content).unwrap();
            }
            for (name, rgb) in EXPECTED {
                let config = ThemeRegistry::global(cx)
                    .themes()
                    .get(*name)
                    .unwrap_or_else(|| panic!("{name} is not in the registry"))
                    .clone();
                Theme::global_mut(cx).apply_config(&config);
                let rendered = Theme::global(cx).warning;
                let expected = gpui::rgb(*rgb);
                assert!(
                    hsla_close(rendered, expected.into()),
                    "{name}: the file names #{rgb:06x} but the kit renders {rendered:?} — \
                     the key in `colors` is not one `ThemeConfigColors` reads"
                );
            }
        });
    }

    /// Two colours are the same to within one 8-bit step per channel, which is
    /// all a hex value can express.
    #[cfg(test)]
    fn hsla_close(a: gpui::Hsla, b: gpui::Hsla) -> bool {
        let (x, y) = (gpui::Rgba::from(a), gpui::Rgba::from(b));
        [(x.r, y.r), (x.g, y.g), (x.b, y.b)]
            .iter()
            .all(|(p, q)| (p - q).abs() <= 1.5 / 255.0)
    }

    #[test]
    fn zed_default_themes_are_present_under_their_registry_names() {
        // `init` looks these names up to install the Zed defaults; a rename in
        // the JSON would silently fall back to gpui-component's defaults.
        let mut registry = ThemeRegistry::default();
        for (_, content) in EMBEDDED_THEME_FILES {
            registry.load_themes_from_str(content).unwrap();
        }
        assert!(registry.themes().contains_key("Zed One Dark"));
        assert!(registry.themes().contains_key("Zed One Light"));
        assert!(registry.themes().len() >= EMBEDDED_THEME_FILES.len());
    }
}
