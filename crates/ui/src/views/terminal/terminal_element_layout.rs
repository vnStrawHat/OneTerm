//! Layout state + cache + per-row layout cho `TerminalElement`.
//!
//! Chỉ recompute layout cho dirty rows; non-dirty rows reuse cached
//! `RowLayout` artifacts qua frames.

use std::collections::HashSet;

use alacritty_terminal::term::cell::Flags;
use gpui::{Bounds, Font, Hsla, Pixels, SharedString, TextRun, UnderlineStyle};

use myterm2_core::terminal::{IndexedCell, TermDamageInfo, is_default_background_color};

use super::terminal_element_box::{box_drawing_rects, is_box_drawing};
use super::terminal_element_cell::{cell_colors, cell_style, is_blank};
use super::theme::TerminalTheme;

/// Metrics grid sau layout — View đọc để convert mouse pixel → (row,col).
/// Element ghi ở prepaint, View đọc ở handler (cùng thread → `Rc<RefCell>`).
#[derive(Clone, Copy, Default)]
pub(crate) struct GridMetrics {
    pub bounds: Option<Bounds<Pixels>>,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    /// Chiều rộng gutter (time + line number) bên trái terminal.
    pub gutter_width: Pixels,
}

/// Layout point (display line/col, 0-based từ top viewport).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct LayoutPoint {
    pub line: i32,
    pub column: i32,
}

/// Rect nền (1 dòng, batch ngang).
#[derive(Clone, Debug)]
pub(crate) struct LayoutRect {
    pub point: LayoutPoint,
    pub num_cells: usize,
    pub color: Hsla,
}

/// Text run batch (các cell liên tiếp cùng style, cùng dòng).
pub(crate) struct BatchedTextRun {
    pub start: LayoutPoint,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
}

/// Một dòng gutter: text + vị trí pixel (top-left) + byte length của phần clock.
pub(crate) struct GutterEntry {
    pub text: SharedString,
    /// Byte length của phần clock "[HH:MM:SS] " (không bao gồm line number).
    pub clock_len: usize,
    pub y: Pixels,
}

/// Thông tin layout computed ở prepaint → paint.
/// Per-row artifacts (rects, runs, box_draws) lives trong `RowLayoutCache`
/// (Rc<RefCell>) — paint đọc từ đó, không clone. Giống AtlasEngine `_p.rows`.
pub(crate) struct LayoutState {
    pub selection_rects: Vec<LayoutRect>,
    pub cursor: Option<CursorPaint>,
    pub background: Hsla,
    /// Pixel metrics.
    pub cell_width: Pixels,
    pub line_height: Pixels,
    /// Origin của grid (sau gutter/canh).
    pub grid_origin: gpui::Point<Pixels>,
    /// Chiều rộng gutter.
    pub gutter_width: Pixels,
    /// Mục gutter cho mỗi dòng hiển thị.
    pub gutter_entries: Vec<GutterEntry>,
    /// Màu nền gutter.
    pub gutter_bg: Hsla,
    /// Số display lines (để paint biết iterate bao nhiêu row trong cache).
    pub num_lines: usize,
}

/// Con trỏ để paint.
pub(crate) struct CursorPaint {
    pub point: LayoutPoint,
    pub color: Hsla,
    pub shape: alacritty_terminal::vte::ansi::CursorShape,
}

/// Một cell box-drawing (U+2500–U+257F) sẽ được vẽ bằng primitive fill
/// thay vì rasterize font glyph → pixel-perfect, không anti-alias blur.
pub(crate) struct BoxDrawCell {
    pub point: LayoutPoint,
    pub color: Hsla,
    pub c: char,
}

