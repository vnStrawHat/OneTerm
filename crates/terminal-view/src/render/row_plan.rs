//! One display row turned into paint-ready primitives: background spans,
//! shaped text runs with color spans, coalesced shape quads and decorations. Built only for rows whose hash changed; every vector is
//! cleared and refilled, never replaced, so a rebuild allocates nothing once
//! the row has been planned once at this width.

use gpui::{Hsla, Pixels, ShapedLine, Window};
use oneterm_highlight::{Class, ClassStyle, Decoration};
use oneterm_terminal::is_decorative_character;

use super::diagnostics::FrameStats;
use super::frame::{Cell, CellFlags, Color, FrameRow};
use super::glyphs::{FontKey, FontSet, GlyphCache};
use super::shapes::{CellSizeDevicePx, DeviceRect, is_shape_char, shape_quads};
use crate::highlight::{SemanticOverlay, to_gpui_hsla};
use crate::theme::TerminalTheme;

/// Alpha multiplier for SGR dim (parity item 5).
const DIM_ALPHA: f32 = 0.7;

/// A run of columns with the same non-default background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BgSpan {
    pub col: u16,
    pub cols: u16,
    pub color: Hsla,
}

/// Foreground color up to `byte_end` of the run text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ColorSpan {
    pub byte_end: u32,
    pub color: Hsla,
}

/// A shaped span of consecutive glyph cells with one font variant. `colors`
/// indexes `RowPlan::colors` (flattened so a rebuild allocates nothing).
#[derive(Clone, Debug)]
pub(crate) struct TextRunPlan {
    pub col: u16,
    /// Columns covered; the painter positions by `col` and the shaped line,
    /// so this is a test seam for run extents (wide chars).
    #[cfg(test)]
    pub cols: u16,
    pub line: ShapedLine,
    pub color_start: u32,
    pub color_end: u32,
}

/// A quad of a shape glyph; `rect.x` is measured from the grid's left edge,
/// `rect.y` from the row top, both in device pixels. The rect's coverage
/// `alpha` is already multiplied into `color`, so painting is a plain quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShapeQuad {
    pub rect: DeviceRect,
    pub color: Hsla,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecorationKind {
    Underline { wavy: bool },
    Strikethrough,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DecorationSpan {
    pub col: u16,
    pub cols: u16,
    pub kind: DecorationKind,
    pub color: Hsla,
}

/// Everything the painter needs for one row.
#[derive(Default)]
pub(crate) struct RowPlan {
    /// Hash of the row this plan was built from; `0` = never built.
    pub hash: u64,
    pub bg: Vec<BgSpan>,
    pub text: Vec<TextRunPlan>,
    pub colors: Vec<ColorSpan>,
    pub shapes: Vec<ShapeQuad>,
    pub decorations: Vec<DecorationSpan>,
}

impl RowPlan {
    fn clear(&mut self) {
        self.hash = 0;
        self.bg.clear();
        self.text.clear();
        self.colors.clear();
        self.shapes.clear();
        self.decorations.clear();
    }

    /// Mark the plan stale without dropping its buffers.
    pub(crate) fn invalidate(&mut self) {
        self.hash = 0;
    }

    pub(crate) fn colors_of(&self, run: &TextRunPlan) -> &[ColorSpan] {
        &self.colors[run.color_start as usize..run.color_end as usize]
    }
}

/// Reusable per-row working buffers (HLD "Cross-frame State").
#[derive(Default)]
pub(crate) struct Scratch {
    pub line_text: String,
    pub char_cols: Vec<u16>,
    pub char_wide: Vec<bool>,
    pub class_chars: Vec<u8>,
    pub class: Vec<u8>,
    pub run_text: String,
    pub rects: Vec<DeviceRect>,
    /// Indices into `RowPlan::shapes` of the previous column's rects, which a
    /// new rect may extend instead of being pushed.
    pub open_prev: Vec<usize>,
    pub open_cur: Vec<usize>,
    pub label: String,
}

impl Scratch {
    pub(crate) fn new() -> Self {
        Self {
            line_text: String::with_capacity(256),
            char_cols: Vec::with_capacity(256),
            char_wide: Vec::with_capacity(256),
            class_chars: Vec::with_capacity(256),
            class: Vec::with_capacity(256),
            run_text: String::with_capacity(256),
            rects: Vec::with_capacity(32),
            open_prev: Vec::with_capacity(8),
            open_cur: Vec::with_capacity(8),
            label: String::with_capacity(32),
        }
    }
}

/// Frame-constant inputs of row planning.
pub(crate) struct PlanContext<'a> {
    pub theme: &'a TerminalTheme,
    pub fonts: &'a FontSet,
    pub font_size: Pixels,
    pub cell_width: Pixels,
    pub device: CellSizeDevicePx,
    /// `None` when semantic highlighting is disabled.
    pub semantic: Option<&'a SemanticOverlay>,
    pub window: &'a Window,
}

