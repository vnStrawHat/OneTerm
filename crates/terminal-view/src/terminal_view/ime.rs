//! IME (`EntityInputHandler`) for [`TerminalView`].
//!
//! The "document" is the backend's marked-text buffer, not the scrollback:
//! composition state lives in the session, commits are typed input, and the
//! candidate window docks at the cursor cell. Alt-screen programs manage
//! their own input, so composition is disabled there.

use std::ops::Range;

use gpui::{Bounds, EntityInputHandler, Pixels, Point, UTF16Selection, Window, size};

use super::TerminalView;

impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.session.read(cx).is_alt_screen() {
            return None;
        }
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Range<usize>> {
        self.session
            .read(cx)
            .marked_text()
            .map(|t| 0..t.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.session.update(cx, |s, _| s.clear_marked_text());
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // The trusted write source for typed text: `on_key_down` leaves plain
        // characters to this path so nothing is typed twice. Typing snaps the
        // viewport back to the live screen.
        self.session.update(cx, |s, _| {
            s.scroll_to_bottom();
            s.commit_text(text);
        });
        self.has_bell = false;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        _new_selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if new_text.is_empty() {
            self.session.update(cx, |s, _| s.clear_marked_text());
        } else {
            self.session
                .update(cx, |s, _| s.set_marked_text(new_text.to_string()));
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // The candidate window docks at the terminal cursor: its cell in the
        // grid geometry of the last paint, shifted by the preedit offset.
        let geometry = self.render_state.borrow().geometry?;
        let query = self.session.read(cx).query_state();
        let row = (query.cursor_line + query.display_offset as i32).max(0) as usize;
        let col = query.cursor_col + range_utf16.start;
        let origin = geometry.cell_origin(row, col);
        let metrics = geometry.metrics;
        Some(Bounds::new(
            origin,
            size(metrics.cell_width, metrics.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> bool {
        !self.session.read(cx).is_alt_screen()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, EntityInputHandler as _, TestAppContext, point, size};
    use oneterm_terminal::test_support::{FakeInputCall, FakeTerminalSession};

    use super::super::test_support::open_view;

    #[gpui::test]
    fn ime_disabled_on_alt_screen(cx: &mut TestAppContext) {
        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        let (view, cx) = open_view(cx, session);

        let (accepts, selection) = view.update_in(cx, |v, window, cx| {
            (
                v.accepts_text_input(window, cx),
                v.selected_text_range(false, window, cx),
            )
        });
        assert!(accepts && selection.is_some(), "primary screen composes");

        probe.set_alt_screen(true);
        let (accepts, selection) = view.update_in(cx, |v, window, cx| {
            (
                v.accepts_text_input(window, cx),
                v.selected_text_range(false, window, cx),
            )
        });
        assert!(!accepts && selection.is_none(), "alt screen disables IME");
    }

    #[gpui::test]
    fn ime_commit_scrolls_and_clears_bell(cx: &mut TestAppContext) {
        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        let (view, cx) = open_view(cx, session);

        view.update_in(cx, |v, window, cx| {
            v.has_bell = true;
            v.replace_text_in_range(None, "ạ", window, cx);
        });
        assert!(
            probe.input_calls().contains(&FakeInputCall::ScrollToBottom),
            "a commit snaps to the live screen"
        );
        assert_eq!(probe.writes(), vec!["ạ".as_bytes().to_vec()]);
        assert!(!view.read_with(cx, |v, _| v.has_bell));
    }

    #[gpui::test]
    fn ime_bounds_at_cursor_cell(cx: &mut TestAppContext) {
        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        let (view, cx) = open_view(cx, session);
        // Paint once so the grid geometry (the hit-test contract) exists.
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        probe.set_cursor(2, 5);

        let geometry = view.read_with(cx, |v, _| {
            v.render_state.borrow().geometry.expect("painted once")
        });
        let bounds = view
            .update_in(cx, |v, window, cx| {
                v.bounds_for_range(1..1, Bounds::default(), window, cx)
            })
            .expect("bounds after the first paint");
        // Cursor cell (2, 5) plus one preedit column.
        assert_eq!(bounds.origin, geometry.cell_origin(2, 6));
        assert_eq!(
            bounds.size,
            size(geometry.metrics.cell_width, geometry.metrics.line_height)
        );
        assert_ne!(bounds.origin, point(gpui::px(0.0), gpui::px(0.0)));
    }
}
