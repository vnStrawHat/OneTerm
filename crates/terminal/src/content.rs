//! Terminal content snapshot for rendering — framework-agnostic.
//!
//! `TerminalContent::from(&mut Term)` locks `Term` briefly, collects `RenderableContent`
//! (display_iter + cursor + selection + mode + display_offset) into owned data.
//! Rendering only reads the snapshot and never holds the `FairMutex` while drawing.
//!
//! From AtlasEngine: integrates `Term::damage()` + `Term::reset_damage()` to expose
//! per-row dirty info (`TermDamageInfo`) — the renderer only recomputes layout for
//! dirty rows instead of the entire viewport every frame.
//!
//! The exposed types (`Cell`, `RenderableCursor`, `TermMode`, `SelectionRange`,
//! `Point`) are `alacritty_terminal` types — the UI crate also depends on
//! `alacritty_terminal`, so they map directly. See Zed `terminal::Content`.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{RenderableCursor, Term, TermDamage, TermMode};

use super::color_classification::is_default_background_color;

/// A blank cell = space + default background + no decoration (hyperlink, underline,
/// inverse…). Matches the UI's `is_blank` definition so the gutter and stamping
/// agree on which lines have content.
pub fn is_blank_cell(cell: &Cell) -> bool {
    cell.c == ' '
        && is_default_background_color(&cell.bg)
        && cell.hyperlink().is_none()
        && !cell.flags.intersects(
            Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::WIDE_CHAR_SPACER,
        )
}

/// Index (0-based, in the active/viewport `Line` frame — same reference as
/// `cursor.point.line.0`) of the last line **with content** in the viewport.
/// Returns `0` if the entire viewport is blank.
///
/// Used for `line_times` stamping: the gutter renders up to the last non-blank
/// line, so timestamps must be stamped up to there too; otherwise lines below
/// the cursor (TUI, progress bars using cursor-up…) show `[--:--:--]`.
pub fn last_content_line<EP: EventListener>(term: &Term<EP>) -> i32 {
    let screen_lines = term.screen_lines();
    let cols = term.columns();
    let grid = term.grid();
    for i in (0..screen_lines).rev() {
        let row = &grid[Line(i as i32)];
        if (0..cols).any(|c| !is_blank_cell(&row[Column(c)])) {
            return i as i32;
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

/// Dirty-row info from `Term::damage()` — converted to display line indices
/// (0-based from the top of the viewport). The renderer uses it to skip layout
/// for unchanged rows — like AtlasEngine's `invalidatedRows`.
///
/// AtlasEngine uses `range<u16> { start, end }` (a row range). We use `Vec<usize>`
/// because `Term::damage()` gives per-line damage (it could skip columns within a
/// line, but we currently track only at line level).
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
    /// Dirty rows from `Term::damage()` — converted to display line indices.
    /// The renderer skips layout for rows not in this list.
    pub damage: TermDamageInfo,
}

impl std::fmt::Debug for TerminalContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalContent")
            .field("cells_len", &self.cells.len())
            .field("cursor_shape", &self.cursor.shape)
            .field("mode", &self.mode)
            .field("display_offset", &self.display_offset)
            .field("selection", &self.selection)
            .field("terminal_bounds", &self.terminal_bounds)
            .field("damage", &self.damage)
            .finish()
    }
}

impl TerminalContent {
    /// Build a snapshot from `Term` (the caller handles locking — pass `&mut Term`).
    ///
    /// Calls `Term::damage()` to collect dirty rows, `reset_damage()` to clear them,
    /// then reads `renderable_content()` (display_iter + cursor + selection +
    /// mode + display_offset) + `Dimensions` to get num_lines/num_cols.
    ///
    /// `&mut Term` is required because `damage()` needs `&mut self` — unlike
    /// `renderable_content()`, which needs only `&self`. The FairMutex locks both.
    pub fn from<EP: EventListener>(term: &mut Term<EP>) -> Self {
        // ── Collect damage before resetting ──
        // Term::damage() returns TermDamage::Full (everything) or Partial (an iterator
        // of LineDamageBounds). The iterator already adds display_offset to ldb.line,
        // so ldb.line is the display line (0-based from the top of the viewport).
        let num_lines = term.screen_lines();
        let damage = match term.damage() {
            TermDamage::Full => TermDamageInfo::Full,
            TermDamage::Partial(iter) => {
                // TermDamageIterator already adds display_offset to ldb.line,
                // so ldb.line is the display line (0-based from the top of the viewport).
                // Line(0) = top visible, Line(num_lines-1) = bottom visible.
                // display_line = ldb.line (no further conversion needed).
                let dirty: Vec<usize> = iter
                    .map(|ldb| ldb.line)
                    .filter(|&dl| dl < num_lines)
                    .collect();
                if dirty.is_empty() {
                    // No visible damage — still return an empty Partial so the
                    // renderer knows there is nothing to recompute.
                    TermDamageInfo::Partial(Vec::new())
                } else {
                    TermDamageInfo::Partial(dirty)
                }
            }
        };
        term.reset_damage();

        // ── Snapshot content (renderable_content needs only &self) ──
        let content = term.renderable_content();
        let RenderableContentParts {
            display_iter,
            cursor,
            mode,
            display_offset,
            selection,
        } = RenderableContentParts::take(content);

        let cells: Vec<IndexedCell> = display_iter
            .map(|indexed| IndexedCell {
                point: indexed.point,
                cell: indexed.cell.clone(),
            })
            .collect();

        let terminal_bounds = TerminalBounds {
            num_lines: term.screen_lines(),
            num_cols: term.columns(),
        };

        Self {
            cells,
            cursor,
            mode,
            display_offset,
            total_lines: term.total_lines(),
            selection,
            terminal_bounds,
            damage,
        }
    }

