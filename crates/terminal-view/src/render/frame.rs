//! The per-frame terminal snapshot in view-owned types.
//!
//! This is the only file under `render/` that names an `alacritty_terminal`
//! type: it wraps the engine's `TerminalContent` and exposes cells, cursor,
//! selection and damage through `Cell`, `Color`, `CellFlags`, `CursorShape`,
//! `Selection` and `Damage`, so the rest of the engine never depends on the
//! grid implementation (HLD idea 3).

use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{
    Color as VteColor, CursorShape as VteCursorShape, NamedColor, Rgb as VteRgb,
};
use oneterm_terminal::{IndexedCell, TermDamageInfo, TerminalContent, TerminalSession};

/// FNV-1a, the hash used for row hashes, shaped-run keys and the style key.
/// Deterministic across processes so tests can reason about hash equality.
#[derive(Clone, Copy)]
pub(crate) struct Fnv1a(u64);

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub(crate) fn new() -> Self {
        Self(Self::OFFSET)
    }

    #[inline]
    pub(crate) fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    #[inline]
    pub(crate) fn write_u8(&mut self, v: u8) {
        self.write(&[v]);
    }

    #[inline]
    pub(crate) fn write_u16(&mut self, v: u16) {
        self.write(&v.to_le_bytes());
    }

    #[inline]
    pub(crate) fn write_u32(&mut self, v: u32) {
        self.write(&v.to_le_bytes());
    }

    pub(crate) fn finish(self) -> u64 {
        self.0
    }
}

/// Number of display rows and columns of a frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct GridSize {
    pub rows: u16,
    pub cols: u16,
}

/// Which display rows the engine reported as changed since the last snapshot.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Damage<'a> {
    Full,
    Rows(&'a [usize]),
}

/// A cell color before palette resolution. `theme.color(c)` maps it to `Hsla`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Color {
    Foreground,
    Background,
    Cursor,
    BrightForeground,
    DimForeground,
    /// 0..16, `NamedColor::Black..=BrightWhite`.
    Ansi(u8),
    /// 0..8, `NamedColor::DimBlack..=DimWhite`.
    DimAnsi(u8),
    /// 0..=255.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// True-color or extended-palette foregrounds are exactly what the program
    /// asked for; contrast enforcement leaves them alone.
    pub(crate) fn is_app_chosen_exact(self) -> bool {
        matches!(self, Color::Rgb(..)) || matches!(self, Color::Indexed(n) if n >= 16)
    }

    fn from_vte(c: VteColor) -> Self {
        match c {
            VteColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
            VteColor::Indexed(n) => Color::Indexed(n),
            VteColor::Named(nc) => match nc {
                NamedColor::Foreground => Color::Foreground,
                NamedColor::Background => Color::Background,
                NamedColor::Cursor => Color::Cursor,
                NamedColor::BrightForeground => Color::BrightForeground,
                NamedColor::DimForeground => Color::DimForeground,
                NamedColor::DimBlack
                | NamedColor::DimRed
                | NamedColor::DimGreen
                | NamedColor::DimYellow
                | NamedColor::DimBlue
                | NamedColor::DimMagenta
                | NamedColor::DimCyan
                | NamedColor::DimWhite => {
                    Color::DimAnsi((nc as usize - NamedColor::DimBlack as usize) as u8)
                }
                // The remaining variants are the 16 ANSI colors, `Black = 0`.
                other => Color::Ansi((other as usize).min(15) as u8),
            },
        }
    }

    /// The engine color this value came from; used by the theme to build its
    /// lookup table through `oneterm_terminal::resolve_color`.
    pub(crate) fn to_vte(self) -> VteColor {
        match self {
            Color::Foreground => VteColor::Named(NamedColor::Foreground),
            Color::Background => VteColor::Named(NamedColor::Background),
            Color::Cursor => VteColor::Named(NamedColor::Cursor),
            Color::BrightForeground => VteColor::Named(NamedColor::BrightForeground),
            Color::DimForeground => VteColor::Named(NamedColor::DimForeground),
            Color::Ansi(n) => VteColor::Named(ANSI_NAMED[usize::from(n.min(15))]),
            Color::DimAnsi(n) => VteColor::Named(DIM_NAMED[usize::from(n.min(7))]),
            Color::Indexed(n) => VteColor::Indexed(n),
            Color::Rgb(r, g, b) => VteColor::Spec(VteRgb { r, g, b }),
        }
    }
}

