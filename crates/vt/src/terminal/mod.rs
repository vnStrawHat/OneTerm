//! `Terminal` — parser + dispatch + grid + the render hand-off.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md`
//! and `.../events-and-api.md`.
//!
//! This is the engine's one public object. It holds no lock, spawns no thread,
//! returns no `Result` and never panics on input: a malformed or hostile stream
//! is dropped, truncated or degraded to a documented fallback, and counted in
//! [`FeedStats`].
//!
//! The field split the design asks for (R-32) is one indirection rather than
//! fifteen borrows: [`Terminal`] owns the [`Parser`] and a [`State`], and
//! `feed` builds a `Handler { state, out, now }` over the second while the
//! first drives it.

mod color;
mod dispatch;
mod mode;
mod osc;

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;

use std::collections::VecDeque;
use std::time::Instant;

pub use color::{COLOR_COUNT, ColorKey, ColorOverrides};
pub use mode::{
    CursorShape, CursorStyle, FlagApply, KEYBOARD_STACK_MAX, KeyboardFlags, KeyboardStacks, Mode,
    ModeState, Modes, TITLE_STACK_MAX, TitleState,
};
pub use osc::OscClaims;

use crate::cell::{Cell, Style};
use crate::event::{EventBatch, FeedStats};
use crate::grid::{
    AnchorId, Charset, DEFAULT_SCROLLBACK, Pos, RowId, Screen, Size, TerminalGrid, Viewport,
};
use crate::intern::Interner;
use crate::parser::Parser;
use crate::reflow::{ResizeOutcome, ResizePolicy};
use crate::render::{
    EngineView, ModeSnapshot, MouseProtocol, Palette, RenderState, RenderUpdate, SyncState,
};
use crate::selection::{Selection, SelectionKind, SelectionRange, Side};

/// The theme the engine needs to answer a colour query. The renderer still
/// resolves final colours itself, including bold-to-bright and dim mixing.
pub type ThemeColors = Palette;

/// Tracked `OSC 133` marks. Bounded because a mark per prompt from a hostile
/// stream is otherwise unbounded anchor growth; the oldest is released.
const MARK_MAX: usize = 1024;

/// Everything a terminal is configured with. No dead knobs: the fork's
/// `vi_mode_cursor_style`, `kitty_keyboard` and `osc52` are gone, the last
/// because the engine never applies a clipboard policy.
#[derive(Clone, Debug)]
pub struct Config {
    pub scrollback_limit: u32,
    /// Which OSC numbers reach the embedder, and which may spill.
    pub osc_claims: OscClaims,
    pub default_cursor_style: CursorStyle,
    /// Word-selection characters (`selection.md`); carried here so the engine
    /// has one configuration object.
    pub semantic_escape_chars: String,
    /// The `S8C1T` hook. `false`, so an 8-bit C1 byte is executed rather than
    /// treated as an introducer (trap 48).
    pub accept_c1: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            scrollback_limit: DEFAULT_SCROLLBACK,
            osc_claims: OscClaims::new(),
            default_cursor_style: CursorStyle::default(),
            semantic_escape_chars: ",│`|:\"' ()[]{}<>\t".to_owned(),
            accept_c1: false,
        }
    }
}

/// Everything but the parser, so `feed` can hand one `&mut` to the handler.
pub(crate) struct State {
    pub(crate) grid: TerminalGrid,
    pub(crate) interner: Interner,
    pub(crate) modes: Modes,
    pub(crate) colors: ColorOverrides,
    pub(crate) title: TitleState,
    pub(crate) keyboard: KeyboardStacks,
    pub(crate) sync: SyncState,
    /// Owns two entries in the anchor list while it lives, which is why every
    /// path that drops it goes through `Terminal::selection_clear`.
    pub(crate) selection: Option<Selection>,
    pub(crate) config: Config,
    pub(crate) theme: ThemeColors,
    pub(crate) cursor_style: Option<CursorStyle>,
    /// Which of `G0..G3` `SI` / `SO` selected. Lives on the terminal, not the
    /// cursor, so `DECSC` / `DECRC` do not save it — reference behaviour.
    pub(crate) active_charset: usize,
    /// `REP`'s source, which survives intervening escape sequences (trap 43).
    pub(crate) preceding_char: Option<char>,
    pub(crate) modify_other_keys: u8,
    pub(crate) cell_pixels: (u16, u16),
    pub(crate) marks: VecDeque<AnchorId>,
    pub(crate) next_mark: u32,
    /// Bumped on anything that invalidates every row.
    pub(crate) generation: u32,
    /// Bumped when the OSC colour overrides change.
    pub(crate) palette_epoch: u32,
    pub(crate) stats: FeedStats,
    /// `log::warn!` once per session, not once per link.
    pub(crate) hyperlink_warned: bool,
    /// Whether this batch dispatched anything at all, which is what decides the
    /// end-of-batch `Repaint` hint.
    pub(crate) dispatched: bool,
}

