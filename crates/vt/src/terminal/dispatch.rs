//! The semantic layer: which sequence calls which grid operation.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md`.
//!
//! Replaces `vendor/vte/src/ansi.rs` — the file every OneTerm patch has lived in
//! — and the control half of `vendor/alacritty_terminal/src/term/mod.rs`.
//!
//! The governing rule is **correctness first**: where the engine being replaced
//! is wrong, this one is right from the start, and every such row is a `C`
//! correction carried as a declared, cell-level expected difference in the
//! parity corpus. Where the reference is right it is reproduced exactly,
//! including the quirks the corpus pins — `CUU` re-offsetting under `DECOM`,
//! `CHA` moving the cursor down under `DECOM`, and `DECSTBM`'s one-based
//! validity test.

use std::time::Instant;

use crate::cell::{Cell, CellContent, Color, NamedColor, Rgb, Semantic, Style};
use crate::event::{ClipboardKind, VtEvent};
use crate::graphics::{self, SixelParser};
use crate::grid::{
    AnchorKind, Charset, DisplayClear, LineClear, Pos, PrintMode, ScrollRegion, ScrollReport,
};
use crate::parser::{Dispatch, MAX_OSC_PARAMS, OscParams, ParamGroups, Params, StringTerm};
use crate::selection::Invalidation;

/// The parser and the event module each carry the "how did this string end"
/// distinction; they are the same two cases, so the boundary converts.
fn event_term(term: StringTerm) -> crate::event::StringTerm {
    match term {
        StringTerm::Bel => crate::event::StringTerm::Bel,
        StringTerm::St => crate::event::StringTerm::St,
    }
}
use crate::terminal::color::{ColorKey, parse_color};
use crate::terminal::mode::{CursorShape, CursorStyle, FlagApply, KeyboardFlags, Mode, ModeState};
use crate::terminal::{MARK_MAX, State};

/// The dispatch sink, built by `Terminal::feed` over its own fields.
pub(crate) struct Handler<'a> {
    pub(crate) state: &'a mut State,
    pub(crate) out: &'a mut crate::event::EventBatch,
    pub(crate) now: Instant,
}

/// CSI parameters, consumed the way the reference consumes them: `next_or`
/// takes the **first value of the next `;`-separated parameter**, and a value of
/// zero means the default (trap 22).
struct CsiArgs<'a> {
    groups: ParamGroups<'a>,
}

impl<'a> CsiArgs<'a> {
    fn new(params: &'a Params) -> CsiArgs<'a> {
        CsiArgs {
            groups: params.groups(),
        }
    }

    fn next_or(&mut self, default: u16) -> u16 {
        match self.groups.next() {
            Some([value, ..]) if *value != 0 => *value,
            _ => default,
        }
    }

    /// The raw next value, without the zero-means-default rule. `DECSTBM`'s
    /// second parameter needs it: absent and zero are both "to the last row",
    /// but the distinction between "absent" and "present" is what the validity
    /// test reads.
    fn next_raw(&mut self) -> Option<u16> {
        self.groups.next().and_then(|group| group.first().copied())
    }
}

impl Handler<'_> {
    // ── Small accessors ─────────────────────────────────────────────────────

    fn rows(&self) -> u16 {
        self.state.grid.screen().rows()
    }

    fn cols(&self) -> u16 {
        self.state.grid.screen().cols()
    }

    fn cursor_index(&self) -> u16 {
        self.state.grid.screen().cursor_row_index()
    }

    fn cursor_col(&self) -> u16 {
        self.state.grid.screen().cursor().pos.col
    }

    fn region(&self) -> ScrollRegion {
        self.state.grid.screen().region()
    }

    fn origin(&self) -> bool {
        self.state.modes.contains(Mode::Origin)
    }

    fn unhandled(&mut self) {
        self.state.stats.unhandled_sequences = self.state.stats.unhandled_sequences.wrapping_add(1);
    }

    fn reply(&mut self, text: &str) {
        self.out.push_reply(text.as_bytes());
    }

    fn template(&self) -> Cell {
        self.state.grid.screen().cursor().template()
    }

    fn set_template(&mut self, cell: Cell) {
        let State { grid, interner, .. } = self.state;
        grid.screen_mut().set_template(cell, interner);
    }

    fn style(&self) -> Style {
        *self
            .state
            .interner
            .resolve_style(self.template().style_id())
    }

    fn set_style(&mut self, style: Style) {
        let id = self.state.interner.style(&style);
        let template = self.template().with_style(id);
        self.set_template(template);
    }

    fn print_mode(&self) -> PrintMode {
        PrintMode {
            insert: self.state.modes.contains(Mode::Insert),
            autowrap: self.state.modes.contains(Mode::LineWrap),
        }
    }

    fn report(&mut self, report: Option<ScrollReport>) {
        let Some(report) = report else { return };
        self.state.stats.rows_scrolled = self
            .state
            .stats
            .rows_scrolled
            .saturating_add(u32::from(report.history_rows));
        self.out.push_scroll_report(&report);
    }

    // ── Cursor motion ───────────────────────────────────────────────────────

    /// The reference's `goto`, in `i32` because the intermediate line can be
    /// negative: `CUU` past the top computes a negative row before clamping, and
    /// under `DECOM` it is additionally re-offset by the region top. Both are
    /// quirks the corpus pins, so they are reproduced rather than tidied.
    fn goto(&mut self, line: i32, col: u16) {
        let (offset, max) = if self.origin() {
            let region = self.region();
            (i32::from(region.top), i32::from(region.bottom) - 1)
        } else {
            (0, i32::from(self.rows()) - 1)
        };
        let index = (line + offset).min(max).max(0) as u16;
        self.state.grid.screen_mut().goto(index, col);
    }

    // ── Printing ────────────────────────────────────────────────────────────

    fn input(&mut self, c: char) {
        let charset = {
            let cursor = self.state.grid.screen().cursor();
            cursor.charsets[self.state.active_charset]
        };
        let mode = self.print_mode();
        let State { grid, interner, .. } = self.state;
        grid.print(map_charset(charset, c), mode, interner);
    }

    // ── Erase and scroll ────────────────────────────────────────────────────

    /// The invalidation matrix is a **predicate**, and it is stated against the
    /// grid **before** the operation runs — every rule is about the rows the
    /// operation is going to blank. A dispatch that evaluates it afterwards, or
    /// forgets it, leaves a selection pointing at erased cells.
    fn invalidate_selection(&mut self, op: Invalidation) {
        let Some(selection) = self.state.selection else {
            return;
        };
        if selection.invalidated_by(&self.state.grid, op) {
            self.state.selection = None;
            selection.release(&mut self.state.grid);
        }
    }

