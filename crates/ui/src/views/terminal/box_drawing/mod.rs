//! Box-drawing / block / powerline glyph primitives for `TerminalElement`.
//!
//! Drawn with `paint_quad` instead of font glyphs → pixel-perfect, no AA blur.

pub(crate) mod block;
pub(crate) mod drawing;
pub(crate) mod powerline;
pub(crate) mod rounded;
pub(crate) mod shade;

pub(crate) use drawing::box_drawing_rects;
pub(crate) use rounded::rounded_corner_rects_aa;

/// Heavy line thickness (device px). Extracted as a function so the rounded-corner
/// AA path shares the same formula as `box_drawing_rects`.
pub(crate) fn heavy_thickness(cw_d: i32) -> i32 {
    (cw_d as f32 / 3.0).round().max(2.0) as i32 + 1
}
