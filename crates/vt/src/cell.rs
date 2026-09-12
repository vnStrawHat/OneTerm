//! What one grid position stores.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md`.
//!
//! A [`Cell`] is eight bytes with no pointer and no allocation. Everything that
//! is rare — the style, a hyperlink, an image — is a `u16` id into a table the
//! terminal owns ([`crate::intern`]), so a screen of uniformly styled text
//! stores one style, not one per cell.

use std::fmt;

use bitflags::bitflags;

use crate::intern::{ExtrasId, GraphemeArena, GraphemeId, Interner, StyleId};

// Bit layout, exactly as the design states it. There is deliberately no
// `has_extras` bit (R-34): `extras_id() != ExtrasId::NONE` is the same test in
// one compare, and the freed bit joins the reserved five.
//
//   0..21   content    Unicode scalar value, or a GraphemeId when `is_grapheme`
//   21      is_grapheme
//   22..24  width      Narrow | Wide | WideSpacer | LeadingWideSpacer
//   24..26  semantic   None | Prompt | Input | Output              (OSC 133)
//   26      protected                                              (DECSCA, reserved)
//   27..32  reserved   5 bits — kitty virtual placeholders, and headroom
//   32..48  style_id   into StyleSet   (0 = default, never evicted)
//   48..64  extras_id  into ExtrasTable (0 = none)
const CONTENT_MASK: u64 = (1 << 21) - 1;
const IS_GRAPHEME_SHIFT: u32 = 21;
const WIDTH_SHIFT: u32 = 22;
const SEMANTIC_SHIFT: u32 = 24;
const PROTECTED_SHIFT: u32 = 26;
const STYLE_SHIFT: u32 = 32;
const EXTRAS_SHIFT: u32 = 48;
const TWO_BIT_MASK: u64 = 0b11;
const ID_MASK: u64 = u16::MAX as u64;

/// The widest value the content bits hold: the whole Unicode scalar range
/// (`0x10FFFF` needs 21 bits) and, disjointly, the grapheme id space.
pub const CONTENT_LIMIT: u32 = 1 << 21;

/// Eight bytes, packed. `Cell::EMPTY` (all zero) is a valid empty cell: a space
/// with the default style and no extras, so a zeroed row is a blank row and
/// clearing a row with the default style is a `memset`.
///
/// The engine never stores a literal `NUL`; content `0` reads back as `' '`.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Cell(u64);

const _: () = assert!(size_of::<Cell>() == 8);

/// What the content bits mean, decided by the `is_grapheme` bit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CellContent {
    Scalar(char),
    Grapheme(GraphemeId),
}

/// One exhaustive enum instead of the reference's three independent flags, so
/// "wide character with no spacer" is unrepresentable.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum CellWidth {
    #[default]
    Narrow,
    /// The glyph of a double-width pair; its `WideSpacer` is the next column.
    Wide,
    /// The second column of a double-width pair.
    WideSpacer,
    /// A blank in the last column, holding the place of a wide glyph that did
    /// not fit and wrapped to the next row.
    LeadingWideSpacer,
}

/// OSC 133 shell integration: which part of the dialogue wrote this cell.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Semantic {
    #[default]
    None,
    Prompt,
    Input,
    Output,
}

impl CellWidth {
    const fn from_bits(bits: u64) -> Self {
        match bits {
            1 => CellWidth::Wide,
            2 => CellWidth::WideSpacer,
            3 => CellWidth::LeadingWideSpacer,
            _ => CellWidth::Narrow,
        }
    }

    const fn to_bits(self) -> u64 {
        match self {
            CellWidth::Narrow => 0,
            CellWidth::Wide => 1,
            CellWidth::WideSpacer => 2,
            CellWidth::LeadingWideSpacer => 3,
        }
    }

    /// Whether this half of a pair carries no glyph of its own.
    pub const fn is_spacer(self) -> bool {
        matches!(self, CellWidth::WideSpacer | CellWidth::LeadingWideSpacer)
    }
}

impl Semantic {
    const fn from_bits(bits: u64) -> Self {
        match bits {
            1 => Semantic::Prompt,
            2 => Semantic::Input,
            3 => Semantic::Output,
            _ => Semantic::None,
        }
    }

    const fn to_bits(self) -> u64 {
        match self {
            Semantic::None => 0,
            Semantic::Prompt => 1,
            Semantic::Input => 2,
            Semantic::Output => 3,
        }
    }
}

impl Cell {
    pub const EMPTY: Cell = Cell(0);