/// Layout artifacts cho 1 display row — cached qua frames để skip recompute.
/// Giống AtlasEngine `ShapedRow` (dirtyTop/dirtyBottom + glyphIndices/Advances).
pub(crate) struct RowLayout {
    pub rects: Vec<LayoutRect>,
    pub runs: Vec<BatchedTextRun>,
    pub box_draws: Vec<BoxDrawCell>,
    /// Cached `ShapedLine` cho mỗi `BatchedTextRun` — parallel vec với `runs`.
    /// None = chưa shape (mới recompute), Some = đã shape (reuse qua frames).
    /// Giống AtlasEngine `ShapedRow.glyphIndices` — glyph data persisted,
    /// chỉ re-rasterize khi row dirty.
    pub shaped_lines: Vec<Option<gpui::ShapedLine>>,
    /// Content hash của dòng ở frame trước — dùng để detect thay đổi
    /// mà Term::damage() không track (vd input()/write_at_cursor()
    /// không gọi damage_line()).
    pub prev_hash: u64,
}

impl RowLayout {
    fn empty() -> Self {
        Self {
            rects: Vec::new(),
            runs: Vec::new(),
            box_draws: Vec::new(),
            shaped_lines: Vec::new(),
            prev_hash: 0,
        }
    }
}

/// Thống kê render 1 frame — đếm paint calls để đo bottleneck.
/// Log mỗi 60 frame (~1s) qua eprintln.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct FrameStats {
    /// Tổng số display lines trong viewport.
    pub total_lines: usize,
    /// Số lines thực sự recompute layout (dirty).
    pub dirty_lines: usize,
    /// Số `paint_quad` calls trong paint().
    pub paint_quad_calls: usize,
    /// Số `shape_line` calls trong prepaint() — chỉ cho dirty rows.
    /// Non-dirty rows reuse cached ShapedLine → không gọi shape_line.
    pub shape_line_calls: usize,
    /// Số text runs painted trong paint() — tất cả dùng cached ShapedLine.
    pub text_run_paints: usize,
    /// Số background rects sau khi coalesce adjacent same-color cells.
    /// AtlasEngine: 1 quad per contiguous same-color run thay vì 1 per cell.
    pub bg_rect_count: usize,
    /// Số `line_hash` calls trong update_row_cache — chỉ cursor line
    /// thay vì tất cả non-dirty rows.
    pub hash_calls: usize,
    /// Frame counter — log mỗi 60 frame.
    pub frame_count: u64,
}

/// Per-row layout cache — persisted qua frames qua `Rc<RefCell>`.
/// Giống AtlasEngine `_p.rows` (Vec<ShapedRow*>) + `_p.colorBitmapGenerations`.
///
/// Chỉ recompute layout cho dirty rows (từ `TermDamageInfo`).
/// Non-dirty rows reuse cached `RowLayout` — skip cell iteration + color
/// resolution + text batching.
///
/// Invalidate toàn bộ khi:
/// - Grid size đổi (resize) — `prev_grid_size` mismatch.
/// - `display_offset` đổi (scroll) — damage thường đã là `Full`.
/// - Selection / hover URL / Ctrl state đổi — affect per-cell styling.
pub(crate) struct RowLayoutCache {
    /// Per-row layout artifacts, indexed by display line (0 = top viewport).
    pub rows: Vec<RowLayout>,
    /// Previous grid size — detect resize.
    pub prev_grid_size: Option<(u16, u16)>,
    /// Previous display_offset — detect scroll.
    pub prev_display_offset: usize,
    /// Previous selection — detect change (affects inverse video).
    pub prev_selection: Option<alacritty_terminal::selection::SelectionRange>,
    /// Previous hovered URL — detect change (affects link highlight).
    pub prev_hovered_url: Option<super::url::DetectedUrl>,
    /// Previous Ctrl held — detect change.
    pub prev_ctrl_held: bool,
    /// Frame stats — updated mỗi frame, log mỗi 60 frame.
    pub stats: FrameStats,
}

impl RowLayoutCache {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            prev_grid_size: None,
            prev_display_offset: 0,
            prev_selection: None,
            prev_hovered_url: None,
            prev_ctrl_held: false,
            stats: FrameStats::default(),
        }
    }

    /// Đảm bảo `rows` có đúng `num_lines` entries — resize cache khi grid đổi.
    fn ensure_size(&mut self, num_lines: usize) {
        if self.rows.len() != num_lines {
            self.rows.clear();
            self.rows.reserve(num_lines);
            for _ in 0..num_lines {
                self.rows.push(RowLayout::empty());
            }
        }
    }
}

