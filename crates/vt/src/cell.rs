//! What one grid position stores.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md>
//!
//! A [`Cell`] is eight bytes with no pointer and no allocation. Everything that
//! is rare — the style, a hyperlink, an image — is a `u16` id into a table the
//! terminal owns ([`crate::intern`]), so a screen of uniformly styled text
//! stores one style, not one per cell.

use std::fmt;

use bitflags::bitflags;

use crate::intern::{ExtrasId, GraphemeArena, GraphemeId, Interner, StyleId};

// Bit layout, exactly as the design states it. There is deliberately no
// `has_extras` bit: `extras_id() != ExtrasId::NONE` is the same test in
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
pub(crate) const CONTENT_LIMIT: u32 = 1 << 21;

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
    /// A single Unicode scalar value.
    Scalar(char),
    /// A multi-scalar grapheme cluster, stored in the terminal's arena.
    Grapheme(GraphemeId),
}

/// How one cell takes part in a double-width pair.
///
/// One exhaustive enum instead of three independent flags, so "wide character
/// with no spacer" is unrepresentable.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum CellWidth {
    /// An ordinary single-column cell.
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
    /// Written outside any marked region, or before the shell reported one.
    #[default]
    None,
    /// Inside the prompt, `OSC 133;A` to `OSC 133;B`.
    Prompt,
    /// The command line the user typed, `OSC 133;B` to `OSC 133;C`.
    Input,
    /// The command's output, `OSC 133;C` to `OSC 133;D`.
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
    /// The all-zero cell: a space, the default style, no extras.
    pub const EMPTY: Cell = Cell(0);

    /// The cell's text, as a scalar or as a grapheme id.
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

    /// Which half of a double-width pair this cell is, if any.
    pub fn width(self) -> CellWidth {
        CellWidth::from_bits((self.0 >> WIDTH_SHIFT) & TWO_BIT_MASK)
    }

    pub(crate) fn semantic(self) -> Semantic {
        Semantic::from_bits((self.0 >> SEMANTIC_SHIFT) & TWO_BIT_MASK)
    }

    /// DECSCA: stored, not yet honoured by any erase (reserved).
    pub(crate) fn protected(self) -> bool {
        self.0 & (1 << PROTECTED_SHIFT) != 0
    }

    /// The cell's style, as an id into the terminal's style table.
    /// `StyleId::DEFAULT` is the default style and is never evicted.
    pub fn style_id(self) -> StyleId {
        StyleId(((self.0 >> STYLE_SHIFT) & ID_MASK) as u16)
    }

    /// `ExtrasId::NONE` when the cell carries neither a hyperlink nor a graphic.
    pub fn extras_id(self) -> ExtrasId {
        ExtrasId(((self.0 >> EXTRAS_SHIFT) & ID_MASK) as u16)
    }

    /// This cell with different text. A grapheme id must fit the 21 content
    /// bits.
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

    /// This cell with a different [`CellWidth`] role.
    #[must_use]
    pub fn with_width(self, width: CellWidth) -> Cell {
        Cell((self.0 & !(TWO_BIT_MASK << WIDTH_SHIFT)) | (width.to_bits() << WIDTH_SHIFT))
    }

    /// This cell tagged with a different shell-integration region.
    #[must_use]
    pub fn with_semantic(self, semantic: Semantic) -> Cell {
        Cell((self.0 & !(TWO_BIT_MASK << SEMANTIC_SHIFT)) | (semantic.to_bits() << SEMANTIC_SHIFT))
    }

    /// This cell with the `DECSCA` protected bit set or cleared.
    #[must_use]
    pub fn with_protected(self, protected: bool) -> Cell {
        Cell((self.0 & !(1 << PROTECTED_SHIFT)) | (u64::from(protected) << PROTECTED_SHIFT))
    }

    /// This cell pointing at a different interned style.
    #[must_use]
    pub fn with_style(self, id: StyleId) -> Cell {
        Cell((self.0 & !(ID_MASK << STYLE_SHIFT)) | (u64::from(id.0) << STYLE_SHIFT))
    }

    /// This cell pointing at a different extras entry (hyperlink or graphic).
    #[must_use]
    pub fn with_extras(self, id: ExtrasId) -> Cell {
        Cell((self.0 & !(ID_MASK << EXTRAS_SHIFT)) | (u64::from(id.0) << EXTRAS_SHIFT))
    }

    /// The renderer's fast-path question: is there provably nothing to draw?
    ///
    /// Deliberately stricter than [`Cell::is_erasable`]: a bold space counts as
    /// blank for the erase scan, but must not take the renderer's blank fast
    /// path, because its background may differ.
    ///
    /// **Width is ignored by design.** A default-styled `WideSpacer` or
    /// `LeadingWideSpacer` is blank — correctly, since a spacer paints nothing —
    /// yet it is not equal to [`Cell::EMPTY`]. A caller asking "is this cell
    /// identical to a fresh one?" must compare against `Cell::EMPTY` instead.
    pub(crate) fn is_blank(self) -> bool {
        matches!(self.content(), CellContent::Scalar(' '))
            && self.style_id() == StyleId::DEFAULT
            && self.extras_id() == ExtrasId::NONE
    }

    /// The looser "this cell holds nothing worth keeping" rule, used by row
    /// shrink and the `ED 2` occupancy scan.
    ///
    /// Takes the interner because the rule inspects the cell's colours and
    /// attributes, which live behind `style_id`, as well as its extras.
    pub(crate) fn is_erasable(self, interner: &Interner) -> bool {
        if !matches!(self.content(), CellContent::Scalar(' ' | '\t')) {
            return false;
        }
        if self.width().is_spacer() {
            return false;
        }
        // A cell covered by an image is never erasable, or shrinking a
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

    /// The raw packed word. For tests and snapshots only; the bit layout is
    /// not part of the crate's stable surface.
    pub fn to_bits(self) -> u64 {
        self.0
    }
}

