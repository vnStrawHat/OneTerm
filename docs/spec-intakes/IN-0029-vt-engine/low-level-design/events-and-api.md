# Low-Level Design: Events and the public API

Intake: IN-0029
HLD: ../high-level-design.md
Topic: events-and-api
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/event.rs`,
> `crates/vt/src/lib.rs` and `crates/vt/src/testing.rs`.

## Concern

The shape of the engine's outward surface: what an event is, how `feed` and drain work, what a
test can assert against, and how the engine behaves when its input is malformed.

Replaces `alacritty_terminal::event::{Event, EventListener}` — a `&self` callback that fires
**during** `Processor::advance`, with the `Term` lock held, which is why
`crates/terminal/src/backend/` needs a two-tier deferred/reliable event sink, a
`flush_reliable_blocking`, a CORR-01 deadlock test, and a `Vec<Vec<u8>>` deep copy per forwarded
OSC (`crates/terminal/src/backend/pump.rs:163-178`,
`crates/terminal/src/backend/osc_router.rs:249-257`).

## Design

### Events are values in a caller-owned batch

```rust
pub struct EventBatch {
    events: Vec<VtEvent>,
    arena: Vec<u8>,          // backs every StrSpan / ByteSpan below; reused across batches
}

pub struct StrSpan  { start: u32, len: u32 }   // UTF-8, validated on insert
pub struct ByteSpan { start: u32, len: u32 }
pub struct ParamSpans { first: u32, count: u16 }   // into a parallel Vec<ByteSpan>

pub enum VtEvent {
    Repaint,                                       // at most one per batch
    Title(StrSpan),
    TitleReset,
    Bell,
    ClipboardStore { selection: ClipboardKind, text: StrSpan },   // already base64-decoded
    ClipboardLoad  { selection: ClipboardKind },                  // embedder formats the reply
    Reply(ByteSpan),                               // DA / DSR / DECRQM / XTVERSION bytes
    ColorQuery { key: ColorKey, terminator: StringTerm },
    ScreenCleared,                                 // ED 2, ED 3, RIS only
    Osc { code: u32, params: ParamSpans, terminator: StringTerm, truncated: bool },
    RowsScrolled { top: RowId, bottom: RowId, delta: i32 },
    RowsTrimmed { oldest: RowId },
    GraphicReleased(GraphicId),   // queued by feed/resize, delivered by feed — including feed(&[])
}
```

Why spans and one arena rather than owned `String` / `Vec<u8>` per event: the fork allocates one
`Vec` per OSC parameter plus an outer `Vec` **on the hot path**, purely so the event can cross a
channel, and the consumer immediately re-borrows them as `&[&[u8]]`. One arena, cleared per
batch, makes the steady state allocation-free while keeping the batch owned and `Send`.

```rust
impl EventBatch {
    pub fn new() -> Self;
    pub fn clear(&mut self);                                  // called at the top of feed()
    pub fn iter(&self) -> impl Iterator<Item = &VtEvent>;
    pub fn str(&self, s: StrSpan) -> &str;
    pub fn bytes(&self, b: ByteSpan) -> &[u8];
    pub fn params(&self, p: ParamSpans) -> impl Iterator<Item = &[u8]>;
    pub fn is_empty(&self) -> bool;
}
```

### The `Terminal` struct and the field split (R-32)

`feed` needs `&mut Parser` and a `Handler` borrowing everything else at the same time, so the
owning struct and its split are part of the design, not an implementation detail:

```rust
pub struct Terminal {
    parser:   Parser,
    screens:  Screens,          // primary, alternate, and which is active
    intern:   Interner,         // styles, extras, graphemes, hyperlinks (one per terminal)
    anchors:  Anchors,
    modes:    Modes,
    colors:   ColorOverrides,
    title:    TitleState,
    keyboard: KeyboardStacks,
    graphics: GraphicsState,    // placements + the pending pixel queue
    sync:     SyncState,
    config:   Config,
    seq:      SeqNo,
    lines_produced: u64,
    generation: u32,
    stats:    FeedStats,
}

impl Terminal {
    pub fn feed(&mut self, bytes: &[u8], batch: &mut EventBatch, now: Instant) -> FeedStats {
        let Terminal { parser, screens, intern, anchors, modes, colors, title,
                       keyboard, graphics, sync, config, seq, lines_produced,
                       stats, .. } = self;                 // disjoint borrows
        let mut handler = Handler { screens, intern, anchors, modes, colors, title,
                                    keyboard, graphics, sync, config, seq, lines_produced,
                                    stats, out: batch, now };
        parser.advance(&mut handler, bytes);
        // end of batch: grapheme sweep if needed, graphics release sweep, Repaint
        ...
    }
}
```

### `feed` and drain

```rust
pub struct FeedStats {
    pub bytes: usize,
    pub rows_scrolled: u32,
    pub malformed_sequences: u32,
    pub truncated_osc: u32,
    pub aborted_dcs: u32,
    pub grapheme_truncated: u32,
    pub unhandled_sequences: u32,
    pub style_table_exhausted: u32,
}

