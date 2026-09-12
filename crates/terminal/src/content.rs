//! Terminal content snapshot for rendering — framework-agnostic.
//!
//! `TerminalContent::refill(&mut Engine)` runs one `Terminal::render_update`
//! under the adapter lock and reshapes the result — cells, cursor, selection,
//! mode, scroll offset, damage — into owned data. Rendering only reads the
//! snapshot and never holds the lock while drawing.
//!
//! The damage the renderer sees is the engine's per-row sequence number read
//! through this consumer's watermark ([`crate::engine_shim::LegacySnapshot`]),
//! converted into the display-line shape the view already consumes, so a row
//! that did not change is not laid out again.
//!
//! The exposed types (`Cell`, `RenderableCursor`, `TermMode`, `SelectionRange`,
//! `Point`) are still `alacritty_terminal` types: they are the seam's value
//! vocabulary until `US-0085` moves `crates/terminal-view` onto the engine's
//! own `RenderRow` / `RenderCell`.

use std::sync::Arc;

use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::graphics::GraphicData;
use alacritty_terminal::term::{RenderableCursor, TermMode};
use oneterm_vt::{Attrs, CellWidth, Color, NamedColor, Terminal};

use crate::engine::Engine;

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

/// Snapshot of all displayable terminal content.
#[derive(Clone)]
pub struct TerminalContent {
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
    /// An empty snapshot (no cells, hidden cursor, full damage) — the initial
    /// value for a reusable buffer passed to [`TerminalContent::refill`].
    fn default() -> Self {
        Self {
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
    /// Build a snapshot from the engine (the caller handles locking).
    ///
    /// Allocating convenience over [`refill`](Self::refill) — the render path
    /// reuses one buffer via `refill` instead.
    pub fn from(engine: &mut Engine) -> Self {
        let mut content = Self::default();
        content.refill(engine);
        content
    }

    /// Refill `self` from the engine, reusing the `cells` and dirty-line
    /// allocations — the steady-state render loop allocates nothing (PERF).
    ///
    /// Consumes this consumer's damage: the render state's watermark advances,
    /// so the next call reports only what changed after this one.
    pub fn refill(&mut self, engine: &mut Engine) {
        engine.refill(self);
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
mod tests {
    use super::*;
    use crate::backend::GridSize;
    use crate::test_engine::{engine, feed};

    fn snapshot(engine: &mut Engine) -> TerminalContent {
        TerminalContent::from(engine)
    }

    #[test]
    fn snapshot_has_cells_and_bounds() {
        let mut term = engine(GridSize { cols: 5, lines: 2 });
        feed(&mut term, b"hello\r\nworld");
        let snap = snapshot(&mut term);
        assert_eq!(snap.terminal_bounds.num_cols, 5);
        assert_eq!(snap.terminal_bounds.num_lines, 2);
        assert_eq!(snap.cells.len(), 10);
        assert_eq!(snap.cells[0].cell.c, 'h');
        assert_eq!(snap.cells[5].cell.c, 'w');
    }

    #[test]
    fn snapshot_is_owned_clone() {
        let mut term = engine(GridSize { cols: 4, lines: 1 });
        feed(&mut term, b"ab");
        let snap = snapshot(&mut term);
        let clone = snap.clone();
        // Clone does not borrow the engine → the snapshot is truly owned.
        drop(term);
        assert!(!clone.cells.is_empty());
    }

    #[test]
    fn cursor_visible_default() {
        let mut term = engine(GridSize { cols: 4, lines: 1 });
        feed(&mut term, b"x");
        let snap = snapshot(&mut term);
        // The cursor is visible at power-on.
        assert!(snap.cursor_visible());
        assert_eq!(snap.cursor.point.column.0, 1);
    }

    #[test]
    fn damage_full_on_first_snapshot() {
        // The first snapshot of a fresh render state must be Full.
        let mut term = engine(GridSize { cols: 5, lines: 2 });
        feed(&mut term, b"hello");
        let snap = snapshot(&mut term);
        assert_eq!(snap.damage, TermDamageInfo::Full);
    }

    /// TEST-24: a second snapshot with no new output must not report full
    /// damage; at most the cursor line is dirty.
    #[test]
    fn damage_partial_on_unchanged() {
        let mut term = engine(GridSize { cols: 5, lines: 2 });
        feed(&mut term, b"hello");
        let mut content = TerminalContent::default();
        content.refill(&mut term);
        let cursor_line = content.cursor.point.line.0 as usize;
        // Second snapshot — no changes, only cursor damage.
        content.refill(&mut term);
        match &content.damage {
            TermDamageInfo::Partial(lines) => {
                assert!(
                    lines.iter().all(|line| *line == cursor_line),
                    "only the cursor line may be dirty, got {lines:?} (cursor {cursor_line})"
                );
            }
            TermDamageInfo::Full => panic!("unchanged terminal must not report full damage"),
        }
    }

    #[test]
    fn last_content_line_finds_the_last_written_row() {
        let mut term = engine(GridSize { cols: 8, lines: 5 });
        feed(&mut term, b"one\r\ntwo\r\n");
        assert_eq!(last_content_line(&term), 1);
    }
}
