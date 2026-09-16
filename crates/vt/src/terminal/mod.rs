//! `Terminal`: parser + dispatch + grid + the snapshot hand-off.
//!
//! This is the engine's one public object. It holds no lock, spawns no thread,
//! returns no `Result` and never panics on input: a malformed or hostile stream
//! is dropped, truncated or degraded to a documented fallback, and counted in
//! [`FeedStats`].
//!
//! Design: <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md>.

// The field split is one indirection rather than fifteen borrows: `Terminal`
// owns the `Parser` and a `State`, and `feed` builds a
// `Handler { state, out, now }` over the second while the first drives it.

mod color;
mod dispatch;
mod mode;
mod osc;
mod query;

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dcs_routing_tests.rs"]
mod dcs_routing_tests;

#[cfg(test)]
#[path = "query_tests.rs"]
mod query_tests;

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

pub use color::ColorKey;
pub(crate) use color::ColorOverrides;
pub(crate) use dispatch::ClusterCarry;
pub use mode::{CursorShape, CursorStyle, KeyboardFlags, Mode};
pub(crate) use mode::{KeyboardStacks, Modes, TitleState};
pub use osc::{OscRoute, OscRoutes};

use crate::cell::{Cell, Style};
use crate::event::{EventBatch, FeedStats};
use crate::graphics::{self, GraphicData, GraphicsState};
use crate::grid::{
    AnchorId, Charset, DEFAULT_SCROLLBACK, Pos, RowId, Screen, Size, TerminalGrid, Viewport,
};
use crate::input::{KeyEvent, KeyMods, KeySpec};
use crate::intern::Interner;
use crate::parser::Parser;
use crate::reflow::{ResizeOutcome, ResizePolicy};
use crate::selection::{Selection, SelectionKind, SelectionRange, Side};
use crate::snapshot::{
    EngineView, ModeSnapshot, MouseProtocol, Palette, SnapshotState, SnapshotUpdate, SyncState,
};

/// The theme the engine needs to answer a colour query. The renderer still
/// resolves final colours itself, including bold-to-bright and dim mixing.
pub(crate) type ThemeColors = Palette;

/// Tracked `OSC 133` marks. Bounded because a mark per prompt from a hostile
/// stream is otherwise unbounded anchor growth; the oldest is released.
const MARK_MAX: usize = 1024;

