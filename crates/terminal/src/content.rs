//! The frame: one `Terminal::render_update` into the render state this buffer
//! owns.
//!
//! `TerminalContent` **is** the consumer `DEC-0015` describes. It owns the
//! [`RenderState`] and therefore the damage watermark, so "changed for me"
//! means changed since *this* buffer last looked, and a second consumer needs
//! no engine change. The render path reuses one buffer
//! (`terminal-view/src/render/frame.rs`), so the watermark survives across
//! frames exactly where it belongs; a freshly built `TerminalContent` reports
//! `Full`, which is correct by construction rather than by convention.
//!
//! Since `US-0085` there is nothing else in it. The dense `Vec<IndexedCell>`
//! in the reference's signed grid lines, the `alacritty_terminal` value types
//! around it and the per-frame rebuild that produced them are gone; the view
//! reads [`TerminalContent::rows`] and resolves the engine's own
//! [`RenderRow`] / [`RenderCell`] itself.

use std::sync::Arc;
use std::time::Instant;

use oneterm_vt::intern::Hyperlink;
use oneterm_vt::render::{ModeSnapshot, RenderCursor, RenderPlacement, RenderRow};
use oneterm_vt::{
    Attrs, CellWidth, Color, CursorShape, GraphicData, GraphicId, HyperlinkId, NamedColor,
    RenderState, RenderUpdate, RowId, SelectionRange, Size, Terminal,
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
        if row.cells().iter().any(|cell| !is_blank_cell(*cell, term)) {
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
pub struct SnapshotCell {
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
    pub cells: Vec<SnapshotCell>,
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
    state: RenderState,
    /// What the last [`TerminalContent::refill`] did.
    update: RenderUpdate,
    /// `DECSCUSR`. The render state carries where the cursor is and whether it
    /// is visible, not what it looks like, so the shape is copied beside it.
    cursor_shape: CursorShape,
    /// Scrollback + viewport, for the scrollbar.
    total_lines: usize,
    /// Images decoded since the previous frame, each handed out once; the cells
    /// reference them through [`RenderCell::graphic`].
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
            state: RenderState::new(),
            update: RenderUpdate::Full,
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
        self.update = term.render_update(&mut self.state, Instant::now());
        self.cursor_shape = term.cursor_style().shape;
        let screen = term.screen();
        self.total_lines = screen.history_len() as usize + usize::from(screen.rows());
        // The engine is the one drain (R-16): `render_update` never takes the
        // pixels, so the adapter does, right here, once per frame.
        self.graphics.clear();
        self.graphics.extend(term.take_graphics());
    }

    /// What the last [`refill`](Self::refill) did: nothing, a shift plus
    /// [`changed`](Self::changed), or everything.
    pub fn update(&self) -> RenderUpdate {
        self.update
    }

    /// The full viewport, indexed by display row — always, whatever
    /// [`update`](Self::update) says (R-15).
    pub fn rows(&self) -> &[RenderRow] {
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
    pub fn render_cursor(&self) -> &RenderCursor {
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
    pub fn placements(&self) -> &[RenderPlacement] {
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

    /// Force the next [`refill`](Self::refill) to rebuild everything.
    pub fn invalidate(&mut self) {
        self.state.invalidate();
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
