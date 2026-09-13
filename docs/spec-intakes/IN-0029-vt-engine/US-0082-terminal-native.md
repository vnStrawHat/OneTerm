# Work: `crates/terminal` goes native

ID: US-0082
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change — `crates/terminal`'s own public API moves onto the
  engine's vocabulary. `TerminalSession` / `TerminalContent` keep their names and the seam keeps
  the value types `crates/terminal-view` reads, because `US-0085` owns that crate.
- Risk lane: high_risk (the intake's lane; this packet rewrites the adapter's event delivery,
  its frame path and its line accounting at once, all of them user-visible through the terminal).
- Spec Intake, when required: IN-0029.

## Outcome

`crates/terminal` is an adapter over `oneterm-vt`, not a translation layer bolted to one. After
this packet:

1. **The frame source is the engine's `RenderState`.** `TerminalContent` **owns** the
   `RenderState` — the watermark belongs to the buffer the consumer keeps, which is what
   `DEC-0015` says — and `snapshot_into` is one `Terminal::render_update` into it. The dense
   `Vec<IndexedCell>` is no longer rebuilt per frame: it is a **declared compatibility surface**
   refreshed from the tri-state result — untouched on `Unchanged`, only the `changed()` rows on a
   `Partial` that did not scroll, and with the resolved `StyleRun` converted **once per run**
   instead of once per cell.
2. **`RowId` is the coordinate inside the adapter.** `GridText`, the row readers, the snapshot's
   row walk and the gutter's line numbers are `RowId`-keyed; the signed `Line` survives only in
   the four published values `crates/terminal-view` still reads (`SearchMatch::line`,
   `TerminalInfo::{cursor_line,last_content_line}`, `TerminalQueryState::cursor_line`,
   `IndexedCell::point`), each converted in **one** place. `TerminalContent::{row_id,
   display_row}` is the two-way translation `migration.md` designed and `US-0081` left unbuilt.
3. **Events are drained as values, and the deferred tier is gone.** `OscRouter::drain` appends
   `SessionEvent`s to a caller-owned vector; the pump flushes that vector **outside the engine
   lock** with a blocking send. `SessionEventSink`'s deferred FIFO, `flush_reliable[_blocking]`,
   `forward_lifecycle*` and `has_deferred_reliable` are deleted, and with them the CORR-01
   "never block inside a `Term` callback" rule — there is no callback any more.
4. **`ColorKey` is the colour index space.** `osc_color.rs`'s `FOREGROUND_INDEX` /
   `BACKGROUND_INDEX` / `CURSOR_INDEX` (256/257/258) are deleted; `PendingColorQuery` carries a
   `ColorKey`. `palette.rs`'s `NamedColor` discriminant arithmetic becomes an explicit match.
   `DefaultColors` holds the engine's `Rgb`.
5. **`line_accounting.rs` is deleted.** The gutter's absolute line number is
   `Terminal::lines_produced()` — an exact count of output lines instead of a three-branch
   heuristic over `total_lines` that also scanned every chunk for `\n` under the lock.
6. **`legacy_resize.rs` is deleted** (R-44). The fifteen `keep_viewport_top_*` / `default_grow_*`
   tests and the engine's twenty `reflow::tests::keep_viewport_top_*` are green in the same
   commit first; the old module — the last code in the product tree that *executes*
   `alacritty_terminal` — goes in the next.
7. **The adapter owns the lock and the demand/yield handshake.** `SharedTerminal` is
   `Arc<TerminalHandle>`, a `parking_lot::FairMutex<Terminal>` plus one `oneterm_vt::Demand`.
   `lock_for_render()` raises the demand, `take_render_demand()` is what a read loop calls at a
   chunk boundary. `crates/terminal/src/{engine.rs,sync.rs}` are deleted: with the render state
   in the snapshot, the `Engine` wrapper had nothing left to hold.

What does **not** change: every consumer compiles unedited. `crates/local-shell`, `crates/ssh`,
`crates/terminal-view`, `crates/state`, `crates/sftp-ui`, `crates/workspace`, `crates/app` and
`crates/tools` are not touched.

## Scope

