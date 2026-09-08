//! Cursor resolution and painting (parity items 14–19).
//!
//! Resolution is split in two: `resolve` is pure (shape, color, hollow,
//! whether the covered glyph must be re-painted) and unit-tested without a
//! window; `CursorPaint::build` adds geometry and the optional shaped glyph.

use gpui::{Bounds, Hsla, Pixels, Point, ShapedLine, Window, fill, point, size};

use super::diagnostics::FrameStats;
use super::frame::{CellFlags, Color, CursorShape, Frame};
use super::glyphs::{FontSet, GlyphCache};
use super::metrics::GridGeometry;
use crate::theme::TerminalTheme;

/// Cursor settings that travel in `RenderInputs`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CursorConfig {
    /// Configured shape; overrides the snapshot shape except `Hidden`.
    pub shape: Option<CursorShape>,
    /// `None` = palette cursor color.
    pub color: Option<Hsla>,
    pub focused: bool,
    pub blink_visible: bool,
}

/// The pure part of cursor resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedCursor {
    pub row: u16,
    pub col: u16,
    /// Columns covered: 2 over a wide char.
    pub cols: u16,
    pub shape: CursorShape,
    pub hollow: bool,
    /// A filled block over a non-blank cell re-paints the glyph in the cell
    /// background so it stays readable (parity item 19).
    pub repaint_glyph: bool,
}

pub(crate) fn resolve(frame: &Frame, config: &CursorConfig) -> Option<ResolvedCursor> {
    let cursor = frame.cursor();
    let size = frame.size();
    if cursor.shape == CursorShape::Hidden {
        return None;
    }
    if cursor.row < 0 || cursor.row >= i32::from(size.rows) || cursor.col >= size.cols {
        return None;
    }
    // An unfocused terminal always shows its (hollow) cursor; a focused one
    // honors the blink phase (parity item 17).
    if config.focused && !config.blink_visible {
        return None;
    }
    let shape = config.shape.unwrap_or(cursor.shape);
    let hollow = !config.focused || shape == CursorShape::HollowBlock;
    let cell = frame.row(cursor.row as usize).cell(usize::from(cursor.col));
    let wide = cell.flags.contains(CellFlags::WIDE_CHAR);
    let non_blank = !matches!(cell.ch, ' ' | '\0') || !cell.zerowidth.is_empty();
    Some(ResolvedCursor {
        row: cursor.row as u16,
        col: cursor.col,
        cols: if wide { 2 } else { 1 },
        shape,
        hollow,
        repaint_glyph: shape == CursorShape::Block && !hollow && non_blank,
    })
}

/// A resolved cursor with its geometry, ready for the cursor layer.
pub(crate) struct CursorPaint {
    pub bounds: Bounds<Pixels>,
    pub color: Hsla,
    pub shape: CursorShape,
    pub hollow: bool,
    /// The covered glyph and the color (cell background) to re-paint it in.
    pub glyph: Option<(ShapedLine, Hsla)>,
    glyph_origin: Point<Pixels>,
    device_px: Pixels,
}

impl CursorPaint {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build(
        resolved: ResolvedCursor,
        frame: &Frame,
        config: &CursorConfig,
        theme: &TerminalTheme,
        geometry: &GridGeometry,
        fonts: &FontSet,
        font_size: Pixels,
        glyphs: &mut GlyphCache,
        text: &mut String,
        window: &Window,
        stats: &mut FrameStats,
    ) -> Self {
        let m = &geometry.metrics;
        let row = usize::from(resolved.row);
        let col = usize::from(resolved.col);
        let cell_origin = geometry.cell_origin(row, col);
        let cell_end = geometry.cell_origin(row + 1, col + usize::from(resolved.cols));
        let device_px = m.logical(1);
        let bounds = match resolved.shape {
            CursorShape::Beam => {
                // 20 % of the cell, at least one device pixel (parity item 14).
                let w = (m.cell_width * 0.2).max(device_px);
                Bounds::new(cell_origin, size(w, m.line_height))
            }
            CursorShape::Underline => {
                // 15 % of the line, at least two device pixels, bottom-aligned.
                let h = (m.line_height * 0.15).max(m.logical(2));
                Bounds::new(
                    point(cell_origin.x, cell_end.y - h),
                    size(cell_end.x - cell_origin.x, h),
                )
            }
            _ => Bounds::from_corners(cell_origin, cell_end),
        };
        let color = config.color.unwrap_or_else(|| theme.color(Color::Cursor));
        let glyph = if resolved.repaint_glyph {
            let cell = frame.row(row).cell(col);
            let bg = if cell.flags.contains(CellFlags::INVERSE) {
                cell.fg
            } else {
                cell.bg
            };
            let bold = cell.flags.contains(CellFlags::BOLD);
            let italic = cell.flags.contains(CellFlags::ITALIC);
            let (font, key) = fonts.get(bold, italic);
            text.clear();
            text.push(cell.ch);
            text.extend(cell.zerowidth.iter());
            let force_width = (resolved.cols == 1).then_some(m.cell_width);
            let line = glyphs.shape(text, font, key, font_size, force_width, window, stats);
            Some((line, theme.color(bg)))
        } else {
            None
        };
        Self {
            bounds,
            color,
            shape: resolved.shape,
            hollow: resolved.hollow,
            glyph,
            glyph_origin: point(cell_origin.x, cell_origin.y + m.baseline),
            device_px,
        }
    }

