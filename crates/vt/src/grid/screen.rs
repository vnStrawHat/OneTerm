//! One screen: the ring of rows, the viewport, the cursor and every primitive
//! that moves or erases a row.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md`.
//!
//! Two coordinate notions live here and they are deliberately different:
//!
//! * the **screen** is always the bottom [`Screen::rows`] rows, and every VT
//!   operation addresses it (`row_of_index`), because a program writing to the
//!   terminal cannot see the user's scrollback position;
//! * the **viewport** is what the user sees, `offset` rows above the screen.
//!
//! Nothing here returns an error and nothing here panics. Counts clamp, an
//! out-of-range row reads as blanks, and a glyph that cannot be placed is
//! dropped and counted.

use std::cmp::Ordering;
use std::ops::Range;

use crate::cell::Style;
use crate::cell::{Cell, CellContent, CellWidth};
use crate::grid::anchor::{AnchorId, AnchorKind, Anchors};
use crate::grid::row::{Row, RowFlags, RowMut, RowRef, SeqNo};
use crate::grid::{MAX_COLS, MAX_ROWS, Pos, RowId, ScrollRegion, Size, Viewport};
use crate::intern::{GraphemeArena, Interner, StyleId};
use crate::width::scalar_width;

/// Which run of the row-id space a screen draws from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ScreenKind {
    Primary,
    Alternate,
}

impl ScreenKind {
    fn origin(self) -> RowId {
        match self {
            ScreenKind::Primary => RowId::PRIMARY_ORIGIN,
            ScreenKind::Alternate => RowId::ALT_ORIGIN,
        }
    }
}

/// A G0-G3 designation. Which one `SI` / `SO` selects lives on the terminal, not
/// the cursor, so `DECSC` / `DECRC` do not save it — reference behaviour.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Charset {
    #[default]
    Ascii,
    SpecialCharacterAndLineDrawing,
}

/// Whether a positioning operation is relative to the screen or to the scroll
/// region (`DECOM`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum CursorOrigin {
    #[default]
    Screen,
    Region,
}

/// Where the next glyph lands and what it will look like.
///
/// Two cells, because the reference erases with less than it prints:
///
/// * `template` is the reference's `cursor.template` — a blank carrying the SGR
///   style, the open hyperlink or image and the OSC 133 semantic. It is what a
///   printed glyph inherits.
/// * `erase` is the reference's `bg.into()` / `Cell::reset` — the **default**
///   cell with only the template's background. It is what every erase and every
///   row reset fills with, so an erase under an open underline or hyperlink
///   leaves plain blanks rather than decorated ones.
///
/// Keeping both on the cursor is what lets the erase paths stay off the
/// interner: the pair is recomputed once, in [`Screen::set_template`], whenever
/// the SGR template changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cursor {
    pub pos: Pos,
    pub pending_wrap: bool,
    pub charsets: [Charset; 4],
    template: Cell,
    erase: Cell,
}

impl Cursor {
    fn at(row: RowId) -> Cursor {
        Cursor {
            pos: Pos { row, col: 0 },
            pending_wrap: false,
            charsets: [Charset::Ascii; 4],
            template: Cell::EMPTY,
            erase: Cell::EMPTY,
        }
    }

    /// What a printed glyph inherits.
    pub fn template(&self) -> Cell {
        self.template
    }

    /// What an erase fills with: the default cell plus the template background.
    pub fn erase(&self) -> Cell {
        self.erase
    }
}

/// The two modes the print path has to know about. Both are owned by the mode
/// table (`US-0076`) and passed in, because the grid holds no mode state.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PrintMode {
    /// `IRM`.
    pub insert: bool,
    /// `DECAWM`.
    pub autowrap: bool,
}

impl Default for PrintMode {
    fn default() -> PrintMode {
        PrintMode {
            insert: false,
            autowrap: true,
        }
    }
}

/// `EL`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LineClear {
    Right,
    Left,
    All,
}

/// `ED`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DisplayClear {
    Below,
    Above,
    All,
    Saved,
}

/// Content moved from `top..=bottom` **onto other row ids** by `delta` rows.
///
/// `damage-and-render-state.md` defines this as in-region motion, "because that
/// moves content without moving the viewport": a consumer holding a `RowId`-keyed
/// row cache shifts the named rows by `delta` instead of rebuilding them. A whole
/// screen scroll therefore reports **nothing** — every row keeps its id and its
/// content, and the viewport's own motion reaches the renderer as
/// `RenderUpdate::Partial { scrolled }` instead.
///
/// It is a notification, not how anchors move; but it always agrees with what the
/// anchors did (R-02). `US-0079` wraps it in `VtEvent::RowsScrolled`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RowsScrolled {
    pub top: RowId,
    pub bottom: RowId,
    pub delta: i32,
}

/// What one scroll primitive did.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ScrollReport {
    /// `None` when no row's content changed id: a no-op, or a whole-screen
    /// scroll where the rows simply left the viewport into history.
    pub scrolled: Option<RowsScrolled>,
    /// The new oldest row when history was trimmed. `US-0079` wraps it in
    /// `VtEvent::RowsTrimmed`.
    pub trimmed: Option<RowId>,
    /// Rows that entered scrollback, which is what `lines_produced` counts when
    /// the scroll was not itself caused by a line feed.
    pub history_rows: u16,
}

/// A bitmap of horizontal tab stops, one entry per column.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TabStops {
    stops: Vec<bool>,
}

/// The reference's `INITIAL_TABSTOPS`.
const TAB_INTERVAL: u16 = 8;

impl TabStops {
    pub fn new(cols: u16) -> TabStops {
        TabStops {
            stops: (0..cols).map(|col| col % TAB_INTERVAL == 0).collect(),
        }
    }

    pub fn is_stop(&self, col: u16) -> bool {
        self.stops.get(col as usize).copied().unwrap_or(false)
    }

    pub fn set(&mut self, col: u16) {
        if let Some(stop) = self.stops.get_mut(col as usize) {
            *stop = true;
        }
    }

    pub fn clear(&mut self, col: u16) {
        if let Some(stop) = self.stops.get_mut(col as usize) {
            *stop = false;
        }
    }

    pub fn clear_all(&mut self) {
        self.stops.fill(false);
    }

    /// `CSI ? 5 W` (correction C10, wired up in `US-0076`).
    pub fn reset_defaults(&mut self) {
        for (col, stop) in self.stops.iter_mut().enumerate() {
            *stop = col as u16 % TAB_INTERVAL == 0;
        }
    }

    /// Trap 27: stops cleared in the retained prefix stay cleared, and the
    /// regrown region gets default stops at absolute multiples of eight.
    fn resize(&mut self, cols: u16) {
        let old = self.stops.len();
        self.stops.resize_with(cols as usize, {
            let mut index = old;
            move || {
                let stop = index as u16 % TAB_INTERVAL == 0;
                index += 1;
                stop
            }
        });
    }

    fn heap_bytes(&self) -> usize {
        self.stops.capacity()
    }
}

/// One screen's rows, viewport, cursor and tab stops.
pub struct Screen {
    kind: ScreenKind,
    /// `None` until first written. The whole ring is allocated once, and its
    /// length is a session constant (R-30).
    slots: Box<[Option<Row>]>,
    mask: usize,
    newest: RowId,
    oldest: RowId,
    rows: u16,
    cols: u16,
    scrollback_limit: u32,
    /// Distance from the newest row. `0` is sticky bottom.
    offset: u32,
    region: ScrollRegion,
    cursor: Cursor,
    saved_cursor: Cursor,
    tabs: TabStops,
    seq: SeqNo,
    cursor_anchor: AnchorId,
    saved_cursor_anchor: AnchorId,
    viewport_anchor: AnchorId,
    /// Wide glyphs that could not be placed at all: `cols == 1`, or the last
    /// column with `DECAWM` reset.
    dropped_wide: u32,
}