impl Default for RowLayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Build selection highlight rects từ `SelectionRange` (grid coords) →
/// display coords. Mỗi dòng trong selection → 1 rect. Block selection →
/// rect cột đều; Simple/Lines → full width (trừ dòng đầu/cuối).
pub(crate) fn layout_selection(
    selection: &alacritty_terminal::selection::SelectionRange,
    display_offset: usize,
    num_lines: usize,
    num_cols: usize,
    color: Hsla,
) -> Vec<LayoutRect> {
    use alacritty_terminal::index::Line;

    // Convert grid line → display line.
    let to_display = |line: Line| -> i32 { line.0 + display_offset as i32 };
    let start_line = to_display(selection.start.line);
    let end_line = to_display(selection.end.line);

    // Bỏ qua nếu selection hoàn toàn ngoài viewport.
    if end_line < 0 || start_line >= num_lines as i32 {
        return Vec::new();
    }

    let clamped_start = start_line.max(0);
    let clamped_end = end_line.min(num_lines as i32 - 1);

    let mut rects = Vec::new();
    if selection.is_block {
        // Block: rect cột đều trên mỗi dòng.
        let start_col = selection.start.column.0 as i32;
        let end_col = (selection.end.column.0 as i32).min(num_cols as i32 - 1);
        if end_col < start_col {
            return Vec::new();
        }
        for line in clamped_start..=clamped_end {
            rects.push(LayoutRect {
                point: LayoutPoint {
                    line,
                    column: start_col,
                },
                num_cells: (end_col - start_col + 1) as usize,
                color,
            });
        }
    } else {
        // Simple / Lines / Semantic: full width trừ dòng đầu & dòng cuối.
        for line in clamped_start..=clamped_end {
            let (col_start, num_cells) = if line == start_line && line == end_line {
                // Cùng dòng: từ start column tới end column.
                let s = selection.start.column.0 as i32;
                let e = (selection.end.column.0 as i32 + 1).min(num_cols as i32);
                (s, (e - s).max(0) as usize)
            } else if line == start_line {
                // Dòng đầu: từ start column tới cuối.
                let s = selection.start.column.0 as i32;
                (s, (num_cols as i32 - s) as usize)
            } else if line == end_line {
                // Dòng cuối: từ đầu tới end column.
                let e = (selection.end.column.0 as i32 + 1).min(num_cols as i32);
                (0, e as usize)
            } else {
                // Dòng giữa: full width.
                (0, num_cols)
            };
            if num_cells > 0 {
                rects.push(LayoutRect {
                    point: LayoutPoint {
                        line,
                        column: col_start,
                    },
                    num_cells,
                    color,
                });
            }
        }
    }
    rects
}

/// Build set of (line, column) trong selection → để swap fg/bg khi vẽ text.
pub(crate) fn build_selection_set(selection_rects: &[LayoutRect]) -> HashSet<LayoutPoint> {
    let mut set = HashSet::new();
    for r in selection_rects {
        for c in 0..r.num_cells {
            set.insert(LayoutPoint {
                line: r.point.line,
                column: r.point.column + c as i32,
            });
        }
    }
    set
}

