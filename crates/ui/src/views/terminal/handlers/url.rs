//! URL hover / click helpers for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Entity, InteractiveElement as _, ModifiersChangedEvent};

use oneterm_core::TerminalSession;

use super::super::element::GridMetrics;
use super::super::url::{DetectedUrl, detect_url_at};
use super::super::view::LocalTerminalView;

/// Update `hovered_url` based on the current position.
///
/// URLs are always detected on hover (no Ctrl required). The `ctrl` parameter
/// is still tracked in `ctrl_held` so the view knows when a click would open
/// the URL, but it no longer gates the highlight.
pub(crate) fn update_hovered_url(
    session: &Entity<Box<dyn TerminalSession>>,
    metrics: &Rc<RefCell<GridMetrics>>,
    view: &Entity<LocalTerminalView>,
    position: gpui::Point<gpui::Pixels>,
    ctrl: bool,
    cx: &mut App,
) {
    let (row, col) = match LocalTerminalView::pixel_to_grid(&metrics.borrow(), position) {
        Some(rc) => rc,
        None => return,
    };
    let snap = session.read(cx).snapshot_query();
    let new_url = detect_url_at(
        &snap.cells,
        snap.terminal_bounds.num_cols,
        row as usize,
        col as usize,
    );
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

/// `on_modifiers_changed` handler — re-detect the URL when Ctrl is pressed/released.
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
