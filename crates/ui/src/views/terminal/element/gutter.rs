//! Gutter (line timestamps + line numbers) helpers cho `TerminalElement`.

use gpui::{Font, Pixels, SharedString, TextRun, Window, px};

use super::super::layout::GutterEntry;
use super::super::theme::TerminalTheme;
use super::measure::snap;

/// Tính chiều rộng gutter theo số line hiện tại.
pub(crate) fn compute_gutter_width(
    line_times: &[String],
    font: &Font,
    font_size: Pixels,
    _theme: &TerminalTheme,
    window: &mut Window,
) -> Pixels {
    let num_digits = line_times.len().max(1).to_string().len().max(2);
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
/// `viewport_lines` là chiều cao viewport (grid rows) — dùng để tính absolute
/// line number. `max_entries` giới hạn số entry thực tế render (vd chỉ đến dòng
/// con trỏ khi ở bottom, bỏ qua các row rỗng phía dưới).
pub(crate) fn compute_gutter_entries(
    line_times: &[String],
    total_lines: usize,
    display_offset: usize,
    viewport_lines: usize,
    max_entries: usize,
    bounds_origin: gpui::Point<Pixels>,
    line_height: Pixels,
    scale_factor: f32,
) -> Vec<GutterEntry> {
    let num_digits = line_times.len().max(1).to_string().len().max(2);

    let mut entries = Vec::with_capacity(max_entries);
    for i in 0..max_entries {
        let line_num =
            total_lines as i32 - display_offset as i32 - viewport_lines as i32 + i as i32 + 1;
        let line_num = line_num.max(1) as usize;
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