    fn erase_display(&mut self, mode: DisplayClear) {
        // Trap 12: `ED 2` and `ED 3` clear the screen for the embedder, and the
        // event fires before the "is there history" check, so an `ED 3` with an
        // empty scrollback still reports.
        if matches!(mode, DisplayClear::All | DisplayClear::Saved) {
            self.out.push(VtEvent::ScreenCleared);
        }
        self.invalidate_selection(match mode {
            DisplayClear::Below => Invalidation::EraseBelow,
            DisplayClear::Above => Invalidation::EraseAbove,
            DisplayClear::All => Invalidation::EraseScreen,
            DisplayClear::Saved => Invalidation::EraseHistory,
        });
        let State { grid, interner, .. } = self.state;
        let report = grid.erase_display(mode, interner);
        self.report(report);
    }

    /// `DECSTBM`. The validity test is on the **one-based** parameters, exactly
    /// as the reference writes it, so `CSI 1;1r` is invalid where a zero-based
    /// `top < bottom` test would accept it.
    fn set_scrolling_region(&mut self, top: u16, bottom: Option<u16>) {
        let rows = self.rows();
        let bottom = bottom.unwrap_or(rows);
        if top >= bottom {
            return;
        }
        let start = top.saturating_sub(1).min(rows);
        let end = bottom.min(rows);
        self.state.grid.screen_mut().set_region_raw(start, end);
        self.goto(0, 0);
    }

    /// `DECCOLM`, both `h` and `l`: reset the region, wipe the screen, and
    /// **do not change the column count** (trap 40).
    fn deccolm(&mut self) {
        self.set_scrolling_region(1, None);
        // Not in the design's list of four, but it blanks the whole screen, so
        // the same rule applies: a selection must not survive over wiped cells.
        self.invalidate_selection(Invalidation::EraseScreen);
        let rows = self.rows();
        self.state.grid.screen_mut().reset_rows(0..rows);
        self.state.generation = self.state.generation.wrapping_add(1);
    }

    /// `ESC # 8`. Every cell becomes a default-styled `E` — the reference's
    /// `Cell::default()` with `c = 'E'`, which deliberately ignores the SGR
    /// template — and the cursor does not move.
    fn decaln(&mut self) {
        self.invalidate_selection(Invalidation::EraseScreen);
        let (rows, cols) = (self.rows(), self.cols());
        let fill = Cell::EMPTY.with_content(CellContent::Scalar('E'));
        let screen = self.state.grid.screen_mut();
        for index in 0..rows {
            let id = screen.row_of_index(index);
            screen.row_mut(id).fill(0..cols, fill);
        }
        self.state.generation = self.state.generation.wrapping_add(1);
    }

    // ── Screens, modes and resets ───────────────────────────────────────────

    fn swap_alt(&mut self) {
        self.invalidate_selection(Invalidation::SwapAlt);
        self.state.grid.swap_alt();
        self.state.keyboard.swap();
        self.state.generation = self.state.generation.wrapping_add(1);
    }

    fn set_private_mode(&mut self, code: u16, on: bool) {
        let Some(mode) = Mode::from_private(code) else {
            self.unhandled();
            return;
        };
        match mode {
            // Correction C8: `? 47` and `? 1047` switch screens like `? 1049`,
            // where the reference ignores all three of 47 / 1047 / 1048
            // (trap 13). The three differ in xterm only in whether the cursor is
            // saved and whether the alternate screen is wiped on the way out;
            // `swap_alt` does both, which is `? 1049`'s contract and a superset
            // of the other two.
            Mode::AltScreen | Mode::AltScreen47 | Mode::AltScreen1047 => {
                if on != self.state.grid.alt_active() {
                    self.swap_alt();
                }
            }
            // Correction C8: `? 1048` is `DECSC` / `DECRC` with no screen swap.
            Mode::SaveCursor1048 => {
                let screen = self.state.grid.screen_mut();
                if on {
                    screen.save_cursor();
                } else {
                    screen.restore_cursor();
                }
            }
            Mode::Origin => {
                self.state.modes.set(Mode::Origin, on);
                if on {
                    self.goto(0, 0);
                }
            }
            Mode::DecCoLm => self.deccolm(),
            Mode::CursorBlink => {
                let default = self.state.config.default_cursor_style;
                self.state.cursor_style.get_or_insert(default).blinking = on;
            }
            // Deviation D4: the real state, so `DECRQM` can report it and the
            // host need not poll.
            Mode::SyncUpdate => {
                if on {
                    self.state.sync.begin(self.now);
                } else {
                    self.state.sync.end();
                }
            }
            Mode::MouseClick | Mode::MouseDrag | Mode::MouseMotion => {
                if on {
                    self.state.modes.set_mouse_reporting(mode);
                } else {
                    self.state.modes.set(mode, false);
                }
            }
            Mode::SgrMouse => {
                if on {
                    self.state.modes.set(Mode::Utf8Mouse, false);
                }
                self.state.modes.set(Mode::SgrMouse, on);
            }
            Mode::Utf8Mouse => {
                if on {
                    self.state.modes.set(Mode::SgrMouse, false);
                }
                self.state.modes.set(Mode::Utf8Mouse, on);
            }
            // R-36 and R-56: recognised, inert, and never counted as unhandled.
            // conhost sends `? 9001 h` unprompted at session start and re-injects
            // it after any `l`.
            other => self.state.modes.set(other, on),
        }
    }

    fn set_ansi_mode(&mut self, code: u16, on: bool) {
        match Mode::from_ansi(code) {
            // Deviation D2: `IRM` off does **not** force full damage, where the
            // reference silently disables partial redraw for the session.
            Some(mode) => self.state.modes.set(mode, on),
            None => self.unhandled(),
        }
    }

    fn private_mode_state(&self, code: u16) -> ModeState {
        let Some(mode) = Mode::from_private(code) else {
            return ModeState::NotSupported;
        };
        // A mode the engine recognises but nothing reads never answers `Set`.
        // The rule is a table on `Mode`, not arms here, so a mode that gains a
        // reader leaves it in one place — which is what `? 45` just did (D12).
        if let Some(state) = mode.inert_state() {
            return state;
        }
        match mode {
            Mode::SyncUpdate => self.state.sync.is_set().into(),
            Mode::AltScreen | Mode::AltScreen47 | Mode::AltScreen1047 => {
                self.state.grid.alt_active().into()
            }
            Mode::CursorBlink => self
                .state
                .cursor_style
                .unwrap_or(self.state.config.default_cursor_style)
                .blinking
                .into(),
            other => self.state.modes.contains(other).into(),
        }
    }

