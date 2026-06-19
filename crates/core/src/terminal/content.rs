//! Snapshot nội dung terminal để render — framework-agnostic.
//!
//! `TerminalContent::from(&Term)` lock `Term` ngắn, collect `RenderableContent`
//! (display_iter + cursor + selection + mode + display_offset) thành dữ liệu
//! owned. Render chỉ đọc snapshot, không giữ `FairMutex` khi vẽ.
//!
//! Type lộ ra (`Cell`, `RenderableCursor`, `TermMode`, `SelectionRange`,
//! `Point`) là type `alacritty_terminal` — UI crate cũng phụ thuộc
//! `alacritty_terminal` nên map trực tiếp. Tham chiếu Zed `terminal::Content`.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{RenderableCursor, Term, TermMode};

/// Một cell kèm vị trí grid (snapshot owned, không borrow grid).
#[derive(Debug, Clone)]
pub struct IndexedCell {
    pub point: Point,
    pub cell: Cell,
}

/// Kích thước grid (số dòng/cột hiển thị). Pixel cell_width/line_height do UI
/// tính từ font, không thuộc snapshot này.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalBounds {
    pub num_lines: usize,
    pub num_cols: usize,
}

/// Snapshot toàn bộ nội dung hiển thị được của terminal.
#[derive(Clone)]
pub struct TerminalContent {
    /// Tất cả cell hiển thị (display order, đã tính display_offset).
    pub cells: Vec<IndexedCell>,
    /// Con trỏ (shape có thể `Hidden`).
    pub cursor: RenderableCursor,
    /// Mode hiện hành (mouse, alt-screen, bracketed paste…).
    pub mode: TermMode,
    /// Offset scrollback hiện tại (0 = đang ở bottom).
    pub display_offset: usize,
    /// Tổng số dòng (scrollback + visible) — cho scrollbar.
    pub total_lines: usize,
    /// Vùng đang chọn, nếu có.
    pub selection: Option<SelectionRange>,
    /// Kích thước grid.
    pub terminal_bounds: TerminalBounds,
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
            .finish()
    }
}

impl TerminalContent {
    /// Build snapshot từ `Term` (lock caller lo — truyền `&Term`).
    ///
    /// Đọc `renderable_content()` (display_iter + cursor + selection + mode +
    /// display_offset) + `Dimensions` để lấy num_lines/num_cols.
    pub fn from<EP: EventListener>(term: &Term<EP>) -> Self {
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
        }
    }

    /// true nếu cursor đang hiển thị (shape ≠ Hidden).
    pub fn cursor_visible(&self) -> bool {
        // RenderableCursor.shape là CursorShape; Hidden = ẩn.
        !matches!(
            self.cursor.shape,
            alacritty_terminal::vte::ansi::CursorShape::Hidden
        )
    }
}

/// Helper tách các phần Copy/move của `RenderableContent` (tránh partial-move
/// lằng nhằng trong `from`).
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
        let term = mock_term("hello\r\nworld");
        let snap = TerminalContent::from(&term);
        assert_eq!(snap.terminal_bounds.num_cols, 5);
        // screen_lines mặc định của mock_term.
        assert!(snap.terminal_bounds.num_lines > 0);
        assert!(!snap.cells.is_empty());
    }

    #[test]
    fn snapshot_is_owned_clone() {
        let term = mock_term("ab");
        let snap = TerminalContent::from(&term);
        let _clone = snap.clone();
        // Clone không cần borrow term → snapshot thực sự owned.
        drop(term);
        assert!(!_clone.cells.is_empty());
    }

    #[test]
    fn cursor_visible_default() {
        let term = mock_term("x");
        let snap = TerminalContent::from(&term);
        // mock_term mặc định show cursor.
        assert!(snap.cursor_visible());
    }
}