/// Layout 1 display row — build rects + text runs + box draws cho cells
/// trên cùng 1 dòng. Trích từ `layout_grid` cũ, tách per-row để cache.
///
/// `line_cells` — cells thuộc 1 display line (cùng `point.line`).
/// `display_line` — index 0-based từ top viewport.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_row(
    line_cells: Vec<&IndexedCell>,
    display_line: i32,
    theme: &TerminalTheme,
    base_font: &Font,
    selection_set: &HashSet<LayoutPoint>,
    hovered_url: Option<&super::url::DetectedUrl>,
    ctrl_held: bool,
) -> RowLayout {
    let mut rects: Vec<LayoutRect> = Vec::new();
    let mut runs: Vec<BatchedTextRun> = Vec::new();
    let mut box_draws: Vec<BoxDrawCell> = Vec::new();
    let mut current_batch: Option<BatchedTextRun> = None;
    let mut prev_had_extras = false;

    for ic in line_cells {
        let point = ic.point;
        let cell = &ic.cell;
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        // Skip space theo emoji variation sequence.
        if cell.c == ' ' && prev_had_extras {
            prev_had_extras = false;
            continue;
        }
        prev_had_extras = matches!(cell.zerowidth(), Some(c) if !c.is_empty());

        let lp = LayoutPoint {
            line: display_line,
            column: point.column.0 as i32,
        };
        let _is_selected = selection_set.contains(&lp);

        // ── AtlasEngine color bitmap caching ──
        // Skip cell_colors() cho blank cells (space + default bg +
        // no flags) — không cần fg hay bg → skip resolve_cell_color()
        // + ensure_minimum_contrast() + DIM alpha.
        // Tiết kiệm ~80% color resolution cho dòng prompt trống.
        if is_blank(cell) {
            continue;
        }

        let (fg, bg) = cell_colors(cell, theme);

        // Nền khác default → rect.
        // AtlasEngine: merge adjacent same-color cells thành 1 quad
        // để giảm paint_quad calls (coalescing).
        if !is_default_background_color(&cell.bg) || cell.flags.contains(Flags::INVERSE) {
            let col = point.column.0 as i32;
            let merged = if let Some(last) = rects.last_mut() {
                if last.color == bg
                    && last.point.line == display_line
                    && last.point.column + last.num_cells as i32 == col
                {
                    last.num_cells += 1;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !merged {
                rects.push(LayoutRect {
                    point: LayoutPoint {
                        line: display_line,
                        column: col,
                    },
                    num_cells: 1,
                    color: bg,
                });
            }
        }

        let mut style = cell_style(cell, fg, base_font);
        // Ctrl+hover URL highlight.
        if ctrl_held {
            if let Some(url) = hovered_url {
                if url.row == display_line as usize
                    && point.column.0 >= url.start_col
                    && point.column.0 < url.end_col
                {
                    style.color = gpui::hsla(0.6, 0.85, 0.65, 1.0);
                    style.underline = Some(UnderlineStyle {
                        color: Some(style.color),
                        thickness: gpui::px(1.0),
                        wavy: false,
                    });
                }
            }
        }
        let zw = cell.zerowidth();

        // Box-drawing chars — vẽ primitive thay vì font glyph.
        // AtlasEngine: text run không bị interrupt bởi box-drawing —
        // insert space placeholder vào batch để giữ position
        // continuity, giảm số runs → giảm shape_line calls.
        // Box-drawing primitive vẽ trên cùng, che space invisible.
        //
        // ⚠️ GIỮ nguyên underline/strikethrough của space placeholder —
        // không strip. Lý do:
        //   1. Box-drawing primitive vẽ line ở cell CENTER (cy/cx),
        //      text underline vẽ ở BASELINE — khác vị trí, không duplicate.
        //   2. Nếu strip underline → can_append() fail (underline mismatch)
        //      → batch bị SPLIT → text underline đứt đoạn tại mỗi
        //      box-drawing position → "đường kẻ không liền mạch".
        //   3. Giữ underline → text run liền mạch → underline liên tục.
        //      Box-drawing primitive ở vị trí khác nên không xung đột.
        if is_box_drawing(cell.c) && !box_drawing_rects(cell.c, 16, 16).is_empty() {
            box_draws.push(BoxDrawCell {
                point: lp,
                color: style.color,
                c: cell.c,
            });
            // Insert space vào current batch — giữ nguyên style (incl.
            // underline/strikethrough) để can_append() thành công,
            // text run không bị split, underline liên tục.
            let mut sp = style;
            sp.len = ' '.len_utf8();
            if let Some(b) = current_batch.as_mut() {
                if b.start.column + b.cell_count as i32 == lp.column && b.can_append(&sp) {
                    b.append_char(' ');
                } else {
                    // Column gap hoặc style incompatible — flush và tạo batch mới.
                    let old = current_batch.take().unwrap();
                    runs.push(old);
                    current_batch = Some(BatchedTextRun::new(lp, ' ', sp));
                }
            } else {
                current_batch = Some(BatchedTextRun::new(lp, ' ', sp));
            }
            continue;
        }

        if let Some(b) = current_batch.as_mut() {
            if b.can_append(&style)
                && b.start.line == lp.line
                && b.start.column + b.cell_count as i32 == lp.column
            {
                b.append_char(cell.c);
                if let Some(cs) = zw {
                    for &c in cs {
                        b.append_zw(c);
                    }
                }
            } else {
                let old = current_batch.take().unwrap();
                runs.push(old);
                let mut nb = BatchedTextRun::new(lp, cell.c, style);
                if let Some(cs) = zw {
                    for &c in cs {
                        nb.append_zw(c);
                    }
                }
                current_batch = Some(nb);
            }
        } else {
            let mut nb = BatchedTextRun::new(lp, cell.c, style);
            if let Some(cs) = zw {
                for &c in cs {
                    nb.append_zw(c);
                }
            }
            current_batch = Some(nb);
        }
    }
    if let Some(b) = current_batch {
        runs.push(b);
    }
    RowLayout {
        rects,
        runs,
        box_draws,
        shaped_lines: Vec::new(),
        prev_hash: 0,
    }
}

/// Update row cache: chỉ recompute layout cho dirty rows, reuse cached
/// artifacts cho non-dirty rows. Giống AtlasEngine `_p.invalidatedRows`.
///
/// Dirty sources:
/// - `TermDamageInfo::Full` → invalidate all.
/// - `TermDamageInfo::Partial(lines)` → invalidate those display lines.
/// - Grid size change / selection / hover / ctrl change → invalidate all
///   (global state affect per-cell styling).
/// - **Scroll-only change** (`display_offset` đổi, không có global change
///   khác) → **shift cache rows** thay vì full invalidate. Chỉ recompute
///   rows mới visible (top/bottom `|delta|` rows). Giống AtlasEngine
///   `_p.rows` shift khi scroll.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_row_cache(
    cache: &mut RowLayoutCache,
    cells: &[IndexedCell],
    damage: &TermDamageInfo,
    num_lines: usize,
    display_offset: usize,
    grid_size: (u16, u16),
    selection: Option<alacritty_terminal::selection::SelectionRange>,
    hovered_url: Option<&super::url::DetectedUrl>,
    ctrl_held: bool,
    theme: &TerminalTheme,
    base_font: &Font,
    selection_set: &HashSet<LayoutPoint>,
    cursor_display_line: i32,
) {
    use itertools::Itertools;

    // ── Detect global state changes → full invalidate ──
    let size_changed = cache.prev_grid_size != Some(grid_size);
    let scroll_delta = display_offset as i32 - cache.prev_display_offset as i32;
    let scroll_changed = scroll_delta != 0;
    let selection_changed = cache.prev_selection != selection;
    let hover_changed = cache.prev_hovered_url.as_ref() != hovered_url;
    let ctrl_changed = cache.prev_ctrl_held != ctrl_held;
    // Scroll-only: chỉ display_offset đổi, không có size/selection/hover/ctrl
    // → shift cache rows thay vì full invalidate.
    let scroll_only =
        scroll_changed && !size_changed && !selection_changed && !hover_changed && !ctrl_changed;
    let global_invalidate = size_changed
        || (scroll_changed && !scroll_only)
        || selection_changed
        || hover_changed
        || ctrl_changed;

    // Ensure cache has correct number of rows.
    cache.ensure_size(num_lines);

    // ── Scroll shift: di chuyển cached rows thay vì recompute tất cả ──
    // Giống AtlasEngine `_p.rows` shift khi scroll.
    //
    // scroll_delta > 0 (scroll UP): rotate_right(delta)
    //   → rows[delta..] = old rows[0..num_lines-delta] (shifted down)
    //   → top `delta` rows stale → cần recompute (mới visible)
    // scroll_delta < 0 (scroll DOWN): rotate_left(|delta|)
    //   → rows[0..num_lines-|delta|] = old rows[|delta|..] (shifted up)
    //   → bottom |delta| rows stale → cần recompute (mới visible)
    let mut scroll_dirty: Vec<usize> = Vec::new();
    if scroll_only {
        if scroll_delta > 0 {
            let delta = (scroll_delta as usize).min(num_lines);
            if delta < num_lines {
                cache.rows.rotate_right(delta);
                scroll_dirty = (0..delta).collect();
            } else {
                scroll_dirty = (0..num_lines).collect();
            }
        } else if scroll_delta < 0 {
            let delta = ((-scroll_delta) as usize).min(num_lines);
            if delta < num_lines {
                cache.rows.rotate_left(delta);
                scroll_dirty = ((num_lines - delta)..num_lines).collect();
            } else {
                scroll_dirty = (0..num_lines).collect();
            }
        }
    }

    // Determine which rows are dirty.
    let full_dirty = global_invalidate || matches!(damage, TermDamageInfo::Full);
    let dirty_set: HashSet<usize> = if full_dirty {
        (0..num_lines).collect()
    } else if scroll_only {
        // Scroll shift: dirty = scroll_dirty + Term::damage() partial
        let mut ds: HashSet<usize> = scroll_dirty.into_iter().collect();
        if let TermDamageInfo::Partial(lines) = damage {
            for &l in lines.iter() {
                if l < num_lines {
                    ds.insert(l);
                }
            }
        }
        ds
    } else if let TermDamageInfo::Partial(lines) = damage {
        lines.iter().copied().filter(|l| *l < num_lines).collect()
    } else {
        HashSet::new()
    };

    // Update stats.
    cache.stats.total_lines = num_lines;
    cache.stats.dirty_lines = dirty_set.len();
    cache.stats.hash_calls = 0;
    // ── Group cells by display line ──
    // cells trong snapshot là display order (top → bottom),
    // group bằng grid line (point.line).
    let linegroups = cells.iter().chunk_by(|ic| ic.point.line);
    for (display_line, (_, line_cells)) in linegroups.into_iter().enumerate() {
        if display_line >= num_lines {
            break;
        }
        // Collect cells for this line (need Vec để iterate được).
        let line_vec: Vec<&IndexedCell> = line_cells.collect();

        // ── Dirty detection: damage + content hash ──
        // Term::damage() không track input()/write_at_cursor(),
        // nhưng những thay đổi này chỉ xảy ra tại dòng cursor.
        // Chỉ hash cursor display line thay vì hash tất cả non-dirty
        // rows mỗi frame → giảm từ ~24 hash ops/frame xuống ≤1.
        let is_dirty = if dirty_set.contains(&display_line) {
            true
        } else if display_line as i32 == cursor_display_line
            && cursor_display_line >= 0
            && cursor_display_line < num_lines as i32
        {
            // Hash cursor line — fallback cho input()/write_at_cursor().
            cache.stats.hash_calls += 1;
            let hashed = super::terminal_element_cell::line_hash(&line_vec);
            hashed != cache.rows[display_line].prev_hash
        } else {
            // Non-dirty, non-cursor row → trust cache, skip hash.
            false
        };

        if is_dirty {
            // Compute hash mới + recompute layout.
            let new_hash = super::terminal_element_cell::line_hash(&line_vec);
            let layout = layout_row(
                line_vec,
                display_line as i32,
                theme,
                base_font,
                selection_set,
                hovered_url,
                ctrl_held,
            );
            cache.rows[display_line] = RowLayout {
                rects: layout.rects,
                runs: layout.runs,
                box_draws: layout.box_draws,
                shaped_lines: Vec::new(), // sẽ fill ở prepaint
                prev_hash: new_hash,
            };
        }
        // Non-dirty row → giữ cached RowLayout (incl. shaped_lines) nguyên.
    }

    // ── Update prev state ──
    cache.prev_grid_size = Some(grid_size);
    cache.prev_display_offset = display_offset;
    cache.prev_selection = selection;
    cache.prev_hovered_url = hovered_url.cloned();
    cache.prev_ctrl_held = ctrl_held;
}
