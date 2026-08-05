//! Font / cell measurement helpers for `TerminalElement`.

use std::cell::RefCell;
use std::rc::Rc;

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

/// Resize the session to the measured bounds.
pub(crate) fn resize_session(
    session: &gpui::Entity<Box<dyn oneterm_terminal::TerminalSession>>,
    bounds_size: gpui::Size<Pixels>,
    sizing: &GridSizing,
    last_grid_size: &Rc<RefCell<Option<(u16, u16)>>>,
    window: &mut Window,
    cx: &mut App,
) -> (u16, u16) {
    let scale_factor = window.scale_factor().max(1.0);
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

    if last_grid_size.borrow().as_ref() != Some(&(rows, cols)) {
        session.update(cx, |s, _| {
            if let Err(error) = s.resize(rows, cols) {
                log::warn!("terminal resize delivery failed: {error}");
            }
        });
        *last_grid_size.borrow_mut() = Some((rows, cols));
    }
    (rows, cols)
}
