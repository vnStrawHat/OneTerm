//! Keyboard handler for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, ClipboardItem, Entity, FocusHandle, Focusable as _, InteractiveElement as _, KeyDownEvent,
};
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

            // Platform modifier: Cmd on macOS, Ctrl on Windows/Linux.
            let plat = mods.control || mods.platform;

            // ── Search (platform+F) ──
            if plat && !mods.shift && !mods.alt && e.keystroke.key.as_str() == "f" {
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

            // The search input emits PressEnter to navigate matches, then
            // propagates Enter by design. Do not let that propagated key reach
            // the terminal while the search input owns focus.
            if matches!(e.keystroke.key.as_str(), "enter" | "return") {
                let search_input = view.read(cx).search_input.clone();
                let search_focused = search_input
                    .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(_w));
                if search_focused {
                    cx.stop_propagation();
                    return;
                }
            }

            // ── Zoom shortcuts (platform +/−/0) ──
            if plat && !mods.alt {
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
                let qs = s.read(cx).query_state();
                let viewport = qs.rows as i32;
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
                // Platform+Shift+Up/Down: scroll 1 line.
                if plat {
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

            // Copy/paste: Ctrl+Shift+C/V (Linux/Windows) or Cmd+C/V (macOS).
            let copy_paste = (mods.control && mods.shift) || (mods.platform && !mods.shift);
            if copy_paste {
                match e.keystroke.key.as_str() {
                    "c" => {
                        if let Some(text) = s.read(cx).selection_text() {
                            if !text.is_empty() {
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            }
                        }
                        cx.stop_propagation();
                        return;
                    }
                    "v" => {
                        if let Some(item) = cx.read_from_clipboard() {
                            if let Some(text) = item.text() {
                                s.update(cx, |s, _| s.paste(&text));
                            }
                        }
                        cx.stop_propagation();
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
                cx.stop_propagation();
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

            let app_cursor = s.read(cx).query_state().mode.contains(TermMode::APP_CURSOR);
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
