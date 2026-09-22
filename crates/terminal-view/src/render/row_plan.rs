//! One display row turned into paint-ready primitives: background spans,
//! shaped text runs with color spans, coalesced shape quads and decorations. Built only for rows whose hash changed; every vector is
//! cleared and refilled, never replaced, so a rebuild allocates nothing once
//! the row has been planned once at this width.

use std::ops::Range;

use gpui::{Hsla, Pixels, ShapedLine, Window};
use oneterm_highlight::{Class, ClassStyle, Decoration, RowRole, tint_prompt_sign};
use oneterm_terminal::{Semantic, is_decorative_character};

use super::diagnostics::FrameStats;
use super::frame::{Cell, CellFlags, Color, Frame, FrameRow};
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
    /// Range into `RowPlan::cell_starts`: the run-text byte offset where each
    /// of the run's cells begins. The painter anchors every glyph at the cell
    /// its byte index falls in, so a ligature (one glyph for several bytes)
    /// does not pull the following glyphs left.
    pub cells_start: u32,
    pub cells_end: u32,
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
    pub bg: Vec<BgSpan>,
    pub text: Vec<TextRunPlan>,
    pub colors: Vec<ColorSpan>,
    pub cell_starts: Vec<u32>,
    pub shapes: Vec<ShapeQuad>,
    pub decorations: Vec<DecorationSpan>,
}

impl RowPlan {
    fn clear(&mut self) {
        self.bg.clear();
        self.text.clear();
        self.colors.clear();
        self.cell_starts.clear();
        self.shapes.clear();
        self.decorations.clear();
    }

    pub(crate) fn colors_of(&self, run: &TextRunPlan) -> &[ColorSpan] {
        &self.colors[run.color_start as usize..run.color_end as usize]
    }

    pub(crate) fn cells_of(&self, run: &TextRunPlan) -> &[u32] {
        &self.cell_starts[run.cells_start as usize..run.cells_end as usize]
    }
}

/// Reusable per-row working buffers (HLD "Cross-frame State").
#[derive(Default)]
pub(crate) struct Scratch {
    /// The joined text of one logical line — the wrap-connected run of display
    /// rows — which the semantic scanner reads as a single line (`BUG-0071`).
    pub line_text: String,
    /// The display row every char of `line_text` came from.
    pub char_rows: Vec<u16>,
    pub char_cols: Vec<u16>,
    pub char_wide: Vec<bool>,
    /// The OSC 133 region every char of `line_text` was written in.
    pub char_semantic: Vec<Semantic>,
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
            char_rows: Vec::with_capacity(256),
            char_cols: Vec::with_capacity(256),
            char_wide: Vec::with_capacity(256),
            char_semantic: Vec::with_capacity(256),
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

/// A bold cell draws its shapes this much heavier than the settings weight
/// (CSS scale, capped at 900), the same step as normal -> bold text.
const BOLD_WEIGHT_STEP: f32 = 300.0;
const MAX_FONT_WEIGHT: f32 = 900.0;

/// Frame-constant inputs of row planning.
pub(crate) struct PlanContext<'a> {
    pub theme: &'a TerminalTheme,
    pub fonts: &'a FontSet,
    pub font_size: Pixels,
    /// The settings font weight (CSS scale); the shape strokes of a bold cell
    /// use `min(900, font_weight + 300)`.
    pub font_weight: f32,
    pub cell_width: Pixels,
    pub device: CellSizeDevicePx,
    /// `None` when semantic highlighting is disabled.
    pub semantic: Option<&'a SemanticOverlay>,
    /// `DECSCNM` (`? 5`): draw the whole screen with the two defaults swapped.
    pub reverse_video: bool,
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

/// `row_bg` is what is actually behind a cell that paints no background of its
/// own: the terminal background, or the prompt-line band on a prompt row
/// (`US-0134`). It is the reference the contrast pass measures against, which is
/// what keeps every foreground legible on the band without extending the theme
/// contrast gate to colours it cannot see.
fn resolve_style(
    cell: &Cell<'_>,
    class: u8,
    theme: &TerminalTheme,
    reverse_video: bool,
    row_bg: Hsla,
) -> CellStyle {
    let inverse = cell.flags.contains(CellFlags::INVERSE);
    let (fg_color, bg_color) = if inverse {
        (cell.bg, cell.fg)
    } else {
        (cell.fg, cell.bg)
    };
    // `DECSCNM`: the whole screen is drawn with the two **defaults** swapped,
    // and no cell's own style changes — the engine never touches a cell for it.
    // So the swap belongs here, where a default becomes a pixel, and nowhere
    // else: a cell that named a colour still gets the colour it named, and
    // clearing the mode restores the screen exactly.
    let (fg_color, bg_color) = if reverse_video {
        (swap_default(fg_color), swap_default(bg_color))
    } else {
        (fg_color, bg_color)
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
    let paint_bg = inverse || bg_color != Color::Background;
    if !fg_color.is_app_chosen_exact() && !is_decorative_character(cell.ch) {
        // A cell that paints no background of its own shows whatever is under
        // the row, which on a prompt row is the band and not the theme default.
        fg = theme.ensure_contrast(fg, if paint_bg { bg } else { row_bg });
    }
    CellStyle {
        fg,
        bg,
        paint_bg,
        bold: cell.flags.contains(CellFlags::BOLD) || class.font.bold,
        italic: cell.flags.contains(CellFlags::ITALIC) || class.font.italic,
        class,
    }
}

/// `Color::Foreground` and `Color::Background` trade places; every other colour
/// is what the program asked for and is left alone.
fn swap_default(color: Color) -> Color {
    match color {
        Color::Foreground => Color::Background,
        Color::Background => Color::Foreground,
        other => other,
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
    cells_start: u32,
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
            cells_start: run.cells_start,
            cells_end: self.plan.cell_starts.len() as u32,
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
                cells_start: self.plan.cell_starts.len() as u32,
                last_color: style.fg,
            });
        }
        let text_len = self.scratch.run_text.len() as u32;
        self.plan.cell_starts.push(text_len);
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