/// The VT engine.
pub struct Terminal {
    parser: Parser,
    state: State,
}

impl Terminal {
    /// A terminal at `size`, in its power-on state.
    pub fn new(size: Size, config: Config) -> Terminal {
        let size = size.clamped();
        Terminal {
            parser: Parser::new(),
            state: State {
                grid: TerminalGrid::new(size, config.scrollback_limit),
                interner: Interner::default(),
                modes: Modes::default(),
                colors: ColorOverrides::default(),
                title: TitleState::default(),
                keyboard: KeyboardStacks::default(),
                sync: SyncState::new(),
                selection: None,
                theme: ThemeColors::new(),
                cursor_style: None,
                active_charset: 0,
                preceding_char: None,
                modify_other_keys: 0,
                cell_pixels: (0, 0),
                marks: VecDeque::new(),
                next_mark: 0,
                generation: 0,
                palette_epoch: 0,
                stats: FeedStats::default(),
                hyperlink_warned: false,
                dispatched: false,
                config,
            },
        }
    }

    // ── Feeding ─────────────────────────────────────────────────────────────

    /// Parse `bytes`, mutating the grid and filling `batch`.
    ///
    /// Clears `batch` first: a caller who has not drained the previous batch
    /// loses it, which is a programming error rather than a recoverable one.
    pub fn feed(&mut self, bytes: &[u8], batch: &mut EventBatch, now: Instant) -> FeedStats {
        batch.clear();
        self.state.stats = FeedStats {
            bytes: bytes.len(),
            ..FeedStats::default()
        };
        self.state.dispatched = false;
        if bytes.is_empty() {
            return self.state.stats;
        }
        self.state.grid.begin_batch();

        let mut handler = dispatch::Handler {
            state: &mut self.state,
            out: batch,
            now,
        };
        self.parser.advance(&mut handler, bytes);

        self.state.grid.sync_anchors();
        self.prune_selection();
        if self.state.dispatched {
            batch.push_repaint();
        }
        // R-28: the full two-screen walk runs once per feed, never per mutation.
        self.state.grid.assert_integrity(Some(&self.state.interner));
        self.state.stats
    }

    // ── Render hand-off ─────────────────────────────────────────────────────

    /// Phase 1 of the hand-off, under the caller's lock. The three-line shim
    /// `damage-and-render-state.md` names.
    pub fn render_update(&mut self, render: &mut RenderState, now: Instant) -> RenderUpdate {
        let modes = self.mode_snapshot();
        let selection = self.selection_range();
        let view = EngineView {
            grid: &self.state.grid,
            interner: &self.state.interner,
            sync: &mut self.state.sync,
            modes,
            selection,
            generation: self.state.generation,
            palette_epoch: self.state.palette_epoch,
        };
        render.begin_update(view, now)
    }

    // ── Selection ───────────────────────────────────────────────────────────
    //
    // Seven one-line wrappers over `crate::selection`, which is written against
    // `TerminalGrid` because it shipped before this type did. The escape set
    // moves here too: the module takes it as a parameter, `Config` owns it.

    /// Begin a drag, releasing whatever was selected before.
    pub fn selection_start(&mut self, pos: Pos, side: Side, kind: SelectionKind) {
        self.selection_clear();
        self.state.selection = Some(Selection::new(&mut self.state.grid, kind, pos, side));
    }

