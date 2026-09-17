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

/// How an indicator shortens a label the window is too narrow for.
///
/// The status bar does not wrap or scroll: whatever does not fit is clipped at
/// the window edge, which is how a path lost the directory the user was in and
/// how `MEM 577.0 MB` lost its unit (`US-0112`). Bounding the one unbounded
/// indicator keeps the rest on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shorten {
    /// Show the sampled text as it is. Every fixed-width indicator (clock,
    /// speeds, CPU/memory) fits by construction.
    Never,
    /// A filesystem path: drop leading components, keep the tail.
    PathTail,
}

/// Width the status bar's other contents need: the clock and the git status on
/// the left, the speed and CPU/memory indicators on the right, the separators
/// and the dock button. Everything left over is the path's.
const STATUS_BAR_RESERVED: gpui::Pixels = gpui::px(440.);

/// Mean advance of one status-bar character, as a share of the root font size.
/// The bar renders at `text_xs` (0.75 rem) in a proportional UI font whose mean
/// advance is about half its size — ~6 px per character at the default 16 px
/// root font, which is what the before frames measure.
const CHAR_ADVANCE_PER_REM: f32 = 0.375;

/// How many characters of path the window has room for.
fn path_budget(window: &Window) -> usize {
    let available = (window.viewport_size().width - STATUS_BAR_RESERVED).max(gpui::px(120.));
    let advance = (window.rem_size() * CHAR_ADVANCE_PER_REM).max(gpui::px(1.));
    (available / advance) as usize
}

/// Shorten `path` to `max_chars` by dropping leading components and keeping the
/// tail — the directory the user is actually in — behind a leading ellipsis.
///
/// The cut lands on a separator whenever one fits, so a component is never
/// halved. Only a trailing component longer than the whole budget is cut inside
/// it, and then from the left so its end still reads.
///
/// Characters, not pixels: the status bar font is proportional, so the budget is
/// an estimate that errs narrow rather than a measurement.
fn elide_path_left(path: &str, max_chars: usize) -> String {
    const ELLIPSIS: char = '…';

    if path.chars().count() <= max_chars {
        return path.to_string();
    }
    // The ellipsis takes a column of its own.
    let budget = max_chars.saturating_sub(1);
    // Left to right, so the first separator whose tail fits keeps the most
    // components.
    let at_separator = path
        .char_indices()
        .filter(|(_, c)| *c == '\\' || *c == '/')
        .map(|(ix, _)| &path[ix..])
        .find(|tail| tail.chars().count() <= budget);
    match at_separator {
        Some(tail) => format!("{ELLIPSIS}{tail}"),
        None => {
            let skipped = path.chars().count().saturating_sub(budget);
            let tail: String = path.chars().skip(skipped).collect();
            format!("{ELLIPSIS}{tail}")
        }
    }
}

/// How an indicator presents the label it samples.
pub struct Presentation {
    /// Leading icon, shown only while the label is.
    pub icon: Option<Icon>,
    /// Show a click-to-copy affordance on the label. The copied text is always
    /// the sampled value, never the shortened one.
    pub copyable: bool,
    /// How the label is shortened when the window is too narrow for it.
    pub shorten: Shorten,
}

/// A status-bar text indicator driven by a periodic sampler.
pub struct StatusText {
    id: &'static str,
    label: Option<Label>,
    presentation: Presentation,
    sample: Sampler,
    _timer: Task<()>,
}

impl StatusText {
    /// Create an indicator that calls `sample` every `interval`.
    pub fn new_entity(
        id: &'static str,
        interval: Duration,
        presentation: Presentation,
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
                presentation,
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

impl Shorten {
    /// Apply this policy to a sampled label, given the room the window has.
    fn apply(self, mut label: Label, max_chars: usize) -> Label {
        // A multi-segment label (the git diffstat) is built from parts that
        // must stay whole, and none of them is a path. Neither is a value with
        // its unit: `Shorten::Never` is what keeps `MEM 577.0 MB` intact.
        if self == Shorten::PathTail
            && let [segment] = label.0.as_mut_slice()
        {
            segment.text = elide_path_left(&segment.text, max_chars);
        }
        label
    }
}

impl Render for StatusText {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let copyable = self.presentation.copyable;
        let shorten = self.presentation.shorten;
        let icon = self.presentation.icon.clone();
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
                // Copy the value the indicator sampled, not the shortened one.
                let text = label.plain_text();
                let label = shorten.apply(label, path_budget(window));
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

#[cfg(test)]
mod tests {
    use super::{Label, Segment, Shorten, Tone, elide_path_left};

    #[test]
    fn a_path_that_fits_is_shown_whole() {
        assert_eq!(elide_path_left(r"C:\Users\me", 40), r"C:\Users\me");
        // Exactly the budget is still whole.
        assert_eq!(elide_path_left(r"C:\Users\me", 11), r"C:\Users\me");
        assert_eq!(elide_path_left("", 0), "");
    }

    #[test]
    fn a_long_path_keeps_its_tail_from_a_separator() {
        let path = r"C:\Users\trunglt\AppData\Local\Temp\claude\scratchpad\ux\home";
        // The budget keeps as many trailing components as fit, and the
        // ellipsis is one of the columns it has to pay for.
        assert_eq!(elide_path_left(path, 12), r"…\ux\home");
        assert_eq!(elide_path_left(path, 20), r"…\scratchpad\ux\home");
        assert_eq!(elide_path_left(path, 30), r"…\claude\scratchpad\ux\home");
        // Forward slashes cut the same way.
        assert_eq!(elide_path_left("/var/log/nginx/access", 12), "…/access");
    }

    #[test]
    fn the_cut_never_lands_inside_a_component() {
        let path = r"C:\Users\trunglt\scratchpad\ux\home";
        for budget in 1..=path.chars().count() {
            let elided = elide_path_left(path, budget);
            assert!(
                path.ends_with(elided.trim_start_matches('…')),
                "{elided:?} is not a tail of {path:?}"
            );
        }
    }

    #[test]
    fn a_single_component_longer_than_the_budget_keeps_its_end() {
        // No separator boundary is available, so the component itself is cut.
        assert_eq!(elide_path_left("averylongdirectoryname", 8), "…oryname");
        assert_eq!(elide_path_left("averylongdirectoryname", 1), "…");
    }

    #[test]
    fn multi_byte_components_are_never_cut_mid_character() {
        let path = r"C:\Users\trunglt\文档\プロジェクト\本番";
        assert_eq!(elide_path_left(path, 12), r"…\プロジェクト\本番");
        // Cutting inside a wide component still yields valid characters.
        assert_eq!(elide_path_left("プロジェクト管理", 4), "…ト管理");
    }

    #[test]
    fn only_a_path_label_is_shortened_and_never_a_value_with_its_unit() {
        let memory = Label::from("CPU 0.2%  MEM 577.0 MB".to_string());
        assert_eq!(
            Shorten::Never.apply(memory.clone(), 8).plain_text(),
            "CPU 0.2%  MEM 577.0 MB",
            "a value and its unit are one token; the indicator never cuts it"
        );

        // A diffstat-style multi-segment label is left alone by construction.
        let git = Label(vec![
            Segment::new("main", Tone::Foreground),
            Segment::new(" +12 -3", Tone::Success),
        ]);
        assert_eq!(Shorten::PathTail.apply(git.clone(), 4), git);

        let path = Label::from(r"C:\Users\me\scratchpad\ux\home".to_string());
        assert_eq!(
            Shorten::PathTail.apply(path, 20).plain_text(),
            r"…\scratchpad\ux\home"
        );
    }
}