const ANSI_NAMED: [NamedColor; 16] = [
    NamedColor::Black,
    NamedColor::Red,
    NamedColor::Green,
    NamedColor::Yellow,
    NamedColor::Blue,
    NamedColor::Magenta,
    NamedColor::Cyan,
    NamedColor::White,
    NamedColor::BrightBlack,
    NamedColor::BrightRed,
    NamedColor::BrightGreen,
    NamedColor::BrightYellow,
    NamedColor::BrightBlue,
    NamedColor::BrightMagenta,
    NamedColor::BrightCyan,
    NamedColor::BrightWhite,
];

const DIM_NAMED: [NamedColor; 8] = [
    NamedColor::DimBlack,
    NamedColor::DimRed,
    NamedColor::DimGreen,
    NamedColor::DimYellow,
    NamedColor::DimBlue,
    NamedColor::DimMagenta,
    NamedColor::DimCyan,
    NamedColor::DimWhite,
];

/// SGR attribute bits of a cell. Every underline kind collapses into
/// `UNDERLINE`; `UNDERCURL` is additionally set for the wavy variant.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash, Debug)]
pub(crate) struct CellFlags(u16);

impl CellFlags {
    #[cfg(test)]
    pub(crate) const NONE: Self = Self(0);
    pub(crate) const INVERSE: Self = Self(1 << 0);
    pub(crate) const BOLD: Self = Self(1 << 1);
    pub(crate) const ITALIC: Self = Self(1 << 2);
    pub(crate) const DIM: Self = Self(1 << 3);
    pub(crate) const HIDDEN: Self = Self(1 << 4);
    pub(crate) const UNDERLINE: Self = Self(1 << 5);
    pub(crate) const UNDERCURL: Self = Self(1 << 6);
    pub(crate) const STRIKEOUT: Self = Self(1 << 7);
    pub(crate) const WIDE_CHAR: Self = Self(1 << 8);
    pub(crate) const WIDE_CHAR_SPACER: Self = Self(1 << 9);
    pub(crate) const LEADING_WIDE_CHAR_SPACER: Self = Self(1 << 10);
    pub(crate) const WRAPLINE: Self = Self(1 << 11);

    #[inline]
    pub(crate) fn contains(self, f: Self) -> bool {
        self.0 & f.0 == f.0
    }

    #[inline]
    pub(crate) fn intersects(self, f: Self) -> bool {
        self.0 & f.0 != 0
    }

    pub(crate) fn union(self, f: Self) -> Self {
        Self(self.0 | f.0)
    }

    fn from_vte(f: Flags) -> Self {
        let mut out = 0u16;
        let pairs = [
            (Flags::INVERSE, Self::INVERSE),
            (Flags::BOLD, Self::BOLD),
            (Flags::ITALIC, Self::ITALIC),
            (Flags::DIM, Self::DIM),
            (Flags::HIDDEN, Self::HIDDEN),
            (Flags::ALL_UNDERLINES, Self::UNDERLINE),
            (Flags::UNDERCURL, Self::UNDERCURL),
            (Flags::STRIKEOUT, Self::STRIKEOUT),
            (Flags::WIDE_CHAR, Self::WIDE_CHAR),
            (Flags::WIDE_CHAR_SPACER, Self::WIDE_CHAR_SPACER),
            (
                Flags::LEADING_WIDE_CHAR_SPACER,
                Self::LEADING_WIDE_CHAR_SPACER,
            ),
            (Flags::WRAPLINE, Self::WRAPLINE),
        ];
        for (vte, ours) in pairs {
            if f.intersects(vte) {
                out |= ours.0;
            }
        }
        Self(out)
    }