/// Everything a terminal is configured with.
///
/// Build one with [`Config::default`] and assign the fields you care about.
/// Every knob here is read by the engine; one it could not honour is not
/// offered.
// No dead knobs: the fork's `vi_mode_cursor_style`, `kitty_keyboard` and
// `osc52` are gone, the last because the engine never applies a clipboard
// policy.
//
// **Deliberately not `#[non_exhaustive]`**, against the accepted IN-0038
// design, which assumed `Config { ..Config::default() }` would keep working
// under the mark. It does not: Rust forbids a struct expression for a
// non-exhaustive struct outside the defining crate, functional-update syntax
// included (E0639), so the mark would break every embedder, this crate's own
// integration tests, the `headless` example and six guide doctests at once.
// A new field here is therefore a minor version bump under clause 1 of guide
// chapter 12 rather than a patch. Recorded in `US-0106`.
#[derive(Clone, Debug)]
pub struct Config {
    /// Rows of scrollback to keep above the screen. The grid clamps what it
    /// actually keeps to [`SCROLLBACK_MAX`](crate::grid::SCROLLBACK_MAX); this
    /// field reports the number you asked for.
    pub scrollback_limit: u32,
    /// Which OSC numbers the engine handles, forwards, or ignores, and which
    /// may spill past [`crate::parser::OSC_INLINE`].
    pub osc_routes: OscRoutes,
    /// The cursor shape and blink a fresh terminal starts with, before the
    /// stream picks one with `CSI Ps SP q`.
    pub default_cursor_style: CursorStyle,
    /// The characters that end a word, for word and line selection.
    pub semantic_escape_chars: String,
    /// What the terminal calls itself in `XTVERSION` (`CSI > 0 q`) and `DA2`
    /// (`CSI > c`).
    ///
    /// `None` answers with the engine's own identity, `oneterm-vt(<version>)`.
    /// An embedder shipping a product should set this, because programs such as
    /// tmux and vim key capability detection off the `XTVERSION` string.
    ///
    /// A trailing `(<major>.<minor>.<patch>)` is also the version `DA2`
    /// reports; with no parsable version there, `DA2` reports the engine's own.
    /// `DA2` answers one number, `major * 10000 + minor * 100 + patch`, so each
    /// component saturates at 99: `1.0.100` reports what `1.0.99` reports.
    ///
    /// **The value is sanitised by [`Terminal::new`].** `XTVERSION` replies
    /// inside a DCS string, so every C0 control, `DEL` and every C1 control
    /// (`0x00..=0x1f`, `0x7f`, `0x80..=0x9f`) is dropped rather than allowed to
    /// end that string early, and what is left is cut to **64 bytes** on a
    /// character boundary. A name that is empty, or that sanitises to nothing,
    /// is stored as `None`. [`Terminal::config`] therefore reports the
    /// sanitised name, not the one you passed.
    pub product_name: Option<Cow<'static, str>>,
    /// Whether `DECRQCRA` (`CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y`) may answer.
    /// **Default `false`.**
    ///
    /// The sequence reports a checksum of a rectangle of the screen. That is
    /// how a conformance harness reads the screen back — and how a program
    /// running inside the terminal could read back text it did not write, from
    /// a password prompt to another program's output. xterm gates it behind
    /// `allowWindowOps` and WezTerm behind `enable_checksum_rectangular_area`;
    /// this is the same gate, shut by default.
    ///
    /// With this `false` the sequence answers nothing and is counted in
    /// [`FeedStats::unhandled_sequences`](crate::FeedStats::unhandled_sequences),
    /// which is byte for byte what the engine did before it was implemented at
    /// all.
    ///
    /// The checksum covers the **visible screen** only, never the scrollback,
    /// so even with the gate open a program cannot read scrolled-off history
    /// with it.
    pub allow_screen_readback: bool,
    // `accept_c1` (the `S8C1T` hook) is deliberately **absent**. The LLD
    // publishes it, but the parser hard-codes trap 48 — an 8-bit C1 byte is
    // executed, never treated as an introducer — so the field would be a knob
    // that silently does nothing, which is exactly the dead-configuration
    // problem that `Config` section sets out to avoid. It comes back with the
    // parser change that honours it (`US-0076` verification, M6).
}

