//! `LocalTerminalView` — GPUI view render 1 terminal session (local/ssh).
//!
//! Module gốc `view.rs` đã được tách thành `view/`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{ClipboardItem, Context, Entity, FocusHandle, KeyBinding, NoAction, Window};

use async_channel::Receiver;
use myterm2_core::{SessionEvent, TerminalInfo, TerminalSession};

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
    /// Per-line timestamps (gutter). `line_times[j]` = giờ render của dòng có
    /// **chỉ số absolute** (0-based) = `line_time_base + j`. Grow-only: mỗi dòng
    /// được stamp đúng một lần và không bao giờ ghi đè (xem `update_line_times`).
    pub(crate) line_times: Vec<String>,
    /// Chỉ số absolute (0-based) của `line_times[0]` — dòng cũ nhất còn track.
    /// Tăng dần khi dòng cũ rời scrollback.
    pub(crate) line_time_base: usize,
    /// Per-row layout cache — skip recompute cho non-dirty rows.
    pub(crate) row_cache: Rc<RefCell<RowLayoutCache>>,
    /// Cached gutter width + num_digits — chỉ recompute khi num_digits đổi.
    /// Tránh gọi shape_line mỗi frame → ngăn dao động gutter_width gây resize loop.
    pub(crate) cached_gutter: Rc<RefCell<Option<(gpui::Pixels, usize)>>>,
    /// Last terminal size (rows, cols) — persist giữa các frame để tránh
    /// gọi s.resize() mỗi frame (TerminalElement tạo mới mỗi frame).
    pub(crate) last_grid_size: Rc<RefCell<Option<(u16, u16)>>>,
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
                            // Stamp tại thời điểm OUTPUT (không chỉ render): task
                            // subscribe chạy độc lập với render, nên tab inactive
                            // (không render) vẫn cập nhật timestamp đúng giờ dòng
                            // được tạo, thay vì dồn về giờ lúc active lại tab.
                            let info = s.read(cx).terminal_info();
                            view.update_line_times(&info);
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
            line_time_base: 0,
            row_cache: Rc::new(RefCell::new(RowLayoutCache::new())),
            cached_gutter: Rc::new(RefCell::new(None)),
            last_grid_size: Rc::new(RefCell::new(None)),
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

    /// Cập nhật `line_times` tại **thời điểm render**, theo model **grow-only**
    /// keyed bằng chỉ số absolute của dòng.
    ///
    /// Mỗi dòng được gán timestamp đúng **một lần** — tại frame đầu tiên nó xuất
    /// hiện — và **không bao giờ bị ghi đè**. Đây là điểm mấu chốt để chống lại
    /// ConPTY repaint / reflow: những thao tác này làm `total_lines` (và do đó
    /// `absolute_line_count` qua `terminal_info`) dao động giảm tạm thời. Code
    /// cũ phản ứng bằng cách clear + refill `now` → mọi dòng nhảy về cùng một
    /// giờ. Ở đây giảm tạm thời chỉ đơn giản là "không thêm gì", timestamp đã có
    /// được giữ nguyên.
    ///
    /// `line_times[j]` ↔ dòng có absolute index `line_time_base + j`.
    pub(crate) fn update_line_times(&mut self, info: &TerminalInfo) {
        let total = info.total_lines;
        let absolute = info.absolute_line_count;
        let now = chrono::Local::now().format("%H:%M:%S").to_string();

        // Số dòng ĐÃ CÓ NỘI DUNG (high-water mark) = absolute index của cursor + 1.
        //
        // `absolute_line_count` bị "thổi phồng" tới đáy viewport vì
        // `total_lines = history + screen_lines` luôn tính cả các dòng TRỐNG bên
        // dưới cursor (lưới luôn cao `num_lines`). Nếu stamp tới `absolute`, các
        // dòng trống đó bị gán giờ hiện tại; khi output sau này ghi đè vào chúng,
        // chúng giữ giờ cũ → đúng triệu chứng "một khối dòng mang giờ sai".
        //
        // Cursor là nơi output đang được ghi, nên dừng stamp ở ngay sau cursor.
        // Absolute index của cursor = absolute − num_lines + cursor_line.
        let cursor_row = info.cursor_line.max(0) as usize;
        let content_high = absolute
            .saturating_sub(info.num_lines)
            .saturating_add(cursor_row + 1)
            .min(absolute);

        // Reset cứng: chỉ khi nội dung mới bắt đầu TRƯỚC dòng cũ nhất đang track
        // (counter absolute bị reset hẳn). ConPTY repaint/reflow chỉ làm dao
        // động trong phạm vi nội dung hiện có nên KHÔNG kích hoạt nhánh này.
        if absolute < self.line_time_base {
            self.line_times.clear();
            self.line_time_base = absolute.saturating_sub(total);
        }
        if self.line_times.is_empty() {
            self.line_time_base = absolute.saturating_sub(total);
        }

        // Stamp các dòng mới CÓ NỘI DUNG (index ≥ covered) bằng giờ render hiện
        // tại. Grow-only: dao động giảm tạm thời → không push gì; dòng trống dưới
        // cursor chưa được stamp tới khi cursor (nội dung) thực sự chạm tới.
        let covered = self.line_time_base + self.line_times.len();
        if content_high > covered {
            let new_lines = content_high - covered;
            self.line_times.reserve(new_lines);
            for _ in 0..new_lines {
                self.line_times.push(now.clone());
            }
        }

        // Drop timestamp của dòng đã rời scrollback (front) để bound memory.
        let oldest = absolute.saturating_sub(total);
        if oldest > self.line_time_base {
            let drop = (oldest - self.line_time_base).min(self.line_times.len());
            self.line_times.drain(0..drop);
            self.line_time_base += drop;
        }
    }
}
