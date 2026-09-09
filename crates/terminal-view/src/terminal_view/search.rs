//! Terminal in-buffer search — the Ctrl+F bar and match highlighting.
//!
//! [`SearchState`] holds the query, options, matches in grid coordinates and
//! the active index. Matching is delegated to the backend
//! (`TerminalSession::search`); the view converts a match to a display row
//! with `display_row = line + display_offset` to highlight it in the viewport
//! and to scroll it into view.
//!
//! Key bindings: `Ctrl+F` toggles the bar, `Enter` / `Shift+Enter` step
//! forward / backward (wrapping), `Esc` closes.

use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, ParentElement as _, SharedString, Styled, Subscription, Task,
    Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _, Toggle, ToggleVariants as _},
};
use oneterm_terminal::{SearchMatch, SearchOptions};

use super::TerminalView;
use crate::render::overlay::SearchHighlight;

/// Debounce delay between the last keystroke in the search input and the
/// full-grid scan (typing must not fire a search per character).
const SEARCH_DEBOUNCE_MS: u64 = 150;

/// In-buffer search state owned by the view.
#[derive(Default)]
pub(super) struct SearchState {
    /// Whether the search bar is open.
    active: bool,
    /// The search query (kept in sync with the `InputState`).
    query: String,
    /// Search options (case-sensitivity, whole-word).
    options: SearchOptions,
    /// Matches in grid coordinates (top-to-bottom order).
    matches: Vec<SearchMatch>,
    /// Index into `matches` of the active (current) match.
    active_idx: Option<usize>,
    /// The `InputState` for the search bar input.
    input: Option<Entity<InputState>>,
    /// Debounce task for the search — delays the full-grid scan after the
    /// last keystroke; replaced on every change.
    debounce_task: Option<Task<()>>,
    /// Subscription to the search input's events (kept for the life of the
    /// bar; dropping it in `clear` unsubscribes).
    input_subscription: Option<Subscription>,
    /// Terminal output arrived since the matches were computed: their grid
    /// coordinates are stale and must be refreshed on the next frame.
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
    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Whether a refresh is due: stale matches for an open bar with a query.
    fn needs_refresh(&self) -> bool {
        self.dirty && self.has_query()
    }

    /// Whether the search bar's text input owns keyboard focus.
    pub(super) fn input_is_focused(&self, window: &Window, cx: &App) -> bool {
        self.input
            .as_ref()
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Refill `out` with the visible search highlights (display coordinates,
    /// clamped to the viewport) for the element to paint.
    pub(super) fn visible_highlights_into(
        &self,
        display_offset: usize,
        num_lines: usize,
        num_cols: usize,
        out: &mut Vec<SearchHighlight>,
    ) {
        out.clear();
        if !self.active || self.matches.is_empty() {
            return;
        }
        let active = self.active_idx;
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
    }
}

/// The `display_offset` that centres `match_line` (grid line of the match) in
/// a viewport of `num_lines`, clamped to the scrollback (`total_lines`).
/// `None` when the viewport is empty.
fn centered_offset(match_line: i32, total_lines: usize, num_lines: usize) -> Option<usize> {
    if num_lines == 0 {
        return None;
    }
    let max_offset = total_lines.saturating_sub(num_lines);
    // display_row = num_lines / 2 → offset = num_lines / 2 - line.
    let desired = (num_lines / 2) as i32 - match_line;
    Some(desired.clamp(0, max_offset as i32) as usize)
}

impl TerminalView {
    /// Open the search bar (Ctrl+F): a fresh `InputState`, focused. Reopening
    /// an open bar only refocuses the input.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.active {
            if let Some(state) = self.search.input.as_ref() {
                state.update(cx, |s, cx| s.focus(window, cx));
            }
            return;
        }
        self.search.active = true;
        let state = cx.new(|cx| InputState::new(window, cx).placeholder("Find in terminal"));
        self.search.input = Some(state.clone());

        let subscription =
            cx.subscribe(&state, |this, state, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.search.query = state.read(cx).value().to_string();
                    // Replacing the task cancels the previous scan, so typing
                    // never runs a full-grid search per keystroke.
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
    /// Dropping the debounce task cancels any pending search. Keyboard focus
    /// returns to the terminal: the bar's input owned it and is being dropped,
    /// so without this the window would be left with nothing focused.
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.clear();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Ctrl+F / Edit ▸ Find: open the bar, or close it when already open.
    pub(crate) fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.active {
            self.close_search(window, cx);
        } else {
            self.open_search(window, cx);
        }
    }

    /// Run the search against the session, start from the first match and
    /// scroll it into view.
    fn run_search(&mut self, cx: &mut Context<Self>) {
        let matches = self
            .session
            .read(cx)
            .search(&self.search.query, self.search.options);
        self.search.set_matches(matches);
        self.search.dirty = false;
        self.scroll_to_active_match(cx);
    }

    /// Re-run the search **without** resetting the active match or moving the
    /// viewport, when output arrived since the last run. Called once per frame
    /// from `render`: new output shifts the grid coordinate system as lines
    /// scroll into history, so the stored `line` values would otherwise point
    /// at the wrong visual rows.
    pub(super) fn refresh_search_if_dirty(&mut self, cx: &mut Context<Self>) {
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

    /// Navigate to the previous (`backward`, Shift+Enter) or next match and
    /// scroll it into view.
    fn goto_match(&mut self, backward: bool, cx: &mut Context<Self>) {
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

    /// The search bar overlay (top-right); `None` while search is inactive.
    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.search.active {
            return None;
        }
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
                // The bar overlays the grid. Left presses must not bubble into
                // the terminal's mouse handlers: rapid clicks on the nav
                // buttons would accumulate `click_count` (triple-click →
                // select line) and the release would copy the selection.
                // Button `on_click` still fires: click synthesis runs on the
                // deeper button hitbox before propagation stops here.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
                .child(
                    Button::new("search-close")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .tooltip("Close (Esc)")
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                view.update(cx, |v, cx| v.close_search(window, cx));
                            }
                        }),
                )
                // Esc closes the bar; scoped to the bar so the terminal never
                // sees it.
                .on_key_down({
                    let view = view.clone();
                    move |e: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        if e.keystroke.key.as_str() == "escape" {
                            view.update(cx, |v, cx| v.close_search(window, cx));
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

    use super::{SearchHighlight, SearchState, centered_offset};

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

    fn highlights(
        s: &SearchState,
        display_offset: usize,
        num_lines: usize,
        num_cols: usize,
    ) -> Vec<SearchHighlight> {
        let mut out = Vec::new();
        s.visible_highlights_into(display_offset, num_lines, num_cols, &mut out);
        out
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
        let hl = highlights(&s, 0, 3, 80);
        assert_eq!(hl.len(), 1);
        assert_eq!(
            (hl[0].display_line, hl[0].start_col, hl[0].end_col),
            (0, 78, 80)
        );
        assert!(!hl[0].active, "the active match (index 0) is scrolled off");
        // Scrolling up 5 lines brings the history match into view as row 0.
        let hl = highlights(&s, 5, 3, 80);
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
        assert!(highlights(&s, 0, 24, 80).is_empty());
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
