//! [`StatusText`] — a status-bar label refreshed on a timer.
//!
//! Every status-bar indicator has the same shape: a `Task` that ticks on an
//! interval, samples a `String` (or `None` to hide the indicator), and
//! re-renders only when the text changed (PERF-29). The indicators differ only
//! in element id, interval, and sampler, so they share this one widget.
//!
//! The timer spawns on the window context (`cx.spawn_in`) so it fires reliably
//! regardless of focus — spawning on `AsyncApp` without holding the window can
//! be dropped, leaving the timer unfired until a click refreshes the view.

use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement, Render, StatefulInteractiveElement as _, Styled, Task, Window, div,
};
use gpui_component::{ActiveTheme as _, tooltip::Tooltip};

/// Produces the text to show, or `None` to hide the indicator.
pub type Sampler = Box<dyn FnMut(&App) -> Option<String> + 'static>;

/// A status-bar text indicator driven by a periodic sampler.
pub struct StatusText {
    id: &'static str,
    label: Option<String>,
    /// Show a click-to-copy affordance on the label.
    copyable: bool,
    sample: Sampler,
    _timer: Task<()>,
}

impl StatusText {
    /// Create an indicator that calls `sample` every `interval`.
    pub fn new_entity(
        id: &'static str,
        interval: Duration,
        copyable: bool,
        mut sample: Sampler,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let timer = cx.spawn_in(window, async move |this, window| {
                loop {
                    window.background_executor().timer(interval).await;
                    // The window or the indicator is gone: stop ticking.
                    if this
                        .update_in(window, |this: &mut Self, _, cx| this.tick(cx))
                        .is_err()
                    {
                        break;
                    }
                }
            });
            Self {
                id,
                label: sample(cx),
                copyable,
                sample,
                _timer: timer,
            }
        })
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let label = (self.sample)(cx);
        if label != self.label {
            self.label = label;
            cx.notify();
        }
    }
}

impl Render for StatusText {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let copyable = self.copyable;
        div()
            .id(self.id)
            .text_color(cx.theme().muted_foreground)
            .when_some(self.label.clone(), |this, label| {
                this.child(label.clone()).when(copyable, |this| {
                    this.cursor_pointer()
                        .tooltip(move |window, cx| Tooltip::new("Click to copy").build(window, cx))
                        .on_click(move |_: &ClickEvent, _window, cx: &mut App| {
                            cx.write_to_clipboard(ClipboardItem::new_string(label.clone()));
                        })
                })
            })
    }
}
