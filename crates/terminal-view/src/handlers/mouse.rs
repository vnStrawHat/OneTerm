//! Mouse handlers for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, ClipboardItem, Entity, InteractiveElement as _, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent,
};

use oneterm_terminal::TerminalSession;
use oneterm_terminal::url_policy::{ExternalTargetPolicy, TargetDecision};

use super::super::element::GridMetrics;
use super::super::url::detect_url_at;
use super::super::view::LocalTerminalView;
use super::map_button;

/// Attach mouse handlers: down / move / up / modifiers.
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
            // Platform-modifier+click (Cmd on macOS, Ctrl on others) → open URL.
            if e.modifiers.control || e.modifiers.platform {
                // PERF-09: Query only a small window around the click.
                const URL_WINDOW: usize = 5;
                let row_u = row as usize;
                let window_start = row_u.saturating_sub(URL_WINDOW);
                let (cells, num_cols) = s
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
            s.update(cx, |s, _| {
                s.mouse_down(
                    row,
                    col,
                    map_button(e.button),
                    LocalTerminalView::sel_type(e.click_count, e.modifiers.alt),
                    mods,
                )
            });
            // Trigger a re-render to draw the selection highlight.
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
            // Scrollbar drag: check this BEFORE selection.
            if view.read(cx).scrollbar_drag_start.is_some() {
                // Mouse left the terminal and button released → clear drag.
                if e.pressed_button != Some(MouseButton::Left) {
                    let _ = view.update(cx, |v, cx| {
                        v.scrollbar_drag_start = None;
                        cx.notify();
                    });
                    return;
                }
                // e.position is window coordinates → subtract the terminal origin.
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
                let mods = oneterm_terminal::mouse_encode::MouseModifiers {
                    shift: e.modifiers.shift,
                    alt: e.modifiers.alt,
                    ctrl: e.modifiers.control,
                };
                s.update(cx, |s, _| s.mouse_drag(row, col, mods));
                let _ = view.update(cx, |v, cx| {
                    v.last_scroll_time = Some(std::time::Instant::now());
                    cx.notify();
                });
            } else {
                let mods = oneterm_terminal::mouse_encode::MouseModifiers {
                    shift: e.modifiers.shift,
                    alt: e.modifiers.alt,
                    ctrl: e.modifiers.control,
                };
                s.update(cx, |s, _| s.mouse_move(row, col, mods));
            }
            // URL detection on hover — highlight + cursor pointer (Ctrl+click to open).
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
            let mods = oneterm_terminal::mouse_encode::MouseModifiers {
                shift: e.modifiers.shift,
                alt: e.modifiers.alt,
                ctrl: e.modifiers.control,
            };
            s.update(cx, |s, _| s.mouse_up(row, col, map_button(e.button), mods));
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
