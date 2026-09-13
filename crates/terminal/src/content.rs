//! The frame: one `Terminal::render_update` into the render state this buffer
//! owns, plus the legacy shape the view still reads.
//!
//! `TerminalContent` **is** the consumer `DEC-0015` describes. It owns the
//! [`RenderState`] and therefore the damage watermark, so "changed for me"
//! means changed since *this* buffer last looked, and a second consumer needs
//! no engine change. The render path reuses one buffer
//! (`terminal-view/src/render/frame.rs`), so the watermark survives across
//! frames exactly where it belongs; a freshly built `TerminalContent` reports
//! `Full`, which is correct by construction rather than by convention.
//!
//! Native reads go through the accessors below — [`TerminalContent::rows`],
//! [`TerminalContent::changed`], [`TerminalContent::update`],
//! [`TerminalContent::row_id`]. The public fields are the **compatibility
//! surface**: the dense `Vec<IndexedCell>` in the reference's signed grid lines
//! and the `alacritty_terminal` value types around it, which
//! `crates/terminal-view` reads directly until `US-0085` moves it onto
//! `RenderRow` / `RenderCell` and deletes them together with
//! [`crate::engine_shim`].
//!
//! The compatibility cells are refreshed from the tri-state result rather than
//! rebuilt: nothing at all on `Unchanged`, only [`TerminalContent::changed`] on
//! a `Partial` that did not scroll, everything on `Full`.

use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::graphics::GraphicData;
use alacritty_terminal::term::{RenderableCursor, TermMode};
use oneterm_vt::intern::Hyperlink;
use oneterm_vt::render::{ModeSnapshot, RenderCursor, RenderPlacement, RenderRow};
use oneterm_vt::{
    Attrs, CellWidth, Color, HyperlinkId, NamedColor, RenderState, RenderUpdate, RowId, Size,
    Terminal,
};

/// A blank cell = space + default background + no decoration (hyperlink,
/// underline, inverse…). Matches the UI's `is_blank` definition so the gutter
/// and stamping agree on which lines have content.
///
/// Read straight off the engine's packed cell: `last_content_line` runs on every
/// `terminal_info()` call, and converting a whole viewport into legacy cells to
/// answer it would allocate a `Hyperlink` per decorated cell.
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

/// Index (0-based, in the active/viewport `Line` frame — same reference as
/// `cursor.point.line.0`) of the last line **with content** in the viewport.
/// Returns `0` if the entire viewport is blank.
///
/// Used for `line_times` stamping: the gutter renders up to the last non-blank
/// line, so timestamps must be stamped up to there too; otherwise lines below
/// the cursor (TUI, progress bars using cursor-up…) show `[--:--:--]`.
pub fn last_content_line(term: &Terminal) -> i32 {
    let screen = term.screen();
    let rows = screen.rows();
    let top = screen.screen_top();
    for index in (0..rows).rev() {
        let row = screen.row(top + u64::from(index));
        if row.cells().iter().any(|cell| !is_blank_cell(*cell, term)) {
            return i32::from(index);
        }
    }
    0
}

/// A cell together with its grid position (owned snapshot, does not borrow the grid).
#[derive(Debug, Clone)]
pub struct IndexedCell {
    pub point: Point,
    pub cell: Cell,
}

/// Dirty-row info — display line indices (0-based from the top of the
/// viewport). The renderer uses it to skip layout for unchanged rows.
///
/// We use `Vec<usize>` of damaged row indices rather than a single row range
/// because damage is per line (it could skip columns within a line, but we
/// currently track only at line level).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermDamageInfo {
    /// The entire viewport is dirty — repaint all rows.
    Full,
    /// Only these display line indices (0-based from the top) are dirty.
    Partial(Vec<usize>),
}

/// Grid size (number of displayed lines/columns). Pixel cell_width/line_height
/// are computed by the UI from the font and are not part of this snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalBounds {
    pub num_lines: usize,
    pub num_cols: usize,
}

