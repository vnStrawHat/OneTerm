//! `LocalTerminalView` — GPUI `Render` render `TerminalElement` + wire session
//! events → `cx.notify` + keyboard + mouse/selection/wheel.
//!
//! Giữ `Entity<Box<dyn TerminalSession>>` (không biết local/ssh). #16: render +
//! events + keyboard. #17: mouse/selection/wheel. IME ở #19.
//!
//! Group A: cursor blink (500ms timer), cursor shape config, bell indicator,
//! font features.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use alacritty_terminal::selection::SelectionType;
use gpui::{
    App, ClipboardItem, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, KeyBinding, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, NoAction, ParentElement as _, Pixels, Point, Render, ScrollDelta,
    ScrollWheelEvent, SharedString, Styled as _, Window, div, point, px, size,
};
use gpui_component::ActiveTheme as _;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};

use myterm2_core::terminal::{KeyMods, KeySpec, NamedKey, TerminalMouseButton, encode_key};
use myterm2_core::{SessionEvent, TerminalSession};

use super::terminal_element::{GridMetrics, TerminalElement};
use super::terminal_scrollbar::TerminalScrollHandle;
use super::theme::{TerminalTheme, build_terminal_theme};
use crate::state::{TerminalBlink, TerminalSettings};

/// Khoảng thời gian nhấp nháy con trỏ (ms) — giống Zed `CURSOR_BLINK_INTERVAL`.
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// View render 1 terminal session (local hoặc ssh — qua `dyn TerminalSession`).
pub struct LocalTerminalView {
    session: Entity<Box<dyn TerminalSession>>,
    focus: FocusHandle,
    font_family: SharedString,
    font_size: Pixels,
    line_height_factor: f32,
    /// Sink layout metrics (Element ghi ở prepaint, mouse handler đọc).
    metrics: Rc<RefCell<GridMetrics>>,
    /// Scrollbar handle — cache scrollback state, apply drag → session.
    scroll_handle: TerminalScrollHandle,
    /// Con trỏ có đang hiện không (blink toggle). True = vẽ, false = ẩn.
    cursor_blink_visible: bool,
    /// Bell indicator — true khi nhận `\x07`, clear khi user gõ phím.
    has_bell: bool,
    /// Scrollbar drag state: Some(drag_start_y) khi đang kéo thumb.
    scrollbar_drag_start: Option<f32>,
    /// Scrollbar last scroll time — để auto-hide sau 2s.
    last_scroll_time: Option<std::time::Instant>,
    /// Vi mode state — khi active, phím di chuyển cursor trong scrollback
    /// thay vì gửi vào PTY. Tương đương Zed `ToggleViMode`.
    vi_mode: bool,
    /// Vi mode cursor position (display row, col) — 0-based từ top.
    vi_cursor: (usize, usize),
    /// Vi mode selection active (v pressed).
    vi_selecting: bool,
}

