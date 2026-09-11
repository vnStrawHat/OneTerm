//! [`StatusText`] — a status-bar label refreshed on a timer.
//!
//! Every status-bar indicator has the same shape: a `Task` that ticks on an
//! interval, samples a [`Label`] (or `None` to hide the indicator), and
//! re-renders only when it changed (PERF-29). The indicators differ only in
//! element id, interval, icon, and sampler, so they share this one widget.
//!
//! A label is a list of [`Segment`]s, each with a [`Tone`] mapped to a theme
//! colour, so an indicator can mix foreground text with highlighted parts (the
//! git diffstat). A plain `String` converts into a single foreground segment.
//!
//! The timer spawns on the window context (`cx.spawn_in`) so it fires reliably
//! regardless of focus — spawning on `AsyncApp` without holding the window can
//! be dropped, leaving the timer unfired until a click refreshes the view.

use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Render, StatefulInteractiveElement as _,
    Styled, Task, Window, div,
};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, tooltip::Tooltip};

/// Theme colour role of a [`Segment`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Muted,
    Foreground,
    Success,
    Danger,
}

impl Tone {
    fn color(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            Tone::Muted => theme.muted_foreground,
            Tone::Foreground => theme.foreground,
            Tone::Success => theme.success,
            Tone::Danger => theme.danger,
        }
    }
}

/// One coloured run of text in a [`Label`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub tone: Tone,
}

impl Segment {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

/// The text of an indicator: one or more coloured segments. The icon takes the
/// tone of the first segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label(pub Vec<Segment>);

impl Label {
    fn plain_text(&self) -> String {
        self.0.iter().map(|s| s.text.as_str()).collect()
    }
}

impl From<String> for Label {
    fn from(text: String) -> Self {
        Self(vec![Segment::new(text, Tone::Foreground)])
    }
}

/// Produces the label to show, or `None` to hide the indicator.
pub type Sampler = Box<dyn FnMut(&App) -> Option<Label> + 'static>;

/// A status-bar text indicator driven by a periodic sampler.
pub struct StatusText {
    id: &'static str,
    label: Option<Label>,
    /// Leading icon, shown only while the label is.
    icon: Option<Icon>,
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
        icon: Option<Icon>,
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
                icon,
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
        let icon = self.icon.clone();
        let base_tone = self
            .label
            .as_ref()
            .and_then(|label| label.0.first())
            .map_or(Tone::Foreground, |segment| segment.tone);
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap_1()
            .text_color(base_tone.color(cx))
            .when_some(self.label.clone(), |this, label| {
                let text = label.plain_text();
                this.children(icon.map(|icon| icon.xsmall()))
                    // Segments sit in their own flex row so the outer `gap_1`
                    // does not open space between them; spacing is in the text.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .children(label.0.into_iter().map(|segment| {
                                div().text_color(segment.tone.color(cx)).child(segment.text)
                            })),
                    )
                    .when(copyable, |this| {
                        this.cursor_pointer()
                            .tooltip(move |window, cx| {
                                Tooltip::new("Click to copy").build(window, cx)
                            })
                            .on_click(move |_: &ClickEvent, _window, cx: &mut App| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            })
                    })
            })
    }
}