    #[cfg(test)]
    fn to_vte(self) -> Flags {
        let mut out = Flags::empty();
        let pairs = [
            (Flags::INVERSE, Self::INVERSE),
            (Flags::BOLD, Self::BOLD),
            (Flags::ITALIC, Self::ITALIC),
            (Flags::DIM, Self::DIM),
            (Flags::HIDDEN, Self::HIDDEN),
            (Flags::UNDERLINE, Self::UNDERLINE),
            (Flags::UNDERCURL, Self::UNDERCURL),
            (Flags::STRIKEOUT, Self::STRIKEOUT),
            (Flags::WIDE_CHAR, Self::WIDE_CHAR),
            (Flags::WIDE_CHAR_SPACER, Self::WIDE_CHAR_SPACER),
            (
                Flags::LEADING_WIDE_CHAR_SPACER,
                Self::LEADING_WIDE_CHAR_SPACER,
            ),
            (Flags::WRAPLINE, Self::WRAPLINE),
        ];
        for (vte, ours) in pairs {
            if self.contains(ours) {
                out |= vte;
            }
        }
        out
    }
}

/// One grid cell, borrowed from the frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cell<'a> {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: CellFlags,
    /// Combining marks / ZWJ sequences attached to `ch`.
    pub zerowidth: &'a [char],
    /// FNV of the OSC 8 id + uri, for hashing and span detection.
    pub hyperlink: Option<u64>,
}

impl Cell<'_> {
    /// Whether this column holds glyph-less padding of a neighbouring wide char.
    pub(crate) fn is_spacer(&self) -> bool {
        self.flags
            .intersects(CellFlags::WIDE_CHAR_SPACER.union(CellFlags::LEADING_WIDE_CHAR_SPACER))
    }

    /// The view-owned cell for a session `IndexedCell` — also the bridge for
    /// `query_line_range_cells` slices (URL detection), which never become a
    /// `Frame`.
    pub(crate) fn from_indexed(ic: &IndexedCell) -> Cell<'_> {
        let c = &ic.cell;
        Cell {
            ch: c.c,
            fg: Color::from_vte(c.fg),
            bg: Color::from_vte(c.bg),
            flags: CellFlags::from_vte(c.flags),
            zerowidth: c.zerowidth().unwrap_or(&[]),
            hyperlink: c.hyperlink().map(|h| {
                let mut hasher = Fnv1a::new();
                hasher.write(h.id().as_bytes());
                hasher.write_u8(0);
                hasher.write(h.uri().as_bytes());
                hasher.finish()
            }),
        }
    }
}

/// The OSC 8 target attached to a session cell, if any (rare path: click).
pub(crate) fn hyperlink_uri(ic: &IndexedCell) -> Option<String> {
    ic.cell.hyperlink().map(|h| h.uri().to_string())
}

/// One display row of the frame.
#[derive(Clone, Copy)]
pub(crate) struct FrameRow<'a> {
    cells: &'a [IndexedCell],
    row: usize,
}

