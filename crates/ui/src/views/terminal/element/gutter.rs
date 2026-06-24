//! Gutter (line timestamps + line numbers) helpers cho `TerminalElement`.

use gpui::{Font, Pixels, SharedString, TextRun, Window, px};

use super::super::layout::GutterEntry;
use super::super::theme::TerminalTheme;
use super::measure::snap;

/// Tính chiều rộng gutter theo số line hiện tại.
///
/// Dùng `absolute_line_count` (monotonically increasing) thay vì `line_times.len()`
/// (bị cap bởi scrollback) để gutter rộng đủ cho line number lớn.
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

/// Build các `GutterEntry` cho từng display line.
///
/// `total_lines` = số dòng trong buffer (scrollback + viewport) — dùng để index
/// `line_times` (timestamps synced với buffer thực tế).
/// `absolute_line_count` = tổng số dòng đã output kể cả khi scrollback đầy —
/// dùng cho line number (monotonically increasing, độc lập với scrollback).
/// `viewport_lines` là chiều cao viewport (grid rows). `max_entries` giới hạn
/// số entry thực tế render.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_gutter_entries(
    line_times: &[String],
    total_lines: usize,
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
        // Line number dùng absolute_line_count (monotonically increasing)
        // thay vì total_lines (bị cap bởi scrollback).
        let line_num = absolute_line_count as i32 - display_offset as i32 - viewport_lines as i32
            + i as i32
            + 1;
        let line_num = line_num.max(1) as usize;
        // line_times index vẫn dùng total_lines (synced với buffer thực tế).
        let abs_idx = (total_lines as i32 - display_offset as i32 - viewport_lines as i32
            + i as i32)
            .max(0) as usize;
        let time_str = if abs_idx < line_times.len() {
            line_times[abs_idx].as_str()
        } else {
            "--:--:--"
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
