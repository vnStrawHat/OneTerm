//! Theme registration cho myTerm2.
//!
//! Skeleton này dùng built-in themes của gpui-component (`ThemeRegistry::init_default_themes`
//! đã được `gpui_component::init` gọi). Theme switching qua menu Appearance/Theme
//! được wire ở `app_menus`.
//!
//! Sau này: đăng ký theme JSON từ `config/themes/` qua `ThemeRegistry::watch_dir`
//! (xem `reference/.../story/src/themes.rs`).

use gpui::App;
use gpui_component::{ActiveTheme, Theme, ThemeRegistry};

use crate::actions::{SwitchTheme, SwitchThemeMode};

/// Khởi tạo theme: wire action `SwitchTheme` / `SwitchThemeMode`.
pub fn init(cx: &mut App) {
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

    // Observe theme changes — placeholder để sau này persist theme name.
    cx.observe_global::<Theme>(|_cx| {}).detach();

    let _ = cx.theme();
}