/// Resolved style of one cell.
#[derive(Clone, Copy)]
struct CellStyle {
    fg: Hsla,
    bg: Hsla,
    /// Paint a background quad (non-default bg or inverse).
    paint_bg: bool,
    bold: bool,
    italic: bool,
    class: ClassStyle,
}

fn resolve_style(cell: &Cell<'_>, class: u8, theme: &TerminalTheme) -> CellStyle {
    let inverse = cell.flags.contains(CellFlags::INVERSE);
    let (fg_color, bg_color) = if inverse {
        (cell.bg, cell.fg)
    } else {
        (cell.fg, cell.bg)
    };
    let class = theme.class_styles.style(class);
    let mut fg = theme.color(fg_color);
    let bg = theme.color(bg_color);
    if let Some(class_fg) = class.fg
        && (fg_color == Color::Foreground || class.override_ansi)
    {
        fg = to_gpui_hsla(class_fg);
    }
    if cell.flags.contains(CellFlags::DIM) {
        fg.a *= DIM_ALPHA;
    }
    if !fg_color.is_app_chosen_exact() && !is_decorative_character(cell.ch) {
        fg = theme.ensure_contrast(fg, bg);
    }
    CellStyle {
        fg,
        bg,
        paint_bg: inverse || bg_color != Color::Background,
        bold: cell.flags.contains(CellFlags::BOLD) || class.font.bold,
        italic: cell.flags.contains(CellFlags::ITALIC) || class.font.italic,
        class,
    }
}

fn push_bg(plan: &mut RowPlan, col: u16, color: Hsla) {
    if let Some(last) = plan.bg.last_mut()
        && last.color == color
        && last.col + last.cols == col
    {
        last.cols += 1;
        return;
    }
    plan.bg.push(BgSpan {
        col,
        cols: 1,
        color,
    });
}

fn push_decoration(plan: &mut RowPlan, col: u16, cols: u16, kind: DecorationKind, color: Hsla) {
    if let Some(last) = plan.decorations.last_mut()
        && last.kind == kind
        && last.color == color
        && last.col + last.cols == col
    {
        last.cols += cols;
        return;
    }
    plan.decorations.push(DecorationSpan {
        col,
        cols,
        kind,
        color,
    });
}

fn push_decorations(plan: &mut RowPlan, cell: &Cell<'_>, col: u16, cols: u16, style: &CellStyle) {
    let underline = if cell.flags.contains(CellFlags::UNDERLINE) || cell.hyperlink.is_some() {
        Some(cell.flags.contains(CellFlags::UNDERCURL))
    } else if style.class.deco == Decoration::Underline {
        Some(false)
    } else {
        None
    };
    if let Some(wavy) = underline {
        push_decoration(
            plan,
            col,
            cols,
            DecorationKind::Underline { wavy },
            style.fg,
        );
    }
    if cell.flags.contains(CellFlags::STRIKEOUT) {
        push_decoration(plan, col, cols, DecorationKind::Strikethrough, style.fg);
    }
}

