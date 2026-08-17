//! Row layout cache update logic.

use std::collections::HashSet;

use alacritty_terminal::term::cell::Flags;
use gpui::Font;

use oneterm_highlight::Class;
use oneterm_terminal::{IndexedCell, TermDamageInfo};

use super::super::highlight::SemanticOverlay;
use super::super::theme::TerminalTheme;
use super::super::url::url_masks_wrapped;
use super::row::{layout_row, line_hash};
use super::types::{RenderStyleKey, RowLayout, RowLayoutCache};

/// Frame inputs for [`update_row_cache`] — the per-frame terminal grid state
/// that determines which rows are dirty and need re-layout.
pub(crate) struct RowCacheFrame<'a> {
    pub cells: &'a [IndexedCell],
    pub damage: &'a TermDamageInfo,
    pub num_lines: usize,
    pub display_offset: usize,
    pub grid_size: (u16, u16),
    pub cursor_display_line: i32,
}

/// Style inputs for [`update_row_cache`] — theme, font, and overlay used to
/// lay out a dirty row. Changes to `style_key` invalidate the whole cache.
pub(crate) struct RowCacheStyle<'a> {
    pub theme: &'a TerminalTheme,
    pub base_font: &'a Font,
    pub style_key: &'a RenderStyleKey,
    pub overlay: &'a SemanticOverlay,
}

