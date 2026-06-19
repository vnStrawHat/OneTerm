//! Theme registration cho myTerm2.
//!
//! Giống reference `reference/.../story/src/themes.rs`, myTerm2:
//! - Nạp 24 theme JSON (22 theme community + Zed One Dark + Zed One Light)
//!   embed sẵn vào binary qua `ThemeRegistry::load_themes_from_str`. Không phụ
//!   thuộc working dir.
//! - Wire action `SwitchTheme` / `SwitchThemeMode`.
//!
//! Built-in themes (`Default Light` / `Default Dark`) do `gpui_component::init`
//! đăng ký. Theme menu (app_menus) sẽ liệt kê tất cả theme trong `ThemeRegistry`.
//!
//! Active tab distinction: mỗi theme tự định nghĩa `tab.active.background`
//! (thường = content background) và `tab_bar.background` (darker) — cho hiệu
//! ứng "tab active nối với content" kiểu Zed/editor, không cần override.

use gpui::App;
use gpui_component::{ActiveTheme as _, Theme, ThemeRegistry};

use crate::actions::{SwitchTheme, SwitchThemeMode};

/// Danh sách theme JSON embed sẵn.
///
/// 22 theme community clone từ `reference/gpui-component/themes/`, thêm
/// `zed-one-dark` / `zed-one-light` — palette Atom One (Zed editor default).
const EMBEDDED_THEME_FILES: &[(&str, &str)] = &[
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
    // Zed editor default themes (Atom One palette).
    ("zed-one-dark", include_str!("../themes/zed-one-dark.json")),
    (
        "zed-one-light",
        include_str!("../themes/zed-one-light.json"),
    ),
];

/// Khởi tạo theme: nạp embedded themes + wire action `SwitchTheme` / `SwitchThemeMode`.
pub fn init(cx: &mut App) {
    // Nạp embedded theme JSON vào ThemeRegistry (bổ sung cho 2 built-in theme).
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
        }
        cx.refresh_windows();
    });

    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        Theme::change(mode, None, cx);
        cx.refresh_windows();
    });

    // Đặt theme Zed làm mặc định (light_theme/dark_theme), rồi apply Zed One Dark
    // (Zed editor default) làm theme khởi động.
    //
    // `load_themes_from_str` chỉ đưa theme vào `themes` map, không cập nhật
    // `default_themes` (vốn do `init_default_themes` set = Default Light/Dark).
    // Nên phải tự gán `Theme::light_theme` / `dark_theme` bằng config Zed, rồi
    // `Theme::change` để apply.
    {
        let registry = ThemeRegistry::global(cx);
        let zed_dark = registry.themes().get("Zed One Dark").cloned();
        let zed_light = registry.themes().get("Zed One Light").cloned();
        if let (Some(dark), Some(light)) = (zed_dark, zed_light) {
            let theme = Theme::global_mut(cx);
            theme.dark_theme = dark;
            theme.light_theme = light;
            // Khởi động ở Dark mode với Zed One Dark (Zed default iconic).
            Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        }
    }

    // Observe theme changes — placeholder để sau này persist theme name.
    cx.observe_global::<Theme>(|_cx| {}).detach();

    let _ = cx.theme();
}