    pub fn content(self) -> CellContent {
        let bits = (self.0 & CONTENT_MASK) as u32;
        if self.0 & (1 << IS_GRAPHEME_SHIFT) != 0 {
            return CellContent::Grapheme(GraphemeId(bits));
        }
        // Content 0 is the blank cell, and no scalar above 0x10FFFF can be
        // stored, so the fallback is unreachable rather than lossy.
        CellContent::Scalar(if bits == 0 {
            ' '
        } else {
            char::from_u32(bits).unwrap_or(' ')
        })
    }

    pub fn width(self) -> CellWidth {
        CellWidth::from_bits((self.0 >> WIDTH_SHIFT) & TWO_BIT_MASK)
    }

    pub fn semantic(self) -> Semantic {
        Semantic::from_bits((self.0 >> SEMANTIC_SHIFT) & TWO_BIT_MASK)
    }

    /// DECSCA: stored, not yet honoured by any erase (reserved).
    pub fn protected(self) -> bool {
        self.0 & (1 << PROTECTED_SHIFT) != 0
    }

    pub fn style_id(self) -> StyleId {
        StyleId(((self.0 >> STYLE_SHIFT) & ID_MASK) as u16)
    }

    /// `ExtrasId::NONE` when the cell carries neither a hyperlink nor a graphic.
    pub fn extras_id(self) -> ExtrasId {
        ExtrasId(((self.0 >> EXTRAS_SHIFT) & ID_MASK) as u16)
    }

    #[must_use]
    pub fn with_content(self, content: CellContent) -> Cell {
        let (bits, is_grapheme) = match content {
            CellContent::Scalar(c) => (c as u32, false),
            CellContent::Grapheme(id) => {
                debug_assert!(id.0 < CONTENT_LIMIT, "grapheme id outside the content bits");
                (id.0, true)
            }
        };
        let mut raw = self.0 & !(CONTENT_MASK | (1 << IS_GRAPHEME_SHIFT));
        raw |= u64::from(bits) & CONTENT_MASK;
        raw |= u64::from(is_grapheme) << IS_GRAPHEME_SHIFT;
        Cell(raw)
    }

    #[must_use]
    pub fn with_width(self, width: CellWidth) -> Cell {
        Cell((self.0 & !(TWO_BIT_MASK << WIDTH_SHIFT)) | (width.to_bits() << WIDTH_SHIFT))
    }

    #[must_use]
    pub fn with_semantic(self, semantic: Semantic) -> Cell {
        Cell((self.0 & !(TWO_BIT_MASK << SEMANTIC_SHIFT)) | (semantic.to_bits() << SEMANTIC_SHIFT))
    }

    #[must_use]
    pub fn with_protected(self, protected: bool) -> Cell {
        Cell((self.0 & !(1 << PROTECTED_SHIFT)) | (u64::from(protected) << PROTECTED_SHIFT))
    }

    #[must_use]
    pub fn with_style(self, id: StyleId) -> Cell {
        Cell((self.0 & !(ID_MASK << STYLE_SHIFT)) | (u64::from(id.0) << STYLE_SHIFT))
    }

    #[must_use]
    pub fn with_extras(self, id: ExtrasId) -> Cell {
        Cell((self.0 & !(ID_MASK << EXTRAS_SHIFT)) | (u64::from(id.0) << EXTRAS_SHIFT))
    }

    /// The renderer's fast-path question: is there provably nothing to draw?
    ///
    /// Deliberately stricter than [`Cell::is_erasable`]: a bold space is blank
    /// to the reference's erase scan (trap 37) but must not take the
    /// renderer's blank fast path, because its background may differ.
    pub fn is_blank(self) -> bool {
        matches!(self.content(), CellContent::Scalar(' '))
            && self.style_id() == StyleId::DEFAULT
            && self.extras_id() == ExtrasId::NONE
    }

    /// The reference's looser "this cell holds nothing worth keeping" rule,
    /// used by row shrink and the `ED 2` occupancy scan.
    ///
    /// Takes the interner because the rule inspects the cell's colours and
    /// attributes, which live behind `style_id`, as well as its extras.
    pub fn is_erasable(self, interner: &Interner) -> bool {
        if !matches!(self.content(), CellContent::Scalar(' ' | '\t')) {
            return false;
        }
        if self.width().is_spacer() {
            return false;
        }
        // R-13: a cell covered by an image is never erasable, or shrinking a
        // row would drop rows an image still occupies.
        if interner.resolve_extras(self.extras_id()).graphic.is_some() {
            return false;
        }
        let style = interner.resolve_style(self.style_id());
        style.fg == Style::DEFAULT.fg
            && style.bg == Style::DEFAULT.bg
            && !style
                .attrs
                .intersects(Attrs::INVERSE | Attrs::ALL_UNDERLINES | Attrs::STRIKEOUT)
    }

