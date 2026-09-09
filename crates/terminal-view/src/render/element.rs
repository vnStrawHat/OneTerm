//! `TerminalElement`: one GPUI element that paints the whole grid.
//!
//! `request_layout` asks for a full-size node; `prepaint` does every piece of
//! work (geometry, resize, the frame's only snapshot, plan update, overlays,
//! cursor, gutter labels, hitbox); `paint` only pushes primitives inside one
//! layer for the grid and one for the cursor.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, CursorStyle, Element, ElementId, Entity, GlobalElementId, Hitbox, HitboxBehavior,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, Size, StrikethroughStyle,
    Style, UnderlineStyle, Window, fill, point, px,
};
use oneterm_terminal::TerminalSession;

use super::cursor::CursorPaint;
use super::diagnostics::FrameStats;
use super::frame::GridSize;
use super::metrics::GridGeometry;
use super::overlay::RowSpan;
use super::row_plan::{ColorSpan, DecorationKind, RowPlan};
use super::state::{Overlays, RenderState};
use crate::theme::TerminalTheme;

/// What the view hands the element each frame.
pub(crate) struct TerminalElementSpec {
    pub id: ElementId,
    pub session: Entity<Box<dyn TerminalSession>>,
    /// Inputs already refreshed by the view.
    pub state: Rc<RefCell<RenderState>>,
    /// Installs the IME handler (`window.handle_input`) in paint; the view
    /// builds it only while focused.
    pub ime: Option<Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut App)>>,
}

pub(crate) struct TerminalElement {
    spec: TerminalElementSpec,
}

impl TerminalElement {
    pub(crate) fn new(spec: TerminalElementSpec) -> Self {
        Self { spec }
    }
}

pub(crate) struct PrepaintState {
    hitbox: Hitbox,
    cursor: Option<CursorPaint>,
    visible_rows: Range<usize>,
    ime: Option<Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut App)>>,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some(self.spec.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: Size::full(),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        #[cfg(any(test, feature = "terminal-diagnostics"))]
        let started = std::time::Instant::now();
        let state = &mut *self.spec.state.borrow_mut();
        state.stats = FrameStats::default();

        let metrics = state.metrics(window, cx);
        let gutter_width = state.gutter_width(window);
        let geometry = GridGeometry::new(bounds, metrics, gutter_width, state.inputs.padding);
        if state.last_grid != Some(geometry.size) {
            let GridSize { rows, cols } = geometry.size;
            self.spec.session.update(cx, |session, _| {
                if let Err(error) = session.resize(rows, cols) {
                    log::warn!("terminal resize to {rows}x{cols} failed: {error}");
                }
            });
            // Recorded even on failure so a broken session does not trigger a
            // resize storm.
            state.last_grid = Some(geometry.size);
        }

        // The frame's only `snapshot_into`: it consumes the engine's damage.
        state.frame.snapshot(&**self.spec.session.read(cx));
        state.stats.snapshot_calls += 1;
        state.glyphs.begin_frame();
        state.update_plans(&metrics, window);
        state.compute_overlays();
        let cursor = state.resolve_cursor(&geometry, window);
        let rows = usize::from(geometry.size.rows);
        let display_offset = state.frame.display_offset();
        state.build_gutter_labels(rows, display_offset, window);
        state.geometry = Some(geometry);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        #[cfg(any(test, feature = "terminal-diagnostics"))]
        {
            state.stats.prepaint_us =
                started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        }
        PrepaintState {
            hitbox,
            cursor,
            visible_rows: 0..rows,
            ime: self.spec.ime.take(),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        #[cfg(any(test, feature = "terminal-diagnostics"))]
        let started = std::time::Instant::now();
        let ime = prepaint.ime.take();
        {
            let state = &mut *self.spec.state.borrow_mut();
            let Some(geometry) = state.geometry else {
                return;
            };
            let font_size = state.inputs.font_size;
            let url_hovering = state.inputs.url_hovering;
            let RenderState {
                plans,
                inputs,
                overlays,
                gutter,
                stats,
                ..
            } = state;
            let mut painter = GridPainter {
                bounds,
                geometry: &geometry,
                theme: &inputs.theme,
                overlays,
                gutter_labels: &gutter.labels,
                gutter_inset: RenderState::gutter_inset(),
                font_size,
                stats,
            };
            window.paint_layer(bounds, |window| {
                painter.paint(plans.rows(), prepaint.visible_rows.clone(), window);
            });
            stats.layers += 1;
            if let Some(cursor) = &prepaint.cursor {
                window.paint_layer(cursor.bounds, |window| {
                    cursor.paint(font_size, window, stats);
                });
                stats.layers += 1;
            }
            let style = if url_hovering {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            };
            window.set_cursor_style(style, &prepaint.hitbox);
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            {
                stats.paint_us = started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
                state
                    .latency
                    .push(stats.prepaint_us.saturating_add(stats.paint_us));
                #[cfg(feature = "terminal-diagnostics")]
                state.log.maybe_log(stats, &mut state.latency);
            }
        }
        // Outside the state borrow: the closure reaches back into the view.
        if let Some(ime) = ime {
            ime(bounds, window, cx);
        }
    }
}