/// One frame: the engine's render state, plus the legacy view of it.
pub struct TerminalContent {
    /// The frame source. Owns this consumer's damage watermark.
    pub(crate) state: RenderState,
    /// What the last [`TerminalContent::refill`] did.
    pub(crate) update: RenderUpdate,
    /// The display row the previous frame put the cursor on.
    ///
    /// The reference damaged both the old and the new cursor row on every
    /// `damage()` call; the engine reports a cursor move as `Partial` with an
    /// empty `changed` list, so the two rows are added here instead. Without the
    /// old row a moved cursor leaves a ghost.
    pub(crate) last_cursor_row: Option<usize>,
    /// The scroll offset the compatibility cells were last built against.
    ///
    /// `IndexedCell::point.line` is `row_index - display_offset`, so a changed
    /// offset invalidates every stored point even when no row changed.
    pub(crate) built_offset: Option<usize>,

    // ── Compatibility surface — `US-0085` deletes all of it ───────────────
    /// All displayed cells (in display order, display_offset already applied).
    pub cells: Vec<IndexedCell>,
    /// The cursor (shape may be `Hidden`).
    pub cursor: RenderableCursor,
    /// The current mode (mouse, alt-screen, bracketed paste…).
    pub mode: TermMode,
    /// The current scrollback offset (0 = at the bottom).
    pub display_offset: usize,
    /// Total number of lines (scrollback + visible) — for the scrollbar.
    pub total_lines: usize,
    /// The current selection, if any.
    pub selection: Option<SelectionRange>,
    /// Grid size.
    pub terminal_bounds: TerminalBounds,
    /// Dirty rows — converted to display line indices. The renderer skips
    /// layout for rows not in this list.
    pub damage: TermDamageInfo,
    /// Images decoded since the previous snapshot (Sixel), each handed out once;
    /// the cells reference them through `Cell::graphic()`.
    pub graphics: Vec<Arc<GraphicData>>,
}

impl std::fmt::Debug for TerminalContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalContent")
            .field("update", &self.update)
            .field("rows", &self.state.rows().len())
            .field("changed", &self.state.changed().len())
            .field("cells_len", &self.cells.len())
            .field("graphics_len", &self.graphics.len())
            .field("cursor_shape", &self.cursor.shape)
            .field("mode", &self.mode)
            .field("display_offset", &self.display_offset)
            .field("selection", &self.selection)
            .field("terminal_bounds", &self.terminal_bounds)
            .field("damage", &self.damage)
            .finish()
    }
}

impl Default for TerminalContent {
    /// An empty frame (no rows, no cells, hidden cursor, full damage) — the
    /// initial value for the reusable buffer passed to
    /// [`TerminalContent::refill`].
    fn default() -> Self {
        Self {
            state: RenderState::new(),
            update: RenderUpdate::Full,
            last_cursor_row: None,
            built_offset: None,
            cells: Vec::new(),
            cursor: RenderableCursor {
                shape: alacritty_terminal::vte::ansi::CursorShape::Hidden,
                point: Point::default(),
            },
            mode: TermMode::empty(),
            display_offset: 0,
            total_lines: 0,
            selection: None,
            terminal_bounds: TerminalBounds {
                num_lines: 0,
                num_cols: 0,
            },
            damage: TermDamageInfo::Full,
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
        crate::engine_shim::refill(self, term, Instant::now());
    }

    // ── Native accessors ─────────────────────────────────────────────────

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

    /// The modes the view reads at paint time, refreshed every refill.
    pub fn modes(&self) -> ModeSnapshot {
        self.state.modes()
    }

    /// The selection in engine coordinates.
    pub fn selection_range(&self) -> Option<oneterm_vt::SelectionRange> {
        self.state.selection()
    }

    /// Live image placements, ids only — the pixels are in
    /// [`graphics`](Self::graphics), drained once.
    pub fn placements(&self) -> &[RenderPlacement] {
        self.state.placements()
    }

    /// The strings behind a cell's hyperlink id, resolved under the lock.
    pub fn hyperlink(&self, id: HyperlinkId) -> Option<&Hyperlink> {
        self.state.hyperlink(id)
    }

    /// Rows between the viewport and the newest row; `0` is the sticky bottom.
    pub fn scroll_offset(&self) -> u32 {
        self.state.scroll_offset()
    }

    /// The `RowId` of a display row — the half of the two-way translation
    /// `migration.md` designed that a consumer keying a cache on row identity
    /// needs (`US-0085`'s `plan_cache`).
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

    /// true if the cursor is visible (shape ≠ Hidden).
    pub fn cursor_visible(&self) -> bool {
        // RenderableCursor.shape is CursorShape; Hidden = hidden.
        !matches!(
            self.cursor.shape,
            alacritty_terminal::vte::ansi::CursorShape::Hidden
        )
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
