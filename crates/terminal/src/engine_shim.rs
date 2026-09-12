//! `LegacySnapshot` — today's `TerminalContent` produced from the new engine.
//!
//! Design:
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md`
//! § "`US-0081` — the shim, and why it is a real slice".
//!
//! This file exists for four packets. It is the whole reason `US-0081` can be
//! accepted as "workspace green, zero behaviour diff, no consumer changed": the
//! engine underneath is `oneterm-vt`, and everything above
//! `crates/terminal`'s seam still sees the `alacritty_terminal` value types it
//! saw before — `Cell`, `Flags`, `Point`, `Line`, `Column`, `TermMode`,
//! `SelectionRange`, `RenderableCursor`. `US-0085` moves the view onto
//! `RenderRow` / `RenderCell` and deletes this file.
//!
//! Coordinates are the contract that must not move. `IndexedCell::point.line`
//! is the **grid** line the reference publishes: `0` is the top of the viewport
//! when `display_offset == 0`, negative values are scrollback, and every
//! consumer adds `display_offset` to get a display row
//! (`crates/terminal-view/src/render/frame.rs:514-560`). The engine has no
//! negative rows at all, so the conversion is one subtraction against the
//! screen top, in one place.
//!
//! The two-way `display_row` / `row_id` translation `migration.md` sketches is
//! deliberately **not** built: no consumer in this packet speaks `RowId` — the
//! snapshot is display-row shaped end to end — and `US-0082` moves the
//! consumers that would need it.

use std::time::Instant;

use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange as LegacySelectionRange;
use alacritty_terminal::term::cell::{Cell as LegacyCell, Flags, Hyperlink as LegacyHyperlink};
use alacritty_terminal::term::{RenderableCursor, TermMode};
use alacritty_terminal::vte::ansi::{
    Color as LegacyColor, CursorShape as LegacyCursorShape, NamedColor as LegacyNamedColor,
    Rgb as LegacyRgb,
};
use oneterm_vt::grid::{RowId, RowRef};
use oneterm_vt::intern::{ExtrasId, Interner};
use oneterm_vt::render::{ModeSnapshot, MouseEncoding, MouseReporting, RenderContent, RenderRow};
use oneterm_vt::{
    Attrs, Cell as VtCell, CellContent, CellWidth, Color as VtColor, CursorShape, NamedColor,
    RenderState, RenderUpdate, Rgb, SelectionRange, Style, Terminal,
};

use crate::content::{IndexedCell, TermDamageInfo, TerminalBounds, TerminalContent};

/// One consumer's view of the engine, reshaped into the legacy snapshot.
///
/// Owned by [`crate::engine::Engine`] so the render watermark survives across
/// frames: `TerminalModel` is rebuilt per call (`impl_pty_terminal_session!`),
/// so it cannot hold render state of its own.
#[derive(Debug, Default)]
pub(crate) struct LegacySnapshot {
    state: RenderState,
    /// The display row the previous snapshot put the cursor on.
    ///
    /// The reference damages both the old and the new cursor row on every
    /// `damage()` call; the engine reports a cursor move as `Partial` with an
    /// empty `changed` list, so the two rows are added here instead. Without
    /// the old row a moved cursor leaves a ghost.
    last_cursor_row: Option<usize>,
}

impl LegacySnapshot {
    pub(crate) fn new() -> LegacySnapshot {
        LegacySnapshot::default()
    }

    /// Rebuild `out` from the engine. Call with the adapter lock held.
    pub(crate) fn refill(&mut self, term: &mut Terminal, out: &mut TerminalContent) {
        let update = term.render_update(&mut self.state, Instant::now());
        let size = self.state.size();
        let (rows, cols) = (usize::from(size.rows), usize::from(size.cols));
        let display_offset = self.state.scroll_offset() as usize;
        // The reference's `Line(0)`: the viewport top at `display_offset == 0`.
        let screen_top = self.state.viewport_top() + u64::from(self.state.scroll_offset());

        out.cells.clear();
        out.cells.reserve(rows * cols);
        for (index, row) in self.state.rows().iter().enumerate() {
            let line = Line(index as i32 - display_offset as i32);
            push_render_row(&self.state, row, line, &mut out.cells);
        }

        let cursor = self.state.cursor();
        let cursor_line = row_to_line(cursor.id, screen_top);
        out.cursor = RenderableCursor {
            shape: legacy_cursor_shape(term.cursor_style().shape),
            point: Point::new(Line(cursor_line), Column(usize::from(cursor.col))),
        };
        out.mode = legacy_mode(self.state.modes());
        out.display_offset = display_offset;
        out.selection = self
            .state
            .selection()
            .map(|range| legacy_selection(range, screen_top));
        out.terminal_bounds = TerminalBounds {
            num_lines: rows,
            num_cols: cols,
        };
        let screen = term.screen();
        out.total_lines = screen.history_len() as usize + usize::from(screen.rows());
        // `US-0080` owns the engine's graphics; until it lands nothing decodes
        // an image, so the vector is empty rather than stale.
        out.graphics.clear();

        let cursor_row = cursor
            .row
            .map(usize::from)
            .filter(|_| cursor.visible)
            .filter(|row| *row < rows);
        out.damage = self.damage(
            update,
            cursor_row,
            std::mem::replace(&mut out.damage, TermDamageInfo::Full),
        );
        self.last_cursor_row = cursor_row;
    }

    /// Force the next [`LegacySnapshot::refill`] to report full damage.
    pub(crate) fn invalidate(&mut self) {
        self.state.invalidate();
    }

    /// Translate the engine's tri-state into the reference's damage shape.
    ///
    /// The reference escalates to `Full` on any scroll and has no way to say
    /// "shift your cache", so a `Partial { scrolled }` with a non-zero delta is
    /// reported as `Full`. That is the behaviour the view has today; `US-0085`
    /// is where the delta starts paying.
    fn damage(
        &self,
        update: RenderUpdate,
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
        rows.extend(self.state.changed().iter().map(|index| usize::from(*index)));
        for row in [self.last_cursor_row, cursor_row].into_iter().flatten() {
            if !rows.contains(&row) {
                rows.push(row);
            }
        }
        TermDamageInfo::Partial(rows)
    }
}

/// Copy one render row into the snapshot's dense cell vector.
fn push_render_row(state: &RenderState, row: &RenderRow, line: Line, out: &mut Vec<IndexedCell>) {
    let last = row.cells.len().saturating_sub(1);
    for (col, cell) in row.cells.iter().enumerate() {
        let style = row.style_of(cell);
        let mut legacy = legacy_cell(style, cell.width, row.wrapped && col == last);
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
        out.push(IndexedCell {
            point: Point::new(line, Column(col)),
            cell: legacy,
        });
    }
}

/// The damage-free path: one grid row, read straight off the screen.
///
/// `query_line_range_cells`, `last_content_line` and the search snapshot all
/// need cells without touching the render watermark, which is exactly what the
/// reference's `renderable_content()` gave them.
pub(crate) fn push_grid_row(
    row: RowRef<'_>,
    interner: &Interner,
    line: Line,
    out: &mut Vec<IndexedCell>,
) {
    let cells = row.cells();
    let last = cells.len().saturating_sub(1);
    let wrapped = row.wrapped();
    for (col, cell) in cells.iter().enumerate() {
        out.push(IndexedCell {
            point: Point::new(line, Column(col)),
            cell: grid_cell(*cell, interner, wrapped && col == last),
        });
    }
}

/// One grid cell as the legacy snapshot spells it.
pub(crate) fn grid_cell(cell: VtCell, interner: &Interner, wrapline: bool) -> LegacyCell {
    let style = interner.resolve_style(cell.style_id());
    let mut legacy = legacy_cell(style, cell.width(), wrapline);
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
    }
    legacy
}

/// The base cell: colours, attributes and the width class.
fn legacy_cell(style: &Style, width: CellWidth, wrapline: bool) -> LegacyCell {
    let mut cell = LegacyCell {
        fg: legacy_color(style.fg),
        bg: legacy_color(style.bg),
        flags: legacy_flags(style.attrs) | width_flag(width),
        ..LegacyCell::default()
    };
    if wrapline {
        cell.flags |= Flags::WRAPLINE;
    }
    cell
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

/// Blink and overline have no reference bit and the renderer draws neither, so
/// they are dropped rather than approximated (deviation D11, `US-0086`).
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
/// `Cursor`, and `crates/terminal/src/palette.rs:136` does arithmetic on the
/// reference's order. `US-0082` deletes both the arithmetic and this function.
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