    /// Move the drag's far end.
    pub fn selection_update(&mut self, pos: Pos, side: Side) {
        if let Some(selection) = &mut self.state.selection {
            selection.update(&mut self.state.grid, pos, side);
        }
    }

    /// The resolved range. `O(1)`, and it never materialises text.
    pub fn selection_range(&self) -> Option<SelectionRange> {
        self.state
            .selection
            .as_ref()?
            .to_range(&self.state.grid, &self.state.config.semantic_escape_chars)
    }

    pub fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }

    /// The selected text, materialised.
    pub fn selection_text(&self) -> Option<String> {
        self.state.selection.as_ref()?.text(
            &self.state.grid,
            &self.state.interner,
            &self.state.config.semantic_escape_chars,
        )
    }

    /// Release both anchors. Idempotent.
    pub fn selection_clear(&mut self) {
        if let Some(selection) = self.state.selection.take() {
            selection.release(&mut self.state.grid);
        }
    }

    pub fn select_all(&mut self) {
        self.selection_clear();
        self.state.selection = Some(Selection::all(&mut self.state.grid));
    }

    /// A pointer position in viewport coordinates to a grid position and the
    /// half of the cell it fell on.
    pub fn hit_test(&self, viewport_row: f32, col: f32) -> (Pos, Side) {
        let viewport = self.viewport();
        let row = (viewport_row.max(0.0) as u64).min(u64::from(viewport.rows.saturating_sub(1)));
        let column = col.max(0.0);
        let last = u32::from(viewport.cols.saturating_sub(1));
        let raw = column as u32;
        let index = raw.min(last) as u16;
        // A drag that ran off the right edge selects the whole last cell, which
        // is the right half of it; inside the grid the half is the fraction.
        let side = if raw > last || column - column.floor() >= 0.5 {
            Side::Right
        } else {
            Side::Left
        };
        (
            Pos {
                row: viewport.top + row,
                col: index,
            },
            side,
        )
    }

    /// The modes the view reads at paint time.
    pub fn mode_snapshot(&self) -> ModeSnapshot {
        ModeSnapshot {
            alt_screen: self.state.grid.alt_active(),
            app_cursor: self.state.modes.contains(Mode::AppCursor),
            app_keypad: self.state.modes.contains(Mode::AppKeypad),
            bracketed_paste: self.state.modes.contains(Mode::BracketedPaste),
            show_cursor: self.state.modes.contains(Mode::ShowCursor),
            insert: self.state.modes.contains(Mode::Insert),
            alternate_scroll: self.state.modes.contains(Mode::AlternateScroll),
            mouse: self.state.modes.mouse_reporting(),
        }
    }

    // ── Geometry ────────────────────────────────────────────────────────────

    /// Resize both screens under one policy (`US-0077`), invalidating every row.
    pub fn resize(&mut self, size: Size, policy: ResizePolicy) -> ResizeOutcome {
        let outcome = self.state.grid.resize(size, policy);
        self.state.generation = self.state.generation.wrapping_add(1);
        self.prune_selection();
        outcome
    }

    /// Give back the anchor entries of a selection whose content is gone.
    ///
    /// A reflow kills selection anchors where it stands (trap 28) and a history
    /// trim kills anything that fell off the oldest end, neither of which can
    /// reach the `Selection` value that owns the entries. It already reads as
    /// "no selection" through [`Terminal::selection_range`]; this is what stops
    /// the two entries leaking across a drag-resize.
    fn prune_selection(&mut self) {
        if self.state.selection.is_some() && self.selection_range().is_none() {
            self.selection_clear();
        }
    }

    /// Cell metrics have one owner (R-40, N-08): the embedder passes them when
    /// the font changes and they answer `CSI 14 t`.
    pub fn set_cell_pixels(&mut self, width: u16, height: u16) {
        self.state.cell_pixels = (width, height);
    }

    pub fn set_scrollback_limit(&mut self, limit: u32) {
        self.state.config.scrollback_limit = limit;
        self.state.grid.set_scrollback_limit(limit);
        self.state.generation = self.state.generation.wrapping_add(1);
    }

    pub fn viewport(&self) -> Viewport {
        self.state.grid.screen().viewport()
    }

    pub fn size(&self) -> Size {
        self.state.grid.screen().size()
    }

    // ── Reading ─────────────────────────────────────────────────────────────

    pub fn grid(&self) -> &TerminalGrid {
        &self.state.grid
    }

    pub fn grid_mut(&mut self) -> &mut TerminalGrid {
        &mut self.state.grid
    }

    pub fn screen(&self) -> &Screen {
        self.state.grid.screen()
    }

    pub fn interner(&self) -> &Interner {
        &self.state.interner
    }

    pub fn config(&self) -> &Config {
        &self.state.config
    }

    /// Output lines, not rows created (R-05): the number the gutter shows.
    pub fn lines_produced(&self) -> u64 {
        self.state.grid.lines_produced()
    }

    pub fn mode(&self, mode: Mode) -> bool {
        match mode {
            Mode::AltScreen | Mode::AltScreen47 | Mode::AltScreen1047 => {
                self.state.grid.alt_active()
            }
            Mode::SyncUpdate => self.state.sync.is_set(),
            other => self.state.modes.contains(other),
        }
    }

    /// One accessor instead of the `MOUSE_MODE` bit union the current code
    /// recombines in seven places.
    pub fn mouse_reporting(&self) -> Option<MouseProtocol> {
        self.state.modes.mouse_reporting()
    }

    /// The live kitty keyboard flags, which can differ from the stack top.
    pub fn keyboard_flags(&self) -> KeyboardFlags {
        self.state.keyboard.active.live()
    }

    pub fn modify_other_keys(&self) -> u8 {
        self.state.modify_other_keys
    }

    /// Shape plus blink. `Hidden` carries `DECTCEM`.
    pub fn cursor_style(&self) -> CursorStyle {
        let style = self
            .state
            .cursor_style
            .unwrap_or(self.state.config.default_cursor_style);
        if self.state.modes.contains(Mode::ShowCursor) {
            style
        } else {
            CursorStyle {
                shape: CursorShape::Hidden,
                blinking: style.blinking,
            }
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.state.title.title.as_deref()
    }

    pub fn title_depth(&self) -> usize {
        self.state.title.depth()
    }

    /// The OSC-override layer only. `None` means "use the theme".
    pub fn color(&self, key: ColorKey) -> Option<crate::cell::Rgb> {
        self.state.colors.get(key)
    }

    pub fn colors(&self) -> &ColorOverrides {
        &self.state.colors
    }

    /// The defaults the engine needs to answer a query.
    pub fn set_theme_colors(&mut self, theme: &ThemeColors) {
        self.state.theme = *theme;
        self.state.palette_epoch = self.state.palette_epoch.wrapping_add(1);
    }

    pub fn stats(&self) -> FeedStats {
        self.state.stats
    }

    pub fn sync(&self) -> &SyncState {
        &self.state.sync
    }

    /// The active `G0..G3` designation, for the parity snapshot and tests.
    pub fn active_charset(&self) -> Charset {
        self.state.grid.screen().cursor().charsets[self.state.active_charset]
    }

    /// One row's text, spacers skipped and graphemes expanded.
    pub fn row_text(&self, id: RowId) -> String {
        let mut out = String::new();
        self.state
            .grid
            .screen_of(id)
            .row_text(id, &self.state.interner.graphemes, &mut out);
        out
    }

    /// The SGR template a printed glyph inherits.
    pub fn template(&self) -> Cell {
        self.state.grid.screen().cursor().template()
    }

    /// The template's resolved style.
    pub fn style(&self) -> Style {
        *self
            .state
            .interner
            .resolve_style(self.template().style_id())
    }
}

#[cfg(test)]
impl Terminal {
    /// Reach past the public surface, so a test can drive a table to its bound
    /// without paying a print per entry.
    pub(crate) fn state_for_tests(&mut self) -> &mut State {
        &mut self.state
    }
}

/// `CARGO_PKG_VERSION` as the DA2 answer encodes it.
#[cfg(test)]
pub(crate) fn dispatch_version_for_tests() -> u32 {
    dispatch::version_number(env!("CARGO_PKG_VERSION"))
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("grid", &self.state.grid)
            .field("modes", &self.state.modes)
            .field("title", &self.state.title.title)
            .finish_non_exhaustive()
    }
}
