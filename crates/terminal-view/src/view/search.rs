//! Terminal in-buffer search — Ctrl+F search bar + match highlighting.
//!
//! The search state lives in [`SearchState`], owned by [`LocalTerminalView`]
//! (query, options, matches in grid coordinates, active index). The search bar
//! is a small overlay (top-right of the terminal panel) built with
//! `gpui_component::input`.
//!
//! Matching is delegated to the backend via
//! [`TerminalSession::search`](oneterm_terminal::TerminalSession::search), which
//! returns matches in grid coordinates. The view converts a match to a display
//! row with `display_row = line + display_offset` (see
//! `docs/terminal-backend.md`) to highlight it in the viewport and to scroll
//! it into view.
//!
//! Key bindings:
//! - `Ctrl+F` — toggle the search bar (open / close).
//! - `Enter` (in the search input) — next match.
//! - `Shift+Enter` — previous match.
//! - `Esc` — close the search bar.
//!
//! Navigation wraps around (last → first, first → last).

use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, ParentElement as _, SharedString, Styled, Subscription, Task, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _, Toggle, ToggleVariants as _},
};
use oneterm_terminal::{SearchMatch, SearchOptions};

use super::LocalTerminalView;

/// Debounce delay between the last keystroke in the search input and the
/// full-grid scan (typing must not fire a search per character).
const SEARCH_DEBOUNCE_MS: u64 = 150;

/// A search highlight to paint, already in **display coordinates** (0-based from
/// the top of the viewport) and filtered to the visible range. Passed from the
/// view to the element each frame.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SearchHighlight {
    /// Display row (0-based from the top of the viewport).
    pub display_line: i32,
    /// Start column (inclusive, 0-based).
    pub start_col: i32,
    /// End column (exclusive).
    pub end_col: i32,
    /// Whether this is the active (current) match.
    pub active: bool,
}

/// In-buffer search state owned by the view.
#[derive(Default)]
pub(crate) struct SearchState {
    /// Whether the search bar is open.
    pub(crate) active: bool,
    /// The search query (kept in sync with the `InputState`).
    pub(crate) query: String,
    /// Search options (case-sensitivity, whole-word).
    pub(crate) options: SearchOptions,
    /// Matches in grid coordinates (top-to-bottom order).
    pub(crate) matches: Vec<SearchMatch>,
    /// Index into `matches` of the active (current) match.
    pub(crate) active_idx: Option<usize>,
    /// The `InputState` for the search bar input.
    pub(crate) input: Option<Entity<InputState>>,
    /// Debounce task for the search — delays the full-grid scan after the
    /// last keystroke.
    pub(crate) debounce_task: Option<Task<()>>,
    /// Subscription to the search input's events (kept for the life of the
    /// bar; dropping it in `clear` unsubscribes — CORR-65).
    pub(crate) input_subscription: Option<Subscription>,
    /// Terminal output arrived since the matches were computed: their grid
    /// coordinates are stale and must be refreshed on the next frame
    /// (PERF-04: one refresh per frame instead of one per PTY read).
    dirty: bool,
}

impl SearchState {
    /// Whether the bar is open and a query is set (i.e. matches are meaningful).
    fn has_query(&self) -> bool {
        self.active && !self.query.is_empty()
    }

    /// Store a fresh match list and start from the first match.
    fn set_matches(&mut self, matches: Vec<SearchMatch>) {
        self.matches = matches;
        self.active_idx = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Replace the match list with refreshed grid coordinates while keeping
    /// the active index (clamped to the new length).
    fn refresh_matches(&mut self, matches: Vec<SearchMatch>) {
        self.matches = matches;
        self.active_idx = self.active_idx.filter(|&i| i < self.matches.len());
    }

    /// Advance the active index: `backward = true` (Shift+Enter) → previous;
    /// otherwise next. Wraps around. Returns `false` when there are no matches.
    fn step(&mut self, backward: bool) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        let len = self.matches.len();
        let idx = match self.active_idx {
            Some(i) if backward => (i + len - 1) % len,
            Some(i) => (i + 1) % len,
            None => 0,
        };
        self.active_idx = Some(idx);
        true
    }

    /// The active match, if any.
    fn active_match(&self) -> Option<SearchMatch> {
        self.matches.get(self.active_idx?).copied()
    }

    /// Reset everything (bar closed, no matches, debounce cancelled).
    fn clear(&mut self) {
        *self = Self::default();
    }

    /// Flag the stored matches as stale (new terminal output).
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Whether a refresh is due: stale matches for an open bar with a query.
    fn needs_refresh(&self) -> bool {
        self.dirty && self.has_query()
    }