impl<'a> FrameRow<'a> {
    pub(crate) fn index(&self) -> usize {
        self.row
    }

    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }

    pub(crate) fn cell(&self, col: usize) -> Cell<'a> {
        Cell::from_indexed(&self.cells[col])
    }

    pub(crate) fn cells(&self) -> impl Iterator<Item = Cell<'a>> + '_ {
        self.cells.iter().map(Cell::from_indexed)
    }

    /// `WRAPLINE` on the last cell: the logical line continues on the next row.
    pub(crate) fn wraps(&self) -> bool {
        self.cells
            .last()
            .is_some_and(|c| c.cell.flags.contains(Flags::WRAPLINE))
    }

    /// FNV-1a over everything a row plan depends on: char, colors, flags,
    /// zero-width chars and hyperlink identity. Reads the engine cell directly
    /// (no `Cell` conversion): with `Damage::Full` every row is hashed each
    /// frame, so this is the hottest loop of an idle-but-scrolling terminal.
    pub(crate) fn hash(&self) -> u64 {
        let mut h = Fnv1a::new();
        h.write_u32(self.cells.len() as u32);
        for ic in self.cells {
            let c = &ic.cell;
            h.write_u32(u32::from(c.c));
            hash_vte_color(c.fg, &mut h);
            hash_vte_color(c.bg, &mut h);
            h.write_u16(c.flags.bits());
            if let Some(extra) = c.zerowidth() {
                h.write_u8(extra.len() as u8);
                for &z in extra {
                    h.write_u32(u32::from(z));
                }
            } else {
                h.write_u8(0);
            }
            if let Some(link) = c.hyperlink() {
                h.write(link.id().as_bytes());
                h.write_u8(0);
                h.write(link.uri().as_bytes());
            }
        }
        h.finish()
    }

    /// The OSC 8 target of the cell at `col`.
    #[cfg(test)]
    pub(crate) fn hyperlink_uri(&self, col: usize) -> Option<String> {
        hyperlink_uri(&self.cells[col])
    }

    /// The row's display text for the semantic scanner: one entry per
    /// non-spacer cell (`\0` and tab read as a space, zero-width chars are
    /// skipped because classes are per column), with the column of every char
    /// and whether it is a wide char so classes can be flattened back.
    pub(crate) fn text_into(
        &self,
        text: &mut String,
        char_cols: &mut Vec<u16>,
        char_wide: &mut Vec<bool>,
    ) {
        text.clear();
        char_cols.clear();
        char_wide.clear();
        for (col, cell) in self.cells().enumerate() {
            if cell.is_spacer() {
                continue;
            }
            text.push(match cell.ch {
                '\0' | '\t' => ' ',
                c => c,
            });
            char_cols.push(col as u16);
            char_wide.push(cell.flags.contains(CellFlags::WIDE_CHAR));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CursorShape {
    Block,
    Beam,
    Underline,
    HollowBlock,
    Hidden,
}

impl CursorShape {
    fn from_vte(s: VteCursorShape) -> Self {
        match s {
            VteCursorShape::Block => CursorShape::Block,
            VteCursorShape::Underline => CursorShape::Underline,
            VteCursorShape::Beam => CursorShape::Beam,
            VteCursorShape::HollowBlock => CursorShape::HollowBlock,
            VteCursorShape::Hidden => CursorShape::Hidden,
        }
    }
}

/// The cursor in display coordinates. `row` may fall outside `0..rows` when
/// the viewport is scrolled into history; callers treat that as no cursor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Cursor {
    pub row: i32,
    pub col: u16,
    pub shape: CursorShape,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct GridPoint {
    pub row: i32,
    pub col: u16,
}

/// The selection in display coordinates, both ends inclusive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Selection {
    pub start: GridPoint,
    pub end: GridPoint,
    pub block: bool,
}

/// The reusable snapshot of one frame.
pub(crate) struct Frame {
    content: TerminalContent,
    size: GridSize,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    pub(crate) fn new() -> Self {
        let content = TerminalContent::default();
        let size = size_of(&content);
        Self { content, size }
    }

    /// The frame's only `snapshot_into`: it consumes the engine's damage, so it
    /// must run exactly once per painted frame.
    pub(crate) fn snapshot(&mut self, session: &dyn TerminalSession) {
        session.snapshot_into(&mut self.content);
        self.size = size_of(&self.content);
    }

    pub(crate) fn size(&self) -> GridSize {
        self.size
    }

    pub(crate) fn display_offset(&self) -> usize {
        self.content.display_offset
    }

    pub(crate) fn damage(&self) -> Damage<'_> {
        match &self.content.damage {
            TermDamageInfo::Full => Damage::Full,
            TermDamageInfo::Partial(rows) => Damage::Rows(rows),
        }
    }

    /// Display row `r`. The snapshot is dense in display order, so this is a
    /// slice; a snapshot that is not (a race with a resize) is searched by the
    /// cells' own line numbers instead.
    pub(crate) fn row(&self, r: usize) -> FrameRow<'_> {
        let cols = usize::from(self.size.cols);
        let rows = usize::from(self.size.rows);
        let cells = &self.content.cells;
        if cells.len() == rows * cols && r < rows {
            return FrameRow {
                cells: &cells[r * cols..(r + 1) * cols],
                row: r,
            };
        }
        let offset = self.content.display_offset as i64;
        let display_line = |c: &IndexedCell| i64::from(c.point.line.0) + offset;
        let start = cells.partition_point(|c| display_line(c) < r as i64);
        let end = start + cells[start..].partition_point(|c| display_line(c) == r as i64);
        FrameRow {
            cells: &cells[start..end],
            row: r,
        }
    }

    pub(crate) fn cursor(&self) -> Cursor {
        let c = &self.content.cursor;
        Cursor {
            row: c
                .point
                .line
                .0
                .saturating_add(self.content.display_offset as i32),
            col: c.point.column.0.min(usize::from(u16::MAX)) as u16,
            shape: CursorShape::from_vte(c.shape),
        }
    }

    pub(crate) fn selection(&self) -> Option<Selection> {
        let offset = self.content.display_offset as i32;
        self.content.selection.map(|s: SelectionRange| Selection {
            start: GridPoint {
                row: s.start.line.0.saturating_add(offset),
                col: s.start.column.0.min(usize::from(u16::MAX)) as u16,
            },
            end: GridPoint {
                row: s.end.line.0.saturating_add(offset),
                col: s.end.column.0.min(usize::from(u16::MAX)) as u16,
            },
            block: s.is_block,
        })
    }

    /// DECCKM: arrows send `ESC O x` instead of `ESC [ x`.
    pub(crate) fn app_cursor(&self) -> bool {
        self.content.mode.contains(TermMode::APP_CURSOR)
    }
}