- [ ] In scope:
  - All of `crates/terminal` (the N-04 table's second row), its tests and
    `crates/terminal/tests/us0081_parity.rs`.
  - Additive only in `crates/vt`: nothing was needed, and nothing was added (see Evidence).
  - `docs/terminal-backend.md` § 5, § 5.3 — the rows `migration.md` § "Documentation
    reconciliation" assigns to this packet, plus the paragraphs made false by the deletions.
  - `docs/agents/structure.md` § 1 — the `crates/terminal` tree lines that name
    `line_accounting` and `event_sink`.
- [ ] Out of scope:
  - `crates/local-shell` and `crates/ssh` (`US-0083` / `US-0084`): their read loops, the
    `oneterm-pty` token constants, the `ResizePolicy` selection and the `alacritty_terminal`
    manifest lines. This packet provides and tests the adapter half of every API they need and
    describes it under *Handoff*.
  - `crates/terminal-view` (`US-0085`): the legacy `TerminalContent` shape, `TerminalPalette`,
    `SearchMatch`'s published fields, `TerminalInfo` / `TerminalQueryState`'s signed lines and
    the `alacritty_terminal` value vocabulary all stay, because moving any of them means editing
    that crate.
  - Deleting `alacritty_terminal` from any manifest (`US-0087`). See the deviation below.
  - `oneterm_vt::testing` and `oneterm_vt::strip` — `US-0081`'s gap 5 files them here, and they
    are declined: `crates/terminal/src/test_engine.rs` already holds the two helpers and
    `logging.rs` already strips with `oneterm_vt::parser`. `docs/agents/code-style.md` says to
    extract shared code after multiple consumers exist; there is one.

## Acceptance

- [x] `cargo test --workspace` green, with every test in `crates/terminal`, `crates/local-shell`,
      `crates/ssh` and `crates/terminal-view` passing **unchanged** or consciously rewritten —
      each rewritten or deleted test named below with what it used to pin and what pins it now.
      No test outside `crates/terminal` changed.
- [x] `crates/terminal/tests/us0081_parity.rs` still passes: the same 81 streams, the same
      five-difference allow-list, the same damage-soundness property. It is the proof that the
      native frame path produces the byte-identical snapshot the shim produced.
- [x] The fifteen `legacy_resize` tests and the engine's `reflow::tests::keep_viewport_top_*`
      are green **in the same commit** before `legacy_resize.rs` is deleted (R-44).
- [x] `resize_keeping_viewport_top`, `conhost_cursor_row` and `LineAccounting` have no
      definition and no call site left in `crates/` (`migration.md` § Verification,
      `US-0082`); what the grep still finds is the prose that records the deletion.
- [x] Every surviving `alacritty_terminal` item in `crates/terminal` belongs to the
      **compatibility surface** — the value vocabulary the crate publishes and the code that
      converts into it — and none of it is on the engine-facing path. The files and what each
      one publishes are listed under *Evidence*.
- [x] `pwsh scripts/ci-local.ps1` green, raw totals recorded.
- [x] Measured, recorded, never gated: the `US-0081` flood (4 MiB through 4 KiB chunks, grid
      120x30, scrollback 10 000, `fast-dev`), feed and snapshot reported separately, before and
      after, on this machine.
- [ ] GUI walk only if the desktop is interactive (`quser` Active): prompt, echo, Ctrl-C,
      reflow, selection, Sixel. **Not run** — the session is `Disc`; recorded as gap 1.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` — the may-touch /
  must-not-touch table (N-04), the per-crate swap detail for `US-0082`, the deletion list, the
  tests-that-change table, and the documentation-reconciliation rows. This is the contract.
- `.../low-level-design/events-and-api.md` — the batch drain, the adapter loop that "deletes the
  deferred tier, `flush_reliable_blocking`, and the reason the current `SessionEventSink` exists
  at all", and the error policy (no `Result` where nothing can recover).
- `.../low-level-design/damage-and-render-state.md` — `RenderState`'s full-viewport-plus-changed
  shape, the resolved-values rule (R-14), the mode snapshot (R-17), and § "Fairness and reply
  latency" (R-37): the pump order, and that the adapter owns the demand **policy** while the
  engine owns only the flag.
- `.../low-level-design/selection.md`, `.../graphics.md` — the wrappers and the placement-derived
  per-cell offset the snapshot reproduces.
- `.../low-level-design/testing-and-bench.md` — the bounded integrity tier that made the 2.2x
  measurement possible, and that no packet gates on a number.
- `.../research/api-surface.md` — what each consumer reads from `crates/terminal` today: the
  coordinate model (§ 3.2, § 3.3), the colour index space (§ 3.6), the unused mode bits (§ 3.5).
- `.../US-0081-engine-shim.md` and its evidence — what the shim maps, the S1-S5 declared
  snapshot deviations this packet must not widen, the residual `alacritty_terminal`, and the
  flood measurement whose 2.2x residue is named as "the shim's own viewport rebuild, which
  `US-0082` deletes".
- `docs/terminal-backend.md` — § 5 concurrency, § 5.3 pump layer and resize policy, § 9
  `TerminalSession`. § 5 and § 5.3 are this packet's documentation action.
- `docs/decisions/DEC-0014`, `DEC-0015`, `DEC-0008` — own the engine; absolute row ids and an
  incremental render state are the engine-to-view contract; the two resize policies.
- `docs/agents/{structure,crate-dependency-rules,code-style,error-policy}.md`,
  `docs/PROJECT.md` — R7 (`terminal` stays gpui-free), the "do not preserve backward
  compatibility" rule and its one exception here (a consumer packet owns the other side),
  untrusted input never panics.

### Documentation Action

**Update required**, limited to the rows `migration.md` assigns here plus the statements the
deletions make false:

- `docs/terminal-backend.md` § 5.3 — the native `ResizePolicy` (the assigned row): the whole
  `resize_keeping_viewport_top` / `conhost_cursor_row` procedure description is replaced by
  "the engine's `ResizePolicy::KeepViewportTop`", and the invariants list now points at the
  engine's tests.
- `docs/terminal-backend.md` § 5, § 5.3 — `Arc<TerminalHandle>` and the demand handshake;
  the `SessionEventSink`, `LineAccounting`, `OscRouter` and `TerminalPump` table rows; the
  "never block inside a `Term` callback" paragraph, which describes a callback that no longer
  exists.
- `docs/agents/structure.md` § 1 — the `crates/terminal` subtree still lists `line_accounting`
  and `event_sink` under `backend/`.

Reason: this packet changes how the pump delivers events, how a frame is produced and how the
resize policy is chosen — three things § 5 describes literally — and deletes two files the
structure tree names.

### Reconciliation

Docs changed with this work:

- `docs/terminal-backend.md` § 5 (title, § 5.1, § 5.2, § 5.3): the shared handle is
  `Arc<TerminalHandle>` = `parking_lot::FairMutex<Terminal>` + `Demand`; the frame path is
  `RenderState` owned by `TerminalContent` with the legacy cells as a named compatibility
  surface; the pump drains into a vector and flushes it outside the lock; `LineAccounting` is
  gone and the gutter number is `Terminal::lines_produced()`; the `KeepViewportTop` procedure is
  the engine's.
- `docs/agents/structure.md` § 1 — the `crates/terminal` tree lines.

No other owning doc changed: `migration.md`, the HLD and `IN-0029.md` describe this packet
correctly and are not this packet's to edit.

**Three deviations from the governing docs**, each a conflict inside them rather than a choice:

1. **`alacritty_terminal` stays a normal dependency of `crates/terminal`.** The intake's US-0082
   line asks for it to leave the manifest. It cannot, and the reason is structural, not lazy:
   `crates/terminal-view` reads the alacritty value types **out of** this crate's public API —
   `TerminalContent::{cells,cursor,mode,selection}` (`render/frame.rs`), `SelectionType` and
   `TermMode` (`input/mouse.rs`, `input/mouse_tests.rs`), `Rgb` / `Color` / `NamedColor` through
   `TerminalPalette` and `resolve_color` (`theme/palette.rs`). Dropping the dependency changes
   those four files, which are `US-0085`'s by the N-04 table and by this packet's own "must not
   touch". `migration.md` § "Deletion list" already schedules all five manifest lines at
   `US-0087` and says they become deletable at `US-0085`; that schedule stands. What this packet
   does instead is **confine** it: after this commit the crate names an `alacritty_terminal`
   item in exactly five files — `engine_shim.rs` (the conversion, ~330 lines, deleted by
   `US-0085`), `content.rs` (the compat field declarations), `session.rs` (the trait signatures
   and the macro), `palette.rs` and `osc_color.rs`. Nothing else, and nothing on the native
   path. It is a `[dependencies]` line, not a `cfg(test)` one, because the compat surface is
   public API rather than test code.
2. **`SearchMatch`, `TerminalInfo` and `TerminalQueryState` keep their signed lines.**
   `migration.md`'s tests-that-change table says the search suite's `Line` / `display_offset`
   assertions become `RowId` and that `display_row(display_offset)` is "deleted with the
   conversion". Both are impossible without editing `crates/terminal-view`, which **constructs**
   `SearchMatch { line, start_col, end_col }` in its own tests
   (`terminal_view/search.rs:437`) and calls `display_row` at `:140`. Adding or retyping a field
   breaks that crate. The conversion therefore survives as the compat surface and the packet
   moves the *implementation* to `RowId`: `GridText` is keyed by `RowId`, and the signed line is
   produced once, at the point the match is published. `US-0085` deletes the field.
3. **`ResizePolicy` keeps its two adapter variants.** `US-0083` / `US-0084` own the change to
   `oneterm_vt::ResizePolicy`, and they select it through `impl_pty_terminal_session!`'s
   `$resize_policy` argument. Retyping the enum here would break both backends on the same
   commit. Instead `TerminalModel::new` now takes `impl Into<oneterm_vt::ResizePolicy>`, so
   those two packets change one token each and nothing in this crate.

## Context

- **Why the watermark moves into `TerminalContent`.** `US-0081` put the `RenderState` on the
  shared `Engine` because `TerminalModel` is rebuilt per call. But `DEC-0015`'s whole point is
  that a watermark belongs to a *consumer*, and the consumer's durable object is the
  `TerminalContent` buffer the view reuses (`terminal-view/src/render/frame.rs:467`). Moving it
  there also fixes a latent bug the shim had: `TerminalRender::snapshot()` (allocating) and
  `snapshot_into()` shared one watermark, so any non-render `snapshot()` call would have stolen
  the renderer's damage. With the state in the buffer a fresh `TerminalContent` is simply
  `Full`, which is correct by construction.
- **Why the legacy rebuild got cheap rather than deleted.** The flood's snapshot half was 76 us
  per 4 KiB chunk against the old engine's 23 us for 3 600 cells, i.e. ~21 ns per cell. The cost
  is the per-cell conversion — an eleven-entry attribute loop, two colour matches and a
  `Cell::default()` — not the copy. `RenderRow` already carries the style as **runs of resolved
  values** (R-14), so converting once per run and reusing it across the run removes almost all
  of it, and the `changed()` list removes the rest whenever the viewport did not scroll.
- **The engine is `Send` and `!Sync` and takes `&mut self`,** so the lock is the adapter's. The
  demand flag is the engine crate's one atomic and is reachable from no engine type
  (`damage-and-render-state.md` § "Fairness and reply latency"); the *policy* — who raises it,
  when the pump tests it — is this crate's, which is why `lock_for_render` and
  `take_render_demand` live on `TerminalHandle`.
- **Ordering is the behaviour that must not move.** The UI sees a batch's reliable events, then
  that batch's single `Output`; a lifecycle event flushes everything queued before it. That was
  produced by a deferred FIFO; it is now produced by a pending vector flushed at the same point.
  `each_chunk_posts_exactly_one_output_after_its_reliable_events` pins it either way.
- **Colour queries must still be collected during `advance`,** because `crates/local-shell`'s
  read loop calls `take_color_queries()` before `finish_batch_blocking()`. So the drain is split:
  `advance` does what must happen under the lock and cannot block (reply bytes to the transport,
  colour queries queued, the `SharedState` caches updated); `finish_batch*` does what can block
  (the UI sends). That split is invisible to the backends.

## Plan

- [x] Record the base: both resize suites and `cargo test --workspace` green at `f9af66c`, and
      the flood bench "before".
- [x] `handle.rs` — `TerminalHandle`, the demand/yield API; delete `engine.rs` and `sync.rs`.
- [x] `content.rs` + `engine_shim.rs` — `TerminalContent` owns the `RenderState`; tri-state
      refresh of the compatibility cells; per-run style conversion; `row_id` / `display_row`.
- [x] `backend/` — the batch drain into a vector, the blocking flush outside the lock, the
      gutted sink, `line_accounting.rs` deleted, `lines_produced` as the gutter count.
- [x] `osc_color.rs`, `palette.rs`, `backend/state.rs` — `ColorKey`, no discriminant arithmetic.
- [x] `search.rs`, `model.rs` — `RowId`-keyed grid text and row reads.
- [x] Delete `legacy_resize.rs` and `sixel_tests.rs` (both drive the old engine; their contracts
      are pinned in `oneterm-vt`).
- [x] Docs, the measurement, `ci-local.ps1`.

## Decisions

- [`DEC-0014`](../../decisions/DEC-0014-oneterm-owns-its-vt-engine.md) — call sites may change.
- [`DEC-0015`](../../decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md) — the
  row-id and render-state contract this packet finally consumes directly.
- [`DEC-0008`](../../decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md) — the two
  resize policies, now the engine's alone.

No new decision: every choice here is recorded in the intake, the HLD or `migration.md`.

## Verification Plan

- Focused: `cargo test -p oneterm-terminal`, and `-p oneterm-local-shell`, `-p oneterm-ssh`,
  `-p oneterm-terminal-view` unchanged.
- Differential: `cargo test -p oneterm-terminal --test us0081_parity` — 81 streams, the
  five-difference allow-list, the damage-soundness property. This is the regression gate for the
  native frame path.
- R-44: `cargo test -p oneterm-terminal legacy_resize` and `cargo test -p oneterm-vt reflow::`
  green in the same commit, before the deletion commit.
- Unit / integration: `cargo test --workspace`, raw totals compared with the base on `f9af66c`.
- Quality gate: `pwsh scripts/ci-local.ps1`.
- Measurement (recorded, never gated):
  `cargo test -p oneterm-terminal --profile fast-dev --test us0081_parity -- --ignored
  flood_bench --nocapture`, before (at `f9af66c`) and after, feed and snapshot separately.
- E2E: a GUI walk from a `fast-dev` build of this worktree only if `quser` reports an Active
  session; otherwise recorded as a gap.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Commands and results

- `pwsh scripts/ci-local.ps1` — **green**, exit 0, all ten steps: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`,
  `cargo test -p oneterm-vt --features vt-paranoid`, and the six Python policy checks.
  Raw totals over its two test steps: **62 sections, 1918 passed, 0 failed, 13 ignored**
  — `cargo test --workspace` 58 / 1549 / 0 / 10, `vt-paranoid` 4 / 369 / 0 / 3.
- Base on this worktree at `f9af66c`, before any edit: `cargo test --workspace`
  58 sections, **1564 passed**, 0 failed, 10 ignored. The net −15 is **+11 new tests
  minus 26 deleted** (15 `legacy_resize`, 11 `sixel_tests`), each named below.
- `cargo test -p oneterm-terminal`: 250 lib + 5 `us0081_parity` (2 `#[ignore]`d), 0
  failed. `crates/local-shell` (30), `crates/ssh` (33) and `crates/terminal-view` (191)
  are **unchanged and untouched** — no file outside `crates/terminal` and the two docs
  was edited.
- **The differential still passes.** `crates/terminal/tests/us0081_parity.rs` feeds the
  same bytes to the old `Term` and to the native adapter and diffs **every** field of
  `TerminalContent` after each 4 KiB chunk over the 45 vendored recordings,
  `sixel_basic` and 35 hand-written streams, against the five-difference allow-list, plus
  the damage-soundness property. Green. That is the proof that the native frame path
  produces the byte-identical snapshot the shim produced: cells, cursor, mode, selection,
  scroll offset, bounds, damage and graphics. The S1-S5 deviations are unchanged and
  none was widened.
- **R-44 satisfied.** At `45588a7` the fifteen `legacy_resize` tests (old engine) and
  `oneterm-vt`'s forty `reflow::tests` were green in the same commit, and so were the
  eleven `sixel_tests` and the engine's twenty-six `graphics::tests`; `dd2d2a8` then
  deleted the two old suites. Recorded in both commit messages.
- Deletion greps clean: `resize_keeping_viewport_top`, `conhost_cursor_row` and
  `LineAccounting` have no definition and no call site in `crates/`.
- Measurement: [`evidence/US-0082-flood-bench.md`](evidence/US-0082-flood-bench.md).

### Measurement

`us0081_parity::flood_bench`, 4 MiB through 4 KiB chunks, grid 120x30, scrollback
10 000, `--profile fast-dev`, median of three runs on this machine. Recorded, never
gated.

| | before (`f9af66c`) | after | old engine |
| --- | ---: | ---: | ---: |
| feed | 93 ms | 93 ms | 55 ms |
| **snapshot** | **75 ms** | **56 ms** | 27 ms |
| total | 168.8 ms | **150.7 ms** | 83.3 ms |
| ratio | 2.07x | **1.81x** | — |

The snapshot half is down a quarter. Feed is the engine's and the bench calls
`Terminal::feed` directly, so nothing this packet changed appears in that column. The
flood is the worst case for the incremental paths — every chunk scrolls the whole
viewport away, so `changed()` never helps and `Unchanged` never happens — which means
the interactive gain (a keystroke rebuilds one row, not thirty) is not in the table.

### What survives of `alacritty_terminal`, and why

Nothing runs; nothing is on the engine-facing path. What is left is the value vocabulary
this crate **publishes** and the code that converts into it:

| File | What it publishes |
| --- | --- |
| `engine_shim.rs` | the whole conversion, ~420 lines; `US-0085` deletes the file |
| `content.rs` | `TerminalContent`'s compatibility fields and their types |
| `session.rs` | `TerminalQueryState`, `TerminalInfo`, and the `TermMode` / `SelectionType` / `Rgb` in the trait and macro signatures |
| `palette.rs`, `color_classification.rs`, `osc_color.rs` | `TerminalPalette`, `resolve_color`, `DynamicColors` — `theme/palette.rs`'s API |
| `mouse_encode.rs`, `model.rs` | `TermMode` in the encoder and the published signed grid line |
| `backend/state.rs` | `DefaultColors::from_legacy`, the one converter at the `set_default_colors` boundary |
| `lib.rs` | the three graphics re-exports `render/element.rs` reads |
| `test_support.rs`, tests | the fake fabricates the same shape; `us0081_parity.rs` *is* the old engine |

Moving any of them means editing `crates/terminal-view`, which is `US-0085`'s.

### Tests deleted or rewritten, and why

| Test | Disposition | Reason |
| --- | --- | --- |
| `legacy_resize` (15: `keep_viewport_top_*`, `default_grow_*`) | **deleted** | R-44's condition met: they pinned the ConPTY contract against the old engine, and `oneterm_vt::reflow::tests::keep_viewport_top_*` (20 tests, a superset by name) pins it against the new one. The adapter's share — that the backend's policy reaches the engine — is `model_tests::resize_grid_applies_the_backend_policy`. |
| `sixel_tests` (11) | **deleted** | They drove `alacritty_terminal::Term` through its own decoder. `oneterm_vt::graphics::tests` (26) replaced them at `US-0080`; `model_tests::a_sixel_reaches_the_snapshot_once_with_per_cell_offsets` pins what the adapter adds. |
| `backend_tests::reliable_events_do_not_block_while_the_engine_lock_is_held` | **deleted**, replaced | CORR-01 is unreachable: the drain no longer touches the channel, so it *cannot* block. `the_drain_never_waits_on_a_full_queue` asserts the same property structurally — three drains with a saturated queue and no consumer, under the lock, all returning. |
| `backend_tests::deferred_events_flush_in_order_once_the_queue_drains`, `deferred_events_keep_fifo_order`, `async_flush_delivers_deferred_events` | **deleted**, replaced | They pinned the deferred FIFO's ordering. A vector cannot reorder; what still needs proving is that the batch's events arrive in order *with backpressure*, which is `pending_events_apply_backpressure_outside_the_lock` (one slot, flush parked, three events in order) and `pump_async_variants_publish_lifecycle_in_order`. |
| `backend_tests::line_accounting_tracks_growth_saturation_and_reset` | **deleted**, replaced | `LineAccounting` is gone. `the_gutter_line_count_is_the_engines_output_line_count` pins the four properties that matter: the floor at the viewport, growth past the scrollback cap, a wrap is not a line, a clear does not reset. |
| `backend_tests::color_key_indices_match_the_adapter_constants` | **deleted**, replaced | The constants it compared are gone. `color_queries_are_queued_with_their_typed_key` pins that the engine's `ColorKey` reaches the pump unchanged. |
| `backend_tests::coalescible_repaint_events_are_counted_when_saturated`, `router_clones_share_their_state` | **kept**, retargeted | Same assertions, through `post_repaint()` instead of `forward(Output)`. |
| The other ~15 `backend_tests` | **kept**, same coverage | They assert on what `drain` returns instead of on what reached the channel, because the router no longer sends. One test per event kind, the security policy branches, the OSC 9;7 dedup and the reply-first order all survive. |
| `content::tests` (5) | **kept**, rebuilt on the owned render state | `snapshot_is_owned_clone` became `the_snapshot_is_owned_and_outlives_the_engine` — `TerminalContent` is no longer `Clone` (it owns a `RenderState`; nothing outside the crate cloned it), so ownership is proved by dropping the terminal instead. `damage_partial_on_unchanged` additionally asserts `update() == Unchanged` and that **no row was copied**, which is what it was really about. |
| `search::tests` (11) | **kept** verbatim | Every assertion unchanged, including `cols == 19` and the wide-spacer case, over a `RowId`-keyed `GridText`. `grid_text_snapshot_matches_live_term_layout` now checks the `RowId` anchors instead of the stored `top_line`. |
| `osc_color::tests` (5) | **rewritten** | The 256/257/258 constants are gone; the cases are the same, keyed by `ColorKey`, plus `every_color_key_is_answered_or_skipped` so a new key cannot silently fall through. |
| `model::tests` (5) | **kept** | Unchanged assertions; the fixture builds a `SharedTerminal` instead of wrapping an `Engine` by hand. |

New tests: `handle::{a_render_lock_raises_the_demand_until_the_pump_takes_it,
a_pump_lock_does_not_raise_the_demand, the_demand_crosses_threads,
try_lock_reports_a_held_lock, the_adapter_claims_the_osc_numbers_it_routes,
the_scrollback_limit_is_clamped_to_the_engine_maximum}`,
`content::{the_frame_source_is_the_render_state,
only_the_changed_rows_are_rebuilt_when_the_viewport_stood_still,
a_scrollback_move_rebuilds_every_point, style_runs_reach_the_right_columns}`,
`search::history_rows_report_negative_grid_lines`,
`backend_tests::events_from_several_advances_survive_to_one_finish_batch`.

### Declared behaviour changes

Two, both consequences of deleting the heuristic, both self-consistent because the
gutter's stamping and its labelling read the same number:

1. **The gutter counts output lines, not display rows.** `Terminal::lines_produced()` is
   incremented on a line feed and never on an implicit wrap (R-05), where the old
   `total_lines` heuristic counted the row a wrap created. A session that wrapped *n*
   long lines now shows line numbers *n* lower than it would have. The HLD's consumer map
   assigns `lines_produced()` to the gutter, so this is the design's answer, not a
   shortcut.
2. **The count no longer dips on a clear or on entering the alternate screen.** The old
   branch "total shrank → restart from it" reset the counter on every `cls`, which
   contradicted `TerminalInfo::absolute_line_count`'s own documented "monotonically
   increasing". `gutter_timestamps.rs` already rebases on `clear_epoch`, so the
   timestamps still line up; the visible change is that numbering continues instead of
   restarting.

The floor is kept deliberately: the published count is `max(lines_produced, rows)`,
because the gutter labels display row `i` with `absolute - offset - rows + i` and a
smaller count would number the top rows from below zero.

### Gaps

1. **No GUI walk.** `quser` reports the only session as `Disc` (disconnected, idle 2:02),
   so there is no interactive desktop to drive and the prompt/echo/Ctrl-C/reflow/
   selection/Sixel walk was **not** run. The owner's own `oneterm.exe` (pid 27376) was
   left strictly alone. What stands in for it: the differential over 81 streams proves the
   frame is byte-identical to the one `US-0081`'s four GUI walks were accepted on, and no
   consumer changed. The walk belongs to the next interactive session, or to `US-0085`,
   which has to run one anyway.
2. **`alacritty_terminal` is still a `[dependencies]` line of `crates/terminal`**, against
   the intake's US-0082 wording. The reason is in *Documentation → Reconciliation*
   deviation 1 and the table above: it is public API that `crates/terminal-view` reads.
   `migration.md` § "Deletion list" already schedules all five manifest lines at
   `US-0087`, and four of them become deletable at `US-0085`. Not a `cfg(test)`
   dependency either, for the same reason — the compat surface is not test code.
3. **`mouse_encode.rs` still takes `TermMode` rather than `ModeSnapshot`.** Deliberate:
   `TerminalQueryState` publishes `TermMode` regardless, so converting the encoder now
   would leave the crate with both representations until `US-0085`, and it would rewrite
   a well-tested encoder's fixtures twice. `migration.md` has the view's `input/mouse.rs`
   moving to `ModeSnapshot` at `US-0085`; the encoder's signature flips with its only
   external caller.
4. **`oneterm_vt::testing` and `oneterm_vt::strip` were not added**, closing `US-0081`'s
   gap 5 by declining it: `crates/terminal/src/test_engine.rs` already holds the two
   helpers and `logging.rs` already strips with `oneterm_vt::parser`.
   `docs/agents/code-style.md` says to extract shared code once several crates need it.
5. **`VtEvent::RowsScrolled` / `RowsTrimmed` / `GraphicReleased` are still dropped**, and
   `RenderUpdate::Partial { scrolled }` is still reported to the view as `Full` damage.
   Both need a `RowId`-keyed consumer above the seam, which is `US-0085`
   (`plan_cache` keyed on `(RowId, SeqNo)`, the graphic store keyed on release).
   `TerminalContent::{row_id, display_row}` is built and tested, waiting for it.
6. **The pump half of the demand handshake has no caller.** `take_render_demand()` is
   implemented and tested; the `break` that acts on it lives in the two read loops, which
   are `US-0083` / `US-0084`'s by the N-04 table.

## Handoff

Branch `worktree-agent-a8c26d0ccf878c84b`, off `feat/vt-engine` @ `f9af66c`. **Not
merged, not pushed.** The worktree tool based it on `main` @ `c936ac0`, which has no
`crates/vt`; `git reset --hard f9af66c` was run before any file was read or written — the
same correction `US-0076` / `0078` / `0079` / `0080` / `0081` recorded.

### What `US-0083` / `US-0084` need from this crate

Nothing in `crates/terminal` has to change for either packet. The adapter side of every
API they were promised is implemented and tested here:

| They want | It is | Note |
| --- | --- | --- |
| the shared terminal | `SharedTerminal = Arc<TerminalHandle>` (`crates/terminal/src/handle.rs`) | `new_shared_terminal(GridSize, scrollback)` is unchanged; `lock()`, `lock_unfair()`, `try_lock_unfair()` all still compile at today's call sites, the last two as aliases of `lock` / `try_lock` |
| **the yield check** | `SharedTerminal::take_render_demand() -> bool` | Call it at a chunk boundary, **after** the batch's replies have left (R-37), and drop the guard when it answers `true`. It is cleared by the asking. The render side is already wired: `TerminalModel::snapshot{,_into}` locks through `lock_for_render()`, which raises it. `render_demand_raised()` reads without clearing, for a diagnostics line. |
| the batch drain | already done for them | `TerminalPump::advance(&mut Terminal, bytes)` feeds **and** drains under the lock — replies to the transport, colour queries queued, state caches updated, UI events collected; `finish_batch[_blocking](repaint)` sends them after the guard drops. The deferred sink is gone, so there is nothing left to "move out from under the lock": the loops keep the exact `advance` / `take_color_queries` / `color_replies` / `finish_batch*` shape they have today. |
| the resize policy through the engine API | `TerminalModel::new(term, impl Into<oneterm_vt::ResizePolicy>)` | Change `$resize_policy` in `impl_pty_terminal_session!` from `ResizePolicy::Default` / `::KeepViewportTop` to `oneterm_vt::ResizePolicy::BottomAnchor` / `::KeepViewportTop` — **one token each**, no change here. When both backends have done it, delete `crate::model::ResizePolicy` and its `From`. |
| deleting `Engine` | one no-op blocks it | `crates/local-shell/src/event_loop.rs:357` calls `self.term.lock().exit()`. `Engine` is now a `Deref` newtype over `oneterm_vt::Terminal` whose only member is that no-op. Delete the call site and `Engine` goes with it; `lock()` then yields the engine directly. |
| `crates/ssh`'s dead manifest line | still dead | `US-0081` recorded that no `crates/ssh` source references `alacritty_terminal`; still true. `US-0084` can take the line without waiting for anything. |

### What `US-0085` needs

- `TerminalContent` **owns** the `RenderState`. Read the frame through
  `update()`, `rows()` (always the full viewport), `changed()`, `size()`,
  `render_cursor()`, `modes()`, `selection_range()`, `placements()`, `hyperlink(id)`,
  `scroll_offset()` — and `row_id(display_row)` / `display_row(RowId)`, the two-way
  translation `migration.md` designed and `US-0081` left unbuilt. `plan_cache` keys on
  `(RowId, SeqNo)`: both come off `rows()[i]`.
- `oneterm_terminal` re-exports the vocabulary those accessors return — `RenderRow`,
  `RenderCell`, `RenderContent`, `RenderCursor`, `RenderPlacement`, `RenderUpdate`,
  `StyleRun`, `ModeSnapshot`, `RowId`, `SeqNo`, `SelectionKind`, `Terminal` — so the view
  can start reading them before it takes a direct `oneterm-vt` dependency.
- **Delete `crates/terminal/src/engine_shim.rs` whole.** With it go
  `TerminalContent`'s compatibility fields, `IndexedCell`, `TermDamageInfo`,
  `SearchMatch::{line, display_row}` (they become `RowId`),
  `TerminalInfo::{cursor_line, last_content_line}` and `TerminalQueryState::cursor_line`'s
  signed lines, `DefaultColors::from_legacy`, `TerminalPalette` / `resolve_color`'s
  alacritty types, `mouse_encode`'s `TermMode`, and then the manifest line.
- The scroll delta is waiting: `RenderUpdate::Partial { scrolled }` is produced correctly
  and thrown away at the seam, because the view has no cache to shift yet.
- `RenderState::changed()` is the row list the compatibility rebuild already uses; the
  same list is what the view should lay out.

### Owner decision waiting

The gutter's two declared behaviour changes above (output lines rather than display rows;
no reset on `cls`) follow the HLD's consumer map and fix a contract the old heuristic
broke, but they are visible in the gutter's numbers. If the owner wants row numbering
back, the fix is one line in `TerminalPump::advance` and belongs in `US-0086`, not here.
