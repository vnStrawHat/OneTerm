//! Cell metrics and grid geometry: the hit-test contract shared by the element
//! and the input handlers.
//!
//! Every cell dimension is a whole number of device pixels. Quad edges are
//! then computed as `origin + n * cell` from the same logical values on both
//! sides of a boundary, so GPUI's per-edge rounding reproduces identical device
//! edges for abutting cells (no seams, no double coverage) and every glyph
//! lands on the same subpixel variant.

use gpui::{App, Bounds, Edges, Font, Pixels, Point, Size, Window, point, px};

use super::frame::GridSize;
use super::shapes::CellSizeDevicePx;

/// Fallback advance when the font reports neither `0` nor `m`.
const FALLBACK_ADVANCE: f32 = 8.0;

/// Measured cell size for one font / size / scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellMetrics {
    pub cell_width: Pixels,
    pub line_height: Pixels,
    /// Distance from the row top to the glyph baseline.
    pub baseline: Pixels,
    pub x_height: Pixels,
    pub device: CellSizeDevicePx,
    pub scale_factor: f32,
}

impl CellMetrics {
    /// Snap raw measurements to whole device pixels. `scale_factor` below 1 is
    /// clamped: a fractional device pixel cannot be painted.
    pub(crate) fn snapped(
        raw_cell_width: f32,
        raw_line_height: f32,
        ascent: f32,
        descent: f32,
        x_height: f32,
        scale_factor: f32,
    ) -> Self {
        let scale = scale_factor.max(1.0);
        let (cell_width, w) = snap_device(raw_cell_width, scale);
        let (line_height, h) = snap_device(raw_line_height, scale);
        // Same formula as GPUI's `baseline_offset`, on the snapped line height.
        let baseline = px((f32::from(line_height) - ascent - descent) / 2.0 + ascent);
        Self {
            cell_width,
            line_height,
            baseline,
            x_height: px(x_height),
            device: CellSizeDevicePx {
                w: w.max(1),
                h: h.max(1),
            },
            scale_factor: scale,
        }
    }

    /// Convert cell-relative device pixels to logical pixels.
    #[inline]
    pub(crate) fn logical(&self, device: i32) -> Pixels {
        px(device as f32 / self.scale_factor)
    }
}

/// Round `logical` to a whole device pixel; returns the snapped logical value
/// and the device size.
pub(crate) fn snap_device(logical: f32, scale: f32) -> (Pixels, i32) {
    let device = (logical * scale).round().max(1.0);
    (px(device / scale), device as i32)
}

/// Measure the cell for `font` at `font_size`. Costs a platform text-system
/// call; the state caches the result per (font, size, scale).
pub(crate) fn measure(
    font: &Font,
    font_size: Pixels,
    line_height_factor: f32,
    cell_width_override: Option<f32>,
    window: &Window,
    cx: &App,
) -> CellMetrics {
    let text_system = cx.text_system();
    let font_id = text_system.resolve_font(font);
    let raw_width = cell_width_override.unwrap_or_else(|| {
        text_system
            .ch_advance(font_id, font_size)
            .map(f32::from)
            .or_else(|_| {
                text_system
                    .advance(font_id, font_size, 'm')
                    .map(|s| f32::from(s.width))
            })
            .unwrap_or(FALLBACK_ADVANCE)
    });
    let ascent = f32::from(text_system.ascent(font_id, font_size));
    let descent = f32::from(text_system.descent(font_id, font_size));
    let x_height = f32::from(text_system.x_height(font_id, font_size));
    let raw_height = (f32::from(font_size) * line_height_factor).max(ascent + descent);
    CellMetrics::snapped(
        raw_width,
        raw_height,
        ascent,
        descent,
        x_height,
        window.scale_factor(),
    )
}

