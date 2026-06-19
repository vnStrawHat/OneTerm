//! UI-level actions cho myTerm2.

use gpui::{SharedString, actions};
use gpui_component::{ThemeMode, dock::DockPlacement};
use serde::Deserialize;

/// Thêm một panel mới vào dock ở placement đã cho.
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = myterm2, no_json)]
pub struct AddPanel(pub DockPlacement);

actions!(
    myterm2,
    [
        /// Thoát ứng dụng.
        Quit,
        /// Mở hộp thoại About.
        About,
        /// Bật/tắt nút toggle dock.
        ToggleDockToggleButton,
    ]
);

/// Đổi ngôn ngữ UI.
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = myterm2, no_json)]
pub struct SelectLocale(pub SharedString);

/// Đổi theme (theo tên đăng ký trong `ThemeRegistry`).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = myterm2, no_json)]
pub struct SwitchTheme(pub SharedString);

/// Đổi mode theme (Light/Dark).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = myterm2, no_json)]
pub struct SwitchThemeMode(pub ThemeMode);