/// An open text run while walking the row.
struct OpenRun {
    col: u16,
    cols: u16,
    bold: bool,
    italic: bool,
    font: FontKey,
    forced: bool,
    color_start: u32,
    last_color: Hsla,
}

struct RowBuilder<'a, 'b> {
    ctx: &'a PlanContext<'b>,
    scratch: &'a mut Scratch,
    glyphs: &'a mut GlyphCache,
    stats: &'a mut FrameStats,
    plan: &'a mut RowPlan,
    run: Option<OpenRun>,
    /// Column of the last shape cell, so only touching cells coalesce.
    last_shape_col: Option<u16>,
}

impl RowBuilder<'_, '_> {
    fn flush_run(&mut self) {
        let Some(run) = self.run.take() else {
            return;
        };
        let text_len = self.scratch.run_text.len() as u32;
        if let Some(last) = self.plan.colors.last_mut() {
            last.byte_end = text_len;
        }
        let (font, _) = self.ctx.fonts.get(run.bold, run.italic);
        let force_width = if run.forced {
            Some(self.ctx.cell_width)
        } else {
            None
        };
        let line = self.glyphs.shape(
            &self.scratch.run_text,
            font,
            run.font,
            self.ctx.font_size,
            force_width,
            self.ctx.window,
            self.stats,
        );
        self.plan.text.push(TextRunPlan {
            col: run.col,
            #[cfg(test)]
            cols: run.cols,
            line,
            color_start: run.color_start,
            color_end: self.plan.colors.len() as u32,
        });
        self.scratch.run_text.clear();
    }

    fn append_to_run(&mut self, cell: &Cell<'_>, col: u16, cols: u16, style: &CellStyle) {
        let forced = cols == 1;
        let needs_new = match &self.run {
            Some(run) => {
                run.bold != style.bold || run.italic != style.italic || run.forced != forced
            }
            None => true,
        };
        if needs_new {
            self.flush_run();
            let (_, font) = self.ctx.fonts.get(style.bold, style.italic);
            self.plan.colors.push(ColorSpan {
                byte_end: 0,
                color: style.fg,
            });
            self.run = Some(OpenRun {
                col,
                cols: 0,
                bold: style.bold,
                italic: style.italic,
                font,
                forced,
                color_start: self.plan.colors.len() as u32 - 1,
                last_color: style.fg,
            });
        }
        let text_len = self.scratch.run_text.len() as u32;
        if let Some(run) = self.run.as_mut() {
            if run.last_color != style.fg {
                if let Some(last) = self.plan.colors.last_mut() {
                    last.byte_end = text_len;
                }
                self.plan.colors.push(ColorSpan {
                    byte_end: 0,
                    color: style.fg,
                });
                run.last_color = style.fg;
            }
            run.cols += cols;
        }
        self.scratch.run_text.push(cell.ch);
        for &z in cell.zerowidth {
            self.scratch.run_text.push(z);
        }
        // A wide char is always its own run: `force_width` shaping treats
        // the next glyph as a new base and would misplace it.
        if !forced {
            self.flush_run();
        }
    }

    fn push_shape(&mut self, ch: char, col: u16, color: Hsla) {
        let device = self.ctx.device;
        let touching = self.last_shape_col == Some(col.wrapping_sub(1));
        self.last_shape_col = Some(col);
        if !touching {
            self.scratch.open_prev.clear();
        }
        self.scratch.open_cur.clear();
        self.scratch.rects.clear();
        shape_quads(ch, device, &mut self.scratch.rects);
        let x_offset = i32::from(col) * device.w;
        for rect in self.scratch.rects.iter() {
            let rect = DeviceRect {
                x: rect.x + x_offset,
                ..*rect
            };
            // Coverage becomes color alpha; equal-alpha neighbours still
            // coalesce because the comparison below is on the final color.
            let color = Hsla {
                a: color.a * rect.alpha,
                ..color
            };
            let extended = self.scratch.open_prev.iter().copied().find(|&i| {
                let prev = &self.plan.shapes[i];
                prev.color == color
                    && prev.rect.y == rect.y
                    && prev.rect.h == rect.h
                    && prev.rect.x + prev.rect.w == rect.x
            });
            match extended {
                Some(i) => {
                    self.plan.shapes[i].rect.w += rect.w;
                    self.scratch.open_cur.push(i);
                }
                None => {
                    self.scratch.open_cur.push(self.plan.shapes.len());
                    self.plan.shapes.push(ShapeQuad { rect, color });
                }
            }
        }
        std::mem::swap(&mut self.scratch.open_prev, &mut self.scratch.open_cur);
    }
}

