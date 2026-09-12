//! The compatibility surface: today's `TerminalContent` shape produced from the
//! engine's render state.
//!
//! Design:
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md`
//! § "`US-0081` — the shim, and why it is a real slice".
//!
//! **This file is the only place in the crate that converts to
//! `alacritty_terminal` values on the frame path, and it exists until
//! `US-0085`.** Everything above `crates/terminal`'s seam still sees the value
//! types it saw before the engine swap — `Cell`, `Flags`, `Point`, `Line`,
//! `Column`, `TermMode`, `SelectionRange`, `RenderableCursor` — because moving
//! the view onto `RenderRow` / `RenderCell` means editing `crates/terminal-view`,
//! which `US-0085` owns. When it does, this file is deleted whole and
//! `TerminalContent` keeps only its native accessors.
//!
//! Coordinates are the contract that must not move. `IndexedCell::point.line`
//! is the **grid** line the reference published: `0` is the top of the viewport
//! when `display_offset == 0`, negative values are scrollback, and every
//! consumer adds `display_offset` to get a display row
//! (`crates/terminal-view/src/render/frame.rs:514-560`). The engine has no
//! negative rows at all, so the conversion is one subtraction against the
//! viewport top, in one place.
//!
//! Two things make the conversion cheap enough to keep until `US-0085`:
//!
//! * the tri-state result decides how much of it runs at all — nothing on
//!   `Unchanged`, only the changed rows on a `Partial` that did not scroll;
//! * a `RenderRow` carries its style as **runs of resolved values** (R-14), so
//!   the colours and the attribute flags are converted once per run instead of
//!   once per cell. At 120 columns that is about three conversions per row
//!   rather than a hundred and twenty.

use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange as LegacySelectionRange;
use alacritty_terminal::term::cell::{Cell as LegacyCell, Flags, Hyperlink as LegacyHyperlink};
use alacritty_terminal::term::graphics::{
    GraphicCell as LegacyGraphicCell, GraphicData as LegacyGraphicData,
    GraphicId as LegacyGraphicId,
};
use alacritty_terminal::term::{RenderableCursor, TermMode};
use alacritty_terminal::vte::ansi::{
    Color as LegacyColor, CursorShape as LegacyCursorShape, NamedColor as LegacyNamedColor,
    Rgb as LegacyRgb,
};
use oneterm_vt::graphics::{GraphicData, Placement};
use oneterm_vt::grid::{Anchors, RowId, RowRef};
use oneterm_vt::intern::{ExtrasId, GraphicId, Interner};
use oneterm_vt::render::{ModeSnapshot, MouseEncoding, MouseReporting, RenderContent, RenderRow};
use oneterm_vt::{
    Attrs, Cell as VtCell, CellContent, CellWidth, Color as VtColor, CursorShape, NamedColor,
    RenderState, RenderUpdate, Rgb, SelectionRange, Style, Terminal,
};

use crate::content::{IndexedCell, TermDamageInfo, TerminalBounds, TerminalContent};

/// Rebuild `content` from the engine. Call with the adapter lock held.
pub(crate) fn refill(content: &mut TerminalContent, term: &mut Terminal, now: Instant) {
    let update = term.render_update(&mut content.state, now);
    content.update = update;

    let state = &content.state;
    let size = state.size();
    let (rows, cols) = (usize::from(size.rows), usize::from(size.cols));
    let display_offset = state.scroll_offset() as usize;
    // The reference's `Line(0)`: the viewport top at `display_offset == 0`.
    let screen_top = state.viewport_top() + u64::from(state.scroll_offset());

    refresh_cells(
        &mut content.cells,
        &mut content.built_offset,
        state,
        update,
        rows,
        cols,
        display_offset,
    );

    let cursor = state.cursor();
    let cursor_line = row_to_line(cursor.id, screen_top);
    content.cursor = RenderableCursor {
        shape: legacy_cursor_shape(term.cursor_style().shape),
        point: Point::new(Line(cursor_line), Column(usize::from(cursor.col))),
    };
    content.mode = legacy_mode(state.modes());
    content.display_offset = display_offset;
    content.selection = state
        .selection()
        .map(|range| legacy_selection(range, screen_top));
    content.terminal_bounds = TerminalBounds {
        num_lines: rows,
        num_cols: cols,
    };
    let screen = term.screen();
    content.total_lines = screen.history_len() as usize + usize::from(screen.rows());
    // The engine is the one drain (R-16): `render_update` never takes the
    // pixels, so the adapter does, right here, once per frame — which is the
    // hand-out-once contract `TerminalContent.graphics` already had.
    content.graphics.clear();
    content
        .graphics
        .extend(term.take_graphics().iter().map(legacy_graphic));

    let cursor_row = cursor
        .row
        .map(usize::from)
        .filter(|_| cursor.visible)
        .filter(|row| *row < rows);
    content.damage = damage(
        &content.state,
        update,
        content.last_cursor_row,
        cursor_row,
        std::mem::replace(&mut content.damage, TermDamageInfo::Full),
    );
    content.last_cursor_row = cursor_row;
}

/// Bring the dense legacy vector back in step with the render state, copying as
/// little as the update allows.
///
/// The incremental path needs the stored `point`s to still be right, and
/// `point.line` is `row_index - display_offset`: a scroll or a changed offset
/// invalidates every one of them, which is also exactly when the reference
/// reported `Full` damage. So the cheap path is "the viewport stood still and
/// only these rows changed", which is the interactive case — a keystroke, a
/// cursor move, a TUI repainting one line.
#[allow(clippy::too_many_arguments)]
fn refresh_cells(
    cells: &mut Vec<IndexedCell>,
    built_offset: &mut Option<usize>,
    state: &RenderState,
    update: RenderUpdate,
    rows: usize,
    cols: usize,
    display_offset: usize,
) {
    let dense = rows * cols;
    let laid_out = cells.len() == dense && *built_offset == Some(display_offset);
    if laid_out && matches!(update, RenderUpdate::Unchanged) {
        return;
    }

    let incremental = laid_out && matches!(update, RenderUpdate::Partial { scrolled: 0 });
    if !laid_out {
        cells.clear();
        cells.resize_with(dense, blank_indexed);
    }

    if incremental {
        for &index in state.changed() {
            let index = usize::from(index);
            if let Some(row) = state.rows().get(index) {
                write_row(cells, state, row, index, cols, display_offset);
            }
        }
    } else {
        for (index, row) in state.rows().iter().enumerate() {
            write_row(cells, state, row, index, cols, display_offset);
        }
    }
    *built_offset = Some(display_offset);
}

/// One display row of the dense vector, written in place.
///
/// Iterates the row's **style runs** and converts each one once; the inner loop
/// then only copies the resolved values and the per-cell content.
fn write_row(
    cells: &mut [IndexedCell],
    state: &RenderState,
    row: &RenderRow,
    index: usize,
    cols: usize,
    display_offset: usize,
) {
    let line = Line(index as i32 - display_offset as i32);
    let base = index * cols;
    let Some(out) = cells.get_mut(base..base + cols) else {
        return;
    };
    let last = row.cells.len().saturating_sub(1);
    for run in &row.runs {
        let (fg, bg, attrs) = legacy_style(&run.style);
        for col in run.cols.start..run.cols.end {
            let col = usize::from(col);
            let (Some(cell), Some(slot)) = (row.cells.get(col), out.get_mut(col)) else {
                continue;
            };
            let mut legacy = LegacyCell {
                fg,
                bg,
                flags: attrs | width_flag(cell.width),
                ..LegacyCell::default()
            };
            if row.wrapped && col == last {
                legacy.flags |= Flags::WRAPLINE;
            }
            match cell.content {
                RenderContent::Scalar(scalar) => legacy.c = scalar,
                RenderContent::Cluster { start, len } => {
                    write_cluster(&mut legacy, row.cluster(start, len));
                }
            }
            if let Some(link) = cell.hyperlink.and_then(|id| state.hyperlink(id)) {
                legacy.set_hyperlink(Some(LegacyHyperlink::new(
                    Some(link.id.as_ref()),
                    link.uri.to_string(),
                )));
            }
            // The cell carries only *which* image (R-21); the offset inside the
            // image's own cell grid — what the reference stored per cell — is
            // derived from the placement.
            if let Some(id) = cell.graphic
                && let Some((across, down)) = state.graphic_offset(id, row.id, col as u16)
            {
                legacy.set_graphic(Some(LegacyGraphicCell {
                    id: LegacyGraphicId(id.0),
                    col: across,
                    row: down,
                }));
            }
            slot.point = Point::new(line, Column(col));
            slot.cell = legacy;
        }
    }
}

fn blank_indexed() -> IndexedCell {
    IndexedCell {
        point: Point::default(),
        cell: LegacyCell::default(),
    }
}

/// Translate the engine's tri-state into the reference's damage shape.
///
/// The reference escalated to `Full` on any scroll and had no way to say "shift
/// your cache", so a `Partial { scrolled }` with a non-zero delta is reported as
/// `Full`. That is the behaviour the view has today; `US-0085` is where the
/// delta starts paying.
fn damage(
    state: &RenderState,
    update: RenderUpdate,
    last_cursor_row: Option<usize>,
    cursor_row: Option<usize>,
    previous: TermDamageInfo,
) -> TermDamageInfo {
    let mut rows = match previous {
        // Reuse the allocation, as `TerminalContent::refill` always has.
        TermDamageInfo::Partial(rows) => rows,
        TermDamageInfo::Full => Vec::new(),
    };
    rows.clear();
    match update {
        RenderUpdate::Full => return TermDamageInfo::Full,
        RenderUpdate::Partial { scrolled } if scrolled != 0 => return TermDamageInfo::Full,
        RenderUpdate::Partial { .. } | RenderUpdate::Unchanged => {}
    }
    rows.extend(state.changed().iter().map(|index| usize::from(*index)));
    for row in [last_cursor_row, cursor_row].into_iter().flatten() {
        if !rows.contains(&row) {
            rows.push(row);
        }
    }
    TermDamageInfo::Partial(rows)
}

/// The damage-free path: one grid row, read straight off the screen.
///
/// `query_line_range_cells` needs cells without touching a render watermark,
/// which is exactly what the reference's `renderable_content()` gave it. It
/// reads at most a handful of rows per call, so it stays per-cell rather than
/// per-run.
///
/// `placements` is `Terminal::placements()`: the same table `RenderState`
/// copies, so a cell read this way carries the same `GraphicCell` the render
/// path would give it.
pub(crate) fn push_grid_row(
    row: RowRef<'_>,
    interner: &Interner,
    placements: &Placements<'_>,
    line: Line,
    out: &mut Vec<IndexedCell>,
) {
    let cells = row.cells();
    let last = cells.len().saturating_sub(1);
    let wrapped = row.wrapped();
    for (col, cell) in cells.iter().enumerate() {
        out.push(IndexedCell {
            point: Point::new(line, Column(col)),
            cell: grid_cell(
                *cell,
                interner,
                placements,
                row.id(),
                col as u16,
                wrapped && col == last,
            ),
        });
    }
}

/// One grid cell as the legacy snapshot spells it.
fn grid_cell(
    cell: VtCell,
    interner: &Interner,
    placements: &Placements<'_>,
    row: RowId,
    col: u16,
    wrapline: bool,
) -> LegacyCell {
    let style = interner.resolve_style(cell.style_id());
    let (fg, bg, attrs) = legacy_style(style);
    let mut legacy = LegacyCell {
        fg,
        bg,
        flags: attrs | width_flag(cell.width()),
        ..LegacyCell::default()
    };
    if wrapline {
        legacy.flags |= Flags::WRAPLINE;
    }
    match cell.content() {
        CellContent::Scalar(scalar) => legacy.c = scalar,
        CellContent::Grapheme(id) => write_cluster(&mut legacy, interner.resolve_grapheme(id)),
    }
    if cell.extras_id() != ExtrasId::NONE {
        let extras = interner.resolve_extras(cell.extras_id());
        if let Some(link) = extras
            .hyperlink
            .and_then(|id| interner.hyperlinks.resolve(id))
        {
            legacy.set_hyperlink(Some(LegacyHyperlink::new(
                Some(link.id.as_ref()),
                link.uri.to_string(),
            )));
        }
        if let Some(id) = extras.graphic
            && let Some((across, down)) = placements.offset(id, row, col)
        {
            legacy.set_graphic(Some(LegacyGraphicCell {
                id: LegacyGraphicId(id.0),
                col: across,
                row: down,
            }));
        }
    }
    legacy
}

/// The live placements plus the anchor list they resolve their top-left cell
/// through, for the readers that do not go through a [`RenderState`].
pub(crate) struct Placements<'a> {
    placements: &'a [Placement],
    anchors: &'a Anchors,
}

impl<'a> Placements<'a> {
    pub(crate) fn new(term: &'a Terminal) -> Placements<'a> {
        Placements {
            placements: term.placements(),
            anchors: term.grid().anchors(),
        }
    }

    /// The cell's `(col, row)` offset inside the image's own cell grid — the
    /// same arithmetic `RenderState::graphic_offset` does, against the anchor
    /// rather than against a copied position.
    fn offset(&self, id: GraphicId, row: RowId, col: u16) -> Option<(u16, u16)> {
        let placement = self.placements.iter().find(|entry| entry.id == id)?;
        let top_left = self.anchors.get(placement.anchor)?;
        let down = u16::try_from(row.distance(top_left.row)).ok()?;
        let across = col.checked_sub(top_left.col)?;
        (row >= top_left.row && down < placement.rows && across < placement.cols)
            .then_some((across, down))
    }
}

/// The decoded image as `TerminalContent.graphics` still spells it.
///
/// One copy of the pixels per image, on the drain path only: `US-0085` moves
/// the view onto `oneterm_vt::GraphicData` and this disappears with it.
fn legacy_graphic(image: &Arc<GraphicData>) -> Arc<LegacyGraphicData> {
    Arc::new(LegacyGraphicData {
        id: LegacyGraphicId(image.id.0),
        width: image.width,
        height: image.height,
        rgba: image.rgba.clone(),
    })
}

/// One resolved style as the three legacy fields it becomes. Called once per
/// style run on the frame path.
fn legacy_style(style: &Style) -> (LegacyColor, LegacyColor, Flags) {
    (
        legacy_color(style.fg),
        legacy_color(style.bg),
        legacy_flags(style.attrs),
    )
}

/// A grapheme cluster is a base character plus zero-width followers, which is
/// exactly the reference's shape.
fn write_cluster(cell: &mut LegacyCell, cluster: &[char]) {
    let Some((base, rest)) = cluster.split_first() else {
        return;
    };
    cell.c = *base;
    for follower in rest {
        cell.push_zerowidth(*follower);
    }
}

fn width_flag(width: CellWidth) -> Flags {
    match width {
        CellWidth::Narrow => Flags::empty(),
        CellWidth::Wide => Flags::WIDE_CHAR,
        CellWidth::WideSpacer => Flags::WIDE_CHAR_SPACER,
        CellWidth::LeadingWideSpacer => Flags::LEADING_WIDE_CHAR_SPACER,
    }
}

/// Blink and overline are dropped because the legacy `Flags` has no bit for
/// either: `US-0076`'s correction C11 stores them in the engine, and the
/// snapshot cannot carry them until `US-0085` puts the view on `RenderRow`.
/// Structural, not a behaviour change — the renderer draws neither today.
fn legacy_flags(attrs: Attrs) -> Flags {
    let pairs = [
        (Attrs::BOLD, Flags::BOLD),
        (Attrs::DIM, Flags::DIM),
        (Attrs::ITALIC, Flags::ITALIC),
        (Attrs::INVERSE, Flags::INVERSE),
        (Attrs::HIDDEN, Flags::HIDDEN),
        (Attrs::STRIKEOUT, Flags::STRIKEOUT),
        (Attrs::UNDERLINE, Flags::UNDERLINE),
        (Attrs::DOUBLE_UNDERLINE, Flags::DOUBLE_UNDERLINE),
        (Attrs::UNDERCURL, Flags::UNDERCURL),
        (Attrs::DOTTED_UNDERLINE, Flags::DOTTED_UNDERLINE),
        (Attrs::DASHED_UNDERLINE, Flags::DASHED_UNDERLINE),
    ];
    let mut flags = Flags::empty();
    for (engine, legacy) in pairs {
        if attrs.contains(engine) {
            flags |= legacy;
        }
    }
    flags
}

/// The two enums are matched member by member: their discriminants differ after
/// `Cursor`, so nothing here may do arithmetic on them.
pub(crate) fn legacy_color(color: VtColor) -> LegacyColor {
    match color {
        VtColor::Rgb(rgb) => LegacyColor::Spec(legacy_rgb(rgb)),
        VtColor::Palette(index) => LegacyColor::Indexed(index),
        VtColor::Named(named) => LegacyColor::Named(legacy_named(named)),
    }
}

fn legacy_named(named: NamedColor) -> LegacyNamedColor {
    match named {
        NamedColor::Black => LegacyNamedColor::Black,
        NamedColor::Red => LegacyNamedColor::Red,
        NamedColor::Green => LegacyNamedColor::Green,
        NamedColor::Yellow => LegacyNamedColor::Yellow,
        NamedColor::Blue => LegacyNamedColor::Blue,
        NamedColor::Magenta => LegacyNamedColor::Magenta,
        NamedColor::Cyan => LegacyNamedColor::Cyan,
        NamedColor::White => LegacyNamedColor::White,
        NamedColor::BrightBlack => LegacyNamedColor::BrightBlack,
        NamedColor::BrightRed => LegacyNamedColor::BrightRed,
        NamedColor::BrightGreen => LegacyNamedColor::BrightGreen,
        NamedColor::BrightYellow => LegacyNamedColor::BrightYellow,
        NamedColor::BrightBlue => LegacyNamedColor::BrightBlue,
        NamedColor::BrightMagenta => LegacyNamedColor::BrightMagenta,
        NamedColor::BrightCyan => LegacyNamedColor::BrightCyan,
        NamedColor::BrightWhite => LegacyNamedColor::BrightWhite,
        NamedColor::Foreground => LegacyNamedColor::Foreground,
        NamedColor::Background => LegacyNamedColor::Background,
        NamedColor::Cursor => LegacyNamedColor::Cursor,
        NamedColor::BrightForeground => LegacyNamedColor::BrightForeground,
        NamedColor::DimForeground => LegacyNamedColor::DimForeground,
        NamedColor::DimBlack => LegacyNamedColor::DimBlack,
        NamedColor::DimRed => LegacyNamedColor::DimRed,
        NamedColor::DimGreen => LegacyNamedColor::DimGreen,
        NamedColor::DimYellow => LegacyNamedColor::DimYellow,
        NamedColor::DimBlue => LegacyNamedColor::DimBlue,
        NamedColor::DimMagenta => LegacyNamedColor::DimMagenta,
        NamedColor::DimCyan => LegacyNamedColor::DimCyan,
        NamedColor::DimWhite => LegacyNamedColor::DimWhite,
    }
}

pub(crate) fn legacy_rgb(rgb: Rgb) -> LegacyRgb {
    LegacyRgb {
        r: rgb.r,
        g: rgb.g,
        b: rgb.b,
    }
}

pub(crate) fn engine_rgb(rgb: LegacyRgb) -> Rgb {
    Rgb {
        r: rgb.r,
        g: rgb.g,
        b: rgb.b,
    }
}

pub(crate) fn legacy_cursor_shape(shape: CursorShape) -> LegacyCursorShape {
    match shape {
        CursorShape::Block => LegacyCursorShape::Block,
        CursorShape::Beam => LegacyCursorShape::Beam,
        CursorShape::Underline => LegacyCursorShape::Underline,
        CursorShape::HollowBlock => LegacyCursorShape::HollowBlock,
        CursorShape::Hidden => LegacyCursorShape::Hidden,
    }
}

/// The mode bits the workspace actually reads (`research/api-surface.md` § 3.5).
///
/// The reference's mouse modes are exclusive — `? 1002` clears `MOUSE_MODE`
/// before setting `MOUSE_DRAG` — and so is [`MouseReporting`], so the mapping is
/// one bit each rather than a union.
pub(crate) fn legacy_mode(modes: ModeSnapshot) -> TermMode {
    let mut mode = TermMode::empty();
    mode.set(TermMode::SHOW_CURSOR, modes.show_cursor);
    mode.set(TermMode::APP_CURSOR, modes.app_cursor);
    mode.set(TermMode::APP_KEYPAD, modes.app_keypad);
    mode.set(TermMode::BRACKETED_PASTE, modes.bracketed_paste);
    mode.set(TermMode::INSERT, modes.insert);
    mode.set(TermMode::ALT_SCREEN, modes.alt_screen);
    mode.set(TermMode::ALTERNATE_SCROLL, modes.alternate_scroll);
    if let Some(mouse) = modes.mouse {
        mode |= match mouse.reporting {
            MouseReporting::Normal => TermMode::MOUSE_REPORT_CLICK,
            MouseReporting::ButtonEvent => TermMode::MOUSE_DRAG,
            MouseReporting::AnyEvent => TermMode::MOUSE_MOTION,
        };
        match mouse.encoding {
            MouseEncoding::Default => {}
            MouseEncoding::Utf8 => mode |= TermMode::UTF8_MOUSE,
            MouseEncoding::Sgr => mode |= TermMode::SGR_MOUSE,
        }
    }
    mode
}

/// A `RowId` as the reference's signed grid line: `0` is the viewport top at
/// `display_offset == 0`, negative values are scrollback.
pub(crate) fn row_to_line(row: RowId, screen_top: RowId) -> i32 {
    (row.0 as i64 - screen_top.0 as i64).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn legacy_selection(range: SelectionRange, screen_top: RowId) -> LegacySelectionRange {
    LegacySelectionRange {
        start: Point::new(
            Line(row_to_line(range.start.row, screen_top)),
            Column(usize::from(range.start.col)),
        ),
        end: Point::new(
            Line(row_to_line(range.end.row, screen_top)),
            Column(usize::from(range.end.col)),
        ),
        is_block: range.is_block,
    }
}