    fn push_shape(&mut self, ch: char, col: u16, color: Hsla, bold: bool) {
        let device = self.ctx.device;
        let touching = self.last_shape_col == Some(col.wrapping_sub(1));
        self.last_shape_col = Some(col);
        if !touching {
            self.scratch.open_prev.clear();
        }
        self.scratch.open_cur.clear();
        self.scratch.rects.clear();
        let weight = if bold {
            (self.ctx.font_weight + BOLD_WEIGHT_STEP).min(MAX_FONT_WEIGHT)
        } else {
            self.ctx.font_weight
        };
        shape_quads(ch, device, weight, &mut self.scratch.rects);
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
            // The comparison is by rect, so a bold cell (thicker strokes)
            // never extends the quads of a plain neighbour.
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

/// Semantic classes for the display rows in `range`, scanned one **logical**
/// line at a time and written into `classes[range]`.
///
/// A logical line is a wrap-connected run of display rows: `wraps[r]` means row
/// `r + 1` continues row `r`. The scanner's state — inside a quoted string,
/// after the prompt sign, mid-token — belongs to that whole line, so the run is
/// joined into one string, scanned once, and the resulting classes are sliced
/// back per visual row (`BUG-0071`). Scanning each row on its own restarted the
/// state at every wrap: a prompt whose cwd wrapped lost its prompt sign, a
/// string opened on one row ended at the row edge, and a resize that moved the
/// wrap point changed the colours of text that had not changed.
///
/// The logical line's own cells supply its role (`US-0133`): `roles[r]` is set
/// for every row of the run, so a wrapped prompt is a prompt on all of its rows
/// — which is what the prompt-line background reads (`US-0134`). `wraps` holds
/// the frame's per-row `WRAPLINE` flags
/// ([`fill_wraps`](crate::url::fill_wraps)), and the inner buffers of the rows
/// in `range` are reused so a per-frame recomputation allocates nothing.
///
/// **`range` must be closed under wrap runs**, exactly as for the URL masks:
/// every class in the range then depends only on rows inside it, which is what
/// lets the caller rescan the changed runs rather than the viewport (§10 of
/// `docs/terminal-semantic-highlighting.md`).
///
/// **A run whose head is above the viewport starts at the first visible row**,
/// and takes its role from there. `frame` is the viewport and nothing else
/// (`SnapshotState::rows`), so the rows above it cannot be read here. That is a
/// decided rule, not an oversight: it is the same limit the URL pass has carried
/// since `US-0092`, it costs at most the classes of the one partial run at the
/// top of the screen until its head scrolls back into view, and §10 records why
/// lifting it is not worth a second invalidation edge (`US-0135`). The same
/// limit is why the viewport's first line has no known predecessor and is never
/// a marked prompt (`US-0133`).
#[allow(clippy::too_many_arguments)] // one output buffer per pass, plus the bounds
pub(crate) fn class_rows_into(
    frame: &Frame,
    overlay: &SemanticOverlay,
    classes: &mut [Vec<u8>],
    roles: &mut [Option<RowRole>],
    heads: &mut [Semantic],
    wraps: &[bool],
    range: Range<usize>,
    scratch: &mut Scratch,
    stats: &mut FrameStats,
) {
    debug_assert!(range.end <= classes.len() && range.end <= wraps.len());
    for r in range.clone() {
        let out = &mut classes[r];
        out.clear();
        out.resize(frame.row(r).len(), Class::Default as u8);
        roles[r] = None;
        heads[r] = Semantic::None;
    }
    stats.class_rows_scanned += range.len() as u32;

    let mut start = range.start;
    while start < range.end {
        let mut end = start;
        while end + 1 < range.end && wraps[end] {
            end += 1;
        }
        stats.class_scans += 1;
        scan_logical_line(frame, overlay, classes, roles, heads, start..=end, scratch);
        start = end + 1;
    }
}

/// The OSC 133 region of a logical line: the region of its **first marked
/// character**, or `Semantic::None` when no character of it is marked.
///
/// This is the raw reading, before the transition rule below. It is kept per
/// row so the next logical line can ask what the previous one was.
fn line_head_region(char_semantic: &[Semantic]) -> Semantic {
    char_semantic
        .iter()
        .copied()
        .find(|&s| s != Semantic::None)
        .unwrap_or(Semantic::None)
}

/// The role of one logical line and the char index its typed command starts at.
///
/// `head` is this line's region ([`line_head_region`]) and `prev` the previous
/// logical line's, or `None` when there is no previous line inside the viewport
/// — which is *unknown*, not "not a prompt".
///
/// **A prompt line is a line the region TRANSITIONS into.** The OSC 133 region
/// is a sticky cell attribute: `OSC 133;A` puts `Semantic::Prompt` on the cell
/// template and only another OSC 133 takes it off, so a shell that emits `A` and
/// nothing else — OneTerm's own bash `PROMPT_COMMAND`
/// (`crates/core/src/config/shell.rs`) and the SSH bootstrap
/// (`crates/ssh/src/session.rs`), which runs on *every* remote session — leaves
/// every line it prints afterwards tagged `Prompt`. Reading the tag alone made
/// a whole screen of output into prompt lines, with invented signs, command
/// colouring and a background band on every row. Requiring the transition costs
/// nothing where the shell closes the region (`A`/`B`: the prompt follows an
/// `Input` or `Output` line; `A`/`B`/`C`/`D`: it follows `Output`) and falls back
/// to the prompt regex — today's behaviour — where it does not.
///
/// The same reasoning in the other direction is the `Input` rule: a line that
/// *starts* in `Input` is not trusted as command input, because `cmd.exe`'s
/// built-in `PROMPT` emits `A` and `B` and never `C`, so every line it prints
/// afterwards is tagged `Input`. A typed command is unaffected: it continues its
/// prompt's logical line, which is `Prompt`-headed and carries `input_at`.
fn line_role(
    head: Semantic,
    prev: Option<Semantic>,
    char_semantic: &[Semantic],
) -> (Option<RowRole>, Option<usize>) {
    match head {
        // A mixed-region line — the prompt and the command the user typed on one
        // row — starts in `PromptLine` and switches where the region changes, so
        // the boundary column travels with the role rather than being guessed.
        Semantic::Prompt if prev.is_some_and(|p| p != Semantic::Prompt) => (
            Some(RowRole::Prompt),
            char_semantic.iter().position(|&s| s == Semantic::Input),
        ),
        Semantic::Output => (Some(RowRole::Output), None),
        Semantic::Prompt | Semantic::Input | Semantic::None => (None, None),
    }
}

/// Scan one logical line (`rows`, a wrap run) and scatter its classes back to
/// the columns of the rows it came from.
#[allow(clippy::too_many_arguments)] // one output buffer per pass, plus the run
fn scan_logical_line(
    frame: &Frame,
    overlay: &SemanticOverlay,
    classes: &mut [Vec<u8>],
    roles: &mut [Option<RowRole>],
    heads: &mut [Semantic],
    rows: std::ops::RangeInclusive<usize>,
    scratch: &mut Scratch,
) {
    scratch.line_text.clear();
    scratch.char_rows.clear();
    scratch.char_cols.clear();
    scratch.char_wide.clear();
    scratch.char_semantic.clear();
    for r in rows.clone() {
        frame.row(r).append_text_into(
            &mut scratch.line_text,
            &mut scratch.char_rows,
            &mut scratch.char_cols,
            &mut scratch.char_wide,
            &mut scratch.char_semantic,
        );
    }
    // The previous logical line's region, for the transition rule. The last row
    // before this run belongs to it, and `heads` is authoritative for every row
    // of the viewport (a row outside this frame's rescan did not change, so
    // neither did its region). No previous row means the viewport's top edge cut
    // the history: unknown, not "not a prompt".
    let prev = rows.start().checked_sub(1).map(|r| heads[r]);
    let head = line_head_region(&scratch.char_semantic);
    let (role, input_at) = line_role(head, prev, &scratch.char_semantic);
    for r in rows {
        roles[r] = role;
        heads[r] = head;
    }
    // A blank line carries nothing to classify; skip the scanner entirely.
    if scratch.line_text.trim().is_empty() {
        return;
    }
    overlay.scan_into(&scratch.line_text, role, input_at, &mut scratch.class_chars);
    for (i, &class) in scratch.class_chars.iter().enumerate() {
        let (Some(&row), Some(&col)) = (scratch.char_rows.get(i), scratch.char_cols.get(i)) else {
            break;
        };
        let out = &mut classes[usize::from(row)];
        let col = usize::from(col);
        let Some(slot) = out.get_mut(col) else {
            continue;
        };
        *slot = class;
        // A wide char owns its spacer column too, so a run stays unbroken.
        if scratch.char_wide.get(i).copied().unwrap_or(false) {
            if let Some(spacer) = out.get_mut(col + 1) {
                *spacer = class;
            }
        }
    }
}

/// Fill `scratch.class` with one class byte per column of `row`: the semantic
/// classes computed for its logical line, with URL columns on top and, when
/// this row belongs to the most recently completed command block, its prompt
/// sign re-tagged from the exit code (`US-0133`).
///
/// The tint is applied here rather than inside the scanner because *which*
/// prompt the code belongs to is a fact about the whole viewport, not about the
/// line being scanned — and because a tint that changed inside the scan would
/// have to invalidate a rescan the plan cache has already decided not to do.
fn classify(
    row: &FrameRow<'_>,
    classes: &[u8],
    url_mask: &[bool],
    tint: Option<i32>,
    scratch: &mut Scratch,
) {
    let cols = row.len();
    scratch.class.clear();
    scratch.class.resize(cols, Class::Default as u8);
    let shared = cols.min(classes.len());
    scratch.class[..shared].copy_from_slice(&classes[..shared]);
    if let Some(exit_code) = tint {
        tint_prompt_sign(&mut scratch.class, exit_code);
    }
    for (col, &masked) in url_mask.iter().enumerate().take(cols) {
        if masked {
            scratch.class[col] = Class::Url as u8;
        }
    }
}

/// Push the prompt-line background band (§8 item 6) and return what is behind a
/// cell that paints no background of its own.
///
/// Order is the whole of the mechanism: `RowPlan::bg` is painted in push order,
/// so one full-width rect pushed **before** the per-cell loop goes down first
/// and every explicit cell background — a selection, an ANSI `bg`, an inverse
/// cell — paints on top of it exactly as it does on any other row. Spanning
/// `0..cols` rather than per cell is also what covers a
/// `LEADING_WIDE_CHAR_SPACER` at a wrap boundary: the spacer carries no class,
/// and per-cell painting would leave it as a one-cell hole (§13 Q4).
///
/// Only a **marked** row gets the band. Under the regex fallback a line-level,
/// full-width element that appears and disappears as the regex changes its mind
/// flashes the whole row, which is the defect `BUG-0071` was reported for; a
/// wrong foreground on one word is not.
fn push_prompt_band(
    plan: &mut RowPlan,
    role: Option<RowRole>,
    cols: u16,
    ctx: &PlanContext<'_>,
) -> Hsla {
    let default_bg = ctx.theme.color(if ctx.reverse_video {
        Color::Foreground
    } else {
        Color::Background
    });
    if !matches!(role, Some(RowRole::Prompt | RowRole::Command)) || cols == 0 {
        return default_bg;
    }
    let band = ctx.theme.prompt_line_bg(ctx.reverse_video);
    plan.bg.push(BgSpan {
        col: 0,
        cols,
        color: band,
    });
    band
}

/// Rebuild `plan` for `row`. `classes` holds the row's semantic classes as
/// [`class_rows_into`] computed them for its logical line, and `url_mask` the
/// row's URL columns; either may be shorter than the row (or empty) when
/// semantic highlighting is off or no URL was detected. `tint` is the exit code
/// of the command this row's prompt launched, when it is the most recently
/// completed one (`US-0133`), and `role` is its OSC 133 role, which is what
/// decides the prompt-line band (`US-0134`).
#[allow(clippy::too_many_arguments)] // the frame-constant half is already in `ctx`
pub(crate) fn build_row_plan(
    row: FrameRow<'_>,
    ctx: &PlanContext<'_>,
    classes: &[u8],
    url_mask: &[bool],
    role: Option<RowRole>,
    tint: Option<i32>,
    scratch: &mut Scratch,
    glyphs: &mut GlyphCache,
    stats: &mut FrameStats,
    plan: &mut RowPlan,
) {
    plan.clear();
    classify(&row, classes, url_mask, tint, scratch);
    scratch.run_text.clear();
    scratch.open_prev.clear();
    let theme = ctx.theme;
    let row_bg = push_prompt_band(plan, role, row.len() as u16, ctx);
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
        let style = resolve_style(&cell, class, theme, ctx.reverse_video, row_bg);
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
            builder.push_shape(cell.ch, col16, style.fg, style.bold);
            continue;
        }
        if matches!(cell.ch, ' ' | '\0') && cell.zerowidth.is_empty() {
            builder.flush_run();
            continue;
        }
        builder.append_to_run(&cell, col16, cols, &style);
    }
    builder.flush_run();
}