    /// `ESC c`. Resets both grids, the region, tab stops, title and title stack,
    /// both keyboard stacks, the charset, the cursor style and the mode set;
    /// keeps the row ids (`DEC-0015`) and `lines_produced`.
    ///
    /// Correction C6: the colour override table is reset too, where the
    /// reference leaves `OSC 4 / 10 / 11 / 12` overrides in place (trap 39).
    fn reset_state(&mut self) {
        self.invalidate_selection(Invalidation::Reset);
        self.state.grid.reset();
        // Pending images and the in-flight decoder go; the id counter does not,
        // so a stale id in the view's store can never name a new image. The
        // placements die through the ordinary release sweep, because the reset
        // blanked every row they cover.
        self.state.graphics.reset();
        self.state.active_charset = 0;
        self.state.cursor_style = None;
        self.state.title.reset();
        self.state.keyboard.reset();
        self.state.modes = crate::terminal::Modes::default();
        self.state.preceding_char = None;
        self.state.modify_other_keys = 0;
        self.state.sync.end();
        if self.state.colors.reset_all() {
            self.state.palette_epoch = self.state.palette_epoch.wrapping_add(1);
        }
        // The hyperlink table's implicit ids are recycled by `RIS`, so a session
        // that clears and restarts cannot accumulate across the cycle.
        self.state.interner.hyperlinks.clear();
        for id in self.state.marks.drain(..) {
            self.state.grid.anchors_mut().release(id);
        }
        self.state.generation = self.state.generation.wrapping_add(1);
        self.out.push(VtEvent::ScreenCleared);
    }

    /// `CSI ! p`, correction C9: the reference does not implement it at all, so
    /// programs that send it currently get nothing. It does **not** clear the
    /// screen, the scrollback, the title or the palette.
    fn soft_reset(&mut self) {
        self.state.modes.set(Mode::Origin, false);
        self.state.modes.set(Mode::Insert, false);
        self.state.modes.set(Mode::LineWrap, true);
        self.state.modes.set(Mode::ShowCursor, true);
        let rows = self.rows();
        self.state.grid.screen_mut().set_region_raw(0, rows);
        self.set_template(Cell::EMPTY);
        self.state.grid.screen_mut().goto(0, 0);
        self.state.grid.screen_mut().save_cursor();
    }

    // ── Tabs ────────────────────────────────────────────────────────────────

    /// `CHT`. Moves only: unlike `HT` it writes no tab cell, and unlike every
    /// other motion it leaves the pending-wrap flag alone — reference behaviour.
    fn move_forward_tabs(&mut self, count: u16) {
        let cols = self.cols();
        let screen = self.state.grid.screen_mut();
        for _ in 0..count {
            let mut col = screen.cursor().pos.col;
            if col == cols - 1 {
                break;
            }
            for candidate in col + 1..cols {
                col = candidate;
                if screen.tabs().is_stop(col) {
                    break;
                }
            }
            screen.cursor_mut().pos.col = col;
        }
    }

    /// `CBT`.
    fn move_backward_tabs(&mut self, count: u16) {
        let screen = self.state.grid.screen_mut();
        for _ in 0..count {
            let mut col = screen.cursor().pos.col;
            if col == 0 {
                break;
            }
            for candidate in (0..col).rev() {
                if screen.tabs().is_stop(candidate) {
                    col = candidate;
                    break;
                }
            }
            screen.cursor_mut().pos.col = col;
        }
    }

    // ── Answers ─────────────────────────────────────────────────────────────

    fn identify_terminal(&mut self, intermediate: Option<u8>) {
        match intermediate {
            // Deviation D13: VT220, Sixel, ANSI colour. `4` is what tmux, lsix,
            // chafa and timg look for.
            None => self.reply("\x1b[?62;4;22c"),
            Some(b'>') => {
                let version = version_number(env!("CARGO_PKG_VERSION"));
                self.reply(&format!("\x1b[>0;{version};1c"));
            }
            _ => self.unhandled(),
        }
    }

    /// `CSI Ps n`. Correction C5: `CPR` is region-relative while `DECOM` is set
    /// (trap 38). Conhost's handshake is unaffected because conhost never sets
    /// origin mode.
    fn device_status(&mut self, arg: u16, private: bool) {
        match (arg, private) {
            (5, false) => self.reply("\x1b[0n"),
            (6, false) => {
                let (row, col) = self.reported_position();
                self.reply(&format!("\x1b[{row};{col}R"));
            }
            // Deviation D7, DECXCPR.
            (6, true) => {
                let (row, col) = self.reported_position();
                self.reply(&format!("\x1b[?{row};{col};1R"));
            }
            _ => self.unhandled(),
        }
    }

    fn reported_position(&self) -> (u16, u16) {
        let index = self.cursor_index();
        let row = if self.origin() {
            index.saturating_sub(self.region().top)
        } else {
            index
        };
        (row + 1, self.cursor_col() + 1)
    }

    fn window_ops(&mut self, arg: u16) {
        match arg {
            14 => {
                let (width, height) = self.state.cell_pixels;
                let pixels_h = u32::from(self.rows()) * u32::from(height);
                let pixels_w = u32::from(self.cols()) * u32::from(width);
                self.reply(&format!("\x1b[4;{pixels_h};{pixels_w}t"));
            }
            18 => {
                let (rows, cols) = (self.rows(), self.cols());
                self.reply(&format!("\x1b[8;{rows};{cols}t"));
            }
            22 => self.state.title.push(),
            23 => {
                if let Some(title) = self.state.title.pop() {
                    self.set_title(title);
                }
            }
            _ => self.unhandled(),
        }
    }

    // ── Title, hyperlinks and marks ─────────────────────────────────────────

    fn set_title(&mut self, title: Option<String>) {
        match &title {
            Some(text) => {
                self.out.push_title(text.as_bytes());
            }
            None => self.out.push(VtEvent::TitleReset),
        }
        self.state.title.title = title;
    }

    /// The `HyperlinkTable` ladder: reuse, insert, or drop the attribute so the
    /// text still renders and the link is simply not clickable. A stream of
    /// un-`id=`-ed OSC 8 links is attacker-reachable from any SSH session, so
    /// the bound cannot wait for a later packet.
    fn set_hyperlink(&mut self, id: Option<&str>, uri: Option<&str>) {
        let extras_id = self.template().extras_id();
        let mut extras = *self.state.interner.resolve_extras(extras_id);
        extras.hyperlink = match uri {
            None => None,
            Some(uri) => match self.state.interner.hyperlinks.intern(id, uri) {
                Some(link) => Some(link),
                None => {
                    self.state.stats.hyperlink_table_exhausted =
                        self.state.stats.hyperlink_table_exhausted.saturating_add(1);
                    if !self.state.hyperlink_warned {
                        self.state.hyperlink_warned = true;
                        log::warn!(
                            "hyperlink table is full; further OSC 8 links render as plain text \
                             for the life of this terminal"
                        );
                    }
                    None
                }
            },
        };
        let new_id = self.state.interner.extras(&extras);
        let template = self.template().with_extras(new_id);
        self.set_template(template);
    }

