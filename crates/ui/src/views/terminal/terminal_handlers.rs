//! Event handlers (mouse, wheel, key, context menu) cho `LocalTerminalView`.
//!
//! Tách từ `terminal_render.rs` để giảm độ dài file.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, ClipboardItem, Entity, FocusHandle, InteractiveElement as _, KeyDownEvent,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta,
    ScrollWheelEvent,
};
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};

use alacritty_terminal::selection::SelectionType;
use myterm2_core::TerminalSession;
use myterm2_core::terminal::{KeySpec, TerminalMouseButton, encode_key};

use super::terminal_element::GridMetrics;
use super::terminal_view::LocalTerminalView;
use super::url::detect_url_at;
use crate::state::TerminalSettings;

fn map_button(b: MouseButton) -> TerminalMouseButton {
    match b {
        MouseButton::Left => TerminalMouseButton::Left,
        MouseButton::Right => TerminalMouseButton::Right,
        MouseButton::Middle => TerminalMouseButton::Middle,
        MouseButton::Navigate(_) => TerminalMouseButton::Left,
    }
}

/// Gắn event handlers + context menu vào terminal div.
pub(crate) fn attach(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    metrics: Rc<RefCell<GridMetrics>>,
    view: Entity<LocalTerminalView>,
    focus: FocusHandle,
) -> impl gpui::IntoElement {
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
    .on_mouse_down(MouseButton::Middle, {
        let s = session.clone();
        move |_e: &MouseDownEvent, _w, cx: &mut App| {
            // Middle-click = paste (X11 PRIMARY/CLIPBOARD).
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    s.update(cx, |s, _| s.paste(&text));
                }
            }
        }
    })
    // Mouse move — scrollbar drag HOẶC selection drag.
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
            let ctrl = e.modifiers.control;
            let new_url = if ctrl {
                let snap = s.read(cx).snapshot();
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
                v.last_mouse_pos = Some(e.position);
                let changed = v.ctrl_held != ctrl
                    || v.hovered_url
                        .as_ref()
                        .map(|u| (&u.url, u.row, u.start_col, u.end_col))
                        != new_url
                            .as_ref()
                            .map(|u| (&u.url, u.row, u.start_col, u.end_col));
                if changed {
                    v.ctrl_held = ctrl;
                    v.hovered_url = new_url;
                    cx.notify();
                }
            });
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
    // Modifier changed (Ctrl pressed/released) — re-detect URL
    // tại last mouse position mà không cần mouse move.
    .on_modifiers_changed({
        let s = session.clone();
        let m = metrics.clone();
        let view = view.clone();
        move |e: &ModifiersChangedEvent, _w, cx: &mut App| {
            let ctrl = e.modifiers.control;
            let pos = match view.read(cx).last_mouse_pos {
                Some(p) => p,
                None => return,
            };
            let new_url = if ctrl {
                let (row, col) = match LocalTerminalView::pixel_to_grid(&m.borrow(), pos) {
                    Some(rc) => rc,
                    None => return,
                };
                let snap = s.read(cx).snapshot();
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
                let changed = v.ctrl_held != ctrl
                    || v.hovered_url
                        .as_ref()
                        .map(|u| (&u.url, u.row, u.start_col, u.end_col))
                        != new_url
                            .as_ref()
                            .map(|u| (&u.url, u.row, u.start_col, u.end_col));
                if changed {
                    v.ctrl_held = ctrl;
                    v.hovered_url = new_url;
                    cx.notify();
                }
            });
        }
    })
    .on_scroll_wheel({
        let s = session.clone();
        let m = metrics.clone();
        let view = view.clone();
        move |e: &ScrollWheelEvent, _w, cx: &mut App| {
            let (row, col) = match LocalTerminalView::pixel_to_grid(&m.borrow(), e.position) {
                Some(rc) => rc,
                None => return,
            };
            let line_h = f32::from(m.borrow().line_height);
            let delta_y = match e.delta {
                ScrollDelta::Pixels(p) => {
                    if line_h > 0.0 {
                        f32::from(p.y) / line_h
                    } else {
                        0.0
                    }
                }
                ScrollDelta::Lines(l) => l.y,
            };
            // Apply scroll_multiplier setting.
            let multiplier = TerminalSettings::global(cx).read(cx).scroll_multiplier;
            let delta_y = delta_y * multiplier;
            if delta_y.abs() >= 0.001 {
                s.update(cx, |s, _| s.wheel(delta_y as f64, row, col));
                // Re-render + update scrollbar visibility.
                let _ = view.update(cx, |v, cx| {
                    v.last_scroll_time = Some(std::time::Instant::now());
                    cx.notify();
                });
            }
        }
    })
    .on_key_down({
        let s = session.clone();
        let view = view.clone();
        move |e: &KeyDownEvent, _w, cx: &mut App| {
            let mods = e.keystroke.modifiers;

            // ── Vi mode (Group H) ──
            // Ctrl+Shift+Space: toggle vi mode.
            if mods.control && mods.shift && e.keystroke.key.as_str() == "space" {
                let _ = view.update(cx, |v, cx| {
                    v.vi_mode = !v.vi_mode;
                    if v.vi_mode {
                        // Enter vi mode: set cursor to current cursor position.
                        let snap = s.read(cx).snapshot();
                        v.vi_cursor = (
                            snap.cursor.point.line.0 as usize,
                            snap.cursor.point.column.0,
                        );
                        v.vi_selecting = false;
                    } else {
                        // Exit vi mode: clear any selection.
                        v.vi_selecting = false;
                        s.update(cx, |s, _| s.clear_selection());
                    }
                    cx.notify();
                });
                cx.stop_propagation();
                return;
            }

            // Vi mode: intercept keys for navigation.
            if view.read(cx).vi_mode {
                let key = e.keystroke.key.as_str();
                let key_char = e.keystroke.key_char.as_deref().unwrap_or("");
                match (key, key_char) {
                    ("escape", _) => {
                        let _ = view.update(cx, |v, cx| {
                            v.vi_selecting = false;
                            s.update(cx, |s, _| s.clear_selection());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("h", _) | ("left", _) => {
                        let _ = view.update(cx, |v, cx| {
                            if v.vi_cursor.1 > 0 {
                                v.vi_cursor.1 -= 1;
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("l", _) | ("right", _) => {
                        let _ = view.update(cx, |v, cx| {
                            let snap = s.read(cx).snapshot();
                            let max_col = snap.terminal_bounds.num_cols.saturating_sub(1);
                            if v.vi_cursor.1 < max_col {
                                v.vi_cursor.1 += 1;
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("k", _) | ("up", _) => {
                        let _ = view.update(cx, |v, cx| {
                            if v.vi_cursor.0 > 0 {
                                v.vi_cursor.0 -= 1;
                            } else {
                                s.update(cx, |s, _| s.scroll(1));
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("j", _) | ("down", _) => {
                        let _ = view.update(cx, |v, cx| {
                            let snap = s.read(cx).snapshot();
                            let max_row = snap.terminal_bounds.num_lines.saturating_sub(1);
                            if v.vi_cursor.0 < max_row {
                                v.vi_cursor.0 += 1;
                            } else {
                                s.update(cx, |s, _| s.scroll(-1));
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("0", _) | ("home", _) => {
                        let _ = view.update(cx, |v, cx| {
                            v.vi_cursor.1 = 0;
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("$", _) | ("end", _) => {
                        let _ = view.update(cx, |v, cx| {
                            let snap = s.read(cx).snapshot();
                            v.vi_cursor.1 = snap.terminal_bounds.num_cols.saturating_sub(1);
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("g", _) => {
                        // gg: scroll to top.
                        s.update(cx, |s, _| s.scroll_to_top());
                        let _ = view.update(cx, |v, cx| {
                            v.vi_cursor.0 = 0;
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("G", _) => {
                        s.update(cx, |s, _| s.scroll_to_bottom());
                        let _ = view.update(cx, |v, cx| {
                            let snap = s.read(cx).snapshot();
                            v.vi_cursor.0 = snap.terminal_bounds.num_lines.saturating_sub(1);
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("w", _) => {
                        // Jump word forward.
                        let _ = view.update(cx, |v, cx| {
                            let snap = s.read(cx).snapshot();
                            let max_col = snap.terminal_bounds.num_cols;
                            let mut col = v.vi_cursor.1 + 1;
                            // Skip current word.
                            while col < max_col {
                                let idx = v.vi_cursor.0 * snap.terminal_bounds.num_cols + col;
                                if idx < snap.cells.len() {
                                    let c = snap.cells[idx].cell.c;
                                    if c == ' ' || c == '\t' {
                                        break;
                                    }
                                }
                                col += 1;
                            }
                            // Skip whitespace.
                            while col < max_col {
                                let idx = v.vi_cursor.0 * snap.terminal_bounds.num_cols + col;
                                if idx < snap.cells.len() {
                                    let c = snap.cells[idx].cell.c;
                                    if c != ' ' && c != '\t' {
                                        break;
                                    }
                                }
                                col += 1;
                            }
                            v.vi_cursor.1 = col.min(max_col.saturating_sub(1));
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("b", _) => {
                        // Jump word backward.
                        let _ = view.update(cx, |v, cx| {
                            if v.vi_cursor.1 > 0 {
                                let snap = s.read(cx).snapshot();
                                let mut col = v.vi_cursor.1.saturating_sub(1);
                                // Skip whitespace.
                                while col > 0 {
                                    let idx = v.vi_cursor.0 * snap.terminal_bounds.num_cols + col;
                                    if idx < snap.cells.len() {
                                        let c = snap.cells[idx].cell.c;
                                        if c != ' ' && c != '\t' {
                                            break;
                                        }
                                    }
                                    col -= 1;
                                }
                                // Skip word.
                                while col > 0 {
                                    let idx = v.vi_cursor.0 * snap.terminal_bounds.num_cols + col;
                                    if idx < snap.cells.len() {
                                        let c = snap.cells[idx].cell.c;
                                        if c == ' ' || c == '\t' {
                                            break;
                                        }
                                    }
                                    col -= 1;
                                }
                                v.vi_cursor.1 = col;
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("v", _) => {
                        // Toggle selection mode.
                        let _ = view.update(cx, |v, cx| {
                            v.vi_selecting = !v.vi_selecting;
                            if v.vi_selecting {
                                let (row, col) = v.vi_cursor;
                                s.update(cx, |s, _| {
                                    s.mouse_down(
                                        row as f32,
                                        col as f32,
                                        TerminalMouseButton::Left,
                                        SelectionType::Simple,
                                    );
                                });
                            } else {
                                s.update(cx, |s, _| s.clear_selection());
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("y", _) => {
                        // Yank (copy) selection.
                        if let Some(text) = s.read(cx).selection_text() {
                            if !text.is_empty() {
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            }
                        }
                        let _ = view.update(cx, |v, cx| {
                            v.vi_selecting = false;
                            s.update(cx, |s, _| s.clear_selection());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    ("q", _) => {
                        // q: exit vi mode.
                        let _ = view.update(cx, |v, cx| {
                            v.vi_mode = false;
                            v.vi_selecting = false;
                            s.update(cx, |s, _| s.clear_selection());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    _ => {
                        // Unknown vi key — swallow to prevent sending to PTY.
                        cx.stop_propagation();
                        return;
                    }
                }
            }

            // Update vi selection if selecting.
            if view.read(cx).vi_selecting {
                let _ = view.update(cx, |v, cx| {
                    let (row, col) = v.vi_cursor;
                    s.update(cx, |s, _| s.mouse_drag(row as f32, col as f32));
                    cx.notify();
                });
            }

            // ── Scroll keyboard actions (Group C) ──
            // Shift+PageUp/Down: scroll scrollback 1 viewport.
            // Shift+Home/End: scroll to top/bottom.
            // Ctrl+Shift+Up/Down: scroll 1 line.
            if mods.shift {
                let snap = s.read(cx).snapshot();
                let viewport = snap.terminal_bounds.num_lines as i32;
                match e.keystroke.key.as_str() {
                    "pageup" => {
                        // Alacritty: Delta(+) = scroll UP (back in history).
                        s.update(cx, |s, _| s.scroll(viewport));
                        let _ = view.update(cx, |v, cx| {
                            v.last_scroll_time = Some(std::time::Instant::now());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    "pagedown" => {
                        // Alacritty: Delta(-) = scroll DOWN (toward bottom).
                        s.update(cx, |s, _| s.scroll(-viewport));
                        let _ = view.update(cx, |v, cx| {
                            v.last_scroll_time = Some(std::time::Instant::now());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    "home" => {
                        s.update(cx, |s, _| s.scroll_to_top());
                        let _ = view.update(cx, |v, cx| {
                            v.last_scroll_time = Some(std::time::Instant::now());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    "end" => {
                        s.update(cx, |s, _| s.scroll_to_bottom());
                        let _ = view.update(cx, |v, cx| {
                            v.last_scroll_time = Some(std::time::Instant::now());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    _ => {}
                }
                // Ctrl+Shift+Up/Down: scroll 1 line.
                if mods.control {
                    match e.keystroke.key.as_str() {
                        "up" => {
                            s.update(cx, |s, _| s.scroll(1));
                            let _ = view.update(cx, |v, cx| {
                                v.last_scroll_time = Some(std::time::Instant::now());
                                cx.notify();
                            });
                            cx.stop_propagation();
                            return;
                        }
                        "down" => {
                            s.update(cx, |s, _| s.scroll(-1));
                            let _ = view.update(cx, |v, cx| {
                                v.last_scroll_time = Some(std::time::Instant::now());
                                cx.notify();
                            });
                            cx.stop_propagation();
                            return;
                        }
                        _ => {}
                    }
                }
            }
            // Ctrl+Shift+C = copy, Ctrl+Shift+V = paste (terminal: Ctrl+C
            // là SIGINT nên dùng Shift).
            if mods.control && mods.shift {
                match e.keystroke.key.as_str() {
                    "c" => {
                        if let Some(text) = s.read(cx).selection_text() {
                            if !text.is_empty() {
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            }
                        }
                        return;
                    }
                    "v" => {
                        if let Some(item) = cx.read_from_clipboard() {
                            if let Some(text) = item.text() {
                                s.update(cx, |s, _| s.paste(&text));
                            }
                        }
                        return;
                    }
                    _ => {}
                }
            }
            // IME active (không alt-screen): ký tự thường do
            // replace_text_in_range lo → skip on_key_down để tránh double.
            // Alt-screen (vim/less): IME tắt → on_key_down lo ký tự thường.
            if !s.read(cx).is_alt_screen() {
                let m = e.keystroke.modifiers;
                if !m.control && !m.alt && !m.platform {
                    if let Some(ch) = e.keystroke.key_char.as_deref() {
                        if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                            return; // để replace_text_in_range ghi
                        }
                    }
                }
            }
            let Some((spec, mods)) = LocalTerminalView::map_key(&e.keystroke) else {
                return;
            };
            // Ctrl+C (không Shift) = SIGINT — dùng send_ctrl_c()
            // thay vì encode_key(\x03) để tránh ConPTY gửi
            // CTRL_C_EVENT đến shell (causes shell exit).
            // GenerateConsoleCtrlEvent đi qua console subsystem
            // → shell's SetConsoleCtrlHandler handle đúng.
            if mods.ctrl && !mods.shift {
                if let KeySpec::Character(ch) = &spec {
                    if ch == "c" || ch == "C" {
                        s.update(cx, |s, _| s.send_ctrl_c());
                        let _ = view.update(cx, |view, cx| {
                            if view.has_bell {
                                view.has_bell = false;
                                cx.notify();
                            }
                        });
                        cx.stop_propagation();
                        return;
                    }
                }
            }
            let Some(bytes) = encode_key(&spec, mods) else {
                return;
            };
            s.update(cx, |s, _| s.write(&bytes));
            // Clear bell indicator khi user gõ phím.
            let _ = view.update(cx, |view, cx| {
                if view.has_bell {
                    view.has_bell = false;
                    cx.notify();
                }
            });
            // Ngăn GPUI xử lý tiếp (vd Tab = focus traversal, arrow =
            // scroll). Nếu không stop_propagation, focus sẽ bị chuyển đi.
            cx.stop_propagation();
        }
    })
    // Right-click context menu: Copy / Paste / Select All / Clear.
    .context_menu({
        let session = session.clone();
        let focus = focus.clone();
        move |menu, _window, cx| {
            let has_selection = session
                .read(cx)
                .selection_text()
                .map(|t| !t.is_empty())
                .unwrap_or(false);

            menu.item(
                PopupMenuItem::new("Copy")
                    .disabled(!has_selection)
                    .on_click({
                        let s = session.clone();
                        let f = focus.clone();
                        move |_, window, cx| {
                            if let Some(text) = s.read(cx).selection_text() {
                                if !text.is_empty() {
                                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                                }
                            }
                            window.focus(&f, cx);
                        }
                    }),
            )
            .item(PopupMenuItem::new("Paste").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            s.update(cx, |s, _| s.paste(&text));
                        }
                    }
                    window.focus(&f, cx);
                }
            }))
            .separator()
            .item(PopupMenuItem::new("Select All").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    s.update(cx, |s, _| s.select_all());
                    window.focus(&f, cx);
                }
            }))
            .item(PopupMenuItem::new("Clear").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    s.update(cx, |s, _| s.clear());
                    window.focus(&f, cx);
                }
            }))
        }
    })
}
