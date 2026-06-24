//! Event handlers (mouse, wheel, key, context menu) cho `LocalTerminalView`.
//!
//! Đã tách nhỏ thành module `handlers/`:
//! - `handlers::mouse` — mouse down / move / up
//! - `handlers::scroll` — scroll wheel
//! - `handlers::keyboard` — key down
//! - `handlers::url` — Ctrl+hover URL detection
//! - `handlers::vi` — vi mode key handling
//! - `handlers::menu` — right-click context menu

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, FocusHandle};

use myterm2_core::TerminalSession;

use super::element::GridMetrics;
use super::terminal_view::LocalTerminalView;
use crate::views::terminal::handlers;

/// Gắn event handlers + context menu vào terminal div.
pub(crate) fn attach(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    metrics: Rc<RefCell<GridMetrics>>,
    view: Entity<LocalTerminalView>,
    focus: FocusHandle,
) -> impl gpui::IntoElement {
    let div = handlers::attach_mouse(div, session.clone(), metrics.clone(), view.clone());
    let div = handlers::attach_scroll(div, session.clone(), metrics.clone(), view.clone());
    let div = handlers::url::attach_modifiers_changed(
        div,
        session.clone(),
        metrics.clone(),
        view.clone(),
    );
    let div = handlers::attach_key(div, session.clone(), metrics, view, focus.clone());
    handlers::attach_context_menu(div, session, focus)
}
