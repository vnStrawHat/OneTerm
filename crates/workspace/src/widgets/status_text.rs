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

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Render,
    StatefulInteractiveElement as _, Styled, Task, Window, div, px,
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

/// The width the status bar has left for one shortening indicator, refreshed
/// every frame by [`crate::layout::statusbar::build_status_bar`].
///
/// A shared cell rather than entity state: the bar is the only place that knows
/// every label, and it computes the split while building the same frame the
/// indicator renders in — so this must not mark anything dirty.
pub type Budget = Rc<Cell<Pixels>>;

/// How an indicator shortens a label the window is too narrow for.
///
/// The status bar does not wrap or scroll. The pinned ends never shrink (see
/// the bar's centre region), so the two indicators that can grow without bound
/// are the ones that give way: the cwd and the git branch (`US-0112`).
#[derive(Clone, Debug, PartialEq)]
pub enum Shorten {
    /// Show the sampled text as it is. Every bounded indicator (clock, speeds,
    /// CPU/memory) fits by construction, and a value and its unit are one token.
    Never,
    /// A filesystem path: drop leading components, keep the tail.
    PathTail(Budget),
    /// A label the head of which identifies it (a git branch): keep the head.
    HeadFirst(Budget),
}

impl Shorten {
    fn budget(&self) -> Option<Pixels> {
        match self {
            Shorten::Never => None,
            Shorten::PathTail(budget) | Shorten::HeadFirst(budget) => Some(budget.get()),
        }
    }
}

/// The status bar renders at `text_xs`, three quarters of the root font size.
fn status_font_size(window: &Window) -> Pixels {
    window.rem_size() * 0.75
}

/// The width `text` takes in the status bar's own font.
///
/// Measured through the window's text system, not estimated from a character
/// count: the bar's font is proportional, and an estimate is what let the memory
/// unit and the git branch fall off the bar (`US-0112`).
pub(crate) fn measure_status_text(window: &Window, text: &str) -> Pixels {
    if text.is_empty() {
        return px(0.);
    }
    let font_size = status_font_size(window);
    let mut style = window.text_style();
    style.font_size = font_size.into();
    let run = style.to_run(text.len());
    window
        .text_system()
        .layout_line(text, font_size, &[run], None)
        .width
}

/// The longest elision of `text` that fits `max_width`.
///
/// `elide` is the shortening rule — which end to keep, where it may cut — and
/// this only chooses how much of it fits, by measuring. `elide` must never grow
/// as its budget shrinks, and must fit anything at a budget of zero.
fn fit_to_width(
    text: &str,
    max_width: Pixels,
    window: &Window,
    elide: fn(&str, usize) -> String,
) -> String {
    if measure_status_text(window, text) <= max_width {
        return text.to_string();
    }
    // Invariant: `fits` fits (zero characters is the empty string), `over` does
    // not. Bisect until they are adjacent.
    let (mut fits, mut over) = (0usize, text.chars().count());
    while fits + 1 < over {
        let mid = fits + (over - fits) / 2;
        if measure_status_text(window, &elide(text, mid)) <= max_width {
            fits = mid;
        } else {
            over = mid;
        }
    }
    elide(text, fits)
}

const ELLIPSIS: char = '…';

/// Shorten `path` to `max_chars` by dropping leading components and keeping the
/// tail — the directory the user is actually in — behind a leading ellipsis.
///
/// The cut lands on a separator whenever one fits, so a component is never
/// halved. Only a trailing component longer than the whole budget is cut inside
/// it, and then from the left so its end still reads. The result never exceeds
/// `max_chars`, the ellipsis included.
fn elide_path_left(path: &str, max_chars: usize) -> String {
    if path.chars().count() <= max_chars {
        return path.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    // The ellipsis takes a column of its own.
    let budget = max_chars - 1;
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
            let skipped = path.chars().count() - budget;
            let tail: String = path.chars().skip(skipped).collect();
            format!("{ELLIPSIS}{tail}")
        }
    }
}

