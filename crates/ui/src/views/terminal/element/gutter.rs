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
/// `line_time_base` = absolute index (0-based) của `line_times[0]`. Time của một
/// dòng được tra theo **absolute index** của chính dòng đó (bằng với line number
/// − 1), nên không bị lệch khi `total_lines` dao động vì ConPTY repaint/reflow.
/// `absolute_line_count` = tổng số dòng đã output (monotonically increasing).
/// `viewport_lines` là chiều cao viewport (grid rows). `max_entries` giới hạn
/// số entry thực tế render.
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
        // Absolute index (0-based) của dòng tại display row `i`.
        let abs_index =
            absolute_line_count as i32 - display_offset as i32 - viewport_lines as i32 + i as i32;
        let line_num = (abs_index + 1).max(1) as usize;
        // Tra timestamp theo absolute index (qua base). Khi dòng CÓ NỘI DUNG được
        // gutter render nhưng chưa có timestamp tương ứng — thường do lệch trạng
        // thái giữa lần đọc `terminal_info` lúc stamp (render) và lúc dựng gutter
        // (prepaint), đặc biệt sau `clear` khi ConPTY repaint khiến
        // `absolute_line_count` dao động — ta KHÔNG hiện `[--:--:--]` mà fallback
        // về timestamp gần nhất đã biết. `[--:--:--]` chỉ dành cho vùng phía TRÊN
        // dòng đầu tiên (`abs_index < 0`) hoặc khi chưa có timestamp nào.
        let time_str = if abs_index < 0 {
            "--:--:--"
        } else {
            let ai = abs_index as usize;
            if ai >= line_time_base {
                let j = ai - line_time_base;
                line_times
                    .get(j)
                    .map(|s| s.as_str())
                    // Dòng mới hơn vùng đã stamp (read skew) → giờ gần nhất.
                    .or_else(|| line_times.last().map(|s| s.as_str()))
                    .unwrap_or("--:--:--")
            } else {
                // Dòng cũ hơn vùng đang track → giờ cũ nhất còn lưu.
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
