//! IME (`EntityInputHandler`) cho `LocalTerminalView`.

use gpui::{EntityInputHandler, UTF16Selection, Window};

use super::terminal_view::LocalTerminalView;

impl EntityInputHandler for LocalTerminalView {
    fn text_for_range(
        &mut self,
        _range: std::ops::Range<usize>,
        _adjusted: &mut Option<std::ops::Range<usize>>,
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
        // Alt-screen (vd vim/less): tắt IME.
        if self.session.read(cx).is_alt_screen() {
            None
        } else {
            Some(UTF16Selection {
                range: (0..0),
                reversed: false,
            })
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
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
        _range: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Commit IME hoặc ký tự thường (normal mode). Đây là nguồn ghi tin cậy —
        // on_key_down skip ký tự thường khi IME active để tránh double (aa).
        self.session.update(cx, |s, _| s.commit_text(text));
        if self.has_bell {
            self.has_bell = false;
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        new_text: &str,
        _new_selected: Option<std::ops::Range<usize>>,
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
        range_utf16: std::ops::Range<usize>,
        element_bounds: gpui::Bounds<gpui::Pixels>,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        let cur = self.session.read(cx).cursor_bounds()?;
        let m = *self.metrics.borrow();
        let cw = f32::from(m.cell_width).max(1.0);
        let lh = f32::from(m.line_height).max(1.0);
        let x = f32::from(element_bounds.origin.x) + cur.x + cw * range_utf16.start as f32;
        let y = f32::from(element_bounds.origin.y) + cur.y;
        Some(gpui::Bounds::new(
            gpui::point(gpui::px(x), gpui::px(y)),
            gpui::size(gpui::px(cw), gpui::px(lh)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> bool {
        true
    }
}
