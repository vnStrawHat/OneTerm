//! Mouse handlers cho `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, ClipboardItem, Entity, InteractiveElement as _, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent,
};

use oneterm_core::TerminalSession;

use super::super::element::GridMetrics;
use super::super::url::detect_url_at;
use super::super::view::LocalTerminalView;
use super::map_button;

/// Gắn mouse handlers: down / move / up / modifiers.
pub(crate) fn attach_mouse(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    metrics: Rc<RefCell<GridMetrics>>,
    view: Entity<LocalTerminalView>,
) -> gpui::Stateful<gpui::Div> {
    div.on_mouse_down(MouseButton::Left, {
        let s = session.clone();
        let m = metrics.clone();
        let view = view.clone();
        move |e: &MouseDownEvent, _w, cx: &mut App| {
            let (row, col) = match LocalTerminalView::pixel_to_grid(&m.borrow(), e.position) {
                Some(rc) => rc,
                None => return,
            };
            // Ctrl+click → mở URL (OSC 8 hyperlink hoặc plain text URL).
            if e.modifiers.control {
                let snap = s.read(cx).snapshot();
                if let Some(url) = detect_url_at(
                    &snap.cells,
                    snap.terminal_bounds.num_cols,
                    row as usize,
                    col as usize,
                ) {
                    cx.open_url(&url.url);
                    return;
                }
            }
            s.update(cx, |s, _| {
                s.mouse_down(
                    row,
                    col,
                    map_button(e.button),
                    LocalTerminalView::sel_type(e.click_count, e.modifiers.alt),
                )
            });
            // Trigger re-render để vẽ selection highlight.
            let _ = view.update(cx, |v, cx| {
                v.last_scroll_time = Some(std::time::Instant::now());
                cx.notify();
            });
        }
    })
    .on_mouse_move({
        let s = session.clone();
        let m = metrics.clone();
        let view = view.clone();
        move |e: &MouseMoveEvent, _w, cx: &mut App| {
            // Scrollbar drag: check TRƯỚC selection.
            if view.read(cx).scrollbar_drag_start.is_some() {
                // Mouse ra ngoài terminal + nhả chuột → clear drag.
                if e.pressed_button != Some(MouseButton::Left) {
                    let _ = view.update(cx, |v, cx| {
                        v.scrollbar_drag_start = None;
                        cx.notify();
                    });
                    return;
                }
                // e.position là tọa độ window → trừ terminal origin.
                let track_y = {
                    let gm = m.borrow();
                    match gm.bounds {
                        Some(b) => f32::from(e.position.y - b.origin.y),
                        None => return,
                    }
                };
                let _ = view.update(cx, |v, cx| {
                    let (total, vp, _, lh) = v.scroll_handle.state_info();
                    if lh <= 0.0 {
                        return;
                    }
                    let track_h = vp as f32 * lh;
                    let max_off = total.saturating_sub(vp);
                    let frac = 1.0 - ((track_y / track_h).clamp(0.0, 1.0));
                    let new_offset = (frac * max_off as f32).round() as usize;
                    v.scroll_handle.update(total, vp, new_offset, lh);
                    v.scroll_handle.future_display_offset.set(Some(new_offset));
                    v.last_scroll_time = Some(std::time::Instant::now());
                    cx.notify();
                });
                return;
            }
            // Normal mouse move: selection drag / hover.
            let (row, col) = match LocalTerminalView::pixel_to_grid(&m.borrow(), e.position) {
                Some(rc) => rc,
                None => {
                    // Mouse outside grid — clear hover + save pos.
                    let _ = view.update(cx, |v, cx| {
                        v.last_mouse_pos = Some(e.position);
                        if v.hovered_url.is_some() || v.ctrl_held {
                            v.hovered_url = None;
                            v.ctrl_held = false;
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            if e.pressed_button == Some(MouseButton::Left) {
                s.update(cx, |s, _| s.mouse_drag(row, col));
                let _ = view.update(cx, |v, cx| {
                    v.last_scroll_time = Some(std::time::Instant::now());
                    cx.notify();
                });
            } else {
                s.update(cx, |s, _| s.mouse_move(row, col));
            }
            // Ctrl+hover URL detection — highlight + cursor pointer.
            super::url::update_hovered_url(&s, &m, &view, e.position, e.modifiers.control, cx);
        }
    })
    .on_mouse_up(MouseButton::Left, {
        let s = session.clone();
        let m = metrics.clone();
        let view = view.clone();
        move |e: &MouseUpEvent, _w, cx: &mut App| {
            // Scrollbar drag: clear FIRST.
            if view.read(cx).scrollbar_drag_start.is_some() {
                let _ = view.update(cx, |v, cx| {
                    v.scrollbar_drag_start = None;
                    cx.notify();
                });
                return;
            }
            let (row, col) = match LocalTerminalView::pixel_to_grid(&m.borrow(), e.position) {
                Some(rc) => rc,
                None => return,
            };
            s.update(cx, |s, _| s.mouse_up(row, col, map_button(e.button)));
            if let Some(text) = s.read(cx).selection_text() {
                if !text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            let _ = view.update(cx, |v, cx| {
                v.last_scroll_time = Some(std::time::Instant::now());
                cx.notify();
            });
        }
    })
}
