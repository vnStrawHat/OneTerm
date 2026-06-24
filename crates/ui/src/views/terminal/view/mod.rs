//! `LocalTerminalView` — GPUI view render 1 terminal session (local/ssh).
//!
//! Module gốc `view.rs` đã được tách thành `view/`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{ClipboardItem, Context, Entity, FocusHandle, KeyBinding, NoAction, Window};

use async_channel::Receiver;
use myterm2_core::{SessionEvent, TerminalSession};

use super::element::{GridMetrics, RowLayoutCache};
use super::scrollbar::TerminalScrollHandle;

pub(crate) mod cursor;
pub(crate) mod font;
pub(crate) mod grid;
pub(crate) mod key;
pub(crate) mod scrollbar;

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
    /// thay vì gửi vào PTY.
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
    pub(crate) last_mouse_pos: Option<gpui::Point<gpui::Pixels>>,
    /// Per-line timestamps (gutter) — indexed by absolute line number (0 = oldest).
    pub(crate) line_times: Vec<String>,
    /// Previous total_lines — detect new lines added.
    pub(crate) prev_total_lines: usize,
    /// Previous cursor line (alacritty Line.0) — detect new line vs modification.
    pub(crate) prev_cursor_line: i32,
    /// Per-row layout cache — skip recompute cho non-dirty rows.
    pub(crate) row_cache: Rc<RefCell<RowLayoutCache>>,
}

impl LocalTerminalView {
    /// Tạo view từ session entity. Subscribe events → re-render task.
    pub fn new(
        session: Entity<Box<dyn TerminalSession>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();

        cx.bind_keys([
            KeyBinding::new("tab", NoAction {}, Some("Terminal")),
            KeyBinding::new("shift-tab", NoAction {}, Some("Terminal")),
        ]);

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
                        let s = session_for_spawn.clone();
                        Self::drain_coalesced_events(&rx, &this, cx);
                        cx.background_executor()
                            .timer(Duration::from_millis(1))
                            .await;
                        Self::drain_coalesced_events(&rx, &this, cx);
                        let _ = this.update(cx, |view, cx| {
                            cx.notify();
                            s.read(cx).scroll_to_bottom();
                            view.update_line_times(&s, cx);
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

    /// Drain tất cả pending events trong channel — coalesce Output events,
    /// xử lý Clipboard/Bell/Title ngay.
    pub(crate) fn drain_coalesced_events(
        rx: &Receiver<SessionEvent>,
        this: &gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncApp,
    ) {
        loop {
            match rx.try_recv() {
                Ok(SessionEvent::Output) => {}
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
                    let _ = this.update(cx, |_, cx| cx.notify());
                }
                Err(_) => break,
            }
        }
    }

    /// Cập nhật `line_times` khi output mới.
    fn update_line_times(&mut self, s: &Entity<Box<dyn TerminalSession>>, cx: &mut Context<Self>) {
        let _ = cx;
        let info = s.read(cx).terminal_info();
        let total = info.total_lines;
        let cur_line = info.cursor_line;
        let now = chrono::Local::now().format("%H:%M:%S").to_string();
        if total > self.prev_total_lines {
            let delta = total - self.prev_total_lines;
            for _ in 0..delta {
                self.line_times.push(now.clone());
            }
        } else if total == self.prev_total_lines {
            if total != 0 {
                if cur_line < self.prev_cursor_line {
                    if !self.line_times.is_empty() {
                        self.line_times.remove(0);
                    }
                    self.line_times.push(now.clone());
                } else {
                    let abs = (cur_line + total as i32 - 1) as usize;
                    if abs < self.line_times.len() {
                        self.line_times[abs] = now.clone();
                    } else if abs == self.line_times.len() {
                        self.line_times.push(now.clone());
                    }
                }
            }
        } else {
            self.line_times.truncate(total);
        }
        while self.line_times.len() < total {
            self.line_times.push(now.clone());
        }
        while self.line_times.len() > total {
            self.line_times.pop();
        }
        self.prev_total_lines = total;
        self.prev_cursor_line = cur_line;
    }
}