/// Fill `scratch.class` with one class byte per column of `row`.
fn classify(row: &FrameRow<'_>, ctx: &PlanContext<'_>, url_mask: &[bool], scratch: &mut Scratch) {
    let cols = row.len();
    scratch.class.clear();
    scratch.class.resize(cols, Class::Default as u8);
    if let Some(overlay) = ctx.semantic {
        row.text_into(
            &mut scratch.line_text,
            &mut scratch.char_cols,
            &mut scratch.char_wide,
        );
        // Blank rows carry nothing to classify; skip the scanner entirely.
        if !scratch.line_text.trim().is_empty() {
            overlay.scan_into(&scratch.line_text, row.index(), &mut scratch.class_chars);
            for (i, &class) in scratch.class_chars.iter().enumerate() {
                let Some(&col) = scratch.char_cols.get(i) else {
                    break;
                };
                let col = usize::from(col);
                scratch.class[col] = class;
                if scratch.char_wide.get(i).copied().unwrap_or(false) && col + 1 < cols {
                    scratch.class[col + 1] = class;
                }
            }
        }
    }
    for (col, &masked) in url_mask.iter().enumerate().take(cols) {
        if masked {
            scratch.class[col] = Class::Url as u8;
        }
    }
}

/// Rebuild `plan` for `row`. `url_mask` may be shorter than the row (or empty)
/// when no URL was detected.
pub(crate) fn build_row_plan(
    row: FrameRow<'_>,
    ctx: &PlanContext<'_>,
    url_mask: &[bool],
    scratch: &mut Scratch,
    glyphs: &mut GlyphCache,
    stats: &mut FrameStats,
    plan: &mut RowPlan,
) {
    plan.clear();
    classify(&row, ctx, url_mask, scratch);
    scratch.run_text.clear();
    scratch.open_prev.clear();
    let theme = ctx.theme;
    let mut builder = RowBuilder {
        ctx,
        scratch,
        glyphs,
        stats,
        plan,
        run: None,
        last_shape_col: None,
    };

    for (col, cell) in row.cells().enumerate() {
        let col16 = col as u16;
        let class = builder.scratch.class[col];
        let style = resolve_style(&cell, class, theme);
        if style.paint_bg {
            push_bg(builder.plan, col16, style.bg);
        }
        if cell.is_spacer() {
            builder.flush_run();
            continue;
        }
        if cell.flags.contains(CellFlags::HIDDEN) {
            builder.flush_run();
            continue;
        }
        let wide = cell.flags.contains(CellFlags::WIDE_CHAR);
        let cols = if wide { 2 } else { 1 };
        push_decorations(builder.plan, &cell, col16, cols, &style);
        if is_shape_char(cell.ch) {
            builder.flush_run();
            builder.push_shape(cell.ch, col16, style.fg);
            continue;
        }
        if matches!(cell.ch, ' ' | '\0') && cell.zerowidth.is_empty() {
            builder.flush_run();
            continue;
        }
        builder.append_to_run(&cell, col16, cols, &style);
    }
    builder.flush_run();
    plan.hash = row.hash();
}

#[cfg(test)]
mod tests {
    use gpui::{Font, FontFeatures, FontStyle, FontWeight, TestAppContext, px};
    use oneterm_highlight::ShellProfile;

    use super::*;
    use crate::render::frame::Frame;
    use crate::render::frame::test_support::FrameBuilder;
    use crate::theme::build_terminal_theme;

