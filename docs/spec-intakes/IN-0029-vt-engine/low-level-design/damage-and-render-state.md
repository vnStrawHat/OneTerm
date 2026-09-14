# Low-Level Design: Damage and render state

Intake: IN-0029
HLD: ../high-level-design.md
Topic: damage-and-render-state
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/damage.rs` and
> `crates/vt/src/render.rs`.

## Concern

How the engine records what changed, and how a frame's worth of rows reaches the renderer without
a viewport copy and without holding the lock through paint.

Replaces `Term::damage()` + `Term::reset_damage()` (viewport-only, single-consumer, escalating to
`Full` on any scroll) and `TerminalContent::refill`
(`crates/terminal/src/content.rs:173-222`), which clones every visible cell once per painted
frame. The contract is fixed by
[`DEC-0015`](../../../decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md).

The current per-frame copy costs 29.9 us at 200x50, 0.18 % of a 60 Hz budget
([`../research/perf-baseline.md`](../research/perf-baseline.md) § 4), so this is a change of
shape, not of speed: it scales with change rather than with viewport area, and it removes the
reason `query_line_range_cells` had to exist instead of a general snapshot.

The premise binds the consumers too, not just `render_update`. `US-0092` (`IN-0032`) brought the
last two viewport-area loops above this layer into line: the view's URL mask rescans the changed
rows closed under their wrap runs rather than the whole viewport
(`crates/terminal-view/src/render/plan_cache.rs`), and `last_content_row` skips a row the engine's
`RowHeader.occ` hint says was never written (`crates/terminal/src/content.rs`). The URL pass is
therefore **not** documented as deliberately whole-viewport anywhere; a future change that widens
either scan back to the viewport contradicts `DEC-0015`.

## Design

### Sequence numbers

```rust
pub struct SeqNo(pub u64);      // monotonic, engine-wide

// Terminal
seq: SeqNo,                     // bumped once per feed() batch, not per mutation
// RowHeader
seq: SeqNo,                     // the batch that last mutated this row
flags: RowFlags,                // includes DIRTY
```

One bump per batch, so a burst that rewrites a row twenty times costs one stamp comparison. A
consumer keeps a watermark; "changed for me" is `row.seq > watermark`. There is no reset pass and
no consumer can clear another's damage.

**One watermark type, one owner for now (R-24, R-54).** `Watermark` is a newtype over `SeqNo` and
lives inside `RenderState`. The speculative multi-consumer surface — `Terminal::changed_rows()`,
a free-standing `damage` module, `VtEvent::ModeChanged` and `Config::mode_watch` — is **deleted**:
its named consumers (a search index, a semantic-highlight pass, session logging) are not
watermark consumers today and none is scoped in this intake. The per-row `SeqNo` is kept, because
it is free and it is the mechanism; a second consumer API is added when a second consumer exists.

`RowFlags::DIRTY` stays as a hint for the grapheme sweep and the graphics release scan, where
false positives are allowed and false negatives are not. It is never cleared per consumer.

Column bounds are not tracked: the reference tracks `left`/`right` per row and OneTerm's renderer
ignores them (`crates/terminal/src/content.rs` reads only `.line`).

### `render_update` — one call, under the lock

```rust
pub enum RenderUpdate {
    Unchanged,
    Partial { scrolled: i32 },
    Full,
}