fn ring_len_for(scrollback_limit: u32) -> usize {
    (scrollback_limit as usize + MAX_ROWS as usize).next_power_of_two()
}

impl Screen {
    pub fn new(
        kind: ScreenKind,
        size: Size,
        scrollback_limit: u32,
        anchors: &mut Anchors,
    ) -> Screen {
        let size = size.clamped();
        let scrollback_limit = scrollback_limit.min(crate::grid::SCROLLBACK_MAX);
        let ring_len = ring_len_for(scrollback_limit);
        let origin = kind.origin();
        let newest = origin + (size.rows as u64 - 1);
        let cursor = Cursor::at(origin);
        let screen = Screen {
            kind,
            slots: vec![None; ring_len].into_boxed_slice(),
            mask: ring_len - 1,
            newest,
            oldest: origin,
            rows: size.rows,
            cols: size.cols,
            scrollback_limit,
            offset: 0,
            region: ScrollRegion::full(size.rows),
            cursor,
            saved_cursor: cursor,
            tabs: TabStops::new(size.cols),
            seq: SeqNo::default(),
            cursor_anchor: anchors.register(AnchorKind::Cursor, cursor.pos),
            saved_cursor_anchor: anchors.register(AnchorKind::SavedCursor, cursor.pos),
            viewport_anchor: anchors.register(
                AnchorKind::ViewportTop,
                Pos {
                    row: origin,
                    col: 0,
                },
            ),
            dropped_wide: 0,
        };
        screen.debug_assert_integrity();
        screen
    }

    // ── Geometry ────────────────────────────────────────────────────────────

    pub fn kind(&self) -> ScreenKind {
        self.kind
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn size(&self) -> Size {
        Size {
            rows: self.rows,
            cols: self.cols,
        }
    }

    pub fn newest(&self) -> RowId {
        self.newest
    }

    pub fn oldest(&self) -> RowId {
        self.oldest
    }

    /// `oldest..newest + 1`.
    pub fn row_range(&self) -> Range<RowId> {
        self.oldest..self.newest + 1
    }

    pub fn history_len(&self) -> u32 {
        (self.newest.distance(self.oldest) + 1).saturating_sub(self.rows as u64) as u32
    }

    /// The first row an escape sequence can address.
    pub fn screen_top(&self) -> RowId {
        self.newest - (self.rows as u64 - 1)
    }

    /// The first row the user sees.
    pub fn visible_top(&self) -> RowId {
        self.screen_top() - self.offset as u64
    }

    pub fn viewport(&self) -> Viewport {
        Viewport {
            top: self.visible_top(),
            rows: self.rows,
            cols: self.cols,
        }
    }

    pub fn scroll_offset(&self) -> u32 {
        self.offset
    }

    /// Screen index to row id. Out-of-range indices clamp to the screen.
    pub fn row_of_index(&self, index: u16) -> RowId {
        self.screen_top() + index.min(self.rows.saturating_sub(1)) as u64
    }

    /// Row id to screen index, or `None` when the row is in history.
    pub fn index_of(&self, id: RowId) -> Option<u16> {
        let top = self.screen_top();
        if id < top || id > self.newest {
            return None;
        }
        Some(id.distance(top) as u16)
    }

    pub fn cursor_row_index(&self) -> u16 {
        self.index_of(self.cursor.pos.row).unwrap_or(0)
    }

    /// A viewport-coordinate region as a row-id range (N-13): the anchor list
    /// only ever sees row ids.
    pub fn rows_of(&self, region: ScrollRegion) -> Range<RowId> {
        let region = self.clamp_region(region);
        let top = self.screen_top() + region.top as u64;
        let bottom = self.screen_top() + region.bottom as u64;
        top..bottom
    }

    fn clamp_region(&self, region: ScrollRegion) -> ScrollRegion {
        let bottom = region.bottom.min(self.rows);
        ScrollRegion {
            top: region.top.min(bottom),
            bottom,
        }
    }

    // ── Rows ────────────────────────────────────────────────────────────────

    fn slot_of(&self, id: RowId) -> usize {
        (id.0 as usize) & self.mask
    }

    /// A slot that was never written reads as blanks and allocates nothing.
    ///
    /// So does a row that has been trimmed out of history, and the two are
    /// indistinguishable here by design: [`Screen::row_range`] is what tells a
    /// consumer which ids are still live.
    pub fn row(&self, id: RowId) -> RowRef<'_> {
        let slot = self.slot_of(id);
        match &self.slots[slot] {
            Some(row) if row.id() == id => RowRef::present(row),
            _ => RowRef::blank(id, self.cols),
        }
    }

