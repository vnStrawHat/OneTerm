//! The frame: one `Terminal::snapshot_update` into the snapshot state this buffer
//! owns.
//!
//! `TerminalContent` **is** the consumer `DEC-0015` describes. It owns the
//! [`SnapshotState`] and therefore the damage watermark, so "changed for me"
//! means changed since *this* buffer last looked, and a second consumer needs
//! no engine change. The render path reuses one buffer
//! (`terminal-view/src/render/frame.rs`), so the watermark survives across
//! frames exactly where it belongs; a freshly built `TerminalContent` reports
//! `Full`, which is correct by construction rather than by convention.
//!
//! Since `US-0085` there is nothing else in it. The dense `Vec<IndexedCell>`
//! in the reference's signed grid lines, the forked engine's value types
//! around it and the per-frame rebuild that produced them are gone; the view
//! reads [`TerminalContent::rows`] and resolves the engine's own
//! [`SnapshotRow`] / [`SnapshotCell`] itself.

use std::sync::Arc;
use std::time::Instant;

use oneterm_vt::grid::RowFlags;
use oneterm_vt::{
    Attrs, CellWidth, Color, CursorShape, GraphicData, GraphicId, Hyperlink, HyperlinkId,
    ModeSnapshot, NamedColor, RowId, SelectionRange, Size, SnapshotCursor, SnapshotPlacement,
    SnapshotRow, SnapshotState, SnapshotUpdate, Terminal,
};

/// A blank cell = space + default background + no decoration (hyperlink,
/// underline, inverse…). Matches the UI's `is_blank` definition so the gutter
/// and stamping agree on which lines have content.
///
/// Read straight off the engine's packed cell: `last_content_row` runs on every
/// `terminal_info()` call, and building a frame to answer it would copy a
/// viewport per gutter update.
fn is_blank_cell(cell: oneterm_vt::Cell, term: &Terminal) -> bool {
    let style = term.interner().resolve_style(cell.style_id());
    cell.text_char(&term.interner().graphemes) == ' '
        && style.bg == Color::Named(NamedColor::Background)
        && cell.width() != CellWidth::WideSpacer
        && !style
            .attrs
            .intersects(Attrs::INVERSE | Attrs::ALL_UNDERLINES | Attrs::STRIKEOUT)
        && term
            .interner()
            .resolve_extras(cell.extras_id())
            .hyperlink
            .is_none()
}

// Cells `last_content_row` examined, for the counted-work test (`US-0092`).
#[cfg(test)]
thread_local! {
    pub(crate) static CELLS_EXAMINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Index (0-based from the top of the active screen) of the last row **with
/// content**. Returns `0` when the whole screen is blank.
///
/// Used for `line_times` stamping: the gutter renders up to the last non-blank
/// line, so timestamps must be stamped up to there too; otherwise lines below
/// the cursor (TUI, progress bars using cursor-up…) show `[--:--:--]`.
pub fn last_content_row(term: &Terminal) -> usize {
    let screen = term.screen();
    let rows = screen.rows();
    let top = screen.screen_top();
    for index in (0..rows).rev() {
        let row = screen.row(top + u64::from(index));
        // `RowHeader.occ` is "the reference's over-approximating hint: no column
        // at or above `occ` has been touched since the last reset"
        // (`crates/vt/src/grid/row.rs:61-63`). False positives are allowed,
        // never false negatives — exactly what a skip needs, and what keeps
        // this O(content) rather than O(viewport) on an idle screen where every
        // row below the prompt is blank (`US-0092`).
        //
        // **`occ == 0` is "untouched since the reset", not "blank"**, and the
        // two part company at a background-colour erase: `Row::reset` fills the
        // row with the erase template and *then* zeroes `occ`
        // (`crates/vt/src/grid/row.rs:161-171`), so after `CSI 44 m` + `ED` — or
        // any scroll, `IL`, `DL` under a non-default background, which is what
        // every full-screen TUI does — the cells carry `bg = Blue` while `occ`
        // reads 0. `is_blank_cell` calls those cells content, so skipping them
        // loses the gutter timestamps on painted lines (`US-0092` verification,
        // defect 1). `reset` also sets `flags_for(template)`, so the row itself
        // says the template was not plain: the content hints answer this for
        // free, and they over-approximate in the safe direction.
        let styled = row
            .flags()
            .intersects(RowFlags::STYLED | RowFlags::HAS_EXTRAS | RowFlags::HAS_GRAPHEME);
        let occ = usize::from(row.occ());
        if !row.is_allocated() || (occ == 0 && !styled) {
            continue;
        }
        // Same reason the skip is gated: with a styled erase template the cells
        // above `occ` are that template, not blanks, so the narrowing only holds
        // when the row carries no content hint.
        let cells = row.cells();
        let cells = if styled {
            cells
        } else {
            &cells[..occ.min(cells.len())]
        };
        #[cfg(test)]
        CELLS_EXAMINED.with(|n| n.set(n.get() + cells.len()));
        if cells.iter().any(|cell| !is_blank_cell(*cell, term)) {
            return usize::from(index);
        }
    }
    0
}

/// One cell of a damage-free line-range read
/// ([`crate::TerminalRender::query_line_range_cells`]).
///
/// Owned and deliberately narrow: this is the pointer-move path (URL hover) and
/// the completion lookup, which read a handful of rows and want none of the
/// style machinery a painted frame needs. A frame goes through
/// [`TerminalContent`] instead.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentCell {
    /// The cell's text as one scalar: `' '` for a blank, `'\t'` for a tab cell,
    /// the base scalar of a grapheme cluster.
    pub ch: char,
    /// This column carries no glyph of its own — the second half of a wide pair,
    /// or the blank left where a wide glyph did not fit before a wrap.
    pub spacer: bool,
    /// This is the last column of a row whose logical line continues on the
    /// next one.
    pub wrapline: bool,
    /// The OSC 8 link this cell belongs to, if any. Identity only: the target
    /// comes from [`LineRangeCells::hyperlink_uri`], because one run of cells
    /// shares one link.
    pub hyperlink: Option<HyperlinkId>,
}