    /// `OSC 133`: the semantic goes on the template, and a prompt or output mark
    /// registers a tracked anchor so it survives a reflow. Bounded, because a
    /// mark per prompt from a hostile stream is otherwise unbounded growth.
    fn shell_mark(&mut self, kind: u8) {
        let semantic = match kind {
            b'A' => Semantic::Prompt,
            b'B' => Semantic::Input,
            b'C' => Semantic::Output,
            b'D' => Semantic::None,
            _ => {
                self.unhandled();
                return;
            }
        };
        let template = self.template().with_semantic(semantic);
        self.set_template(template);
        if !matches!(kind, b'A' | b'C') {
            return;
        }
        let pos = Pos {
            row: self.state.grid.screen().cursor().pos.row,
            col: 0,
        };
        let mark = self.state.next_mark;
        self.state.next_mark = mark.wrapping_add(1);
        let anchor = self
            .state
            .grid
            .anchors_mut()
            .register(AnchorKind::Mark(mark), pos);
        self.state.marks.push_back(anchor);
        if self.state.marks.len() > MARK_MAX
            && let Some(oldest) = self.state.marks.pop_front()
        {
            self.state.grid.anchors_mut().release(oldest);
        }
    }

    // ── Colours ─────────────────────────────────────────────────────────────

    fn set_color(&mut self, key: ColorKey, color: Rgb) {
        if self.state.colors.set(key, color) {
            self.state.palette_epoch = self.state.palette_epoch.wrapping_add(1);
            self.state.generation = self.state.generation.wrapping_add(1);
        }
    }

    fn reset_color(&mut self, key: ColorKey) {
        if self.state.colors.reset(key) {
            self.state.palette_epoch = self.state.palette_epoch.wrapping_add(1);
            self.state.generation = self.state.generation.wrapping_add(1);
        }
    }

    fn query_color(&mut self, key: ColorKey, terminator: StringTerm) {
        self.out.push(VtEvent::ColorQuery {
            key,
            terminator: event_term(terminator),
        });
    }

    // ── SGR ─────────────────────────────────────────────────────────────────

    fn sgr(&mut self, args: &mut CsiArgs<'_>) {
        let mut style = self.style();
        while let Some(group) = args.groups.next() {
            match group {
                [0] => style = Style::DEFAULT,
                [1] => style.attrs.insert(Attrs::BOLD),
                [2] => style.attrs.insert(Attrs::DIM),
                [3] => style.attrs.insert(Attrs::ITALIC),
                [4, 0] | [24] => style.attrs.remove(Attrs::ALL_UNDERLINES),
                [4, 2] => set_underline(&mut style, Attrs::DOUBLE_UNDERLINE),
                [4, 3] => set_underline(&mut style, Attrs::UNDERCURL),
                [4, 4] => set_underline(&mut style, Attrs::DOTTED_UNDERLINE),
                [4, 5] => set_underline(&mut style, Attrs::DASHED_UNDERLINE),
                [4, ..] => set_underline(&mut style, Attrs::UNDERLINE),
                // Correction C11: stored, where the reference parses them and
                // drops them.
                [5] => style.attrs.insert(Attrs::BLINK_SLOW),
                [6] => style.attrs.insert(Attrs::BLINK_FAST),
                [7] => style.attrs.insert(Attrs::INVERSE),
                [8] => style.attrs.insert(Attrs::HIDDEN),
                [9] => style.attrs.insert(Attrs::STRIKEOUT),
                // Trap 21: `SGR 21` is cancel-bold, not double underline.
                [21] => style.attrs.remove(Attrs::BOLD),
                [22] => style.attrs.remove(Attrs::BOLD | Attrs::DIM),
                [23] => style.attrs.remove(Attrs::ITALIC),
                [25] => style.attrs.remove(Attrs::BLINK_SLOW | Attrs::BLINK_FAST),
                [27] => style.attrs.remove(Attrs::INVERSE),
                [28] => style.attrs.remove(Attrs::HIDDEN),
                [29] => style.attrs.remove(Attrs::STRIKEOUT),
                [value @ 30..=37] => style.fg = Color::Named(ansi_color(value - 30, false)),
                [38] => {
                    if let Some(color) = parse_sgr_color(&mut args.groups) {
                        style.fg = color;
                    }
                }
                [38, rest @ ..] => {
                    if let Some(color) = colon_color(rest) {
                        style.fg = color;
                    }
                }
                [39] => style.fg = Color::Named(NamedColor::Foreground),
                [value @ 40..=47] => style.bg = Color::Named(ansi_color(value - 40, false)),
                [48] => {
                    if let Some(color) = parse_sgr_color(&mut args.groups) {
                        style.bg = color;
                    }
                }
                [48, rest @ ..] => {
                    if let Some(color) = colon_color(rest) {
                        style.bg = color;
                    }
                }
                [49] => style.bg = Color::Named(NamedColor::Background),
                // Correction C11.
                [53] => style.attrs.insert(Attrs::OVERLINE),
                [55] => style.attrs.remove(Attrs::OVERLINE),
                [58] => {
                    if let Some(color) = parse_sgr_color(&mut args.groups) {
                        style.underline_color = Some(color);
                    }
                }
                [58, rest @ ..] => {
                    if let Some(color) = colon_color(rest) {
                        style.underline_color = Some(color);
                    }
                }
                [59] => style.underline_color = None,
                [value @ 90..=97] => style.fg = Color::Named(ansi_color(value - 90, true)),
                [value @ 100..=107] => style.bg = Color::Named(ansi_color(value - 100, true)),
                // Skipped; the rest of the SGR list still processes.
                _ => {}
            }
        }
        self.set_style(style);
    }
}

use crate::cell::Attrs;

fn set_underline(style: &mut Style, attr: Attrs) {
    // Trap 21: any underline attribute clears the other underline bits first.
    style.attrs.remove(Attrs::ALL_UNDERLINES);
    style.attrs.insert(attr);
}