    /// The cell's text as one scalar: `' '` for a blank, `'\t'` for a tab cell,
    /// the scalar otherwise, and the base scalar of a grapheme cell.
    ///
    /// Callers that need the whole cluster resolve it through
    /// [`GraphemeArena::resolve`].
    pub fn text_char(self, graphemes: &GraphemeArena) -> char {
        match self.content() {
            CellContent::Scalar(c) => c,
            CellContent::Grapheme(id) => graphemes.resolve(id).first().copied().unwrap_or(' '),
        }
    }

    /// The raw packed word. For tests and for the corpus snapshot only.
    pub fn to_bits(self) -> u64 {
        self.0
    }
}

/// Repair the wide pair a write at `col` is about to split — the same-row half
/// of trap 6.
///
/// Writing over a `Wide` cell clears the `WideSpacer` to its right; writing
/// over a `WideSpacer` clears the `Wide` cell to its left, which drops its
/// glyph and its grapheme but keeps its style. The cross-row half (a previous
/// row's trailing `LeadingWideSpacer`) needs a grid and lives with the print
/// path.
pub fn repair_wide_pair_in_row(row: &mut [Cell], col: usize) {
    let Some(cell) = row.get(col).copied() else {
        return;
    };
    match cell.width() {
        CellWidth::Wide if col + 1 < row.len() => {
            row[col + 1] = row[col + 1].with_width(CellWidth::Narrow);
        }
        CellWidth::Wide | CellWidth::WideSpacer if col > 0 => {
            row[col - 1] = row[col - 1]
                .with_width(CellWidth::Narrow)
                .with_content(CellContent::Scalar(' '));
        }
        _ => {}
    }
}

impl fmt::Debug for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cell")
            .field("content", &self.content())
            .field("width", &self.width())
            .field("semantic", &self.semantic())
            .field("protected", &self.protected())
            .field("style", &self.style_id().0)
            .field("extras", &self.extras_id().0)
            .finish()
    }
}

/// Everything about a cell's appearance, interned once per distinct value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    /// SGR 58/59. Stored now, rendered when the renderer grows underline
    /// colours; today's engine drops it entirely.
    pub underline_color: Option<Color>,
    pub attrs: Attrs,
}

impl Style {
    pub const DEFAULT: Style = Style {
        fg: Color::Named(NamedColor::Foreground),
        bg: Color::Named(NamedColor::Background),
        underline_color: None,
        attrs: Attrs::empty(),
    };
}

impl Default for Style {
    fn default() -> Self {
        Style::DEFAULT
    }
}

bitflags! {
    /// Blink and overline are stored even though the current renderer draws
    /// neither: the bits are already allocated, so a later renderer packet
    /// needs no engine change.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct Attrs: u16 {
        const BOLD             = 1 << 0;
        const DIM              = 1 << 1;
        const ITALIC           = 1 << 2;
        const INVERSE          = 1 << 3;
        const HIDDEN           = 1 << 4;
        const STRIKEOUT        = 1 << 5;
        const BLINK_SLOW       = 1 << 6;
        const BLINK_FAST       = 1 << 7;
        const UNDERLINE        = 1 << 8;
        const DOUBLE_UNDERLINE = 1 << 9;
        const UNDERCURL        = 1 << 10;
        const DOTTED_UNDERLINE = 1 << 11;
        const DASHED_UNDERLINE = 1 << 12;
        const OVERLINE         = 1 << 13;

        const ALL_UNDERLINES = Self::UNDERLINE.bits()
            | Self::DOUBLE_UNDERLINE.bits()
            | Self::UNDERCURL.bits()
            | Self::DOTTED_UNDERLINE.bits()
            | Self::DASHED_UNDERLINE.bits();
    }
}

/// A colour as the stream named it. Resolution to pixels happens outside the
/// lock, against the theme and the OSC override table.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Color {
    Named(NamedColor),
    Palette(u8),
    Rgb(Rgb),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Typed colour keys, replacing the magic indices 256 / 257 / 258 the current
/// code carries in three files.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground,
    Background,
    Cursor,
    BrightForeground,
    DimForeground,
    DimBlack,
    DimRed,
    DimGreen,
    DimYellow,
    DimBlue,
    DimMagenta,
    DimCyan,
    DimWhite,
}

// Kept in a sibling file (the suite is substantial) while staying `cell::tests`,
// which is the path the design's verification list names.
#[cfg(test)]
#[path = "cell_tests.rs"]
mod tests;
