//! Event handlers cho `LocalTerminalView`.
//!
//! Tách từ `terminal_handlers.rs` để giảm độ dài file.

pub mod keyboard;
pub mod menu;
pub mod mouse;
pub mod scroll;
pub mod url;
pub mod vi;

pub(crate) use keyboard::attach_key;
pub(crate) use menu::attach_context_menu;
pub(crate) use mouse::attach_mouse;
pub(crate) use scroll::attach_scroll;

use gpui::MouseButton;
use myterm2_core::terminal::TerminalMouseButton;

/// Map GPUI `MouseButton` sang `TerminalMouseButton`.
pub(crate) fn map_button(b: MouseButton) -> TerminalMouseButton {
    match b {
        MouseButton::Left => TerminalMouseButton::Left,
        MouseButton::Right => TerminalMouseButton::Right,
        MouseButton::Middle => TerminalMouseButton::Middle,
        MouseButton::Navigate(_) => TerminalMouseButton::Left,
    }
}
