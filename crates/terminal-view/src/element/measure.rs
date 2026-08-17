//! Font / cell measurement helpers for `TerminalElement`.

use gpui::{App, Font, Pixels, Window, px};

/// Metrics measured from font + config.
pub(crate) struct FontMetrics {
    pub cell_width: Pixels,
    pub line_height: Pixels,
}

/// Measure cell width and line height for the current font.
pub(crate) fn measure_font(
    font: &Font,
    font_size: Pixels,
    line_height_factor: f32,
    cell_width_override: Option<f32>,
    window: &mut Window,
    cx: &mut App,
) -> FontMetrics {
    let scale_factor = window.scale_factor().max(1.0);
    let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

    let font_id = cx.text_system().resolve_font(font);
    let cell_width = if let Some(cw) = cell_width_override {
        px(snap_px(cw))
    } else {
        let raw = cx
            .text_system()
            .ch_advance(font_id, font_size)
            .map(|s| f32::from(s))
            .unwrap_or_else(|_| {
                cx.text_system()
                    .advance(font_id, font_size, 'm')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(8.0)
            });
        px(snap_px(raw))
    };

    let font_ascent = cx.text_system().ascent(font_id, font_size);
    let font_descent = cx.text_system().descent(font_id, font_size);
    let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
    let factor_height = f32::from(font_size) * line_height_factor;
    let line_height = px(snap_px(factor_height.max(natural_line_height)));

    FontMetrics {
        cell_width,
        line_height,
    }
}

/// Snap a pixel value to the device scale.
pub(crate) fn snap(value: f32, scale_factor: f32) -> f32 {
    (value * scale_factor).round() / scale_factor
}

/// Grid sizing inputs — gutter width, padding, and cell metrics used to convert
/// pixel bounds into a (rows, cols) terminal grid.
pub(crate) struct GridSizing {
    pub gutter_width: Pixels,
    pub pad_left: Pixels,
    pub pad_right: Pixels,
    pub pad_top: Pixels,
    pub pad_bottom: Pixels,
    pub cell_width: Pixels,
    pub line_height: Pixels,
}

/// Compute the `(rows, cols)` grid that fits `bounds_size` after the gutter and
/// padding, at device-pixel granularity (never below 1×1).
pub(crate) fn grid_size_for(
    bounds_size: gpui::Size<Pixels>,
    sizing: &GridSizing,
    scale_factor: f32,
) -> (u16, u16) {
    let grid_width = (f32::from(bounds_size.width)
        - f32::from(sizing.gutter_width)
        - f32::from(sizing.pad_left)
        - f32::from(sizing.pad_right))
    .max(f32::from(sizing.cell_width));
    let grid_width_device = (grid_width * scale_factor).floor().max(1.0);
    let cell_width_device = f32::from(sizing.cell_width) * scale_factor;
    let cols = ((grid_width_device / cell_width_device).floor() as u16).max(1);

    let avail_height =
        f32::from(bounds_size.height) - f32::from(sizing.pad_top) - f32::from(sizing.pad_bottom);
    let avail_height_device = (avail_height * scale_factor).floor().max(0.0);
    let line_height_device = f32::from(sizing.line_height) * scale_factor;
    let rows = ((avail_height_device / line_height_device).floor() as u16).max(1);
    (rows, cols)
}

/// Resize the session to the measured bounds. `last_grid_size` is the size
/// pushed on the previous frame — the resize is only delivered when it changes.
pub(crate) fn resize_session(
    session: &gpui::Entity<Box<dyn oneterm_terminal::TerminalSession>>,
    bounds_size: gpui::Size<Pixels>,
    sizing: &GridSizing,
    last_grid_size: &mut Option<(u16, u16)>,
    window: &mut Window,
    cx: &mut App,
) -> (u16, u16) {
    let scale_factor = window.scale_factor().max(1.0);
    let (rows, cols) = grid_size_for(bounds_size, sizing, scale_factor);
    if *last_grid_size != Some((rows, cols)) {
        session.update(cx, |s, _| {
            if let Err(error) = s.resize(rows, cols) {
                log::warn!("terminal resize delivery failed: {error}");
            }
        });
        *last_grid_size = Some((rows, cols));
    }
    (rows, cols)
}

#[cfg(test)]
mod tests {
    use gpui::{px, size};

    use super::{GridSizing, grid_size_for};

    fn sizing() -> GridSizing {
        GridSizing {
            gutter_width: px(60.0),
            pad_left: px(4.0),
            pad_right: px(4.0),
            pad_top: px(2.0),
            pad_bottom: px(2.0),
            cell_width: px(8.0),
            line_height: px(16.0),
        }
    }

    #[test]
    fn grid_size_subtracts_gutter_and_padding() {
        // 800 − 60 − 8 = 732 px → 91 columns; 400 − 4 = 396 px → 24 rows.
        assert_eq!(
            grid_size_for(size(px(800.0), px(400.0)), &sizing(), 1.0),
            (24, 91)
        );
    }

    #[test]
    fn grid_size_never_drops_below_one_cell() {
        assert_eq!(
            grid_size_for(size(px(10.0), px(5.0)), &sizing(), 1.0),
            (1, 1)
        );
    }

    #[test]
    fn grid_size_rounds_at_device_pixels() {
        // At 1.5× scale, 100.4 px of grid width is 150.6 device px → floor 150 →
        // 150 / 12 = 12.5 → 12 columns.
        let s = GridSizing {
            gutter_width: px(0.0),
            pad_left: px(0.0),
            pad_right: px(0.0),
            pad_top: px(0.0),
            pad_bottom: px(0.0),
            cell_width: px(8.0),
            line_height: px(16.0),
        };
        assert_eq!(grid_size_for(size(px(100.4), px(48.0)), &s, 1.5), (3, 12));
    }
}
