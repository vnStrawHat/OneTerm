//! Which way the Terminal Tab row being drawn is laid out (`IN-0055`).
//!
//! A panel draws its own tab row (`Panel::title`) without knowing which tab group it sits in,
//! and the shell cannot name the terminal panel type (R4). The vertical tab rail therefore says
//! "this row goes in a rail" around its own `title()` calls with [`TabRowLayout::scope`], and
//! the row reads [`TabRowLayout::current`] while it builds itself. Every other caller (the kit's
//! strip, a drag preview, a terminal tab dropped into the right dock) sees the horizontal row.

use std::ops::{Deref, DerefMut};

use gpui::{App, Global};

/// The shape of a Terminal Tab's row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabRowLayout {
    /// A content-sized tab in the strip above the terminal.
    #[default]
    Horizontal,
    /// A full-width row in the vertical rail.
    Vertical,
    /// A compact cell in the collapsed rail: the tab's position (1-based) and its status.
    Collapsed(usize),
}

impl Global for TabRowLayout {}

/// The vertical row's geometry in logical pixels. The panel draws the row and the rail draws the
/// grip beside it, so both read the same numbers: two lines (the title, then the directory and
/// the status icon) between equal padding.
impl TabRowLayout {
    /// Height of one row of the vertical rail.
    pub const VERTICAL_ROW_HEIGHT: f32 = 64.0;
    /// Padding above the first line and below the second, divider included.
    pub const VERTICAL_ROW_PADDING: f32 = 10.0;
    /// The rail's 1 px divider under every row. It takes 1 px of the bottom padding, so the
    /// row's content box is still the lines plus the padding and the row is 64 px.
    pub const VERTICAL_ROW_DIVIDER: f32 = 1.0;
    /// Height of the title line.
    pub const VERTICAL_FIRST_LINE_HEIGHT: f32 = 24.0;
    /// Height of the directory and status line.
    pub const VERTICAL_SECOND_LINE_HEIGHT: f32 = 20.0;

    /// Distance from the top of a vertical row to the middle of its title line, where the
    /// rail centres its reorder grip.
    pub const fn vertical_first_line_centre() -> f32 {
        Self::VERTICAL_ROW_PADDING + Self::VERTICAL_FIRST_LINE_HEIGHT / 2.0
    }
}

impl TabRowLayout {
    /// The layout the row being built now should take.
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    /// Make rows take this layout until the returned guard is dropped, which puts the previous
    /// layout back, on every exit including an early return or a panic that unwinds. The guard
    /// stands in for the `&mut App` while it lives.
    pub fn scope(self, cx: &mut App) -> TabRowScope<'_> {
        let previous = Self::current(cx);
        cx.set_global(self);
        TabRowScope { cx, previous }
    }
}

/// The app context with a [`TabRowLayout`] in force; see [`TabRowLayout::scope`].
pub struct TabRowScope<'a> {
    cx: &'a mut App,
    previous: TabRowLayout,
}

impl Deref for TabRowScope<'_> {
    type Target = App;

    fn deref(&self) -> &App {
        self.cx
    }
}

impl DerefMut for TabRowScope<'_> {
    fn deref_mut(&mut self) -> &mut App {
        self.cx
    }
}

impl Drop for TabRowScope<'_> {
    fn drop(&mut self) {
        self.cx.set_global(self.previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vertical_row_is_two_lines_between_equal_padding() {
        assert_eq!(
            2.0 * TabRowLayout::VERTICAL_ROW_PADDING
                + TabRowLayout::VERTICAL_FIRST_LINE_HEIGHT
                + TabRowLayout::VERTICAL_SECOND_LINE_HEIGHT,
            TabRowLayout::VERTICAL_ROW_HEIGHT
        );
        assert_eq!(TabRowLayout::vertical_first_line_centre(), 22.0);
        // The divider is carved out of the bottom padding, not added to the row.
        assert!(TabRowLayout::VERTICAL_ROW_DIVIDER < TabRowLayout::VERTICAL_ROW_PADDING);
    }

    #[gpui::test]
    fn the_layout_is_horizontal_except_while_a_rail_draws(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert_eq!(TabRowLayout::current(cx), TabRowLayout::Horizontal);
            {
                let mut vertical = TabRowLayout::Vertical.scope(cx);
                assert_eq!(TabRowLayout::current(&vertical), TabRowLayout::Vertical);
                {
                    // A nested scope restores the layout of the one around it.
                    let inner = TabRowLayout::Horizontal.scope(&mut vertical);
                    assert_eq!(TabRowLayout::current(&inner), TabRowLayout::Horizontal);
                }
                assert_eq!(TabRowLayout::current(&vertical), TabRowLayout::Vertical);
            }
            assert_eq!(TabRowLayout::current(cx), TabRowLayout::Horizontal);
        });
    }

    #[gpui::test]
    fn a_collapsed_row_carries_its_position(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let scope = TabRowLayout::Collapsed(3).scope(cx);
            assert_eq!(TabRowLayout::current(&scope), TabRowLayout::Collapsed(3));
        });
    }

    #[gpui::test]
    fn the_layout_is_restored_when_the_drawing_unwinds(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _scope = TabRowLayout::Vertical.scope(cx);
                panic!("a title that fails to build");
            }));
            assert!(result.is_err());
            assert_eq!(TabRowLayout::current(cx), TabRowLayout::Horizontal);
        });
    }
}
