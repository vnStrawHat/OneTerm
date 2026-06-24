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
    App, ClipboardItem, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyBinding, MouseButton, MouseDownEvent, NoAction, ParentElement as _, Pixels, Point,
    SharedString, Styled as _, Window, div, px,
};

use async_channel::Receiver;
use myterm2_core::terminal::{KeyMods, KeySpec, NamedKey};
use myterm2_core::{SessionEvent, TerminalSession};

use super::element::{GridMetrics, RowLayoutCache};
use super::terminal_scrollbar::TerminalScrollHandle;
use super::theme::TerminalTheme;
use crate::state::{TerminalBlink, TerminalSettings};

/// Khoảng thời gian nhấp nháy con trỏ (ms) — giống Zed `CURSOR_BLINK_INTERVAL`.
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// View render 1 terminal session (local hoặc ssh — qua `dyn TerminalSession`).
pub struct LocalTerminalView {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    pub(crate) focus: FocusHandle,
    /// Sink layout metrics (Element ghi ở prepaint, mouse handler đọc).
    pub(crate) metrics: Rc<RefCell<GridMetrics>>,
    /// Scrollbar handle — cache scrollback state, apply drag → session.
    pub(crate) scroll_handle: TerminalScrollHandle,
    /// Con trỏ có đang hiện không (blink toggle). True = vẽ, false = ẩn.
    pub(crate) cursor_blink_visible: bool,
    /// Bell indicator — true khi nhận `\x07`, clear khi user gõ phím.
    pub(crate) has_bell: bool,
    /// Scrollbar drag state: Some(drag_start_y) khi đang kéo thumb.
    pub(crate) scrollbar_drag_start: Option<f32>,
    /// Scrollbar last scroll time — để auto-hide sau 2s.
    pub(crate) last_scroll_time: Option<std::time::Instant>,
    /// Vi mode state — khi active, phím di chuyển cursor trong scrollback
    /// thay vì gửi vào PTY. Tương đương Zed `ToggleViMode`.
    pub(crate) vi_mode: bool,
    /// Vi mode cursor position (display row, col) — 0-based từ top.
    pub(crate) vi_cursor: (usize, usize),
    /// Vi mode selection active (v pressed).
    pub(crate) vi_selecting: bool,
    /// URL đang hover (Ctrl held) — để highlight + click mở URL.
    pub(crate) hovered_url: Option<super::url::DetectedUrl>,
    /// Ctrl đang held — track để toggle cursor style.
    pub(crate) ctrl_held: bool,
    /// Last mouse position — để re-detect URL khi Ctrl pressed/released
    /// mà không cần mouse move.
    pub(crate) last_mouse_pos: Option<Point<Pixels>>,
    /// Per-line timestamps (gutter) — indexed by absolute line number (0 = oldest).
    pub(crate) line_times: Vec<String>,
    /// Previous total_lines — detect new lines added.
    pub(crate) prev_total_lines: usize,
    /// Previous cursor line (alacritty Line.0) — detect new line vs modification.
    pub(crate) prev_cursor_line: i32,
    /// Per-row layout cache — skip recompute cho non-dirty rows.
    /// Giống AtlasEngine `_p.rows` (ShapedRow cache).
    pub(crate) row_cache: Rc<RefCell<RowLayoutCache>>,
}

impl LocalTerminalView {
    /// Drain tất cả pending events trong channel — coalesce Output events,
    /// xử lý Clipboard/Bell/Title ngay. Dùng cho frame coalescing:
    /// drain → debounce 1ms → drain lại → notify 1 lần.
    pub(crate) fn drain_coalesced_events(
        rx: &Receiver<SessionEvent>,
        this: &gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncApp,
    ) {
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
                        cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
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
    }