impl Terminal {
    pub fn render_update(&mut self, state: &mut RenderState, now: Instant) -> RenderUpdate;
}
```

`&mut self` because it advances nothing the caller can see but does touch the sync deadline (the
graphics release scan runs in `feed`, **never here**: `render_update` has no `EventBatch` to deliver
a `GraphicReleased` into, and R-16 keeps `RenderState` out of graphics ownership — see
[`graphics.md`](graphics.md) § "Liveness and the release signal"); `now` is passed in rather than read from the clock (R-11) so a replay
is deterministic and the sync tests are not time-dependent. `feed` takes `now` for the same
reason.

### `RenderState` — full viewport plus a changed list

The earlier design defined `rows()` as both "indexed by viewport row" and "the ones that
changed", which cannot both be true (R-15). The renderer paints the whole viewport every frame
and separately needs to know what to rebuild, so it gets both:

```rust
pub struct RenderState {
    generation: u32,              // engine generation; a mismatch forces Full
    watermark: Watermark,
    viewport_top: RowId,
    scroll_offset: u32,
    rows: Vec<RenderRow>,         // ALWAYS the full viewport, indexed by viewport row
    changed: Vec<u16>,            // viewport row indices copied by this update
    cursor: RenderCursor,
    selection: Option<SelectionRange>,
    modes: ModeSnapshot,
    palette_epoch: u32,
}

pub struct RenderRow {
    pub id: RowId,
    pub seq: SeqNo,
    pub wrapped: bool,
    pub cells: Vec<RenderCell>,   // reused; one entry per column
    pub runs: Vec<StyleRun>,      // run-length over `cells`, resolved values
}

pub struct RenderCell {
    pub content: RenderContent,   // Scalar(char) | Cluster(range into the row's char arena)
    pub width: CellWidth,
    pub semantic: Semantic,
    pub run: u16,                 // index into `runs`
    pub hyperlink: Option<HyperlinkId>,
    pub graphic: Option<GraphicId>,
}

