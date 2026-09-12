//! One row: a header the renderer reads and a vector of cells.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md`
//! section "Row", and `damage-and-render-state.md` for the two header fields
//! (`seq`, `RowFlags::DIRTY`) the render state reads.
//!
//! **One row representation (R-51).** The dual-form (uniform run plus general
//! vector) design is deferred: every operation that matters coerces to the
//! general form anyway, and the memory win the design claims comes from the
//! ring's lazy `Option<Row>` slots, which is independent. [`RowRef`] and
//! [`RowMut`] exist so that change stays internal.

use bitflags::bitflags;

use crate::cell::{Cell, CellContent, CellWidth};
use crate::grid::{MAX_COLS, RowId};
use crate::intern::{ExtrasId, StyleId};

/// A monotonic, engine-wide batch stamp. One bump per `feed`, not per mutation,
/// so a burst that rewrites a row twenty times costs one stamp comparison.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SeqNo(pub u64);

impl SeqNo {
    pub fn next(self) -> SeqNo {
        SeqNo(self.0.saturating_add(1))
    }
}

bitflags! {
    /// Ghostty's "false positives allowed, never false negatives" rule: the
    /// content hints are set on write and cleared only on reset, so a sweep or a
    /// render pass can skip a row with one load.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct RowFlags: u8 {
        /// The logical line continues on the next row. A row flag rather than a
        /// flag on the last cell (deviation G1).
        const WRAPPED      = 1 << 0;
        const DIRTY        = 1 << 1;
        const STYLED       = 1 << 2;
        const HAS_GRAPHEME = 1 << 3;
        const HAS_EXTRAS   = 1 << 4;
        const HAS_GRAPHIC  = 1 << 5;

        /// What a reset keeps: everything else describes content that is gone.
        const CONTENT_HINTS = Self::WRAPPED.bits()
            | Self::STYLED.bits()
            | Self::HAS_GRAPHEME.bits()
            | Self::HAS_EXTRAS.bits()
            | Self::HAS_GRAPHIC.bits();
    }
}

/// The fixed-size half of a row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RowHeader {
    /// The batch that last mutated this row.
    pub seq: SeqNo,
    pub id: RowId,
    pub flags: RowFlags,
    /// The reference's over-approximating hint: no column at or above `occ` has
    /// been touched since the last reset. Not exact, not part of any equality,
    /// and deliberately **not** checked by `assert_integrity` (R-10).
    pub occ: u16,
}

/// One row, always in the general form.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    header: RowHeader,
    cells: Vec<Cell>,
}

/// Read-only backing for a slot that was never written. A `None` slot reads as
/// blanks, so nothing has to allocate to answer a question about an empty row.
static BLANK_CELLS: [Cell; MAX_COLS as usize] = [Cell::EMPTY; MAX_COLS as usize];

impl Row {
    pub fn new(id: RowId, cols: u16, seq: SeqNo, template: Cell) -> Row {
        Row {
            header: RowHeader {
                seq,
                id,
                flags: RowFlags::DIRTY | flags_for(template),
                // Nothing has been written since this row was created, which is
                // what `occ` measures.
                occ: 0,
            },
            cells: vec![template; cols as usize],
        }
    }

    /// Build a row the reflow has just laid out (`US-0077`).
    ///
    /// `extra` carries the content hints the cells cannot imply — only
    /// `HAS_GRAPHIC`, which needs the interner to derive and which the caller
    /// therefore carries over from the logical line's source rows. The id is
    /// assigned when the row is placed in the ring.
    pub(crate) fn from_cells(cells: Vec<Cell>, wrapped: bool, extra: RowFlags, seq: SeqNo) -> Row {
        let mut flags = RowFlags::DIRTY | extra;
        flags.set(RowFlags::WRAPPED, wrapped);
        for cell in &cells {
            flags.insert(flags_for(*cell));
        }
        Row {
            header: RowHeader {
                seq,
                id: RowId::default(),
                flags,
                // Over-approximating, which is all `occ` promises: the row was
                // laid out cell by cell, so nothing above its width was touched.
                occ: cells.len() as u16,
            },
            cells,
        }
    }