    fn font() -> Font {
        Font {
            family: "Test Mono".into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        }
    }

    struct Fixture {
        theme: TerminalTheme,
        fonts: FontSet,
        semantic: Option<SemanticOverlay>,
    }

    impl Fixture {
        fn new(semantic: bool) -> Self {
            Self {
                theme: build_terminal_theme(&gpui_component::Theme::default()),
                fonts: FontSet::new(&font(), px(13.0)),
                semantic: semantic.then(|| SemanticOverlay::new(ShellProfile::Unix, true)),
            }
        }

        fn plan(
            &self,
            cx: &mut TestAppContext,
            frame: &Frame,
            row: usize,
            mask: &[bool],
        ) -> RowPlan {
            let cx = cx.add_empty_window();
            let mut plan = RowPlan::default();
            cx.update(|window, _| {
                let ctx = PlanContext {
                    theme: &self.theme,
                    fonts: &self.fonts,
                    font_size: px(13.0),
                    cell_width: px(8.0),
                    device: CellSizeDevicePx { w: 8, h: 16 },
                    semantic: self.semantic.as_ref(),
                    window,
                };
                let mut scratch = Scratch::new();
                let mut glyphs = GlyphCache::new();
                let mut stats = FrameStats::default();
                build_row_plan(
                    frame.row(row),
                    &ctx,
                    mask,
                    &mut scratch,
                    &mut glyphs,
                    &mut stats,
                    &mut plan,
                );
            });
            plan
        }
    }