impl Terminal {
    pub fn feed(&mut self, bytes: &[u8], batch: &mut EventBatch, now: Instant) -> FeedStats;
}
```

Contract, and every clause is a test:

1. `feed` clears `batch` first. A caller who has not drained the previous batch loses it; that is
   a programming error and a debug assertion catches it.
   **The rule, and who must satisfy it: `US-0079` owns `EventBatch`, so `US-0079` owns the
   assertion.** It cannot be a `&self` check — detecting "not drained" from an accessor would need
   interior mutability the engine forbids — so it is a **generation counter on `EventBatch`**:
   `feed` bumps it, every accessor (`iter`, `str`, `bytes`, `params`) records the generation it
   read, and `feed` debug-asserts that the previous batch was read at its own generation before
   clearing it. `US-0076` could not implement this, because the type is not its own; it is listed
   here so the obligation does not evaporate between packets.
2. `feed` **never blocks, never calls back, never allocates in the steady state** (the arena and
   the event vector grow to a high-water mark and stay).
3. Parser state, the OSC accumulator and the UTF-8 carry survive between calls, so an arbitrary
   chunking of the same stream produces the same grid and the same event sequence. (There is no
   sequence echo: R-35 cut it, N-09.)
4. Events are in byte order. `Repaint` is appended at most once, at the end, when anything changed.
   Ordering among the rest is the order the bytes produced them — which is what
   `docs/osc-sequences-checklist.md` promises for multiple OSCs in one read batch.
   **`Repaint` is the engine's statement that something changed, not the embedder's repaint hint.**
   The adapter does **not** forward it: the pump posts exactly one `SessionEvent::Output` per batch,
   *after* that batch's reliable events have been flushed. Forwarding `Repaint` as well produces a
   second, earlier hint that arrives mid-drain, doubling the hints under load and inverting the
   order `docs/terminal-backend.md` promises ([`migration.md`](migration.md) § "The event-order
   rule").
5. `feed` never panics on input. Every malformed case increments a `FeedStats` counter and
   continues.
6. The engine holds no lock and spawns no thread. The caller's lock discipline is the caller's
   (`../high-level-design.md`, "Threading and locking").

The adapter's loop becomes:

```rust
let stats = { let mut t = term.lock(); t.feed(chunk, &mut batch) };   // guard dropped here
for event in batch.iter() { router.handle(event, &batch); }           // nothing under the lock
```

which deletes the deferred tier, `flush_reliable_blocking`, and the reason the current
`SessionEventSink` exists at all.

### What is deliberately not an event

- **Damage.** It is a per-row sequence number read through a watermark
  ([`damage-and-render-state.md`](damage-and-render-state.md)), not a stream. `Repaint` is only
  a hint that something changed at all.
- **Mode changes.** `VtEvent::ModeChanged` and `Config::mode_watch` are **deleted** (R-54): no
  consumer was ever named for them, and the view reads modes from `ModeSnapshot` in the render
  state instead ([`damage-and-render-state.md`](damage-and-render-state.md), R-17).
- **Cursor movement and selection changes.** `render_update` refreshes both every call. The fork
  emits `MouseCursorDirty`, `CursorBlinkingChange` and `TextAreaSizeRequest`, and OneTerm ignores
  all three (`crates/terminal/src/backend/osc_router.rs:264-266`).
- **Passthrough of unhandled sequences.** Cut in v1 (R-35, [`parser.md`](parser.md)); they are
  counted in `FeedStats::unhandled_sequences`.
- **Child exit.** That is the transport's business, not the engine's
  ([`pty.md`](pty.md)). The fork's `Event::ChildExit` is emitted only by alacritty's own event
  loop, which OneTerm does not use.

### The `Config`

```rust
pub struct Config {
    pub scrollback_limit: u32,            // default 10_000, clamped to SCROLLBACK_MAX
    pub osc_claims: OscClaims,            // which OSC numbers reach the embedder, and which spill
    pub default_cursor_style: CursorStyle,
    pub semantic_escape_chars: String,    // for word selection (selection.md)
    pub accept_c1: bool,                  // default false; the S8C1T hook
}

