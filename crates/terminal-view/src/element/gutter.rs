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