#[cfg(test)]
mod tests {
    use gpui::{Font, FontFeatures, FontStyle, FontWeight, TestAppContext, px};
    use oneterm_highlight::ShellProfile;

    use super::*;
    use crate::render::frame::test_support::FrameBuilder;
    use crate::theme::build_terminal_theme;
    use crate::url::fill_wraps;

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
        font_weight: f32,
        reverse_video: bool,
    }

    impl Fixture {
        fn new(semantic: bool) -> Self {
            Self::with_profile(semantic, ShellProfile::Unix)
        }

        fn with_profile(semantic: bool, profile: ShellProfile) -> Self {
            Self {
                theme: build_terminal_theme(&gpui_component::Theme::default()),
                fonts: FontSet::new(&font(), px(13.0)),
                semantic: semantic.then(|| SemanticOverlay::new(profile, true)),
                font_weight: FontWeight::NORMAL.0,
                reverse_video: false,
            }
        }

        fn reversed(mut self) -> Self {
            self.reverse_video = true;
            self
        }

        fn with_weight(mut self, font_weight: f32) -> Self {
            self.font_weight = font_weight;
            self
        }

        /// The whole frame's semantic classes, logical line by logical line —
        /// what the plan cache computes before it rebuilds a row.
        fn classes(&self, frame: &Frame) -> Vec<Vec<u8>> {
            self.scan(frame).0
        }

        /// The whole frame'''s semantic classes and per-row roles, logical line
        /// by logical line — what the plan cache computes before it rebuilds a
        /// row.
        fn scan(&self, frame: &Frame) -> (Vec<Vec<u8>>, Vec<Option<RowRole>>) {
            let rows = usize::from(frame.size().rows);
            let mut classes = vec![Vec::new(); rows];
            let mut roles = vec![None; rows];
            let mut heads = vec![Semantic::None; rows];
            let Some(overlay) = self.semantic.as_ref() else {
                return (classes, roles);
            };
            let mut wraps = Vec::new();
            fill_wraps(frame, &mut wraps);
            class_rows_into(
                frame,
                overlay,
                &mut classes,
                &mut roles,
                &mut heads,
                &wraps,
                0..rows,
                &mut Scratch::new(),
                &mut FrameStats::default(),
            );
            (classes, roles)
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
                    font_weight: self.font_weight,
                    cell_width: px(8.0),
                    device: CellSizeDevicePx { w: 8, h: 16 },
                    semantic: self.semantic.as_ref(),
                    reverse_video: self.reverse_video,
                    window,
                };
                let mut scratch = Scratch::new();
                let mut glyphs = GlyphCache::new();
                let mut stats = FrameStats::default();
                build_row_plan(
                    frame.row(row),
                    &ctx,
                    &self.classes(frame)[row],
                    mask,
                    self.scan(frame).1[row],
                    None,
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
    fn decscnm_swaps_the_two_defaults_and_nothing_else(cx: &mut TestAppContext) {
        // `? 5` is a screen-level flag: the engine changes no cell for it, so
        // the renderer is what has to honour it, and it does so by swapping
        // what the two **defaults** resolve to.
        let frame = FrameBuilder::new(1, 8)
            .text(0, 0, "ab")
            .styled(0, 2, 'c', Color::Ansi(1), Color::Ansi(4), CellFlags::NONE)
            .build();
        let mask = vec![false; 8];
        let fx = Fixture::new(false);
        let plain = fx.plan(cx, &frame, 0, &mask);
        let reversed = Fixture::new(false).reversed().plan(cx, &frame, 0, &mask);

        let fg = fx.theme.color(Color::Foreground);
        let bg = fx.theme.color(Color::Background);

        // Default-coloured text comes out background-on-foreground, and the
        // default background now has to be painted, where before it was the
        // one colour the renderer could skip.
        let at = |plan: &RowPlan, col: u16| {
            plan.bg
                .iter()
                .find(|span| span.col <= col && col < span.col + span.cols)
                .map(|span| span.color)
        };
        assert_eq!(at(&plain, 0), None, "a default background paints nothing");
        assert_eq!(at(&reversed, 0), Some(fg), "and now it takes the old fg");
        // Both go through the same contrast enforcement they always did, with
        // the pair the other way round.
        assert_eq!(
            reversed.colors_of(&reversed.text[0])[0].color,
            fx.theme.ensure_contrast(bg, fg)
        );
        assert_eq!(
            plain.colors_of(&plain.text[0])[0].color,
            fx.theme.ensure_contrast(fg, bg)
        );

        // A cell that named its own colours is untouched: `? 5` swaps the
        // defaults, not every colour on the screen.
        assert_eq!(at(&plain, 2), at(&reversed, 2));
        assert_eq!(at(&plain, 2), Some(fx.theme.color(Color::Ansi(4))));
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
        assert_eq!(
            plan.cells_of(&plan.text[0]),
            &[0, 1, 4],
            "the mark shares its base cell"
        );
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

    /// Rects of the plan's shapes with the column offset removed, sorted.
    fn cell_rects(plan: &RowPlan, col: u16) -> Vec<DeviceRect> {
        let mut rects: Vec<DeviceRect> = plan
            .shapes
            .iter()
            .map(|q| DeviceRect {
                x: q.rect.x - i32::from(col) * 8,
                ..q.rect
            })
            .collect();
        rects.sort_by_key(|r| (r.x, r.y, r.w, r.h));
        rects
    }

    fn sorted_quads(c: char, font_weight: f32) -> Vec<DeviceRect> {
        let mut rects = Vec::new();
        shape_quads(c, CellSizeDevicePx { w: 8, h: 16 }, font_weight, &mut rects);
        rects.sort_by_key(|r| (r.x, r.y, r.w, r.h));
        rects
    }

    #[gpui::test]
    fn bold_cell_uses_heavier_strokes(cx: &mut TestAppContext) {
        let fx = Fixture::new(false);
        let corner = "\u{250C}";
        let plain = fx.plan(
            cx,
            &FrameBuilder::new(1, 3).text(0, 1, corner).build(),
            0,
            &[],
        );
        let bold_frame = FrameBuilder::new(1, 3)
            .text(0, 1, corner)
            .flags(0, 1, CellFlags::BOLD)
            .build();
        let bold = fx.plan(cx, &bold_frame, 0, &[]);
        let (plain, bold) = (cell_rects(&plain, 1), cell_rects(&bold, 1));
        assert_ne!(plain, bold, "bold does not change the corner");
        assert_eq!(
            bold,
            sorted_quads('\u{250C}', FontWeight::NORMAL.0 + BOLD_WEIGHT_STEP),
            "bold cell is not drawn at base + 300"
        );
        let thickness = |rects: &[DeviceRect]| rects.iter().map(|r| r.w.min(r.h)).max();
        assert!(
            thickness(&bold) > thickness(&plain),
            "bold strokes are not thicker: {bold:?} vs {plain:?}"
        );

        // A bold cell next to a plain one keeps its own quads: the
        // coalescing comparison is by rect.
        let mixed = FrameBuilder::new(1, 4)
            .text(0, 0, "\u{2500}\u{2500}")
            .flags(0, 1, CellFlags::BOLD)
            .build();
        let plan = fx.plan(cx, &mixed, 0, &[]);
        assert_eq!(plan.shapes.len(), 2, "{:?}", plan.shapes);
        assert!(plan.shapes[1].rect.h > plan.shapes[0].rect.h);
    }

    #[gpui::test]
    fn settings_weight_scales_strokes(cx: &mut TestAppContext) {
        let frame = FrameBuilder::new(1, 3)
            .text(0, 0, "\u{2500}\u{2500}\u{2500}")
            .build();
        let normal = Fixture::new(false).plan(cx, &frame, 0, &[]);
        let heavy = Fixture::new(false)
            .with_weight(FontWeight::BOLD.0)
            .plan(cx, &frame, 0, &[]);
        assert_eq!(normal.shapes.len(), 1);
        assert_eq!(heavy.shapes.len(), 1, "same-weight cells still coalesce");
        assert!(
            heavy.shapes[0].rect.h > normal.shapes[0].rect.h,
            "{:?} vs {:?}",
            heavy.shapes[0],
            normal.shapes[0]
        );
        assert_eq!(heavy.shapes[0].rect.w, 3 * 8);

        // Bold on top of a heavy setting caps at 900.
        let bold_on_heavy = FrameBuilder::new(1, 1)
            .text(0, 0, "\u{2500}")
            .flags(0, 0, CellFlags::BOLD)
            .build();
        let capped = Fixture::new(false)
            .with_weight(800.0)
            .plan(cx, &bold_on_heavy, 0, &[]);
        assert_eq!(
            cell_rects(&capped, 0),
            sorted_quads('\u{2500}', MAX_FONT_WEIGHT)
        );
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

    // ── BUG-0071: a logical line is classified as one line ──────────────────

    /// A quoted string, a keyword and a URL that straddle the wrap boundaries
    /// of a 20-column grid. Laid out so the string crosses row 1 -> 2, the
    /// keyword crosses row 2 -> 3 and the URL crosses row 3 -> 4.
    const WRAPPED_LINE: &str =
        "user@host:~$ echo \"hello error world\" WARNING https://x.test/a /etc/hosts";

    /// Build `text` into a `cols`-wide grid, wrapping every full row.
    fn wrapped_frame(text: &str, cols: usize) -> Frame {
        let chars: Vec<char> = text.chars().collect();
        let rows = chars.len().div_ceil(cols);
        let mut builder = FrameBuilder::new(rows, cols);
        for (r, chunk) in chars.chunks(cols).enumerate() {
            builder = builder.text(r, 0, &chunk.iter().collect::<String>());
            if chunk.len() == cols && r + 1 < rows {
                builder = builder.flags(r, cols - 1, CellFlags::WRAPLINE);
            }
        }
        builder.build()
    }

    /// Every row's classes, in display order, as one flat vector.
    fn flat_classes(fx: &Fixture, frame: &Frame) -> Vec<u8> {
        fx.classes(frame).concat()
    }

    /// The headline of `BUG-0071`: the same text classifies identically whether
    /// it fits one row or wraps over four, because the scan runs over the
    /// logical line, not the visual row.
    #[test]
    fn wrapped_line_classifies_like_the_same_text_unwrapped() {
        let fx = Fixture::new(true);
        let narrow = wrapped_frame(WRAPPED_LINE, 20);
        let wide = wrapped_frame(WRAPPED_LINE, 80);
        assert_eq!(usize::from(narrow.size().rows), 4, "the text must wrap");
        assert_eq!(usize::from(wide.size().rows), 1, "the text must fit");
        assert_eq!(flat_classes(&fx, &narrow), flat_classes(&fx, &wide));
    }

    /// The prompt sign and the command word keep their classes wherever the
    /// wrap drops them — the case the owner reported as a long cwd. Before the
    /// fix a prompt whose sign fell on a later row was not recognised as a
    /// prompt at all, so nothing after it was a command.
    ///
    /// The Windows fixtures are the ones that matter here: a Unix prompt's tail
    /// row re-matches the Unix pattern on its own, so a Unix-only fixture
    /// passes even with the scan put back per visual row (`BUG-0071` F4). Each
    /// of these lines is built so that **no** visual row is a prompt by itself
    /// — the drive letter and the `>` land on different rows.
    #[test]
    fn a_prompt_that_wraps_keeps_its_sign_and_command() {
        for (profile, long) in [
            (
                ShellProfile::Unix,
                "user@host:/srv/customer/acme/backend/services/gateway$ echo hello",
            ),
            (
                ShellProfile::Cmd,
                r"C:\Users\John Doe\customer\acme\backend\gateway>echo hello",
            ),
            (
                ShellProfile::PowerShell,
                r"PS C:\Users\John Doe\customer\acme\gateway> echo hello",
            ),
        ] {
            let fx = Fixture::with_profile(true, profile);
            let classes = flat_classes(&fx, &wrapped_frame(long, 20));
            let sign = long.find(['$', '>']).expect("the prompt sign");
            assert!(
                sign >= 20,
                "{long:?}: the sign must land past the first row"
            );
            assert_eq!(
                classes[sign],
                Class::PromptSign as u8,
                "{long:?}: the sign is on row {}",
                sign / 20
            );
            let echo = long.find("echo").expect("the command");
            assert!(
                classes[echo..echo + 4]
                    .iter()
                    .all(|&c| c == Class::Command as u8),
                "{long:?}: the command after a wrapped prompt: {:?}",
                &classes[echo..echo + 4]
            );
        }
    }

    /// A wrapped Windows cwd is one `Path` run, spaces and all: the visible
    /// half of the owner's report (`BUG-0071` F2).
    #[test]
    fn a_wrapped_windows_cwd_is_one_path_run() {
        let long = r"C:\Users\John Doe\customer\acme workspace\gateway>dir";
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let classes = flat_classes(&fx, &wrapped_frame(long, 20));
        let sign = long.find('>').expect("the prompt sign");
        assert!(
            classes[..sign].iter().all(|&c| c == Class::Path as u8),
            "the cwd is not one run: {:?}",
            &classes[..sign]
        );
    }

    /// `BUG-0073`: drive-anchored output that reaches an unspaced `>` is
    /// output, and the rule is applied to the **logical** line. Wrapped at 8
    /// the second row is `> C:\dst`, which on its own is `cmd`'s continuation
    /// prompt — joining the run is what keeps the whole line output.
    #[test]
    fn a_wrapped_arrow_line_is_not_a_prompt() {
        let line = r"C:\src -> C:\dst";
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = wrapped_frame(line, 8);
        assert_eq!(usize::from(frame.size().rows), 2, "the text must wrap");
        let classes = flat_classes(&fx, &frame);
        assert!(
            !classes.contains(&(Class::PromptSign as u8)),
            "the arrow line is output, on every row: {classes:?}"
        );
    }

    /// A wrapped line carrying CJK: the classes must land on the same
    /// characters they land on unwrapped (`BUG-0071` F3).
    #[test]
    fn a_wrapped_run_with_cjk_keeps_its_classes_on_the_right_chars() {
        let fx = Fixture::new(true);
        let line = "\u{65e5}\u{672c}\u{8a9e} error here and a path /etc/hosts at the end";
        let narrow = flat_classes(&fx, &wrapped_frame(line, 20));
        let wide = flat_classes(&fx, &wrapped_frame(line, 120));
        let shared = narrow.len().min(wide.len());
        assert_eq!(narrow[..shared], wide[..shared], "{narrow:?}");
        let error = line.chars().position(|c| c == 'e').expect("the keyword");
        assert_eq!(narrow[error], Class::Error as u8, "{narrow:?}");
    }

    /// A quoted string opened on one row still ends on the row its closing
    /// quote is on, as one unbroken run.
    #[test]
    fn a_string_that_straddles_a_wrap_is_one_run() {
        let fx = Fixture::new(true);
        let line = "value = \"abcdefghijklmno pqr\" end";
        let classes = flat_classes(&fx, &wrapped_frame(line, 20));
        let open = line.find('"').expect("the opening quote");
        let close = line.rfind('"').expect("the closing quote");
        assert!(
            open < 20 && close >= 20,
            "the string must straddle the wrap"
        );
        assert!(
            classes[open..=close]
                .iter()
                .all(|&c| c == Class::String as u8),
            "the string is split at the wrap: {:?}",
            &classes[open..=close]
        );
    }

    /// A row without the wrap flag ends the logical line: the next row is
    /// scanned on its own, exactly as a hard newline should behave.
    #[test]
    fn a_hard_newline_is_not_joined() {
        let fx = Fixture::new(true);
        let chars: Vec<char> = WRAPPED_LINE.chars().collect();
        let rows: Vec<String> = chars.chunks(20).map(|c| c.iter().collect()).collect();
        let mut hard = FrameBuilder::new(rows.len(), 20);
        for (r, text) in rows.iter().enumerate() {
            hard = hard.text(r, 0, text);
        }
        let hard = hard.build();
        let joined = wrapped_frame(WRAPPED_LINE, 20);
        assert_ne!(
            flat_classes(&fx, &hard),
            flat_classes(&fx, &joined),
            "rows without the wrap flag must not be joined"
        );
        // Each row of the unwrapped grid classifies as its own single line.
        for (r, text) in rows.iter().enumerate() {
            let alone = FrameBuilder::new(1, 20).text(0, 0, text).build();
            assert_eq!(
                fx.classes(&hard)[r],
                fx.classes(&alone)[0],
                "row {r} must be scanned alone"
            );
        }
    }

    /// Editing one row of a wrapped line re-classifies the whole line: the
    /// classes of row 1 depend on text that lives on row 0.
    #[test]
    fn editing_one_row_reclassifies_the_whole_logical_line() {
        let fx = Fixture::new(true);
        let opened = wrapped_frame("echo \"aaaaaaaaaaaaaaabbbbbbbbbbbbbbb\" x", 20);
        // Same rows, but row 0 no longer opens the quote.
        let closed = wrapped_frame("echo  aaaaaaaaaaaaaaabbbbbbbbbbbbbbb\" x", 20);
        assert_ne!(
            fx.classes(&opened)[1],
            fx.classes(&closed)[1],
            "row 1 must follow the quote opened on row 0"
        );
    }

    // ── Roles from the OSC 133 marks (`US-0133`) ───────────────────────────

    /// The rows above a marked prompt matter: a prompt line is one the OSC 133
    /// region **transitions into**, so every fixture that expects a marked
    /// prompt gives it a predecessor. Row 0 of a viewport has none — its
    /// predecessor scrolled off the top — and is therefore never a prompt.
    const PREV_OUTPUT: &str = "done";

    /// A prompt whose cwd wraps, with the marks a shell emits: the whole prompt
    /// region is `Prompt`, the typed command after `OSC 133;B` is `Input`.
    ///
    /// Row 0 is the previous command's output, so the prompt below it is a
    /// transition. The prompt itself is 37 chars, so at 20 columns it runs onto
    /// row 2 and the input straddles the wrap.
    fn marked_wrapped_prompt() -> Frame {
        const PROMPT: &str = r"C:\Users\John Doe\ws\src> cargo build";
        let chars: Vec<char> = PROMPT.chars().collect();
        let boundary = PROMPT.find("cargo").unwrap();
        let mut b = FrameBuilder::new(4, 20).text(0, 0, PREV_OUTPUT).mark(
            0,
            0..PREV_OUTPUT.len(),
            Semantic::Output,
        );
        for (chunk_index, chunk) in chars.chunks(20).enumerate() {
            let r = chunk_index + 1;
            let text: String = chunk.iter().collect();
            b = b.text(r, 0, &text);
            let start = chunk_index * 20;
            let end = start + chunk.len();
            let split = boundary.clamp(start, end) - start;
            b = b.mark(r, 0..split, Semantic::Prompt);
            b = b.mark(r, split..chunk.len(), Semantic::Input);
            if end < chars.len() {
                b = b.flags(r, 19, CellFlags::WRAPLINE);
            }
        }
        b.build()
    }

    /// The marks put every row of the run in `Prompt`, and the boundary — not a
    /// glyph hunt — finds the sign, on the row the cwd happened to end on.
    #[test]
    fn a_wrapped_prompt_takes_its_role_from_the_marks() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = marked_wrapped_prompt();
        let (classes, roles) = fx.scan(&frame);
        assert_eq!(roles[0], Some(RowRole::Output), "the predecessor");
        assert_eq!(roles[1], Some(RowRole::Prompt));
        assert_eq!(roles[2], Some(RowRole::Prompt), "the continuation row too");
        assert_eq!(roles[3], None, "the blank row below carries no mark");
        // `>` is char 24 of the logical line, i.e. row 2 column 4.
        assert_eq!(classes[2][4], Class::PromptSign as u8, "{:?}", classes[2]);
        // `cargo` starts at char 26 = row 2 column 6.
        assert_eq!(classes[2][6], Class::Command as u8, "{:?}", classes[2]);
        assert_eq!(classes[1][0], Class::Path as u8, "{:?}", classes[1]);
    }

    /// The same prompt on one row gets the same answer as the wrapped one.
    #[test]
    fn a_marked_prompt_is_classified_the_same_wrapped_or_not() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        const PROMPT: &str = r"C:\Users\John Doe\ws\src> cargo build";
        let boundary = PROMPT.find("cargo").unwrap();
        let flat = FrameBuilder::new(2, 40)
            .text(0, 0, PREV_OUTPUT)
            .mark(0, 0..PREV_OUTPUT.len(), Semantic::Output)
            .text(1, 0, PROMPT)
            .mark(1, 0..boundary, Semantic::Prompt)
            .mark(1, boundary..PROMPT.len(), Semantic::Input)
            .build();
        let wrapped: Vec<u8> = fx
            .scan(&marked_wrapped_prompt())
            .0
            .into_iter()
            .skip(1)
            .flatten()
            .take(PROMPT.len())
            .collect();
        assert_eq!(&fx.scan(&flat).0[1][..PROMPT.len()], &wrapped[..]);
    }

    /// A continuation row whose own cells carry no mark does not drag the run
    /// away from the role its head declared.
    #[test]
    fn an_unmarked_continuation_row_keeps_the_runs_role() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(3, 20)
            .text(0, 0, PREV_OUTPUT)
            .mark(0, 0..PREV_OUTPUT.len(), Semantic::Output)
            .text(1, 0, r"C:\Users\John Doe\ws")
            .mark(1, 0..20, Semantic::Prompt)
            .flags(1, 19, CellFlags::WRAPLINE)
            .text(2, 0, r"\src> dir")
            .build();
        let (classes, roles) = fx.scan(&frame);
        assert_eq!(roles[1], Some(RowRole::Prompt));
        assert_eq!(roles[2], Some(RowRole::Prompt), "the run's head decides");
        assert_eq!(classes[2][4], Class::PromptSign as u8, "{:?}", classes[2]);
    }

    /// A session with no marks at all is classified exactly as before: the
    /// prompt regex still finds the prompt.
    #[test]
    fn a_session_with_no_marks_falls_back_to_the_regex() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(1, 20).text(0, 0, r"C:\ws> dir").build();
        let (classes, roles) = fx.scan(&frame);
        assert_eq!(roles[0], None);
        assert_eq!(classes[0][5], Class::PromptSign as u8, "{:?}", classes[0]);
    }

    /// Marks that appear mid-session: the unmarked rows keep the regex and the
    /// marked ones do not run it. "No mark" is per row, not per session — and an
    /// unmarked predecessor is a perfectly good transition, which is what makes
    /// the first prompt of a session a prompt.
    #[test]
    fn marks_appearing_mid_session_are_decided_per_row() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(2, 20)
            .text(0, 0, r"C:\ws> dir")
            .text(1, 0, r"C:\ws> dir")
            .mark(1, 0..6, Semantic::Prompt)
            .mark(1, 6..10, Semantic::Input)
            .build();
        let (classes, roles) = fx.scan(&frame);
        assert_eq!(roles[0], None);
        assert_eq!(roles[1], Some(RowRole::Prompt));
        assert_eq!(classes[0], classes[1], "both find the same prompt sign");
    }

    /// `cmd.exe`'s built-in `PROMPT` emits `OSC 133;A` and `;B` and never `;C`,
    /// so every output line after it is still tagged `Input`. A line that starts
    /// in `Input` with no prompt on it is therefore reported unmarked, and the
    /// output is classified as output rather than as a command line — while the
    /// prompt itself, which follows an `Input`-tagged line, is still a prompt.
    #[test]
    fn output_left_tagged_input_by_a_shell_without_osc_133_c_is_unmarked() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(3, 20)
            .text(0, 0, "a.txt")
            .mark(0, 0..5, Semantic::Input)
            .text(1, 0, r"C:\ws> dir")
            .mark(1, 0..6, Semantic::Prompt)
            .mark(1, 6..10, Semantic::Input)
            .text(2, 0, "error: no files")
            .mark(2, 0..15, Semantic::Input)
            .build();
        let (classes, roles) = fx.scan(&frame);
        assert_eq!(roles[0], None, "an Input-headed line is not trusted");
        assert_eq!(roles[1], Some(RowRole::Prompt), "cmd's prompt still works");
        assert_eq!(roles[2], None, "an Input-headed line is not trusted");
        assert_eq!(classes[1][5], Class::PromptSign as u8, "{:?}", classes[1]);
        assert_eq!(
            classes[2][0],
            Class::Error as u8,
            "output matchers, not command mode: {:?}",
            classes[2]
        );
    }

    /// **The A-only flood (`MAJ-1`).** OneTerm's own bash `PROMPT_COMMAND` and
    /// its SSH bootstrap emit `OSC 133;A` and nothing else. The region is a
    /// sticky cell attribute, so every line printed afterwards carries
    /// `Semantic::Prompt`. Only a line the region *transitions into* is a prompt,
    /// so the flood is reported unmarked and the prompt regex decides — which is
    /// what it did before the fast path existed.
    #[test]
    fn an_a_only_shell_does_not_turn_its_output_into_prompts() {
        let fx = Fixture::new(true);
        let lines = [
            "user@host:~$ ls",
            "ERROR: build failed, 100% of targets stale",
            "cc -o a.out main.c   # 2 warnings",
            "done in 12s",
        ];
        let mut b = FrameBuilder::new(1 + lines.len(), 48).text(0, 0, "starting");
        for (i, line) in lines.iter().enumerate() {
            b = b
                .text(i + 1, 0, line)
                .mark(i + 1, 0..line.len(), Semantic::Prompt);
        }
        let frame = b.build();
        let (classes, roles) = fx.scan(&frame);
        // Row 1 is the real prompt and it *is* a transition (row 0 is unmarked).
        assert_eq!(roles[1], Some(RowRole::Prompt));
        for row in 2..=lines.len() {
            assert_eq!(roles[row], None, "row {row} is inside the flood");
        }
        // And the output keeps its output classes instead of inventing a sign.
        let error_row = &classes[2];
        assert_eq!(error_row[0], Class::Error as u8, "{error_row:?}");
        assert!(
            !classes[4].contains(&(Class::PromptSign as u8)),
            "the last non-space char must not become a prompt sign: {:?}",
            classes[4]
        );
    }

    /// A shell that emits the whole set gets the whole answer: the prompt line
    /// is a prompt with a command on it, and the output below it is output.
    #[test]
    fn a_full_a_b_c_d_shell_is_classified_exactly() {
        let fx = Fixture::new(true);
        let prompt = "user@host:~$ ls -la";
        let boundary = prompt.find("ls").unwrap();
        let frame = FrameBuilder::new(3, 32)
            .text(0, 0, "previous output")
            .mark(0, 0..15, Semantic::Output)
            .text(1, 0, prompt)
            .mark(1, 0..boundary, Semantic::Prompt)
            .mark(1, boundary..prompt.len(), Semantic::Input)
            .text(2, 0, r"C:\src -> C:\dst")
            .mark(2, 0..16, Semantic::Output)
            .build();
        let (classes, roles) = fx.scan(&frame);
        assert_eq!(roles[0], Some(RowRole::Output));
        assert_eq!(roles[1], Some(RowRole::Prompt));
        assert_eq!(roles[2], Some(RowRole::Output));
        assert_eq!(classes[1][11], Class::PromptSign as u8, "{:?}", classes[1]);
        assert_eq!(
            classes[1][boundary],
            Class::Command as u8,
            "{:?}",
            classes[1]
        );
        assert_eq!(
            classes[1][boundary + 3],
            Class::Option as u8,
            "{:?}",
            classes[1]
        );
        assert!(
            !classes[2].contains(&(Class::PromptSign as u8)),
            "marked output never runs the prompt regex: {:?}",
            classes[2]
        );
    }

    /// The viewport's top line has no predecessor on screen, so its region is
    /// unknown and it is never a prompt — the one cost of the transition rule,
    /// and a deterministic one.
    #[test]
    fn the_viewports_first_line_is_never_a_prompt() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(2, 20)
            .text(0, 0, r"C:\ws> dir")
            .mark(0, 0..6, Semantic::Prompt)
            .mark(0, 6..10, Semantic::Input)
            .text(1, 0, "a.txt")
            .mark(1, 0..5, Semantic::Input)
            .build();
        let (classes, roles) = fx.scan(&frame);
        assert_eq!(roles[0], None);
        // ...and the regex fallback still colours it exactly as it always did.
        assert_eq!(classes[0][5], Class::PromptSign as u8, "{:?}", classes[0]);
    }

    /// A marked output row never reaches the prompt regex, so a line the regex
    /// reads as a prompt keeps its output classes.
    #[test]
    fn a_marked_output_row_is_not_read_as_a_prompt() {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        // A line the regex fallback still reads as a prompt — `BUG-0073`
        // tightened the sign's rule, and `C:\src -> C:\dst` no longer reaches
        // it, but a real redirection does.
        let line = r"C:\work>dir > out.txt";
        let marked = FrameBuilder::new(1, 24)
            .text(0, 0, line)
            .mark(0, 0..line.len(), Semantic::Output)
            .build();
        let plain = FrameBuilder::new(1, 24).text(0, 0, line).build();
        let (marked, roles) = fx.scan(&marked);
        assert_eq!(roles[0], Some(RowRole::Output));
        assert!(
            !marked[0].contains(&(Class::PromptSign as u8)),
            "{:?}",
            marked[0]
        );
        assert_ne!(
            marked[0],
            fx.scan(&plain).0[0],
            "the mark changes the answer"
        );
    }

    // ── The prompt-line background (`US-0134`) ─────────────────────────────

    /// The band of a row's plan, when it has one: the first `bg` span, which is
    /// the one pushed before the per-cell loop.
    fn band_of(plan: &RowPlan, cols: u16, band: Hsla) -> Option<&BgSpan> {
        plan.bg
            .first()
            .filter(|s| s.col == 0 && s.cols == cols && s.color == band)
    }

    /// One marked prompt at row 1, with the previous command's output above it
    /// so the region transitions.
    fn marked_prompt_frame(cols: usize, text: &str, boundary: usize) -> Frame {
        FrameBuilder::new(2, cols)
            .text(0, 0, PREV_OUTPUT)
            .mark(0, 0..PREV_OUTPUT.len(), Semantic::Output)
            .text(1, 0, text)
            .mark(1, 0..boundary, Semantic::Prompt)
            .mark(1, boundary..text.len(), Semantic::Input)
            .build()
    }

    #[gpui::test]
    fn a_marked_prompt_row_carries_a_full_width_band(cx: &mut TestAppContext) {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = marked_prompt_frame(20, r"C:\ws> dir", 6);
        let plan = fx.plan(cx, &frame, 1, &[]);
        let band = fx.theme.prompt_line_bg(false);
        assert!(
            band_of(&plan, 20, band).is_some(),
            "expected one 0..20 band first: {:?}",
            plan.bg
        );
    }

    /// Every row of a wrapped prompt gets it, including the row that holds only
    /// the tail of the cwd and the typed command.
    #[gpui::test]
    fn every_row_of_a_wrapped_prompt_carries_the_band(cx: &mut TestAppContext) {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = marked_wrapped_prompt();
        let band = fx.theme.prompt_line_bg(false);
        for row in 1..3 {
            let plan = fx.plan(cx, &frame, row, &[]);
            assert!(
                band_of(&plan, 20, band).is_some(),
                "row {row} has no band: {:?}",
                plan.bg
            );
        }
        // Neither the output line above nor the blank row below is part of it.
        for row in [0, 3] {
            let plan = fx.plan(cx, &frame, row, &[]);
            assert!(plan.bg.is_empty(), "row {row}: {:?}", plan.bg);
        }
    }

    /// An A-only session gets no band anywhere: nothing is a marked prompt.
    #[gpui::test]
    fn an_a_only_shell_paints_no_band(cx: &mut TestAppContext) {
        let fx = Fixture::new(true);
        let frame = FrameBuilder::new(3, 32)
            .text(0, 0, "user@host:~$ ls")
            .mark(0, 0..15, Semantic::Prompt)
            .text(1, 0, "ERROR: 100% nope")
            .mark(1, 0..16, Semantic::Prompt)
            .text(2, 0, "done in 12s")
            .mark(2, 0..11, Semantic::Prompt)
            .build();
        for row in 0..3 {
            let plan = fx.plan(cx, &frame, row, &[]);
            assert!(plan.bg.is_empty(), "row {row} was banded: {:?}", plan.bg);
        }
    }

    /// The band goes down first, so a selection or an ANSI background paints on
    /// top of it and looks exactly as it does on a non-prompt row.
    #[gpui::test]
    fn the_band_is_painted_under_the_per_cell_backgrounds(cx: &mut TestAppContext) {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(2, 20)
            .text(0, 0, PREV_OUTPUT)
            .mark(0, 0..PREV_OUTPUT.len(), Semantic::Output)
            .text(1, 0, r"C:\ws> dir")
            .mark(1, 0..6, Semantic::Prompt)
            .mark(1, 6..10, Semantic::Input)
            .styled(
                1,
                7,
                'i',
                Color::Foreground,
                Color::Ansi(1),
                CellFlags::NONE,
            )
            .build();
        let plan = fx.plan(cx, &frame, 1, &[]);
        let band = fx.theme.prompt_line_bg(false);
        assert_eq!(plan.bg[0].color, band, "the band is first: {:?}", plan.bg);
        let ansi = plan.bg[1];
        assert_eq!((ansi.col, ansi.cols), (7, 1));
        assert_eq!(ansi.color, fx.theme.color(Color::Ansi(1)));
    }

    /// A `LEADING_WIDE_CHAR_SPACER` carries no class and paints no background of
    /// its own, so only a full-width band covers it (§13 Q4).
    #[gpui::test]
    fn the_band_covers_a_leading_wide_char_spacer(cx: &mut TestAppContext) {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(2, 8)
            .text(0, 0, "done")
            .mark(0, 0..4, Semantic::Output)
            .text(1, 0, r"C:\ws> ")
            .mark(1, 0..8, Semantic::Prompt)
            .flags(1, 7, CellFlags::LEADING_WIDE_CHAR_SPACER)
            .build();
        assert!(frame.row(1).cell(7).is_spacer());
        let plan = fx.plan(cx, &frame, 1, &[]);
        let band = *band_of(&plan, 8, fx.theme.prompt_line_bg(false))
            .unwrap_or_else(|| panic!("no band: {:?}", plan.bg));
        assert!(
            (band.col..band.col + band.cols).contains(&7),
            "the spacer column is inside the band"
        );
    }

    /// Under the regex fallback there is no band at all, even on a row the
    /// fallback does read as a prompt.
    #[gpui::test]
    fn an_unmarked_prompt_row_gets_no_band(cx: &mut TestAppContext) {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(1, 20).text(0, 0, r"C:\ws> dir").build();
        assert_eq!(fx.scan(&frame).0[0][5], Class::PromptSign as u8);
        let plan = fx.plan(cx, &frame, 0, &[]);
        assert!(plan.bg.is_empty(), "no band without a mark: {:?}", plan.bg);
    }

    /// The contrast pass measures against the band, not against the theme
    /// background: every glyph the plan paints on a prompt row clears 4.5:1
    /// against what is actually behind it. (The per-theme sweep of the same
    /// floor lives in `crate::theme::tests`.)
    #[gpui::test]
    fn every_glyph_on_a_prompt_row_clears_the_band(cx: &mut TestAppContext) {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = marked_prompt_frame(24, r"C:\ws> dir /b", 6);
        let plan = fx.plan(cx, &frame, 1, &[]);
        let band = fx.theme.prompt_line_bg(false);
        assert_eq!(plan.bg[0].color, band);
        assert!(!plan.colors.is_empty(), "the row has shaped text");
        for span in &plan.colors {
            let ratio = crate::theme::contrast_ratio(span.color, band);
            assert!(ratio >= 4.5, "{:?} on the band is {ratio:.2}:1", span.color);
        }
    }

    /// A marked **output** row gets no band either.
    #[gpui::test]
    fn a_marked_output_row_gets_no_band(cx: &mut TestAppContext) {
        let fx = Fixture::with_profile(true, ShellProfile::Cmd);
        let frame = FrameBuilder::new(1, 20)
            .text(0, 0, "a.txt")
            .mark(0, 0..5, Semantic::Output)
            .build();
        let plan = fx.plan(cx, &frame, 0, &[]);
        assert!(plan.bg.is_empty(), "{:?}", plan.bg);
    }
}
