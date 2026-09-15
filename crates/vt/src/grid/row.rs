//! One row: a header the renderer reads and a vector of cells.
//!
//! Design: <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md>
//!
//! A row has **one** representation, the general cell vector. The dual-form
//! alternative (a uniform run beside the general vector) is deferred: every
//! operation that matters coerces to the general form anyway, and the memory it
//! would save is already saved by leaving unwritten ring slots unallocated.
//! [`RowRef`] and [`RowMut`] exist so that change would stay internal.

use bitflags::bitflags;

use crate::cell::{Cell, CellContent, CellWidth};
use crate::grid::{MAX_COLS, RowId};
use crate::intern::{ExtrasId, StyleId};

/// A monotonic, engine-wide batch stamp. One bump per `feed`, not per mutation,
/// so a burst that rewrites a row twenty times costs one stamp comparison.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SeqNo(pub u64);

impl SeqNo {
    pub(crate) fn next(self) -> SeqNo {
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
        /// flag on the last cell.
        const WRAPPED      = 1 << 0;
        /// The row was written since it was last handed to a consumer.
        const DIRTY        = 1 << 1;
        /// At least one cell carries a non-default style.
        const STYLED       = 1 << 2;
        /// At least one cell holds a multi-scalar grapheme cluster.
        const HAS_GRAPHEME = 1 << 3;
        /// At least one cell carries out-of-line data such as a hyperlink.
        const HAS_EXTRAS   = 1 << 4;
        /// At least one cell is covered by an image placement.
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
    /// The row's stream-wide id.
    pub id: RowId,
    /// The row's damage bit and content hints.
    pub flags: RowFlags,
    /// An over-approximating hint: no column at or above `occ` has been touched
    /// since the last reset. Not exact, and not part of any equality.
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
    /// A fresh row of `cols` cells, every one a copy of `template`.
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

    /// Build a row the reflow has just laid out.
    ///
    /// `hints` is the **complete** content-hint set: the reflow visits every
    /// cell as it lays the row out, so it accumulates the hints there rather
    /// than paying a second pass over the same cells here. It also carries
    /// `HAS_GRAPHIC`, which needs the interner to derive and which the caller
    /// brings over from the logical line's source rows. The id is assigned when
    /// the row is placed in the ring.
    pub(crate) fn from_cells(cells: Vec<Cell>, wrapped: bool, hints: RowFlags, seq: SeqNo) -> Row {
        let mut flags = RowFlags::DIRTY | hints;
        flags.set(RowFlags::WRAPPED, wrapped);
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

    /// Hand a row that already fits the new width straight through.
    ///
    /// The reference's own rule: a row is a reflow target only when it is short
    /// **and** carries the wrap flag; everything else it merely grows in place
    /// (Alacritty's `alacritty_terminal/src/grid/resize.rs:103-107, 231-238`). The
    /// caller has checked that this row is a whole logical line and that nothing
    /// above `keep` is content, so only the width changes — no copy, no
    /// allocation and no hint rescan.
    ///
    /// The tail is cleared so the result is cell-for-cell what laying the row
    /// out from scratch would have produced. The existing hints stay: they can
    /// only over-approximate now, which is what a content hint promises.
    ///
    /// No `repair_wide_pairs` sweep: neither half of a wide pair is a trimmable
    /// blank, so nothing at or above `keep` is one and the clear cannot split a
    /// pair; and the caller refuses a row whose last content cell is a
    /// `LeadingWideSpacer`, which is the only other width the clear could
    /// strand. `Screen::assert_integrity` re-checks every row after every resize
    /// in debug builds, so the argument is not left standing on its own.
    pub(crate) fn into_refitted(mut self, cols: u16, keep: u16, seq: SeqNo) -> Row {
        self.cells.resize(cols as usize, Cell::EMPTY);
        self.cells[keep as usize..].fill(Cell::EMPTY);
        self.header.occ = cols;
        self.header.seq = seq;
        self.header.flags.insert(RowFlags::DIRTY);
        self.header.flags.remove(RowFlags::WRAPPED);
        self
    }

    pub(crate) fn header(&self) -> &RowHeader {
        &self.header
    }

    /// Every cell of the row, left to right.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Clear the row to `template`, reusing the allocation.
    ///
    /// Only `0..occ` is cleared when the last cell already carries the
    /// template's style, and the whole row otherwise. Comparing the interned
    /// style id rather than the background colour over-approximates in the safe
    /// direction: it can only clear more than strictly necessary, never less,
    /// and it keeps the erase paths off the interner.
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
pub(crate) fn flags_for(cell: Cell) -> RowFlags {
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
pub(crate) fn repair_wide_pairs(cells: &mut [Cell]) {
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

    /// The row's stream-wide id.
    pub fn id(&self) -> RowId {
        self.id
    }

    /// `SeqNo::default()` for a row that was never written, which is below every
    /// watermark a consumer can hold.
    pub(crate) fn seq(&self) -> SeqNo {
        self.header.map_or(SeqNo::default(), |header| header.seq)
    }

    /// The row's damage bit and content hints.
    ///
    /// A caller scanning for the last row that holds content needs these to
    /// read [`RowRef::occ`] correctly: a reset sets the flags from the erase
    /// template and *then* zeroes `occ`, so the hints are the only thing that
    /// says whether an `occ == 0` row is blank or painted with a non-default
    /// background.
    pub fn flags(&self) -> RowFlags {
        self.header.map_or(RowFlags::empty(), |header| header.flags)
    }

    /// Whether the logical line continues on the next row.
    pub fn wrapped(&self) -> bool {
        self.flags().contains(RowFlags::WRAPPED)
    }

    /// The over-approximating occupancy hint from the row header; `0` for a row
    /// that was never written.
    pub fn occ(&self) -> u16 {
        self.header.map_or(0, |header| header.occ)
    }

    /// Every cell of the row, left to right; blanks for an unwritten row.
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
    /// Open `row` for writing, stamping it with the current batch and marking it
    /// dirty.
    pub fn new(row: &'a mut Row, seq: SeqNo) -> RowMut<'a> {
        row.header.seq = seq;
        row.header.flags.insert(RowFlags::DIRTY);
        RowMut { row }
    }

    /// The row's stream-wide id.
    pub fn id(&self) -> RowId {
        self.row.header.id
    }

    /// Every cell of the row, left to right.
    pub fn cells(&self) -> &[Cell] {
        &self.row.cells
    }

    /// Write one cell over whatever half of a wide pair was there.
    ///
    /// The repair runs against the cell being overwritten, before the new one
    /// lands.
    pub(crate) fn write_repairing(&mut self, col: u16, cell: Cell) {
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
        self.clear_wrap_at(col as usize + 1);
    }

    /// Write one cell **without** clearing the row's wrap flag.
    ///
    /// A wide-pair repair, and a tab stop's fill, change one cell in place
    /// rather than assigning a whole new cell, so the wrap flag has to survive
    /// them. Here the wrap flag lives on the row rather than on its last cell,
    /// so those paths need a write that says "this is a repair, not an
    /// overwrite".
    ///
    /// Without it, a wide glyph repainted over the first columns of a row
    /// releases the previous row's trailing `LeadingWideSpacer` and clears
    /// **that row's** wrap flag with it, splitting a wrapped CJK line on the
    /// next reflow.
    pub(crate) fn repair(&mut self, col: u16, cell: Cell) {
        let Some(slot) = self.row.cells.get_mut(col as usize) else {
            return;
        };
        *slot = cell;
        self.row.header.occ = self.row.header.occ.max(col.saturating_add(1));
        self.row.header.flags.insert(flags_for(cell));
    }

    /// The wrap flag dies with the cell that carried it.
    ///
    /// A terminal that keeps the wrap flag on the row's last **cell** wipes it
    /// whenever that cell is overwritten or erased. Here the flag lives on the
    /// row, so every write that reaches the last column clears it explicitly.
    /// Otherwise a TUI repainting over the end of a wrapped row leaves a stale
    /// flag behind and a later reflow rejoins two rows that are not one logical
    /// line.
    ///
    /// `end` is the exclusive end of what was written.
    fn clear_wrap_at(&mut self, end: usize) {
        if end >= self.row.cells.len() {
            self.row.header.flags.remove(RowFlags::WRAPPED);
        }
    }

    /// Fill `range` with `template`, repairing any wide pair the fill splits.
    ///
    /// The reference leaves an orphaned spacer here; the integrity assertion
    /// this design mandates ("no `Wide` without its `WideSpacer`") makes the
    /// repair compulsory.
    pub(crate) fn fill(&mut self, range: std::ops::Range<u16>, template: Cell) {
        let end = (range.end as usize).min(self.row.cells.len());
        let start = (range.start as usize).min(end);
        self.row.cells[start..end].fill(template);
        repair_wide_pairs(&mut self.row.cells);
        self.row.header.occ = self.row.header.occ.max(end as u16);
        self.row.header.flags.insert(flags_for(template));
        self.clear_wrap_at(end);
    }

    /// `DCH`: a plain shift left by `n`, not an `end`-clamped swap.
    /// `end`-clamped swap.
    pub(crate) fn delete_cells(&mut self, col: u16, n: u16, template: Cell) {
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
        self.clear_wrap_at(cols);
    }

    /// `ICH`: shift right from `col` by `n`, filling the opened cells.
    pub(crate) fn insert_cells(&mut self, col: u16, n: u16, template: Cell) {
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
        self.clear_wrap_at(cols);
    }

    /// Insert mode's shift, which moves the whole row right by the glyph width.
    /// A wide pair the shift splits is repaired rather than left as an orphaned
    /// spacer.
    pub(crate) fn shift_right_from(&mut self, col: u16, n: u16) {
        let cols = self.row.cells.len();
        let col = (col as usize).min(cols);
        let n = (n as usize).min(cols - col);
        if n == 0 {
            return;
        }
        self.row.cells.copy_within(col..cols - n, col + n);
        repair_wide_pairs(&mut self.row.cells);
        self.row.header.occ = cols as u16;
        self.clear_wrap_at(cols);
    }

    /// Declare whether the logical line continues on the next row.
    pub fn set_wrapped(&mut self, wrapped: bool) {
        self.row.header.flags.set(RowFlags::WRAPPED, wrapped);
    }

    /// Declare that a graphic covers part of this row. The grid cannot derive it
    /// (the extras id only resolves through the interner), so the graphics
    /// packet stamps it explicitly.
    pub(crate) fn mark_graphic(&mut self) {
        self.row.header.flags.insert(RowFlags::HAS_GRAPHIC);
    }

    /// Clear the row to `template`, reusing the allocation.
    pub fn reset(&mut self, template: Cell, seq: SeqNo) {
        self.row.reset(template, seq);
    }

    pub(crate) fn resize(&mut self, cols: u16, template: Cell, seq: SeqNo) {
        self.row.resize(cols, template, seq);
    }
}

/// Ring-side helpers, kept out of the public surface.
impl Row {
    /// The row's stream-wide id.
    pub fn id(&self) -> RowId {
        self.header.id
    }

    pub(crate) fn take_rehomed(mut self, id: RowId, seq: SeqNo) -> Row {
        self.rehome(id, seq);
        self
    }

    /// Live heap this row owns, in bytes.
    pub fn bytes(&self) -> usize {
        self.heap_bytes()
    }
}