impl Default for Config {
    fn default() -> Config {
        Config {
            scrollback_limit: DEFAULT_SCROLLBACK,
            osc_routes: OscRoutes::new(),
            default_cursor_style: CursorStyle::default(),
            semantic_escape_chars: crate::selection::SEMANTIC_ESCAPE_CHARS.to_owned(),
            product_name: None,
            allow_screen_readback: false,
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
    /// Decoded images, their placements and the in-flight Sixel decoder.
    /// Owns one anchor entry per live placement.
    pub(crate) graphics: GraphicsState,
    /// The `DCS $ q` (`DECRQSS`) or `DCS + q` (`XTGETTCAP`) currently being
    /// received, if any. Never set at the same time as `graphics.parser`: a
    /// DCS cannot nest, so `dcs_hook` opens exactly one sink.
    pub(crate) dcs_query: Option<query::DcsQuery>,
    /// That query's payload so far. `clear()`ed rather than dropped, so a
    /// program polling `XTGETTCAP` in a loop allocates once.
    ///
    /// Bounded by `query::QUERY_MAX_BYTES` (8 KiB), **not** by the parser's
    /// `DCS_MAX_BYTES` (16 MiB): keeping the capacity is what makes the reuse
    /// work, so inheriting the image ceiling would let one hostile `DCS + q`
    /// retain 16 MiB for the session. Reaching the query ceiling answers
    /// nothing and moves `FeedStats::unhandled_sequences`; it is **not** an
    /// abort, so `aborted_dcs` does not move. `DCS_MAX_BYTES` and `aborted_dcs`
    /// still apply above it, unchanged.
    pub(crate) dcs_payload: Vec<u8>,
    /// Owns two entries in the anchor list while it lives, which is why every
    /// path that drops it goes through `Terminal::selection_clear`.
    pub(crate) selection: Option<Selection>,
    pub(crate) config: Config,
    pub(crate) theme: ThemeColors,
    pub(crate) cursor_style: Option<CursorStyle>,
    /// Which of `G0..G3` `SI` / `SO` and the locking shifts selected. Lives on
    /// the terminal, not the cursor, so `DECSC` / `DECRC` do not save it —
    /// reference behaviour.
    pub(crate) active_charset: usize,
    /// `SS2` / `SS3`: the set the **next printed character** comes from, and
    /// only that one. Like `preceding_char` it survives an intervening escape
    /// sequence, because only printing consumes it.
    pub(crate) single_shift: Option<usize>,
    /// What `DECSC` saved of the two above, one slot per screen because the
    /// saved cursor is per screen. Correction C12: VT510's `DECSC` saves the
    /// sets in GL and GR and any pending single shift, and the engine being
    /// replaced saves neither.
    pub(crate) saved_shifts: [(usize, Option<usize>); 2],
    /// Mode `? 2027` only: the grapheme cluster the last printed run ended on,
    /// so a cluster split across two `feed` calls is measured whole. `None`
    /// whenever the mode is reset, and cleared by any dispatch that is not a
    /// print.
    pub(crate) cluster_carry: Option<ClusterCarry>,
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
    /// A terminal at `size`, in its power-on state. The size is clamped for
    /// you by [`Size::clamped`], so an absurd one is not an error.
    pub fn new(size: Size, config: Config) -> Terminal {
        let size = size.clamped();
        // The product name is sanitised once, here, rather than on every
        // `XTVERSION` and `DA2`: the replies then read a value that is already
        // safe to splice into a DCS string, the two can never disagree, and
        // neither walks the embedder's string again. A name that sanitises to
        // nothing is stored as `None`, which is what it means.
        let mut config = config;
        config.product_name = config
            .product_name
            .as_deref()
            .map(dispatch::sanitize_product_name)
            .filter(|name| !name.is_empty())
            .map(Cow::Owned);
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
                graphics: GraphicsState::default(),
                dcs_query: None,
                dcs_payload: Vec::new(),
                selection: None,
                theme: ThemeColors::new(),
                cursor_style: None,
                active_charset: 0,
                single_shift: None,
                saved_shifts: [(0, None); 2],
                cluster_carry: None,
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
            // Still the delivery point: a `resize` releases placements and has
            // no batch of its own to report them in.
            graphics::drain_released(&mut self.state, batch);
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
        // R-22: the release sweep is end-of-batch, never per mutation, because
        // liveness is derived from the rows the batch left behind.
        graphics::sweep(&mut self.state);
        graphics::drain_released(&mut self.state, batch);
        if self.state.dispatched {
            batch.push_repaint();
        }
        // R-28: the full two-screen walk runs once per feed, never per mutation.
        self.state.grid.assert_integrity(Some(&self.state.interner));
        graphics::assert_integrity(&self.state);
        self.state.stats
    }

    // ── Graphics ────────────────────────────────────────────────────────────

    /// Take the images decoded since the last call, oldest first.
    ///
    /// This is the only drain. More than one consumer may hold its own
    /// [`SnapshotState`], so draining inside `snapshot_update` would hand an image
    /// to whichever consumer asked first and nothing to the rest. A paint
    /// skipped by synchronised output (`CSI ? 2026 h`) therefore loses nothing:
    /// the pixels wait here until somebody takes them.
    pub fn take_graphics(&mut self) -> Vec<Arc<GraphicData>> {
        std::mem::take(&mut self.state.graphics.pending)
    }

    /// Every live placement, for a consumer that is not going through a
    /// [`SnapshotState`]. The painter reads `SnapshotState::placements` instead.
    pub fn placements(&self) -> &[crate::graphics::Placement] {
        &self.state.graphics.placements
    }

    // ── Snapshot hand-off ───────────────────────────────────────────────────

    /// Take everything that changed since this [`SnapshotState`] last asked.
    ///
    /// Phase 1 of the hand-off, cheap enough to run under the caller's lock;
    /// the returned [`SnapshotUpdate`] borrows the engine, so drawing happens
    /// after it is dropped.
    pub fn snapshot_update(
        &mut self,
        snapshot: &mut SnapshotState,
        now: Instant,
    ) -> SnapshotUpdate {
        let modes = self.mode_snapshot();
        let selection = self.selection_range();
        let view = EngineView {
            grid: &self.state.grid,
            interner: &self.state.interner,
            sync: &mut self.state.sync,
            modes,
            selection,
            placements: &self.state.graphics.placements,
            generation: self.state.generation,
            palette_epoch: self.state.palette_epoch,
        };
        snapshot.begin_update(view, now)
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

    /// Whether anything is selected right now.
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

    /// Select the whole grid, scrollback included.
    pub fn select_all(&mut self) {
        self.selection_clear();
        self.state.selection = Some(Selection::all(&mut self.state.grid));
    }

    /// A pointer position in viewport coordinates to a grid position and the
    /// half of the cell it fell on.
    ///
    /// Off the right edge, off the bottom, and on a wide glyph's second half,
    /// this answers exactly what a drag started at the same point would select:
    /// there is one implementation of the rule, not two.
    pub fn hit_test(&self, viewport_row: f32, col: f32) -> (Pos, Side) {
        crate::selection::hit_test(&self.state.grid, viewport_row, col)
    }

    /// The bytes a key press sends to this terminal.
    ///
    /// [`crate::input::encode_key`] with this terminal's own modes, so the
    /// embedder does not have to fetch a snapshot to encode one key. `None`
    /// means the chord has no terminal encoding and the event is dropped.
    pub fn encode_key(&self, key: &KeySpec, mods: KeyMods) -> Option<Vec<u8>> {
        crate::input::encode_key(key, mods, &self.mode_snapshot())
    }

    /// The bytes a whole key event sends to this terminal.
    ///
    /// [`crate::input::encode_key_event`] with this terminal's own modes, so a
    /// program that negotiated the kitty keyboard protocol with *this* terminal
    /// gets the encoding it asked for. `None` means the event sends nothing --
    /// a release with no event-type reporting, or a chord with no encoding at
    /// all -- and the embedder drops it.
    pub fn encode_key_event(&self, event: &KeyEvent) -> Option<Vec<u8>> {
        crate::input::encode_key_event(event, &self.mode_snapshot())
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
            reverse_video: self.state.modes.contains(Mode::ReverseVideo),
            mouse: self.state.modes.mouse_reporting(),
            keyboard_flags: self.state.keyboard.active.live(),
            modify_other_keys: self.state.modify_other_keys,
        }
    }

    // ── Geometry ────────────────────────────────────────────────────────────

    /// Resize both screens under one policy, invalidating every row.
    pub fn resize(&mut self, size: Size, policy: ResizePolicy) -> ResizeOutcome {
        let outcome = self.state.grid.resize(size, policy);
        self.state.generation = self.state.generation.wrapping_add(1);
        // A reflow moves cells between rows, so a half-printed cluster's cell
        // is no longer where it was; the continuation starts a new one.
        self.state.cluster_carry = None;
        self.prune_selection();
        // A reflow can destroy the rows a placement was anchored to; the
        // releases queue until the next `feed` delivers them.
        graphics::sweep(&mut self.state);
        outcome
    }

    /// Give back the anchor entries of a selection whose content is gone.
    ///
    /// A reflow kills selection anchors where it stands and a history trim
    /// kills anything that fell off the oldest end, neither of which can reach
    /// the `Selection` value that owns the entries. It already reads as
    /// "no selection" through [`Terminal::selection_range`]; this is what stops
    /// the two entries leaking across a drag-resize.
    fn prune_selection(&mut self) {
        if self.state.selection.is_some() && self.selection_range().is_none() {
            self.selection_clear();
        }
    }

    /// The cell's size in pixels, which only the embedder knows. Pass it
    /// whenever the font changes; it is what `CSI 14 t` reports.
    pub fn set_cell_pixels(&mut self, width: u16, height: u16) {
        self.state.cell_pixels = (width, height);
    }

    /// Change the scrollback ceiling; trims history at once and invalidates every row.
    pub fn set_scrollback_limit(&mut self, limit: u32) {
        self.state.config.scrollback_limit = limit;
        self.state.grid.set_scrollback_limit(limit);
        self.state.generation = self.state.generation.wrapping_add(1);
    }

    /// Which rows the screen is currently showing, after any scrollback scroll.
    pub fn viewport(&self) -> Viewport {
        self.state.grid.screen().viewport()
    }

    /// The screen's size in cells.
    pub fn size(&self) -> Size {
        self.state.grid.screen().size()
    }

    // ── Reading ─────────────────────────────────────────────────────────────

    /// Both screens, for a consumer that reads the grid directly.
    pub fn grid(&self) -> &TerminalGrid {
        &self.state.grid
    }

    /// Both screens, mutably. Writing here bypasses dispatch; tests are the intended caller.
    pub fn grid_mut(&mut self) -> &mut TerminalGrid {
        &mut self.state.grid
    }

    /// The active screen: the alternate one while it is up, otherwise the primary.
    pub fn screen(&self) -> &Screen {
        self.state.grid.screen()
    }

    /// The table that resolves the style, grapheme and hyperlink ids a `Cell` carries.
    pub fn interner(&self) -> &Interner {
        &self.state.interner
    }

    /// The interner, mutably. **Not a supported entry point.**
    ///
    /// The only hook an embedder's **test** has for writing a styled cell
    /// straight into the grid through [`Terminal::grid_mut`] instead of driving
    /// an SGR stream: the style and extras ids a `Cell` carries are meaningless
    /// without the table that minted them.
    ///
    /// `#[doc(hidden)]` because handing a consumer mutable access to the intern
    /// tables is not something this crate offers: an id minted outside the
    /// engine's own write paths has no integrity check behind it. A caller that
    /// is not a test wants a real API instead.
    #[doc(hidden)]
    pub fn interner_mut(&mut self) -> &mut Interner {
        &mut self.state.interner
    }

    /// The configuration this terminal was built with.
    pub fn config(&self) -> &Config {
        &self.state.config
    }

    /// Output lines, not grid rows created: a wrapped line counts once, which
    /// is the number a gutter shows.
    pub fn lines_produced(&self) -> u64 {
        self.state.grid.lines_produced()
    }

    /// Whether one mode is set.
    pub fn mode(&self, mode: Mode) -> bool {
        match mode {
            Mode::AltScreen | Mode::AltScreen47 | Mode::AltScreen1047 => {
                self.state.grid.alt_active()
            }
            Mode::SyncUpdate => self.state.sync.is_set(),
            other => self.state.modes.contains(other),
        }
    }

    /// Which mouse protocol the stream turned on, if any.
    pub fn mouse_reporting(&self) -> Option<MouseProtocol> {
        self.state.modes.mouse_reporting()
    }

    /// The live kitty keyboard flags, which can differ from the stack top.
    pub fn keyboard_flags(&self) -> KeyboardFlags {
        self.state.keyboard.active.live()
    }

    /// The `CSI > 4 ; Ps m` level the stream asked for: `0`, `1` or `2`.
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

    /// The window title last set with `OSC 0` or `OSC 2`.
    pub fn title(&self) -> Option<&str> {
        self.state.title.title.as_deref()
    }

    /// How many titles are on the `CSI 22 t` save stack.
    pub fn title_depth(&self) -> usize {
        self.state.title.depth()
    }

    /// The OSC-override layer only. `None` means "use the theme".
    pub fn color(&self, key: ColorKey) -> Option<crate::cell::Rgb> {
        self.state.colors.get(key)
    }

    /// Every colour the stream overrode, keyed by [`ColorKey`]: the 256 indexed
    /// colours (`OSC 4`), the defaults (`OSC 10`, `OSC 11`), the cursor
    /// (`OSC 12`), and the bright and dim variants the engine derives.
    pub fn colors(&self) -> &ColorOverrides {
        &self.state.colors
    }

    /// The defaults the engine needs to answer a query.
    pub fn set_theme_colors(&mut self, theme: &ThemeColors) {
        self.state.theme = *theme;
        self.state.palette_epoch = self.state.palette_epoch.wrapping_add(1);
    }

    /// The counters as they stand, without feeding anything.
    pub fn stats(&self) -> FeedStats {
        self.state.stats
    }

    /// Synchronised output (`CSI ? 2026 h`) state, including its timeout.
    pub fn sync(&self) -> &SyncState {
        &self.state.sync
    }

    /// Which of `G0..G3` is currently mapped, as `SI`, `SO` and the locking
    /// shifts left it.
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