pub struct StyleRun { pub cols: Range<u16>, pub style: Style }   // RESOLVED, not an id
```

**`Partial { scrolled }` means:** shift your own cache by `scrolled` viewport rows, then rebuild
exactly the rows listed in `changed`. `rows` is valid in full either way.

### Resolved values, never ids (R-14)

Phase 1, **under the lock**, copies resolved data:

- Each changed row's cells are walked once; consecutive cells sharing a `style_id` become one
  `StyleRun` carrying the **`Style` value** read from the interner, not the id.
- A grapheme cell's cluster is copied into the row's own char arena.
- A hyperlink id is resolved to `(id, uri)` strings in a small per-state table, keyed by
  `HyperlinkId`, so the view's identity hash is stable and needs no engine access.
- `graphic` carries only the `GraphicId`; the placement geometry comes from the engine's
  placement table, copied into the render state alongside
  ([`graphics.md`](graphics.md)).

Phase 2, **outside the lock**, is `RenderState::map_colors(&Palette)`: it maps
`Color::Named` / `Color::Palette(u8)` through the theme and the OSC override table (copied as a
small `PaletteSnapshot` under the lock, versioned by `palette_epoch`) into concrete `Rgb`. That is
the only work that genuinely needs no engine state.

**`Palette` carries OneTerm's dim rule, it does not derive one.** A dim colour is a **50 % mix
with the background** (`crates/terminal/src/palette.rs:128-137`), not a fixed fraction toward
black, and the view applies its own alpha on top (`row_plan.rs:197-199`). `Palette` therefore
takes its dim entries from the adapter alongside the sixteen ANSI colours; it must not hard-code a
derivation with no override hook, or the adapter cannot supply the colours the product already
ships.

This kills the whole hazard class the earlier design had: interned ids could be renumbered by the
pump thread between `render_update` and `resolve`, and no assertion could catch the wrong colours
that resulted. It also removes the need for the style sweep that created the hazard
([`cell-and-style.md`](cell-and-style.md), R-52) — and with style ids now immutable for the life
of the terminal, even a future decision to pass ids would be safe.

Cost: roughly 20 bytes per `StyleRun` instead of 2 per cell, for **changed rows only**. A
uniformly styled 200-column row is one run.

### Mode snapshot (R-17)

```rust
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ModeSnapshot {
    pub alt_screen: bool,
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    pub show_cursor: bool,
    pub insert: bool,
    pub alternate_scroll: bool,
    pub mouse: Option<MouseProtocol>,   // composite of 1000/1002/1003 + 1005/1006 encoding
}
```

Refreshed on **every** `render_update`, including one that returns `Unchanged`, because
`crates/terminal-view/src/render/frame.rs:564` reads `APP_CURSOR` at paint time and
`crates/terminal/src/model.rs:192-405` reads alt-screen and mouse state about ten times without
holding the lock. Without this the view would have to take the lock during paint, which is the
one thing `docs/terminal-backend.md:173-175` forbids.

`ModeSnapshot` is `Copy` and compared by value, so a mode change with no row change still
produces `Partial` with an empty `changed` list, which is what makes a mouse-mode transition
repaint the cursor without repainting the screen.

### Graphics have exactly one drain owner (R-16)

`Terminal::take_graphics()` is the only drain, and the **adapter** calls it after the batch, next
to the other event handling. `RenderState` never touches pixels; it carries `GraphicId`s and the
placement geometry. The view's store is keyed by `GraphicId` and fed from the adapter, and entries
are evicted on `VtEvent::GraphicReleased`.

This matters because `DEC-0015` allows several render states: if `render_update` drained, only
the first caller would ever receive an image, and the adapter's own `take_graphics` would see
nothing.

### Scroll damage

`scrolled` is the signed change in the viewport's bottom row since the consumer's watermark, in
rows. A consumer with a per-row cache keyed by `RowId` shifts it instead of rebuilding.
`VtEvent::RowsScrolled { top, bottom, delta }` reports **content motion between row ids**, which is
a different question from the viewport delta above. Its contract is the table in
[`grid-and-scrollback.md`](grid-and-scrollback.md) § "Tracked anchors", restated here so the two
cannot drift: a **whole-viewport** scroll reports **nothing** (every surviving id keeps its
content, so a `RowId`-keyed cache is already correct and a delta would corrupt it); a region
anchored at row 0 with a bounded bottom reports the region with `delta = -n` **and** the tail below
it with `delta = +n`; a region not anchored at row 0 reports one event over its own id range; an
invalid or empty region reports nothing and never a range with `bottom < top`.

`Full` is returned by: the first call on a fresh `RenderState`; any resize or reflow; an
alternate-screen swap; `RIS`; a palette epoch change; a generation mismatch; and a `scrolled`
larger than the viewport height. **The list is inclusive, not exhaustive**: returning `Full`
whenever every viewport row was copied this update is correct and expected — `Full` means "rebuild
everything", and a `Partial` naming every row says the same thing more expensively.

### Fairness and reply latency

**The demand is a waiter count, and the waiter clears its own.** A one-shot flag consumed by the
pump's ask loses the frame that raised it: a pump asking in the window between "renderer raises"
and "renderer acquires the lock" takes the only signal that frame had, every later ask answers
"nobody is waiting", and the frame starves for the whole flood — measured at over five seconds
against 11-17 ms for a frame that keeps its signal. The rule:

```rust
// crates/terminal/src/handle.rs (crates/vt/src/render/demand.rs until US-0090)
pub struct Demand(Arc<AtomicUsize>);
pub fn raise(&self);             // fetch_add          — the renderer, BEFORE it blocks
pub fn release(&self);           // saturating sub     — the renderer, ONCE it holds the lock
pub fn is_raised(&self) -> bool; // load > 0           — the pump; it takes nothing away
```

- **`lock_for_render()` is raise, lock, release.** Between the raise and the acquisition the frame
  is both waiting and visible, so there is no window left: a pump that asks anywhere inside it
  yields, and yields again at the next boundary if the frame still has not got in.
- **`render_demand_raised()` reads without clearing.** (It was `take_render_demand()` until
  `US-0090` renamed it onto what it does: the `take_` was residue from the one-shot flag this
  section replaced, and the identically-bodied twin went with the rename.) The only visible change
  for the two pump loops is that a standing demand survives more than one ask, so a pump may yield
  at several consecutive boundaries while a frame is queued. That is intended.
- `Demand::take`, the clearing swap, is **deleted**; it has no caller.
- **A count, not a level flag cleared on acquisition.** A flag is one line shorter and fixes the
  reported case, but the first waiter's acquisition then clears the second waiter's signal and it
  starves identically. The count is the same size with no such edge.

The window is pinned deterministically rather than raced for:
`handle::tests::a_pumps_ask_does_not_consume_a_frame_that_is_still_waiting` holds the engine on the
pump thread for the whole test, so once the demand is visible the frame provably has not acquired
anything — then asserts the demand survives **100 consecutive asks**, drops the guard once, and
requires the frame to arrive and the demand to be gone. Against the one-shot code it fails at
ask 1.

**As shipped, the flag lives on `TerminalHandle` beside the lock** (`crates/terminal/src/handle.rs`):
`lock_for_render()` raises then releases once it holds the lock, and `render_demand_raised()` is the
pump's yield check, which takes nothing away. Measured: a pump that honours it hands the
lock over in **one batch / 157 us**; the same pump ignoring it makes the renderer wait **3 800
batches / 354 ms** ([`migration.md`](migration.md) § "The adapter contract"). Calling it from the
two backend loops is `US-0083` and `US-0084`'s.

```rust
// the original one-shot shape, replaced by the count above
pub struct Demand(Arc<AtomicBool>);
impl Demand {
    pub fn raise(&self);          // store(Release)      — the render thread
    pub fn take(&self) -> bool;   // swap(false, AcqRel) — the pump, at a chunk boundary
}
```

**Placement: the adapter crate, `crates/terminal/src/handle.rs`.** This section originally
recorded the opposite — the primitive shipped in `crates/vt/src/render/demand.rs` as a stated
exception to the HLD's "`oneterm-vt` contains no lock, no atomic and no interior mutability",
"until `US-0081` moves the pump loop over". `US-0081`, `US-0083` and `US-0084` shipped, and
`US-0090` moved it, so the exception is gone and the HLD's sentence is now true as written.

- It is **not engine state**. It holds no terminal data, `Terminal` does not own one, and no
  engine method reads it. Every caller — the raise, the release and the pump's ask — is in
  `crates/terminal`, so the primitive now sits beside the policy that owns it, in the same file as
  [`TerminalHandle`].
- `Terminal` stays `Send + !Sync` and contains no atomic; the rule the HLD is really protecting —
  that engine state is reached only through `&mut self` under the caller's lock — is untouched,
  and `grep -rn "Atomic" crates/vt/src` now returns nothing outside test files.

The adapter owns the **policy**: who raises it, when the pump tests it, and how long it parks.
Nothing in the engine parks or yields.

The pump's loop, in this order, and the order is the contract (R-37):

```
1. take a chunk (<= 64 KiB)
2. lock, feed(chunk, &mut batch, now), unlock
3. drain the batch:  ONE PASS, IN BYTE ORDER — a Reply leaves the transport in the
                     position the input asked for, ahead of any yield
