//! [`DateTimeClock`] — đồng hồ datetime hiển thị ở góc trái status bar.
//!
//! `Entity` + `Render` + `Focusable`, cập nhật mỗi 1s qua `cx.spawn` timer.

use std::time::Duration;

use chrono::Local;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement, Render, Styled, Task, div,
};
use gpui_component::ActiveTheme;

/// Đồng hồ hiển thị thời gian local, refresh mỗi 1 giây.
pub struct DateTimeClock {
    focus_handle: FocusHandle,
    now: chrono::DateTime<Local>,
    _timer: Task<()>,
}

impl DateTimeClock {
    /// Tạo đồng hồ mới, bắt đầu timer 1s.
    pub fn new(_window: &mut gpui::Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let timer = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if let Some(this) = this.upgrade() {
                    this.update(cx, |this, cx| {
                        this.now = Local::now();
                        cx.notify();
                    });
                }
            }
        });
        Self {
            focus_handle,
            now: Local::now(),
            _timer: timer,
        }
    }

    /// Helper tạo `Entity<Self>`.
    pub fn new_entity(window: &mut gpui::Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn formatted(&self) -> String {
        self.now.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

impl Focusable for DateTimeClock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DateTimeClock {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("datetime-clock")
            .track_focus(&self.focus_handle)
            .child(self.formatted())
            .text_color(cx.theme().muted_foreground)
    }
}
