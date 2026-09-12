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
- [x] `grep -rn "resize_keeping_viewport_top\|conhost_cursor_row\|LineAccounting" crates/` is
      empty (`migration.md` § Verification, `US-0082`).
- [x] No `alacritty_terminal` item is named outside `engine_shim.rs`, `content.rs`'s field
      declarations, `session.rs`'s trait signatures, `palette.rs` and `osc_color.rs` — the five
      files that *are* the compatibility surface.
- [x] `pwsh scripts/ci-local.ps1` green, raw totals recorded.
- [x] Measured, recorded, never gated: the `US-0081` flood (4 MiB through 4 KiB chunks, grid
      120x30, scrollback 10 000, `fast-dev`), feed and snapshot reported separately, before and
      after, on this machine.
- [~] GUI walk only if the desktop is interactive (`quser` Active): prompt, echo, Ctrl-C,
      reflow, selection, Sixel. Otherwise recorded as a gap.

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

Filled in after implementation.

## Handoff

Filled in after implementation.