fn ansi_color(index: u16, bright: bool) -> NamedColor {
    match (index, bright) {
        (0, false) => NamedColor::Black,
        (1, false) => NamedColor::Red,
        (2, false) => NamedColor::Green,
        (3, false) => NamedColor::Yellow,
        (4, false) => NamedColor::Blue,
        (5, false) => NamedColor::Magenta,
        (6, false) => NamedColor::Cyan,
        (0, true) => NamedColor::BrightBlack,
        (1, true) => NamedColor::BrightRed,
        (2, true) => NamedColor::BrightGreen,
        (3, true) => NamedColor::BrightYellow,
        (4, true) => NamedColor::BrightBlue,
        (5, true) => NamedColor::BrightMagenta,
        (6, true) => NamedColor::BrightCyan,
        (_, false) => NamedColor::White,
        (_, true) => NamedColor::BrightWhite,
    }
}

/// `38;5;n` and `38;2;r;g;b`: the selector and its values are **following
/// parameters**. A value above 255 aborts the attribute after the parameters it
/// already consumed (trap 20).
fn parse_sgr_color(groups: &mut ParamGroups<'_>) -> Option<Color> {
    let mut next = || groups.next().and_then(|group| group.first().copied());
    match next()? {
        2 => Some(Color::Rgb(Rgb {
            r: u8::try_from(next()?).ok()?,
            g: u8::try_from(next()?).ok()?,
            b: u8::try_from(next()?).ok()?,
        })),
        5 => Some(Color::Palette(u8::try_from(next()?).ok()?)),
        _ => None,
    }
}

/// `38:5:n`, `38:2:r:g:b` and `38:2:cs:r:g:b`: the values are sub-parameters of
/// this parameter, and the six-element form carries a colour-space id that is
/// skipped.
fn colon_color(rest: &[u16]) -> Option<Color> {
    let start = if rest.len() > 4 { 2 } else { 1 };
    let mut values = std::iter::once(rest[0]).chain(rest.get(start..)?.iter().copied());
    let mut next = || values.next();
    match next()? {
        2 => Some(Color::Rgb(Rgb {
            r: u8::try_from(next()?).ok()?,
            g: u8::try_from(next()?).ok()?,
            b: u8::try_from(next()?).ok()?,
        })),
        5 => Some(Color::Palette(u8::try_from(next()?).ok()?)),
        _ => None,
    }
}

/// `CARGO_PKG_VERSION` as `major * 10000 + minor * 100 + patch`.
pub(super) fn version_number(version: &str) -> u32 {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    major * 10_000 + minor * 100 + patch
}

/// The DEC special graphics set. Ported verbatim, because the recordings pin it.
fn map_charset(charset: Charset, c: char) -> char {
    match charset {
        Charset::Ascii => c,
        Charset::SpecialCharacterAndLineDrawing => match c {
            '_' => ' ',
            '`' => '◆',
            'a' => '▒',
            'b' => '\u{2409}',
            'c' => '\u{240c}',
            'd' => '\u{240d}',
            'e' => '\u{240a}',
            'f' => '°',
            'g' => '±',
            'h' => '\u{2424}',
            'i' => '\u{240b}',
            'j' => '┘',
            'k' => '┐',
            'l' => '┌',
            'm' => '└',
            'n' => '┼',
            'o' => '⎺',
            'p' => '⎻',
            'q' => '─',
            'r' => '⎼',
            's' => '⎽',
            't' => '├',
            'u' => '┤',
            'v' => '┴',
            'w' => '┬',
            'x' => '│',
            'y' => '≤',
            'z' => '≥',
            '{' => 'π',
            '|' => '≠',
            '}' => '£',
            '~' => '·',
            _ => c,
        },
    }
}

