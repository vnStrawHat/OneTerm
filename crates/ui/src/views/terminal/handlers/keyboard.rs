//! Keyboard handler for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, ClipboardItem, Entity, FocusHandle, InteractiveElement as _, KeyDownEvent};
use gpui_component::ActiveTheme as _;

use alacritty_terminal::term::TermMode;
use oneterm_core::TerminalSession;
use oneterm_core::terminal::{KeySpec, encode_key};

use super::super::element::GridMetrics;
use super::super::view::LocalTerminalView;
use crate::state::TerminalSettings;

/// Attach the keyboard handler.
pub(crate) fn attach_key(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    metrics: Rc<RefCell<GridMetrics>>,
    view: Entity<LocalTerminalView>,
    _focus: FocusHandle,
) -> gpui::Stateful<gpui::Div> {
    let _ = metrics;
    div.on_key_down({
        let s = session.clone();
        let view = view.clone();
        move |e: &KeyDownEvent, _w, cx: &mut App| {
            let mods = e.keystroke.modifiers;

            // ── Search (Ctrl+F) ──
            if mods.control && !mods.shift && !mods.alt && e.keystroke.key.as_str() == "f" {
                let _ = view.update(cx, |v, cx| {
                    if v.search_active {
                        v.close_search(cx);
                    } else {
                        v.open_search(_w, cx);
                    }
                });
                cx.stop_propagation();
                return;
            }

            // ── Zoom shortcuts (Ctrl +/−/0) ──
            if mods.control && !mods.alt {
                match e.keystroke.key.as_str() {
                    "-" => {
                        let settings_e = TerminalSettings::global(cx);
                        let theme_default = f32::from(cx.theme().mono_font_size);
                        settings_e.update(cx, |st, cx| {
                            st.zoom_out(theme_default);
                            cx.notify();
                        });
                        let _ = view.update(cx, |_, cx| cx.notify());
                        cx.stop_propagation();
                        return;
                    }
                    "=" | "+" => {
                        let settings_e = TerminalSettings::global(cx);
                        let theme_default = f32::from(cx.theme().mono_font_size);
                        settings_e.update(cx, |st, cx| {
                            st.zoom_in(theme_default);
                            cx.notify();
                        });
                        let _ = view.update(cx, |_, cx| cx.notify());
                        cx.stop_propagation();
                        return;
                    }
                    "0" => {
                        let settings_e = TerminalSettings::global(cx);
                        settings_e.update(cx, |st, cx| {
                            st.reset_zoom();
                            cx.notify();
                        });
                        let _ = view.update(cx, |_, cx| cx.notify());
                        cx.stop_propagation();
                        return;
                    }
                    _ => {}
                }
            }

            // ── Scroll keyboard actions ──
            if mods.shift {
                let snap = s.read(cx).snapshot();
                let viewport = snap.terminal_bounds.num_lines as i32;
                match e.keystroke.key.as_str() {
                    "pageup" => {
                        s.update(cx, |s, _| s.scroll(viewport));
                        let _ = view.update(cx, |v, cx| {
                            v.last_scroll_time = Some(std::time::Instant::now());
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    "pagedown" => {
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

            // Ctrl+Shift+C/V copy/paste.
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

            // ── Shift+Insert = paste (X11 convention) ──
            if mods.shift && e.keystroke.key.as_str() == "insert" {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        s.update(cx, |s, _| s.paste(&text));
                    }
                }
                return;
            }

            // IME active (not alt-screen): normal characters are handled by
            // replace_text_in_range, so skip on_key_down to avoid double input.
            if !s.read(cx).is_alt_screen() {
                let m = e.keystroke.modifiers;
                if !m.control && !m.alt && !m.platform {
                    if let Some(ch) = e.keystroke.key_char.as_deref() {
                        if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                            return; // let replace_text_in_range write it
                        }
                    }
                }
            }

            let Some((spec, mods)) = LocalTerminalView::map_key(&e.keystroke) else {
                return;
            };

            // Ctrl+C (without Shift) = SIGINT — use send_ctrl_c().
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

            let app_cursor = s.read(cx).snapshot().mode.contains(TermMode::APP_CURSOR);
            let Some(bytes) = encode_key(&spec, mods, app_cursor) else {
                return;
            };
            s.update(cx, |s, _| s.write(&bytes));

            // Clear the bell indicator when the user presses a key.
            let _ = view.update(cx, |view, cx| {
                if view.has_bell {
                    view.has_bell = false;
                    cx.notify();
                }
            });

            cx.stop_propagation();
        }
    })
}