impl LocalTerminalView {
    /// Tạo view từ session entity. Subscribe events → re-render task.
    pub fn new(
        session: Entity<Box<dyn TerminalSession>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let theme = cx.theme().clone();
        let font_family = theme.mono_font_family.clone();
        let font_size = theme.mono_font_size;

        // Tab/Shift+Tab: gpui-component root binds "tab" → focus_next.
        // NoAction trong context "Terminal" (depth cao hơn "Root") → override →
        // Tab rơi vào on_key_down → gửi \t vào PTY (shell auto-complete).
        cx.bind_keys([
            KeyBinding::new("tab", NoAction {}, Some("Terminal")),
            KeyBinding::new("shift-tab", NoAction {}, Some("Terminal")),
        ]);

        // Subscribe session events → cx.notify (re-render) + OSC 52 clipboard.
        // Burst-coalescing: khi output dồn (vd `cat` file lớn), nhiều Wakeup
        // liên tiếp → chỉ notify 1 lần, drain các Output event còn trong queue.
        let rx = session.read(cx).subscribe();
        let session_for_spawn = session.clone();
        cx.spawn(async move |this, cx| {
            while let Ok(ev) = rx.recv().await {
                match ev {
                    SessionEvent::Clipboard(Some(t)) => {
                        let _ = this.update(cx, |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(t));
                        });
                    }
                    SessionEvent::Clipboard(None) => {
                        let _ = this.update(cx, |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                        });
                    }
                    SessionEvent::Output => {
                        // Scroll to bottom + Notify 1 lần rồi drain tất cả
                        // Output event đang chờ trong queue → tránh re-render
                        // từng event khi `cat` file lớn (hàng nghìn Wakeup).
                        let _ = this.update(cx, |_, cx| cx.notify());
                        let s = session_for_spawn.clone();
                        let _ = this.update(cx, |_, cx| {
                            s.read(cx).scroll_to_bottom();
                        });
                        loop {
                            match rx.try_recv() {
                                Ok(SessionEvent::Output) => {} // coalesced
                                Ok(SessionEvent::Clipboard(Some(t))) => {
                                    let _ = this.update(cx, |_, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(t));
                                    });
                                }
                                Ok(SessionEvent::Clipboard(None)) => {
                                    let _ = this.update(cx, |_, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            String::new(),
                                        ));
                                    });
                                }
                                Ok(SessionEvent::Bell) => {
                                    let _ = this.update(cx, |view, cx| {
                                        view.has_bell = true;
                                        cx.notify();
                                    });
                                }
                                Ok(_) => {
                                    // Title/Cwd/Exited/Closed → notify.
                                    let _ = this.update(cx, |_, cx| cx.notify());
                                }
                                Err(_) => break,
                            }
                        }
                        // Follow-up: ConPTY (Windows) buffer output, chỉ flush
                        // khi có interaction. Gửi DSR query (\x1b[6n) để force
                        // ConPTY flush + re-render để bắt data đến muộn.
                        // Multiple flushes + renders cho output lớn (vd cat Cargo.lock).
                        let this_a = this.clone();
                        let this_b = this.clone();
                        let this_c = this.clone();
                        let this_d = this.clone();
                        let this_e = this.clone();
                        let s_flush = session_for_spawn.clone();
                        cx.spawn(async move |cx| {
                            // 50ms: flush ConPTY buffer + render + scroll to bottom
                            cx.background_executor().timer(Duration::from_millis(50)).await;
                            let _ = cx.update(|cx| s_flush.read(cx).flush_pty());
                            let _ = this_a.update(cx, |_, cx| { s_flush.read(cx).scroll_to_bottom(); cx.notify(); });
                            // 100ms: flush again + render
                            cx.background_executor().timer(Duration::from_millis(50)).await;
                            let _ = cx.update(|cx| s_flush.read(cx).flush_pty());
                            let _ = this_b.update(cx, |_, cx| { s_flush.read(cx).scroll_to_bottom(); cx.notify(); });
                            // 200ms: flush + render
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                            let _ = cx.update(|cx| s_flush.read(cx).flush_pty());
                            let _ = this_c.update(cx, |_, cx| { s_flush.read(cx).scroll_to_bottom(); cx.notify(); });
                            // 400ms: flush + render
                            cx.background_executor().timer(Duration::from_millis(200)).await;
                            let _ = cx.update(|cx| s_flush.read(cx).flush_pty());
                            let _ = this_d.update(cx, |_, cx| { s_flush.read(cx).scroll_to_bottom(); cx.notify(); });
                            // 800ms: final flush + render (delay dài cho ConPTY chậm)
                            cx.background_executor().timer(Duration::from_millis(400)).await;
                            let _ = cx.update(|cx| s_flush.read(cx).flush_pty());
                            let _ = this_e.update(cx, |_, cx| { s_flush.read(cx).scroll_to_bottom(); cx.notify(); });
                        }).detach();
                    }
                    SessionEvent::Bell => {
                        let _ = this.update(cx, |view, cx| {
                            view.has_bell = true;
                            cx.notify();
                        });
                    }
                    _ => {
                        let _ = this.update(cx, |_, cx| cx.notify());
                    }
                }
            }
        })
        .detach();

        // Cursor blink timer — toggle visible mỗi 500ms.
        // Chỉ nhấp nháy khi focus + blink on. Timer luôn chạy, View quyết định
        // có vẽ không dựa trên focused + cursor_blink_visible.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(CURSOR_BLINK_INTERVAL_MS))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.cursor_blink_visible = !view.cursor_blink_visible;
                    cx.notify();
                });
            }
        })
        .detach();

        Self {
            session,
            focus,
            font_family,
            font_size,
            line_height_factor: 1.2,
            metrics: Rc::new(RefCell::new(GridMetrics::default())),
            scroll_handle: TerminalScrollHandle::new(),
            cursor_blink_visible: true,
            has_bell: false,
            scrollbar_drag_start: None,
            last_scroll_time: None,
            vi_mode: false,
            vi_cursor: (0, 0),
            vi_selecting: false,
        }
    }

    /// Map GPUI `Keystroke` → `KeySpec` + `KeyMods`.
    fn map_key(keystroke: &Keystroke) -> Option<(KeySpec, KeyMods)> {
        let mods = keystroke.modifiers;
        let keymods = KeyMods {
            shift: mods.shift,
            ctrl: mods.control,
            alt: mods.alt,
        };
        let named = match keystroke.key.as_str() {
            "enter" | "return" => Some(NamedKey::Enter),
            "backspace" => Some(NamedKey::Backspace),
            "delete" => Some(NamedKey::Delete),
            "tab" => Some(NamedKey::Tab),
            "escape" => Some(NamedKey::Escape),
            // GPUI uses "up"/"down"/"left"/"right" (not "arrowup"/...).
            "up" => Some(NamedKey::ArrowUp),
            "down" => Some(NamedKey::ArrowDown),
            "left" => Some(NamedKey::ArrowLeft),
            "right" => Some(NamedKey::ArrowRight),
            "home" => Some(NamedKey::Home),
            "end" => Some(NamedKey::End),
            "pageup" => Some(NamedKey::PageUp),
            "pagedown" => Some(NamedKey::PageDown),
            "insert" => Some(NamedKey::Insert),
            _ => None,
        };
        let spec = if let Some(n) = named {
            KeySpec::Named(n)
        } else {
            let ch = keystroke
                .key_char
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| keystroke.key.clone());
            KeySpec::Character(ch)
        };
        Some((spec, keymods))
    }

    fn font(&self, settings: &TerminalSettings) -> gpui::Font {
        gpui::Font {
            family: self.font_family.clone().into(),
            weight: gpui::FontWeight::default(),
            style: gpui::FontStyle::Normal,
            fallbacks: None,
            features: gpui::FontFeatures(std::sync::Arc::new(
                settings.font_features.iter().map(|f| (f.to_string(), 1u32)).collect()
            )),
        }
    }

    /// Convert pixel position → (row, col) display (0-based từ top viewport).
    fn pixel_to_grid(metrics: &GridMetrics, pos: Point<Pixels>) -> Option<(f32, f32)> {
        let b = metrics.bounds?;
        if f32::from(metrics.cell_width) == 0.0 || f32::from(metrics.line_height) == 0.0 {
            return None;
        }
        let x = f32::from(pos.x - b.origin.x);
        let y = f32::from(pos.y - b.origin.y);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let col = x / f32::from(metrics.cell_width);
        let row = y / f32::from(metrics.line_height);
        Some((row, col))
    }

    /// Selection type theo click count + alt.
    fn sel_type(click_count: usize, alt: bool) -> SelectionType {
        if alt {
            SelectionType::Block
        } else {
            match click_count {
                2 => SelectionType::Semantic,
                n if n >= 3 => SelectionType::Lines,
                _ => SelectionType::Simple,
            }
        }
    }

    /// Quyết định có vẽ cursor không (blink logic).
    /// - Không focus → luôn vẽ (để user thấy cursor ở đâu).
    /// - Focus + blink off → luôn vẽ.
    /// - Focus + blink on → vẽ khi cursor_blink_visible.
    fn should_show_cursor(&self, focused: bool, settings: &TerminalSettings) -> bool {
        if !focused {
            return true;
        }
        match settings.cursor_blink {
            TerminalBlink::Off => true,
            TerminalBlink::On => self.cursor_blink_visible,
        }
    }

    /// Render custom scrollbar — div overlay ở cạnh phải.
    fn render_scrollbar(
        &mut self, _theme: &TerminalTheme, metrics: &Rc<RefCell<GridMetrics>>, cx: &mut Context<LocalTerminalView>) -> Option<impl IntoElement> {
        let (total, viewport, display_offset, line_h) = self.scroll_handle.state_info();

        // Không có scrollback → không hiện scrollbar.
        if total <= viewport || line_h <= 0.0 {
            return None;
        }

        let max_offset = total.saturating_sub(viewport);
        let thumb_ratio = viewport as f32 / total as f32;
        let track_height = viewport as f32 * line_h;
        let thumb_height = (thumb_ratio * track_height).max(24.0);
        let scroll_fraction = if max_offset > 0 {
            display_offset as f32 / max_offset as f32
        } else {
            0.0
        };
        // display_offset=0 → bottom (thumb ở dưới)
        let thumb_top = (1.0 - scroll_fraction) * (track_height - thumb_height);

        // Auto-hide: hiện khi đang drag hoặc scroll gần đây (<2s)
        let now = std::time::Instant::now();
        let is_dragging = self.scrollbar_drag_start.is_some();
        let is_visible = is_dragging
            || self.last_scroll_time
                .map(|t| now.duration_since(t).as_secs_f32() < 3.0)
                .unwrap_or(false);

        if !is_visible {
            return None;
        }

        // Fade out: 2s-3s
        let opacity = if is_dragging {
            1.0
        } else {
            self.last_scroll_time
                .map(|t| {
                    let elapsed = now.duration_since(t).as_secs_f32();
                    if elapsed < 2.0 {
                        1.0
                    } else if elapsed < 3.0 {
                        1.0 - (elapsed - 2.0).powi(4)
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0)
        };

        let thumb_bg = gpui::hsla(0.0, 0.0, 0.5, opacity * 0.8);
        let view = cx.entity();
        let m_down = metrics.clone();

        Some(
            div()
                .id("terminal-scrollbar")
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(12.0))
                .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _w, cx: &mut App| {
                    // e.position = window coords -> subtract terminal origin.
                    let track_y = {
                        let gm = m_down.borrow();
                        match gm.bounds {
                            Some(b) => f32::from(e.position.y - b.origin.y),
                            None => return,
                        }
                    };
                    let _ = view.update(cx, |v, cx| {
                        let (total, vp, _, lh) = v.scroll_handle.state_info();
                        if lh <= 0.0 { return; }
                        let track_h = vp as f32 * lh;
                        let max_off = total.saturating_sub(vp);
                        let frac = 1.0 - ((track_y / track_h).clamp(0.0, 1.0));
                        let new_offset = (frac * max_off as f32).round() as usize;
                        v.scroll_handle.update(total, vp, new_offset, lh);
                        v.scroll_handle.future_display_offset.set(Some(new_offset));
                        v.scrollbar_drag_start = Some(track_y);
                        v.last_scroll_time = Some(std::time::Instant::now());
                        cx.notify();
                    });
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .id("scrollbar-thumb")
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.0))
                        .w(px(8.0))
                        .h(px(thumb_height))
                        .rounded_sm()
                        .bg(thumb_bg)
                )
        )
    }
}