impl Dispatch for Handler<'_> {
    fn print_str(&mut self, text: &str) {
        self.state.dispatched = true;
        for c in text.chars() {
            self.input(c);
            // Trap 43: `preceding_char` is the raw scalar, before the charset
            // mapping, and it survives intervening escape sequences.
            self.state.preceding_char = Some(c);
        }
    }

    fn execute(&mut self, byte: u8) {
        self.state.dispatched = true;
        match byte {
            0x09 => {
                let autowrap = self.state.modes.contains(Mode::LineWrap);
                self.state.grid.put_tab(1, autowrap);
            }
            0x08 => {
                // Deviation D12: `? 45` is the reader `US-0086` gave the mode.
                let reverse_wrap = self.state.modes.contains(Mode::ReverseWrap);
                self.state.grid.screen_mut().backspace(reverse_wrap);
            }
            0x0d => self.state.grid.screen_mut().carriage_return(),
            0x0a..=0x0c => {
                let report = self.state.grid.linefeed();
                self.report(report);
            }
            0x07 => self.out.push(VtEvent::Bell),
            // `SUB` is a no-op, as in the reference; `DEL` is execute-no-op.
            0x1a | 0x7f => {}
            0x0f => self.state.active_charset = 0,
            0x0e => self.state.active_charset = 1,
            _ => self.unhandled(),
        }
    }

    fn esc(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.state.dispatched = true;
        if ignore || intermediates.len() > 2 {
            self.unhandled();
            return;
        }
        let charset_index = |intermediates: &[u8]| match intermediates {
            [b'('] => Some(0),
            [b')'] => Some(1),
            [b'*'] => Some(2),
            [b'+'] => Some(3),
            _ => None,
        };
        match (byte, intermediates) {
            (b'B', rest) | (b'0', rest) => {
                let Some(index) = charset_index(rest) else {
                    self.unhandled();
                    return;
                };
                let charset = if byte == b'B' {
                    Charset::Ascii
                } else {
                    Charset::SpecialCharacterAndLineDrawing
                };
                self.state.grid.screen_mut().cursor_mut().charsets[index] = charset;
            }
            (b'D', []) => {
                let report = self.state.grid.linefeed();
                self.report(report);
            }
            (b'E', []) => {
                let report = self.state.grid.linefeed();
                self.report(report);
                self.state.grid.screen_mut().carriage_return();
            }
            (b'H', []) => {
                let col = self.cursor_col();
                self.state.grid.screen_mut().tabs_mut().set(col);
            }
            (b'M', []) => {
                let report = self.state.grid.reverse_index();
                self.report(report);
            }
            (b'Z', []) => self.identify_terminal(None),
            (b'c', []) => self.reset_state(),
            (b'7', []) => self.state.grid.screen_mut().save_cursor(),
            (b'8', [b'#']) => self.decaln(),
            (b'8', []) => self.state.grid.screen_mut().restore_cursor(),
            (b'=', []) => self.state.modes.set(Mode::AppKeypad, true),
            (b'>', []) => self.state.modes.set(Mode::AppKeypad, false),
            // String terminator: the parser already closed the string.
            (b'\\', []) => {}
            _ => self.unhandled(),
        }
    }

    fn csi(&mut self, params: &Params, intermediates: &[u8], ignore: bool, byte: u8) {
        self.state.dispatched = true;
        if ignore || intermediates.len() > 2 {
            self.unhandled();
            return;
        }
        let mut args = CsiArgs::new(params);
        match (byte, intermediates) {
            (b'@', []) => {
                let count = args.next_or(1).min(self.cols() - self.cursor_col());
                self.state.grid.screen_mut().insert_blanks(count);
            }
            (b'A', []) => {
                let (line, col) = (i32::from(self.cursor_index()), self.cursor_col());
                let n = i32::from(args.next_or(1));
                self.goto(line - n, col);
            }
            (b'B', []) | (b'e', []) => {
                let (line, col) = (i32::from(self.cursor_index()), self.cursor_col());
                let n = i32::from(args.next_or(1));
                self.goto(line + n, col);
            }
            // Trap 43: `REP` replays through the full print path, so it wraps,
            // honours insert mode and re-triggers wide handling.
            (b'b', []) => {
                let count = args.next_or(1);
                if let Some(c) = self.state.preceding_char {
                    for _ in 0..count {
                        self.input(c);
                    }
                }
            }
            (b'C', []) | (b'a', []) => {
                let n = args.next_or(1);
                self.state.grid.screen_mut().move_forward(n);
            }
            (b'c', rest) => {
                if args.next_or(0) == 0 {
                    self.identify_terminal(rest.first().copied());
                } else {
                    self.unhandled();
                }
            }
            (b'D', []) => {
                let n = args.next_or(1);
                self.state.grid.screen_mut().move_backward(n);
            }
            (b'd', []) => {
                let col = self.cursor_col();
                let n = i32::from(args.next_or(1));
                self.goto(n - 1, col);
            }
            (b'E', []) => {
                let line = i32::from(self.cursor_index());
                let n = i32::from(args.next_or(1));
                self.goto(line + n, 0);
            }
            (b'F', []) => {
                let line = i32::from(self.cursor_index());
                let n = i32::from(args.next_or(1));
                self.goto(line - n, 0);
            }
            (b'G', []) | (b'`', []) => {
                // The reference routes this through `goto` with the *absolute*
                // current line, so under `DECOM` a bare `CHA` also moves the
                // cursor down into the region. Reproduced.
                let line = i32::from(self.cursor_index());
                let col = args.next_or(1) - 1;
                self.goto(line, col);
            }
            // Correction C10: `CSI ? 5 W` restores the default tab stops.
            (b'W', [b'?']) => {
                if args.next_or(0) == 5 {
                    self.state.grid.screen_mut().tabs_mut().reset_defaults();
                } else {
                    self.unhandled();
                }
            }
            (b'g', []) => {
                let col = self.cursor_col();
                let tabs = self.state.grid.screen_mut().tabs_mut();
                match args.next_or(0) {
                    0 => tabs.clear(col),
                    3 => tabs.clear_all(),
                    _ => self.unhandled(),
                }
            }
            (b'H', []) | (b'f', []) => {
                let row = i32::from(args.next_or(1));
                let col = args.next_or(1) - 1;
                self.goto(row - 1, col);
            }
            (b'h', []) => {
                while let Some(code) = args.next_raw() {
                    self.set_ansi_mode(code, true);
                }
            }
            (b'h', [b'?']) => {
                while let Some(code) = args.next_raw() {
                    self.set_private_mode(code, true);
                }
            }
            (b'l', []) => {
                while let Some(code) = args.next_raw() {
                    self.set_ansi_mode(code, false);
                }
            }
            (b'l', [b'?']) => {
                while let Some(code) = args.next_raw() {
                    self.set_private_mode(code, false);
                }
            }
            (b'I', []) => {
                let n = args.next_or(1);
                self.move_forward_tabs(n);
            }
            (b'Z', []) => {
                let n = args.next_or(1);
                self.move_backward_tabs(n);
            }
            (b'J', []) => match args.next_or(0) {
                0 => self.erase_display(DisplayClear::Below),
                1 => self.erase_display(DisplayClear::Above),
                2 => self.erase_display(DisplayClear::All),
                3 => self.erase_display(DisplayClear::Saved),
                _ => self.unhandled(),
            },
            (b'K', []) => {
                let mode = match args.next_or(0) {
                    0 => LineClear::Right,
                    1 => LineClear::Left,
                    2 => LineClear::All,
                    _ => {
                        self.unhandled();
                        return;
                    }
                };
                self.invalidate_selection(Invalidation::EraseLine);
                self.state.grid.screen_mut().erase_line(mode);
            }
            // SCP: parsed and ignored, as today, but now counted.
            (b'k', [b' ']) => self.unhandled(),
            (b'L', []) => {
                let n = args.next_or(1).min(self.region().height());
                let report = self.state.grid.insert_lines(n);
                self.report(report);
            }
            (b'M', []) => {
                let index = self.cursor_index();
                let n = args
                    .next_or(1)
                    .min(self.rows() - index)
                    .min(self.region().height());
                let report = self.state.grid.delete_lines(n);
                self.report(report);
            }
            (b'm', []) => self.sgr(&mut args),
            // Deviation D10: the level is stored and reportable, where the
            // reference parses both forms and implements neither.
            (b'm', [b'>']) => {
                let level = if args.next_or(1) == 4 {
                    Some(args.next_or(0))
                } else {
                    None
                };
                match level {
                    Some(value @ 0..=2) => self.state.modify_other_keys = value as u8,
                    _ => self.unhandled(),
                }
            }
            (b'm', [b'?']) => {
                if args.next_raw() == Some(4) {
                    let level = self.state.modify_other_keys;
                    self.reply(&format!("\x1b[>4;{level}m"));
                } else {
                    self.unhandled();
                }
            }
            (b'n', []) => {
                let arg = args.next_or(0);
                self.device_status(arg, false);
            }
            (b'n', [b'?']) => {
                let arg = args.next_or(0);
                self.device_status(arg, true);
            }
            (b'P', []) => {
                let n = args.next_or(1).min(self.cols());
                self.state.grid.screen_mut().delete_chars(n);
            }
            (b'p', [b'!']) => self.soft_reset(),
            (b'p', [b'$']) => {
                let code = args.next_or(0);
                let state = match Mode::from_ansi(code) {
                    // Same rule as the private space, from the same table: a
                    // mode the engine recognises but nothing reads never
                    // answers `Set` (`US-0087`). LNM is the only one.
                    Some(mode) => mode
                        .inert_state()
                        .unwrap_or_else(|| self.state.modes.contains(mode).into()),
                    None => ModeState::NotSupported,
                } as u8;
                self.reply(&format!("\x1b[{code};{state}$y"));
            }
            (b'p', [b'?', b'$']) => {
                let code = args.next_or(0);
                let state = self.private_mode_state(code) as u8;
                self.reply(&format!("\x1b[?{code};{state}$y"));
            }
            // Deviation D8: XTVERSION.
            (b'q', [b'>']) => {
                if args.next_or(0) == 0 {
                    let version = env!("CARGO_PKG_VERSION");
                    self.reply(&format!("\x1bP>|OneTerm({version})\x1b\\"));
                } else {
                    self.unhandled();
                }
            }
            (b'q', [b' ']) => {
                let id = args.next_or(0);
                let shape = match id {
                    0 => None,
                    1 | 2 => Some(CursorShape::Block),
                    3 | 4 => Some(CursorShape::Underline),
                    5 | 6 => Some(CursorShape::Beam),
                    _ => {
                        self.unhandled();
                        return;
                    }
                };
                self.state.cursor_style = shape.map(|shape| CursorStyle {
                    shape,
                    blinking: id % 2 == 1,
                });
            }
            (b'r', []) => {
                let top = args.next_or(1);
                let bottom = args.next_raw().filter(|&value| value != 0);
                self.set_scrolling_region(top, bottom);
            }
            (b'S', []) => {
                let n = args.next_or(1);
                let region = self.region();
                let report = self.state.grid.scroll_up(region, n);
                self.report(Some(report));
            }
            (b's', []) => self.state.grid.screen_mut().save_cursor(),
            (b'T', []) => {
                let n = args.next_or(1);
                let region = self.region();
                let report = self.state.grid.scroll_down(region, n);
                self.report(Some(report));
            }
            (b't', []) => {
                let arg = args.next_or(1);
                self.window_ops(arg);
            }
            // Trap 42: the query reads the stack top, which can legitimately
            // differ from the live flags after `CSI = Ps u`.
            (b'u', [b'?']) => {
                let flags = self.state.keyboard.active.top().bits();
                self.reply(&format!("\x1b[?{flags}u"));
            }
            (b'u', [b'=']) => {
                let flags = KeyboardFlags::from_bits_truncate(args.next_or(0) as u8);
                let how = match args.next_or(1) {
                    3 => FlagApply::Difference,
                    2 => FlagApply::Union,
                    _ => FlagApply::Replace,
                };
                self.state.keyboard.active.apply(flags, how);
            }
            (b'u', [b'>']) => {
                let flags = KeyboardFlags::from_bits_truncate(args.next_or(0) as u8);
                self.state.keyboard.active.push(flags);
            }
            // Deviation D15: the default is 1, and popping at or beyond the
            // depth resets this stack — the reference pops the *title* stack.
            (b'u', [b'<']) => {
                let count = args.next_or(1);
                self.state.keyboard.active.pop(count);
            }
            (b'u', []) => self.state.grid.screen_mut().restore_cursor(),
            (b'X', []) => {
                let n = args.next_or(1);
                self.state.grid.screen_mut().erase_chars(n);
            }
            _ => self.unhandled(),
        }
    }

    fn osc(
        &mut self,
        code: Option<u32>,
        params: &OscParams<'_>,
        term: StringTerm,
        truncated: bool,
    ) {
        self.state.dispatched = true;
        if truncated {
            self.state.stats.truncated_osc = self.state.stats.truncated_osc.saturating_add(1);
        }
        let Some(code) = code else {
            self.unhandled();
            return;
        };
        match code {
            0 | 2 => {
                if params.len() < 2 {
                    self.unhandled();
                    return;
                }
                let title = (1..params.len())
                    .filter_map(|index| params.get(index))
                    .filter_map(|part| str::from_utf8(part).ok())
                    .collect::<Vec<&str>>()
                    .join(";")
                    .trim()
                    .to_owned();
                self.set_title(Some(title));
            }
            4 => self.osc_palette(params, term),
            8 => {
                if params.len() <= 2 {
                    self.unhandled();
                    return;
                }
                let link_params = params.get(1).unwrap_or_default();
                let uri = (2..params.len())
                    .filter_map(|index| params.get(index))
                    .map(|part| String::from_utf8_lossy(part).into_owned())
                    .collect::<Vec<String>>()
                    .join(";");
                if uri.is_empty() {
                    self.set_hyperlink(None, None);
                    return;
                }
                let id = link_params
                    .split(|&byte| byte == b':')
                    .find_map(|pair| pair.strip_prefix(b"id="))
                    .and_then(|value| str::from_utf8(value).ok())
                    .map(str::to_owned);
                self.set_hyperlink(id.as_deref(), Some(&uri));
            }
            10..=12 => self.osc_dynamic_color(code, params, term),
            // Mouse cursor icon: parsed and ignored, but counted.
            22 => self.unhandled(),
            50 => {
                let shape = params
                    .get(1)
                    .and_then(|value| value.strip_prefix(b"CursorShape="))
                    .and_then(|value| value.first().copied());
                let shape = match shape {
                    Some(b'0') => CursorShape::Block,
                    Some(b'1') => CursorShape::Beam,
                    Some(b'2') => CursorShape::Underline,
                    _ => {
                        self.unhandled();
                        return;
                    }
                };
                let default = self.state.config.default_cursor_style;
                self.state.cursor_style.get_or_insert(default).shape = shape;
            }
            52 => self.osc_clipboard(params),
            104 => self.osc_reset_palette(params),
            110 => self.reset_color(ColorKey::Foreground),
            111 => self.reset_color(ColorKey::Background),
            112 => self.reset_color(ColorKey::Cursor),
            other => {
                // `OSC 133` marks are engine state as well as an embedder
                // signal: the semantic goes on the template here, and the
                // claim still forwards the whole sequence.
                if other == 133
                    && let Some(kind) = params.get(1).and_then(|value| value.first().copied())
                {
                    self.shell_mark(kind);
                }
                if self.state.config.osc_claims.is_claimed(other) {
                    self.forward_osc(other, params, term, truncated);
                } else {
                    self.unhandled();
                }
            }
        }
    }

    fn osc_allows_large(&self, code: u32) -> bool {
        self.state.config.osc_claims.allows_large(code)
    }

    /// Only Sixel (`DCS q`) is decoded (`US-0080`). Any other final byte clears
    /// an in-flight decoder, so a non-Sixel DCS arriving mid-Sixel aborts the
    /// prior unterminated one — parity with the engine being replaced.
    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], byte: u8) {
        self.state.dispatched = true;
        if byte == b'q' {
            self.state.graphics.parser = Some(SixelParser::new());
        } else {
            self.state.graphics.parser = None;
            self.unhandled();
        }
    }

    fn dcs_put(&mut self, byte: u8) {
        if let Some(parser) = self.state.graphics.parser.as_mut() {
            parser.put(byte);
        }
    }

    fn dcs_unhook(&mut self, aborted: bool) {
        let parser = self.state.graphics.parser.take();
        if aborted {
            // A `CAN`/`SUB` abort or a payload past `DCS_MAX_BYTES`: the partial
            // image is discarded and no cell is stamped.
            self.state.stats.aborted_dcs = self.state.stats.aborted_dcs.saturating_add(1);
            return;
        }
        let Some(image) = parser.and_then(SixelParser::finish) else {
            return;
        };
        for report in graphics::place(self.state, image) {
            self.report(Some(report));
        }
    }

    fn apc_start(&mut self, _introducer: u8) {
        self.state.dispatched = true;
        self.unhandled();
    }

    fn apc_put(&mut self, _byte: u8) {}

    fn apc_end(&mut self, _aborted: bool) {}
}