/// Shorten `text` to `max_chars` by keeping its head behind a trailing ellipsis.
///
/// For a name whose beginning identifies it — a git branch, where
/// `worktree-agent-a18…` still says which branch it is. The result never exceeds
/// `max_chars`, the ellipsis included.
fn elide_head(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let head: String = text.chars().take(max_chars - 1).collect();
    format!("{head}{ELLIPSIS}")
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

    /// The text this indicator sampled, before any shortening. The status bar
    /// measures it to share out the width it has.
    pub(crate) fn text(&self) -> Option<String> {
        self.label.as_ref().map(Label::plain_text)
    }

    /// Tell this indicator how much width the bar has left for it. No-op for an
    /// indicator that never shortens.
    pub(crate) fn set_budget(&self, width: Pixels) {
        match &self.presentation.shorten {
            Shorten::Never => {}
            Shorten::PathTail(budget) | Shorten::HeadFirst(budget) => budget.set(width),
        }
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
    /// Apply this policy to a sampled label, within the width the bar left for
    /// it. `Shorten::Never` is what keeps `MEM 577.0 MB` whole.
    fn apply(&self, mut label: Label, window: &Window) -> Label {
        let Some(budget) = self.budget() else {
            return label;
        };
        match self {
            Shorten::Never => {}
            Shorten::PathTail(_) => {
                if let [segment] = label.0.as_mut_slice() {
                    segment.text = fit_to_width(&segment.text, budget, window, elide_path_left);
                }
            }
            Shorten::HeadFirst(_) => {
                // Only the first segment is the name; the rest (a diffstat,
                // ahead/behind counts) are short and already bounded, so what
                // they take comes off the name's budget.
                let Some((name, rest)) = label.0.split_first_mut() else {
                    return label;
                };
                let rest: String = rest.iter().map(|segment| segment.text.as_str()).collect();
                let budget = (budget - measure_status_text(window, &rest)).max(px(0.));
                name.text = fit_to_width(&name.text, budget, window, elide_head);
            }
        }
        label
    }
}

impl Render for StatusText {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let copyable = self.presentation.copyable;
        let shorten = self.presentation.shorten.clone();
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
                let label = shorten.apply(label, window);
                // A shortened label hides part of its value, so hovering has to
                // show the whole of it; an unshortened one only advertises the
                // copy (`US-0112`).
                let tooltip = match label.plain_text() {
                    shown if shown != text => Some(text.clone()),
                    _ if copyable => Some("Click to copy".to_string()),
                    _ => None,
                };
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
                    .when_some(tooltip, |this, tooltip| {
                        this.tooltip(move |window, cx| {
                            Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                    })
                    .when(copyable, |this| {
                        this.cursor_pointer().on_click(
                            move |_: &ClickEvent, _window, cx: &mut App| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            },
                        )
                    })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Budget, Cell, Context, IntoElement, Label, Pixels, Rc, Render, Segment, Shorten, Tone,
        Window, div, elide_head, elide_path_left, fit_to_width, measure_status_text, px,
    };

    #[test]
    fn a_path_that_fits_is_shown_whole() {
        assert_eq!(elide_path_left(r"C:\Users\me", 40), r"C:\Users\me");
        // Exactly the budget is still whole.
        assert_eq!(elide_path_left(r"C:\Users\me", 11), r"C:\Users\me");
        assert_eq!(elide_path_left("", 0), "");
    }

    #[test]
    fn nothing_ever_comes_back_wider_than_its_budget() {
        // The width search bisects down to a zero budget, so an elision that
        // came back one column over would be picked as "fitting".
        let path = r"C:\Users\trunglt\scratchpad\ux\home";
        for budget in 0..=path.chars().count() + 2 {
            assert!(
                elide_path_left(path, budget).chars().count() <= budget,
                "path budget {budget} overflowed"
            );
            assert!(
                elide_head("worktree-agent-a1857284c8b933f27", budget)
                    .chars()
                    .count()
                    <= budget,
                "head budget {budget} overflowed"
            );
        }
        assert_eq!(elide_path_left(path, 0), "");
        assert_eq!(elide_head("main", 0), "");
    }

    #[test]
    fn a_long_name_keeps_the_head_that_identifies_it() {
        let branch = "worktree-agent-a1857284c8b933f27";
        assert_eq!(elide_head(branch, 40), branch);
        assert_eq!(elide_head(branch, 32), branch);
        assert_eq!(elide_head(branch, 18), "worktree-agent-a1…");
        assert_eq!(elide_head(branch, 2), "w…");
        assert_eq!(elide_head("プロジェクト管理", 4), "プロジ…");
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
    fn the_cut_lands_on_a_separator_while_one_fits() {
        let path = r"C:\Users\trunglt\scratchpad\ux\home";
        // The last component is 4 characters, so from budget 6 up there is
        // always a separator boundary that fits, and the cut must be on it.
        for budget in 6..=path.chars().count() {
            let elided = elide_path_left(path, budget);
            let kept = elided.trim_start_matches('…');
            assert!(path.ends_with(kept), "{elided:?} is not a tail of {path:?}");
            if elided != path {
                assert!(
                    kept.starts_with('\\'),
                    "budget {budget} cut inside a component: {elided:?}"
                );
            }
        }
        // Below that only the last component's own end can be kept, and then
        // the cut is inside it — with the ellipsis that says so.
        assert_eq!(elide_path_left(path, 4), "…ome");
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

    /// A view to hang a test window on, so the text system can measure.
    struct Probe;

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn budget_of(width: Pixels) -> Budget {
        Rc::new(Cell::new(width))
    }

    #[gpui::test]
    fn the_width_search_picks_the_longest_elision_that_fits(cx: &mut gpui::TestAppContext) {
        let (_probe, cx) = cx.add_window_view(|_, _| Probe);
        cx.update(|window, _| {
            let path = r"C:\Users\me\scratchpad\ux\home";
            let full = measure_status_text(window, path);
            assert!(full > px(0.), "the test window must have font metrics");

            // Room for all of it: nothing is elided.
            assert_eq!(fit_to_width(path, full, window, elide_path_left), path);

            // Half the room: elided at a separator, and it really does fit.
            let half = fit_to_width(path, full / 2., window, elide_path_left);
            assert!(half.starts_with('…'), "{half:?}");
            assert!(path.ends_with(half.trim_start_matches('…')), "{half:?}");
            assert!(measure_status_text(window, &half) <= full / 2.);

            // No room at all: nothing, rather than something over budget.
            assert_eq!(fit_to_width(path, px(0.), window, elide_path_left), "");
        });
    }

    #[gpui::test]
    fn only_an_unbounded_label_shortens_and_never_a_value_with_its_unit(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_probe, cx) = cx.add_window_view(|_, _| Probe);
        cx.update(|window, _| {
            // A value and its unit are one token, and its policy is `Never`:
            // no budget can cut `MB` off it.
            let memory = Label::from("CPU 0.2%  MEM 577.0 MB".to_string());
            assert_eq!(Shorten::Never.apply(memory.clone(), window), memory);

            // The git label's head is the branch; the diffstat segments stay.
            let git = Label(vec![
                Segment::new("worktree-agent-a1857284c8b933f27", Tone::Foreground),
                Segment::new(" (+12 -3)", Tone::Success),
            ]);
            // Room for the diffstat and about half the branch.
            let room = measure_status_text(window, " (+12 -3)")
                + measure_status_text(window, "worktree-agent");
            let narrow = Shorten::HeadFirst(budget_of(room)).apply(git.clone(), window);
            assert!(
                narrow.0[0].text.ends_with('…'),
                "the branch keeps its head: {:?}",
                narrow.0[0].text
            );
            assert!(
                git.0[0]
                    .text
                    .starts_with(narrow.0[0].text.trim_end_matches('…'))
            );
            assert_eq!(narrow.0[1], git.0[1], "the diffstat is not touched");

            // A path policy on a multi-segment label leaves it alone: only a
            // single-run label is a path.
            assert_eq!(
                Shorten::PathTail(budget_of(px(1.))).apply(git.clone(), window),
                git
            );
        });
    }
}