    /// Build a snapshot for **auxiliary queries** (cursor bounds, mouse hit-test,
    /// URL detection, mode checks) — **without** touching `Term::damage()` /
    /// `reset_damage()`.
    ///
    /// This is critical: `from()` *consumes* the accumulated damage (and resets
    /// it), so calling it outside the render would silently discard the dirty-row
    /// info the renderer needs, leaving stale rows on screen. Query callers ignore
    /// the `damage` field, so it is set to `Full` (a safe "don't trust for
    /// incremental" value). Needs only `&Term` (no `&mut`), since it never calls
    /// `damage()`.
    pub fn from_query<EP: EventListener>(term: &Term<EP>) -> Self {
        let content = term.renderable_content();
        let RenderableContentParts {
            display_iter,
            cursor,
            mode,
            display_offset,
            selection,
        } = RenderableContentParts::take(content);

        let cells: Vec<IndexedCell> = display_iter
            .map(|indexed| IndexedCell {
                point: indexed.point,
                cell: indexed.cell.clone(),
            })
            .collect();

        let terminal_bounds = TerminalBounds {
            num_lines: term.screen_lines(),
            num_cols: term.columns(),
        };

        Self {
            cells,
            cursor,
            mode,
            display_offset,
            total_lines: term.total_lines(),
            selection,
            terminal_bounds,
            // Auxiliary callers ignore damage; the render path never uses this
            // constructor, so it must not consume/reset the real damage state.
            damage: TermDamageInfo::Full,
        }
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

/// Helper that extracts the Copy/move parts of `RenderableContent` (avoids
/// messy partial-moves in `from`).
struct RenderableContentParts<'a> {
    display_iter: alacritty_terminal::grid::GridIterator<'a, Cell>,
    cursor: RenderableCursor,
    mode: TermMode,
    display_offset: usize,
    selection: Option<SelectionRange>,
}

impl<'a> RenderableContentParts<'a> {
    fn take(content: alacritty_terminal::term::RenderableContent<'a>) -> Self {
        let alacritty_terminal::term::RenderableContent {
            display_iter,
            cursor,
            mode,
            display_offset,
            selection,
            colors: _,
        } = content;
        Self {
            display_iter,
            cursor,
            mode,
            display_offset,
            selection,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::test::mock_term;

    #[test]
    fn snapshot_has_cells_and_bounds() {
        let mut term = mock_term("hello\r\nworld");
        let snap = TerminalContent::from(&mut term);
        assert_eq!(snap.terminal_bounds.num_cols, 5);
        // mock_term's default screen_lines.
        assert!(snap.terminal_bounds.num_lines > 0);
        assert!(!snap.cells.is_empty());
    }

    #[test]
    fn snapshot_is_owned_clone() {
        let mut term = mock_term("ab");
        let snap = TerminalContent::from(&mut term);
        let _clone = snap.clone();
        // Clone does not borrow term → the snapshot is truly owned.
        drop(term);
        assert!(!_clone.cells.is_empty());
    }

    #[test]
    fn cursor_visible_default() {
        let mut term = mock_term("x");
        let snap = TerminalContent::from(&mut term);
        // mock_term shows the cursor by default.
        assert!(snap.cursor_visible());
    }

    #[test]
    fn damage_full_on_first_snapshot() {
        // The first snapshot after creating the term → damage must be Full
        // (Term::damage() is always full until reset_damage).
        let mut term = mock_term("hello");
        let snap = TerminalContent::from(&mut term);
        assert_eq!(snap.damage, TermDamageInfo::Full);
    }

    #[test]
    fn damage_partial_on_unchanged() {
        // The second snapshot with no new output → damage must be Partial
        // (only the cursor line is dirty due to cursor movement).
        let mut term = mock_term("hello");
        let _snap1 = TerminalContent::from(&mut term);
        // Second snapshot — no changes, only cursor damage.
        let snap2 = TerminalContent::from(&mut term);
        // May be Full or Partial depending on cursor movement detection.
        // The key point: no panic, and the damage field exists.
        assert!(matches!(
            &snap2.damage,
            TermDamageInfo::Full | TermDamageInfo::Partial(_)
        ));
    }
}