    #[gpui::test]
    fn bg_spans_merge_adjacent_same_color(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 8)
            .styled(
                0,
                1,
                'a',
                Color::Foreground,
                Color::Ansi(4),
                CellFlags::NONE,
            )
            .styled(
                0,
                2,
                'b',
                Color::Foreground,
                Color::Ansi(4),
                CellFlags::NONE,
            )
            .styled(
                0,
                3,
                ' ',
                Color::Foreground,
                Color::Ansi(4),
                CellFlags::NONE,
            )
            .styled(
                0,
                5,
                'c',
                Color::Foreground,
                Color::Ansi(2),
                CellFlags::NONE,
            )
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.bg.len(), 2, "{:?}", plan.bg);
        assert_eq!((plan.bg[0].col, plan.bg[0].cols), (1, 3));
        assert_eq!((plan.bg[1].col, plan.bg[1].cols), (5, 1));
        assert_ne!(plan.hash, 0);
    }

    #[gpui::test]
    fn inverse_swaps_colors_and_forces_bg(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 4)
            .styled(
                0,
                0,
                'x',
                Color::Ansi(1),
                Color::Background,
                CellFlags::INVERSE,
            )
            .build();
        let fx = Fixture::new(false);
        let plan = fx.plan(cx, &frame, 0, &[]);
        assert_eq!(plan.bg.len(), 1);
        assert_eq!(plan.bg[0].color, fx.theme.color(Color::Ansi(1)));
        assert_eq!(plan.text.len(), 1);
        let fg = plan.colors_of(&plan.text[0])[0].color;
        let expected = fx.theme.ensure_contrast(
            fx.theme.color(Color::Background),
            fx.theme.color(Color::Ansi(1)),
        );
        assert_eq!(fg, expected);
    }

    #[gpui::test]
    fn dim_reduces_alpha(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 4)
            .styled(
                0,
                0,
                'x',
                Color::Rgb(200, 50, 50),
                Color::Background,
                CellFlags::DIM,
            )
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        let fg = plan.colors_of(&plan.text[0])[0].color;
        assert!((fg.a - DIM_ALPHA).abs() < 1e-5, "alpha {}", fg.a);
    }

    #[gpui::test]
    fn hidden_cells_have_no_text_run(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 6)
            .text(0, 0, "ab")
            .styled(
                0,
                2,
                'c',
                Color::Foreground,
                Color::Ansi(3),
                CellFlags::HIDDEN,
            )
            .text(0, 3, "de")
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.text.len(), 2, "hidden cell splits the run");
        assert_eq!((plan.text[0].col, plan.text[0].cols), (0, 2));
        assert_eq!((plan.text[1].col, plan.text[1].cols), (3, 2));
        assert_eq!(plan.bg.len(), 1, "hidden cell keeps its background");
    }

    #[gpui::test]
    fn undercurl_maps_to_wavy(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 6)
            .styled(
                0,
                0,
                'a',
                Color::Foreground,
                Color::Background,
                CellFlags::UNDERLINE.union(CellFlags::UNDERCURL),
            )
            .styled(
                0,
                1,
                'b',
                Color::Foreground,
                Color::Background,
                CellFlags::UNDERLINE,
            )
            .styled(
                0,
                2,
                'c',
                Color::Foreground,
                Color::Background,
                CellFlags::STRIKEOUT,
            )
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.decorations.len(), 3, "{:?}", plan.decorations);
        assert_eq!(
            plan.decorations[0].kind,
            DecorationKind::Underline { wavy: true }
        );
        assert_eq!(
            plan.decorations[1].kind,
            DecorationKind::Underline { wavy: false }
        );
        assert_eq!(plan.decorations[2].kind, DecorationKind::Strikethrough);
        assert_eq!(plan.text.len(), 1, "decorations do not split runs");
    }

    #[gpui::test]
    fn class_underline_only_without_ansi_underline(cx: &mut TestAppContext) {
        // The URL mask (authoritative, §2.15) classifies the cells `Url`,
        // whose default class style underlines them.
        let frame = FrameBuilder::new(1, 20)
            .text(0, 0, "https://x.test")
            .build();
        let mut mask = vec![false; 20];
        mask[..14].fill(true);
        let plan = Fixture::new(true).plan(cx, &frame, 0, &mask);
        assert_eq!(plan.decorations.len(), 1, "{:?}", plan.decorations);
        assert_eq!(
            plan.decorations[0].kind,
            DecorationKind::Underline { wavy: false }
        );
        assert_eq!((plan.decorations[0].col, plan.decorations[0].cols), (0, 14));

        let curly = FrameBuilder::new(1, 20)
            .text(0, 0, "https://x.test")
            .flags(0, 0, CellFlags::UNDERLINE.union(CellFlags::UNDERCURL))
            .build();
        let plan = Fixture::new(true).plan(cx, &curly, 0, &mask);
        assert_eq!(
            plan.decorations[0].kind,
            DecorationKind::Underline { wavy: true }
        );
        assert_eq!(
            plan.decorations[0].cols, 1,
            "ANSI underline wins for its cell"
        );
        assert_eq!(
            plan.decorations[1].cols, 13,
            "the class underline covers the rest"
        );
    }

    #[gpui::test]
    fn url_mask_forces_url_class(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 10).text(0, 0, "plain text").build();
        let mut mask = vec![false; 10];
        mask[6..10].fill(true);
        let fx = Fixture::new(false);
        let plan = fx.plan(cx, &frame, 0, &mask);
        assert_eq!(plan.decorations.len(), 1);
        assert_eq!((plan.decorations[0].col, plan.decorations[0].cols), (6, 4));
        let url_fg = to_gpui_hsla(
            fx.theme
                .class_styles
                .style(Class::Url as u8)
                .fg
                .expect("default styles color urls"),
        );
        let bg = fx.theme.color(Color::Background);
        let text_fg = plan.colors_of(&plan.text[1])[0].color;
        assert_eq!(text_fg, fx.theme.ensure_contrast(url_fg, bg));
        let plain = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert!(plain.decorations.is_empty());
    }

    #[gpui::test]
    fn wide_char_occupies_two_columns_one_run(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 6)
            .text(0, 0, "a")
            .wide(0, 1, '日')
            .text(0, 3, "b")
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.text.len(), 3, "{:?}", plan.text);
        assert_eq!((plan.text[1].col, plan.text[1].cols), (1, 2));
        assert_eq!((plan.text[2].col, plan.text[2].cols), (3, 1));
    }

    #[gpui::test]
    fn zero_width_marks_join_base_run(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 6)
            .text(0, 0, "ab")
            .zerowidth(0, 1, &['\u{301}'])
            .text(0, 2, "c")
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.text.len(), 1);
        assert_eq!(plan.text[0].cols, 3);
        assert_eq!(plan.text[0].line.len(), "ab\u{301}c".len());
    }

    #[gpui::test]
    fn overflow_slot_keeps_background(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 6)
            .text(0, 0, "e")
            .zerowidth(0, 0, &['\u{301}'])
            .styled(
                0,
                1,
                ' ',
                Color::Foreground,
                Color::Ansi(5),
                CellFlags::NONE,
            )
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.text.len(), 1, "the overflow slot has no run");
        assert_eq!(plan.text[0].cols, 1);
        assert_eq!(plan.bg.len(), 1);
        assert_eq!((plan.bg[0].col, plan.bg[0].cols), (1, 1));
    }

    #[gpui::test]
    fn block_run_coalesces_into_one_quad(cx: &mut TestAppContext) {
        let blocks: String = "█".repeat(40);
        let frame = FrameBuilder::new(1, 40).text(0, 0, &blocks).build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.shapes.len(), 1, "{:?}", plan.shapes);
        assert_eq!(plan.shapes[0].rect.w, 40 * 8);
        assert!(plan.text.is_empty());

        let lines: String = "─".repeat(40);
        let frame = FrameBuilder::new(1, 40).text(0, 0, &lines).build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.shapes.len(), 1, "{:?}", plan.shapes);
        assert_eq!(plan.shapes[0].rect.w, 40 * 8);

        let alternating: String = "▀▄".repeat(20);
        let frame = FrameBuilder::new(1, 40).text(0, 0, &alternating).build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.shapes.len(), 40, "alternating bands never coalesce");

        let gap = FrameBuilder::new(1, 5).text(0, 0, "█ █").build();
        let plan = Fixture::new(false).plan(cx, &gap, 0, &[]);
        assert_eq!(plan.shapes.len(), 2, "a gap breaks the run");
    }

    #[gpui::test]
    fn color_spans_follow_fg_changes_inside_a_run(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 6)
            .styled(
                0,
                0,
                'a',
                Color::Rgb(255, 0, 0),
                Color::Background,
                CellFlags::NONE,
            )
            .styled(
                0,
                1,
                'b',
                Color::Rgb(255, 0, 0),
                Color::Background,
                CellFlags::NONE,
            )
            .styled(
                0,
                2,
                'c',
                Color::Rgb(0, 0, 255),
                Color::Background,
                CellFlags::NONE,
            )
            .styled(
                0,
                3,
                'd',
                Color::Rgb(0, 0, 255),
                Color::Background,
                CellFlags::BOLD,
            )
            .build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        assert_eq!(plan.text.len(), 2, "bold starts a new run");
        let spans = plan.colors_of(&plan.text[0]);
        assert_eq!(spans.len(), 2, "{spans:?}");
        assert_eq!(spans[0].byte_end, 2);
        assert_eq!(spans[1].byte_end, 3);
    }

    #[gpui::test]
    fn rounded_corner_emits_coverage_quads(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 3).text(0, 1, "╭").build();
        let plan = Fixture::new(false).plan(cx, &frame, 0, &[]);
        let solid = plan.shapes.iter().filter(|q| q.color.a == 1.0).count();
        let edges = plan
            .shapes
            .iter()
            .filter(|q| q.color.a > 0.0 && q.color.a < 1.0)
            .count();
        assert!(solid > 0, "the arc has no solid pixels: {:?}", plan.shapes);
        assert!(
            edges > 0,
            "the arc has no anti-aliased edge: {:?}",
            plan.shapes
        );
        for quad in &plan.shapes {
            assert!(
                (quad.color.a - quad.rect.alpha).abs() < 1e-6,
                "coverage not multiplied into the color: {quad:?}"
            );
        }
    }
}
