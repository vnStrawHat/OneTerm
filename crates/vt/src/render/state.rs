//! The incremental render state: what the renderer gets instead of a copy of
//! the viewport.
//!
//! Two phases, and the split is the whole point:
//!
//! * [`RenderState::begin_update`] runs **under the caller's lock** and copies
//!   only the rows whose sequence number passed this state's watermark.
//! * [`RenderState::map_colors`] runs **outside it** and does the one thing that
//!   needs no engine state.
//!
//! The result is tri-state. `Unchanged` lets the element skip layout and paint
//! entirely; `Partial { scrolled }` means "shift your own cache by `scrolled`
//! rows, then rebuild exactly the rows in [`RenderState::changed`]"; `Full`
//! means rebuild everything. [`RenderState::rows`] always holds the whole
//! viewport either way.

// Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`,
// contract: `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md`
// clause 2 (R-15).

use std::time::Instant;

use crate::graphics::Placement;
use crate::grid::SeqNo;
use crate::grid::{RowId, Screen, Size, TerminalGrid, Viewport};
use crate::intern::{GraphicId, Hyperlink, HyperlinkId, Interner};
use crate::render::modes::ModeSnapshot;
use crate::render::palette::Palette;
use crate::render::row::RenderRow;
use crate::render::sync::SyncState;
use crate::selection::SelectionRange;

/// How far a consumer has read the engine's per-row sequence numbers.
///
/// One per consumer, owned by that consumer's [`RenderState`]: there is no reset
/// pass and nobody clears anybody else's damage.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Watermark(pub SeqNo);

/// What one update did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderUpdate {
    /// Nothing changed: no row was copied, and the cursor, the modes and the
    /// viewport all stand where they did.
    Unchanged,
    /// Shift the consumer's own row cache by `scrolled` viewport rows, then
    /// rebuild the rows in [`RenderState::changed`].
    Partial {
        /// Viewport rows the content moved by: positive scrolls up (older
        /// content leaves the top), negative scrolls down.
        scrolled: i32,
    },
    /// Rebuild everything.
    Full,
}

/// Where the cursor is, refreshed every update so a blink or a drag never
/// forces a row rebuild.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RenderCursor {
    /// The absolute id of the row the cursor is on.
    pub id: RowId,
    /// Zero-based column.
    pub col: u16,
    /// The viewport row, or `None` while the user is scrolled back past it.
    pub row: Option<u16>,
    /// Whether the program asked for a visible cursor (`DECTCEM`).
    pub visible: bool,
}

/// One image's place on the grid, with its anchor already resolved.
///
/// The cell carries only the [`GraphicId`]; the painter derives the cell's
/// offset inside the image from this record (see
/// [`RenderState::graphic_offset`]), which is what keeps a whole image to
/// **one** interned entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RenderPlacement {
    /// The image this placement shows.
    pub id: GraphicId,
    /// The row holding the image's top-left cell.
    pub row: RowId,
    /// The column holding the image's top-left cell.
    pub col: u16,
    /// How many columns the image covers.
    pub cols: u16,
    /// How many rows the image covers.
    pub rows: u16,
    /// The decoded image's size in pixels; the renderer rescales it by
    /// `cell_width / 10` and `line_height / 20`.
    pub pixel_size: (u32, u32),
}

/// The engine fields one update reads, borrowed as the disjoint fields they are.
///
/// [`crate::Terminal::render_update`] builds one of these from its own fields;
/// the split exists so the render state can borrow exactly the fields it reads
/// and reach nothing else.
pub(crate) struct EngineView<'a> {
    pub grid: &'a TerminalGrid,
    pub interner: &'a Interner,
    pub sync: &'a mut SyncState,
    pub modes: ModeSnapshot,
    /// Bumped by the terminal on anything that invalidates every row: a resize,
    /// a reflow, an alternate-screen swap, `RIS`.
    pub generation: u32,
    /// Bumped when the theme or the OSC colour overrides change.
    pub palette_epoch: u32,
    /// The active selection, already resolved. Refreshed every
    /// update like the cursor, because a drag must not force a row rebuild —
    /// but a drag over static content must still produce a frame, so a change
    /// here defeats `Unchanged`.
    pub selection: Option<SelectionRange>,
    /// The live placements, copied into the state next to the rows so the
    /// painter needs no engine access. **Never the pixels**: those are drained
    /// once, by `Terminal::take_graphics`.
    pub placements: &'a [Placement],
}