/// The `(rows, cols)` grid that fits `bounds` after the gutter and padding,
/// floored at device pixels and never below 1×1.
pub(crate) fn grid_size_for(
    bounds: Size<Pixels>,
    gutter: Pixels,
    padding: Edges<Pixels>,
    metrics: &CellMetrics,
) -> GridSize {
    let scale = metrics.scale_factor;
    let grid_width = (f32::from(bounds.width)
        - f32::from(gutter)
        - f32::from(padding.left)
        - f32::from(padding.right))
    .max(f32::from(metrics.cell_width));
    let grid_width_device = (grid_width * scale).floor().max(1.0);
    let cols = (grid_width_device / metrics.device.w as f32).floor() as u16;

    let avail_height =
        f32::from(bounds.height) - f32::from(padding.top) - f32::from(padding.bottom);
    let avail_height_device = (avail_height * scale).floor().max(0.0);
    let rows = (avail_height_device / metrics.device.h as f32).floor() as u16;
    GridSize {
        rows: rows.max(1),
        cols: cols.max(1),
    }
}

/// Where the grid sits inside the element this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GridGeometry {
    pub bounds: Bounds<Pixels>,
    /// Top-left of cell (0, 0), pixel-snapped.
    pub origin: Point<Pixels>,
    pub metrics: CellMetrics,
    pub size: GridSize,
    pub gutter_width: Pixels,
    pub padding: Edges<Pixels>,
}

impl GridGeometry {
    pub(crate) fn new(
        bounds: Bounds<Pixels>,
        metrics: CellMetrics,
        gutter_width: Pixels,
        padding: Edges<Pixels>,
    ) -> Self {
        let scale = metrics.scale_factor;
        let snap = |v: Pixels| px((f32::from(v) * scale).round() / scale);
        let origin = point(
            snap(bounds.origin.x + gutter_width + padding.left),
            snap(bounds.origin.y + padding.top),
        );
        Self {
            bounds,
            origin,
            metrics,
            size: grid_size_for(bounds.size, gutter_width, padding, &metrics),
            gutter_width,
            padding,
        }
    }

    /// Top-left corner of cell `(row, col)`; `(rows, cols)` is the grid's
    /// bottom-right corner, so `from_corners(cell_origin(r, c0),
    /// cell_origin(r + 1, c1))` covers columns `c0..c1` of row `r`.
    #[inline]
    pub(crate) fn cell_origin(&self, row: usize, col: usize) -> Point<Pixels> {
        point(
            self.origin.x + self.metrics.cell_width * col as f32,
            self.origin.y + self.metrics.line_height * row as f32,
        )
    }

    /// Fractional `(row, col)` under `p`; `None` outside the grid.
    pub(crate) fn pixel_to_grid(&self, p: Point<Pixels>) -> Option<(f32, f32)> {
        let x = f32::from(p.x - self.origin.x) / f32::from(self.metrics.cell_width);
        let y = f32::from(p.y - self.origin.y) / f32::from(self.metrics.line_height);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        if x >= f32::from(self.size.cols) || y >= f32::from(self.size.rows) {
            return None;
        }
        Some((y, x))
    }

    /// Whole `(row, col)` under `p`; `None` outside the grid.
    pub(crate) fn cell_at(&self, p: Point<Pixels>) -> Option<(usize, usize)> {
        self.pixel_to_grid(p)
            .map(|(row, col)| (row as usize, col as usize))
    }

    /// Bottom edge of the last row.
    pub(crate) fn grid_bottom(&self) -> Pixels {
        self.cell_origin(usize::from(self.size.rows), 0).y
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Edges, point, px, size};

    use super::*;

    fn metrics(cell_w: f32, line_h: f32, scale: f32) -> CellMetrics {
        CellMetrics::snapped(cell_w, line_h, 10.0, 3.0, 5.0, scale)
    }