#[inline]
fn hash_vte_color(c: VteColor, h: &mut Fnv1a) {
    match c {
        VteColor::Named(nc) => h.write_u16(nc as u16),
        VteColor::Spec(rgb) => h.write(&[0xff, rgb.r, rgb.g, rgb.b]),
        VteColor::Indexed(n) => h.write(&[0xfe, n]),
    }
}

fn size_of(content: &TerminalContent) -> GridSize {
    GridSize {
        rows: content.terminal_bounds.num_lines.min(usize::from(u16::MAX)) as u16,
        cols: content.terminal_bounds.num_cols.min(usize::from(u16::MAX)) as u16,
    }
}

/// Test-only frame construction from view-owned types, so tests of the modules
/// above this one never touch `alacritty_terminal` themselves.
#[cfg(test)]
pub(crate) mod test_support {
    use alacritty_terminal::index::{Column, Line, Point};
    use alacritty_terminal::selection::SelectionRange;
    use alacritty_terminal::term::RenderableCursor;
    use alacritty_terminal::term::cell::{Cell as VteCell, Hyperlink};
    use alacritty_terminal::vte::ansi::CursorShape as VteCursorShape;
    use oneterm_terminal::{IndexedCell, TermDamageInfo, TerminalContent};

    use super::{CellFlags, Color, CursorShape, Frame, GridPoint, GridSize};

    /// Builds a dense frame of `rows × cols` blank cells and lets tests set
    /// individual cells, the cursor, the selection, damage and scroll offset.
    pub(crate) struct FrameBuilder {
        content: TerminalContent,
        rows: usize,
        cols: usize,
    }

    impl FrameBuilder {
        pub(crate) fn new(rows: usize, cols: usize) -> Self {
            let mut content = TerminalContent::default();
            content.cells = (0..rows * cols)
                .map(|i| IndexedCell {
                    point: Point::new(Line((i / cols) as i32), Column(i % cols)),
                    cell: VteCell::default(),
                })
                .collect();
            content.terminal_bounds.num_lines = rows;
            content.terminal_bounds.num_cols = cols;
            content.total_lines = rows;
            content.cursor = RenderableCursor {
                shape: VteCursorShape::Hidden,
                point: Point::new(Line(0), Column(0)),
            };
            content.damage = TermDamageInfo::Full;
            Self {
                content,
                rows,
                cols,
            }
        }

        fn cell_mut(&mut self, row: usize, col: usize) -> &mut VteCell {
            assert!(row < self.rows && col < self.cols, "cell out of range");
            &mut self.content.cells[row * self.cols + col].cell
        }

        /// Write `text` from `(row, col)`; wide chars get their spacer cell.
        pub(crate) fn text(mut self, row: usize, col: usize, text: &str) -> Self {
            for (c, ch) in (col..).zip(text.chars()) {
                self.cell_mut(row, c).c = ch;
            }
            self
        }

        pub(crate) fn styled(
            mut self,
            row: usize,
            col: usize,
            ch: char,
            fg: Color,
            bg: Color,
            flags: CellFlags,
        ) -> Self {
            let cell = self.cell_mut(row, col);
            cell.c = ch;
            cell.fg = fg.to_vte();
            cell.bg = bg.to_vte();
            cell.flags = flags.to_vte();
            self
        }

