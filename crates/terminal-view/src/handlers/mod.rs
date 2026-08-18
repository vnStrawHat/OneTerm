//! Event handlers for `LocalTerminalView`: `attach` wires the mouse, scroll,
//! modifier, keyboard, and context-menu handlers into the terminal div.
//! Each handler receives the view's shared [`RenderCache`] to read the layout
//! metrics written by the element in prepaint.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{AnyElement, Entity, FocusHandle, IntoElement as _, Modifiers, MouseButton};
use oneterm_terminal::TerminalSession;
use oneterm_terminal::mouse_encode::MouseModifiers;

use super::element::RenderCache;
use super::view::LocalTerminalView;

pub(crate) mod edit;
mod keyboard;
pub(crate) mod menu;
mod mouse;
mod scroll;
mod url;

/// Attach all event handlers plus the context menu to the terminal div.
pub(crate) fn attach(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    render_cache: Rc<RefCell<RenderCache>>,
    view: Entity<LocalTerminalView>,
    focus: FocusHandle,
    split_ctx: Option<super::space::SplitContext>,
    show_context_menu: bool,
) -> AnyElement {
    let div = mouse::attach_mouse(
        div,
        session.clone(),
        render_cache.clone(),
        view.clone(),
        !show_context_menu,
    );
    let div = scroll::attach_scroll(div, session.clone(), render_cache.clone(), view.clone());
    let div =
        url::attach_modifiers_changed(div, session.clone(), render_cache.clone(), view.clone());
    let div = keyboard::attach_key(div, session.clone(), view);
    if show_context_menu {
        menu::attach_context_menu(div, session, focus, split_ctx).into_any_element()
    } else {
        div.into_any_element()
    }
}

/// Map a GPUI `MouseButton` to a `TerminalMouseButton`.
fn map_button(b: MouseButton) -> oneterm_terminal::TerminalMouseButton {
    match b {
        MouseButton::Left => oneterm_terminal::TerminalMouseButton::Left,
        MouseButton::Right => oneterm_terminal::TerminalMouseButton::Right,
        MouseButton::Middle => oneterm_terminal::TerminalMouseButton::Middle,
        MouseButton::Navigate(_) => oneterm_terminal::TerminalMouseButton::Left,
    }
}

/// The modifier subset the mouse encoders report (Shift/Alt/Ctrl).
fn mouse_mods(m: &Modifiers) -> MouseModifiers {
    MouseModifiers {
        shift: m.shift,
        alt: m.alt,
        ctrl: m.control,
    }
}