/// One consumer's view of the terminal, reused across frames.
#[derive(Debug, Default)]
pub struct RenderState {
    generation: Option<u32>,
    watermark: Watermark,
    viewport_top: RowId,
    scroll_offset: u32,
    size: Option<Size>,
    rows: Vec<RenderRow>,
    changed: Vec<u16>,
    cursor: RenderCursor,
    selection: Option<SelectionRange>,
    placements: Vec<RenderPlacement>,
    modes: ModeSnapshot,
    palette_epoch: u32,
    mapped_epoch: Option<u32>,
    hyperlinks: Vec<(HyperlinkId, Hyperlink)>,
    force_full: bool,
    rows_copied: u64,
    /// A mode or cursor change observed while frames were suppressed.
    ///
    /// Sticky until a frame actually reports it: a synchronized update that
    /// touches no row would otherwise swallow `DECTCEM` or a cursor move, since
    /// the snapshot is refreshed on the skipped frame and the next frame then
    /// compares equal.
    meta_dirty: bool,
}

impl RenderState {
    /// A fresh state that has read nothing yet, so its first update rebuilds
    /// every row.
    pub fn new() -> RenderState {
        RenderState::default()
    }

    /// Phase 1, under the caller's lock.
    pub(crate) fn begin_update(&mut self, engine: EngineView<'_>, now: Instant) -> RenderUpdate {
        let EngineView {
            grid,
            interner,
            sync,
            modes,
            generation,
            palette_epoch,
            selection,
            placements,
        } = engine;
        let screen = grid.screen();
        let viewport = screen.viewport();

        // Refreshed on every update, a suppressed one included: a placement is
        // cheap to copy (usually none at all) and the painter must never hold a
        // placement whose anchor has moved under it.
        self.placements.clear();
        self.placements
            .extend(placements.iter().filter_map(|placement| {
                let pos = grid.anchors().get(placement.anchor)?;
                Some(RenderPlacement {
                    id: placement.id,
                    row: pos.row,
                    col: pos.col,
                    cols: placement.cols,
                    rows: placement.rows,
                    pixel_size: placement.pixel_size,
                })
            }));

        // R-17: the modes and the cursor are refreshed on every update,
        // including one that returns Unchanged, because the view reads both at
        // paint time and must not take the lock to do it.
        let modes_changed = self.modes != modes;
        self.modes = modes;
        let cursor = cursor_of(screen, modes.show_cursor);
        let cursor_changed = self.cursor != cursor;
        self.cursor = cursor;
        let selection_changed = self.selection != selection;
        self.selection = selection;

        // Mode 2026: the bytes are already applied; the renderer just does not
        // look yet. A state that has never been built is built first, because
        // `rows()` holds the full viewport for every result, `Unchanged`
        // included (R-15); suppression starts from the frame after that.
        if sync.suppresses_frame(now) && !self.rows.is_empty() {
            self.meta_dirty |= modes_changed || cursor_changed || selection_changed;
            self.changed.clear();
            return RenderUpdate::Unchanged;
        }

        let seq = grid.seq();
        debug_assert!(seq >= self.watermark.0, "the watermark moved backwards");
        // R-28: the full two-screen walk runs once per feed, resize and render
        // update, never per mutation. It is a no-op outside debug builds.
        grid.assert_integrity(Some(interner));

        let scrolled = self.viewport_delta(viewport);
        let full = self.needs_full(generation, palette_epoch, viewport, scrolled);
        self.changed.clear();

        if full {
            self.rebuild(screen, interner, viewport);
        } else {
            self.shift_rows(scrolled.unwrap_or(0));
            self.copy_changed(screen, interner, viewport);
        }

        self.generation = Some(generation);
        self.palette_epoch = palette_epoch;
        self.viewport_top = viewport.top;
        self.scroll_offset = screen.scroll_offset();
        self.size = Some(Size {
            rows: viewport.rows,
            cols: viewport.cols,
        });
        self.watermark = Watermark(seq);
        debug_assert_eq!(
            self.rows.len(),
            viewport.rows as usize,
            "rows() must always hold the full viewport"
        );

        // Every row was copied, so there is nothing for a consumer to shift and
        // nothing to keep: that is what Full means. This is also how RIS and a
        // full-screen repaint reach the renderer.
        let update = if full || self.changed.len() == self.rows.len() {
            RenderUpdate::Full
        } else if self.changed.is_empty()
            && scrolled == Some(0)
            && !modes_changed
            && !cursor_changed
            && !selection_changed
            && !self.meta_dirty
        {
            RenderUpdate::Unchanged
        } else {
            RenderUpdate::Partial {
                scrolled: scrolled.unwrap_or(0),
            }
        };
        if update != RenderUpdate::Unchanged {
            // Whatever was held back by a skipped frame has now been reported.
            self.meta_dirty = false;
        }
        update
    }

