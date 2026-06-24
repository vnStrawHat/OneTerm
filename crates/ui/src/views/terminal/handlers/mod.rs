//! Event handlers cho `LocalTerminalView`.
//!
//! Tách từ `terminal_handlers.rs` để giảm độ dài file.
//! `attach` ở đây là facade gắn tất cả handlers vào terminal div.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, FocusHandle, MouseButton};
use myterm2_core::TerminalSession;

use super::element::GridMetrics;
use super::view::LocalTerminalView;

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

/// Gắn tất cả event handlers + context menu vào terminal div.
pub(crate) fn attach(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    metrics: Rc<RefCell<GridMetrics>>,
    view: Entity<LocalTerminalView>,
    focus: FocusHandle,
) -> impl gpui::IntoElement {
    let div = attach_mouse(div, session.clone(), metrics.clone(), view.clone());
    let div = attach_scroll(div, session.clone(), metrics.clone(), view.clone());
    let div = url::attach_modifiers_changed(div, session.clone(), metrics.clone(), view.clone());
    let div = attach_key(div, session.clone(), metrics, view, focus.clone());
    attach_context_menu(div, session, focus)
}

/// Map GPUI `MouseButton` sang `TerminalMouseButton`.
pub(crate) fn map_button(b: MouseButton) -> myterm2_core::terminal::TerminalMouseButton {
    match b {
        MouseButton::Left => myterm2_core::terminal::TerminalMouseButton::Left,
        MouseButton::Right => myterm2_core::terminal::TerminalMouseButton::Right,
        MouseButton::Middle => myterm2_core::terminal::TerminalMouseButton::Middle,
        MouseButton::Navigate(_) => myterm2_core::terminal::TerminalMouseButton::Left,
    }
}