/// Update the row cache: only recompute layout for dirty rows, reuse cached
/// artifacts for non-dirty rows.
///
/// Selection and hover state are painted as separate rectangles rather than
/// baked into row layout, so they never invalidate cached rows and are not
/// part of the inputs here.
pub(crate) fn update_row_cache(
    cache: &mut RowLayoutCache,
    frame: &RowCacheFrame,
    style: &RowCacheStyle,
) {
    use itertools::Itertools;

    let &RowCacheFrame {
        cells,
        damage,
        num_lines,
        display_offset,
        grid_size,
        cursor_display_line,
    } = frame;
    let &RowCacheStyle {
        theme,
        base_font,
        style_key,
        overlay,
    } = style;

    let size_changed = cache.prev_grid_size != Some(grid_size);
    let scroll_delta = display_offset as i32 - cache.prev_display_offset as i32;
    let scroll_changed = scroll_delta != 0;
    let style_changed = cache.prev_style_key.as_ref() != Some(style_key);
    // hover/ctrl no longer affect layout (URLs are always highlighted via
    // url_masks_wrapped), so they are excluded from cache invalidation.
    // Selection is painted as separate rectangles, not baked into row layout,
    // so selection changes do NOT invalidate cached rows.
    let scroll_only = scroll_changed && !size_changed && !style_changed;
    let global_invalidate = size_changed || (scroll_changed && !scroll_only) || style_changed;

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
    // PERF-09: Skip recomputation when no rows are dirty (idle terminal) —
    // reuse cached masks from the last frame with dirty rows.
    let num_cols = grid_size.1 as usize;
    if !dirty_set.is_empty() {
        cache.cached_url_masks = url_masks_wrapped(cells, num_lines, num_cols);
    }
    let url_masks = &cache.cached_url_masks;

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

            let layout = layout_row(line_vec, display_line as i32, theme, base_font, &cell_class);

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
    cache.prev_style_key = Some(style_key.clone());
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
    // PERF-11: Record wide flags in the same pass to avoid O(n²) linear
    // search for each character's wide flag during flattening.
    let mut line_text = String::new();
    let mut char_cols: Vec<usize> = Vec::new(); // char index -> column
    let mut char_wide: Vec<bool> = Vec::new(); // char index -> is wide
    for ic in line_cells {
        if ic.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let col = ic.point.column.0;
        char_cols.push(col);
        char_wide.push(ic.cell.flags.contains(Flags::WIDE_CHAR));
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
        if char_wide[i] && col + 1 < num_cols {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::build_terminal_theme;
    use gpui_component::Theme;

    fn test_style_key(
        font_family: &str,
        palette: oneterm_terminal::TerminalPalette,
    ) -> RenderStyleKey {
        RenderStyleKey {
            font: gpui::Font {
                family: font_family.into(),
                weight: gpui::FontWeight::NORMAL,
                style: gpui::FontStyle::Normal,
                fallbacks: None,
                features: gpui::FontFeatures(std::sync::Arc::new(vec![])),
            },
            font_size_bits: 14.0f32.to_bits(),
            palette,
            min_contrast_bits: 4.5f32.to_bits(),
            semantic_enabled: true,
            shell_profile: oneterm_highlight::ShellProfile::Unix,
        }
    }

    fn test_overlay() -> SemanticOverlay {
        SemanticOverlay::new(oneterm_highlight::ShellProfile::Unix, true)
    }

    fn test_font() -> gpui::Font {
        gpui::Font {
            family: "monospace".into(),
            weight: gpui::FontWeight::NORMAL,
            style: gpui::FontStyle::Normal,
            fallbacks: None,
            features: gpui::FontFeatures(std::sync::Arc::new(vec![])),
        }
    }

    /// Drive `update_row_cache` with the fixed test defaults (no cells, offset 0,
    /// no cursor row), varying only the damage, line count, and style key.
    fn run_update(
        cache: &mut RowLayoutCache,
        damage: TermDamageInfo,
        num_lines: usize,
        theme: &TerminalTheme,
        font: &gpui::Font,
        key: &RenderStyleKey,
        overlay: &SemanticOverlay,
    ) {
        update_row_cache(
            cache,
            &RowCacheFrame {
                cells: &[],
                damage: &damage,
                num_lines,
                display_offset: 0,
                grid_size: (24, 80),
                cursor_display_line: -1,
            },
            &RowCacheStyle {
                theme,
                base_font: font,
                style_key: key,
                overlay,
            },
        );
    }

    fn text_cells(lines: &[&str]) -> Vec<IndexedCell> {
        use alacritty_terminal::index::{Column, Line, Point};
        use alacritty_terminal::term::cell::Cell;

        let mut cells = Vec::new();
        for (row, text) in lines.iter().enumerate() {
            for (col, c) in text.chars().enumerate() {
                let mut cell = Cell::default();
                cell.c = c;
                cells.push(IndexedCell {
                    point: Point::new(Line(row as i32), Column(col)),
                    cell,
                });
            }
        }
        cells
    }

    fn row_text(cache: &RowLayoutCache, row: usize) -> String {
        cache.rows[row]
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect()
    }

    /// Scrolling up by one line with only `Partial([])` damage rotates the
    /// cached rows down and re-lays out only the row that scrolled in.
    #[test]
    fn scroll_only_rotates_rows_and_relayouts_the_new_row() {
        let mut cache = RowLayoutCache::new();
        let theme = build_terminal_theme(&Theme::default());
        let font = test_font();
        let key = test_style_key("monospace", theme.palette);
        let overlay = test_overlay();
        let style = RowCacheStyle {
            theme: &theme,
            base_font: &font,
            style_key: &key,
            overlay: &overlay,
        };

        let first = text_cells(&["aaa", "bbb", "ccc"]);
        update_row_cache(
            &mut cache,
            &RowCacheFrame {
                cells: &first,
                damage: &TermDamageInfo::Full,
                num_lines: 3,
                display_offset: 0,
                grid_size: (3, 80),
                cursor_display_line: -1,
            },
            &style,
        );
        assert_eq!(cache.stats.row_layout_calls, 3);

        // Scroll up one line: "zzz" scrolls in at the top, "ccc" scrolls off.
        let second = text_cells(&["zzz", "aaa", "bbb"]);
        update_row_cache(
            &mut cache,
            &RowCacheFrame {
                cells: &second,
                damage: &TermDamageInfo::Partial(vec![]),
                num_lines: 3,
                display_offset: 1,
                grid_size: (3, 80),
                cursor_display_line: -1,
            },
            &style,
        );
        assert_eq!(cache.stats.dirty_lines, 1);
        assert_eq!(cache.stats.row_layout_calls, 1);
        assert_eq!(row_text(&cache, 0), "zzz");
        assert_eq!(row_text(&cache, 1), "aaa");
        assert_eq!(row_text(&cache, 2), "bbb");

        // Scroll back down: rows rotate up and only the bottom row is rebuilt.
        let third = text_cells(&["aaa", "bbb", "ccc"]);
        update_row_cache(
            &mut cache,
            &RowCacheFrame {
                cells: &third,
                damage: &TermDamageInfo::Partial(vec![]),
                num_lines: 3,
                display_offset: 0,
                grid_size: (3, 80),
                cursor_display_line: -1,
            },
            &style,
        );
        assert_eq!(cache.stats.row_layout_calls, 1);
        assert_eq!(row_text(&cache, 2), "ccc");
    }

    /// Scrolling by a whole viewport (or more) leaves nothing to rotate.
    #[test]
    fn scroll_by_a_full_viewport_dirties_every_row() {
        let mut cache = RowLayoutCache::new();
        let theme = build_terminal_theme(&Theme::default());
        let font = test_font();
        let key = test_style_key("monospace", theme.palette);
        let overlay = test_overlay();
        let style = RowCacheStyle {
            theme: &theme,
            base_font: &font,
            style_key: &key,
            overlay: &overlay,
        };
        let cells = text_cells(&["aaa", "bbb", "ccc"]);
        for (offset, damage) in [
            (0, TermDamageInfo::Full),
            (3, TermDamageInfo::Partial(vec![])),
        ] {
            update_row_cache(
                &mut cache,
                &RowCacheFrame {
                    cells: &cells,
                    damage: &damage,
                    num_lines: 3,
                    display_offset: offset,
                    grid_size: (3, 80),
                    cursor_display_line: -1,
                },
                &style,
            );
        }
        assert_eq!(cache.stats.dirty_lines, 3);
        assert_eq!(cache.stats.row_layout_calls, 3);
    }

    /// Same style key + Partial([]) damage → 0 dirty lines (cache hit).
    #[test]
    fn same_style_key_no_dirty() {
        let mut cache = RowLayoutCache::new();
        let theme = build_terminal_theme(&Theme::default());
        let font = test_font();
        let overlay = test_overlay();
        let key = test_style_key("monospace", theme.palette);

        // First call: establish cache state with 0 lines.
        run_update(
            &mut cache,
            TermDamageInfo::Partial(vec![]),
            0,
            &theme,
            &font,
            &key,
            &overlay,
        );
        assert_eq!(cache.stats.dirty_lines, 0);

        // Second call: same key, same damage → still 0 dirty lines.
        run_update(
            &mut cache,
            TermDamageInfo::Partial(vec![]),
            0,
            &theme,
            &font,
            &key,
            &overlay,
        );
        assert_eq!(cache.stats.dirty_lines, 0);
    }

    /// Different style key (font change) → global invalidate (all lines dirty).
    #[test]
    fn style_key_change_global_invalidate() {
        let mut cache = RowLayoutCache::new();
        let theme = build_terminal_theme(&Theme::default());
        let font = test_font();
        let overlay = test_overlay();
        let key1 = test_style_key("monospace", theme.palette);
        let key2 = test_style_key("courier", theme.palette);

        // First call: establish cache with 2 lines.
        run_update(
            &mut cache,
            TermDamageInfo::Full,
            2,
            &theme,
            &font,
            &key1,
            &overlay,
        );

        // Second call: font changed → all 2 lines dirty.
        run_update(
            &mut cache,
            TermDamageInfo::Partial(vec![]),
            2,
            &theme,
            &font,
            &key2,
            &overlay,
        );
        assert_eq!(cache.stats.dirty_lines, 2);
    }

    /// Palette change (e.g. dynamic OSC color) → global invalidate.
    #[test]
    fn palette_change_global_invalidate() {
        let mut cache = RowLayoutCache::new();
        let theme = build_terminal_theme(&Theme::default());
        let font = test_font();
        let overlay = test_overlay();
        let key1 = test_style_key("monospace", theme.palette);

        run_update(
            &mut cache,
            TermDamageInfo::Full,
            2,
            &theme,
            &font,
            &key1,
            &overlay,
        );

        // Second call: palette foreground changed → all dirty.
        let mut new_palette = theme.palette;
        new_palette.foreground = alacritty_terminal::vte::ansi::Rgb { r: 255, g: 0, b: 0 };
        let key2 = test_style_key("monospace", new_palette);

        run_update(
            &mut cache,
            TermDamageInfo::Partial(vec![]),
            2,
            &theme,
            &font,
            &key2,
            &overlay,
        );
        assert_eq!(cache.stats.dirty_lines, 2);
    }

    /// Semantic toggle (enabled→disabled) → global invalidate.
    #[test]
    fn semantic_toggle_global_invalidate() {
        let mut cache = RowLayoutCache::new();
        let theme = build_terminal_theme(&Theme::default());
        let font = test_font();
        let overlay = test_overlay();
        let key1 = test_style_key("monospace", theme.palette);

        run_update(
            &mut cache,
            TermDamageInfo::Full,
            2,
            &theme,
            &font,
            &key1,
            &overlay,
        );

        // Disable semantic highlighting in the key.
        let key2 = RenderStyleKey {
            semantic_enabled: false,
            ..key1.clone()
        };

        run_update(
            &mut cache,
            TermDamageInfo::Partial(vec![]),
            2,
            &theme,
            &font,
            &key2,
            &overlay,
        );
        assert_eq!(cache.stats.dirty_lines, 2);
    }

    /// Selection change does NOT cause row invalidation — selection is painted
    /// as separate rectangles, not baked into row layout.
    #[test]
    fn selection_change_no_invalidation() {
        let mut cache = RowLayoutCache::new();
        let theme = build_terminal_theme(&Theme::default());
        let font = test_font();
        let overlay = test_overlay();
        let key = test_style_key("monospace", theme.palette);

        // Establish cache: Full damage → all 2 lines dirty.
        run_update(
            &mut cache,
            TermDamageInfo::Full,
            2,
            &theme,
            &font,
            &key,
            &overlay,
        );
        assert_eq!(cache.stats.dirty_lines, 2);

        // Second call: same key + Partial([]) → 0 dirty.
        // Previously, a selection change would invalidate all rows.
        // Now selection is not part of the cache key at all.
        run_update(
            &mut cache,
            TermDamageInfo::Partial(vec![]),
            2,
            &theme,
            &font,
            &key,
            &overlay,
        );
        assert_eq!(cache.stats.dirty_lines, 0);
    }
}
