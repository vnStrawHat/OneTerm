//! Theme registration cho OneTerm.
//!
//! Giống reference `reference/.../story/src/themes.rs`, OneTerm:
//! - Nạp 2 theme JSON (Zed One Dark + Zed One Light) embed sẵn vào binary qua
//!   `ThemeRegistry::load_themes_from_str`. Không phụ thuộc working dir.
//! - Wire action `SwitchTheme` / `SwitchThemeMode`.
//!
//! Built-in themes (`Default Light` / `Default Dark`) do `gpui_component::init`
//! đăng ký. Theme menu (app_menus) sẽ liệt kê tất cả theme trong `ThemeRegistry`.
//!
//! Active tab distinction: mỗi theme tự định nghĩa `tab.active.background`
//! (thường = content background) và `tab_bar.background` (darker) — cho hiệu
//! ứng "tab active nối với content" kiểu Zed/editor, không cần override.

use gpui::{Anchor, App, px};
use gpui_component::{ActiveTheme as _, Theme, ThemeRegistry, scroll::ScrollbarShow};

use crate::actions::{SwitchTheme, SwitchThemeMode};

/// Danh sách theme JSON embed sẵn.
///
/// `zed-one-dark` / `zed-one-light` — palette Atom One (Zed editor default).
const EMBEDDED_THEME_FILES: &[(&str, &str)] = &[
    ("zed-one-dark", include_str!("../themes/zed-one-dark.json")),
    (
        "zed-one-light",
        include_str!("../themes/zed-one-light.json"),
    ),
];

/// Override list selection style: selected item trông giống hover (bg =
/// `list_hover`, không border).
///
/// `apply_config` / `Theme::change` reset `list_active` + `list_active_border`
/// từ theme JSON (hoặc fallback), nên phải gọi lại sau mỗi lần switch theme.
fn apply_list_style_override(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    theme.list_active = theme.list_hover;
    theme.list_active_border = gpui::transparent_black();
    // DataTable selected-row overlay: tắt cả bg lẫn border để highlight
    // do `render_tr` vẽ (= `table_hover`, giống hover, không border).
    theme.table_active = gpui::transparent_black();
    theme.table_active_border = gpui::transparent_black();
}

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

    // FontSizeSelector đã bỏ tùy chọn Border Radius và Scrollbar, nên set mặc
    // định cố định ở đây (sau Theme::change để không bị apply_config override —
    // theme JSON không khai báo radius, nên config.radius = None).
    //
    // - radius = sub-pixel (0.001px), radius_lg = 0px: UI sharp/góc cạnh.
    // - scrollbar_show = Scrolling: hiện scrollbar khi cuộn, tự ẩn khi idle.
    //   (gpui_component::init có thể đã set = Hover qua sync_scrollbar_appearance,
    //   nên ép lại = Scrolling ở đây.)
    //
    // NOTE về radius ≠ 0:
    // gpui-component ép thumb scrollbar vuông khi `theme.radius.is_zero()`
    // (scroll/scrollbar.rs:765). Mục đích muốn thumb scrollbar bo tròn theo
    // THUMB_RADIUS, nên dùng giá trị sub-pixel ≠ 0 → is_zero() = false → thumb
    // được bo tròn. Mọi component khác dùng `.rounded(theme.radius)` vẫn render
    // góc vuông (0.001px < sub-pixel, không nhìn thấy). Slider/PieChart cũng gate
    // trên is_zero nhưng project không dùng → không ảnh hưởng.
    {
        let theme = Theme::global_mut(cx);
        theme.radius = px(0.001);
        theme.radius_lg = px(0.);
        theme.scrollbar_show = ScrollbarShow::Always;
    }

    // Selected item = hover look: bg = list_hover, không border.
    // Phải gọi sau Theme::change (apply_config reset các field này).
    apply_list_style_override(cx);

    // Notification hiển thị ở góc dưới bên phải (mặc định gpui-component là TopRight).
    // `notification` là field `#[serde(skip)]` — không bị `apply_config`/`change`
    // reset nên chỉ cần set 1 lần ở init.
    Theme::global_mut(cx).notification.placement = Anchor::BottomRight;

    // Observe theme changes — placeholder để sau này persist theme name.
    cx.observe_global::<Theme>(|_cx| {}).detach();

    let _ = cx.theme();
}
