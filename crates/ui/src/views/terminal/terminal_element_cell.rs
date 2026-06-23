//! Per-cell rendering helpers cho `TerminalElement`: color resolution,
//! text run batching, line hashing, và `BatchedTextRun` paint.

use std::mem;

use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::Color;
use gpui::{Font, FontStyle, FontWeight, Hsla, Pixels, ShapedLine, TextRun, UnderlineStyle};

use myterm2_core::terminal::{
    IndexedCell, is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};

use super::terminal_element_layout::{BatchedTextRun, LayoutPoint};
use super::theme::{TerminalTheme, ensure_minimum_contrast, resolve_cell_color};

impl BatchedTextRun {
    pub(crate) fn new(start: LayoutPoint, c: char, mut style: TextRun) -> Self {
        // `style.len` từ cell_style đã = c.len_utf8() → KHÔNG cộng thêm.
        let text = c.to_string();
        debug_assert_eq!(style.len, c.len_utf8());
        let _ = &mut style; // giữ style nguyên (len đã đúng)
        Self {
            start,
            text,
            cell_count: 1,
            style,
        }
    }

    pub(crate) fn can_append(&self, other: &TextRun) -> bool {
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

    /// Paint text run dùng cached `ShapedLine` (đã shape ở prepaint).
    /// Không gọi `shape_line` ở đây — skip hoàn toàn cho non-dirty rows.
    /// Giống AtlasEngine `ShapedRow` — glyph data persisted, paint chỉ read.
    ///
    /// `x`, `y` đã là device-pixel snapped logical coords — không re-snap.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint(
        &self,
        shaped: &ShapedLine,
        x: Pixels,
        y: Pixels,
        _cell_w: Pixels,
        line_h: Pixels,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let pos = gpui::point(x, y);
        let _ = shaped.paint(pos, line_h, gpui::TextAlign::Left, None, window, cx);
    }
}

/// Convert cell → (fg Hsla, bg Hsla) sau inverse + contrast + dim.
pub(crate) fn cell_colors(cell: &Cell, theme: &TerminalTheme) -> (Hsla, Hsla) {
    let mut fg = cell.fg;
    let mut bg = cell.bg;
    if cell.flags.contains(Flags::INVERSE) {
        mem::swap(&mut fg, &mut bg);
    }
    let mut fg_h = resolve_cell_color(&fg, theme);
    let bg_h = resolve_cell_color(&bg, theme);
    if !is_app_chosen_exact_color(&fg) && !is_decorative_character(cell.c) {
        fg_h = ensure_minimum_contrast(fg_h, bg_h, theme.min_contrast);
    }
    if cell.flags.contains(Flags::DIM) {
        fg_h.a *= 0.7;
    }
    (fg_h, bg_h)
}

/// Blank cell = space + default bg + no extras.
pub(crate) fn is_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && is_default_background_color(&cell.bg)
        && cell.hyperlink().is_none()
        && !cell.flags.intersects(
            Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::WIDE_CHAR_SPACER,
        )
}

/// FNV-1a hash 1 display line — detect content change mà Term::damage()
/// không track (input()/write_at_cursor() không gọi damage_line()).
/// Hash bao gồm: char, fg, bg, flags, zerowidth, hyperlink.
pub(crate) fn line_hash(cells: &[&IndexedCell]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x100_0000_01b3;
    let mut h = FNV_OFFSET;
    for ic in cells {
        let cell = &ic.cell;
        // char
        h ^= cell.c as u64;
        h = h.wrapping_mul(FNV_PRIME);
        // fg color (Named/Spec/Indexed → u64)
        h ^= color_hash(cell.fg);
        h = h.wrapping_mul(FNV_PRIME);
        // bg color
        h ^= color_hash(cell.bg);
        h = h.wrapping_mul(FNV_PRIME);
        // flags
        h ^= cell.flags.bits() as u64;
        h = h.wrapping_mul(FNV_PRIME);
        // zerowidth + hyperlink
        if let Some(zw) = cell.zerowidth() {
            for &c in zw {
                h ^= c as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
        if let Some(hl) = cell.hyperlink() {
            for b in hl.uri().bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
    }
    h
}

/// Convert alacritty Color → u64 cho hashing.
fn color_hash(c: Color) -> u64 {
    match c {
        Color::Named(n) => n as u64,
        Color::Spec(rgb) => {
            0x1_0000 | (rgb.r as u64) | ((rgb.g as u64) << 8) | ((rgb.b as u64) << 16)
        }
        Color::Indexed(i) => 0x2_0000 | i as u64,
    }
}

/// Build TextRun cho cell (bold/italic/underline/strikethrough).
pub(crate) fn cell_style(cell: &Cell, fg: Hsla, base_font: &Font) -> TextRun {
    let underline = (cell.flags.intersects(Flags::ALL_UNDERLINES) || cell.hyperlink().is_some())
        .then(|| UnderlineStyle {
            color: Some(fg),
            thickness: gpui::px(1.0),
            wavy: cell.flags.contains(Flags::UNDERCURL),
        });
    let strikethrough = cell
        .flags
        .contains(Flags::STRIKEOUT)
        .then(|| gpui::StrikethroughStyle {
            color: Some(fg),
            thickness: gpui::px(1.0),
        });
    let weight = if cell.flags.contains(Flags::BOLD) {
        FontWeight::BOLD
    } else {
        base_font.weight
    };
    let style = if cell.flags.contains(Flags::ITALIC) {
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
