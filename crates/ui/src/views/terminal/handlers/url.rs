//! URL hover / click helpers cho `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Entity, InteractiveElement as _, ModifiersChangedEvent};

use myterm2_core::TerminalSession;

use super::super::element::GridMetrics;
use super::super::terminal_view::LocalTerminalView;
use super::super::url::{DetectedUrl, detect_url_at};

/// Update `hovered_url` dựa trên position hiện tại và Ctrl state.
pub(crate) fn update_hovered_url(
    session: &Entity<Box<dyn TerminalSession>>,
    metrics: &Rc<RefCell<GridMetrics>>,
    view: &Entity<LocalTerminalView>,
    position: gpui::Point<gpui::Pixels>,
    ctrl: bool,
    cx: &mut App,
) {
    let new_url = if ctrl {
        let (row, col) = match LocalTerminalView::pixel_to_grid(&metrics.borrow(), position) {
            Some(rc) => rc,
            None => return,
        };
        let snap = session.read(cx).snapshot();
        detect_url_at(
            &snap.cells,
            snap.terminal_bounds.num_cols,
            row as usize,
            col as usize,
        )
    } else {
        None
    };
    let _ = view.update(cx, |v, cx| {
        v.last_mouse_pos = Some(position);
        let changed = v.ctrl_held != ctrl
            || v.hovered_url.as_ref().map(url_identity) != new_url.as_ref().map(url_identity);
        if changed {
            v.ctrl_held = ctrl;
            v.hovered_url = new_url;
            cx.notify();
        }
    });
}

/// Handler `on_modifiers_changed` — re-detect URL khi Ctrl pressed/released.
pub(crate) fn attach_modifiers_changed(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    metrics: Rc<RefCell<GridMetrics>>,
    view: Entity<LocalTerminalView>,
) -> gpui::Stateful<gpui::Div> {
    div.on_modifiers_changed({
        let s = session.clone();
        let m = metrics.clone();
        let view = view.clone();
        move |e: &ModifiersChangedEvent, _w, cx: &mut App| {
            let pos = match view.read(cx).last_mouse_pos {
                Some(p) => p,
                None => return,
            };
            update_hovered_url(&s, &m, &view, pos, e.modifiers.control, cx);
        }
    })
}

fn url_identity(u: &DetectedUrl) -> (&String, usize, usize, usize) {
    (&u.url, u.row, u.start_col, u.end_col)
}
