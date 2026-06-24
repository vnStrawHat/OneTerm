//! Box-drawing / block / powerline glyph primitives cho `TerminalElement`.
//!
//! Vẽ bằng `paint_quad` thay vì font glyph → pixel-perfect, không AA blur.

pub(crate) mod block;
pub(crate) mod drawing;
pub(crate) mod powerline;
pub(crate) mod rounded;
pub(crate) mod shade;

pub(crate) use drawing::box_drawing_rects;
pub(crate) use rounded::rounded_corner_rects_aa;

/// Bé dày heavy line (device px). Tách hàm để path AA góc bo tròn dùng
/// chung công thức với `box_drawing_rects`.
pub(crate) fn heavy_thickness(cw_d: i32) -> i32 {
    (cw_d as f32 / 3.0).round().max(2.0) as i32 + 1
}