    pub fn header(&self) -> &RowHeader {
        &self.header
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Clear the row to `template`, reusing the allocation.
    ///
    /// Trap 36: only `0..occ` is cleared when the last cell already carries the
    /// template's style, and the whole row otherwise. The reference compares the
    /// background alone; comparing the interned style id instead is an
    /// over-approximation in the safe direction — it can only clear more than
    /// strictly necessary, never less — and it keeps the erase paths off the
    /// interner.
    pub fn reset(&mut self, template: Cell, seq: SeqNo) {
        if self.cells.last().map(|cell| cell.style_id()) != Some(template.style_id()) {
            self.header.occ = self.cells.len() as u16;
        }
        let occ = self.header.occ as usize;
        self.cells[..occ].fill(template);
        self.header.occ = 0;
        self.header.seq = seq;
        self.header.flags = RowFlags::DIRTY | flags_for(template);
    }

    /// Re-home a row that was moved to another position by a scroll.
    fn rehome(&mut self, id: RowId, seq: SeqNo) {
        self.header.id = id;
        self.header.seq = seq;
        self.header.flags.insert(RowFlags::DIRTY);
    }

    /// Grow or shrink to `cols`, padding with `template`.
    fn resize(&mut self, cols: u16, template: Cell, seq: SeqNo) {
        self.cells.resize(cols as usize, template);
        self.header.occ = self.header.occ.min(cols);
        self.header.seq = seq;
        self.header.flags.insert(RowFlags::DIRTY);
        repair_wide_pairs(&mut self.cells);
    }

    /// Live heap this row owns, for the memory probe.
    fn heap_bytes(&self) -> usize {
        self.cells.capacity() * size_of::<Cell>()
    }
}

/// The content hints a cell implies. Never cleared by a write, only by a reset.
fn flags_for(cell: Cell) -> RowFlags {
    let mut flags = RowFlags::empty();
    if matches!(cell.content(), CellContent::Grapheme(_)) {
        flags.insert(RowFlags::HAS_GRAPHEME);
    }
    if cell.style_id() != StyleId::DEFAULT {
        flags.insert(RowFlags::STYLED);
    }
    if cell.extras_id() != ExtrasId::NONE {
        flags.insert(RowFlags::HAS_EXTRAS);
    }
    flags
}

/// Make every wide pair in `cells` well formed again after a shift, an erase or
/// a width change split one.
///
/// A `Wide` whose spacer is gone, and a `WideSpacer` whose glyph is gone, both
/// degrade to a default-width blank that keeps the cell's style — the
/// reference's `clear_wide`. A `LeadingWideSpacer` is left alone: it is a
/// legitimate lone spacer marking a glyph that wrapped to the next row.
pub fn repair_wide_pairs(cells: &mut [Cell]) {
    for col in 0..cells.len() {
        match cells[col].width() {
            CellWidth::Wide => {
                let paired = cells
                    .get(col + 1)
                    .is_some_and(|next| next.width() == CellWidth::WideSpacer);
                if !paired {
                    cells[col] = blank_out(cells[col]);
                }
            }
            CellWidth::WideSpacer => {
                let paired = col > 0 && cells[col - 1].width() == CellWidth::Wide;
                if !paired {
                    cells[col] = blank_out(cells[col]);
                }
            }
            // Only the last column can hold the place of a glyph that wrapped,
            // so a shift or a width change that moved one inland releases it.
            CellWidth::LeadingWideSpacer if col + 1 != cells.len() => {
                cells[col] = blank_out(cells[col]);
            }
            CellWidth::Narrow | CellWidth::LeadingWideSpacer => {}
        }
    }
}

fn blank_out(cell: Cell) -> Cell {
    cell.with_width(CellWidth::Narrow)
        .with_content(CellContent::Scalar(' '))
}

/// A row as the caller sees it. An unwritten slot reads as blanks.
#[derive(Clone, Copy, Debug)]
pub struct RowRef<'a> {
    id: RowId,
    header: Option<&'a RowHeader>,
    cells: &'a [Cell],
}

impl<'a> RowRef<'a> {
    pub(crate) fn present(row: &'a Row) -> RowRef<'a> {
        RowRef {
            id: row.header.id,
            header: Some(&row.header),
            cells: &row.cells,
        }
    }

    pub(crate) fn blank(id: RowId, cols: u16) -> RowRef<'a> {
        RowRef {
            id,
            header: None,
            cells: &BLANK_CELLS[..cols as usize],
        }
    }

    pub fn id(&self) -> RowId {
        self.id
    }

    /// `SeqNo::default()` for a row that was never written, which is below every
    /// watermark a consumer can hold.
    pub fn seq(&self) -> SeqNo {
        self.header.map_or(SeqNo::default(), |header| header.seq)
    }

    pub fn flags(&self) -> RowFlags {
        self.header.map_or(RowFlags::empty(), |header| header.flags)
    }

    pub fn wrapped(&self) -> bool {
        self.flags().contains(RowFlags::WRAPPED)
    }

    pub fn occ(&self) -> u16 {
        self.header.map_or(0, |header| header.occ)
    }

    pub fn cells(&self) -> &'a [Cell] {
        self.cells
    }

    /// Out of range reads as a blank rather than panicking: the column can come
    /// from a hit test on a viewport that has since been resized.
    pub fn cell(&self, col: u16) -> Cell {
        self.cells.get(col as usize).copied().unwrap_or(Cell::EMPTY)
    }

    /// Whether this row was ever written. The memory probe's logical half.
    pub fn is_allocated(&self) -> bool {
        self.header.is_some()
    }
}