    #[test]
    fn metrics_snap_cell_to_device_pixels() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let m = metrics(7.3, 16.7, scale);
            let w_dev = f32::from(m.cell_width) * scale;
            let h_dev = f32::from(m.line_height) * scale;
            assert!(
                (w_dev - w_dev.round()).abs() < 1e-4,
                "scale {scale}: cell width {w_dev} is not a whole device pixel"
            );
            assert!(
                (h_dev - h_dev.round()).abs() < 1e-4,
                "scale {scale}: line height {h_dev} is not a whole device pixel"
            );
            assert_eq!(m.device.w, w_dev.round() as i32);
            assert_eq!(m.device.h, h_dev.round() as i32);
            assert_eq!(m.scale_factor, scale);
        }
        let m = metrics(7.3, 16.7, 1.5);
        assert_eq!(m.device, CellSizeDevicePx { w: 11, h: 25 });
        assert_eq!(f32::from(m.logical(11)), f32::from(m.cell_width));
    }

    #[test]
    fn metrics_clamp_scale_below_one() {
        let m = metrics(8.0, 16.0, 0.5);
        assert_eq!(m.scale_factor, 1.0);
        assert_eq!(m.device, CellSizeDevicePx { w: 8, h: 16 });
    }

    #[test]
    fn grid_size_subtracts_gutter_and_padding() {
        // 800 − 60 − 8 = 732 px → 91 columns; 400 − 4 = 396 px → 24 rows.
        let g = grid_size_for(
            size(px(800.0), px(400.0)),
            px(60.0),
            Edges {
                top: px(2.0),
                right: px(4.0),
                bottom: px(2.0),
                left: px(4.0),
            },
            &metrics(8.0, 16.0, 1.0),
        );
        assert_eq!(g, GridSize { rows: 24, cols: 91 });
    }

    #[test]
    fn grid_size_rounds_at_device_pixels() {
        // At 1.5× scale, 100.4 px is 150.6 device px → floor 150 → 150 / 12 = 12.5 → 12.
        let g = grid_size_for(
            size(px(100.4), px(48.0)),
            px(0.0),
            Edges::default(),
            &metrics(8.0, 16.0, 1.5),
        );
        assert_eq!(g, GridSize { rows: 3, cols: 12 });
    }

    #[test]
    fn grid_size_never_below_one_cell() {
        let g = grid_size_for(
            size(px(10.0), px(5.0)),
            px(60.0),
            Edges::default(),
            &metrics(8.0, 16.0, 1.0),
        );
        assert_eq!(g, GridSize { rows: 1, cols: 1 });
    }

    #[test]
    fn geometry_maps_pixels_to_cells_and_back() {
        let bounds = Bounds {
            origin: point(px(100.0), px(50.0)),
            size: size(px(400.0), px(200.0)),
        };
        let padding = Edges {
            top: px(2.0),
            right: px(0.0),
            bottom: px(0.0),
            left: px(4.0),
        };
        let geometry = GridGeometry::new(bounds, metrics(8.0, 16.0, 1.0), px(20.0), padding);
        assert_eq!(geometry.origin, point(px(124.0), px(52.0)));
        assert_eq!(geometry.cell_origin(2, 3), point(px(148.0), px(84.0)));
        assert_eq!(geometry.cell_at(point(px(148.0), px(84.0))), Some((2, 3)));
        assert_eq!(geometry.cell_at(point(px(155.9), px(99.9))), Some((2, 3)));
        assert_eq!(geometry.cell_at(point(px(123.0), px(84.0))), None);
        let (row, col) = geometry
            .pixel_to_grid(point(px(152.0), px(92.0)))
            .expect("inside");
        assert!((row - 2.5).abs() < 1e-5 && (col - 3.5).abs() < 1e-5);
        assert_eq!(geometry.size, GridSize { rows: 12, cols: 47 });
        assert_eq!(geometry.grid_bottom(), px(52.0 + 12.0 * 16.0));
        assert_eq!(
            geometry.cell_at(point(px(124.0 + 47.0 * 8.0), px(60.0))),
            None
        );
    }
}