/// Cells for a window of display lines — see
/// [`crate::TerminalRender::query_line_range_cells`].
#[derive(Debug, Clone, Default)]
pub struct LineRangeCells {
    /// Up to `count × num_cols` cells starting at the requested display line,
    /// in row-major order. Empty when the range starts below the viewport.
    pub cells: Vec<ContentCell>,
    /// Viewport width in columns; the row stride of `cells`.
    pub num_cols: usize,
    /// The OSC 8 targets the cells reference, one entry per distinct link.
    pub links: Vec<(HyperlinkId, String)>,
}

impl LineRangeCells {
    /// The target of an OSC 8 link a cell carries.
    pub fn hyperlink_uri(&self, id: HyperlinkId) -> Option<&str> {
        self.links
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, uri)| uri.as_str())
    }
}

/// One frame, as the engine hands it over.
pub struct TerminalContent {
    /// The frame source. Owns this consumer's damage watermark.
    state: SnapshotState,
    /// What the last [`TerminalContent::refill`] did.
    update: SnapshotUpdate,
    /// `DECSCUSR`. The snapshot state carries where the cursor is and whether it
    /// is visible, not what it looks like, so the shape is copied beside it.
    cursor_shape: CursorShape,
    /// Scrollback + viewport, for the scrollbar.
    total_lines: usize,
    /// Images decoded since the previous frame, each handed out once; the cells
    /// reference them through [`SnapshotCell::graphic`].
    graphics: Vec<Arc<GraphicData>>,
}

impl std::fmt::Debug for TerminalContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalContent")
            .field("update", &self.update)
            .field("rows", &self.state.rows().len())
            .field("changed", &self.state.changed().len())
            .field("size", &self.state.size())
            .field("cursor", self.state.cursor())
            .field("cursor_shape", &self.cursor_shape)
            .field("modes", &self.state.modes())
            .field("selection", &self.state.selection())
            .field("scroll_offset", &self.state.scroll_offset())
            .field("total_lines", &self.total_lines)
            .field("graphics_len", &self.graphics.len())
            .finish()
    }
}

impl Default for TerminalContent {
    /// An empty frame — the initial value for the reusable buffer passed to
    /// [`TerminalContent::refill`]. Its watermark is fresh, so the first refill
    /// reports `Full`.
    fn default() -> Self {
        Self {
            state: SnapshotState::new(),
            update: SnapshotUpdate::Full,
            cursor_shape: CursorShape::Hidden,
            total_lines: 0,
            graphics: Vec::new(),
        }
    }
}

impl TerminalContent {
    /// Build a frame from the engine (the caller handles locking).
    ///
    /// Allocating convenience over [`refill`](Self::refill) — the render path
    /// reuses one buffer instead. A fresh buffer has a fresh watermark, so the
    /// first result is always `Full` and no other consumer's damage moves.
    pub fn from(term: &mut Terminal) -> Self {
        let mut content = Self::default();
        content.refill(term);
        content
    }

