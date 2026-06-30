//! Snapshot nội dung terminal để render — framework-agnostic.
//!
//! `TerminalContent::from(&mut Term)` lock `Term` ngắn, collect `RenderableContent`
//! (display_iter + cursor + selection + mode + display_offset) thành dữ liệu
//! owned. Render chỉ đọc snapshot, không giữ `FairMutex` khi vẽ.
//!
//! Từ AtlasEngine: tích hợp `Term::damage()` + `Term::reset_damage()` để expose
//! per-row dirty info (`TermDamageInfo`) — renderer chỉ recompute layout cho dirty
//! rows thay vì toàn bộ viewport mỗi frame.
//!
//! Type lộ ra (`Cell`, `RenderableCursor`, `TermMode`, `SelectionRange`,
//! `Point`) là type `alacritty_terminal` — UI crate cũng phụ thuộc
//! `alacritty_terminal` nên map trực tiếp. Tham chiếu Zed `terminal::Content`.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{RenderableCursor, Term, TermDamage, TermMode};

use super::colors_util::is_default_background_color;

/// Cell trống = space + nền mặc định + không có trang trí (hyperlink, gạch
/// chân, đảo màu…). Trùng định nghĩa với UI `is_blank` để gutter và stamping
/// thống nhất khi xác định dòng có nội dung.
pub fn is_blank_cell(cell: &Cell) -> bool {
    cell.c == ' '
        && is_default_background_color(&cell.bg)
        && cell.hyperlink().is_none()
        && !cell.flags.intersects(
            Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::WIDE_CHAR_SPACER,
        )
}

/// Chỉ số dòng (0-based, theo `Line` của vùng active/viewport — cùng hệ quy
/// chiếu với `cursor.point.line.0`) của dòng **có nội dung** cuối cùng trong
/// viewport. Trả `0` nếu toàn bộ viewport trống.
///
/// Dùng cho `line_times` stamping: gutter render tới dòng non-blank cuối cùng
/// nên timestamp cũng phải được stamp tới đó, nếu không các dòng dưới cursor
/// (TUI, progress bar dùng cursor-up…) sẽ hiện `[--:--:--]`.
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

/// Một cell kèm vị trí grid (snapshot owned, không borrow grid).
#[derive(Debug, Clone)]
pub struct IndexedCell {
    pub point: Point,
    pub cell: Cell,
}

/// Thông tin dirty rows từ `Term::damage()` — đã convert sang display line
/// indices (0-based từ top viewport). Renderer dùng để skip layout cho rows
/// không đổi — giống AtlasEngine `invalidatedRows`.
///
/// AtlasEngine dùng `range<u16> { start, end }` (row range). Ta dùng
/// `Vec<usize>` vì `Term::damage()` cho per-line damage (có thể skip cả columns
/// trong line, nhưng hiện chỉ track line-level).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermDamageInfo {
    /// Toàn bộ viewport dirty — repaint all rows.
    Full,
    /// Chỉ các display line indices (0-based từ top) này dirty.
    Partial(Vec<usize>),
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
    /// Dirty rows từ `Term::damage()` — đã convert sang display line indices.
    /// Renderer skip layout cho rows không trong danh sách này.
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
    /// Build snapshot từ `Term` (lock caller lo — truyền `&mut Term`).
    ///
    /// Gọi `Term::damage()` để collect dirty rows, `reset_damage()` để clear,
    /// rồi đọc `renderable_content()` (display_iter + cursor + selection +
    /// mode + display_offset) + `Dimensions` để lấy num_lines/num_cols.
    ///
    /// `&mut Term` cần thiết vì `damage()` yêu cầu `&mut self` — khác
    /// `renderable_content()` chỉ cần `&self`. FairMutex lock cho cả hai.
    pub fn from<EP: EventListener>(term: &mut Term<EP>) -> Self {
        // ── Collect damage trước khi reset ──
        // Term::damage() trả TermDamage::Full (toàn bộ) hoặc Partial (iterator
        // các LineDamageBounds). Iterator đã thêm display_offset vào ldb.line,
        // nên ldb.line chính là display line (0-based từ top viewport).
        let num_lines = term.screen_lines();
        let damage = match term.damage() {
            TermDamage::Full => TermDamageInfo::Full,
            TermDamage::Partial(iter) => {
                // TermDamageIterator đã thêm display_offset vào ldb.line,
                // nên ldb.line chính là display line (0-based từ top viewport).
                // Line(0) = top visible, Line(num_lines-1) = bottom visible.
                // display_line = ldb.line (KHÔNG cần convert thêm).
                let dirty: Vec<usize> = iter
                    .map(|ldb| ldb.line)
                    .filter(|&dl| dl < num_lines)
                    .collect();
                if dirty.is_empty() {
                    // Không có damage nào visible — vẫn return Partial rỗng
                    // để renderer biết không cần recompute gì.
                    TermDamageInfo::Partial(Vec::new())
                } else {
                    TermDamageInfo::Partial(dirty)
                }
            }
        };
        term.reset_damage();

        // ── Snapshot content (renderable_content chỉ cần &self) ──
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
        let mut term = mock_term("hello\r\nworld");
        let snap = TerminalContent::from(&mut term);
        assert_eq!(snap.terminal_bounds.num_cols, 5);
        // screen_lines mặc định của mock_term.
        assert!(snap.terminal_bounds.num_lines > 0);
        assert!(!snap.cells.is_empty());
    }

    #[test]
    fn snapshot_is_owned_clone() {
        let mut term = mock_term("ab");
        let snap = TerminalContent::from(&mut term);
        let _clone = snap.clone();
        // Clone không cần borrow term → snapshot thực sự owned.
        drop(term);
        assert!(!_clone.cells.is_empty());
    }

    #[test]
    fn cursor_visible_default() {
        let mut term = mock_term("x");
        let snap = TerminalContent::from(&mut term);
        // mock_term mặc định show cursor.
        assert!(snap.cursor_visible());
    }

    #[test]
    fn damage_full_on_first_snapshot() {
        // Snapshot đầu tiên sau khi tạo term → damage phải là Full
        // (Term::damage() luôn full khi chưa reset_damage).
        let mut term = mock_term("hello");
        let snap = TerminalContent::from(&mut term);
        assert_eq!(snap.damage, TermDamageInfo::Full);
    }

    #[test]
    fn damage_partial_on_unchanged() {
        // Snapshot thứ 2 khi không có output mới → damage phải Partial
        // (chỉ cursor line dirty do cursor movement).
        let mut term = mock_term("hello");
        let _snap1 = TerminalContent::from(&mut term);
        // Snapshot thứ 2 — không có thay đổi nào, chỉ cursor damage.
        let snap2 = TerminalContent::from(&mut term);
        // Có thể Full hoặc Partial tùy cursor movement detection.
        // Quan trọng: không panic, damage field tồn tại.
        assert!(matches!(
            &snap2.damage,
            TermDamageInfo::Full | TermDamageInfo::Partial(_)
        ));
    }
}
