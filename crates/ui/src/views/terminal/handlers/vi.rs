//! Vi mode key handling for `LocalTerminalView`.

use gpui::{App, ClipboardItem, Entity};

use alacritty_terminal::selection::SelectionType;
use oneterm_core::TerminalSession;
use oneterm_core::terminal::TerminalMouseButton;

/// Toggle vi mode.
pub fn toggle_vi_mode(
    session: &Entity<Box<dyn TerminalSession>>,
    view: &Entity<super::super::view::LocalTerminalView>,
    cx: &mut App,
) {
    let _ = view.update(cx, |v, cx| {
        v.vi_mode = !v.vi_mode;
        if v.vi_mode {
            let snap = session.read(cx).snapshot();
            v.vi_cursor = (
                snap.cursor.point.line.0 as usize,
                snap.cursor.point.column.0,
            );
            v.vi_selecting = false;
        } else {
            v.vi_selecting = false;
            session.update(cx, |s, _| s.clear_selection());
        }
        cx.notify();
    });
}

/// Handle a key press in vi mode.
/// Returns `true` if the key was consumed.
pub fn handle_vi_key(
    key: &str,
    _key_char: &str,
    session: &Entity<Box<dyn TerminalSession>>,
    view: &Entity<super::super::view::LocalTerminalView>,
    cx: &mut App,
) -> bool {
    match key {
        "escape" => {
            let _ = view.update(cx, |v, cx| {
                v.vi_selecting = false;
                session.update(cx, |s, _| s.clear_selection());
                cx.notify();
            });
            true
        }
        "h" | "left" => {
            let _ = view.update(cx, |v, cx| {
                if v.vi_cursor.1 > 0 {
                    v.vi_cursor.1 -= 1;
                }
                cx.notify();
            });
            true
        }
        "l" | "right" => {
            let _ = view.update(cx, |v, cx| {
                let snap = session.read(cx).snapshot();
                let max_col = snap.terminal_bounds.num_cols.saturating_sub(1);
                if v.vi_cursor.1 < max_col {
                    v.vi_cursor.1 += 1;
                }
                cx.notify();
            });
            true
        }
        "k" | "up" => {
            let _ = view.update(cx, |v, cx| {
                if v.vi_cursor.0 > 0 {
                    v.vi_cursor.0 -= 1;
                } else {
                    session.update(cx, |s, _| s.scroll(1));
                }
                cx.notify();
            });
            true
        }
        "j" | "down" => {
            let _ = view.update(cx, |v, cx| {
                let snap = session.read(cx).snapshot();
                let max_row = snap.terminal_bounds.num_lines.saturating_sub(1);
                if v.vi_cursor.0 < max_row {
                    v.vi_cursor.0 += 1;
                } else {
                    session.update(cx, |s, _| s.scroll(-1));
                }
                cx.notify();
            });
            true
        }
        "0" | "home" => {
            let _ = view.update(cx, |v, cx| {
                v.vi_cursor.1 = 0;
                cx.notify();
            });
            true
        }
        "$" | "end" => {
            let _ = view.update(cx, |v, cx| {
                let snap = session.read(cx).snapshot();
                v.vi_cursor.1 = snap.terminal_bounds.num_cols.saturating_sub(1);
                cx.notify();
            });
            true
        }
        "g" => {
            // gg: scroll to top.
            session.update(cx, |s, _| s.scroll_to_top());
            let _ = view.update(cx, |v, cx| {
                v.vi_cursor.0 = 0;
                cx.notify();
            });
            true
        }
        "G" => {
            session.update(cx, |s, _| s.scroll_to_bottom());
            let _ = view.update(cx, |v, cx| {
                let snap = session.read(cx).snapshot();
                v.vi_cursor.0 = snap.terminal_bounds.num_lines.saturating_sub(1);
                cx.notify();
            });
            true
        }
        "w" => {
            let _ = view.update(cx, |v, cx| {
                let snap = session.read(cx).snapshot();
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
            true
        }
        "b" => {
            let _ = view.update(cx, |v, cx| {
                if v.vi_cursor.1 > 0 {
                    let snap = session.read(cx).snapshot();
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
            true
        }
        "v" => {
            let _ = view.update(cx, |v, cx| {
                v.vi_selecting = !v.vi_selecting;
                if v.vi_selecting {
                    let (row, col) = v.vi_cursor;
                    session.update(cx, |s, _| {
                        s.mouse_down(
                            row as f32,
                            col as f32,
                            TerminalMouseButton::Left,
                            SelectionType::Simple,
                        );
                    });
                } else {
                    session.update(cx, |s, _| s.clear_selection());
                }
                cx.notify();
            });
            true
        }
        "y" => {
            if let Some(text) = session.read(cx).selection_text() {
                if !text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            let _ = view.update(cx, |v, cx| {
                v.vi_selecting = false;
                session.update(cx, |s, _| s.clear_selection());
                cx.notify();
            });
            true
        }
        "q" => {
            let _ = view.update(cx, |v, cx| {
                v.vi_mode = false;
                v.vi_selecting = false;
                session.update(cx, |s, _| s.clear_selection());
                cx.notify();
            });
            true
        }
        _ => {
            // Unknown vi key — swallow to prevent sending to PTY.
            true
        }
    }
}

/// Update the vi selection while selecting.
pub fn update_vi_selection(
    session: &Entity<Box<dyn TerminalSession>>,
    view: &Entity<super::super::view::LocalTerminalView>,
    cx: &mut App,
) {
    let _ = view.update(cx, |v, cx| {
        let (row, col) = v.vi_cursor;
        session.update(cx, |s, _| s.mouse_drag(row as f32, col as f32));
        cx.notify();
    });
}
