//! Theme registration for OneTerm.
//!
//! Like the reference `reference/.../story/src/themes.rs`, OneTerm:
//! - Loads 2 JSON themes (Zed One Dark + Zed One Light) embedded into the binary via
//!   `ThemeRegistry::load_themes_from_str`. Does not depend on the working directory.
//! - Wires up the `SwitchTheme` / `SwitchThemeMode` actions.
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

/// List of embedded JSON themes.
///
/// `zed-one-dark` / `zed-one-light` — the Atom One palette (Zed editor default).
const EMBEDDED_THEME_FILES: &[(&str, &str)] = &[
    ("zed-one-dark", include_str!("../themes/zed-one-dark.json")),
    (
        "zed-one-light",
        include_str!("../themes/zed-one-light.json"),
    ),
];

/// Override list selection style: the selected item looks like hover (bg =
/// `list_hover`, no border).
///
/// `apply_config` / `Theme::change` reset `list_active` + `list_active_border`
/// from the theme JSON (or fallback), so this must be called again after each
/// theme switch.
fn apply_list_style_override(cx: &mut App) {
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
            tracing::warn!("failed to load embedded theme {}: {}", name, err);
        } else {
            tracing::debug!("loaded embedded theme: {}", name);
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
            // Start in Dark mode with Zed One Dark (the iconic Zed default).
            Theme::change(gpui_component::ThemeMode::Dark, None, cx);
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
        theme.radius = px(0.001);
        theme.radius_lg = px(0.);
        theme.scrollbar_show = ScrollbarShow::Always;
    }

    // Selected item = hover look: bg = list_hover, no border.
    // Must be called after Theme::change (apply_config resets these fields).
    apply_list_style_override(cx);

    // Notifications display in the bottom-right corner (gpui-component default is TopRight).
    // `notification` is a `#[serde(skip)]` field — not reset by `apply_config`/`change`,
    // so it only needs to be set once at init.
    Theme::global_mut(cx).notification.placement = Anchor::BottomRight;

    // Observe theme changes — placeholder for persisting the theme name later.
    cx.observe_global::<Theme>(|_cx| {}).detach();

    let _ = cx.theme();
}
