//! `TerminalScrollHandle` — `ScrollbarHandle` impl for terminal scrollback.
//!
//! The cached state (total_lines/viewport_lines/display_offset/line_height) is
//! updated each frame from a snapshot. When the user drags the scrollbar thumb,
//! `set_offset` computes the new `display_offset` and stores it in
//! `future_display_offset` — the View applies it on the next `render()`
//! (calling `session.scroll(delta)`).
//!
//! Reference: Zed `terminal_scrollbar.rs::TerminalScrollHandle`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{Pixels, Point, Size, px};

use gpui_component::scroll::ScrollbarHandle;

/// Cached scrollbar state — updated each frame from a snapshot + GridMetrics.
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

/// Handle for `Scrollbar::vertical` — clone-friendly (Rc fields).
#[derive(Clone)]
pub struct TerminalScrollHandle {
    state: Rc<RefCell<TerminalScrollState>>,
    /// display_offset requested by the user via scrollbar drag — applied by the View.
    pub future_display_offset: Rc<Cell<Option<usize>>>,
}

impl TerminalScrollHandle {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(TerminalScrollState::default())),
            future_display_offset: Rc::new(Cell::new(None)),
        }
    }

    /// Update the cache from a snapshot + GridMetrics — called in `render()`.
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

    /// Take the pending future_display_offset (if any) — consumed by the View.
    pub fn take_future_display_offset(&self) -> Option<usize> {
        self.future_display_offset.take()
    }

    /// Returns (total_lines, viewport_lines, display_offset, line_height).
    pub fn state_info(&self) -> (usize, usize, usize, f32) {
        let s = self.state.borrow();
        (
            s.total_lines,
            s.viewport_lines,
            s.display_offset,
            s.line_height,
        )
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