    /// Phase 2, outside the lock: named and palette colours become pixels.
    ///
    /// A palette epoch change remaps every row; otherwise only the rows this
    /// update copied need it, and calling it twice is a no-op.
    pub fn map_colors(&mut self, palette: &Palette) {
        let RenderState {
            rows,
            changed,
            palette_epoch,
            mapped_epoch,
            ..
        } = self;
        if *mapped_epoch == Some(*palette_epoch) {
            for &index in changed.iter() {
                if let Some(row) = rows.get_mut(index as usize) {
                    row.map_colors(palette);
                }
            }
        } else {
            for row in rows.iter_mut() {
                row.map_colors(palette);
            }
        }
        *mapped_epoch = Some(*palette_epoch);
    }

    /// Always the full viewport, indexed by viewport row.
    pub fn rows(&self) -> &[RenderRow] {
        &self.rows
    }

    /// The viewport row indices this update copied.
    pub fn changed(&self) -> &[u16] {
        &self.changed
    }

    /// The viewport this state holds, which the view needs every frame to lay
    /// out its grid. Zero in both dimensions until the first update.
    pub fn size(&self) -> Size {
        self.size.unwrap_or(Size { rows: 0, cols: 0 })
    }

    /// Where the cursor is and whether it is visible, refreshed every update.
    pub fn cursor(&self) -> &RenderCursor {
        &self.cursor
    }

    /// The active selection, refreshed every update. The painter reads it here
    /// rather than asking the engine a second question.
    pub fn selection(&self) -> Option<SelectionRange> {
        self.selection
    }

    /// The live image placements, refreshed every update. Ids only: the pixels
    /// come from `Terminal::take_graphics`, which is the one drain.
    pub fn placements(&self) -> &[RenderPlacement] {
        &self.placements
    }

    /// The placement a cell's [`GraphicId`] names, or `None` once the image is
    /// gone — a stale reference paints nothing rather than painting wrongly.
    pub fn placement(&self, id: GraphicId) -> Option<&RenderPlacement> {
        self.placements.iter().find(|placement| placement.id == id)
    }

    /// The cell's `(col, row)` offset **inside the image's cell grid**, which is
    /// what the painter used to read out of the cell itself.
    ///
    /// ```text
    /// offset    = (col - placement.col, row - placement.row)
    /// origin_px = cell_origin(row, col) - offset * cell_size
    /// ```
    ///
    /// `None` when the image is gone, or when the cell is outside the placement
    /// — which an `IL` or `SD` splitting an image can produce.
    pub fn graphic_offset(&self, id: GraphicId, row: RowId, col: u16) -> Option<(u16, u16)> {
        let placement = self.placement(id)?;
        let down = u16::try_from(row.distance(placement.row)).ok()?;
        let across = col.checked_sub(placement.col)?;
        (row >= placement.row && down < placement.rows && across < placement.cols)
            .then_some((across, down))
    }

    /// The modes the view reads at paint time, refreshed every update.
    pub fn modes(&self) -> ModeSnapshot {
        self.modes
    }

    /// How far this consumer has read the engine's per-row sequence numbers.
    pub fn watermark(&self) -> Watermark {
        self.watermark
    }

    /// The first visible row.
    pub fn viewport_top(&self) -> RowId {
        self.viewport_top
    }