    /// Tạo view từ session entity. Subscribe events → re-render task.
    pub fn new(
        session: Entity<Box<dyn TerminalSession>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();

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
                        // ── Frame coalescing (AtlasEngine-style) ──
                        // Drain tất cả pending events, debounce 1ms để catch
                        // rapid successive events, drain lại, rồi notify 1 lần.
                        // → single render cho tất cả batched output, giảm render
                        // frequency khi `cat` file lớn hoặc rapid PTY output.
                        let s = session_for_spawn.clone();
                        // Drain lần 1 — catch tất cả pending events.
                        Self::drain_coalesced_events(&rx, &this, cx);
                        // Debounce 1ms — catch events arrive ngay sau drain.
                        // 1ms imperceptible cho interactive, nhưng batch được
                        // nhiều rapid outputs hơn (vd network terminal).
                        cx.background_executor()
                            .timer(Duration::from_millis(1))
                            .await;
                        // Drain lần 2 — catch events arrive trong debounce window.
                        Self::drain_coalesced_events(&rx, &this, cx);
                        // NOW notify + update — single render cho tất cả batched output.
                        let _ = this.update(cx, |view, cx| {
                            cx.notify();
                            s.read(cx).scroll_to_bottom();
                            // Track per-line timestamps for gutter display.
                            // Dùng terminal_info() thay vì snapshot() để
                            // KHÔNG clear damage — prepaint cần damage để
                            // biết rows nào dirty để recompute colors.
                            let info = s.read(cx).terminal_info();
                            let total = info.total_lines;
                            let cur_line = info.cursor_line;
                            let now = chrono::Local::now().format("%H:%M:%S").to_string();
                            if total > view.prev_total_lines {
                                // New lines added — push timestamps for each new line.
                                let delta = total - view.prev_total_lines;
                                for _ in 0..delta {
                                    view.line_times.push(now.clone());
                                }
                            } else if total == view.prev_total_lines {
                                if total == 0 {
                                    // nothing
                                } else if cur_line < view.prev_cursor_line {
                                    // Cursor moved down (new line at max scrollback).
                                    // Oldest line dropped, new line at bottom.
                                    if !view.line_times.is_empty() {
                                        view.line_times.remove(0);
                                    }
                                    view.line_times.push(now.clone());
                                } else if total > 0 {
                                    // Same line modified — update cursor's line timestamp.
                                    let abs = (cur_line + total as i32 - 1) as usize;
                                    if abs < view.line_times.len() {
                                        view.line_times[abs] = now.clone();
                                    } else if abs == view.line_times.len() {
                                        view.line_times.push(now.clone());
                                    }
                                }
                            } else {
                                // total < prev — terminal cleared (e.g. `clear` cmd).
                                view.line_times.truncate(total);
                            }
                            // Ensure line_times has exactly `total` entries.
                            while view.line_times.len() < total {
                                view.line_times.push(now.clone());
                            }
                            while view.line_times.len() > total {
                                view.line_times.pop();
                            }
                            view.prev_total_lines = total;
                            view.prev_cursor_line = cur_line;
                        });
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

        // Focus terminal ngay khi tạo — app startup + new tab.
        focus.focus(window, cx);

        Self {
            session,
            focus,
            metrics: Rc::new(RefCell::new(GridMetrics::default())),
            scroll_handle: TerminalScrollHandle::new(),
            cursor_blink_visible: true,
            has_bell: false,
            scrollbar_drag_start: None,
            last_scroll_time: None,
            vi_mode: false,
            vi_cursor: (0, 0),
            vi_selecting: false,
            hovered_url: None,
            ctrl_held: false,
            last_mouse_pos: None,
            line_times: Vec::new(),
            prev_total_lines: 0,
            prev_cursor_line: 0,
            row_cache: Rc::new(RefCell::new(RowLayoutCache::new())),
        }
    }

    /// Map GPUI `Keystroke` → `KeySpec` + `KeyMods`.
    pub(crate) fn map_key(keystroke: &gpui::Keystroke) -> Option<(KeySpec, KeyMods)> {
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

    /// Build GPUI Font từ terminal settings.
    pub(crate) fn font(
        &self,
        settings: &TerminalSettings,
        font_family: &SharedString,
    ) -> gpui::Font {
        let fallbacks = if settings.font_fallbacks.is_empty() {
            None
        } else {
            Some(gpui::FontFallbacks::from_fonts(
                settings
                    .font_fallbacks
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ))
        };
        // Match Zed: disable calt (ligatures) by default for terminal.
        // User can override by adding "calt" to font_features.
        let mut features: Vec<(String, u32)> = vec![("calt".to_string(), 0)];
        for f in &settings.font_features {
            // User-specified features override defaults.
            features.retain(|(tag, _)| tag != f);
            features.push((f.to_string(), 1u32));
        }
        gpui::Font {
            family: font_family.clone().into(),
            weight: settings.font_weight,
            style: gpui::FontStyle::Normal,
            fallbacks,
            features: gpui::FontFeatures(std::sync::Arc::new(features)),
        }
    }

    /// Convert pixel position → (row, col) display (0-based từ top viewport).
    pub(crate) fn pixel_to_grid(metrics: &GridMetrics, pos: Point<Pixels>) -> Option<(f32, f32)> {
        let b = metrics.bounds?;
        if f32::from(metrics.cell_width) == 0.0 || f32::from(metrics.line_height) == 0.0 {
            return None;
        }
        // Trừ gutter_width để x tương đối với grid origin.
        let x = f32::from(pos.x - b.origin.x - metrics.gutter_width);
        let y = f32::from(pos.y - b.origin.y);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let col = x / f32::from(metrics.cell_width);
        let row = y / f32::from(metrics.line_height);
        Some((row, col))
    }

    /// Selection type theo click count + alt.
    pub(crate) fn sel_type(click_count: usize, alt: bool) -> SelectionType {
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
    pub(crate) fn should_show_cursor(&self, focused: bool, settings: &TerminalSettings) -> bool {
        if !focused {
            return true;
        }
        match settings.cursor_blink {
            TerminalBlink::Off => true,
            TerminalBlink::On => self.cursor_blink_visible,
        }
    }

    /// Render custom scrollbar — div overlay ở cạnh phải.
    pub(crate) fn render_scrollbar(
        &mut self,
        _theme: &TerminalTheme,
        metrics: &Rc<RefCell<GridMetrics>>,
        cx: &mut Context<LocalTerminalView>,
    ) -> Option<impl IntoElement> {
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
            || self
                .last_scroll_time
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
                .on_mouse_down(
                    MouseButton::Left,
                    move |e: &MouseDownEvent, _w, cx: &mut App| {
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
                            if lh <= 0.0 {
                                return;
                            }
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
                    },
                )
                .child(
                    div()
                        .id("scrollbar-thumb")
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.0))
                        .w(px(8.0))
                        .h(px(thumb_height))
                        .rounded_sm()
                        .bg(thumb_bg),
                ),
        )
    }
}
