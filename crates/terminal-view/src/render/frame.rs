//! The per-frame terminal snapshot in view-owned types.
//!
//! This is the only file under `render/` that names the engine's own types: it
//! wraps `TerminalContent` — which owns this view's `RenderState` and therefore
//! its damage watermark — and exposes cells, cursor, selection and the changed
//! rows through `Cell`, `Color`, `CellFlags`, `CursorShape` and `Selection`, so
//! the rest of the engine never depends on the grid implementation (HLD idea 3).
//!
//! Since `US-0085` there is no copy in between. A `FrameRow` borrows the
//! engine's `RenderRow` and a `Cell` is read out of it on demand, with the style
//! taken from the row's **runs of resolved values** rather than from a per-cell
//! record: at 120 columns that is about three style conversions per row instead
//! of a hundred and twenty, and the dense `IndexedCell` vector the view used to
//! read is gone.

use std::sync::Arc;

use oneterm_terminal::{
    Attrs, CellWidth, Color as EngineColor, CursorShape as EngineCursorShape, GraphicData,
    GraphicId, HyperlinkId, NamedColor, RenderContent, RenderRow, RenderUpdate, RowId, SeqNo,
    Style, TerminalContent, TerminalSession,
};

/// FNV-1a, the hash used for shaped-run keys and the style key.
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

