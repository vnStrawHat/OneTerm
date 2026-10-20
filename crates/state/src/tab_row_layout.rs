//! Which way the Terminal Tab row being drawn is laid out (`IN-0055`).
//!
//! A panel draws its own tab row (`Panel::title`) without knowing which tab group it sits in,
//! and the shell cannot name the terminal panel type (R4). The vertical tab rail therefore says
//! "this row goes in a rail" around its own `title()` calls with [`TabRowLayout::draw`], and the
//! row reads [`TabRowLayout::current`] while it builds itself. Every other caller (the kit's
//! strip, a drag preview, a terminal tab dropped into the right dock) sees the horizontal row.

use gpui::{App, Global};

/// The shape of a Terminal Tab's row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabRowLayout {
    /// A content-sized tab in the strip above the terminal.
    #[default]
    Horizontal,
    /// A full-width row in the vertical rail.
    Vertical,
}

impl Global for TabRowLayout {}

impl TabRowLayout {
    /// The layout the row being built now should take.
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    /// Run `draw` with rows taking this layout, then put the previous layout back.
    pub fn draw<R>(self, cx: &mut App, draw: impl FnOnce(&mut App) -> R) -> R {
        let previous = Self::current(cx);
        cx.set_global(self);
        let drawn = draw(cx);
        cx.set_global(previous);
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn the_layout_is_horizontal_except_while_a_rail_draws(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert_eq!(TabRowLayout::current(cx), TabRowLayout::Horizontal);
            let seen = TabRowLayout::Vertical.draw(cx, |cx| {
                // A nested scope restores the layout of the one around it.
                TabRowLayout::Horizontal.draw(cx, |cx| {
                    assert_eq!(TabRowLayout::current(cx), TabRowLayout::Horizontal);
                });
                TabRowLayout::current(cx)
            });
            assert_eq!(seen, TabRowLayout::Vertical);
            assert_eq!(TabRowLayout::current(cx), TabRowLayout::Horizontal);
        });
    }
}