    /// Paint inside the cursor layer: quads first, then the re-painted glyph
    /// (sprites always draw over quads within one layer).
    pub(crate) fn paint(&self, font_size: Pixels, window: &mut Window, stats: &mut FrameStats) {
        let b = self.bounds;
        if self.hollow && matches!(self.shape, CursorShape::Block | CursorShape::HollowBlock) {
            let d = self.device_px;
            let edges = [
                Bounds::from_corners(b.origin, point(b.right(), b.top() + d)),
                Bounds::from_corners(point(b.left(), b.bottom() - d), b.bottom_right()),
                Bounds::from_corners(b.origin, point(b.left() + d, b.bottom())),
                Bounds::from_corners(point(b.right() - d, b.top()), b.bottom_right()),
            ];
            for edge in edges {
                window.paint_quad(fill(edge, self.color));
            }
            stats.quads += 4;
        } else {
            window.paint_quad(fill(b, self.color));
            stats.quads += 1;
        }
        if let Some((line, color)) = &self.glyph {
            for run in &line.runs {
                for glyph in &run.glyphs {
                    let origin = point(
                        self.glyph_origin.x + glyph.position.x,
                        self.glyph_origin.y + glyph.position.y,
                    );
                    let painted = if glyph.is_emoji {
                        window.paint_emoji(origin, run.font_id, glyph.id, font_size)
                    } else {
                        window.paint_glyph(origin, run.font_id, glyph.id, font_size, *color)
                    };
                    match painted {
                        Ok(()) => stats.glyphs += 1,
                        Err(_) => stats.glyph_errors += 1,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::frame::test_support::FrameBuilder;

    fn config(focused: bool, blink_visible: bool, shape: Option<CursorShape>) -> CursorConfig {
        CursorConfig {
            shape,
            color: None,
            focused,
            blink_visible,
        }
    }

    #[test]
    fn cursor_override_respects_hidden() {
        let hidden = FrameBuilder::new(3, 5)
            .cursor(1, 1, CursorShape::Hidden)
            .build();
        assert!(resolve(&hidden, &config(true, true, Some(CursorShape::Beam))).is_none());
        let beam = FrameBuilder::new(3, 5)
            .cursor(1, 1, CursorShape::Block)
            .build();
        let r = resolve(&beam, &config(true, true, Some(CursorShape::Beam))).expect("cursor");
        assert_eq!(r.shape, CursorShape::Beam);
        let r = resolve(&beam, &config(true, true, None)).expect("cursor");
        assert_eq!(r.shape, CursorShape::Block);
        // Outside the viewport (scrolled back) → no cursor.
        let away = FrameBuilder::new(3, 5)
            .display_offset(5)
            .cursor(7, 1, CursorShape::Block)
            .build();
        assert!(resolve(&away, &config(true, true, None)).is_none());
    }

    #[test]
    fn cursor_hollow_when_unfocused() {
        let frame = FrameBuilder::new(3, 5)
            .cursor(1, 1, CursorShape::Block)
            .build();
        let r = resolve(&frame, &config(false, false, None)).expect("unfocused still paints");
        assert!(r.hollow);
        let r = resolve(&frame, &config(true, true, None)).expect("cursor");
        assert!(!r.hollow);
        let r =
            resolve(&frame, &config(true, true, Some(CursorShape::HollowBlock))).expect("cursor");
        assert!(r.hollow);
    }

    #[test]
    fn cursor_glyph_repaint_only_for_filled_block() {
        let frame = FrameBuilder::new(3, 5)
            .text(1, 1, "x")
            .cursor(1, 1, CursorShape::Block)
            .build();
        assert!(
            resolve(&frame, &config(true, true, None))
                .expect("cursor")
                .repaint_glyph
        );
        assert!(
            !resolve(&frame, &config(false, true, None))
                .expect("cursor")
                .repaint_glyph
        );
        assert!(
            !resolve(&frame, &config(true, true, Some(CursorShape::Beam)))
                .expect("cursor")
                .repaint_glyph
        );
        let blank = FrameBuilder::new(3, 5)
            .cursor(1, 1, CursorShape::Block)
            .build();
        assert!(
            !resolve(&blank, &config(true, true, None))
                .expect("cursor")
                .repaint_glyph
        );
        let wide = FrameBuilder::new(3, 5)
            .wide(1, 1, '日')
            .cursor(1, 1, CursorShape::Block)
            .build();
        assert_eq!(
            resolve(&wide, &config(true, true, None))
                .expect("cursor")
                .cols,
            2
        );
    }

    #[test]
    fn cursor_blink_gating() {
        let frame = FrameBuilder::new(3, 5)
            .cursor(0, 0, CursorShape::Block)
            .build();
        assert!(resolve(&frame, &config(true, false, None)).is_none());
        assert!(resolve(&frame, &config(true, true, None)).is_some());
        assert!(resolve(&frame, &config(false, false, None)).is_some());
    }
}