        pub(crate) fn flags(mut self, row: usize, col: usize, flags: CellFlags) -> Self {
            self.cell_mut(row, col).flags = flags.to_vte();
            self
        }

        /// A wide char at `col` plus its spacer at `col + 1`.
        pub(crate) fn wide(mut self, row: usize, col: usize, ch: char) -> Self {
            {
                let cell = self.cell_mut(row, col);
                cell.c = ch;
                cell.flags = CellFlags::WIDE_CHAR.to_vte();
            }
            let spacer = self.cell_mut(row, col + 1);
            spacer.c = ' ';
            spacer.flags = CellFlags::WIDE_CHAR_SPACER.to_vte();
            self
        }

        pub(crate) fn zerowidth(mut self, row: usize, col: usize, marks: &[char]) -> Self {
            let cell = self.cell_mut(row, col);
            for &m in marks {
                cell.push_zerowidth(m);
            }
            self
        }

        pub(crate) fn hyperlink(mut self, row: usize, col: usize, uri: &str) -> Self {
            self.cell_mut(row, col)
                .set_hyperlink(Some(Hyperlink::new(None::<&str>, uri.to_string())));
            self
        }

        /// One OSC 8 link spanning `len` cells from `col` (one shared id, like
        /// a real `\e]8;;uri` run).
        pub(crate) fn hyperlink_run(
            mut self,
            row: usize,
            col: usize,
            len: usize,
            uri: &str,
        ) -> Self {
            let link = Hyperlink::new(None::<&str>, uri.to_string());
            for c in col..col + len {
                self.cell_mut(row, c).set_hyperlink(Some(link.clone()));
            }
            self
        }

        /// The dense cell slice, in the shape `query_line_range_cells` returns.
        pub(crate) fn into_cells(self) -> Vec<IndexedCell> {
            self.content.cells
        }

        pub(crate) fn cursor(mut self, row: i32, col: usize, shape: CursorShape) -> Self {
            self.content.cursor = RenderableCursor {
                shape: match shape {
                    CursorShape::Block => VteCursorShape::Block,
                    CursorShape::Beam => VteCursorShape::Beam,
                    CursorShape::Underline => VteCursorShape::Underline,
                    CursorShape::HollowBlock => VteCursorShape::HollowBlock,
                    CursorShape::Hidden => VteCursorShape::Hidden,
                },
                point: Point::new(Line(row - self.content.display_offset as i32), Column(col)),
            };
            self
        }

        /// Selection in grid coordinates (display row = row + display_offset).
        pub(crate) fn selection(mut self, start: GridPoint, end: GridPoint, block: bool) -> Self {
            self.content.selection = Some(SelectionRange {
                start: Point::new(Line(start.row), Column(usize::from(start.col))),
                end: Point::new(Line(end.row), Column(usize::from(end.col))),
                is_block: block,
            });
            self
        }

        pub(crate) fn display_offset(mut self, offset: usize) -> Self {
            self.content.display_offset = offset;
            self
        }

        pub(crate) fn damage_rows(mut self, rows: &[usize]) -> Self {
            self.content.damage = TermDamageInfo::Partial(rows.to_vec());
            self
        }