4. answer any deferred colour queries (one batch of deferral, as today)
5. if demand.take() { yield before the next lock }
```

Replies must not wait behind a yield: conhost's `VtIo::StartIfNeeded` blocks for up to one second
waiting for the DA1 answer at session start, and that is precisely the moment a burst of output is
arriving ([`dispatch-and-modes.md`](dispatch-and-modes.md) § "Answers"). The adapter tests assert a
startup-latency bound.

**R-37 is "replies promptly", not "replies first" (`US-0088`).** An earlier drain ran two passes —
every `Reply`, then everything else — which reorders a reply against the sequence that asked for
it: a support query answered by the *embedder* in the second pass lost its place to a DA1 answered
by the *engine* in the first, so an agent using the documented "query then `CSI c`" idiom concluded
the terminal did not implement the protocol. The drain is **one pass in byte order**, whichever
side produced the reply, and the latency rule still holds because after `US-0082` the only thing a
reply can queue behind is a push onto a vector that never leaves the function.

`parking_lot::FairMutex` hands the lock over on unlock, but a thread that unlocks and immediately
relocks still beats a sleeping waiter, which is why the explicit demand flag exists. The 64 KiB
chunk cap is the backstop.

### Synchronized output (mode 2026)

```rust
struct SyncState { open_until: Option<Instant>, watchdog: Option<Instant> }
const SYNC_REFRESH: Duration = Duration::from_millis(150);
const SYNC_WATCHDOG: Duration = Duration::from_secs(1);
```

- `CSI ? 2026 h` sets `open_until = now + SYNC_REFRESH` and, on the first open,
  `watchdog = now + SYNC_WATCHDOG`. A further `h` refreshes `open_until` only. `CSI ? 2026 l`
  closes immediately.
- `render_update(state, now)` returns `Unchanged` while the update is open and neither deadline
  has passed — but **never `Unchanged` over an incomplete state**. A suppressed frame must still
  fill the render state to its full-viewport invariant (R-15) before it returns, so the first
  update on a fresh `RenderState` that lands inside a sync block yields `rows().len() ==
  viewport.rows`, not zero.
- **A change made inside a sync block is reported on the next frame, never swallowed.** Mode and
  cursor changes are refreshed on every call, including a suppressed one, so a suppressed frame
  that overwrites `state.modes` or `state.cursor` must carry that difference forward: the next
  unsuppressed update compares against the values the consumer last *saw*, not against the values
  the suppressed frame silently stored. `? 2026 h` then `? 25 l` then `? 2026 l` must end in a
  `Partial`, or the view keeps painting a cursor the program turned off. Both deadlines are evaluated against the `now` the caller passes, so a replay with a
  synthetic clock is deterministic and the tests are not flaky.
- Nothing is buffered: the reference's 2 MiB `SYNC_BUFFER_SIZE` of unapplied bytes disappears, and
  with it a memory amplifier a hostile stream can aim at us. `DECRQM` reports the real state.
- Images decoded during a skipped frame are not lost: they queue in the engine until the adapter's
  next `take_graphics`.

### What damage does not cover

Selection and the cursor are not part of row damage; `render_update` refreshes both every call, so
a blinking cursor or a drag does not force a row rebuild.

`INSERT` mode does **not** force full damage (trap 34, deviation D2): the reference calls
`mark_fully_damaged()` on every `damage()` call while insert mode is set, silently disabling
partial redraw for the session.

## Interfaces

```rust
// crates/vt/src/render.rs
impl RenderState {
    pub fn new() -> Self;
    pub fn map_colors(&mut self, palette: &Palette);
    pub fn rows(&self) -> &[RenderRow];          // always the full viewport
    pub fn changed(&self) -> &[u16];             // viewport row indices to rebuild
    pub fn cursor(&self) -> &RenderCursor;
    pub fn selection(&self) -> Option<SelectionRange>;
    pub fn modes(&self) -> ModeSnapshot;
    pub fn placements(&self) -> &[Placement];    // geometry only; pixels come from the adapter
    pub fn invalidate(&mut self);                // force Full on the next update
}
```

## Edge Cases and Failure Modes

- [ ] **Trap 34 — insert mode forcing full damage** — deviation D2.
- [ ] **Trap 35 — damage indices shifting with the scroll offset.** Not applicable: rows carry
  `RowId` and the viewport is an offset, so an off-screen change stamps its row and is simply not
  copied ([`grid-and-scrollback.md`](grid-and-scrollback.md) § "Viewport anchoring").
- [ ] **Generation mismatch** — a `RenderState` reused against a different `Terminal`, or after a
  resize the consumer did not observe, returns `Full` and rebuilds.
- [ ] **`scrolled` larger than the viewport** — returns `Full`.
- [ ] **Mode change with no row change** — `Partial` with an empty `changed` list.
- [ ] **Sync update opened and never closed** — the refresh deadline expires and frames resume;
  the watchdog forces the mode off and emits `ModeChanged`-equivalent state in `ModeSnapshot`.
- [ ] **`render_update` called twice with no `feed` between** — the second returns `Unchanged`
  with an empty `changed` list; `map_colors` is a no-op while the palette epoch is unchanged.
- [ ] **A consumer that never calls `map_colors`** paints named colours as their fallback; a debug
  assertion catches an unmapped row reaching a `rows()` reader that expects resolved colours.
- [ ] **Hyperlink strings** are resolved under the lock into the state's own table, so a link
  whose interned entry is later replaced cannot change the painted URL mid-frame.

## Verification

`cargo test -p oneterm-vt render::`

- [ ] `render::tests::first_update_is_full`
- [ ] `render::tests::idle_terminal_returns_unchanged`
- [ ] `render::tests::rows_always_hold_the_full_viewport` — R-15; asserts
  `rows().len() == viewport.rows` for `Unchanged`, `Partial` and `Full`.
- [ ] `render::tests::single_row_change_lists_one_changed_index`
- [ ] `render::tests::pure_scroll_reports_a_delta_with_an_empty_changed_list`
- [ ] `render::tests::scroll_larger_than_the_viewport_returns_full`
- [ ] `render::tests::resize_and_alt_swap_and_ris_each_return_full`
- [ ] `render::tests::cursor_only_movement_returns_partial_with_no_changed_rows`
- [ ] `render::tests::mode_change_only_returns_partial_and_refreshes_the_snapshot` — R-17.
- [ ] `render::tests::mode_snapshot_is_refreshed_even_when_unchanged` — R-17.
- [ ] `render::tests::style_runs_carry_resolved_values` — R-14; asserts a run holds a `Style`,
  and that a subsequent grapheme sweep cannot change a previously copied row.
- [ ] `render::tests::uniform_row_is_one_style_run`
- [ ] `render::tests::dim_colours_come_from_the_palette_not_from_a_derivation` — the 50 %-with-bg
  rule survives `map_colors`.
- [ ] `render::tests::size_is_readable_from_the_render_state`
- [ ] `render::tests::hyperlink_strings_are_resolved_under_the_lock`
- [ ] `render::tests::render_state_never_drains_graphics` — R-16; asserts
  `Terminal::take_graphics` still returns the image after any number of `render_update` calls, and
  that two render states both see the placement.
- [ ] `render::tests::steady_state_makes_no_allocation` — a counting allocator over 600
  update-plus-map cycles.
- [ ] `render::tests::insert_mode_does_not_force_full_damage` — trap 34.
- [ ] `render::tests::changes_while_scrolled_back_are_not_copied` — trap 35.
- [ ] `render::tests::generation_mismatch_forces_full`
- [ ] `render::tests::watermark_never_moves_backwards`

Sync (deterministic, with an injected clock):

- [ ] `sync::tests::mode_2026_suppresses_frames_until_close`
- [ ] `sync::tests::mode_2026_refresh_deadline_resumes_frames`
- [ ] `sync::tests::mode_2026_watchdog_forces_the_mode_off`
- [ ] `sync::tests::mode_2026_with_a_leading_parameter_is_recognised` — the reference's
  eight-byte memcmp cannot do this (trap 41).
- [ ] `sync::tests::decrqm_reports_2026_as_set_while_open`
- [ ] `sync::tests::images_survive_a_skipped_frame`
- [ ] `sync::tests::a_suppressed_frame_still_fills_the_full_viewport` — R-15 holds inside a sync
  block, including on the very first update.
- [ ] `sync::tests::a_mode_change_inside_a_sync_block_is_reported_after_it_closes`
- [ ] `sync::tests::a_cursor_move_inside_a_sync_block_is_reported_after_it_closes`

Adapter-side, at the shim packet:

- [ ] `backend::tests::replies_are_written_before_any_yield` — R-37.
- [ ] `backend::tests::da1_is_answered_within_the_startup_budget` — a burst of output plus a
  raised demand flag, asserting the DA1 reply leaves within a stated millisecond bound.
- [ ] `backend::tests::pump_yields_to_the_render_demand_within_one_chunk`

Benchmark tier 3 and a "frame time under `yes`" measurement, both recorded and never gated
([`testing-and-bench.md`](testing-and-bench.md)).
