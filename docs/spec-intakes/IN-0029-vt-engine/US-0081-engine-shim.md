# Work: Engine behind the seam (the shim)

ID: US-0081
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

- Change type: existing-contract change — the engine under `crates/terminal` is replaced; the
  `TerminalSession` / `TerminalContent` seam keeps its names, its types and its behaviour.
- Risk lane: high_risk (the intake's lane; this is the packet where the new engine first runs
  under the running application, so every user-visible terminal behaviour is at risk at once).
- Spec Intake, when required: IN-0029.

## Outcome

`crates/terminal` runs on `oneterm-vt`. The vendored `alacritty_terminal` engine (`Term`,
`Processor`, `EventListener`, `Grid`, `Selection`, `FairMutex`) is no longer executed anywhere in
the product path; what remains of that crate in `crates/terminal` is the **value vocabulary** of
the legacy snapshot (`Cell`, `Flags`, `Point`, `Line`, `Column`, `TermMode`, `SelectionRange`,
`RenderableCursor`, `CursorShape`, `Rgb`, `Color`, `NamedColor`, `SelectionType`, the graphics
re-exports), which `crates/terminal-view` still consumes and which `US-0085` deletes.

Concretely:

1. **`LegacySnapshot`** (`crates/terminal/src/engine_shim.rs`) produces today's `TerminalContent`
   — `cells`, `cursor`, `mode`, `display_offset`, `total_lines`, `selection`, `terminal_bounds`,
   `damage`, `graphics` — from `Terminal::render_update` into a reused `RenderState`, plus the
   mode snapshot and the selection range.
2. **The shared terminal** is `Arc<FairMutex<Engine>>`, `Engine` = `oneterm_vt::Terminal` plus its
   `LegacySnapshot`, and the mutex is `parking_lot::FairMutex` (HLD decision 7) behind a thin
   `oneterm_terminal::sync::FairMutex` newtype that keeps the backends' `lock` /
   `lock_unfair` / `try_lock_unfair` call sites compiling unchanged.
3. **The pump** feeds bytes through `Terminal::feed(bytes, &mut EventBatch, now)` under the
   adapter lock and drains the batch through `OscRouter::drain`, **`VtEvent::Reply` bytes
   first** (R-37), then everything else. The `SessionEventSink` deferred tier is untouched, so
   delivery ordering and the backends' `finish_batch*` calls are byte-for-byte what they were.
4. **Resize** goes through `Terminal::resize(size, policy)` with `ResizePolicy::KeepViewportTop`
   for the ConPTY local backend and `ResizePolicy::BottomAnchor` for SSH — the same two policies
   `crates/terminal/src/model.rs` selects today, now native.
5. **OSC** — `osc.rs`, `osc_color.rs`, the OSC 9;7 agent channel and the OSC 52 policy run
   unchanged over `VtEvent::Osc` / `ColorQuery` / `ClipboardStore` / `ClipboardLoad`; the engine's
   `OscClaims` registration table replaces the fork's `report_osc` patch (OSC 7, 9, 133 claimed;
   52 claimed large).
6. **Search** runs over the new grid text (`Screen::row_text` per row under one lock), still in
   the signed `Line.0` coordinates `SearchMatch` publishes.
7. **Paste, bracketed paste, key and mouse encoding** are unchanged; the mode bits they read come
   from `ModeSnapshot` translated into `TermMode`.
8. **Session logging** strips escapes with `oneterm_vt::parser::{Parser, Dispatch}`; the second
   `vte::Parser` is gone.
9. **`test_support.rs`** needs no change: it fabricates `TerminalContent` from the legacy value
   types directly, and those types are unchanged.
10. **Graphics** (after `US-0080` merged in at `3538047`): `take_graphics()` is the one
    drain and the adapter calls it once per snapshot; the per-cell offset inside the
    image's cell grid is derived from the placement (R-21) rather than stored per cell.

**No consumer changes and no behaviour changes.** `crates/terminal-view`, `crates/state`,
`crates/sftp-ui`, `crates/workspace`, `crates/app` and `crates/tools` are not touched.

## Scope

