//! Gutter (line timestamps + line numbers) helpers for `TerminalElement`.

use std::collections::VecDeque;

use gpui::{Font, Pixels, SharedString, TextRun, Window, px};

use super::super::layout::GutterEntry;
use super::measure::snap;

/// Number of digits reserved for the line-number column.
///
/// Uses `absolute_line_count` (monotonically increasing) instead of the number
/// of stamped lines (capped by scrollback) so the gutter is wide enough for
/// large line numbers; never fewer than two digits.
pub(crate) fn gutter_digits(absolute_line_count: usize) -> usize {
    absolute_line_count.max(1).to_string().len().max(2)
}

/// Measure the gutter width for `num_digits` line-number digits in `font`.
pub(crate) fn compute_gutter_width(
    num_digits: usize,
    font: &Font,
    font_size: Pixels,
    window: &mut Window,
) -> Pixels {
    let gutter_template = format!("[00:00:00] {}", "0".repeat(num_digits));
    let gutter_text_width = window
        .text_system()
        .shape_line(
            SharedString::from(gutter_template),
            font_size,
            &[TextRun {
                len: "[00:00:00] ".len() + num_digits,
                color: gpui::black(),
                background_color: None,
                font: font.clone(),
                underline: None,
                strikethrough: None,
            }],
            None,
        )
        .width();
    gutter_text_width + px(8.0)
}

/// Line-indexing inputs for the gutter: which absolute lines map to which
/// display rows, and how many entries to emit.
pub(crate) struct GutterLayout {
    /// The absolute index (0-based) of `line_times[0]`.
    pub line_time_base: usize,
    /// Total lines output so far (monotonically increasing).
    pub absolute_line_count: usize,
    pub display_offset: usize,
    /// Viewport height in grid rows.
    pub viewport_lines: usize,
    /// Caps the number of entries actually rendered.
    pub max_entries: usize,
}