/// What identifies a row's content for the plan cache: which row it is, and the
/// engine batch that last changed it.
///
/// This replaces the per-row content hash the cache used to compute every frame.
/// The engine stamps a row only when it actually changes and copies a row into
/// the render state only when the stamp passed this consumer's watermark, so the
/// pair answers "does this plan still describe this row?" exactly, for free.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct RowKey {
    pub id: RowId,
    pub seq: SeqNo,
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

    /// Matched member by member: nothing here may do arithmetic on the
    /// discriminants, which is what the deleted `NamedColor::DimBlack` subtraction
    /// did (`migration.md` § "Deletion list").
    fn from_engine(color: EngineColor) -> Self {
        match color {
            EngineColor::Rgb(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
            EngineColor::Palette(n) => Color::Indexed(n),
            EngineColor::Named(named) => match named {
                NamedColor::Foreground => Color::Foreground,
                NamedColor::Background => Color::Background,
                NamedColor::Cursor => Color::Cursor,
                NamedColor::BrightForeground => Color::BrightForeground,
                NamedColor::DimForeground => Color::DimForeground,
                NamedColor::DimBlack => Color::DimAnsi(0),
                NamedColor::DimRed => Color::DimAnsi(1),
                NamedColor::DimGreen => Color::DimAnsi(2),
                NamedColor::DimYellow => Color::DimAnsi(3),
                NamedColor::DimBlue => Color::DimAnsi(4),
                NamedColor::DimMagenta => Color::DimAnsi(5),
                NamedColor::DimCyan => Color::DimAnsi(6),
                NamedColor::DimWhite => Color::DimAnsi(7),
                NamedColor::Black => Color::Ansi(0),
                NamedColor::Red => Color::Ansi(1),
                NamedColor::Green => Color::Ansi(2),
                NamedColor::Yellow => Color::Ansi(3),
                NamedColor::Blue => Color::Ansi(4),
                NamedColor::Magenta => Color::Ansi(5),
                NamedColor::Cyan => Color::Ansi(6),
                NamedColor::White => Color::Ansi(7),
                NamedColor::BrightBlack => Color::Ansi(8),
                NamedColor::BrightRed => Color::Ansi(9),
                NamedColor::BrightGreen => Color::Ansi(10),
                NamedColor::BrightYellow => Color::Ansi(11),
                NamedColor::BrightBlue => Color::Ansi(12),
                NamedColor::BrightMagenta => Color::Ansi(13),
                NamedColor::BrightCyan => Color::Ansi(14),
                NamedColor::BrightWhite => Color::Ansi(15),
            },
        }
    }

    /// The engine color this value came from; used by the theme to build its
    /// lookup table through `oneterm_terminal::resolve_color`.
    pub(crate) fn to_engine(self) -> EngineColor {
        match self {
            Color::Foreground => EngineColor::Named(NamedColor::Foreground),
            Color::Background => EngineColor::Named(NamedColor::Background),
            Color::Cursor => EngineColor::Named(NamedColor::Cursor),
            Color::BrightForeground => EngineColor::Named(NamedColor::BrightForeground),
            Color::DimForeground => EngineColor::Named(NamedColor::DimForeground),
            Color::Ansi(n) => EngineColor::Named(ANSI_NAMED[usize::from(n.min(15))]),
            Color::DimAnsi(n) => EngineColor::Named(DIM_NAMED[usize::from(n.min(7))]),
            Color::Indexed(n) => EngineColor::Palette(n),
            Color::Rgb(r, g, b) => EngineColor::Rgb(oneterm_terminal::Rgb { r, g, b }),
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

    /// The SGR half, from the style run. Width and wrap are per cell and per row
    /// and are added by [`FrameRow::cell`].
    fn from_attrs(attrs: Attrs) -> Self {
        let mut out = 0u16;
        let pairs = [
            (Attrs::INVERSE, Self::INVERSE),
            (Attrs::BOLD, Self::BOLD),
            (Attrs::ITALIC, Self::ITALIC),
            (Attrs::DIM, Self::DIM),
            (Attrs::HIDDEN, Self::HIDDEN),
            (Attrs::ALL_UNDERLINES, Self::UNDERLINE),
            (Attrs::UNDERCURL, Self::UNDERCURL),
            (Attrs::STRIKEOUT, Self::STRIKEOUT),
        ];
        for (engine, ours) in pairs {
            if attrs.intersects(engine) {
                out |= ours.0;
            }
        }
        Self(out)
    }

    /// The SGR half back, so a test can state a cell in the view's own words.
    #[cfg(test)]
    pub(crate) fn to_attrs(self) -> Attrs {
        let pairs = [
            (Attrs::INVERSE, Self::INVERSE),
            (Attrs::BOLD, Self::BOLD),
            (Attrs::ITALIC, Self::ITALIC),
            (Attrs::DIM, Self::DIM),
            (Attrs::HIDDEN, Self::HIDDEN),
            (Attrs::UNDERLINE, Self::UNDERLINE),
            (Attrs::UNDERCURL, Self::UNDERCURL),
            (Attrs::STRIKEOUT, Self::STRIKEOUT),
        ];
        let mut out = Attrs::empty();
        for (engine, ours) in pairs {
            if self.contains(ours) {
                out |= engine;
            }
        }
        out
    }

    /// The width half, so a test can state a wide pair.
    #[cfg(test)]
    pub(crate) fn to_width(self) -> CellWidth {
        if self.contains(Self::WIDE_CHAR) {
            CellWidth::Wide
        } else if self.contains(Self::WIDE_CHAR_SPACER) {
            CellWidth::WideSpacer
        } else if self.contains(Self::LEADING_WIDE_CHAR_SPACER) {
            CellWidth::LeadingWideSpacer
        } else {
            CellWidth::Narrow
        }
    }

    fn from_width(width: CellWidth) -> Self {
        match width {
            CellWidth::Narrow => Self(0),
            CellWidth::Wide => Self::WIDE_CHAR,
            CellWidth::WideSpacer => Self::WIDE_CHAR_SPACER,
            CellWidth::LeadingWideSpacer => Self::LEADING_WIDE_CHAR_SPACER,
        }
    }
}

/// One grid cell, read out of the frame's row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cell<'a> {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: CellFlags,
    /// Combining marks / ZWJ sequences attached to `ch`.
    pub zerowidth: &'a [char],
    /// The OSC 8 link this cell belongs to. Identity only: one run of cells
    /// shares one id, and the target comes from [`Frame::hyperlink_uri`].
    pub hyperlink: Option<HyperlinkId>,
    /// Image painted over this cell, if any.
    pub graphic: Option<GraphicId>,
}

impl Cell<'_> {
    /// Whether this column holds glyph-less padding of a neighbouring wide char.
    pub(crate) fn is_spacer(&self) -> bool {
        self.flags
            .intersects(CellFlags::WIDE_CHAR_SPACER.union(CellFlags::LEADING_WIDE_CHAR_SPACER))
    }
}

