//! `TerminalScrollHandle` — impl `ScrollbarHandle` cho terminal scrollback.
//!
//! Cache state (total_lines/viewport_lines/display_offset/line_height) được
//! update mỗi frame từ snapshot. Khi user kéo scrollbar thumb, `set_offset`
//! tính `display_offset` mới và lưu vào `future_display_offset` — View áp dụng
//! ở `render()` đầu tiếp theo (gọi `session.scroll(delta)`).
//!
//! Tham chiếu: Zed `terminal_scrollbar.rs::TerminalScrollHandle`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{Pixels, Point, Size, px};

use gpui_component::scroll::ScrollbarHandle;

/// State cache cho scrollbar — update mỗi frame từ snapshot + GridMetrics.
#[derive(Debug, Clone, Copy)]
struct TerminalScrollState {
    total_lines: usize,
    viewport_lines: usize,
    display_offset: usize,
    line_height: f32,
}

impl Default for TerminalScrollState {
    fn default() -> Self {
        Self {
            total_lines: 24,
            viewport_lines: 24,
            display_offset: 0,
            line_height: 16.0,
        }
    }
}

/// Handle cho `Scrollbar::vertical` — clone-thân thiện (Rc fields).
#[derive(Clone)]
pub struct TerminalScrollHandle {
    state: Rc<RefCell<TerminalScrollState>>,
    /// display_offset mà user yêu cầu qua scrollbar drag — View áp dụng.
    pub future_display_offset: Rc<Cell<Option<usize>>>,
}

impl TerminalScrollHandle {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(TerminalScrollState::default())),
            future_display_offset: Rc::new(Cell::new(None)),
        }
    }

    /// Update cache từ snapshot + GridMetrics — gọi ở `render()`.
    pub fn update(
        &self,
        total_lines: usize,
        viewport_lines: usize,
        display_offset: usize,
        line_height: f32,
    ) {
        *self.state.borrow_mut() = TerminalScrollState {
            total_lines,
            viewport_lines,
            display_offset,
            line_height,
        };
    }

    /// Lấy future_display_offset pending (nếu có) — View consume.
    pub fn take_future_display_offset(&self) -> Option<usize> {
        self.future_display_offset.take()
    }
}

impl Default for TerminalScrollHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollbarHandle for TerminalScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        let s = self.state.borrow();
        let max_offset = s.total_lines.saturating_sub(s.viewport_lines);
        let scroll_offset = max_offset.saturating_sub(s.display_offset);
        Point::new(Pixels::ZERO, px(-(scroll_offset as f32 * s.line_height)))
    }

    fn set_offset(&self, point: Point<Pixels>) {
        let s = self.state.borrow();
        if s.line_height <= 0.0 {
            return;
        }
        let offset_delta = (f32::from(point.y) / s.line_height).round() as i32;
        let max_offset = s.total_lines.saturating_sub(s.viewport_lines) as i32;
        let display_offset = (max_offset + offset_delta).clamp(0, max_offset) as usize;
        self.future_display_offset.set(Some(display_offset));
    }

    fn content_size(&self) -> Size<Pixels> {
        let s = self.state.borrow();
        Size::new(Pixels::ZERO, px(s.total_lines as f32 * s.line_height))
    }
}