fn map_button(b: MouseButton) -> TerminalMouseButton {
    match b {
        MouseButton::Left => TerminalMouseButton::Left,
        MouseButton::Right => TerminalMouseButton::Right,
        MouseButton::Middle => TerminalMouseButton::Middle,
        MouseButton::Navigate(_) => TerminalMouseButton::Left,
    }
}

impl Focusable for LocalTerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for LocalTerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let theme: TerminalTheme = build_terminal_theme(cx.theme());
        let focused = self.focus.is_focused(window);
        let session = self.session.clone();
        // Đọc settings + extract dữ liệu cần thiết trước khi mutate session.
        let settings_entity = TerminalSettings::global(cx);
        let (font, cursor_visible, bell_enabled, has_bell) = {
            let settings = settings_entity.read(cx);
            (
                self.font(settings),
                self.should_show_cursor(focused, settings),
                settings.bell_enabled,
                self.has_bell,
            )
        };
        let metrics = self.metrics.clone();
        let view = cx.entity();

        // Cập nhật scroll handle từ snapshot (frame trước — metrics đã có
        // line_height từ prepaint lần trước).
        let snap = session.read(cx).snapshot();
        let m = *metrics.borrow();
        self.scroll_handle.update(
            snap.total_lines,
            snap.terminal_bounds.num_lines,
            snap.display_offset,
            f32::from(m.line_height),
        );

        // Áp dụng future_display_offset từ scrollbar drag.
        if let Some(new_offset) = self.scroll_handle.take_future_display_offset() {
            let delta = new_offset as i32 - snap.display_offset as i32;
            if delta != 0 {
                session.update(cx, |s, _| s.scroll(delta));
                // Re-snapshot để cập nhật scroll_handle với display_offset MỚI
                // (trong cùng frame — tránh lag 1 frame).
                let new_snap = session.read(cx).snapshot();
                self.scroll_handle.update(
                    new_snap.total_lines,
                    new_snap.terminal_bounds.num_lines,
                    new_snap.display_offset,
                    f32::from(m.line_height),
                );
            }
        }

        let theme_ref = cx.theme().clone();

        div()
            .id("local-terminal-view")
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .child(TerminalElement::new(
                session.clone(),
                theme.clone(),
                font,
                self.font_size,
                self.line_height_factor,
                focused,
                cursor_visible,
                metrics.clone(),
                cx.entity(),
                self.focus.clone(),
            ))
            // Bell indicator overlay (góc trên-phải).
            .children(if has_bell && bell_enabled {
                Some(div().id("terminal-bell").absolute().top_1().right_2().px_1().py_0().text_xs().text_color(theme_ref.warning).child("🔔"))
            } else {
                None
            })
            // ── Vi mode indicator (góc trên-trái) ──
            .children(if self.vi_mode {
                Some(div().id("terminal-vi-mode").absolute().top_1().left_2().px_2().py_0p5().text_xs().rounded_sm().bg(theme_ref.accent.opacity(0.8)).text_color(theme_ref.foreground).child(if self.vi_selecting { "-- VISUAL --" } else { "-- NORMAL --" }))
            } else {
                None
            })
            // ── Vi mode cursor overlay ──
            .children(if self.vi_mode {
                let m = *metrics.borrow();
                let cw = f32::from(m.cell_width);
                let lh = f32::from(m.line_height);
                if cw > 0.0 && lh > 0.0 {
                    if let Some(bounds) = m.bounds {
                        let x = f32::from(bounds.origin.x) + self.vi_cursor.1 as f32 * cw;
                        let y = f32::from(bounds.origin.y) + self.vi_cursor.0 as f32 * lh;
                        Some(div().id("vi-cursor").absolute().left(px(x)).top(px(y)).w(px(cw)).h(px(lh)).border_1().border_color(theme_ref.accent).rounded_sm())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            })
            // ── Custom scrollbar ──
            .children(self.render_scrollbar(&theme, &metrics, cx))
            // ── Breadcrumb bar (bottom) — cwd path từ OSC 7 ──
            .children({
                let breadcrumb = session.read(cx).breadcrumb_text();
                let fg_process = session.read(cx).foreground_process();
                if let Some(bc) = breadcrumb {
                    let label = if let Some(proc) = &fg_process {
                        format!("{} — {}", proc, bc)
                    } else {
                        bc
                    };
                    Some(
                        div()
                            .id("terminal-breadcrumb")
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .px_2()
                            .text_xs()
                            .text_color(theme_ref.border)
                            .bg(theme_ref.background.opacity(0.9))
                            .child(label)
                    )
                } else {
                    None
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let s = session.clone();
                let m = metrics.clone();
                let view = view.clone();
                move |e: &MouseDownEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    // Ctrl+click trên cell có hyperlink → mở URL.
                    if e.modifiers.control {
                        let snap = s.read(cx).snapshot();
                        let nc = snap.terminal_bounds.num_cols;
                        let idx = (row as usize) * nc + (col as usize);
                        if idx < snap.cells.len() {
                            if let Some(h) = snap.cells[idx].cell.hyperlink() {
                                cx.open_url(h.uri());
                                return;
                            }
                        }
                    }
                    s.update(cx, |s, _| {
                        s.mouse_down(
                            row,
                            col,
                            map_button(e.button),
                            Self::sel_type(e.click_count, e.modifiers.alt),
                        )
                    });
                    // Trigger re-render để vẽ selection highlight.
                    let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
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
                            if lh <= 0.0 { return; }
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
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    if e.pressed_button == Some(MouseButton::Left) {
                        s.update(cx, |s, _| s.mouse_drag(row, col));
                        let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
                    } else {
                        s.update(cx, |s, _| s.mouse_move(row, col));
                    }
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
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    s.update(cx, |s, _| s.mouse_up(row, col, map_button(e.button)));
                    if let Some(text) = s.read(cx).selection_text() {
                        if !text.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        }
                    }
                    let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
                }
            })
            .on_scroll_wheel({
                let s = session.clone();
                let m = metrics.clone();
                let view = view.clone();
                move |e: &ScrollWheelEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
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
                                    snap.cursor.point.column.0 as usize,
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
                                    if v.vi_cursor.1 > 0 { v.vi_cursor.1 -= 1; }
                                    cx.notify();
                                });
                                cx.stop_propagation();
                                return;
                            }
                            ("l", _) | ("right", _) => {
                                let _ = view.update(cx, |v, cx| {
                                    let snap = s.read(cx).snapshot();
                                    let max_col = snap.terminal_bounds.num_cols.saturating_sub(1);
                                    if v.vi_cursor.1 < max_col { v.vi_cursor.1 += 1; }
                                    cx.notify();
                                });
                                cx.stop_propagation();
                                return;
                            }
                            ("k", _) | ("up", _) => {
                                let _ = view.update(cx, |v, cx| {
                                    if v.vi_cursor.0 > 0 { v.vi_cursor.0 -= 1; }
                                    else { s.update(cx, |s, _| s.scroll(1)); }
                                    cx.notify();
                                });
                                cx.stop_propagation();
                                return;
                            }
                            ("j", _) | ("down", _) => {
                                let _ = view.update(cx, |v, cx| {
                                    let snap = s.read(cx).snapshot();
                                    let max_row = snap.terminal_bounds.num_lines.saturating_sub(1);
                                    if v.vi_cursor.0 < max_row { v.vi_cursor.0 += 1; }
                                    else { s.update(cx, |s, _| s.scroll(-1)); }
                                    cx.notify();
                                });
                                cx.stop_propagation();
                                return;
                            }
                            ("0", _) | ("home", _) => {
                                let _ = view.update(cx, |v, cx| { v.vi_cursor.1 = 0; cx.notify(); });
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
                                let _ = view.update(cx, |v, cx| { v.vi_cursor.0 = 0; cx.notify(); });
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
                                            if c == ' ' || c == '\t' { break; }
                                        }
                                        col += 1;
                                    }
                                    // Skip whitespace.
                                    while col < max_col {
                                        let idx = v.vi_cursor.0 * snap.terminal_bounds.num_cols + col;
                                        if idx < snap.cells.len() {
                                            let c = snap.cells[idx].cell.c;
                                            if c != ' ' && c != '\t' { break; }
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
                                                if c != ' ' && c != '\t' { break; }
                                            }
                                            col -= 1;
                                        }
                                        // Skip word.
                                        while col > 0 {
                                            let idx = v.vi_cursor.0 * snap.terminal_bounds.num_cols + col;
                                            if idx < snap.cells.len() {
                                                let c = snap.cells[idx].cell.c;
                                                if c == ' ' || c == '\t' { break; }
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
                                            s.mouse_down(row as f32, col as f32, TerminalMouseButton::Left, SelectionType::Simple);
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
                                let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
                                cx.stop_propagation();
                                return;
                            }
                            "pagedown" => {
                                // Alacritty: Delta(-) = scroll DOWN (toward bottom).
                                s.update(cx, |s, _| s.scroll(-viewport));
                                let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
                                cx.stop_propagation();
                                return;
                            }
                            "home" => {
                                s.update(cx, |s, _| s.scroll_to_top());
                                let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
                                cx.stop_propagation();
                                return;
                            }
                            "end" => {
                                s.update(cx, |s, _| s.scroll_to_bottom());
                                let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
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
                                    let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
                                    cx.stop_propagation();
                                    return;
                                }
                                "down" => {
                                    s.update(cx, |s, _| s.scroll(-1));
                                    let _ = view.update(cx, |v, cx| { v.last_scroll_time = Some(std::time::Instant::now()); cx.notify(); });
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
                    let Some((spec, mods)) = Self::map_key(&e.keystroke) else {
                        return;
                    };
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
                let focus = self.focus.clone();
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
}

// ── IME (#19) ──────────────────────────────────────────────────────────────
use alacritty_terminal::term::TermMode;
use gpui::{EntityInputHandler, UTF16Selection};

impl EntityInputHandler for LocalTerminalView {
    fn text_for_range(
        &mut self,
        _range: std::ops::Range<usize>,
        _adjusted: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        // Alt-screen (vd vim/less): tắt IME.
        let mode = self.session.read(cx).snapshot().mode;
        if mode.contains(TermMode::ALT_SCREEN) {
            None
        } else {
            Some(UTF16Selection {
                range: (0..0),
                reversed: false,
            })
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        self.session
            .read(cx)
            .marked_text()
            .map(|t| 0..t.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.session.update(cx, |s, _| s.clear_marked_text());
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Commit IME hoặc ký tự thường (normal mode). Đây là nguồn ghi tin cậy —
        // on_key_down skip ký tự thường khi IME active để tránh double (aa).
        self.session.update(cx, |s, _| s.commit_text(text));
        // Clear bell khi user gõ.
        if self.has_bell {
            self.has_bell = false;
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        new_text: &str,
        _new_selected: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if new_text.is_empty() {
            self.session.update(cx, |s, _| s.clear_marked_text());
        } else {
            self.session
                .update(cx, |s, _| s.set_marked_text(new_text.to_string()));
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: gpui::Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Bounds<Pixels>> {
        let cur = self.session.read(cx).cursor_bounds()?;
        let m = *self.metrics.borrow();
        let cw = f32::from(m.cell_width).max(1.0);
        let lh = f32::from(m.line_height).max(1.0);
        let x = f32::from(element_bounds.origin.x) + cur.x + cw * range_utf16.start as f32;
        let y = f32::from(element_bounds.origin.y) + cur.y;
        Some(gpui::Bounds::new(point(px(x), px(y)), size(px(cw), px(lh))))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        true
    }
}