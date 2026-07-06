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
//! "active tab merges with content" effect like Zed/editors, with no override needed.

use gpui::{Anchor, App, px};
use gpui_component::{ActiveTheme as _, Theme, ThemeRegistry, scroll::ScrollbarShow};

use crate::actions::{SwitchTheme, SwitchThemeMode};

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
pub(crate) fn apply_list_style_override(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    theme.list_active = theme.list_hover;
    theme.list_active_border = gpui::transparent_black();
    // DataTable selected-row overlay: disable both bg and border so the highlight
    // is drawn by `render_tr` (= `table_hover`, like hover, no border).
    theme.table_active = gpui::transparent_black();
    theme.table_active_border = gpui::transparent_black();
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
        if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme_config);
            apply_list_style_override(cx);
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
            let saved = crate::state::UiConfig::global(cx).read(cx);
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

    // FontSizeSelector dropped the Border Radius and Scrollbar options, so set fixed
    // defaults here (after Theme::change so apply_config does not override them —
    // the theme JSON does not declare a radius, so config.radius = None).
    //
    // - radius = sub-pixel (0.001px), radius_lg = 0px: sharp/angular UI.
    // - scrollbar_show = Scrolling: show the scrollbar while scrolling, auto-hide when idle.
    //   (gpui_component::init may have set it = Hover via sync_scrollbar_appearance,
    //   so we force it back = Scrolling here.)
    //
    // NOTE on radius ≠ 0:
    // gpui-component forces the scrollbar thumb square when `theme.radius.is_zero()`
    // (scroll/scrollbar.rs:765). We want the thumb rounded by THUMB_RADIUS, so we use
    // a sub-pixel value ≠ 0 → is_zero() = false → the thumb gets rounded. Every other
    // component using `.rounded(theme.radius)` still renders square corners (0.001px <
    // sub-pixel, not visible). Slider/PieChart also gate on is_zero but the project does
    // not use them → no impact.
    {
        let theme = Theme::global_mut(cx);
        theme.radius = px(4.);
        theme.radius_lg = px(6.);
        theme.scrollbar_show = ScrollbarShow::Always;
    }

    // Selected item = hover look: bg = list_hover, no border.
    // Must be called after Theme::change (apply_config resets these fields).
    apply_list_style_override(cx);

    // Notifications display in the bottom-right corner (gpui-component default is TopRight).
    // `notification` is a `#[serde(skip)]` field — not reset by `apply_config`/`change`,
    // so it only needs to be set once at init.
    Theme::global_mut(cx).notification.placement = Anchor::BottomRight;

    // Observe theme changes — persist the theme name + UI font size to ui_config.json
    // whenever the global Theme is mutated (View ▸ Font Size menu, Appearance page, theme
    // menus, …). Registered last so the init mutations above don't trigger a save.
    cx.observe_global::<Theme>(|cx| {
        let (name, size) = {
            let theme = cx.theme();
            (theme.theme_name().to_string(), theme.font_size.as_f32())
        };
        crate::state::UiConfig::global(cx).update(cx, |cfg, _cx| {
            cfg.theme_name = Some(name);
            cfg.ui_font_size = Some(size);
        });
        crate::state::UiConfig::persist(cx);
    })
    .detach();

    let _ = cx.theme();
}