    /// Rows between the viewport and the newest row; `0` is the sticky bottom.
    pub fn scroll_offset(&self) -> u32 {
        self.scroll_offset
    }

    /// The strings behind a cell's [`HyperlinkId`], resolved under the lock so
    /// the painted URL cannot change mid-frame.
    pub fn hyperlink(&self, id: HyperlinkId) -> Option<&Hyperlink> {
        self.hyperlinks
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, link)| link)
    }

    /// Rows copied since this state was created. The evidence that an unchanged
    /// frame costs no copy at all.
    pub fn rows_copied(&self) -> u64 {
        self.rows_copied
    }

    /// Force the next update to rebuild everything.
    pub fn invalidate(&mut self) {
        self.force_full = true;
    }

    fn needs_full(
        &self,
        generation: u32,
        palette_epoch: u32,
        viewport: Viewport,
        scrolled: Option<i32>,
    ) -> bool {
        let size = Size {
            rows: viewport.rows,
            cols: viewport.cols,
        };
        self.force_full
            || self.generation != Some(generation)
            || self.palette_epoch != palette_epoch
            || self.size != Some(size)
            || self.rows.len() != viewport.rows as usize
            // A scroll the consumer's cache cannot absorb is a rebuild.
            || scrolled.is_none_or(|delta| delta.unsigned_abs() >= u32::from(viewport.rows))
    }

    /// `None` when the two tops are not comparable at all: the alternate screen
    /// allocates from its own lane, so entering or leaving it is a rebuild.
    fn viewport_delta(&self, viewport: Viewport) -> Option<i32> {
        if is_alternate(self.viewport_top) != is_alternate(viewport.top) {
            return None;
        }
        i32::try_from(viewport.top.0 as i64 - self.viewport_top.0 as i64).ok()
    }

    fn rebuild(&mut self, screen: &Screen, interner: &Interner, viewport: Viewport) {
        let RenderState {
            rows,
            changed,
            hyperlinks,
            rows_copied,
            force_full,
            ..
        } = self;
        rows.resize_with(viewport.rows as usize, RenderRow::default);
        hyperlinks.clear();
        for index in 0..viewport.rows {
            let id = viewport.top + u64::from(index);
            rows[index as usize].copy_from(screen.row(id), interner, hyperlinks);
            changed.push(index);
        }
        *rows_copied += u64::from(viewport.rows);
        *force_full = false;
    }

    /// Consume the viewport's own motion as a move: the rows that stayed on
    /// screen keep their copies and only change index.
    fn shift_rows(&mut self, scrolled: i32) {
        let len = self.rows.len();
        if scrolled == 0 || len == 0 {
            return;
        }
        let distance = (scrolled.unsigned_abs() as usize).min(len);
        if scrolled > 0 {
            self.rows.rotate_left(distance);
        } else {
            self.rows.rotate_right(distance);
        }
    }

    fn copy_changed(&mut self, screen: &Screen, interner: &Interner, viewport: Viewport) {
        let RenderState {
            rows,
            changed,
            hyperlinks,
            watermark,
            rows_copied,
            ..
        } = self;
        for index in 0..viewport.rows {
            let id = viewport.top + u64::from(index);
            let row = screen.row(id);
            let slot = &mut rows[index as usize];
            // Either the row itself changed since this consumer last looked, or
            // the scroll moved a different row into this position. Blanking is
            // covered by the first test: the grid stamps a row it blanks
            // (`Screen::blank_row`), which is what makes a second watermark
            // consumer possible without an engine change (`DEC-0015`).
            if slot.id != id || row.seq() > watermark.0 {
                slot.copy_from(row, interner, hyperlinks);
                changed.push(index);
                *rows_copied += 1;
            }
        }
    }
}

fn cursor_of(screen: &Screen, visible: bool) -> RenderCursor {
    let cursor = screen.cursor();
    let top = screen.visible_top();
    let row = cursor.pos.row.distance(top);
    RenderCursor {
        id: cursor.pos.row,
        col: cursor.pos.col,
        row: (cursor.pos.row >= top && row < u64::from(screen.rows())).then_some(row as u16),
        visible,
    }
}

fn is_alternate(id: RowId) -> bool {
    id >= RowId::ALT_ORIGIN
}
