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
}

impl Global for TabRowLayout {}

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