/// Paints every plan of the grid inside one layer.
struct GridPainter<'a> {
    bounds: Bounds<Pixels>,
    geometry: &'a GridGeometry,
    theme: &'a TerminalTheme,
    overlays: &'a Overlays,
    gutter_labels: &'a [super::state::GutterLabel],
    gutter_inset: Pixels,
    font_size: Pixels,
    stats: &'a mut FrameStats,
}

impl GridPainter<'_> {
    fn span_bounds(&self, row: usize, col: u16, cols: u16) -> Bounds<Pixels> {
        let g = self.geometry;
        Bounds::from_corners(
            g.cell_origin(row, usize::from(col)),
            g.cell_origin(row + 1, usize::from(col + cols)),
        )
    }

    fn quad(&mut self, bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
        window.paint_quad(fill(bounds, color));
        self.stats.quads += 1;
    }

    fn paint(&mut self, plans: &[RowPlan], rows: Range<usize>, window: &mut Window) {
        // Element background, then the gutter column.
        self.quad(self.bounds, self.theme.bg, window);
        let gutter_width = self.geometry.gutter_width;
        if gutter_width > px(0.0) {
            let b = self.bounds;
            let gutter =
                Bounds::from_corners(b.origin, point(b.origin.x + gutter_width, b.bottom()));
            self.quad(gutter, self.theme.gutter_bg, window);
        }

        // Pass 1 (quads): backgrounds and shape quads, row by row.
        for row in rows.clone() {
            let Some(plan) = plans.get(row) else {
                break;
            };
            for span in &plan.bg {
                self.quad(
                    self.span_bounds(row, span.col, span.cols),
                    span.color,
                    window,
                );
            }
            let row_top = self.geometry.cell_origin(row, 0).y;
            let origin_x = self.geometry.origin.x;
            let m = self.geometry.metrics;
            for shape in &plan.shapes {
                let r = shape.rect;
                let b = Bounds::from_corners(
                    point(origin_x + m.logical(r.x), row_top + m.logical(r.y)),
                    point(
                        origin_x + m.logical(r.x + r.w),
                        row_top + m.logical(r.y + r.h),
                    ),
                );
                self.quad(b, shape.color, window);
            }
        }

        // Pass 2 (quads): search, then selection, above backgrounds and shapes
        // (quads keep insertion order inside a layer).
        for rect in self.overlays.search.iter() {
            let color = if rect.active {
                self.theme.search_active
            } else {
                self.theme.search_match
            };
            let RowSpan { row, col, cols } = rect.span;
            self.quad(self.span_bounds(usize::from(row), col, cols), color, window);
        }
        let selection = self.theme.selection;
        for span in self.overlays.selection.iter() {
            self.quad(
                self.span_bounds(usize::from(span.row), span.col, span.cols),
                selection,
                window,
            );
        }

        // Pass 3: decorations and glyphs. GPUI orders these kinds
        // Quad → Underline → Sprite within the layer regardless of call
        // order, so the walk is per row for cache locality only.
        for row in rows {
            let Some(plan) = plans.get(row) else {
                break;
            };
            self.paint_decorations(row, plan, window);
            self.paint_text(row, plan, window);
        }

        self.paint_gutter(window);
    }

    fn paint_decorations(&mut self, row: usize, plan: &RowPlan, window: &mut Window) {
        if plan.decorations.is_empty() {
            return;
        }
        let m = self.geometry.metrics;
        let row_top = self.geometry.cell_origin(row, 0).y;
        for deco in &plan.decorations {
            let x0 = self.geometry.cell_origin(row, usize::from(deco.col)).x;
            let width = m.cell_width * f32::from(deco.cols);
            match deco.kind {
                DecorationKind::Underline { wavy } => window.paint_underline(
                    point(x0, row_top + m.baseline + px(1.0)),
                    width,
                    &UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(deco.color),
                        wavy,
                    },
                ),
                DecorationKind::Strikethrough => window.paint_strikethrough(
                    point(x0, row_top + m.baseline - m.x_height * 0.5),
                    width,
                    &StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(deco.color),
                    },
                ),
            }
        }
    }

    fn paint_text(&mut self, row: usize, plan: &RowPlan, window: &mut Window) {
        let baseline_y = self.geometry.cell_origin(row, 0).y + self.geometry.metrics.baseline;
        for run in &plan.text {
            let run_x = self.geometry.cell_origin(row, usize::from(run.col)).x;
            let colors = plan.colors_of(run);
            paint_shaped_line(
                &run.line,
                point(run_x, baseline_y),
                colors,
                self.font_size,
                window,
                self.stats,
            );
        }
    }

    fn paint_gutter(&mut self, window: &mut Window) {
        if self.gutter_labels.is_empty() {
            return;
        }
        let x = self.bounds.origin.x + self.gutter_inset;
        let baseline = self.geometry.metrics.baseline;
        let clock = self.theme.clock_fg;
        let numbers = self.theme.line_number_fg;
        for (i, label) in self.gutter_labels.iter().enumerate() {
            let y = self.geometry.cell_origin(i, 0).y + baseline;
            let spans = [
                ColorSpan {
                    byte_end: label.clock_len as u32,
                    color: clock,
                },
                ColorSpan {
                    byte_end: u32::MAX,
                    color: numbers,
                },
            ];
            paint_shaped_line(
                &label.line,
                point(x, y),
                &spans,
                self.font_size,
                window,
                self.stats,
            );
        }
    }
}

/// Paint the glyphs of `line` at `origin` (pen position, baseline), coloring
/// each glyph by the span its byte index falls in. Failures (a glyph the font
/// cannot rasterize) are counted, never propagated.
fn paint_shaped_line(
    line: &ShapedLine,
    origin: gpui::Point<Pixels>,
    colors: &[ColorSpan],
    font_size: Pixels,
    window: &mut Window,
    stats: &mut FrameStats,
) {
    let mut span = 0;
    for run in &line.runs {
        for glyph in &run.glyphs {
            while span + 1 < colors.len() && glyph.index as u32 >= colors[span].byte_end {
                span += 1;
            }
            let color = colors.get(span).map(|c| c.color).unwrap_or(gpui::black());
            let at = point(origin.x + glyph.position.x, origin.y + glyph.position.y);
            let painted = if glyph.is_emoji {
                window.paint_emoji(at, run.font_id, glyph.id, font_size)
            } else {
                window.paint_glyph(at, run.font_id, glyph.id, font_size, color)
            };
            match painted {
                Ok(()) => stats.glyphs += 1,
                Err(_) => stats.glyph_errors += 1,
            }
        }
    }
}

#[cfg(test)]
#[path = "element_tests.rs"]
mod element_tests;
