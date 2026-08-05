//! `BatchedTextRun` methods — text run batching + paint.

use gpui::{Pixels, ShapedLine};

use super::super::layout::{BatchedTextRun, LayoutPoint};

impl BatchedTextRun {
    pub(crate) fn new(start: LayoutPoint, c: char, mut style: gpui::TextRun) -> Self {
        let text = c.to_string();
        debug_assert_eq!(style.len, c.len_utf8());
        let _ = &mut style;
        Self {
            start,
            text,
            cell_count: 1,
            style,
        }
    }

    pub(crate) fn can_append(&self, other: &gpui::TextRun) -> bool {
        self.style.font == other.font
            && self.style.color == other.color
            && self.style.background_color == other.background_color
            && self.style.underline == other.underline
            && self.style.strikethrough == other.strikethrough
    }

    pub(crate) fn append_char(&mut self, c: char) {
        self.text.push(c);
        self.cell_count += 1;
        self.style.len += c.len_utf8();
    }

    pub(crate) fn append_zw(&mut self, c: char) {
        self.text.push(c);
        self.style.len += c.len_utf8();
    }

    /// Paint the text run using the cached `ShapedLine`.
    pub(crate) fn paint(
        &self,
        shaped: &ShapedLine,
        x: Pixels,
        y: Pixels,
        line_h: Pixels,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let pos = gpui::point(x, y);
        let _ = shaped.paint(pos, line_h, gpui::TextAlign::Left, None, window, cx);
    }
}
