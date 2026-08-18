//! URL hover / click helpers for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Entity, InteractiveElement as _, ModifiersChangedEvent};

use oneterm_terminal::TerminalSession;

use super::super::element::RenderCache;
use super::super::url::{DetectedUrl, URL_WINDOW, detect_url_at};
use super::super::view::LocalTerminalView;
use super::super::view::grid::pixel_to_grid;

/// Detect the URL under grid cell `(row, col)`, querying only a small window
/// of lines around the row instead of cloning the entire grid (O(window×cols)
/// vs O(rows×cols)). The window covers wrapped URL detection (URLs rarely
/// span more than 2-3 lines).
pub(crate) fn detect_url_at_cell(
    session: &Entity<Box<dyn TerminalSession>>,
    row: usize,
    col: usize,
    cx: &App,
) -> Option<DetectedUrl> {
    let window_start = row.saturating_sub(URL_WINDOW);
    let (cells, num_cols) = session
        .read(cx)
        .query_line_range_cells(window_start, URL_WINDOW * 2 + 1);
    let adjusted_row = row - window_start;
    detect_url_at(&cells, num_cols, adjusted_row, col).map(|mut u| {
        u.row += window_start;
        u
    })
}

/// Update the view's URL hover state based on the current position.
///
/// URLs are always detected on hover (no Ctrl required). The `ctrl` parameter
/// is still tracked so the view knows when a click would open the URL, but it
/// no longer gates the highlight. The grid is only queried when the pointer
/// moved to another cell or Ctrl changed (PERF-15).
pub(crate) fn update_hovered_url(
    session: &Entity<Box<dyn TerminalSession>>,
    render_cache: &Rc<RefCell<RenderCache>>,
    view: &Entity<LocalTerminalView>,
    position: gpui::Point<gpui::Pixels>,
    ctrl: bool,
    cx: &mut App,
) {
    let metrics = render_cache.borrow().metrics;
    let (row, col) = match pixel_to_grid(&metrics, position) {
        Some(rc) => rc,
        None => return,
    };
    let cell = (row as i32, col as i32);
    let needs_detection = view.update(cx, |v, _| v.url_hover.needs_detection(position, cell, ctrl));
    if !needs_detection {
        return;
    }
    let new_url = detect_url_at_cell(session, row as usize, col as usize, cx);
    view.update(cx, |v, cx| {
        if v.url_hover.set(position, cell, new_url, ctrl) {
            cx.notify();
        }
    });
}

/// `on_modifiers_changed` handler — re-detect the URL when Ctrl is pressed/released.
pub(crate) fn attach_modifiers_changed(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    render_cache: Rc<RefCell<RenderCache>>,
    view: Entity<LocalTerminalView>,
) -> gpui::Stateful<gpui::Div> {
    div.on_modifiers_changed({
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &ModifiersChangedEvent, _w, cx: &mut App| {
            let pos = match view.read(cx).url_hover.last_mouse_pos() {
                Some(p) => p,
                None => return,
            };
            update_hovered_url(&s, &cache, &view, pos, e.modifiers.control, cx);
        }
    })
}
