//! Blank cell detection.

use alacritty_terminal::term::cell::{Cell, Flags};
use oneterm_terminal::is_default_background_color;

/// Blank cell = space + default bg + no extras.
pub(crate) fn is_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && is_default_background_color(&cell.bg)
        && cell.hyperlink().is_none()
        && !cell.flags.intersects(
            Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::WIDE_CHAR_SPACER,
        )
}