// Cell pixel metrics have ONE owner (R-40): the embedder passes them when the font changes,
// they answer `CSI 14 t`, and `oneterm-pty`'s WindowSize carries the same pair to the
// pseudo-console. They are not duplicated in Config.
impl Terminal { pub fn set_cell_pixels(&mut self, w: u16, h: u16); }
```

No dead knobs. The fork's `Config` carries `vi_mode_cursor_style`, `kitty_keyboard` and `osc52`,
which OneTerm never sets ([`../research/api-surface.md`](../research/api-surface.md) § 8), and
the kitty flag in particular gates a protocol behind a boolean rather than simply exposing the
state.

### Test support

Not `#[cfg(test)]`-gated, so downstream crates use it from their own tests without a feature
flag — the property `alacritty_terminal::term::test` has today and that
`crates/terminal/src/content.rs`, `model.rs`, `search.rs` and `sixel_tests.rs` all rely on
([`../research/api-surface.md`](../research/api-surface.md) § 6.1).

```rust
// crates/vt/src/testing.rs  (pub mod testing, not cfg-gated)
pub fn terminal_from_text(text: &str) -> Terminal;          // mock_term contract
pub fn terminal_from_rows(rows: &[(&str, bool)]) -> Terminal; // explicit wrap flags (R-62)
pub fn feed(term: &mut Terminal, bytes: &[u8]) -> EventBatch;
pub fn row_text(term: &Terminal, id: RowId) -> String;
pub fn viewport_text(term: &Terminal) -> String;          // rows joined with '\n'
pub fn cell_at(term: &Terminal, row: u16, col: u16) -> Cell;
```

**Two builders (R-62).** `terminal_from_text` reproduces `mock_term`'s contract exactly, because
eleven existing tests depend on the details — including that `
` marks the previous row
`WRAPPED`, which under deviation G1 makes the whole fixture **one logical line** that any reflow
would rejoin. Reflow tests therefore use `terminal_from_rows(&[(text, wrapped)])`, which takes the
wrap flag explicitly. The contract of the first builder: the grid is sized to the content (columns = the widest line by **display
width**, rows = the line count); `\n` breaks the line **and** marks the previous row `WRAPPED`;
`\r\n` breaks without wrapping; a wide character gets its `WideSpacer`; the cursor is visible;
and the first render update is `Full`.

`snapshot_text()` on `Terminal` is the deterministic dump used in assertions and in the
differential runner: viewport rows, then a marker, then the cursor position, modes that are
non-default, and the style of each distinct run. It is a debugging and comparison aid, not a
serialization format, and it is explicitly **not** stable across versions.

### Error policy

Applied per `docs/agents/error-policy.md`:

| Class | Behaviour here |
| --- | --- |
| Terminal input (untrusted bytes) | Never an error and never a panic. Malformed sequences are dropped or truncated, the corresponding `FeedStats` counter increments, and the embedder logs at `debug` when a counter moves — the "optional telemetry" row. A single `log::warn!` per session is allowed for the style-table exhaustion ladder, because it indicates a pathological stream. |
| Caller misuse | Debug assertion, never a release panic: feeding an undrained batch, reusing a `RenderState` across terminals, passing a zero dimension. Release builds clamp or reset and carry on. |
| Resource exhaustion | Bounded by construction (`../high-level-design.md`, "Memory model and caps"). Every cap truncates or degrades; none returns an error the caller has to handle, because there is no useful recovery for "the remote sent an 8 MiB title". |
| Invariant violation | `assert_integrity()` in debug builds, **once per `feed` / `resize` / `render_update`, not per mutation** (R-28). The full two-screen walk is behind the `vt-paranoid` feature used by the property tests and the fuzz targets; the always-on check is O(1) (counters, ranges, the active screen's cursor). Budget in [`testing-and-bench.md`](testing-and-bench.md). |

No public method returns `Result`. The engine has no I/O, no allocation the caller can handle
failing, and no configuration that can be invalid at run time; a `Result` on `feed` would be an
error nobody could act on. `Config` values that could be nonsensical are clamped at
construction, which is `docs/agents/code-style.md`'s "constructors return fully initialized,
valid objects".

## Interfaces

The full public surface of `oneterm-vt`, in one place, so review can see how small it is:

```rust
pub use cell::{Cell, CellContent, CellWidth, Color, NamedColor, Style, Attrs, Rgb};
pub use event::{EventBatch, VtEvent, FeedStats, ClipboardKind, StringTerm, ByteSpan, StrSpan};
pub use grid::{RowId, Pos, Size, Viewport, RowRef};
pub use graphics::{GraphicId, GraphicData, Placement, VIRTUAL_CELL, MAX_DIMENSION};
pub use mode::{Mode, ModeSnapshot, MouseProtocol, CursorStyle, CursorShape, KeyboardFlags};
pub use osc::{OscClaims, ColorKey, ThemeColors};
pub use reflow::{ResizePolicy, ResizeOutcome};
pub use render::{RenderState, RenderUpdate, RenderRow, RenderCell, StyleRun, RenderCursor};
pub use selection::{SelectionKind, SelectionRange, Side};
pub use anchor::{AnchorId, AnchorKind};
pub use terminal::{Terminal, Config};
pub use damage::SeqNo;
pub mod strip;      // escape-sequence stripper for session logging
pub mod testing;    // not cfg-gated
```

Everything else is `pub(crate)`, including the whole `parser` and `dispatch` modules.

## Edge Cases and Failure Modes

- [ ] **A batch not drained before the next `feed`** — debug assertion; release clears and
  continues.
- [ ] **A batch drained by a consumer that keeps a span** — spans are indices, so a stale span
  read after the next `feed` yields wrong bytes rather than unsafety. `EventBatch` is not
  `Copy`, `VtEvent` is not `Clone`, and the accessors take `&self`, so the borrow checker
  prevents holding a span across a `feed` in practice; the remaining case is a consumer that
  stores the raw `u32`s, which a debug generation counter on the batch catches.
- [ ] **An enormous single event** (an 8 MiB OSC 52 payload) grows the arena once; the arena
  shrinks back after a batch that exceeded `EVENT_ARENA_SOFT = 1 MiB`.
- [ ] **`feed` with an empty slice** returns zeroed stats and appends no *parse* events — but it
  **does** deliver anything a previous `resize` queued, which today means `GraphicReleased`. It is
  therefore not a no-op, and the adapter must issue one after every resize
  ([`graphics.md`](graphics.md)).
- [ ] **A sequence split across `feed` calls** produces exactly one event, at the call that
  completes it.
- [ ] **A reply generated while the transport is closed** is still emitted; discarding it is the
  embedder's decision (error-policy, transport-closure row).
- [ ] **Non-UTF-8 in a `StrSpan`** cannot happen: `Title` and `ClipboardStore` are validated at
  insert and dropped if invalid, matching the fork's behaviour for OSC 52 (trap 25).
- [ ] **`Terminal` moved between threads** is fine (`Send`); shared between them is not
  (`!Sync`), which is enforced by the type rather than by documentation.

## Verification

`cargo test -p oneterm-vt event::` and `api::`

- [ ] `event::tests::feed_clears_the_batch_and_returns_stats`
- [ ] `event::tests::events_are_in_byte_order` — interleaved OSC 7, a bell, OSC 133 and a title
  in one chunk.
- [ ] `event::tests::repaint_appears_at_most_once_per_batch`
- [ ] `event::tests::repaint_is_absent_when_nothing_changed`
- [ ] `event::tests::chunking_is_invariant` — the same stream fed in 1-, 7-, 64 KiB chunks
  produces identical grids, identical event sequences and identical stats.
- [ ] `event::tests::steady_state_feed_makes_no_allocation` — a counting allocator over 1 000
  chunks with OSC, SGR, graphics and scrolling traffic.
- [ ] `event::tests::osc_params_are_spans_not_vectors` — asserts zero allocations for a claimed
  OSC after warm-up.
- [ ] `event::tests::large_osc_grows_then_shrinks_the_arena`
- [ ] `event::tests::undrained_batch_asserts_in_debug`
- [ ] `event::tests::feed_never_panics_on_fuzz_corpus` — the whole seed corpus through `feed`.
- [ ] `event::tests::malformed_input_moves_the_right_counter` — one case per `FeedStats` field.
- [ ] `event::tests::rows_scrolled_is_emitted_for_in_region_motion` — R-02; the notification that
  lets a consumer shift its own cache.
- [ ] `api::tests::public_surface_is_send_not_sync` — a compile-fail test
  (`trybuild` is not added; a `static_assertions`-style const check plus a `#[test]` that
  spawns a thread with the engine moved in).
- [ ] `testing::tests::terminal_from_text_matches_the_mock_term_contract` — sizing by display
  width, `\n` versus `\r\n` wrap marking, wide-char spacers, cursor visible, first update
  `Full`. These are the details the eleven ported tests depend on.
- [ ] `testing::tests::snapshot_text_is_deterministic`

Cross-crate, at `US-0082`: `crates/terminal/src/backend/backend_tests.rs` (about 25 tests)
keeps its coverage — every event kind reaching the right `SessionEvent`, colour-query deferral,
and ordering — but loses the deferred-flush and deadlock cases, because the batch model makes
them unreachable. Each removed test is named in the packet with the reason
([`migration.md`](migration.md)).
