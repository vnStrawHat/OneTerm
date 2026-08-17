//! Mouse handlers for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, Entity, InteractiveElement as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
};

use oneterm_terminal::TerminalSession;
use oneterm_terminal::url_policy::{ExternalTargetPolicy, TargetDecision};

use super::super::element::RenderCache;
use super::super::url::detect_url_at;
use super::super::view::LocalTerminalView;
use super::super::view::grid::{pixel_to_grid, sel_type};
use super::{edit, map_button};

fn handle_mouse_down(
    session: &Entity<Box<dyn TerminalSession>>,
    render_cache: &Rc<RefCell<RenderCache>>,
    view: &Entity<LocalTerminalView>,
    e: &MouseDownEvent,
    cx: &mut App,
    button: MouseButton,
    allow_url_open: bool,
) {
    let metrics = render_cache.borrow().metrics;
    let (row, col) = match pixel_to_grid(&metrics, e.position) {
        Some(rc) => rc,
        None => return,
    };

    if allow_url_open && (e.modifiers.control || e.modifiers.platform) {
        // PERF-09: Query only a small window around the click.
        const URL_WINDOW: usize = 5;
        let row_u = row as usize;
        let window_start = row_u.saturating_sub(URL_WINDOW);
        let (cells, num_cols) = session
            .read(cx)
            .query_line_range_cells(window_start, URL_WINDOW * 2 + 1);
        let adjusted_row = row_u - window_start;
        if let Some(mut url) = detect_url_at(&cells, num_cols, adjusted_row, col as usize) {
            url.row += window_start;
            let policy = ExternalTargetPolicy::default();
            match policy.validate(&url.url) {
                TargetDecision::Allow => {
                    cx.open_url(&url.url);
                }
                TargetDecision::Confirm(reason) => {
                    log::warn!(
                        "terminal: URL requires confirmation: {:?} — {}",
                        reason,
                        url.url
                    );
                    // TODO: show confirmation dialog with the target URL.
                }
                TargetDecision::Deny(reason) => {
                    log::warn!("terminal: URL denied: {:?} — {}", reason, url.url);
                }
            }
            return;
        }
    }

    let mods = oneterm_terminal::mouse_encode::MouseModifiers {
        shift: e.modifiers.shift,
        alt: e.modifiers.alt,
        ctrl: e.modifiers.control,
    };
    session.update(cx, |s, _| {
        s.mouse_down(
            row,
            col,
            map_button(button),
            sel_type(e.click_count, e.modifiers.alt),
            mods,
        )
    });
    if matches!(button, MouseButton::Right) {
        cx.stop_propagation();
    }
    // Trigger a re-render to draw the selection highlight.
    let _ = view.update(cx, |v, cx| {
        v.scrollbar.mark_scrolled();
        cx.notify();
    });
}

fn handle_mouse_up(
    session: &Entity<Box<dyn TerminalSession>>,
    render_cache: &Rc<RefCell<RenderCache>>,
    view: &Entity<LocalTerminalView>,
    e: &MouseUpEvent,
    cx: &mut App,
    button: MouseButton,
    copy_selection: bool,
) {
    let metrics = render_cache.borrow().metrics;
    let (row, col) = match pixel_to_grid(&metrics, e.position) {
        Some(rc) => rc,
        None => return,
    };
    let mods = oneterm_terminal::mouse_encode::MouseModifiers {
        shift: e.modifiers.shift,
        alt: e.modifiers.alt,
        ctrl: e.modifiers.control,
    };
    session.update(cx, |s, _| s.mouse_up(row, col, map_button(button), mods));
    if matches!(button, MouseButton::Right) {
        cx.stop_propagation();
    }
    if copy_selection {
        edit::copy_selection(session, cx);
    }
    let _ = view.update(cx, |v, cx| {
        v.scrollbar.mark_scrolled();
        cx.notify();
    });
}

