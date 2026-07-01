//! Build a `TextRun` for a cell (bold/italic/underline/strikethrough).

use alacritty_terminal::term::cell::Cell;
use gpui::{Font, FontStyle, FontWeight, TextRun, UnderlineStyle};

pub(crate) fn cell_style(cell: &Cell, fg: gpui::Hsla, base_font: &Font) -> TextRun {
    let underline = (cell
        .flags
        .intersects(alacritty_terminal::term::cell::Flags::ALL_UNDERLINES)
        || cell.hyperlink().is_some())
    .then(|| UnderlineStyle {
        color: Some(fg),
        thickness: gpui::px(1.0),
        wavy: cell
            .flags
            .contains(alacritty_terminal::term::cell::Flags::UNDERCURL),
    });
    let strikethrough = cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::STRIKEOUT)
        .then(|| gpui::StrikethroughStyle {
            color: Some(fg),
            thickness: gpui::px(1.0),
        });
    let weight = if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::BOLD)
    {
        FontWeight::BOLD
    } else {
        base_font.weight
    };
    let style = if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::ITALIC)
    {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    TextRun {
        len: cell.c.len_utf8(),
        color: fg,
        background_color: None,
        font: Font {
            weight,
            style,
            ..base_font.clone()
        },
        underline,
        strikethrough,
    }
}