    /// Compute the visible search highlights (display coordinates) for the
    /// current viewport, to pass to the element for painting.
    pub(crate) fn visible_highlights(
        &self,
        display_offset: usize,
        num_lines: usize,
        num_cols: usize,
    ) -> Vec<SearchHighlight> {
        if !self.active || self.matches.is_empty() {
            return Vec::new();
        }
        let active = self.active_idx;
        let mut out = Vec::new();
        for (i, m) in self.matches.iter().enumerate() {
            let row = m.display_row(display_offset);
            if row < 0 || row >= num_lines as i32 {
                continue;
            }
            let start_col = m.start_col.min(num_cols) as i32;
            let end_col = m.end_col.min(num_cols) as i32;
            if end_col <= start_col {
                continue;
            }
            out.push(SearchHighlight {
                display_line: row,
                start_col,
                end_col,
                active: active == Some(i),
            });
        }
        out
    }
}

/// The `display_offset` that centres `match_line` (grid line of the match) in
/// a viewport of `num_lines`, clamped to the scrollback (`total_lines`).
/// `None` when the viewport is empty.
pub(crate) fn centered_offset(
    match_line: i32,
    total_lines: usize,
    num_lines: usize,
) -> Option<usize> {
    if num_lines == 0 {
        return None;
    }
    let max_offset = total_lines.saturating_sub(num_lines);
    // Center the match: display_row = num_lines / 2 → offset = num_lines/2 - line.
    let desired = (num_lines / 2) as i32 - match_line;
    Some(desired.clamp(0, max_offset as i32) as usize)
}

impl LocalTerminalView {
    /// Open the search bar (Ctrl+F). Creates a fresh `InputState`, focuses it,
    /// and seeds it with the current query (if reopening without closing).
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.active {
            // Already open → focus the input.
            if let Some(state) = self.search.input.as_ref() {
                state.update(cx, |s, cx| s.focus(window, cx));
            }
            return;
        }
        self.search.active = true;
        let state = cx.new(|cx| InputState::new(window, cx).placeholder("Find in terminal"));
        self.search.input = Some(state.clone());