/// End a scrollbar thumb drag if one is in progress. Returns `true` when the
/// event was consumed by the drag.
fn end_scrollbar_drag(view: &Entity<LocalTerminalView>, cx: &mut App) -> bool {
    if !view.read(cx).scrollbar.is_dragging() {
        return false;
    }
    let _ = view.update(cx, |v, cx| {
        v.scrollbar.end_drag();
        cx.notify();
    });
    true
}

/// Attach mouse handlers: down / move / up / modifiers.
pub(crate) fn attach_mouse(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    render_cache: Rc<RefCell<RenderCache>>,
    view: Entity<LocalTerminalView>,
    pass_right_click: bool,
) -> gpui::Stateful<gpui::Div> {
    let div = div.on_mouse_down(MouseButton::Left, {
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &MouseDownEvent, _w, cx: &mut App| {
            handle_mouse_down(&s, &cache, &view, e, cx, MouseButton::Left, true)
        }
    });

    let div = if pass_right_click {
        div.on_mouse_down(MouseButton::Right, {
            let s = session.clone();
            let cache = render_cache.clone();
            let view = view.clone();
            move |e: &MouseDownEvent, _w, cx: &mut App| {
                handle_mouse_down(&s, &cache, &view, e, cx, MouseButton::Right, false)
            }
        })
        .on_mouse_up(MouseButton::Right, {
            let s = session.clone();
            let cache = render_cache.clone();
            let view = view.clone();
            move |e: &MouseUpEvent, _w, cx: &mut App| {
                if end_scrollbar_drag(&view, cx) {
                    return;
                }
                handle_mouse_up(&s, &cache, &view, e, cx, MouseButton::Right, false)
            }
        })
    } else {
        div
    };

    div.on_mouse_move({
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &MouseMoveEvent, _w, cx: &mut App| {
            // Scrollbar drag: check this BEFORE selection.
            if view.read(cx).scrollbar.is_dragging() {
                // Mouse left the terminal and button released → clear drag.
                if e.pressed_button != Some(MouseButton::Left) {
                    end_scrollbar_drag(&view, cx);
                    return;
                }
                // e.position is window coordinates → subtract the terminal origin.
                let track_y = match cache.borrow().metrics.bounds {
                    Some(b) => f32::from(e.position.y - b.origin.y),
                    None => return,
                };
                let _ = view.update(cx, |v, cx| {
                    if v.scrollbar.drag_to(track_y) {
                        cx.notify();
                    }
                });
                return;
            }
            // Normal mouse move: selection drag / hover.
            let metrics = cache.borrow().metrics;
            let (row, col) = match pixel_to_grid(&metrics, e.position) {
                Some(rc) => rc,
                None => {
                    // Mouse outside grid — clear hover + save pos.
                    let _ = view.update(cx, |v, cx| {
                        if v.url_hover.leave(e.position) {
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            if e.pressed_button == Some(MouseButton::Left) {
                let mods = oneterm_terminal::mouse_encode::MouseModifiers {
                    shift: e.modifiers.shift,
                    alt: e.modifiers.alt,
                    ctrl: e.modifiers.control,
                };
                s.update(cx, |s, _| s.mouse_drag(row, col, mods));
                let _ = view.update(cx, |v, cx| {
                    v.scrollbar.mark_scrolled();
                    cx.notify();
                });
            } else {
                let mods = oneterm_terminal::mouse_encode::MouseModifiers {
                    shift: e.modifiers.shift,
                    alt: e.modifiers.alt,
                    ctrl: e.modifiers.control,
                };
                s.update(cx, |s, _| s.mouse_move(row, col, mods));
                if e.pressed_button == Some(MouseButton::Right) {
                    cx.stop_propagation();
                }
            }
            // URL detection on hover — highlight + cursor pointer (Ctrl+click to open).
            super::url::update_hovered_url(&s, &cache, &view, e.position, e.modifiers.control, cx);
        }
    })
    .on_mouse_up(MouseButton::Left, {
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &MouseUpEvent, _w, cx: &mut App| {
            // Scrollbar drag: clear FIRST.
            if end_scrollbar_drag(&view, cx) {
                return;
            }
            handle_mouse_up(&s, &cache, &view, e, cx, MouseButton::Left, true);
        }
    })
}
