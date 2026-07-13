//! Row layout cache update logic.

use std::collections::HashSet;

use alacritty_terminal::term::cell::Flags;
use gpui::Font;

use oneterm_core::terminal::{IndexedCell, TermDamageInfo};
use oneterm_highlight::Class;

use super::super::cell::line_hash;
use super::super::highlight::SemanticOverlay;
use super::super::theme::TerminalTheme;
use super::super::url::url_masks_wrapped;
use super::row::layout_row;
use super::types::{LayoutPoint, RowLayout, RowLayoutCache};

/// Update the row cache: only recompute layout for dirty rows, reuse cached
/// artifacts for non-dirty rows.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_row_cache(
    cache: &mut RowLayoutCache,
    cells: &[IndexedCell],
    damage: &TermDamageInfo,
    num_lines: usize,
    display_offset: usize,
    grid_size: (u16, u16),
    selection: Option<alacritty_terminal::selection::SelectionRange>,
    _hovered_url: Option<&super::super::url::DetectedUrl>,
    _ctrl_held: bool,
    theme: &TerminalTheme,
    base_font: &Font,
    selection_set: &HashSet<LayoutPoint>,
    cursor_display_line: i32,
    overlay: &SemanticOverlay,
) {
    use itertools::Itertools;

    let size_changed = cache.prev_grid_size != Some(grid_size);
    let scroll_delta = display_offset as i32 - cache.prev_display_offset as i32;
    let scroll_changed = scroll_delta != 0;
    let selection_changed = cache.prev_selection != selection;
    // hover/ctrl no longer affect layout (URLs are always highlighted via
    // url_masks_wrapped), so they are excluded from cache invalidation.
    let scroll_only = scroll_changed && !size_changed && !selection_changed;
    let global_invalidate = size_changed || (scroll_changed && !scroll_only) || selection_changed;

    cache.ensure_size(num_lines);

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

    let full_dirty = global_invalidate || matches!(damage, TermDamageInfo::Full);
    let dirty_set: HashSet<usize> = if full_dirty {
        (0..num_lines).collect()
    } else if scroll_only {
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

    cache.stats.total_lines = num_lines;
    cache.stats.dirty_lines = dirty_set.len();
    cache.stats.hash_calls = 0;
    cache.stats.row_layout_calls = 0;
    cache.stats.allocation_buffer_sites = if num_lines == 0 {
        0
    } else {
        // `url_masks_wrapped`: masks + wrap flags + chars/mask per row.
        2 + 2 * num_lines
    };
    if !dirty_set.is_empty() {
        // The dirty-row HashSet.
        cache.stats.allocation_buffer_sites += 1;
    }

    // Pre-compute URL masks for all lines (handles wrapped URLs).
    let num_cols = grid_size.1 as usize;
    let url_masks = url_masks_wrapped(cells, num_lines, num_cols);

    let linegroups = cells.iter().chunk_by(|ic| ic.point.line);
    for (display_line, (_, line_cells)) in linegroups.into_iter().enumerate() {
        if display_line >= num_lines {
            break;
        }
        let line_vec: Vec<&IndexedCell> = line_cells.collect();
        if !line_vec.is_empty() {
            cache.stats.allocation_buffer_sites += 1;
        }

        let is_dirty = if dirty_set.contains(&display_line) {
            true
        } else if display_line as i32 == cursor_display_line
            && cursor_display_line >= 0
            && cursor_display_line < num_lines as i32
        {
            cache.stats.hash_calls += 1;
            let hashed = line_hash(&line_vec);
            hashed != cache.rows[display_line].prev_hash
        } else {
            false
        };

        if is_dirty {
            cache.stats.row_layout_calls += 1;
            // Cell classes, text/column maps, and row artifact scratch buffers.
            cache.stats.allocation_buffer_sites += 7;
            let new_hash = line_hash(&line_vec);
            let url_mask = url_masks
                .get(display_line)
                .map(|m| m.as_slice())
                .unwrap_or(&[]);

            // ── Semantic scan (Layer 2) ──
            // Build the line text from cells (skip spacers, map \0/\t to space),
            // scan it, flatten to per-column cell_class, then overlay URL mask.
            let cell_class = build_cell_class(&line_vec, num_cols, url_mask, overlay, display_line);

            let layout = layout_row(
                line_vec,
                display_line as i32,
                theme,
                base_font,
                selection_set,
                &cell_class,
            );
            cache.rows[display_line] = RowLayout {
                rects: layout.rects,
                runs: layout.runs,
                box_draws: layout.box_draws,
                shaped_lines: Vec::new(),
                prev_hash: new_hash,
            };
        }
    }

    cache.prev_grid_size = Some(grid_size);
    cache.prev_display_offset = display_offset;
    cache.prev_selection = selection;
}

/// Build the per-column `cell_class` array for one display row.
///
/// 1. Build the line text from non-spacer cells (one char per cell).
/// 2. Scan the text → `char_class` (one `Class` per char).
/// 3. Flatten to per-column `cell_class` (wide chars → both columns).
/// 4. Overlay the URL mask → `Class::Url` (URL is authoritative — Q3).
fn build_cell_class(
    line_cells: &[&IndexedCell],
    num_cols: usize,
    url_mask: &[bool],
    overlay: &SemanticOverlay,
    display_line: usize,
) -> Vec<u8> {
    let mut cell_class = vec![Class::Default as u8; num_cols];

    // Build the line text + char-to-column mapping.
    let mut line_text = String::new();
    let mut char_cols: Vec<usize> = Vec::new(); // char index -> column
    for ic in line_cells {
        if ic.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let col = ic.point.column.0;
        char_cols.push(col);
        line_text.push(match ic.cell.c {
            '\0' | '\t' => ' ',
            c => c,
        });
    }

    // Scan the line text.
    let char_class = overlay.scan(&line_text, display_line);

    // Flatten: char index -> column (wide chars → both columns).
    for (i, &col) in char_cols.iter().enumerate() {
        let cls = char_class.get(i).copied().unwrap_or(Class::Default as u8);
        if col < num_cols {
            cell_class[col] = cls;
        }
        // Wide char: the 2nd column (spacer column) gets the same class.
        let is_wide = line_cells
            .iter()
            .find(|ic| ic.point.column.0 == col)
            .map(|ic| ic.cell.flags.contains(Flags::WIDE_CHAR))
            .unwrap_or(false);
        if is_wide && col + 1 < num_cols {
            cell_class[col + 1] = cls;
        }
    }

    // Overlay URL mask → Class::Url (URL is authoritative — see Q3).
    for col in 0..num_cols.min(url_mask.len()) {
        if url_mask[col] {
            cell_class[col] = Class::Url as u8;
        }
    }

    cell_class
}