        // Subscribe to input events → drive the search + navigation.
        let subscription =
            cx.subscribe(&state, |this, state, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let query = state.read(cx).value().to_string();
                    this.search.query = query;
                    // Debounce the search — cancel any pending search and schedule
                    // a new one. This prevents a full-grid scan on every keystroke
                    // while typing.
                    this.search.debounce_task = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(SEARCH_DEBOUNCE_MS))
                            .await;
                        this.update(cx, |this, cx| {
                            this.run_search(cx);
                            cx.notify();
                        })
                        .ok();
                    }));
                }
                InputEvent::PressEnter { shift, .. } => {
                    this.goto_match(*shift, cx);
                    cx.notify();
                }
                InputEvent::Focus | InputEvent::Blur => {}
            });
        self.search.input_subscription = Some(subscription);

        state.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Close the search bar (Esc) and clear all match state + highlights.
    /// Dropping the debounce task cancels any pending search.
    pub(crate) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search.clear();
        cx.notify();
    }

    /// Ctrl+F / Edit ▸ Find: open the bar, or close it when already open.
    pub(crate) fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.active {
            self.close_search(cx);
        } else {
            self.open_search(window, cx);
        }
    }

    /// Run the search against the session and store the matches. Resets the
    /// active index to the first match (when navigating forward) — but if the
    /// cursor/viewport is nearer the bottom we keep the last match active so
    /// the first `Enter` jumps forward. For simplicity we start at the first
    /// match; the user navigates from there.
    fn run_search(&mut self, cx: &mut Context<Self>) {
        let matches = self
            .session
            .read(cx)
            .search(&self.search.query, self.search.options);
        self.search.set_matches(matches);
        self.search.dirty = false;
        // Scroll the first active match into view.
        self.scroll_to_active_match(cx);
    }

    /// Re-run the search against the current terminal content **without**
    /// resetting the active match index, when output arrived since the last
    /// run ([`SearchState::mark_dirty`]). Called once per frame from `render`.
    ///
    /// New terminal output shifts the alacritty grid coordinate system as
    /// lines scroll into history, so the `line` values stored in
    /// [`SearchState::matches`] would otherwise point at the wrong visual rows.
    /// Re-running the search refreshes them with the current grid coordinates
    /// so highlights stay aligned with their content.
    ///
    /// Unlike [`run_search`](Self::run_search) this does **not** scroll the
    /// viewport — new output must not move the user's view to the active match.
    /// The active index is kept (clamped to the new length) so the user does
    /// not lose their navigation position.
    pub(crate) fn refresh_search_if_dirty(&mut self, cx: &mut Context<Self>) {
        if !self.search.needs_refresh() {
            self.search.dirty = false;
            return;
        }
        let matches = self
            .session
            .read(cx)
            .search(&self.search.query, self.search.options);
        self.search.refresh_matches(matches);
        self.search.dirty = false;
    }

    /// Navigate to the previous (`backward = true`, Shift+Enter) or next match
    /// and scroll it into view.
    pub(crate) fn goto_match(&mut self, backward: bool, cx: &mut Context<Self>) {
        if self.search.step(backward) {
            self.scroll_to_active_match(cx);
        }
    }

    /// Scroll the viewport so the active match is visible (centered when
    /// possible, otherwise clamped to the top/bottom of the scrollback).
    fn scroll_to_active_match(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.search.active_match() else {
            return;
        };
        let info = self.session.read(cx).terminal_info();
        let Some(desired) = centered_offset(m.line, info.total_lines, info.num_lines) else {
            return;
        };
        let delta = desired as i32 - info.display_offset as i32;
        if delta != 0 {
            self.session.update(cx, |s, _| s.scroll(delta));
        }
    }

    /// Render the search bar overlay (top-right). Returns `None` when search is
    /// inactive.
    pub(crate) fn render_search_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.search.active {
            return None;
        }
        // Read the three colours by value instead of cloning the theme (PERF-05).
        let (bar_bg, border, foreground) = {
            let t = cx.theme();
            (t.background.opacity(0.97), t.border, t.foreground)
        };
        let input_state = self.search.input.clone()?;
        let view = cx.entity();
        let total = self.search.matches.len();
        let current = self.search.active_idx.map(|i| i + 1).filter(|_| total > 0);
        let counter: SharedString = if total == 0 {
            "0/0".into()
        } else {
            format!("{}/{}", current.unwrap_or(0), total).into()
        };

        // Toggle button states.
        let case_on = self.search.options.case_sensitive;
        let word_on = self.search.options.whole_word;

        Some(
            div()
                .id("terminal-search-bar")
                .absolute()
                .top_2()
                .right_2()
                .w(px(360.0))
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .px_1p5()
                .py_1()
                .rounded_md()
                .bg(bar_bg)
                .border_1()
                .border_color(border)
                .shadow_sm()
                // The search bar overlays the terminal grid. Stop left-button
                // mouse down/up from bubbling into the terminal's mouse handlers,
                // otherwise rapid clicks on the nav buttons accumulate click_count
                // in the terminal (triple-click → select line) and mouse_up would
                // copy the terminal selection to the clipboard. Button on_click
                // still fires: click synthesis runs on the (deeper) button hitbox
                // before propagation is stopped here.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Case-sensitivity toggle (Ghost style).
                .child(
                    Toggle::new("search-case")
                        .ghost()
                        .xsmall()
                        .label("Aa")
                        .tooltip("Match case")
                        .checked(case_on)
                        .on_click(cx.listener(|v, checked: &bool, _, cx| {
                            v.search.options.case_sensitive = *checked;
                            v.run_search(cx);
                            cx.notify();
                        })),
                )
                // Whole-word toggle (Ghost style).
                .child(
                    Toggle::new("search-word")
                        .ghost()
                        .xsmall()
                        .label("W")
                        .tooltip("Match whole word")
                        .checked(word_on)
                        .on_click(cx.listener(|v, checked: &bool, _, cx| {
                            v.search.options.whole_word = *checked;
                            v.run_search(cx);
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.0)).h(px(18.0)).bg(border))
                // The text input — grows to fill.
                .child(
                    div()
                        .flex_1()
                        .min_w(px(120.0))
                        .child(Input::new(&input_state).appearance(false).bordered(false)),
                )
                .child(
                    div()
                        .id("search-counter")
                        .px_1()
                        .text_xs()
                        .text_color(foreground)
                        .child(counter),
                )
                // Previous match (↑).
                .child(
                    Button::new("search-prev")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ArrowUp)
                        .tooltip("Previous match (Shift+Enter)")
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |v, cx| v.goto_match(true, cx));
                            }
                        }),
                )
                // Next match (↓).
                .child(
                    Button::new("search-next")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ArrowDown)
                        .tooltip("Next match (Enter)")
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |v, cx| v.goto_match(false, cx));
                            }
                        }),
                )
                // Close (Esc).
                .child(
                    Button::new("search-close")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .tooltip("Close (Esc)")
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |v, cx| v.close_search(cx));
                            }
                        }),
                )
                // Esc closes the search bar (handled here, scoped to the bar).
                .on_key_down({
                    let view = view.clone();
                    move |e: &KeyDownEvent, _, cx: &mut App| {
                        if e.keystroke.key.as_str() == "escape" {
                            view.update(cx, |v, cx| v.close_search(cx));
                            cx.stop_propagation();
                        }
                    }
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use oneterm_terminal::SearchMatch;

    use super::{SearchState, centered_offset};

    fn m(line: i32, start_col: usize, end_col: usize) -> SearchMatch {
        SearchMatch {
            line,
            start_col,
            end_col,
        }
    }

    fn open_with(matches: Vec<SearchMatch>) -> SearchState {
        let mut s = SearchState {
            active: true,
            query: "x".to_string(),
            ..SearchState::default()
        };
        s.set_matches(matches);
        s
    }

    #[test]
    fn set_matches_starts_at_the_first_match() {
        let s = open_with(vec![m(0, 0, 1), m(2, 0, 1)]);
        assert_eq!(s.active_idx, Some(0));
        assert_eq!(open_with(vec![]).active_idx, None);
    }

    #[test]
    fn step_wraps_in_both_directions() {
        let mut s = open_with(vec![m(0, 0, 1), m(1, 0, 1), m(2, 0, 1)]);
        assert!(s.step(false));
        assert_eq!(s.active_idx, Some(1));
        assert!(s.step(false));
        assert!(s.step(false));
        assert_eq!(s.active_idx, Some(0), "next wraps to the first match");
        assert!(s.step(true));
        assert_eq!(s.active_idx, Some(2), "previous wraps to the last match");
        assert!(!open_with(vec![]).step(false));
    }

    #[test]
    fn refresh_keeps_a_valid_active_index_and_drops_a_stale_one() {
        let mut s = open_with(vec![m(0, 0, 1), m(1, 0, 1), m(2, 0, 1)]);
        s.step(false);
        s.step(false);
        s.refresh_matches(vec![m(5, 0, 1), m(6, 0, 1), m(7, 0, 1)]);
        assert_eq!(s.active_idx, Some(2));
        s.refresh_matches(vec![m(5, 0, 1)]);
        assert_eq!(s.active_idx, None);
    }

    #[test]
    fn visible_highlights_filter_to_the_viewport_and_clamp_columns() {
        // Grid lines: −5 is in history, 0..3 on screen, 3 is past a 3-row viewport.
        let s = open_with(vec![m(-5, 0, 2), m(0, 78, 90), m(1, 4, 4), m(3, 0, 1)]);
        // display_offset 0, 3 visible rows, 80 columns.
        let hl = s.visible_highlights(0, 3, 80);
        assert_eq!(hl.len(), 1);
        assert_eq!(
            (hl[0].display_line, hl[0].start_col, hl[0].end_col),
            (0, 78, 80)
        );
        assert!(!hl[0].active, "the active match (index 0) is scrolled off");
        // Scrolling up 5 lines brings the history match into view as row 0.
        let hl = s.visible_highlights(5, 3, 80);
        assert_eq!(hl.len(), 1);
        assert_eq!(hl[0].display_line, 0);
        assert!(hl[0].active);
    }

    #[test]
    fn dirty_flag_only_requests_a_refresh_for_an_open_query() {
        let mut s = open_with(vec![m(0, 0, 1)]);
        assert!(!s.needs_refresh());
        s.mark_dirty();
        assert!(s.needs_refresh());
        // A closed bar (or empty query) never refreshes, dirty or not.
        s.active = false;
        assert!(!s.needs_refresh());
        let mut closed = SearchState::default();
        closed.mark_dirty();
        assert!(!closed.needs_refresh());
    }

    #[test]
    fn visible_highlights_are_empty_when_the_bar_is_closed() {
        let mut s = open_with(vec![m(0, 0, 1)]);
        s.active = false;
        assert!(s.visible_highlights(0, 24, 80).is_empty());
    }

    #[test]
    fn centered_offset_centres_and_clamps() {
        // 100 lines total, 20 visible. A match on grid line −40 (history) is
        // centred with offset 10 + 40 = 50.
        assert_eq!(centered_offset(-40, 100, 20), Some(50));
        // A match on the current screen is centred by scrolling up (line 5 →
        // offset 5) but never past the live view (line 15 → 0, not −5).
        assert_eq!(centered_offset(5, 100, 20), Some(5));
        assert_eq!(centered_offset(15, 100, 20), Some(0));
        // Deep history clamps to the oldest page (max offset 80).
        assert_eq!(centered_offset(-500, 100, 20), Some(80));
        assert_eq!(centered_offset(0, 100, 0), None);
    }
}