impl Handler<'_> {
    /// Parameters, with the sixteenth re-split on `;` (P9): the parser joins
    /// everything past [`MAX_OSC_PARAMS`] into the last slot instead of
    /// discarding it the way the reference does, and `OSC 4` is the sequence
    /// that actually reaches sixteen — a bulk palette set passes it at nine
    /// colours.
    fn osc_args<'a>(&self, params: &OscParams<'a>) -> Vec<&'a [u8]> {
        let mut out: Vec<&[u8]> = Vec::with_capacity(params.len());
        for index in 0..params.len() {
            let Some(param) = params.get(index) else {
                continue;
            };
            if index + 1 == MAX_OSC_PARAMS {
                out.extend(param.split(|&byte| byte == b';'));
            } else {
                out.push(param);
            }
        }
        out
    }

    /// Correction C7: every complete `index;spec` pair is applied and a trailing
    /// odd parameter is ignored, where the reference rejects an even parameter
    /// count wholesale (trap 26). An index above 255 is still rejected.
    fn osc_palette(&mut self, params: &OscParams<'_>, term: StringTerm) {
        let args = self.osc_args(params);
        if args.len() < 3 {
            self.unhandled();
            return;
        }
        for pair in args[1..].chunks_exact(2) {
            let Some(index) = parse_index(pair[0]) else {
                self.unhandled();
                continue;
            };
            let key = ColorKey::Palette(index);
            if let Some(color) = parse_color(pair[1]) {
                self.set_color(key, color);
            } else if pair[1] == b"?" {
                self.query_color(key, term);
            } else {
                self.unhandled();
            }
        }
    }

    /// `OSC 10 / 11 / 12`: a multi-parameter form advances the key and stops
    /// past `Cursor`.
    fn osc_dynamic_color(&mut self, code: u32, params: &OscParams<'_>, term: StringTerm) {
        if params.len() < 2 {
            self.unhandled();
            return;
        }
        let mut dynamic = code;
        for index in 1..params.len() {
            let Some(param) = params.get(index) else {
                continue;
            };
            let key = match dynamic {
                10 => ColorKey::Foreground,
                11 => ColorKey::Background,
                12 => ColorKey::Cursor,
                _ => {
                    self.unhandled();
                    break;
                }
            };
            if let Some(color) = parse_color(param) {
                self.set_color(key, color);
            } else if param == b"?" {
                self.query_color(key, term);
            } else {
                self.unhandled();
            }
            dynamic += 1;
        }
    }

    /// `OSC 104`. Trap 26: with no parameter it resets indices 0..=255 and
    /// **not** foreground / background / cursor.
    fn osc_reset_palette(&mut self, params: &OscParams<'_>) {
        let args = self.osc_args(params);
        if args.len() == 1 || args.get(1).is_some_and(|value| value.is_empty()) {
            if self.state.colors.reset_indexed() {
                self.state.palette_epoch = self.state.palette_epoch.wrapping_add(1);
                self.state.generation = self.state.generation.wrapping_add(1);
            }
            return;
        }
        for param in &args[1..] {
            match parse_index(param) {
                Some(index) => self.reset_color(ColorKey::Palette(index)),
                None => self.unhandled(),
            }
        }
    }

    /// `OSC 52`. Trap 25: the selection byte must be `c`, `p` or `s`, and
    /// undecodable base64 or invalid UTF-8 is dropped silently. The engine
    /// reports; the **policy** stays in `crates/terminal/src/security_policy.rs`.
    fn osc_clipboard(&mut self, params: &OscParams<'_>) {
        if params.len() < 3 {
            self.unhandled();
            return;
        }
        let byte = params
            .get(1)
            .and_then(|value| value.first().copied())
            .unwrap_or(b'c');
        let selection = match byte {
            b'c' => ClipboardKind::Clipboard,
            b'p' => ClipboardKind::Primary,
            b's' => ClipboardKind::Secondary,
            _ => {
                self.unhandled();
                return;
            }
        };
        let payload = params.get(2).unwrap_or_default();
        if payload == b"?" {
            self.out.push(VtEvent::ClipboardLoad { selection });
            return;
        }
        let Some(decoded) = base64_decode(payload) else {
            self.state.stats.malformed_sequences =
                self.state.stats.malformed_sequences.saturating_add(1);
            return;
        };
        if !self.out.push_clipboard_store(selection, &decoded) {
            self.state.stats.malformed_sequences =
                self.state.stats.malformed_sequences.saturating_add(1);
        }
    }

    fn forward_osc(
        &mut self,
        code: u32,
        params: &OscParams<'_>,
        term: StringTerm,
        truncated: bool,
    ) {
        let args = self.osc_args(params);
        self.out.push_osc(code, &args, event_term(term), truncated);
    }
}

/// A palette index: a decimal run that fits in a `u8`, so 256 and above is
/// rejected (trap 26).
fn parse_index(digits: &[u8]) -> Option<u8> {
    if digits.is_empty() {
        return None;
    }
    let mut value: u8 = 0;
    for &byte in digits {
        let digit = (byte as char).to_digit(10)?;
        value = value.checked_mul(10)?.checked_add(digit as u8)?;
    }
    Some(value)
}

/// Standard base64 with canonical padding. `None` for anything else, which is
/// what makes the OSC 52 drop silent rather than partial.
fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    if input.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    for chunk in input.chunks_exact(4) {
        let mut bits: u32 = 0;
        let mut kept = 3;
        for (position, &byte) in chunk.iter().enumerate() {
            let value = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                // Padding is only legal in the last two positions, and only at
                // the end.
                b'=' if position >= 2 => {
                    kept = kept.min(position - 1);
                    0
                }
                _ => return None,
            };
            bits = (bits << 6) | u32::from(value);
        }
        out.extend_from_slice(&bits.to_be_bytes()[1..1 + kept]);
    }
    Some(out)
}