/// Build the `GutterEntry` for each display line.
///
/// A line's time is looked up by its own **absolute index** (equal to line
/// number − 1), so it doesn't drift when `total_lines` fluctuates due to ConPTY
/// repaint/reflow. See [`GutterLayout`] for the index inputs.
pub(crate) fn compute_gutter_entries(
    line_times: &VecDeque<String>,
    layout: &GutterLayout,
    bounds_origin: gpui::Point<Pixels>,
    line_height: Pixels,
    scale_factor: f32,
) -> Vec<GutterEntry> {
    let num_digits = gutter_digits(layout.absolute_line_count);

    let mut entries = Vec::with_capacity(layout.max_entries);
    for i in 0..layout.max_entries {
        // Absolute index (0-based) of the line at display row `i`.
        let abs_index = layout.absolute_line_count as i32
            - layout.display_offset as i32
            - layout.viewport_lines as i32
            + i as i32;
        let line_num = (abs_index + 1).max(1) as usize;
        // Look up the timestamp by absolute index (via base). When a line WITH CONTENT
        // is rendered in the gutter but has no corresponding timestamp yet — usually a
        // state skew between reading `terminal_info` at stamp time (render) and building
        // the gutter (prepaint), especially after `clear` when ConPTY repaint makes
        // `absolute_line_count` fluctuate — we do NOT show `[--:--:--]` but fall back to
        // the most recent known timestamp. `[--:--:--]` is reserved only for the region
        // ABOVE the first line (`abs_index < 0`) or when there is no timestamp yet.
        let time_str = if abs_index < 0 {
            "--:--:--"
        } else {
            let ai = abs_index as usize;
            if ai >= layout.line_time_base {
                let j = ai - layout.line_time_base;
                line_times
                    .get(j)
                    .map(|s| s.as_str())
                    // Line newer than the stamped region (read skew) → most recent time.
                    .or_else(|| line_times.back().map(|s| s.as_str()))
                    .unwrap_or("--:--:--")
            } else {
                // Line older than the tracked region → oldest time still stored.
                line_times.front().map(|s| s.as_str()).unwrap_or("--:--:--")
            }
        };
        let text = format!("[{}] {:>width$}", time_str, line_num, width = num_digits);
        let clock_len = 1 + time_str.len() + 2;
        let y = px(snap(
            f32::from(bounds_origin.y) + i as f32 * f32::from(line_height),
            scale_factor,
        ));
        entries.push(GutterEntry {
            text: SharedString::from(text),
            clock_len,
            y,
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use gpui::{point, px};

    use super::{GutterLayout, compute_gutter_entries, gutter_digits};

    fn times(list: &[&str]) -> VecDeque<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn texts(
        line_times: &VecDeque<String>,
        base: usize,
        absolute: usize,
        offset: usize,
        viewport: usize,
        max: usize,
    ) -> Vec<String> {
        compute_gutter_entries(
            line_times,
            &GutterLayout {
                line_time_base: base,
                absolute_line_count: absolute,
                display_offset: offset,
                viewport_lines: viewport,
                max_entries: max,
            },
            point(px(0.0), px(0.0)),
            px(10.0),
            1.0,
        )
        .into_iter()
        .map(|e| e.text.to_string())
        .collect()
    }

    #[test]
    fn digits_grow_with_the_absolute_count_but_never_below_two() {
        assert_eq!(gutter_digits(0), 2);
        assert_eq!(gutter_digits(9), 2);
        assert_eq!(gutter_digits(150), 3);
    }

    #[test]
    fn entries_look_up_the_time_by_absolute_index() {
        // 3 lines output, all on a 3-row viewport, not scrolled.
        let t = times(&["10:00:01", "10:00:02", "10:00:03"]);
        assert_eq!(
            texts(&t, 0, 3, 0, 3, 3),
            vec!["[10:00:01]  1", "[10:00:02]  2", "[10:00:03]  3"]
        );
    }

    #[test]
    fn rows_above_the_first_line_show_a_placeholder() {
        // Only 1 line output on a 3-row viewport: the two rows above it are
        // before line 1 (`abs_index < 0`) — but the gutter only renders as many
        // rows as requested, from the top.
        let t = times(&["10:00:01"]);
        assert_eq!(texts(&t, 0, 1, 0, 3, 3)[0], "[--:--:--]  1");
    }

    #[test]
    fn newer_lines_than_the_stamps_fall_back_to_the_latest_time() {
        // 5 lines output but only 3 stamped (read skew) → rows 4/5 reuse the
        // most recent stamp instead of the placeholder.
        let t = times(&["10:00:01", "10:00:02", "10:00:03"]);
        let out = texts(&t, 0, 5, 0, 5, 5);
        assert_eq!(out[3], "[10:00:03]  4");
        assert_eq!(out[4], "[10:00:03]  5");
    }

    #[test]
    fn lines_older_than_the_tracked_region_use_the_oldest_time() {
        // Stamps for absolute indices 9.. only (line numbers 10 and 11);
        // scrolled so line 9 (absolute index 8) is visible above them.
        let t = times(&["10:00:10", "10:00:11"]);
        // absolute 12, viewport 3, offset 1 → rows are lines 9, 10, 11.
        let out = texts(&t, 9, 12, 1, 3, 3);
        assert_eq!(out, vec!["[10:00:10]  9", "[10:00:10] 10", "[10:00:11] 11"]);
    }

    #[test]
    fn entries_are_positioned_one_line_height_apart() {
        let t = times(&["10:00:01", "10:00:02"]);
        let entries = compute_gutter_entries(
            &t,
            &GutterLayout {
                line_time_base: 0,
                absolute_line_count: 2,
                display_offset: 0,
                viewport_lines: 2,
                max_entries: 2,
            },
            point(px(0.0), px(4.0)),
            px(10.0),
            1.0,
        );
        assert_eq!(entries[0].y, px(4.0));
        assert_eq!(entries[1].y, px(14.0));
        assert_eq!(entries[0].clock_len, "[10:00:01] ".len());
    }
}