/// One display row of the frame.
#[derive(Clone, Copy)]
pub(crate) struct FrameRow<'a> {
    row: &'a RenderRow,
    index: usize,
}

impl<'a> FrameRow<'a> {
    pub(crate) fn index(&self) -> usize {
        self.index
    }

    pub(crate) fn len(&self) -> usize {
        self.row.cells.len()
    }

    pub(crate) fn id(&self) -> RowId {
        self.row.id
    }

    pub(crate) fn cell(&self, col: usize) -> Cell<'a> {
        let cell = &self.row.cells[col];
        let style: &Style = self.row.style_of(cell);
        let last = self.row.cells.len().saturating_sub(1);
        let mut flags = CellFlags::from_attrs(style.attrs).union(CellFlags::from_width(cell.width));
        if self.row.wrapped && col == last {
            flags = flags.union(CellFlags::WRAPLINE);
        }
        let (ch, zerowidth) = match cell.content {
            RenderContent::Scalar(scalar) => (scalar, &[][..]),
            RenderContent::Cluster { start, len } => {
                let cluster = self.row.cluster(start, len);
                match cluster.split_first() {
                    Some((base, rest)) => (*base, rest),
                    None => (' ', &[][..]),
                }
            }
        };
        Cell {
            ch,
            fg: Color::from_engine(style.fg),
            bg: Color::from_engine(style.bg),
            flags,
            zerowidth,
            hyperlink: cell.hyperlink,
            graphic: cell.graphic,
        }
    }

    pub(crate) fn cells(&self) -> impl Iterator<Item = Cell<'a>> + '_ {
        (0..self.row.cells.len()).map(|col| self.cell(col))
    }

    /// The logical line continues on the next row.
    pub(crate) fn wraps(&self) -> bool {
        self.row.wrapped
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
    fn from_engine(shape: EngineCursorShape) -> Self {
        match shape {
            EngineCursorShape::Block => CursorShape::Block,
            EngineCursorShape::Underline => CursorShape::Underline,
            EngineCursorShape::Beam => CursorShape::Beam,
            EngineCursorShape::HollowBlock => CursorShape::HollowBlock,
            EngineCursorShape::Hidden => CursorShape::Hidden,
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

    /// The frame's only `snapshot_into`: it advances this frame's damage
    /// watermark, so it must run exactly once per painted frame.
    pub(crate) fn snapshot(&mut self, session: &dyn TerminalSession) {
        session.snapshot_into(&mut self.content);
        self.size = size_of(&self.content);
    }

    pub(crate) fn size(&self) -> GridSize {
        self.size
    }

    /// What the last snapshot did: nothing, a shift plus [`changed`](Self::changed),
    /// or everything.
    pub(crate) fn update(&self) -> RenderUpdate {
        self.content.update()
    }

    pub(crate) fn display_offset(&self) -> usize {
        self.content.scroll_offset() as usize
    }

    /// Images the engine decoded since the previous snapshot (each appears once).
    pub(crate) fn graphics(&self) -> &[Arc<GraphicData>] {
        self.content.graphics()
    }

    /// The cell's `(col, row)` offset inside the image's own cell grid, which is
    /// what the painter anchors the image by.
    pub(crate) fn graphic_offset(&self, id: GraphicId, row: RowId, col: u16) -> Option<(u16, u16)> {
        self.content.graphic_offset(id, row, col)
    }

    /// The OSC 8 target behind a cell's link id.
    ///
    /// The painted row needs only the id — one run, one underline — and the
    /// click path reads its target off the damage-free line-range window, so
    /// this exists to pin the resolution, not to serve a painter.
    #[cfg(test)]
    pub(crate) fn hyperlink_uri(&self, id: HyperlinkId) -> Option<&str> {
        self.content.hyperlink(id).map(|link| link.uri.as_ref())
    }

    /// Display row `r`. The render state always holds the full viewport, so a
    /// row index below `size().rows` always resolves.
    pub(crate) fn row(&self, r: usize) -> FrameRow<'_> {
        FrameRow {
            row: &self.content.rows()[r],
            index: r,
        }
    }

    /// The identity of display row `r`, for the plan cache.
    pub(crate) fn row_key(&self, r: usize) -> Option<RowKey> {
        self.content.rows().get(r).map(|row| RowKey {
            id: row.id,
            seq: row.seq,
        })
    }

    pub(crate) fn cursor(&self) -> Cursor {
        let cursor = self.content.render_cursor();
        let shape = if cursor.visible {
            CursorShape::from_engine(self.content.cursor_shape())
        } else {
            CursorShape::Hidden
        };
        Cursor {
            // Outside the viewport (scrolled back past it) reads as a row the
            // painter rejects, which is what the reference's negative line did.
            row: cursor.row.map_or(-1, i32::from),
            col: cursor.col,
            shape,
        }
    }

    pub(crate) fn selection(&self) -> Option<Selection> {
        let top = self.content.viewport_top();
        let display_row = |row: RowId| -> i32 {
            (row.0 as i64 - top.0 as i64).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
        };
        self.content.selection_range().map(|range| Selection {
            start: GridPoint {
                row: display_row(range.start.row),
                col: range.start.col,
            },
            end: GridPoint {
                row: display_row(range.end.row),
                col: range.end.col,
            },
            block: range.is_block,
        })
    }

    /// DECCKM: arrows send `ESC O x` instead of `ESC [ x`.
    pub(crate) fn app_cursor(&self) -> bool {
        self.content.modes().app_cursor
    }
}

fn size_of(content: &TerminalContent) -> GridSize {
    let size = content.size();
    GridSize {
        rows: size.rows,
        cols: size.cols,
    }
}

/// Test-only frame construction from view-owned types, so tests of the modules
/// above this one never touch the engine's types themselves.
///
/// `US-0085` rebuilt it on a **real** engine: `RenderState` has no public
/// constructor for rows, and the cells the tests want — a wide pair on an ASCII
/// char, a wrap flag without a wrap — cannot be stated as an escape-sequence
/// stream. `oneterm_terminal::test_support::GridFixture` writes them into a real
/// grid instead, and the frame comes back through `render_update` like any
/// other. The builder's own API is unchanged apart from `damage_rows`, which had
/// no meaning left once damage became the engine's per-row sequence number.
#[cfg(test)]
pub(crate) mod test_support {
    use oneterm_terminal::TerminalContent;
    use oneterm_terminal::test_support::{FixtureCell, GridFixture};
    use oneterm_terminal::{SelectionKind, Style};

    use super::{CellFlags, Color, CursorShape, Frame, GridPoint, size_of};

    /// One instruction, applied at `build()` in a fixed order so the order the
    /// test wrote them in never matters.
    enum Step {
        Cell {
            row: usize,
            col: usize,
            cell: FixtureCell,
        },
        Wrapped(usize),
        Cursor(i32, usize, CursorShape),
        Selection(GridPoint, GridPoint, bool),
    }

    pub(crate) struct FrameBuilder {
        steps: Vec<Step>,
        rows: usize,
        cols: usize,
        display_offset: usize,
    }

    impl FrameBuilder {
        pub(crate) fn new(rows: usize, cols: usize) -> Self {
            Self {
                steps: Vec::new(),
                rows,
                cols,
                display_offset: 0,
            }
        }

        fn cell_mut(&mut self, row: usize, col: usize) -> &mut FixtureCell {
            assert!(row < self.rows && col < self.cols, "cell out of range");
            if let Some(index) = self.steps.iter().position(
                |step| matches!(step, Step::Cell { row: r, col: c, .. } if *r == row && *c == col),
            ) {
                let Step::Cell { cell, .. } = &mut self.steps[index] else {
                    unreachable!("matched above")
                };
                return cell;
            }
            self.steps.push(Step::Cell {
                row,
                col,
                cell: FixtureCell::default(),
            });
            let Some(Step::Cell { cell, .. }) = self.steps.last_mut() else {
                unreachable!("just pushed")
            };
            cell
        }

        /// Write `text` from `(row, col)`.
        pub(crate) fn text(mut self, row: usize, col: usize, text: &str) -> Self {
            for (c, ch) in (col..).zip(text.chars()) {
                self.cell_mut(row, c).ch = ch;
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
            cell.ch = ch;
            cell.style = Style {
                fg: fg.to_engine(),
                bg: bg.to_engine(),
                underline_color: None,
                attrs: flags.to_attrs(),
            };
            cell.width = flags.to_width();
            self
        }

        pub(crate) fn flags(mut self, row: usize, col: usize, flags: CellFlags) -> Self {
            {
                let cell = self.cell_mut(row, col);
                cell.style.attrs = flags.to_attrs();
                cell.width = flags.to_width();
            }
            if flags.contains(CellFlags::WRAPLINE) {
                self.steps.push(Step::Wrapped(row));
            }
            self
        }

        /// A wide char at `col` plus its spacer at `col + 1`.
        pub(crate) fn wide(mut self, row: usize, col: usize, ch: char) -> Self {
            {
                let cell = self.cell_mut(row, col);
                cell.ch = ch;
                cell.width = oneterm_terminal::CellWidth::Wide;
            }
            let spacer = self.cell_mut(row, col + 1);
            spacer.ch = ' ';
            spacer.width = oneterm_terminal::CellWidth::WideSpacer;
            self
        }

        pub(crate) fn zerowidth(mut self, row: usize, col: usize, marks: &[char]) -> Self {
            self.cell_mut(row, col).zerowidth.extend_from_slice(marks);
            self
        }

        pub(crate) fn hyperlink(self, row: usize, col: usize, uri: &str) -> Self {
            self.hyperlink_run(row, col, 1, uri)
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
            // An explicit id keeps one run one link, as the stream's `id=` does.
            let id = format!("run{row}-{col}");
            for c in col..col + len {
                self.cell_mut(row, c).hyperlink = Some((id.clone(), uri.to_string()));
            }
            self
        }

        pub(crate) fn cursor(mut self, row: i32, col: usize, shape: CursorShape) -> Self {
            self.steps.push(Step::Cursor(row, col, shape));
            self
        }

        /// Selection in grid coordinates (display row = row + display_offset).
        pub(crate) fn selection(mut self, start: GridPoint, end: GridPoint, block: bool) -> Self {
            self.steps.push(Step::Selection(start, end, block));
            self
        }

        pub(crate) fn display_offset(mut self, offset: usize) -> Self {
            self.display_offset = offset;
            self
        }

        /// A frame plus the fixture behind it, so a test can change the terminal
        /// and snapshot again.
        pub(crate) fn build_with_fixture(self) -> (Frame, GridFixture) {
            let mut fixture = GridFixture::new(self.rows, self.cols);
            // History first: it moves the screen top the cells are addressed
            // against, and it is what a later scroll needs to have somewhere to
            // go.
            if self.display_offset > 0 {
                fixture.grow_history(self.display_offset);
            }
            fixture.begin_batch();
            for step in &self.steps {
                if let Step::Cell { row, col, cell } = step {
                    fixture.write(*row, *col, cell);
                }
            }
            for step in &self.steps {
                match step {
                    Step::Wrapped(row) => fixture.set_wrapped(*row, true),
                    Step::Cursor(row, col, shape) => {
                        let row = (row - self.display_offset as i32).max(0) as usize;
                        fixture.cursor(
                            row.min(self.rows.saturating_sub(1)),
                            *col,
                            engine_shape(*shape),
                        );
                    }
                    Step::Selection(start, end, block) => {
                        let kind = if *block {
                            SelectionKind::Block
                        } else {
                            SelectionKind::Simple
                        };
                        let at =
                            |point: &GridPoint| (point.row.max(0) as usize, usize::from(point.col));
                        fixture.select(at(start), at(end), kind);
                    }
                    Step::Cell { .. } => {}
                }
            }
            if self.display_offset > 0 {
                fixture.scroll_back(self.display_offset);
            }
            let mut content = TerminalContent::default();
            fixture.refill(&mut content);
            let size = size_of(&content);
            (Frame { content, size }, fixture)
        }

        pub(crate) fn build(self) -> Frame {
            self.build_with_fixture().0
        }

        /// The same grid as a damage-free line-range read — the shape the URL
        /// click path and the completion lookup consume.
        pub(crate) fn into_line_range_cells(self) -> oneterm_terminal::LineRangeCells {
            let rows = self.rows;
            let (_frame, fixture) = self.build_with_fixture();
            fixture.line_range_cells(0, rows)
        }
    }

    fn engine_shape(shape: CursorShape) -> oneterm_terminal::CursorShape {
        match shape {
            CursorShape::Block => oneterm_terminal::CursorShape::Block,
            CursorShape::Beam => oneterm_terminal::CursorShape::Beam,
            CursorShape::Underline => oneterm_terminal::CursorShape::Underline,
            CursorShape::HollowBlock => oneterm_terminal::CursorShape::HollowBlock,
            CursorShape::Hidden => oneterm_terminal::CursorShape::Hidden,
        }
    }

    /// Refill `frame` from `fixture` — a second frame against the same terminal,
    /// which is how a test states "only this changed".
    pub(crate) fn resnapshot(frame: &mut Frame, fixture: &mut GridFixture) {
        fixture.refill(&mut frame.content);
        frame.size = size_of(&frame.content);
    }

    /// Overwrite one row's text and snapshot again.
    pub(crate) fn rewrite_row(
        frame: &mut Frame,
        fixture: &mut GridFixture,
        row: usize,
        text: &str,
    ) {
        fixture.begin_batch();
        for (col, ch) in text.chars().enumerate() {
            fixture.write(
                row,
                col,
                &FixtureCell {
                    ch,
                    ..FixtureCell::default()
                },
            );
        }
        resnapshot(frame, fixture);
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
            assert_eq!(Color::from_engine(c.to_engine()), c, "{c:?}");
        }
        assert!(Color::Rgb(0, 0, 0).is_app_chosen_exact());
        assert!(Color::Indexed(16).is_app_chosen_exact());
        assert!(!Color::Indexed(15).is_app_chosen_exact());
        assert!(!Color::Ansi(1).is_app_chosen_exact());
    }

    #[test]
    fn frame_flags_convert_every_underline_kind() {
        let f = CellFlags::from_attrs(Attrs::DOUBLE_UNDERLINE | Attrs::BOLD);
        assert!(f.contains(CellFlags::UNDERLINE));
        assert!(f.contains(CellFlags::BOLD));
        assert!(!f.contains(CellFlags::UNDERCURL));
        let f = CellFlags::from_attrs(Attrs::UNDERCURL);
        assert!(f.contains(CellFlags::UNDERLINE.union(CellFlags::UNDERCURL)));
        // The SGR half round-trips; width and wrap are not SGR.
        let sgr = CellFlags(0x00ff);
        assert_eq!(CellFlags::from_attrs(sgr.to_attrs()), sgr);
    }

    /// The plan cache's key: it changes when the row's content changes and not
    /// otherwise, which is the whole reason the per-row hash could go.
    #[test]
    fn row_keys_follow_content_not_frames() {
        let (mut frame, mut fixture) = FrameBuilder::new(2, 6)
            .text(0, 0, "abcd")
            .text(1, 0, "efgh")
            .build_with_fixture();
        let first = [frame.row_key(0).unwrap(), frame.row_key(1).unwrap()];

        // A frame with no new output keeps every key.
        super::test_support::resnapshot(&mut frame, &mut fixture);
        assert_eq!(frame.update(), RenderUpdate::Unchanged);
        assert_eq!(frame.row_key(0).unwrap(), first[0]);
        assert_eq!(frame.row_key(1).unwrap(), first[1]);

        // Changing row 1 moves its key and leaves row 0's alone.
        super::test_support::rewrite_row(&mut frame, &mut fixture, 1, "ZZZZ");
        assert_eq!(frame.update(), RenderUpdate::Partial { scrolled: 0 });
        assert_eq!(frame.row_key(0).unwrap(), first[0]);
        assert_ne!(frame.row_key(1).unwrap(), first[1]);
        assert_eq!(frame.row(1).cell(0).ch, 'Z');
    }

    #[test]
    fn frame_styles_come_from_the_rows_runs() {
        let frame = FrameBuilder::new(2, 4)
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
        let row = frame.row(0);
        assert_eq!(row.cell(0).fg, Color::Foreground);
        assert!(!row.cell(0).flags.contains(CellFlags::BOLD));
        assert_eq!(row.cell(1).fg, Color::Ansi(1));
        assert!(row.cell(1).flags.contains(CellFlags::BOLD));
        assert_eq!(row.len(), 4);
        assert_eq!(row.index(), 0);
    }

    #[test]
    fn frame_hyperlinks_are_one_id_per_run() {
        let frame = FrameBuilder::new(2, 8)
            .text(0, 0, "abcd")
            .hyperlink_run(0, 0, 2, "https://x.test")
            .build();
        let row = frame.row(0);
        let id = row.cell(0).hyperlink.expect("a link");
        assert_eq!(row.cell(1).hyperlink, Some(id), "one run, one id");
        assert!(row.cell(2).hyperlink.is_none());
        assert_eq!(frame.hyperlink_uri(id), Some("https://x.test"));
    }

    #[test]
    fn frame_selection_and_cursor_are_display_rows() {
        let frame = FrameBuilder::new(5, 10)
            .text(0, 0, "one")
            .text(1, 0, "two")
            .text(2, 0, "three")
            .selection(
                GridPoint { row: 1, col: 3 },
                GridPoint { row: 2, col: 4 },
                false,
            )
            .cursor(4, 2, CursorShape::Beam)
            .build();
        let sel = frame.selection().expect("selection");
        assert_eq!(sel.start, GridPoint { row: 1, col: 3 });
        assert_eq!(sel.end, GridPoint { row: 2, col: 4 });
        assert!(!sel.block);
        let cursor = frame.cursor();
        assert_eq!(
            (cursor.row, cursor.col, cursor.shape),
            (4, 2, CursorShape::Beam)
        );
        assert_eq!(frame.display_offset(), 0);
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
    fn frame_wrap_flag_reaches_the_last_cell() {
        let frame = FrameBuilder::new(2, 3)
            .text(0, 0, "abc")
            .flags(0, 2, CellFlags::WRAPLINE)
            .build();
        assert!(frame.row(0).wraps());
        assert!(frame.row(0).cell(2).flags.contains(CellFlags::WRAPLINE));
        assert!(!frame.row(0).cell(1).flags.contains(CellFlags::WRAPLINE));
        assert!(!frame.row(1).wraps());
        let full = FrameBuilder::new(1, 1).build();
        assert_eq!(full.size(), GridSize { rows: 1, cols: 1 });
        assert_eq!(full.update(), RenderUpdate::Full);
    }
}
