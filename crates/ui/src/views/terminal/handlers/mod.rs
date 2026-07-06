//! Event handlers for `LocalTerminalView`.
//!
//! Split out from `terminal_handlers.rs` to keep file length down.
//! `attach` here is a facade that wires all handlers into the terminal div.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, FocusHandle, MouseButton};
use oneterm_core::TerminalSession;

use super::element::GridMetrics;
use super::view::LocalTerminalView;

pub mod keyboard;
pub mod menu;
pub mod mouse;
pub mod scroll;
pub mod url;

pub(crate) use keyboard::attach_key;
pub(crate) use menu::attach_context_menu;
pub(crate) use mouse::attach_mouse;
pub(crate) use scroll::attach_scroll;

/// Attach all event handlers plus the context menu to the terminal div.
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

/// Map a GPUI `MouseButton` to a `TerminalMouseButton`.
pub(crate) fn map_button(b: MouseButton) -> oneterm_core::terminal::TerminalMouseButton {
    match b {
        MouseButton::Left => oneterm_core::terminal::TerminalMouseButton::Left,
        MouseButton::Right => oneterm_core::terminal::TerminalMouseButton::Right,
        MouseButton::Middle => oneterm_core::terminal::TerminalMouseButton::Middle,
        MouseButton::Navigate(_) => oneterm_core::terminal::TerminalMouseButton::Left,
    }
}
