//! Gutter (line timestamps + line numbers) helpers for `TerminalElement`.

use gpui::{Font, Pixels, SharedString, TextRun, Window, px};

use super::super::layout::GutterEntry;
use super::super::theme::TerminalTheme;
use super::measure::snap;

/// Compute the gutter width from the current line count.
///
/// Uses `absolute_line_count` (monotonically increasing) instead of `line_times.len()`
/// (capped by scrollback) so the gutter is wide enough for large line numbers.
pub(crate) fn compute_gutter_width(
    _line_times: &[String],
    absolute_line_count: usize,
    font: &Font,
    font_size: Pixels,
    _theme: &TerminalTheme,
    window: &mut Window,
) -> Pixels {
    let num_digits = absolute_line_count.max(1).to_string().len().max(2);
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

/// Build the `GutterEntry` for each display line.
///
/// `line_time_base` = the absolute index (0-based) of `line_times[0]`. A line's
/// time is looked up by its own **absolute index** (equal to line number − 1),
/// so it doesn't drift when `total_lines` fluctuates due to ConPTY repaint/reflow.
/// `absolute_line_count` = total lines output so far (monotonically increasing).
/// `viewport_lines` is the viewport height (grid rows). `max_entries` caps the
/// number of entries actually rendered.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_gutter_entries(
    line_times: &[String],
    line_time_base: usize,
    absolute_line_count: usize,
    display_offset: usize,
    viewport_lines: usize,
    max_entries: usize,
    bounds_origin: gpui::Point<Pixels>,
    line_height: Pixels,
    scale_factor: f32,
) -> Vec<GutterEntry> {
    let num_digits = absolute_line_count.max(1).to_string().len().max(2);

    let mut entries = Vec::with_capacity(max_entries);
    for i in 0..max_entries {
        // Absolute index (0-based) of the line at display row `i`.
        let abs_index =
            absolute_line_count as i32 - display_offset as i32 - viewport_lines as i32 + i as i32;
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
            if ai >= line_time_base {
                let j = ai - line_time_base;
                line_times
                    .get(j)
                    .map(|s| s.as_str())
                    // Line newer than the stamped region (read skew) → most recent time.
                    .or_else(|| line_times.last().map(|s| s.as_str()))
                    .unwrap_or("--:--:--")
            } else {
                // Line older than the tracked region → oldest time still stored.
                line_times.first().map(|s| s.as_str()).unwrap_or("--:--:--")
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