        pub(crate) fn build(self) -> Frame {
            let size = GridSize {
                rows: self.rows as u16,
                cols: self.cols as u16,
            };
            Frame {
                content: self.content,
                size,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::FrameBuilder;
    use super::*;

    #[test]
    fn frame_colors_convert_round_trip() {
        for c in [
            Color::Foreground,
            Color::Background,
            Color::Cursor,
            Color::BrightForeground,
            Color::DimForeground,
            Color::Ansi(0),
            Color::Ansi(9),
            Color::Ansi(15),
            Color::DimAnsi(0),
            Color::DimAnsi(7),
            Color::Indexed(3),
            Color::Indexed(200),
            Color::Rgb(1, 2, 3),
        ] {
            assert_eq!(Color::from_vte(c.to_vte()), c, "{c:?}");
        }
        assert!(Color::Rgb(0, 0, 0).is_app_chosen_exact());
        assert!(Color::Indexed(16).is_app_chosen_exact());
        assert!(!Color::Indexed(15).is_app_chosen_exact());
        assert!(!Color::Ansi(1).is_app_chosen_exact());
    }

    #[test]
    fn frame_flags_convert_every_underline_kind() {
        let f = CellFlags::from_vte(Flags::DOUBLE_UNDERLINE | Flags::BOLD);
        assert!(f.contains(CellFlags::UNDERLINE));
        assert!(f.contains(CellFlags::BOLD));
        assert!(!f.contains(CellFlags::UNDERCURL));
        let f = CellFlags::from_vte(Flags::UNDERCURL);
        assert!(f.contains(CellFlags::UNDERLINE.union(CellFlags::UNDERCURL)));
        let all = CellFlags(0x0fff);
        assert_eq!(CellFlags::from_vte(all.to_vte()), all);
    }

    #[test]
    fn frame_row_hash_changes_with_content_and_style() {
        let base = FrameBuilder::new(2, 4).text(0, 0, "abcd").build();
        let same = FrameBuilder::new(2, 4).text(0, 0, "abcd").build();
        assert_eq!(base.row(0).hash(), same.row(0).hash());
        assert_ne!(base.row(0).hash(), base.row(1).hash());
        let styled = FrameBuilder::new(2, 4)
            .text(0, 0, "abcd")
            .styled(
                0,
                1,
                'b',
                Color::Ansi(1),
                Color::Background,
                CellFlags::BOLD,
            )
            .build();
        assert_ne!(base.row(0).hash(), styled.row(0).hash());
        let linked = FrameBuilder::new(2, 4)
            .text(0, 0, "abcd")
            .hyperlink(0, 0, "https://x.test")
            .build();
        assert_ne!(base.row(0).hash(), linked.row(0).hash());
        assert!(linked.row(0).cell(0).hyperlink.is_some());
        assert_eq!(
            linked.row(0).hyperlink_uri(0).as_deref(),
            Some("https://x.test")
        );
    }

    #[test]
    fn frame_selection_converts_to_display_rows() {
        let frame = FrameBuilder::new(5, 10)
            .display_offset(2)
            .selection(
                GridPoint { row: -1, col: 3 },
                GridPoint { row: 1, col: 4 },
                false,
            )
            .cursor(4, 2, CursorShape::Beam)
            .build();
        let sel = frame.selection().expect("selection");
        assert_eq!(sel.start, GridPoint { row: 1, col: 3 });
        assert_eq!(sel.end, GridPoint { row: 3, col: 4 });
        assert!(!sel.block);
        let cursor = frame.cursor();
        assert_eq!(
            (cursor.row, cursor.col, cursor.shape),
            (4, 2, CursorShape::Beam)
        );
        assert_eq!(frame.display_offset(), 2);
        assert!(!frame.app_cursor());
    }

    #[test]
    fn frame_text_into_skips_spacers_and_marks_wide_columns() {
        let frame = FrameBuilder::new(1, 6)
            .text(0, 0, "a")
            .wide(0, 1, '日')
            .text(0, 3, "b")
            .zerowidth(0, 3, &['\u{301}'])
            .build();
        let row = frame.row(0);
        let mut text = String::new();
        let mut cols = Vec::new();
        let mut wide = Vec::new();
        row.text_into(&mut text, &mut cols, &mut wide);
        assert_eq!(text, "a日b  ");
        assert_eq!(cols, vec![0, 1, 3, 4, 5]);
        assert_eq!(wide, vec![false, true, false, false, false]);
        assert_eq!(row.cell(3).zerowidth, &['\u{301}']);
        assert!(row.cell(2).is_spacer());
        assert_eq!(row.cell(4).ch, ' ');
        assert!(!row.wraps());
        assert_eq!(row.len(), 6);
    }

    #[test]
    fn frame_damage_and_wrap_flags_pass_through() {
        let frame = FrameBuilder::new(2, 3)
            .damage_rows(&[1])
            .flags(0, 2, CellFlags::WRAPLINE)
            .build();
        assert!(matches!(frame.damage(), Damage::Rows([1])));
        assert!(frame.row(0).wraps());
        assert!(!frame.row(1).wraps());
        let full = FrameBuilder::new(1, 1).build();
        assert!(matches!(full.damage(), Damage::Full));
        assert_eq!(full.size(), GridSize { rows: 1, cols: 1 });
    }
}
