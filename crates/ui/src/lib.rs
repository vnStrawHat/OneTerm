//! GPUI views, layout, theme, state cho OneTerm.
//!
//! Crate này chứa toàn bộ gpui + gpui-component. Giao tiếp với `ssh`/`local`
//! qua trait abstraction (chưa triển khai ở skeleton này).

pub mod actions;
pub mod components;
pub mod icon;
pub mod layout;
pub mod state;
pub mod theme;
pub mod views;

use gpui::App;
use gpui_component::dock::register_panel;

use crate::views::{SessionPanel, SftpPanel, TerminalPanel, TerminalSettingsPanel};

/// Khởi tạo UI: đăng ký 3 leaf panel cho PanelRegistry (deserialize layout).
///
/// `gpui_component::init(cx)` (gọi ở `app::main`) đã tự khởi tạo theme,
/// dock, root, ... và `PanelRegistry::init`. Hàm này chỉ bổ sung 3 panel
/// của OneTerm vào registry để `DockArea::load` tái tạo được layout cũ.
pub fn init(cx: &mut App) {
    theme::init(cx);
    state::AppState::init(cx);
    state::TerminalSettings::init(cx);
    state::SshSessionStore::init(cx);

    register_panel(cx, "terminal", |_, _, _, window, cx| {
        Box::new(TerminalPanel::new_entity(window, cx))
    });
    register_panel(cx, "terminal-settings", |_, _, _, window, cx| {
        Box::new(TerminalSettingsPanel::new_entity(window, cx))
    });
    register_panel(cx, "session", |_, _, _, window, cx| {
        Box::new(SessionPanel::new_entity(window, cx))
    });
    register_panel(cx, "sftp", |_, _, _, window, cx| {
        Box::new(SftpPanel::new_entity(window, cx))
    });
}