    /// Allocate the slot if needed and stamp the current batch on it.
    ///
    /// Materialising an unwritten slot fills it with [`Cell::EMPTY`], **not**
    /// with the cursor's erase cell: an unwritten slot already reads as plain
    /// blanks, so giving it the current background-erase colour would repaint
    /// every untouched column of the row the moment one glyph lands on it.
    /// Deliberate background-erase blanking is [`Screen::blank_row`] and
    /// [`Screen::blank_slot`], which the scroll and reset paths call explicitly.
    /// (Found by the `US-0076` parity gate: it is what made `sgr`'s trailing
    /// blanks carry `48;5;1` where the reference leaves them default.)
    pub fn row_mut(&mut self, id: RowId) -> RowMut<'_> {
        let slot = self.slot_of(id);
        let (cols, seq) = (self.cols, self.seq);
        let entry = &mut self.slots[slot];
        if entry.as_ref().map(Row::id) != Some(id) {
            *entry = None;
        }
        let row = entry.get_or_insert_with(|| Row::new(id, cols, seq, Cell::EMPTY));
        RowMut::new(row, seq)
    }

    pub(crate) fn take_row(&mut self, id: RowId) -> Option<Row> {
        let slot = self.slot_of(id);
        if self.slots[slot].as_ref().map(Row::id) == Some(id) {
            return self.slots[slot].take();
        }
        None
    }

    fn place_row(&mut self, id: RowId, row: Option<Row>) {
        match row {
            Some(row) => {
                let (slot, seq) = (self.slot_of(id), self.seq);
                self.slots[slot] = Some(row.take_rehomed(id, seq));
            }
            // The source was an unwritten slot, so this row has just been
            // blanked — which is a mutation, not the absence of one.
            None => self.blank_row(id, Cell::EMPTY),
        }
    }

    /// Blank one live row, keeping its allocation and stamping it.
    ///
    /// **A blanked row is a changed row.** The damage contract
    /// (`damage-and-render-state.md`, `DEC-0015`) is a per-row `SeqNo` plus
    /// `RowFlags::DIRTY`, and an unwritten slot can carry neither: dropping the
    /// row back to `None` reads back as `SeqNo::default()`, below every
    /// watermark, so a consumer keeps painting the old content and the
    /// `DIRTY`-driven sweeps (grapheme GC, `GraphicReleased`) get a false
    /// negative. So a row that is blanked **in place** is always materialised.
    ///
    /// This is bounded by the screen height: only screen rows are ever blanked
    /// in place. History rows are created by [`Screen::blank_slot`] and dropped
    /// by a trim, which is where "an unwritten row costs no cells" lives.
    fn blank_row(&mut self, id: RowId, template: Cell) {
        let slot = self.slot_of(id);
        let (cols, seq) = (self.cols, self.seq);
        match &mut self.slots[slot] {
            // The width test is for a row that left the live range and came
            // back (see `blank_slot`): a column resize only walks live rows, so
            // that one can be the wrong width. Either arm stamps.
            Some(row) if row.id() == id && row.cells().len() == cols as usize => {
                row.reset(template, seq);
            }
            entry => *entry = Some(Row::new(id, cols, seq, template)),
        }
    }

    /// Clear one row to the current background-erase template.
    fn reset_row(&mut self, id: RowId) {
        self.blank_row(id, self.cursor.erase);
    }

    /// Reset a screen-relative range with the background-erase template.
    pub fn reset_rows(&mut self, rows: Range<u16>) {
        let end = rows.end.min(self.rows);
        for index in rows.start..end {
            let id = self.screen_top() + index as u64;
            self.reset_row(id);
        }
        self.debug_assert_integrity();
    }

    /// Allocate `n` fresh rows at the bottom, trimming history to the limit.
    /// Returns the new oldest row when anything was dropped.
    fn push_rows(&mut self, n: u16) -> Option<RowId> {
        let template = self.cursor.erase;
        self.push_rows_with(n, template)
    }

    /// The same, with an explicit fill. A resize passes `Cell::EMPTY`: the
    /// reference swaps its cursor template out for the default cell so rows
    /// created by a resize never inherit the current background
    /// (`research/engine-semantics.md` § 2.16).
    fn push_rows_with(&mut self, n: u16, template: Cell) -> Option<RowId> {
        let mut trimmed = None;
        for _ in 0..n {
            self.newest = self.newest + 1;
            while self.history_len() > self.scrollback_limit {
                let slot = self.slot_of(self.oldest);
                self.slots[slot] = None;
                self.oldest = self.oldest + 1;
                trimmed = Some(self.oldest);
            }
            let newest = self.newest;
            self.blank_slot(newest, template);
        }
        trimmed
    }

    /// Blank the slot of a row that is **entering** the live range at the
    /// bottom, or leaving it there.
    ///
    /// A brand-new id no consumer has ever seen needs no stamp, so an empty
    /// template leaves the slot unwritten — this is where the memory claim
    /// comes from. The exception is an id a row shrink dropped and a later push
    /// re-creates ([`Screen::drop_trailing_rows`]): that row *has* been seen, so
    /// it is blanked in place, stamp and all.
    fn blank_slot(&mut self, id: RowId, template: Cell) {
        let slot = self.slot_of(id);
        if template == Cell::EMPTY && self.slots[slot].as_ref().map(Row::id) != Some(id) {
            self.slots[slot] = None;
            return;
        }
        self.blank_row(id, template);
    }

    // ── Viewport ────────────────────────────────────────────────────────────

    /// A negative delta scrolls towards history.
    pub fn scroll_viewport(&mut self, delta: i32) {
        let offset = i64::from(self.offset) - i64::from(delta);
        self.offset = offset.clamp(0, i64::from(self.history_len())) as u32;
        self.debug_assert_integrity();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.offset = 0;
        self.debug_assert_integrity();
    }

    // ── Cursor ──────────────────────────────────────────────────────────────

    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    pub fn cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursor
    }

    /// Set the SGR template, deriving the erase cell once.
    ///
    /// This is the only writer of either cell, so an erase can never pick up the
    /// foreground, the attributes, the hyperlink or the semantic that a printed
    /// glyph would inherit — the reference's `bg.into()` rule — while the erase
    /// paths themselves never touch the interner.
    pub fn set_template(&mut self, template: Cell, interner: &mut Interner) {
        let bg = interner.resolve_style(template.style_id()).bg;
        let erase = interner.style(&Style {
            bg,
            ..Style::DEFAULT
        });
        self.cursor.template = template;
        self.cursor.erase = Cell::EMPTY.with_style(erase);
    }

    pub fn saved_cursor(&self) -> &Cursor {
        &self.saved_cursor
    }

    /// `DECSC`.
    pub fn save_cursor(&mut self) {
        self.saved_cursor = self.cursor;
    }

    /// `DECRC`.
    pub fn restore_cursor(&mut self) {
        self.cursor = self.saved_cursor;
        self.clamp_cursors();
        self.debug_assert_integrity();
    }

    /// Screen-relative positioning. The caller applies origin mode by passing a
    /// region-relative index through [`Screen::goto_origin`].
    pub fn goto(&mut self, index: u16, col: u16) {
        self.cursor.pos = Pos {
            row: self.row_of_index(index),
            col: col.min(self.cols - 1),
        };
        self.cursor.pending_wrap = false;
        self.debug_assert_integrity();
    }

    /// `CUP` with `DECOM` applied: in `Region` origin the index is relative to
    /// the scroll region's top and clamps to its bottom.
    pub fn goto_origin(&mut self, index: u16, col: u16, origin: CursorOrigin) {
        let index = match origin {
            CursorOrigin::Screen => index,
            CursorOrigin::Region => index
                .saturating_add(self.region.top)
                .min(self.region.bottom.saturating_sub(1)),
        };
        self.goto(index, col);
    }

    pub fn move_forward(&mut self, n: u16) {
        self.cursor.pos.col = self.cursor.pos.col.saturating_add(n).min(self.cols - 1);
        self.cursor.pending_wrap = false;
        self.debug_assert_integrity();
    }

    pub fn move_backward(&mut self, n: u16) {
        self.cursor.pos.col = self.cursor.pos.col.saturating_sub(n);
        self.cursor.pending_wrap = false;
        self.debug_assert_integrity();
    }

    /// Trap 1: a complete no-op at column 0 — the pending wrap is **not**
    /// cleared there. Reverse wrap (`? 45`) is an additive feature (`US-0086`).
    pub fn backspace(&mut self) {
        if self.cursor.pos.col == 0 {
            return;
        }
        self.cursor.pos.col -= 1;
        self.cursor.pending_wrap = false;
        self.debug_assert_integrity();
    }

    pub fn carriage_return(&mut self) {
        self.cursor.pos.col = 0;
        self.cursor.pending_wrap = false;
        self.debug_assert_integrity();
    }

    fn clamp_cursors(&mut self) {
        let top = self.screen_top();
        let last_col = self.cols - 1;
        for cursor in [&mut self.cursor, &mut self.saved_cursor] {
            cursor.pos.row = cursor.pos.row.max(top).min(self.newest);
            cursor.pos.col = cursor.pos.col.min(last_col);
        }
    }

    // ── Scroll region ───────────────────────────────────────────────────────

    pub fn region(&self) -> ScrollRegion {
        self.region
    }

    /// `DECSTBM`. Trap 15: an invalid range leaves the previous region intact,
    /// and every valid call homes the cursor.
    pub fn set_region(&mut self, top: u16, bottom: u16, origin: CursorOrigin) -> bool {
        let bottom = bottom.min(self.rows);
        let top = top.min(self.rows);
        if top >= bottom {
            return false;
        }
        self.region = ScrollRegion { top, bottom };
        self.goto_origin(0, 0, origin);
        self.debug_assert_integrity();
        true
    }

    /// `DECSTBM` with bounds the dispatch layer has already validated
    /// (`US-0076`): the reference's one-based validity test can produce an
    /// **empty** region, which [`Screen::set_region`]'s `top < bottom` contract
    /// cannot express, and it never homes the cursor itself.
    pub fn set_region_raw(&mut self, top: u16, bottom: u16) {
        let bottom = bottom.min(self.rows);
        self.region = ScrollRegion {
            top: top.min(bottom),
            bottom,
        };
    }

    const fn no_scroll() -> ScrollReport {
        ScrollReport {
            scrolled: None,
            trimmed: None,
            history_rows: 0,
        }
    }

    /// `SU`, a line feed at the region bottom, `DL`.
    ///
    /// The count clamps to the region height, so the short-region case (C3)
    /// reaches the same path as every other count: the region rotates by its own
    /// height and is then blank.
    pub fn scroll_up(
        &mut self,
        region: ScrollRegion,
        n: u16,
        anchors: &mut Anchors,
    ) -> ScrollReport {
        let region = self.clamp_region(region);
        let n = n.min(region.height());
        if n == 0 {
            return Screen::no_scroll();
        }
        let report = if region.top == 0 {
            self.scroll_up_into_history(region, n, anchors)
        } else {
            self.scroll_up_inside_region(region, n, anchors)
        };
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
        report
    }

    /// Trap 17: only a region anchored at row 0 fills scrollback.
    fn scroll_up_into_history(
        &mut self,
        region: ScrollRegion,
        n: u16,
        anchors: &mut Anchors,
    ) -> ScrollReport {
        // The rows below the region keep their screen positions, so their
        // content is lifted out and put back after the whole screen has moved.
        let bounded = region.bottom < self.rows;
        let tail_ids = self.screen_top() + region.bottom as u64..self.newest + 1;
        let tail: Vec<Option<Row>> = if bounded {
            (region.bottom..self.rows)
                .map(|index| {
                    let id = self.screen_top() + index as u64;
                    self.take_row(id)
                })
                .collect()
        } else {
            Vec::new()
        };

        let trimmed = self.push_rows(n);

        // The cursor and the saved cursor keep their position on the screen:
        // content kept its ids here, the screen moved. Done first, so the
        // per-method integrity check below never sees a cursor above the screen.
        self.cursor.pos.row = self.cursor.pos.row + u64::from(n);
        self.saved_cursor.pos.row = self.saved_cursor.pos.row + u64::from(n);
        self.clamp_cursors();

        // Sticky bottom stays sticky; a held view keeps looking at the same
        // content while history grows under it.
        if self.offset != 0 {
            self.offset = (self.offset + u32::from(n)).min(self.history_len());
        }

        // Only the rows below the region change id: everything inside the region
        // kept its content where it was and merely left the viewport, which is
        // why the whole-screen case reports no motion at all.
        let mut scrolled = None;
        if bounded {
            anchors.shift_region(tail_ids.clone(), i32::from(n), RowId(0)..RowId(0));
            for (step, row) in tail.into_iter().enumerate() {
                let id = self.screen_top() + (region.bottom as u64 + step as u64);
                self.place_row(id, row);
            }
            self.reset_rows(region.bottom - n..region.bottom);
            scrolled = Some(RowsScrolled {
                top: tail_ids.start + u64::from(n),
                bottom: self.newest,
                delta: i32::from(n),
            });
        }

        if trimmed.is_some() {
            anchors.trim(self.kind.origin(), self.oldest);
            self.offset = self.offset.min(self.history_len());
        }

        ScrollReport {
            scrolled,
            trimmed,
            history_rows: n,
        }
    }

    fn scroll_up_inside_region(
        &mut self,
        region: ScrollRegion,
        n: u16,
        anchors: &mut Anchors,
    ) -> ScrollReport {
        for index in region.top..region.bottom - n {
            let src = self.screen_top() + (index as u64 + n as u64);
            let dst = self.screen_top() + index as u64;
            let row = self.take_row(src);
            self.place_row(dst, row);
        }
        self.reset_rows(region.bottom - n..region.bottom);

        let ids = self.rows_of(region);
        let kill = ids.start..ids.start + u64::from(n);
        anchors.shift_region(ids.clone(), -i32::from(n), kill);
        ScrollReport {
            scrolled: Some(RowsScrolled {
                top: ids.start,
                bottom: ids.end - 1,
                delta: -i32::from(n),
            }),
            trimmed: None,
            history_rows: 0,
        }
    }

    /// `SD`, `RI` at the region top, `IL`. Trap 18: never pulls rows back out of
    /// scrollback; it always blanks the top `n` rows of the region.
    pub fn scroll_down(
        &mut self,
        region: ScrollRegion,
        n: u16,
        anchors: &mut Anchors,
    ) -> ScrollReport {
        let region = self.clamp_region(region);
        let n = n.min(region.height());
        if n == 0 {
            return Screen::no_scroll();
        }
        for index in (region.top + n..region.bottom).rev() {
            let src = self.screen_top() + (index as u64 - n as u64);
            let dst = self.screen_top() + index as u64;
            let row = self.take_row(src);
            self.place_row(dst, row);
        }
        self.reset_rows(region.top..region.top + n);

        let ids = self.rows_of(region);
        let kill = ids.end - u64::from(n)..ids.end;
        anchors.shift_region(ids.clone(), i32::from(n), kill);
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
        ScrollReport {
            scrolled: Some(RowsScrolled {
                top: ids.start,
                bottom: ids.end - 1,
                delta: i32::from(n),
            }),
            trimmed: None,
            history_rows: 0,
        }
    }

    /// `IL`. Trap 16: a complete no-op when the cursor is outside the region,
    /// and the cursor's row is the origin rather than the region's top.
    pub fn insert_lines(&mut self, n: u16, anchors: &mut Anchors) -> Option<ScrollReport> {
        let index = self.cursor_row_index();
        if !self.region.contains(index) {
            return None;
        }
        let region = ScrollRegion {
            top: index,
            bottom: self.region.bottom,
        };
        Some(self.scroll_down(region, n, anchors))
    }

    /// `DL`. Trap 16, as [`Screen::insert_lines`].
    pub fn delete_lines(&mut self, n: u16, anchors: &mut Anchors) -> Option<ScrollReport> {
        let index = self.cursor_row_index();
        if !self.region.contains(index) {
            return None;
        }
        let region = ScrollRegion {
            top: index,
            bottom: self.region.bottom,
        };
        Some(self.scroll_up(region, n, anchors))
    }

    /// `LF`, `IND`, `NEL`. A cursor below the region walks down to the bottom
    /// row and then stops (trap 16).
    pub fn linefeed(&mut self, anchors: &mut Anchors) -> Option<ScrollReport> {
        let next = self.cursor_row_index() + 1;
        if next == self.region.bottom {
            let region = self.region;
            return Some(self.scroll_up(region, 1, anchors));
        }
        if next < self.rows {
            self.cursor.pos.row = self.cursor.pos.row + 1;
            self.sync_anchors(anchors);
            self.debug_assert_integrity();
        }
        None
    }

    /// `RI`.
    pub fn reverse_index(&mut self, anchors: &mut Anchors) -> Option<ScrollReport> {
        let index = self.cursor_row_index();
        if index == self.region.top {
            let region = self.region;
            return Some(self.scroll_down(region, 1, anchors));
        }
        if index > 0 {
            self.cursor.pos.row = self.cursor.pos.row - 1;
            self.sync_anchors(anchors);
            self.debug_assert_integrity();
        }
        None
    }

    /// The implicit wrap at the end of a row.
    ///
    /// Unconditional: the reference returns immediately with `DECAWM` reset, so
    /// the caller must not reach here in that state. The print path here never
    /// does, because deviation G3 means the pending-wrap flag is never armed
    /// with `DECAWM` reset.
    pub fn wrapline(&mut self, anchors: &mut Anchors) {
        let id = self.cursor.pos.row;
        self.row_mut(id).set_wrapped(true);
        if self.cursor_row_index() + 1 >= self.region.bottom {
            self.linefeed(anchors);
        } else {
            self.cursor.pos.row = self.cursor.pos.row + 1;
        }
        self.cursor.pos.col = 0;
        self.cursor.pending_wrap = false;
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    // ── Tab stops ───────────────────────────────────────────────────────────

    pub fn tabs(&self) -> &TabStops {
        &self.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut TabStops {
        &mut self.tabs
    }

    /// `HT`. Trap 3: a pending wrap wraps the line and **returns**, consuming
    /// the tab.
    pub fn put_tab(&mut self, count: u16, anchors: &mut Anchors) {
        if self.cursor.pending_wrap {
            self.wrapline(anchors);
            return;
        }
        let last_col = self.cols - 1;
        for _ in 0..count {
            // A tab cell is only recorded where nothing was written, so a tab
            // never overwrites text (R-12).
            let id = self.cursor.pos.row;
            let col = self.cursor.pos.col;
            let existing = self.row(id).cell(col);
            if matches!(existing.content(), CellContent::Scalar(' ')) {
                // Only the content changes, exactly as the reference's `put_tab`
                // assigns `cell.c`. Keeping the width matters: the spacer halves
                // of a wide pair also read as a space, and replacing one with a
                // narrow tab cell would orphan its glyph.
                self.row_mut(id)
                    .set(col, existing.with_content(CellContent::Scalar('\t')));
            }
            while self.cursor.pos.col < last_col {
                self.cursor.pos.col += 1;
                if self.tabs.is_stop(self.cursor.pos.col) {
                    break;
                }
            }
        }
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    // ── Print path ──────────────────────────────────────────────────────────

    /// Place one scalar at the cursor.
    ///
    /// Width is decided per scalar, which is what the engine being replaced does
    /// and what the parity corpus pins; a zero-width scalar joins the previous
    /// cell's cluster instead of taking a column of its own.
    pub fn print(
        &mut self,
        c: char,
        mode: PrintMode,
        interner: &mut Interner,
        anchors: &mut Anchors,
    ) {
        let Some(width) = scalar_width(c) else {
            return;
        };
        if width == 0 {
            self.attach_zero_width(c, interner);
            self.debug_assert_integrity();
            return;
        }
        if self.cursor.pending_wrap {
            self.wrapline(anchors);
        }
        let (id, col) = (self.cursor.pos.row, self.cursor.pos.col);
        // The reference's own gate: at the columns where the shift would move
        // nothing it does not run at all, so the row is not opened, stamped or
        // swept (M9).
        if mode.insert && col + u16::from(width) < self.cols {
            // Correction C4: the pair the shift splits is repaired, where the
            // reference leaves an orphaned spacer.
            self.row_mut(id).shift_right_from(col, u16::from(width));
        }
        if width == 2 {
            if self.cols < 2 {
                self.dropped_wide += 1;
                return;
            }
            if self.cursor.pos.col + 1 >= self.cols {
                if !mode.autowrap {
                    // Deviation G3: the pending-wrap flag is not armed with
                    // `DECAWM` reset, so the glyph is simply dropped.
                    self.dropped_wide += 1;
                    return;
                }
                // Through the repairing write, exactly as the reference routes
                // it: the last column may already hold the `WideSpacer` of a
                // pair, and overwriting it with `set` would leave the `Wide` at
                // `cols - 2` orphaned — the state `assert_integrity` forbids.
                self.write_at_cursor(CellContent::Scalar(' '), CellWidth::LeadingWideSpacer);
                self.wrapline(anchors);
            }
            self.write_at_cursor(CellContent::Scalar(c), CellWidth::Wide);
            self.advance(mode);
            self.write_at_cursor(CellContent::Scalar(' '), CellWidth::WideSpacer);
        } else {
            self.write_at_cursor(CellContent::Scalar(c), CellWidth::Narrow);
        }
        self.advance(mode);
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    /// Trap 6, cross-row half: overwriting half of a wide pair in the first
    /// columns of a row also releases the previous row's trailing
    /// `LeadingWideSpacer`, whose glyph was this row's first cell.
    ///
    /// Both repairs are gated on the cell **being overwritten** carrying a pair,
    /// exactly as the reference gates them — otherwise the spacer a wrapping
    /// wide glyph has just placed would be released by its own glyph.
    fn write_at_cursor(&mut self, content: CellContent, width: CellWidth) {
        let (id, col) = (self.cursor.pos.row, self.cursor.pos.col);
        let overwriting_pair = matches!(
            self.row(id).cell(col).width(),
            CellWidth::Wide | CellWidth::WideSpacer
        );
        if overwriting_pair && col <= 1 && id > self.oldest {
            let previous = id - 1;
            let last = self.cols - 1;
            if self.row(previous).cell(last).width() == CellWidth::LeadingWideSpacer {
                let cell = self
                    .row(previous)
                    .cell(last)
                    .with_width(CellWidth::Narrow)
                    .with_content(CellContent::Scalar(' '));
                self.row_mut(previous).set(last, cell);
            }
        }
        let cell = self.cursor.template.with_content(content).with_width(width);
        // Same-row half of trap 6, through `cell::repair_wide_pair_in_row`.
        self.row_mut(id).write_repairing(col, cell);
    }

    fn advance(&mut self, mode: PrintMode) {
        if self.cursor.pos.col + 1 < self.cols {
            self.cursor.pos.col += 1;
        } else if mode.autowrap {
            self.cursor.pending_wrap = true;
        }
    }

    /// Trap 8: a zero-width scalar attaches to the cell to the left, or to
    /// column 0 itself when there is nothing to the left.
    fn attach_zero_width(&mut self, c: char, interner: &mut Interner) {
        let mut col = self.cursor.pos.col;
        if !self.cursor.pending_wrap {
            col = col.saturating_sub(1);
        }
        let id = self.cursor.pos.row;
        if col > 0 && self.row(id).cell(col).width() == CellWidth::WideSpacer {
            col -= 1;
        }
        let cell = self.row(id).cell(col);
        let mut cluster: Vec<char> = match cell.content() {
            CellContent::Scalar(base) => vec![base],
            CellContent::Grapheme(grapheme) => interner.resolve_grapheme(grapheme).to_vec(),
        };
        cluster.push(c);
        let grapheme = interner.grapheme(&cluster);
        self.row_mut(id)
            .set(col, cell.with_content(CellContent::Grapheme(grapheme)));
    }

    pub fn dropped_wide(&self) -> u32 {
        self.dropped_wide
    }

    // ── Erase, insert, delete ───────────────────────────────────────────────

    /// `EL`. Trap 2: `EL 0` erases nothing while the pending wrap is armed.
    pub fn erase_line(&mut self, mode: LineClear) {
        if matches!(mode, LineClear::Right) && self.cursor.pending_wrap {
            return;
        }
        let (col, cols) = (self.cursor.pos.col, self.cols);
        let range = match mode {
            LineClear::Right => col..cols,
            LineClear::Left => 0..col.saturating_add(1).min(cols),
            LineClear::All => 0..cols,
        };
        let (id, template) = (self.cursor.pos.row, self.cursor.erase);
        self.row_mut(id).fill(range, template);
        self.debug_assert_integrity();
    }

    /// `ECH`.
    pub fn erase_chars(&mut self, n: u16) {
        let (col, cols) = (self.cursor.pos.col, self.cols);
        let end = col.saturating_add(n).min(cols);
        let (id, template) = (self.cursor.pos.row, self.cursor.erase);
        self.row_mut(id).fill(col..end, template);
        self.debug_assert_integrity();
    }

    /// `DCH`, correction C1: a plain shift left by `n`.
    pub fn delete_chars(&mut self, n: u16) {
        let (id, col, template) = (self.cursor.pos.row, self.cursor.pos.col, self.cursor.erase);
        self.row_mut(id).delete_cells(col, n, template);
        self.debug_assert_integrity();
    }

    /// `ICH`.
    pub fn insert_blanks(&mut self, n: u16) {
        let (id, col, template) = (self.cursor.pos.row, self.cursor.pos.col, self.cursor.erase);
        self.row_mut(id).insert_cells(col, n, template);
        self.debug_assert_integrity();
    }

    /// `ED`.
    pub fn erase_display(
        &mut self,
        mode: DisplayClear,
        anchors: &mut Anchors,
        interner: &Interner,
    ) -> Option<ScrollReport> {
        let index = self.cursor_row_index();
        let (id, col, cols, template) = (
            self.cursor.pos.row,
            self.cursor.pos.col,
            self.cols,
            self.cursor.erase,
        );
        match mode {
            DisplayClear::Below => {
                self.row_mut(id).fill(col..cols, template);
                self.reset_rows(index + 1..self.rows);
                None
            }
            DisplayClear::Above => {
                // Correction C2: row 0 is cleared too, where the reference's
                // `cursor_row > 1` guard leaves it alone.
                self.reset_rows(0..index);
                self.row_mut(id)
                    .fill(0..col.saturating_add(1).min(cols), template);
                None
            }
            DisplayClear::All => match self.kind {
                ScreenKind::Alternate => {
                    self.reset_rows(0..self.rows);
                    None
                }
                ScreenKind::Primary => Some(self.clear_viewport(anchors, interner)),
            },
            DisplayClear::Saved => {
                self.clear_history(anchors);
                None
            }
        }
    }

    /// `ED 2` on the primary screen. Trap 9: the occupied part of the screen is
    /// scrolled into scrollback rather than discarded, and a user who is
    /// scrolled back keeps seeing the same content.
    pub fn clear_viewport(&mut self, anchors: &mut Anchors, interner: &Interner) -> ScrollReport {
        let positions = self.occupied_rows(interner);
        let region = ScrollRegion::full(self.rows);
        // The ordinary scroll rule already holds the scrolled-back view still:
        // the design's offset table says "offset unchanged" and its prose says
        // "the scrolled-back user keeps seeing the same content", and only the
        // second is achievable once the bottom moves. The reference agrees — its
        // `Grid::scroll_up` bumps `display_offset` here like any other scroll.
        let report = self.scroll_up(region, positions, anchors);
        self.reset_rows(0..self.rows - positions);
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
        report
    }

    /// How many rows from the top hold anything worth keeping.
    ///
    /// R-13: a cell carrying a graphic is not erasable, so `CSI 2 J` over an
    /// image scrolls the image into history instead of deciding it was blank.
    fn occupied_rows(&self, interner: &Interner) -> u16 {
        for index in (0..self.rows).rev() {
            let row = self.row(self.screen_top() + index as u64);
            if row.cells().iter().any(|cell| !cell.is_erasable(interner)) {
                return index + 1;
            }
        }
        0
    }

    /// `ED 3`. Trap 10: the view snaps back to the bottom.
    pub fn clear_history(&mut self, anchors: &mut Anchors) -> Option<RowId> {
        self.offset = 0;
        if self.history_len() == 0 {
            self.debug_assert_integrity();
            return None;
        }
        let top = self.screen_top();
        let mut id = self.oldest;
        while id < top {
            let slot = self.slot_of(id);
            self.slots[slot] = None;
            id = id + 1;
        }
        self.oldest = top;
        anchors.trim(self.kind.origin(), self.oldest);
        self.clamp_cursors();
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
        Some(self.oldest)
    }

    /// `RIS` / the alternate screen's entry wipe. Ids are never reset.
    pub fn reset(&mut self, anchors: &mut Anchors) {
        self.clear_history(anchors);
        self.cursor = Cursor::at(self.screen_top());
        self.saved_cursor = self.cursor;
        self.region = ScrollRegion::full(self.rows);
        self.tabs = TabStops::new(self.cols);
        self.offset = 0;
        self.reset_rows(0..self.rows);
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    /// Wipe every row with the current erase cell, keeping the scroll region,
    /// the tab stops and the cursor.
    ///
    /// This is the alternate screen's entry wipe — the reference's
    /// `inactive_grid.reset_region(..)`, which is a background-erase clear and
    /// emphatically not a `RIS`.
    pub fn clear_all_rows(&mut self, anchors: &mut Anchors) {
        let rows = self.rows_of(ScrollRegion::full(self.rows));
        anchors.shift_region(rows.clone(), 0, rows);
        self.offset = 0;
        self.reset_rows(0..self.rows);
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    /// Adopt another screen's scroll region and tab stops.
    ///
    /// The reference keeps one `scroll_region` and one `tabs` table on `Term`,
    /// shared by both screens. Here each screen owns its own, so the swap copies
    /// them across in both directions, which is observationally the same thing.
    pub(crate) fn adopt_region_and_tabs(&mut self, other: &Screen) {
        self.region = self.clamp_region(other.region);
        self.tabs = other.tabs.clone();
        self.tabs.resize(self.cols);
    }

    // ── Resize ──────────────────────────────────────────────────────────────
    //
    // The column half and the two policies live in `crate::reflow`, which drives
    // the entry points below. What stays here is everything that touches the
    // ring: its session-constant mask (R-30), the scroll-region reset (trap 28)
    // and the tab-stop regrow rule (trap 27).

    /// The rows-only half of a resize, with `BottomAnchor` semantics.
    ///
    /// Grow pulls what history has into the top and appends the rest; shrink
    /// pushes `(cursor_row_index + 1) - rows` rows into history when that is
    /// positive and drops the remainder off the bottom, which is what the
    /// reference's `shrink_lines` does with its rotate.
    ///
    /// Trap 30, R-06: the viewport is restated from the row that held the first
    /// visible character, and a viewport at the bottom stays at the bottom.
    pub(crate) fn resize_rows(&mut self, rows: u16, anchors: &mut Anchors) -> u32 {
        let rows = rows.clamp(1, MAX_ROWS);
        let visible_top = self.visible_top();
        let sticky = self.offset == 0;
        match rows.cmp(&self.rows) {
            Ordering::Greater => {
                // The new height is applied before the append, so the pull *is*
                // the height change and the trim measures history against it.
                let added = rows - self.rows;
                let pulled = self.history_len().min(u32::from(added)) as u16;
                self.rows = rows;
                let deficit = added - pulled;
                if deficit > 0 {
                    self.push_rows_with(deficit, Cell::EMPTY);
                }
            }
            Ordering::Less => {
                let lost = self.rows - rows;
                let pushed = (self.cursor_row_index() + 1).saturating_sub(rows);
                self.rows = rows;
                // Lowering `rows` alone moves the whole screen window down by
                // `lost`; dropping the rows that should have come off the bottom
                // instead of entering history moves it back up.
                self.drop_trailing_rows(lost - pushed.min(lost), anchors);
            }
            Ordering::Equal => {}
        }
        // Rows pushed into history by a shrink can take it over its limit, and
        // the alternate screen has no history at all (trap 32).
        let trimmed = self.trim_history(anchors);
        // Trap 28: a resize unconditionally resets the scroll region.
        self.region = ScrollRegion::full(self.rows);
        self.clamp_cursors();
        self.restate_viewport(visible_top, sticky);
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
        trimmed
    }

    /// Drop rows off the oldest end until history is inside its limit.
    fn trim_history(&mut self, anchors: &mut Anchors) -> u32 {
        let mut trimmed = 0;
        while self.history_len() > self.scrollback_limit {
            let slot = self.slot_of(self.oldest);
            self.slots[slot] = None;
            self.oldest = self.oldest + 1;
            trimmed += 1;
        }
        if trimmed > 0 {
            anchors.trim(self.kind.origin(), self.oldest);
            self.offset = self.offset.min(self.history_len());
        }
        trimmed
    }

    /// The alternate screen's column change: truncate or pad, never reflow
    /// (trap 29). Only the columns of live rows and of every anchor in this
    /// screen's lane move.
    pub(crate) fn truncate_columns(&mut self, cols: u16, anchors: &mut Anchors) {
        let cols = cols.clamp(1, MAX_COLS);
        if cols == self.cols {
            return;
        }
        let (seq, range) = (self.seq, self.row_range());
        let mut id = range.start;
        while id < range.end {
            let slot = self.slot_of(id);
            if let Some(row) = &mut self.slots[slot]
                && row.id() == id
            {
                RowMut::new(row, seq).resize(cols, Cell::EMPTY, seq);
            }
            id = id + 1;
        }
        self.cols = cols;
        self.tabs.resize(cols);
        // An anchor must never point past the last column. Lane-scoped, because
        // `Anchors` is shared by both screens and only this one changed width.
        let (last, lane) = (cols - 1, self.row_range());
        anchors.remap(|pos| {
            let col = if lane.contains(&pos.row) {
                pos.col.min(last)
            } else {
                pos.col
            };
            Some(Pos { row: pos.row, col })
        });
        self.clamp_cursors();
        self.debug_assert_integrity();
    }

    /// Replace every live row with the reflow's output and adopt the new width.
    ///
    /// `rows` is oldest first and must not be empty; ids are allocated fresh
    /// above the current newest, which is what `DEC-0015` means by "reflow: every
    /// row gets a fresh id". The caller has already capped the count at
    /// `scrollback_limit + rows`, so the write always fits the ring.
    pub(crate) fn install_rows(&mut self, rows: Vec<Option<Row>>, cols: u16) {
        debug_assert!(!rows.is_empty(), "reflow produced no rows");
        debug_assert!(
            rows.len() <= self.slots.len(),
            "reflow produced more rows than the ring holds"
        );
        let base = self.newest + 1;
        let (seq, len) = (self.seq, rows.len() as u64);
        self.cols = cols;
        self.tabs.resize(cols);
        for (step, row) in rows.into_iter().enumerate() {
            let id = base + step as u64;
            let slot = self.slot_of(id);
            self.slots[slot] = row.map(|row| row.take_rehomed(id, seq));
        }
        self.oldest = base;
        self.newest = base + (len - 1);
    }

    /// Append `n` blank rows at the bottom without moving either cursor.
    ///
    /// The rows already on the screen keep their ids, so lowering the screen
    /// window is exactly "scroll the whole screen up by `n`": the top rows
    /// return to history and every anchor, cursor included, loses `n` from its
    /// screen index. `US-0077`'s positive `KeepViewportTop` shift.
    pub(crate) fn append_blank_rows(&mut self, n: u16, anchors: &mut Anchors) {
        if n == 0 {
            return;
        }
        if self.push_rows_with(n, Cell::EMPTY).is_some() {
            anchors.trim(self.kind.origin(), self.oldest);
        }
        self.clamp_cursors();
        self.offset = self.offset.min(self.history_len());
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    /// Drop `n` rows off the bottom, pulling the same number back out of
    /// history into the screen. The negative `KeepViewportTop` shift, and the
    /// part of a row shrink that must not enter history.
    pub(crate) fn drop_trailing_rows(&mut self, n: u16, anchors: &mut Anchors) {
        let n = u64::from(n)
            .min(u64::from(self.history_len()))
            .min(self.newest.distance(self.cursor.pos.row));
        if n == 0 {
            return;
        }
        let kill = self.newest - (n - 1)..self.newest + 1;
        let mut id = kill.start;
        while id < kill.end {
            // These ids leave the live range but a later grow re-creates them
            // (`push_rows_with` walks up from the same `newest`), so a row that
            // was written is blanked in place rather than dropped: the stamp is
            // what tells a consumer the id came back empty.
            self.blank_slot(id, Cell::EMPTY);
            id = id + 1;
        }
        anchors.shift_region(kill.clone(), 0, kill);
        self.newest = self.newest - n;
        // A logical line must never dangle past the end of the buffer: the row
        // that has just become the bottom one has nothing left to continue onto.
        let newest = self.newest;
        if self.row(newest).wrapped() {
            self.row_mut(newest).set_wrapped(false);
        }
        self.offset = self.offset.min(self.history_len());
        self.clamp_cursors();
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    /// Put the viewport back on the row that held the first visible character.
    ///
    /// R-06: "the top row is unchanged" is not well formed once reflow gives
    /// every row a fresh id, so the invariant is stated about the character
    /// instead. A viewport that was at the bottom stays at the bottom.
    pub(crate) fn restate_viewport(&mut self, visible_top: RowId, sticky: bool) {
        self.offset = if sticky {
            0
        } else {
            let distance = self.screen_top().distance(visible_top);
            distance.min(u64::from(self.history_len())) as u32
        };
    }

    pub(crate) fn set_scroll_offset(&mut self, offset: u32) {
        self.offset = offset.min(self.history_len());
    }

    /// Install the cursor the reflow read back out of the anchor list.
    ///
    /// **Only `reflow::read_back` may call this**, and only in the window
    /// between `Anchors::remap` and the next [`Screen::sync_anchors`]. It writes
    /// the field without writing the entry, which is exactly the staleness the
    /// read-back exists to repair: called anywhere else it would recreate it.
    /// Every other cursor move goes through [`Screen::goto`] and its siblings.
    pub(crate) fn restate_cursor_after_reflow(&mut self, pos: Pos, pending_wrap: bool) {
        self.cursor.pos = pos;
        self.cursor.pending_wrap = pending_wrap;
        self.clamp_cursors();
    }

    /// As [`Screen::restate_cursor_after_reflow`], for the `DECSC` slot, and
    /// under the same restriction.
    pub(crate) fn restate_saved_cursor_after_reflow(&mut self, pos: Pos) {
        self.saved_cursor.pos = pos;
        self.clamp_cursors();
    }

    /// The one rehome: the user edited the configured scrollback depth. O(live
    /// rows), and deliberately not on the resize path (R-30).
    pub fn set_scrollback_limit(&mut self, limit: u32, anchors: &mut Anchors) {
        let limit = limit.min(crate::grid::SCROLLBACK_MAX);
        if limit == self.scrollback_limit {
            return;
        }
        while self.history_len() > limit {
            let slot = self.slot_of(self.oldest);
            self.slots[slot] = None;
            self.oldest = self.oldest + 1;
        }
        let ring_len = ring_len_for(limit);
        if ring_len != self.slots.len() {
            let mut slots: Vec<Option<Row>> = vec![None; ring_len];
            let mask = ring_len - 1;
            let range = self.row_range();
            let mut id = range.start;
            while id < range.end {
                if let Some(row) = self.take_row(id) {
                    slots[(id.0 as usize) & mask] = Some(row);
                }
                id = id + 1;
            }
            self.slots = slots.into_boxed_slice();
            self.mask = mask;
        }
        self.scrollback_limit = limit;
        self.offset = self.offset.min(self.history_len());
        anchors.trim(self.kind.origin(), self.oldest);
        self.sync_anchors(anchors);
        self.debug_assert_integrity();
    }

    pub fn scrollback_limit(&self) -> u32 {
        self.scrollback_limit
    }

    pub fn ring_len(&self) -> usize {
        self.slots.len()
    }

    pub fn ring_mask(&self) -> usize {
        self.mask
    }

    // ── Batch stamping ──────────────────────────────────────────────────────

    pub fn seq(&self) -> SeqNo {
        self.seq
    }

    pub fn set_seq(&mut self, seq: SeqNo) {
        self.seq = seq;
    }

    // ── Anchors ─────────────────────────────────────────────────────────────

    /// Write the three screen-owned positions back into the anchor list.
    ///
    /// The fields are the authority under the scroll primitives (the cursor
    /// keeps its place on the screen where a content anchor follows its
    /// content); the entries are the authority under reflow, which reads them
    /// back through `Anchors::remap`.
    pub fn sync_anchors(&self, anchors: &mut Anchors) {
        anchors.set(self.cursor_anchor, self.cursor.pos);
        anchors.set(self.saved_cursor_anchor, self.saved_cursor.pos);
        anchors.set(
            self.viewport_anchor,
            Pos {
                row: self.visible_top(),
                col: 0,
            },
        );
    }

    pub fn cursor_anchor(&self) -> AnchorId {
        self.cursor_anchor
    }

    pub fn saved_cursor_anchor(&self) -> AnchorId {
        self.saved_cursor_anchor
    }

    pub fn viewport_anchor(&self) -> AnchorId {
        self.viewport_anchor
    }

    // ── Text ────────────────────────────────────────────────────────────────

    /// Append one row's text, skipping spacers and expanding graphemes.
    pub fn row_text(&self, id: RowId, graphemes: &GraphemeArena, out: &mut String) {
        for cell in self.row(id).cells() {
            if cell.width().is_spacer() {
                continue;
            }
            match cell.content() {
                CellContent::Scalar(c) => out.push(c),
                CellContent::Grapheme(grapheme) => out.extend(graphemes.resolve(grapheme)),
            }
        }
    }

    // ── Memory ──────────────────────────────────────────────────────────────

    /// Live heap this screen owns: the ring's slots, the rows that were actually
    /// written, and the tab bitmap.
    ///
    /// This is the physical half of the design's memory claim, read off the
    /// structure's own capacities rather than off a counting allocator — a
    /// `GlobalAlloc` is an `unsafe` trait and this crate has none.
    pub fn heap_bytes(&self) -> usize {
        self.slots.len() * size_of::<Option<Row>>()
            + self.slots.iter().flatten().map(Row::bytes).sum::<usize>()
            + self.tabs.heap_bytes()
    }

    /// Rows whose cells are actually materialised.
    pub fn allocated_rows(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    // ── Integrity ───────────────────────────────────────────────────────────

    /// The O(1) check every mutating method runs in debug builds (R-28): the
    /// counters are ordered, the cursor is inside the screen, and the viewport
    /// offset is inside history. The full walk is [`Screen::assert_integrity`],
    /// which `feed`, `resize`, `render_update` and the property tests run.
    pub(crate) fn debug_assert_integrity(&self) {
        if !cfg!(debug_assertions) {
            return;
        }
        debug_assert!(self.oldest <= self.newest, "row ids are out of order");
        debug_assert!(
            self.newest.distance(self.oldest) + 1 >= u64::from(self.rows),
            "fewer live rows than the screen height"
        );
        debug_assert!(
            self.newest.distance(self.oldest) < self.slots.len() as u64,
            "live rows exceed the ring"
        );
        debug_assert!(
            self.offset <= self.history_len(),
            "viewport offset is past the oldest row"
        );
        debug_assert!(
            self.cursor.pos.row >= self.screen_top() && self.cursor.pos.row <= self.newest,
            "cursor is outside the screen"
        );
        debug_assert!(
            self.cursor.pos.col < self.cols,
            "cursor is past the last column"
        );
    }

    /// The full walk. Every row's id matches its slot, every row is the right
    /// width, no wide pair is broken, and no content hint has a false negative.
    pub fn assert_integrity(&self) {
        self.debug_assert_integrity();
        if !cfg!(debug_assertions) {
            return;
        }
        let origin = self.kind.origin();
        debug_assert!(self.oldest >= origin, "row id below this screen's origin");
        let mut id = self.oldest;
        while id <= self.newest {
            let row = self.row(id);
            debug_assert_eq!(row.id(), id, "row id does not match its slot");
            if row.is_allocated() {
                debug_assert_eq!(
                    row.cells().len(),
                    self.cols as usize,
                    "row {id:?} is the wrong width"
                );
                assert_wide_pairs(row.cells(), id);
                assert_no_false_negative(row.flags(), row.cells(), id);
            }
            id = id + 1;
        }
        debug_assert!(
            self.saved_cursor.pos.col < self.cols,
            "saved cursor is past the last column"
        );
    }

    /// Every interned id a live cell names still resolves. Split out because it
    /// needs the terminal's interner, which a screen does not own.
    pub fn assert_interned_ids_resolve(&self, interner: &Interner) {
        if !cfg!(debug_assertions) {
            return;
        }
        let mut id = self.oldest;
        while id <= self.newest {
            for cell in self.row(id).cells() {
                debug_assert!(
                    (cell.style_id().0 as usize) < interner.styles.entries(),
                    "style id {:?} was never issued",
                    cell.style_id()
                );
                debug_assert!(
                    (cell.extras_id().0 as usize) < interner.extras.entries(),
                    "extras id {:?} was never issued",
                    cell.extras_id()
                );
                if let CellContent::Grapheme(grapheme) = cell.content() {
                    debug_assert!(
                        !interner.resolve_grapheme(grapheme).is_empty(),
                        "grapheme id {grapheme:?} does not resolve"
                    );
                }
            }
            id = id + 1;
        }
    }
}

/// Deliberately summarised: a screen holds up to a million slots, and the whole
/// ring in a panic message helps nobody.
impl std::fmt::Debug for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screen")
            .field("kind", &self.kind)
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .field("oldest", &self.oldest)
            .field("newest", &self.newest)
            .field("history_len", &self.history_len())
            .field("offset", &self.offset)
            .field("region", &self.region)
            .field("cursor", &self.cursor.pos)
            .field("pending_wrap", &self.cursor.pending_wrap)
            .field("ring_len", &self.slots.len())
            .field("allocated_rows", &self.allocated_rows())
            .finish()
    }
}

fn assert_wide_pairs(cells: &[Cell], id: RowId) {
    for (col, cell) in cells.iter().enumerate() {
        match cell.width() {
            CellWidth::Wide => debug_assert!(
                cells
                    .get(col + 1)
                    .is_some_and(|next| next.width() == CellWidth::WideSpacer),
                "wide cell without its spacer at {id:?}:{col}"
            ),
            CellWidth::WideSpacer => debug_assert!(
                col > 0 && cells[col - 1].width() == CellWidth::Wide,
                "spacer without its wide cell at {id:?}:{col}"
            ),
            CellWidth::Narrow | CellWidth::LeadingWideSpacer => {}
        }
    }
}

fn assert_no_false_negative(flags: RowFlags, cells: &[Cell], id: RowId) {
    let styled = cells.iter().any(|cell| cell.style_id() != StyleId::DEFAULT);
    debug_assert!(
        !styled || flags.contains(RowFlags::STYLED),
        "row {id:?} carries a style but is not flagged"
    );
    let grapheme = cells
        .iter()
        .any(|cell| matches!(cell.content(), CellContent::Grapheme(_)));
    debug_assert!(
        !grapheme || flags.contains(RowFlags::HAS_GRAPHEME),
        "row {id:?} carries a grapheme but is not flagged"
    );
    let extras = cells
        .iter()
        .any(|cell| cell.extras_id() != crate::intern::ExtrasId::NONE);
    debug_assert!(
        !extras || flags.contains(RowFlags::HAS_EXTRAS),
        "row {id:?} carries extras but is not flagged"
    );
}