    /// Refill `self` from the engine, reusing every allocation — the
    /// steady-state render loop allocates nothing (PERF).
    ///
    /// Advances **this buffer's** watermark: the next call reports only what
    /// changed after this one.
    pub fn refill(&mut self, term: &mut Terminal) {
        self.update = term.snapshot_update(&mut self.state, Instant::now());
        self.cursor_shape = term.cursor_style().shape;
        let screen = term.screen();
        self.total_lines = screen.history_len() as usize + usize::from(screen.rows());
        // The engine is the one drain (R-16): `snapshot_update` never takes the
        // pixels, so the adapter does, right here, once per frame.
        self.graphics.clear();
        self.graphics.extend(term.take_graphics());
    }

    /// What the last [`refill`](Self::refill) did: nothing, a shift plus
    /// [`changed`](Self::changed), or everything.
    pub fn update(&self) -> SnapshotUpdate {
        self.update
    }

    /// The full viewport, indexed by display row — always, whatever
    /// [`update`](Self::update) says (R-15).
    pub fn rows(&self) -> &[SnapshotRow] {
        self.state.rows()
    }

    /// The display rows the last refill copied.
    pub fn changed(&self) -> &[u16] {
        self.state.changed()
    }

    /// The viewport size in cells.
    pub fn size(&self) -> Size {
        self.state.size()
    }

    /// Where the cursor is and whether it is visible, refreshed every refill.
    pub fn render_cursor(&self) -> &SnapshotCursor {
        self.state.cursor()
    }

    /// `DECSCUSR`: what the cursor looks like.
    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor_shape
    }

    /// true when the cursor is visible (`DECTCEM` set and the shape is not
    /// `Hidden`).
    pub fn cursor_visible(&self) -> bool {
        self.state.cursor().visible && self.cursor_shape != CursorShape::Hidden
    }

    /// The modes the view reads at paint time, refreshed every refill.
    pub fn modes(&self) -> ModeSnapshot {
        self.state.modes()
    }

    /// The selection in engine coordinates.
    pub fn selection_range(&self) -> Option<SelectionRange> {
        self.state.selection()
    }

    /// Live image placements, ids and geometry only — the pixels are in
    /// [`graphics`](Self::graphics), drained once.
    ///
    /// `cols` x `rows` is the image's footprint in cells, which the engine
    /// derived from its pixels and the cell size pushed through
    /// [`TerminalSession::set_cell_pixels`](crate::TerminalSession::set_cell_pixels)
    /// (`BUG-0062`). Paint the image at `pixel_size`, clipped to that footprint;
    /// do not scale it to the footprint.
    pub fn placements(&self) -> &[SnapshotPlacement] {
        self.state.placements()
    }

    /// The cell's `(col, row)` offset inside the image's own cell grid.
    pub fn graphic_offset(&self, id: GraphicId, row: RowId, col: u16) -> Option<(u16, u16)> {
        self.state.graphic_offset(id, row, col)
    }

    /// Images decoded since the previous refill; each appears exactly once.
    pub fn graphics(&self) -> &[Arc<GraphicData>] {
        &self.graphics
    }

    /// The strings behind a cell's hyperlink id, resolved under the lock.
    pub fn hyperlink(&self, id: HyperlinkId) -> Option<&Hyperlink> {
        self.state.hyperlink(id)
    }

    /// Rows between the viewport and the newest row; `0` is the sticky bottom.
    pub fn scroll_offset(&self) -> u32 {
        self.state.scroll_offset()
    }

    /// Scrollback + viewport, for the scrollbar.
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    /// The first visible row.
    pub fn viewport_top(&self) -> RowId {
        self.state.viewport_top()
    }

    /// The `RowId` of a display row — the half of the two-way translation that
    /// a consumer keying a cache on row identity needs (the view's
    /// `plan_cache`).
    pub fn row_id(&self, display_row: usize) -> Option<RowId> {
        self.state.rows().get(display_row).map(|row| row.id)
    }

    /// The display row a `RowId` currently sits on, or `None` when it has
    /// scrolled out of the viewport.
    pub fn display_row(&self, id: RowId) -> Option<usize> {
        let top = self.state.viewport_top();
        if id < top {
            return None;
        }
        let row = id.distance(top) as usize;
        (row < self.state.rows().len()).then_some(row)
    }

    /// The visible text, row by row, trailing blanks included — what a log
    /// line or a test means by "what the screen says".
    pub fn text(&self) -> String {
        let mut out = String::new();
        for row in self.state.rows() {
            for cell in &row.cells {
                out.push(match cell.content {
                    oneterm_vt::SnapshotContent::Scalar(scalar) => scalar,
                    oneterm_vt::SnapshotContent::Cluster { start, len } => {
                        row.cluster(start, len).first().copied().unwrap_or(' ')
                    }
                });
            }
        }
        out
    }

    /// Force the next [`refill`](Self::refill) to rebuild everything.
    pub fn invalidate(&mut self) {
        self.state.invalidate();
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