/// Repair the wide pair a write at `col` is about to split, within one row.
///
/// Writing over a `Wide` cell clears the `WideSpacer` to its right; writing
/// over a `WideSpacer` clears the `Wide` cell to its left, which drops its
/// glyph and its grapheme but keeps its style. The cross-row half (a previous
/// row's trailing `LeadingWideSpacer`) needs a grid and lives with the print
/// path.
pub(crate) fn repair_wide_pair_in_row(row: &mut [Cell], col: usize) {
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
    /// Foreground colour, as the stream named it.
    pub fg: Color,
    /// Background colour, as the stream named it.
    pub bg: Color,
    /// SGR 58/59, the underline's own colour. Stored and reported unchanged;
    /// whether a renderer honours it is the renderer's business.
    pub underline_color: Option<Color>,
    /// Bold, italic, the underline family, and the rest of the SGR flags.
    pub attrs: Attrs,
}

impl Style {
    /// Default foreground and background, no attributes. Always interned at
    /// `StyleId::DEFAULT`.
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
    /// The SGR attribute flags a style carries.
    ///
    /// Every flag the parser understands is stored, including ones a given
    /// renderer may choose not to draw.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct Attrs: u16 {
        /// SGR 1.
        const BOLD             = 1 << 0;
        /// SGR 2.
        const DIM              = 1 << 1;
        /// SGR 3.
        const ITALIC           = 1 << 2;
        /// SGR 7: swap foreground and background at draw time.
        const INVERSE          = 1 << 3;
        /// SGR 8: draw nothing, keep the background.
        const HIDDEN           = 1 << 4;
        /// SGR 9.
        const STRIKEOUT        = 1 << 5;
        /// SGR 5.
        const BLINK_SLOW       = 1 << 6;
        /// SGR 6.
        const BLINK_FAST       = 1 << 7;
        /// SGR 4.
        const UNDERLINE        = 1 << 8;
        /// SGR 21.
        const DOUBLE_UNDERLINE = 1 << 9;
        /// SGR 4:3.
        const UNDERCURL        = 1 << 10;
        /// SGR 4:4.
        const DOTTED_UNDERLINE = 1 << 11;
        /// SGR 4:5.
        const DASHED_UNDERLINE = 1 << 12;
        /// SGR 53.
        const OVERLINE         = 1 << 13;

        /// Every underline style at once, for "is this cell underlined at all".
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
    /// One of the terminal's named slots, such as the default foreground.
    Named(NamedColor),
    /// An index into the 256-colour palette, SGR 38;5 / 48;5.
    Palette(u8),
    /// A direct 24-bit colour, SGR 38;2 / 48;2.
    Rgb(Rgb),
}

/// A 24-bit colour, one byte per channel.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Rgb {
    /// Red, 0 to 255.
    pub r: u8,
    /// Green, 0 to 255.
    pub g: u8,
    /// Blue, 0 to 255.
    pub b: u8,
}

/// The named colour slots a stream can select, resolved against the embedder's
/// theme and any `OSC 4` / `OSC 10` overrides.
///
/// The first sixteen are palette entries 0 to 15 under their conventional
/// names; the rest are slots that have no palette index at all.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NamedColor {
    /// Palette 0.
    Black,
    /// Palette 1.
    Red,
    /// Palette 2.
    Green,
    /// Palette 3.
    Yellow,
    /// Palette 4.
    Blue,
    /// Palette 5.
    Magenta,
    /// Palette 6.
    Cyan,
    /// Palette 7.
    White,
    /// Palette 8.
    BrightBlack,
    /// Palette 9.
    BrightRed,
    /// Palette 10.
    BrightGreen,
    /// Palette 11.
    BrightYellow,
    /// Palette 12.
    BrightBlue,
    /// Palette 13.
    BrightMagenta,
    /// Palette 14.
    BrightCyan,
    /// Palette 15.
    BrightWhite,
    /// The default foreground, `OSC 10`.
    Foreground,
    /// The default background, `OSC 11`.
    Background,
    /// The cursor colour, `OSC 12`.
    Cursor,
    /// The foreground bold text uses when bright-bold is enabled.
    BrightForeground,
    /// The foreground dim text uses, SGR 2 over the default foreground.
    DimForeground,
    /// SGR 2 over palette 0.
    DimBlack,
    /// SGR 2 over palette 1.
    DimRed,
    /// SGR 2 over palette 2.
    DimGreen,
    /// SGR 2 over palette 3.
    DimYellow,
    /// SGR 2 over palette 4.
    DimBlue,
    /// SGR 2 over palette 5.
    DimMagenta,
    /// SGR 2 over palette 6.
    DimCyan,
    /// SGR 2 over palette 7.
    DimWhite,
}

// Kept in a sibling file (the suite is substantial) while staying `cell::tests`.
#[cfg(test)]
#[path = "cell_tests.rs"]
mod tests;