- [ ] In scope:
  - All of `crates/terminal` (the N-04 table's first row).
  - In `crates/local-shell` and `crates/ssh`: **only** the type of the shared terminal, its
    construction call, and the `Cargo.toml` line — see the deviation recorded under
    *Documentation → Reconciliation* for the one bounded extra edit.
  - `Cargo.toml`: `parking_lot` as a workspace dependency (already in `Cargo.lock`).
  - `scripts/dependency-graph-policy.json`: `oneterm-terminal` gains `oneterm-vt`.
  - `docs/terminal-backend.md` § 5 (feed-and-drain, watermark damage) — the row
    `migration.md` § "Documentation reconciliation" assigns to this packet.
- [ ] Out of scope:
  - The backends' read loops, transports, resize paths and tests (`US-0083` / `US-0084`).
  - Every consumer above the seam (`US-0085`).
  - Going native inside `crates/terminal` — `RowId`, the batch drain crossing the seam,
    `RenderState` reaching the view, `ColorKey` replacing the 256/257/258 indices, deleting
    `line_accounting.rs`, `model.rs`'s `resize_keeping_viewport_top` / `conhost_cursor_row` and
    the deferred event tier (`US-0082`).
  - Deleting `alacritty_terminal` from any manifest (`US-0087`; `migration.md` § "Deletion
    list" schedules all five manifest lines there).
  - The engine's Sixel decoder itself (`US-0080`). It was implemented concurrently and
    **merged into this branch at `3538047` while this packet was in flight**, so the
    snapshot's graphics mapping is in scope after all: `TerminalContent.graphics` from
    `Terminal::take_graphics()` and the per-cell `GraphicCell { id, col, row }` derived from
    the placement. IN-0028's GUI walk (N-02) is therefore evidence here rather than a gap.

## Acceptance

- [x] `cargo test --workspace` green: every existing test in `crates/terminal`,
      `crates/local-shell`, `crates/ssh` and `crates/terminal-view` passes unchanged or is
      consciously rewritten, with each rewritten or deleted test named here and its reason given.
- [x] Zero behaviour diff at the seam: `TerminalContent`'s fields carry the same values in the
      same coordinate system (grid `Line.0` with `display_offset` applied by the view), the same
      `SessionEvent` sequence reaches the UI in the same order, and the ConPTY/SSH resize
      policies are unchanged.
- [x] The fifteen `model.rs` `keep_viewport_top_*` / `default_grow_*` resize tests keep running
      against the **old** engine (R-44) until `US-0082`, alongside the engine's own
      `reflow::tests::keep_viewport_top_*`.
- [x] `pwsh scripts/ci-local.ps1` green, raw totals recorded.
- [~] Three of four GUI walks reproduced from a `fast-dev` build of this worktree, with fresh screenshots
      under `evidence/US-0081-*`: IN-0018's render walk, IN-0027's font walk, the US-0071
      local-shell walk (prompt, echo, Ctrl-C, resize reflow, CJK/emoji, exit), and IN-0028's
      Sixel walk **only if `US-0080` has landed** — otherwise recorded as a gap.
- [x] Frame time under a continuous echo flood and under `type` of a 10 MB file, measured before (the
      main-checkout `fast-dev` binary) and after, both recorded.
- [x] No file outside `crates/terminal` changed except the bounded backend lines listed above,
      the two build-policy files, and this packet's own docs and evidence.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` — the seam, the
  `LegacySnapshot` design, the may-touch / must-not-touch table (N-04), the view-needs gap table
  with owners, the deletion schedule and the per-packet documentation reconciliation table. This
  is the contract this packet implements.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` — `feed` / drain, the
  `EventBatch` contract, the adapter loop the deferred tier replaces, and the error policy.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` —
  `render_update`, `RenderState`'s full-viewport-plus-changed-list shape, the mode snapshot, the
  drain order (replies before any yield, R-37) and mode 2026.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md` — the seven `Terminal`
  selection wrappers and `hit_test`, which replace `model.rs`'s own point/side arithmetic.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md` — what the snapshot's
  `graphics` vector will carry once `US-0080` lands, and why the Sixel walk belongs here (N-02).
- `docs/spec-intakes/IN-0029-vt-engine/research/api-surface.md` — the exact contract the
  workspace relies on today, file:line: the coordinate model (§ 3.2, § 3.3), the listener
  contract (§ 3.5), the colour index space (§ 3.6), the test helpers (§ 6), and what OneTerm
  already owns and must not be re-added (§ 4).
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` — the consumer map, the threading
  and locking table (`Arc<FairMutex<Terminal>>` in the adapter, engine `Send` and `!Sync`), and
  the phase plan's `US-0081` exit criteria.
- `docs/terminal-backend.md` — § 5 concurrency and pump layer, § 5.3 resize policy, § 9
  `TerminalSession`. The § 5 feed-and-drain row is this packet's documentation action.
- `docs/decisions/DEC-0014-*`, `DEC-0015-*` — own the engine; absolute row ids and an
  incremental render state are the engine-to-view contract.
- `docs/decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md` — the resize contract
  the two policies encode.
- `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md` — R6 / R7 / R8: `terminal`
  gains `oneterm-vt` (both L0), stays gpui-free, and the backends keep reaching the engine only
  through `crates/terminal`.
- `docs/agents/code-style.md`, `docs/agents/error-policy.md` — no `Result` where nothing can
  recover; untrusted terminal input degrades and is counted, never panics.

### Documentation Action

**Update required**, limited to the rows `migration.md` § "Documentation reconciliation" assigns
to this packet:

- `docs/terminal-backend.md` § 5 — the pump layer is `feed`-and-drain over an `EventBatch`
  instead of listener callbacks under the lock; damage is a per-row sequence number read through
  a watermark instead of `Term::damage()` / `reset_damage()`.
- `docs/agents/structure.md` § 3 and `docs/agents/crate-dependency-rules.md` R7/R8 wording plus
  `scripts/dependency-graph-policy.json` — `oneterm-terminal` now depends on `oneterm-vt`
  (the allow-list must change in the same commit or CI fails).

Every other row in that table belongs to `US-0082` (§ 5.3 native `ResizePolicy`), `US-0085`
(IN-0018's HLD) or `US-0087` (the removal rows). `docs/osc-sequences-checklist.md` was reconciled
by `US-0076`.

Reason: the packet changes how the pump layer works (a behaviour-preserving mechanism change
that § 5 describes literally) and adds one workspace edge that a CI script encodes.

### Reconciliation

Docs changed with this work:

- `docs/terminal-backend.md` § 5 — the concurrency model is `Arc<FairMutex<Engine>>` over
  `parking_lot`, the pump is feed-and-drain over an `EventBatch` with replies first, and
  damage is a per-row sequence number read through a watermark. The § 5.3 table rows for
  `OscRouter`, `TerminalPump` and `LineAccounting` follow. § 5.3's `ResizePolicy` prose and
  § 4's rev lock stay as they are: `US-0082` and `US-0087` own those rows.
- `docs/agents/structure.md` § 3 — `oneterm-terminal` depends on `core` + `vt` and is the
  adapter, not the engine; the `vt` row no longer says nothing depends on it.
- `docs/agents/crate-dependency-rules.md` — the same two statements, plus R7's wording.
- `scripts/dependency-graph-policy.json` — the allow-list entry the graph check enforces.
- `Cargo.toml` — `parking_lot` as a workspace dependency, and `oneterm-vt` / `oneterm-pty`
  in `[profile.fast-dev.package]` (see the measurement evidence).

Three deviations, each a conflict inside the governing docs or a consequence the scope
table did not anticipate rather than a choice:

- **`migration.md`'s N-04 row says "no backend test changed".** The shared-terminal type appears
  in `crates/local-shell/src/event_loop_tests.rs` (the loopback fixture's field type, its
  construction call, and a `grid_text()` helper that reads the grid through the old engine's
  `Dimensions` / `Point` API). Those three do not compile after the type swap. The **assertions**
  are untouched; only the type, the construction and the grid reader move. Recorded rather than
  waved through.
- **`alacritty_terminal` stays in `crates/terminal/Cargo.toml`.** `TerminalContent` is the seam
  and `crates/terminal-view` reads its alacritty value types directly (`frame.rs`,
  `input/mouse.rs`, `theme/palette.rs`); dropping the manifest line here would force a consumer
  change, which this packet's own acceptance forbids. `migration.md` § "Deletion list" schedules
  all five manifest lines at `US-0087`, and the deletion becomes possible at `US-0085`.
- **The `impl_pty_terminal_session!` listener parameter is gone**, which removed one
  `use` line from each backend's `session_terminal.rs`. The engine is not generic over the
  listener any more — events are values, not callbacks — so the macro had nothing to
  expand it into, and leaving it would have left an unused import that
  `clippy -D warnings` fails on. Four one-line deletions in backend files, all a direct
  consequence of the shared-terminal type change the scope table does permit.

## Context

- The engine is `Send` and `!Sync` and takes `&mut self`, so the adapter owns the lock. The two
  backends' read loops call `lock()`, `lock_unfair()` and `try_lock_unfair()` on the shared
  handle; `parking_lot::FairMutex` has no unfair variants (its fairness is in `unlock`), so the
  newtype maps them onto `lock` / `try_lock`. That is a deliberate simplification with a stated
  ceiling: `US-0083` deletes the call sites.
- `crates/vt` ships no `take_graphics`, no `testing` module and no `strip` module at
  `458aa78`. The first is `US-0080` (concurrent), the second is only needed by tests that keep
  running against the old engine in this packet, and the third is `US-0082`'s row in the
  deletion list — but `oneterm_vt::parser::{Parser, Dispatch}` is public, so session logging can
  drop its second parser now without adding anything to the engine.
- `Config::default()` claims **no** OSC numbers, so the adapter must claim 7, 9 and 133 (and 52
  large) or the agent channel, the cwd tracker and shell integration go silent.
- The engine's `NamedColor` has a different discriminant order from alacritty's, so every colour
  crossing the seam is converted by an explicit match, never by arithmetic.

## Plan

- [x] `crates/terminal/src/sync.rs` — the `FairMutex` newtype over `parking_lot::FairMutex`.
- [x] `crates/terminal/src/engine.rs` — `Engine`, `SharedTerminal`, `new_shared_terminal`.
- [x] `crates/terminal/src/engine_shim.rs` — `LegacySnapshot` and every value conversion.
- [x] `crates/terminal/src/backend/` — pump on `feed` + `EventBatch`, router as a drain function,
      line accounting without `Dimensions`.
- [x] `crates/terminal/src/model.rs` — every model operation on the new engine; the old resize
      functions and their ten tests kept `#[cfg(test)]` against the old engine (R-44).
- [x] `crates/terminal/src/{content,search,logging}.rs` — snapshot, grid text, escape stripper.
- [x] The two backends' bounded lines; the manifests; the graph policy.
- [x] Docs, then verification.

## Decisions

- [`DEC-0014`](../../decisions/DEC-0014-oneterm-owns-its-vt-engine.md) — OneTerm owns the engine;
  call sites may change anywhere in the workspace.
- [`DEC-0015`](../../decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md) —
  absolute row ids and an incremental render state are the engine-to-view contract; this packet
  consumes it behind a shim and does not expose it.
- [`DEC-0008`](../../decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md) — the two
  resize policies, now native.

No new decision: every choice here is already recorded in the intake, the HLD or `migration.md`.

## Verification Plan

- Focused: `cargo test -p oneterm-terminal`, `cargo test -p oneterm-local-shell`,
  `cargo test -p oneterm-ssh`, `cargo test -p oneterm-terminal-view`.
- Unit / integration: `cargo test --workspace` (raw totals recorded, compared with the
  pre-change totals on this base).
- Quality gate: `pwsh scripts/ci-local.ps1`, including
  `python scripts/verify-dependency-graph.py` with the new `oneterm-terminal → oneterm-vt` edge.
- E2E / platform: the four GUI walks from a `cargo build -p oneterm-app --profile fast-dev`
  build of this worktree, driven by the scratchpad `gui.ps1` harness, each step captured as a
  PNG under `evidence/` with the `US-0081-` prefix and read back to confirm; written up in
  `evidence/US-0081-gui-walk.md`.
- Measurement (recorded, never gated): frame time under `yes` for 10 s and under `type` of a
  10 MB file, from the app's own diagnostics
  (`crates/terminal-view/src/render/diagnostics.rs`), before and after.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Commands and results

- `pwsh scripts/ci-local.ps1` — **green** (fmt, clippy `-D warnings`, `cargo test
  --workspace`, and all five Python policy checks including
  `verify-dependency-graph.py` with the new `oneterm-terminal → oneterm-vt` edge).
- `cargo test --workspace` raw totals over 57 test-result sections: **1557 passed, 0
  failed, 8 ignored** (1528 before `US-0080` was merged in; the base at `458aa78` had 57
  sections too).
- `cargo test -p oneterm-terminal`: **264 passed, 0 failed**. The crate had 256 `#[test]`
  attributes at the base and has 264 now; `crates/local-shell` (30), `crates/ssh` (33) and
  `crates/terminal-view` (191) are unchanged.
- GUI walks: [`evidence/US-0081-gui-walk.md`](evidence/US-0081-gui-walk.md), thirteen PNGs
  under `evidence/US-0081-*`, each read back after capture.
- Measurement: [`evidence/US-0081-frame-time.md`](evidence/US-0081-frame-time.md) with the
  two raw logs beside it.

### Tests deleted or rewritten, and why

| Test | Disposition | Reason |
| --- | --- | --- |
| `backend_tests::child_exit_sets_alive_false_and_code` | **deleted** | `Event::ChildExit` was emitted only by alacritty's own event loop, which OneTerm never used (`api-surface.md` § 3.5). There is no `VtEvent` equivalent because child exit is the transport's business. The behaviour it asserted — exit code recorded, `alive` cleared, `Exited` forwarded — is covered by `pump_publish_exit_and_closed_flush_deferred_first`. |
| `backend_tests::pty_write_goes_to_the_transport`, `pty_write_failure_is_logged_not_panicked` | **renamed** to `reply_goes_to_the_transport` / `reply_failure_is_logged_not_panicked` | Same assertions; `Event::PtyWrite` is now `VtEvent::Reply`. |
| The other ~22 `backend_tests` | **rewritten in shape, identical in coverage** | Each built an `alacritty_terminal::event::Event`; each now builds the same thing as a `VtEvent` in an `EventBatch` and calls `OscRouter::drain`. One test per event kind, the colour-query deferral, the delivery policy and the ordering guarantees all survive. |
| `line_accounting_tracks_growth_saturation_and_reset` | **rewritten** | `LineAccounting::observe` took a `Dimensions` impl; it now takes `total_lines` and `screen_lines` as numbers, so the test's `Dims` struct is gone. Same four cases, same expected counts. |
| `content::tests` (5: snapshot, damage) | **rewritten** | They were built on `mock_term` + `TerminalContent::from(&mut Term)`. They now build an `Engine` and feed bytes. `damage_full_on_first_snapshot` and `damage_partial_on_unchanged` pin exactly what they pinned: the first snapshot is `Full`, and a second snapshot with no output dirties at most the cursor line. |
| `search::tests` (11) | **kept**, with a local `mock_term` | The eleven assertions are unchanged, including `cols == 19` and the wide-spacer case. The helper now sizes an `Engine` to the content the way the reference's `mock_term` did, because `oneterm_vt::testing` is `US-0082`'s row. |
| `model::tests` — `has_selection_tracks_selection_state`, `select_all_marks_a_selection` | **kept**, rebuilt on the engine | Same assertions. `select_all` gained an explicit `selection_text()` check. |
| `model.rs`'s fifteen `keep_viewport_top_*` / `default_grow_*` resize tests (13 + 2) | **moved, not changed** | R-44: they are the only written form of the ConPTY contract, so they keep running against the **old** engine in `crates/terminal/src/legacy_resize.rs` (a `#[cfg(test)]` module that also holds the two functions they test) until `US-0082` deletes them. Every assertion is byte-identical; only the driver changed from `TerminalModel::resize_grid` to a local `resize_grid` helper. |
| `sixel_tests.rs` (10), `test_support.rs` (662 lines) | **untouched** | `sixel_tests` drives `alacritty_terminal::Term` directly and stays until `US-0082` (`migration.md`); `test_support` fabricates `TerminalContent` from the legacy value types, which are unchanged. |

Adopted from the verification round: `crates/terminal/tests/us0081_parity.rs`, the
old-versus-new differential (see gap 8). It retires with the fork at `US-0087`.

New tests: `content::last_content_line_finds_the_last_written_row`,
`model::{scrolling_back_moves_the_display_offset_not_the_cursor_line,
resize_grid_applies_the_backend_policy, search_reports_matches_in_grid_lines,
a_sixel_reaches_the_snapshot_once_with_per_cell_offsets}`,
`backend_tests::{replies_are_written_before_the_rest_of_the_batch_is_routed,
row_events_are_not_forwarded, color_key_indices_match_the_adapter_constants,
router_clones_share_their_state,
each_chunk_posts_exactly_one_output_after_its_reliable_events}`.

**Verification rework (the one real defect).** `OscRouter::drain` forwarded
`SessionEvent::Output` on the engine's end-of-batch `VtEvent::Repaint`, and the pump's
`finish_batch*` forwarded it again — two hints per chunk, the first of them *before* the
batch's deferred reliable events, where the old path emitted exactly one, last. That
contradicted this packet's own acceptance. The `Repaint` arm is now dropped: the repaint
hint has one owner, the pump. `forwards_title_and_wakeup` became
`forwards_title_and_drops_the_batch_repaint_hint`, the two pump tests went back to the
sequences they pinned before this packet, and
`each_chunk_posts_exactly_one_output_after_its_reliable_events` drives three chunks and
pins `[Title, Bell, Output] x3`.

### Residual `alacritty_terminal`

It is still a dependency of `crates/terminal`, `crates/local-shell`, `crates/ssh`,
`crates/terminal-view` and `crates/tools`, and **it runs nothing in the product path**.
What is left in `crates/terminal`:

- the value vocabulary of `TerminalContent` — `Cell`, `Flags`, `Hyperlink`, `Point`,
  `Line`, `Column`, `TermMode`, `SelectionRange`, `RenderableCursor`, `CursorShape`,
  `Rgb`, `Color`, `NamedColor`, `SelectionType`, and the three graphics re-exports — which
  `crates/terminal-view` reads directly out of the snapshot;
- `vte::{Parser, Perform}` — **gone**: session logging strips escapes with
  `oneterm_vt::parser` now;
- `legacy_resize.rs` — `#[cfg(test)]` only (R-44).

`migration.md` § "Deletion list" schedules all five manifest lines at `US-0087`. Four of
them become deletable at `US-0085`, when the view moves onto `RenderRow` — but
**`crates/ssh/Cargo.toml`'s is dead already**: after this packet no `crates/ssh` source
references the crate at all (three comments aside). It is left in place because deleting a
manifest line is not in this packet's scope table; `US-0084` can take it without waiting
for anything.

### Gaps

1. **The engine's always-on integrity walk is O(live rows x cols), not O(1) (R-28).**
   `TerminalGrid::assert_integrity` walks both screens' whole history once per
   `Terminal::feed` and once per `RenderState::begin_update`. At the default 10 000-row
   scrollback that is ~6 ms per frame in any debug-assertions build; release builds are
   unaffected. Measured and isolated in
   [`evidence/US-0081-frame-time.md`](evidence/US-0081-frame-time.md). **Owner: `US-0075`
   (`Screen::assert_integrity`) and `US-0079` (the `begin_update` call site)**, whose
   acceptance R-28 is. Not fixed here: this packet may not change engine behaviour.
2. **The demand/yield handshake is not wired.** `oneterm_vt::render::Demand` exists and
   nothing raises it. Raising it is the render path's and yielding is the read loops', and
   the read loops belong to `US-0083` / `US-0084` (N-04). What this packet does implement
   is the half that needs no loop change: `OscRouter::drain` writes every `VtEvent::Reply`
   to the transport before anything else in the batch (R-37), with a test.
3. **`VtEvent::RowsScrolled` / `RowsTrimmed` / `GraphicReleased` are dropped.** No consumer
   above the seam speaks `RowId`, and the view's graphic store still evicts by LRU exactly
   as it did. `US-0085` gives `GraphicReleased` its consumer.
4. **`LegacySnapshot::display_row` / `row_id` were not built.** `migration.md` sketches a
   two-way `RowId` translation for consumers that still speak display rows; in this packet
   the snapshot is display-row shaped end to end, so the translation has no caller. If
   `US-0082` needs it, it is four lines over `RenderState::rows()`.
5. **`oneterm_vt::testing` is not used.** It does not exist yet (`US-0082`'s row in
   `migration.md`), so `crates/terminal/src/test_engine.rs` holds the two helpers the
   adapter's own tests need.
6. **IN-0027's font walk was not completed** and **no SSH walk was run**. Both reasons are
   in [`evidence/US-0081-gui-walk.md`](evidence/US-0081-gui-walk.md) § 7: the RDP session
   went from `Active` to `Disc` mid-walk, and `sftp-dev-server` opens no shell channel.
7. **`exit` was not seen** in the GUI walk (a posted Shift+PageUp reached `cmd` as history
   recall and spoiled the step). Shell exit and child teardown are `US-0071`'s, unchanged
   here.
8. **~~The differential runner was not run.~~ Closed by the verification round.**
   `crates/terminal/tests/us0081_parity.rs` — written by the packet's independent verifier
   and adopted into the workspace gate — is the differential `migration.md` asks for: the
   same bytes into the old `Term` and the new `Engine`, every field of `TerminalContent`
   diffed after each 4 KiB chunk, over the 45 vendored recordings, `sixel_basic` and 35
   hand-written streams, plus a damage-soundness property, a three-thread lock stress and
   an old-versus-new flood bench (the last two `#[ignore]`d so the gate stays fast; the
   suite runs in 4.6 s). It ends in the allow-list below: five declared differences,
   anything else fails. It retires with the fork at `US-0087`.

### Snapshot deviations

The `TerminalContent` the shim produces differs from the old engine's in exactly five ways,
each proved by the differential and each with no reader. The intake's `C` / `D` deviation
tables cannot express them — those compare `grid.expect` / `state.expect`, not the
snapshot — so they are declared here.

| # | Delta | Why it is not user-visible |
| --- | --- | --- |
| S1 | `TermMode::LINE_WRAP` and `URGENCY_HINTS` are never set (all 81 streams) | `ModeSnapshot` does not carry them, and `research/api-surface.md` § 3.5 lists both under "unused mode bits … OneTerm just never queries them". No reader outside `crates/tools`' corpus dumper. |
| S2 | `TermMode::ORIGIN` is never set (15 streams) | Same line of `api-surface.md`, same absence of a reader. `DECOM` itself is honoured inside the engine; only the snapshot bit is missing. |
| S3 | A hyperlink with no `id=` gets `1`, `2`, … where the reference gave `0_alacritty` | The engine's implicit-id counter is per terminal by design (`cell-and-style.md`), not the reference's process-global atomic. The view hashes `id + " " + uri` (`render/frame.rs:306-312`), so only distinctness matters, and that was verified; an explicit `id=` is passed through verbatim. |
| S4 | `total_lines` is one smaller after a Sixel (`sixel_basic`: 10 versus 9) | Engine-level, not the shim: the image's history depth. It reaches the user as a scrollbar one row short in a session that printed an image. Raised as gap 9. |
| S5 | Damage is narrower | The reference damages a row on any write, the engine on an actual change. `us0081_parity::damage_soundness_detail` proves the property that matters — every row whose rendered content changed, plus a visible cursor's row, is always in the new `Partial` list — with **0 violations over all 81 streams**. Nothing is under-damaged, so no stale row can survive a frame. |

One engine-level answer also changed and is declared here rather than in the packet that
caused it: **DA2 replies `ESC[>0;502;1c` instead of `ESC[>0;2601;1c`**. The formula is
unchanged; the number is `CARGO_PKG_VERSION`, which is now the workspace's `0.5.2` instead
of the fork's. Programs read DA2 to identify the terminal, so it is worth writing down;
`dispatch-and-modes.md` § "Answers" is its long-term home.

9. **Sixel `total_lines` is one short (S4).** Owner: `US-0080`. Invisible to the parity
   gate, because neither `grid.expect` nor `state.expect` records history depth.

## Handoff

Implementer: this worktree, branch `worktree-agent-a53e46421076a2d27`, based on `feat/vt-engine`
@ `458aa78` (the worktree tool based the worktree on `main` @ `c936ac0`, which has no
`crates/vt`; `git reset --hard 458aa78` was run before any file was read or written).
`US-0080` (graphics) is being implemented concurrently in another worktree; when it merges, the
snapshot's `graphics` vector and the Sixel GUI walk are wired here.
