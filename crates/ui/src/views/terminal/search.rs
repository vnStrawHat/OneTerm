//! Terminal in-buffer search — Ctrl+F search bar + match highlighting.
//!
//! The search state lives on [`LocalTerminalView`] (query, options, matches in
//! grid coordinates, active index). The search bar is a small overlay
//! (top-right of the terminal panel) built with `gpui_component::input`.
//!
//! Matching is delegated to the backend via
//! [`TerminalSession::search`](oneterm_core::TerminalSession::search), which
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

use gpui::{
    App, AppContext, Context, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, SharedString, Styled, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _, Toggle, ToggleVariants as _},
};

use super::view::LocalTerminalView;

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

impl LocalTerminalView {
    /// Open the search bar (Ctrl+F). Creates a fresh `InputState`, focuses it,
    /// and seeds it with the current query (if reopening without closing).
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_active {
            // Already open → focus the input.
            if let Some(state) = self.search_input.as_ref() {
                state.update(cx, |s, cx| s.focus(window, cx));
            }
            return;
        }
        self.search_active = true;
        let state = cx.new(|cx| InputState::new(window, cx).placeholder("Find in terminal"));
        self.search_input = Some(state.clone());

        // Subscribe to input events → drive the search + navigation.
        cx.subscribe(&state, |this, state, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                let query = state.read(cx).value().to_string();
                this.search_query = query;
                this.run_search(cx);
                cx.notify();
            }
            InputEvent::PressEnter { shift, .. } => {
                this.goto_match(*shift, cx);
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => {}
        })
        .detach();

        state.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Close the search bar (Esc) and clear all match state + highlights.
    pub(crate) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search_active = false;
        self.search_input = None;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_active_idx = None;
        cx.notify();
    }

    /// Run the search against the session and store the matches. Resets the
    /// active index to the first match (when navigating forward) — but if the
    /// cursor/viewport is nearer the bottom we keep the last match active so
    /// the first `Enter` jumps forward. For simplicity we start at the first
    /// match; the user navigates from there.
    fn run_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search_query.clone();
        let opts = self.search_options;
        let matches = self.session.read(cx).search(&query, opts);
        self.search_matches = matches;
        self.search_active_idx = if self.search_matches.is_empty() {
            None
        } else {
            // Pick the first match at or below the current viewport top.
            Some(0)
        };
        // Scroll the first active match into view.
        self.scroll_to_active_match(cx);
    }

    /// Navigate to the next (`forward = false`) or previous (`forward = true`)
    /// match and scroll it into view.
    pub(crate) fn goto_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            return;
        }
        let idx = match self.search_active_idx {
            Some(i) => {
                let len = self.search_matches.len();
                // forward = true (Shift+Enter) → previous; forward = false → next.
                if forward {
                    (i + len - 1) % len
                } else {
                    (i + 1) % len
                }
            }
            None => 0,
        };
        self.search_active_idx = Some(idx);
        self.scroll_to_active_match(cx);
    }

    /// Scroll the viewport so the active match is visible (centered when
    /// possible, otherwise clamped to the top/bottom of the scrollback).
    fn scroll_to_active_match(&mut self, cx: &mut Context<Self>) {
        let Some(idx) = self.search_active_idx else {
            return;
        };
        let Some(m) = self.search_matches.get(idx).copied() else {
            return;
        };
        let info = self.session.read(cx).terminal_info();
        let total = info.total_lines;
        let num_lines = info.num_lines;
        if num_lines == 0 {
            return;
        }
        let max_offset = total.saturating_sub(num_lines);
        // Center the match: display_row = num_lines / 2 → offset = num_lines/2 - line.
        let desired = (num_lines / 2) as i32 - m.line;
        let desired = desired.max(0).min(max_offset as i32) as usize;
        let delta = desired as i32 - info.display_offset as i32;
        if delta != 0 {
            self.session.update(cx, |s, _| s.scroll(delta));
        }
    }

    /// Compute the visible search highlights (display coordinates) for the
    /// current viewport, to pass to the element for painting.
    pub(crate) fn visible_search_highlights(
        &self,
        display_offset: usize,
        num_lines: usize,
        num_cols: usize,
    ) -> Vec<SearchHighlight> {
        if !self.search_active || self.search_matches.is_empty() {
            return Vec::new();
        }
        let active = self.search_active_idx;
        let mut out = Vec::new();
        for (i, m) in self.search_matches.iter().enumerate() {
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

    /// Render the search bar overlay (top-right). Returns `None` when search is
    /// inactive.
    pub(crate) fn render_search_bar(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !self.search_active {
            return None;
        }
        let theme = cx.theme().clone();
        let input_state = self.search_input.clone()?;
        let view = cx.entity();
        let total = self.search_matches.len();
        let current = self.search_active_idx.map(|i| i + 1).filter(|_| total > 0);
        let counter: SharedString = if total == 0 {
            "0/0".into()
        } else {
            format!("{}/{}", current.unwrap_or(0), total).into()
        };

        // Toggle button states.
        let case_on = self.search_options.case_sensitive;
        let word_on = self.search_options.whole_word;

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
                .bg(theme.background.opacity(0.97))
                .border_1()
                .border_color(theme.border)
                .shadow_sm()
                // Case-sensitivity toggle (Ghost style).
                .child(
                    Toggle::new("search-case")
                        .ghost()
                        .xsmall()
                        .label("Aa")
                        .tooltip("Match case")
                        .checked(case_on)
                        .on_click(cx.listener(|v, checked: &bool, _, cx| {
                            v.search_options.case_sensitive = *checked;
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
                            v.search_options.whole_word = *checked;
                            v.run_search(cx);
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.0)).h(px(18.0)).bg(theme.border))
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
                        .text_color(theme.foreground)
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
