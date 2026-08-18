//! [`DateTimeClock`] — a datetime clock shown on the left side of the status bar.
//!
//! `Entity` + `Render` + `Focusable`, updated every 1s via a timer.
//!
//! Uses `cx.spawn_in(window, ...)` + `window.background_executor().timer(...)`
//! so the timer fires reliably (independent of focus/click). Spawning on `AsyncApp`
//! without holding the window can be dropped, leaving the timer unfired until the
//! view is refreshed by a click.

use std::time::Duration;

use chrono::Local;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement, Render, Styled, Task, Window, div,
};
use gpui_component::ActiveTheme as _;

/// Clock that shows the local time, refreshed every 1 second.
pub struct DateTimeClock {
    focus_handle: FocusHandle,
    now: chrono::DateTime<Local>,
    _timer: Task<()>,
}

impl DateTimeClock {
    /// Create a new clock and start the 1s timer.
    ///
    /// NOTE: spawn on the window context (via `cx.spawn_in`) so the timer fires
    /// steadily even when the view is not focused.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let timer = cx.spawn_in(window, async move |this, window| {
            loop {
                window
                    .background_executor()
                    .timer(Duration::from_secs(1))
                    .await;
                // The window or the clock is gone: stop ticking.
                let ticked = this.update_in(window, |this, _, cx| {
                    this.now = Local::now();
                    cx.notify();
                });
                if ticked.is_err() {
                    break;
                }
            }
        });
        Self {
            focus_handle,
            now: Local::now(),
            _timer: timer,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("datetime-clock")
            .track_focus(&self.focus_handle)
            .child(self.formatted())
            .text_color(cx.theme().muted_foreground)
    }
}
