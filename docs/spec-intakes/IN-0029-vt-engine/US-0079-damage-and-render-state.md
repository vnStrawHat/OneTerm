# Work: Damage, render state and events

ID: US-0079
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (the engine-to-view hand-off of `oneterm-vt`, on top of `US-0075`'s
  grid)
- Risk lane: high_risk (the intake's lane; this packet ships no product-reachable behaviour yet —
  nothing in the workspace depends on `oneterm-vt`)
- Spec Intake, when required: IN-0029

## Outcome

The render-state hand-off and the event batch exist and are proven against
[`low-level-design/damage-and-render-state.md`](low-level-design/damage-and-render-state.md) and
[`low-level-design/events-and-api.md`](low-level-design/events-and-api.md):

1. **Damage** — the per-row `SeqNo` `US-0075` already stamps, read through a **consumer-owned
   watermark**. No reset pass, no consumer clearing another's damage, `RowFlags::DIRTY` untouched.
2. **`RenderState`** — a full-viewport `rows()` **plus** a `changed()` list (R-15), a
   tri-state result (`Unchanged` / `Partial { scrolled }` / `Full`), and a scroll reported as a
   signed viewport delta the consumer applies as a **move** instead of a rebuild.
3. **Resolved values, never ids (R-14)** — phase 1 (`begin_update`, under the caller's lock)
   copies changed rows as run-length `StyleRun`s carrying the `Style` **value**, clusters into the
   row's own char arena, and hyperlink strings into the state's own table. Phase 2
   (`map_colors`, outside the lock) maps named and palette colours through a `Palette`.
4. **Mode snapshot (R-17)** — refreshed on **every** update, including one that returns
   `Unchanged`, so the view never takes the lock at paint time.
5. **Synchronized output (mode 2026)** — frames are skipped, nothing is buffered; the 150 ms
   refresh deadline and the 1 s watchdog are evaluated against an injected `now` (R-11).
6. **Fairness** — the `Demand` flag the pump tests at every chunk boundary, with a two-thread
   test showing a waiting renderer is let in.
7. **`EventBatch` / `VtEvent`** — events are values in a caller-owned batch over one reusable
   arena; `StrSpan` / `ByteSpan` / `ParamSpans` instead of per-event `Vec`s; the grid's
   `ScrollReport` is consumed into `RowsScrolled` / `RowsTrimmed`.

## Scope

- [ ] In scope:
  - `crates/vt/src/render/` — `mod.rs`, `state.rs`, `row.rs`, `palette.rs`, `modes.rs`,
    `sync.rs`, `demand.rs` and the test files `render_tests.rs`, `sync_tests.rs`,
    `render_bench.rs`.
  - `crates/vt/src/events/` — `mod.rs`, `batch.rs`, `vt_event.rs`, `event_tests.rs`.
  - `crates/vt/src/lib.rs` — the module declarations and re-exports only.
- [ ] Out of scope:
  - `Terminal` itself, `feed()` and `FeedStats`' producers: the parser (`US-0073`) and dispatch
    (`US-0076`) are implemented concurrently. This packet defines the surface `feed` fills and
    the surface `render_update` will be a three-line shim over; it parses nothing.
  - `crates/vt/src/grid/` (`US-0075`, and resize/reflow under `US-0077` concurrently) — read
    only, not modified.
  - Selection (`US-0078`): `RenderState` carries no `selection` field yet.
  - Graphics (`US-0080`): a `RenderCell` carries the `GraphicId` its extras hold, but there is no
    placement table and no `take_graphics` to keep out of the render path yet.
  - `VtEvent::ColorQuery` and `ColorKey` (`US-0076` owns the colour model).
  - The adapter-side tests the LLD lists under `backend::` (`US-0081` / `US-0082`).

## Acceptance

- [x] The tri-state is proven: an unchanged frame returns `Unchanged` **with zero row copies**,
      one changed row returns `Partial` listing exactly that row, and a resize, an
      alternate-screen swap and `RIS` each return `Full`.
- [x] `rows()` holds the full viewport for `Unchanged`, `Partial` and `Full` alike (R-15).
- [x] Watermark semantics hold for **two independent consumers**: each sees the same change once,
      neither clears the other's damage, and a watermark never moves backwards.
- [x] The run-length copy carries **resolved `Style` values** and survives a later grapheme
      sweep; a 200x50 grid with a uniform style yields exactly one run per row.
- [x] Scroll damage is consumed as a move: a pure viewport scroll returns `Partial { scrolled }`
      whose `changed` list holds only the rows the scroll exposed, and a scroll larger than the
      viewport returns `Full`.
- [x] Mode 2026: frames are suppressed while an update is open, the 150 ms refresh deadline
      resumes them, and a program that never closes the block has the mode forced off by the 1 s
      watchdog and gets a frame.
- [x] The demand/yield handshake is proven with two threads: a pump loop feeding continuously
      lets a waiting renderer in within the stated bound.
- [x] A bench-style `#[test]` (meaningful in release) reports the per-frame render-state build
      cost for an unchanged frame, one changed row and all 50 rows, against the old engine's
      29.9 us full snapshot.
- [x] No `unsafe`, no new external dependency, `crates/vt/Cargo.toml` untouched.
- [x] `pwsh scripts/ci-local.ps1` is green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` — the
  contract this packet implements: sequence numbers and watermarks, `RenderState`'s two lists,
  resolved style runs, the mode snapshot, scroll damage, fairness, mode 2026, and the
  `render::` / `sync::` verification list.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` — `EventBatch`,
  `VtEvent`, the arena and span model, the `feed`/drain contract, the error policy, and the
  crate's public surface.
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — the accepted
  contract: incremental render state, resolved values never ids, the tri-state, scroll as a
  delta, damage as a per-row sequence number plus a consumer watermark, events as values.
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` § "Threading and locking",
  § "Data ownership" — the engine is `Send`, not `Sync`; the lock, the demand flag and the
  drain loop are the adapter's; `RenderState` is caller-owned and reused.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md`,
  `grid-and-scrollback.md` — `StyleRun` resolution from interned ids, the row header fields
  (`seq`, `RowFlags::DIRTY`) and the `RowsScrolled` contract this packet consumes.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the recorded,
  never gated benchmark; tier 3 is "parse + grid + one `render_update` + `map_colors` per
  simulated frame".
- `docs/spec-intakes/IN-0029-vt-engine/research/perf-baseline.md` § 4 — the 29.9 us/frame full
  snapshot this hand-off is measured against.
- `docs/spec-intakes/IN-0029-vt-engine/research/api-surface.md` § 8-9 — what the view actually
  reads (G1, G3, G6, G9, G12, M1, E1-E9), and the idioms not to reproduce.
- `docs/terminal-backend.md` § 5 — the snapshot-versus-live-borrow rule this design has to keep:
  the lock is never held while painting, which is why phase 2 runs outside it.

### Documentation Action

No contract change. The two owning LLDs, `DEC-0015` and the HLD already describe the behaviour
implemented here, and this packet changes no shipped behaviour (nothing depends on `oneterm-vt`
yet, so `docs/terminal-backend.md` still describes the live engine correctly and must not be
edited until `US-0081` puts the new engine behind the seam). Readings taken where an LLD is silent
or where a type it names belongs to a concurrent packet are recorded under "Evidence and Gaps"
for the design owner instead of being written into the LLD, which this packet may not edit.

Reason: the design is accepted and detailed; the deltas are implementation shape (module layout,
the `EngineView` borrow bundle standing in for the not-yet-existing `Terminal`), not contract.

### Reconciliation

No owning doc changed. The no-change reason above remains valid: `damage-and-render-state.md`,
`events-and-api.md`, `DEC-0015` and the HLD describe the shipped surface, and every deviation is
listed in "Evidence and Gaps" for the design owner to fold in when `Terminal` exists (`US-0076`).

## Context

- There is no `Terminal` struct yet (`events-and-api.md` defines it; `US-0076` owns it). The
  render state therefore takes **`EngineView<'_>`**, a borrow bundle of exactly the fields
  `Terminal::render_update` will pass: `&TerminalGrid`, `&Interner`, `&mut SyncState`, the
  `ModeSnapshot`, the generation and the palette epoch. `Terminal::render_update(state, now)`
  becomes a three-line shim over it.
- `US-0075` already stamps `RowHeader::seq` once per batch (`TerminalGrid::begin_batch`) and
  already returns `ScrollReport { scrolled, trimmed, history_rows }` with the `RowsScrolled`
  contract this packet consumes, so nothing in `crates/vt/src/grid/` needed to change.
- The crate holds no lock, atomic or interior mutability (`lib.rs`). `Demand` is the adapter's
  primitive and owns the packet's only `AtomicBool`; it is reachable from no engine type.

## Plan

- [x] Write this packet and mirror the story row into `harness.db`.
- [x] `crates/vt/src/events/` — spans, arena, `VtEvent`, `FeedStats`, `ScrollReport` consumption.
- [x] `crates/vt/src/render/` — `ModeSnapshot`, `SyncState`, `Palette`, `RenderRow` copy,
      `RenderState::begin_update` / `map_colors`, `Demand`.
- [x] Tests: the LLD's `render::`, `sync::` and `event::` lists, minus the ones that need a
      parser, selection or graphics; the two-thread fairness test; the bench-style `#[test]`.
- [x] `pwsh scripts/ci-local.ps1`, then evidence and gaps.

## Decisions

- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — the accepted
  contract this packet implements. No new decision: every choice below is an implementation
  reading recorded in "Evidence and Gaps", not a contract future work must inherit.

## Verification Plan

- `cargo test -p oneterm-vt render:: sync:: event::` — the LLD's named tests.
- `cargo test -p oneterm-vt --release -- --nocapture render::bench` — the recorded per-frame
  numbers (the bench `#[test]` is `#[ignore]`d in debug builds, where timings are meaningless).
- `pwsh scripts/ci-local.ps1` — fmt, clippy `-D warnings`, the whole workspace test suite, the
  dependency-graph, doc-path, English and notices checks.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Verifier rework (merge after fixes)

Report: [`evidence/US-0079-verify.md`](evidence/US-0079-verify.md), verdict **merge after fixes**
against `b080858`. Applied here:

- **F1 (major) — the suppressed frame no longer short-circuits the state.**
  `begin_update` now builds a never-built state before mode 2026 can suppress anything
  (`sync.suppresses_frame(now) && !self.rows.is_empty()`), which closes the R-15 hole the
  verifier proved (`rows()` empty on a first update inside a sync block), and a sticky
  `meta_dirty` flag carries a mode or cursor change observed during a skipped frame to the next
  reported frame, cleared on any non-`Unchanged` result. Three tests:
  `sync::tests::{a_first_update_inside_a_sync_block_still_builds_the_state,
  a_mode_change_inside_a_sync_block_reaches_the_next_frame,
  a_cursor_move_inside_a_sync_block_reaches_the_next_frame}` — the last two also assert the
  change is reported **once**, not forever.
- **F3 (minor) — the crate doc is true again.** `lib.rs` now says the crate holds no lock and no
  interior mutability, and names `render::Demand` as the adapter's flag that no engine type can
  reach. The placement is recorded as deviation 10 below for the design owner.
- **F4 (minor) — dim follows OneTerm's rule.** `Palette::dim` is a 50 % mix with **this palette's
  background** (`crates/terminal/src/palette.rs`, `TerminalPalette::dim`), not a fraction toward
  black, and it is public so an adapter that swaps the background gets matching dim colours.
  `render::tests::dim_colours_match_oneterms_palette` pins the arithmetic against the live
  function, including the rounding, on two backgrounds.
- **F5 (minor) — `RenderState::size() -> Size`**, with
  `render::tests::size_reports_the_viewport` (zero before the first update, and it follows a
  resize).
- **F7** the span doc no longer overstates its guarantee (a stored raw span reads the new
  batch's bytes; it is the *event* the borrow checker keeps from escaping). **F8** the fairness
  test is renamed `…_within_a_bounded_number_of_chunks`, matching its assertion. **F6 (half)**
  `render::sync` is a private module again, public only through its re-exports; the `events/`
  directory keeps its one `#[path]` line because the packet's file scope names that directory.
  **F10 (half)** the undocumented accessors on `RenderState` and `EventBatch` now carry doc
  comments.
- **F2** is not this packet's: `Screen::place_row` / `reset_row` drop a row's batch stamp when
  they de-allocate it, which is being filed as a BUG against `US-0075`. `RenderRow::allocated`
  stays either way — the verifier confirmed it is complete for `RenderState`.
- **F9** (linear hyperlink lookups, table cleared only on a `Full` rebuild) and the rest of
  **F10** (surface with no consumer until `US-0081`) stay as recorded gaps; **F11** is answered
  in the Gaps list below.

### Evidence

**Base correction.** This worktree was branched from `main` (`c936ac0`), not from `feat/vt-engine`,
so `crates/vt` was missing entirely. The branch was `git reset --hard 83933b9` before any file was
written; `git log --oneline -1` is the US-0075 merge and `crates/vt/src/grid/` is present.

**Gate.** `pwsh scripts/ci-local.ps1` green after the verifier rework: **54 test-result sections,
1315 passed / 0 failed / 6 ignored** (it was 1310 before the rework's five new tests).
`US-0075`'s recorded baseline on this branch was 54 sections / 1267 passed / 5 ignored, so the
delta is exactly this packet's 48 new passing tests plus the one release-only benchmark.

**Crate.** `cargo test -p oneterm-vt`: **138 tests, 137 passed, 0 failed, 1 ignored, 0.53 s** —
29 `render::tests`, 8 `render::sync::tests`, 11 `event::tests`, 1 `render::bench` (ignored in
debug). The suite is also green in release (`cargo test -p oneterm-vt --release`: 138 passed).

**Tri-state.** `RenderState::rows_copied()` is the counter the proofs read:

| Case | Result | `changed` | Rows copied |
| --- | --- | --- | --- |
| First update on a fresh state | `Full` | all 10 | 10 |
| Five idle frames | `Unchanged` | empty | **0** |
| One row rewritten | `Partial { scrolled: 0 }` | `[3]` | 1 |
| Viewport scrolled by one line | `Partial { scrolled: 1 }` | `[9]` | 1 (not 10) |
| Scroll of 15 rows in a 10-row viewport | `Full` | all | 10 |
| Resize, alt swap, `RIS` (generation bumped) | `Full` each | all | all |
| Cursor moved only | `Partial { scrolled: 0 }` | empty | 0 |
| Mode changed only | `Partial { scrolled: 0 }` | empty | 0 |

**Watermarks.** `two_consumers_keep_independent_watermarks`: a view and a search state both see
row 6 once and neither clears it for the other; `watermark_never_moves_backwards` walks ten
batches plus an idle update, and a debug assertion guards the same invariant inside
`begin_update`.

**Resolved values.** `style_runs_carry_resolved_values` asserts a run holds a `Style` value;
`a_copied_row_survives_a_grapheme_sweep` renumbers the arena underneath a copied row and the
cluster still reads `['a', '\u{0301}']`; `uniform_row_is_one_style_run` fills a **200x50** grid
with one style and asserts **one run of `0..200` per row**;
`hyperlink_strings_are_resolved_under_the_lock` reads the URI back off the render state with no
interner in hand.

**Scroll as a move.** `scroll_damage_is_consumed_as_a_move_instruction`: after two line feeds the
result is `Partial { scrolled: 2 }`, the eight cached rows are the same `RowId`s shifted by two,
and only `[6, 7]` were rebuilt. `an_in_region_scroll_moves_content_between_row_ids` pairs the
grid's `RowsScrolled` report with the three rows the region scroll actually changed, and
`event::tests::a_whole_screen_scroll_emits_no_motion` pins the other half of the contract.

**Mode 2026.** Deterministic with the injected clock: frames suppressed inside the 150 ms window,
resumed at the refresh deadline, and a program that refreshes the update forever has the mode
forced off at `SYNC_WATCHDOG` (1 s) and gets its frame — `is_set()` is `false` afterwards. The
mode snapshot is refreshed even on a skipped frame.

**Fairness.** `pump_yields_to_the_render_demand_within_one_chunk` runs two threads: a pump
rewriting all 24 rows per chunk in a loop, and a renderer that raises the flag and takes the lock.
The renderer is let in within **8 chunks and under 2 s** (measured: one chunk, sub-millisecond,
five consecutive runs).

**Benchmark** (release, 200x50, 2 000 frames per scenario; only `begin_update` + `map_colors` are
timed, the grid mutation is outside the clock — the same question the baseline answered):

| Frame | Cost | vs the old 29.9 us snapshot |
| --- | ---: | ---: |
| Unchanged | **0.098 us** | 305x cheaper |
| 1 row changed | **0.760 us** | 39x cheaper |
| 50 rows changed | **30.202 us** | 1.0x (parity) |

Recorded, never gated. The full-viewport case is at parity rather than ahead because a
`RenderCell` is about 40 B against the fork's 8 B cell and every colour is mapped; the whole win
is that a frame now costs what *changed*, which is one or two rows in every real workload.

**Finding for the design owner (and `US-0075`).** A row blanked back to an **unwritten ring slot**
loses its batch stamp with its allocation: `Screen::scroll_up_inside_region` takes the row out and
`reset_row` then stores `None` for a default background, so the row reads as `seq == 0` and a
watermark consumer would paint stale content. The render state closes it by carrying
`RenderRow::allocated` and copying when an allocated row became unallocated — no grid change was
needed, and `an_in_region_scroll_moves_content_between_row_ids` is the regression test. The design
owner may prefer to stamp the blank instead, which would cost the lazy-row memory win.

**Files.** New: `crates/vt/src/render/{mod,state,row,palette,modes,sync,demand}.rs` plus
`{render_tests,sync_tests,render_bench}.rs`, and `crates/vt/src/events/{mod,batch,vt_event}.rs`
plus `event_tests.rs`. Modified: `crates/vt/src/lib.rs` (module declarations and re-exports only).
**No file under `crates/vt/src/grid/`, `parser/` or `reflow/` was touched**, `crates/vt/Cargo.toml`
and `Cargo.lock` are unchanged, there is no `unsafe` anywhere in `crates/vt`, and no dependency was
added.

### Deviations from the LLDs

1. **`EngineView` instead of `Terminal::render_update`.** `Terminal` does not exist
   (`US-0076`), so phase 1 is `RenderState::begin_update(EngineView<'_>, now)`, where `EngineView`
   borrows exactly the fields the LLD's `Terminal` lists (`grid`, `intern`, `sync`, the modes, the
   generation, the palette epoch). `Terminal::render_update` becomes a three-line shim.
2. **Not yet carried:** `selection` (`US-0078`), `placements()` and `take_graphics`
   (`US-0080`), `VtEvent::ColorQuery` and `ColorKey` (`US-0076` owns the colour model), the
   cursor *shape* (`CursorStyle` is `US-0076`'s).
3. **Module homes.** `ModeSnapshot` / `MouseProtocol` live in `render::modes` and `SyncState` in
   `render::sync`, because `mode.rs` belongs to `US-0076`; the LLD's `sync::` test filter still
   matches. The event module is named `event` (the LLD's name and test filter) with its files
   under `events/` (this packet's file scope), through one `#[path]` line in `lib.rs`.
4. **`VtEvent::RowsScrolled(RowsScrolled)`** reuses the grid's struct rather than repeating its
   three fields as a struct variant.
5. **`Full` also when every row was copied.** A consumer rebuilds everything either way, and it is
   what makes `RIS` report `Full`. Resize (a size change) and an alternate-screen swap (a row-id
   lane change) are additionally detected by the render state itself, so they do not depend on the
   terminal remembering to bump the generation.
6. **`pure_scroll_reports_a_delta_with_an_empty_changed_list` is impossible as named**: a viewport
   scroll always exposes at least one row the consumer has never seen. The test is
   `pure_scroll_reports_a_delta_and_copies_only_the_exposed_row` — one copy instead of ten.
7. **Allocation is proven by capacity, not by a counting allocator** (`GlobalAlloc` is an unsafe
   trait), the same way `US-0075` measured its heap: `steady_state_makes_no_allocation` watches
   every row's three buffers across 600 update-plus-map cycles, and
   `osc_params_are_spans_not_vectors` watches the batch's three across 1 000 pushes.
8. **The fairness test uses `std::sync::Mutex` plus a 250 us park** in the pump's yield, because
   `std`'s mutex is not fair; the adapter's `parking_lot::FairMutex` hands the lock over on unlock
   and needs only the flag. No dependency was added for the test.
9. **`Watermark`** is a newtype over `SeqNo` in `render::state` rather than in a `damage` module,
   which R-24 / R-54 deleted.
10. **`Demand` lives in `crates/vt`**, while the LLD and `high-level-design.md` both place it in
    `crates/terminal` ("the adapter owns this; the engine has no atomics"). It is here because
    this packet's file scope is `crates/vt`, and it holds the crate's only atomic. It is
    reachable from no engine type and nothing in the engine reads it; `lib.rs` says so. The
    design owner decides whether `US-0081` moves the file or the rule.

### Gaps

- **Tests that need a parser, `feed`, selection or graphics** are not written here and are named
  in their owning packet: `event::tests::{feed_clears_the_batch_and_returns_stats,
  chunking_is_invariant, steady_state_feed_makes_no_allocation, feed_never_panics_on_fuzz_corpus,
  malformed_input_moves_the_right_counter, undrained_batch_asserts_in_debug}`,
  `sync::tests::{mode_2026_with_a_leading_parameter_is_recognised, images_survive_a_skipped_frame}`,
  the `testing::` builders and `api::tests::public_surface_is_send_not_sync` — all `US-0076`, with
  `US-0080` for the images.
- **`render::tests::render_state_never_drains_graphics`** is only half provable today: there is no
  graphics queue to keep undrained, so `two_render_states_both_see_the_graphic` proves the
  reachable half (both states see the same `GraphicId`). `US-0080` owns the drain assertion.
- **The undrained-batch debug assertion** cannot live in `EventBatch`: detecting "the consumer
  never looked" from a `&self` accessor needs interior mutability, which the crate forbids. It
  belongs to `feed`, which clears the batch (`US-0076`).
- **`decrqm_reports_2026_as_set_while_open`** is asserted at the `SyncState` level; the `DECRQM`
  answer itself is `US-0076`.
- **The render state's hyperlink table grows** until the next `Full` rebuild clears it. It is
  bounded by the distinct links a frame references, and `US-0076`'s link capping is where a
  tighter rule belongs.
- **A `map_colors`-less consumer** is not caught by a debug assertion, as the LLD's edge-case list
  suggests: a reader that wants unmapped named colours is legitimate, so the state exposes the
  fact through the mapped epoch instead of asserting on `rows()`.
- **No integration, E2E or platform proof**, because nothing in the workspace depends on
  `oneterm-vt` yet; `US-0081` is where the four GUI walks first cover this code.
- **The 50-row frame is at parity with the old snapshot, not faster** (see the benchmark table).
  Recorded for the design owner; nothing in the intake asks for a throughput win here.
- **This bench is not tier 3** (F11). `testing-and-bench.md` defines tier 3 as parse + grid +
  `render_update` + `map_colors` per simulated frame at 160x45; this times the hand-off only, at
  the 200x50 geometry `perf-baseline.md` § 4 measured the 29.9 us against, because no parser
  exists yet. The real tier 3 belongs to `US-0072`'s `vt-bench render` once `US-0076` lands the
  parser.
- **A mode 2026 watchdog has no cooldown**: after it forces the mode off, the next
  `CSI ? 2026 h` arms a fresh 1 s watchdog, so a hostile stream still costs roughly one frame per
  second. The LLD specifies no cooldown; flagged for the design owner.
- **`changed().len() == rows().len()` is unreachable under `Partial`** (deviation 5), so a
  consumer's "all rows changed" branch would be dead code. Worth one line in the LLD when
  `US-0076` folds these readings in.

## Rework (2026-09-13) — bounded integrity

**Reported by `US-0081`.** `RenderState::update` calls `TerminalGrid::assert_integrity` once per
render update (R-28), and that walk was the whole history of both screens: at a 100 000-row
scrollback it cost **252 771.9 us per `render_update`** in a debug build, against about 200 us for
the whole of the engine being replaced. The same walk at the end of `feed` cost the same again.

The fix is in the grid — `Screen::integrity_lo` now starts the walk at the screen top as it was
when the batch opened, or at the visible top when the viewport is scrolled back, instead of at
`oldest`, and the new `vt-paranoid` feature restores the whole-history walk for CI, the property
tests and the fuzz targets. The full account, the CI wiring and the gaps for the design owner are
in the matching section of [`US-0075`](US-0075-grid-and-scrollback.md#rework-2026-09-13--bounded-integrity);
what belongs to this packet is:

- **The `render_update` call site is unchanged.** `render/state.rs` still calls
  `grid.assert_integrity(Some(interner))` on every update that is not suppressed by mode 2026, and
  still before the rebuild-or-copy decision. Only the row range the call covers changed, so the
  R-17 ordering and the `Unchanged` path are untouched.
- **The probe lives here**, next to the tier-3 bench it belongs with:
  `snapshot::bench::integrity_walk_cost_per_feed_and_snapshot_update` in
  `crates/vt/src/snapshot/snapshot_bench.rs` (the module and the test were still called
  `render` / `render_update` when this rework landed; the names here are the current ones).
  It fills a 100 000-row history in one `feed`, then
  reports the per-`feed` and per-`render_update` cost; run it with `--features vt-paranoid` for the
  "before" number. Debug only — the walk does not exist in a release build — and ~~it asserts a 1 ms
  ceiling when the feature is off~~, so a walk that goes back to O(history) fails here rather than in
  a user's session. **The 1 ms ceiling was the defect `BUG-0075` fixed**
  ([`BUG-0075-integrity-walk-bench-measures-the-runner.md`](BUG-0075-integrity-walk-bench-measures-the-runner.md),
  2026-09-23): a wall-clock bound on the mean of ten calls cannot tell an O(history) walk from a
  descheduled thread, and a loaded CI runner failed it at 1382.4 us with the bound intact. The probe
  now runs twice in one process — a full history and a near-empty one — and asserts the **ratio**
  between them, which is the R-28 property itself and does not move when the machine does. The
  numbers below are unaffected; only what is asserted about them changed.

| | `feed` (one line) | `render_update` |
| --- | --- | --- |
| before (`--features vt-paranoid`) | 258 615.5 us | 252 771.9 us |
| after (default) | 142.9 us | 150.2 us |

**Verification.** `pwsh scripts/ci-local.ps1` green — raw totals **61 `test result:` sections,
1919 passed / 0 failed / 11 ignored**. `cargo test -p oneterm-vt` with and without `--features
vt-paranoid`, plain and at `VT_PROPTEST_CASES=5000`: 359 + 5 + 5 passed / 0 failed / 3 ignored in
every combination. `vt-diff` reports 45 of 45 recordings identical in both feature states.

**Gap.** The render half of R-28 in `testing-and-bench.md` still reads "the full two-screen walk"
at `render_update`; see the `US-0075` gap list for the wording the design owner needs to change and
for the `vt-paranoid` M12 note that can now close.

## Handoff

The bounded-integrity rework is on branch `worktree-agent-aea9b4dea012780de` off `feat/vt-engine`
@f5b06dd, one commit, **not merged and not pushed**; it also touches `US-0075`'s files.

`US-0076` (dispatch) owns `Terminal`, `Modes`, `ColorKey` and `feed`; it wires `EngineView` into
`Terminal::render_update`, adds `VtEvent::ColorQuery`, and calls `EventBatch::clear` at the top of
`feed`. `US-0078` adds the `selection` field to `RenderState`; `US-0080` adds the placement table
and `take_graphics`. `US-0081` / `US-0082` own the adapter-side `Demand` loop tests.