/// A row opened for writing. The slot is allocated and the batch stamp applied
/// before the caller gets here.
#[derive(Debug)]
pub struct RowMut<'a> {
    row: &'a mut Row,
}

impl<'a> RowMut<'a> {
    pub(crate) fn new(row: &'a mut Row, seq: SeqNo) -> RowMut<'a> {
        row.header.seq = seq;
        row.header.flags.insert(RowFlags::DIRTY);
        RowMut { row }
    }

    pub fn id(&self) -> RowId {
        self.row.header.id
    }

    pub fn cells(&self) -> &[Cell] {
        &self.row.cells
    }

    /// Write one cell over whatever half of a wide pair was there.
    ///
    /// Trap 6, same-row half: this is the reference's `write_at_cursor` repair,
    /// run against the cell being overwritten before the new one lands.
    pub fn write_repairing(&mut self, col: u16, cell: Cell) {
        crate::cell::repair_wide_pair_in_row(&mut self.row.cells, col as usize);
        self.set(col, cell);
    }

    /// Write one cell, maintaining `occ` and the content hints.
    pub fn set(&mut self, col: u16, cell: Cell) {
        let Some(slot) = self.row.cells.get_mut(col as usize) else {
            return;
        };
        *slot = cell;
        self.row.header.occ = self.row.header.occ.max(col.saturating_add(1));
        self.row.header.flags.insert(flags_for(cell));
    }

    /// Fill `range` with `template`, repairing any wide pair the fill splits.
    ///
    /// The reference leaves an orphaned spacer here; the integrity assertion
    /// this design mandates ("no `Wide` without its `WideSpacer`") makes the
    /// repair compulsory.
    pub fn fill(&mut self, range: std::ops::Range<u16>, template: Cell) {
        let end = (range.end as usize).min(self.row.cells.len());
        let start = (range.start as usize).min(end);
        self.row.cells[start..end].fill(template);
        repair_wide_pairs(&mut self.row.cells);
        self.row.header.occ = self.row.header.occ.max(end as u16);
        self.row.header.flags.insert(flags_for(template));
    }

    /// `DCH`, correction C1: a plain shift left by `n`, not the reference's
    /// `end`-clamped swap.
    pub fn delete_cells(&mut self, col: u16, n: u16, template: Cell) {
        let cols = self.row.cells.len();
        let col = (col as usize).min(cols);
        let n = (n as usize).min(cols - col);
        if n == 0 {
            return;
        }
        self.row.cells.copy_within(col + n.., col);
        self.row.cells[cols - n..].fill(template);
        repair_wide_pairs(&mut self.row.cells);
        self.row.header.occ = cols as u16;
        self.row.header.flags.insert(flags_for(template));
    }

    /// `ICH`: shift right from `col` by `n`, filling the opened cells.
    pub fn insert_cells(&mut self, col: u16, n: u16, template: Cell) {
        let cols = self.row.cells.len();
        let col = (col as usize).min(cols);
        let n = (n as usize).min(cols - col);
        if n == 0 {
            return;
        }
        self.row.cells.copy_within(col..cols - n, col + n);
        self.row.cells[col..col + n].fill(template);
        repair_wide_pairs(&mut self.row.cells);
        self.row.header.occ = cols as u16;
        self.row.header.flags.insert(flags_for(template));
    }

    /// Insert mode's shift, which moves the whole row right by the glyph width.
    /// Correction C4: the pair the shift splits is repaired rather than left as
    /// an orphaned spacer.
    pub fn shift_right_from(&mut self, col: u16, n: u16) {
        let cols = self.row.cells.len();
        let col = (col as usize).min(cols);
        let n = (n as usize).min(cols - col);
        if n == 0 {
            return;
        }
        self.row.cells.copy_within(col..cols - n, col + n);
        repair_wide_pairs(&mut self.row.cells);
        self.row.header.occ = cols as u16;
    }

    pub fn set_wrapped(&mut self, wrapped: bool) {
        self.row.header.flags.set(RowFlags::WRAPPED, wrapped);
    }

    /// Declare that a graphic covers part of this row. The grid cannot derive it
    /// (the extras id only resolves through the interner), so the graphics
    /// packet stamps it explicitly.
    pub fn mark_graphic(&mut self) {
        self.row.header.flags.insert(RowFlags::HAS_GRAPHIC);
    }

    pub fn reset(&mut self, template: Cell, seq: SeqNo) {
        self.row.reset(template, seq);
    }

    pub(crate) fn resize(&mut self, cols: u16, template: Cell, seq: SeqNo) {
        self.row.resize(cols, template, seq);
    }
}

/// Ring-side helpers, kept out of the public surface.
impl Row {
    pub(crate) fn id(&self) -> RowId {
        self.header.id
    }

    pub(crate) fn take_rehomed(mut self, id: RowId, seq: SeqNo) -> Row {
        self.rehome(id, seq);
        self
    }

    pub(crate) fn bytes(&self) -> usize {
        self.heap_bytes()
    }
}
