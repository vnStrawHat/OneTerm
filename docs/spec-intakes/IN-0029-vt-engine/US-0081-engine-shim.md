# Work: Engine behind the seam (the shim)

ID: US-0081
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [x] In progress
- [ ] Implemented
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
   adapter lock and drains the batch through `OscRouter::handle`, **`VtEvent::Reply` bytes
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
  - Graphics: `US-0080` is being implemented concurrently and has not landed, so the engine has
    no `take_graphics` and the snapshot's `graphics` vector is always empty. Recorded as a gap.

## Acceptance

- [ ] `cargo test --workspace` green: every existing test in `crates/terminal`,
      `crates/local-shell`, `crates/ssh` and `crates/terminal-view` passes unchanged or is
      consciously rewritten, with each rewritten or deleted test named here and its reason given.
- [ ] Zero behaviour diff at the seam: `TerminalContent`'s fields carry the same values in the
      same coordinate system (grid `Line.0` with `display_offset` applied by the view), the same
      `SessionEvent` sequence reaches the UI in the same order, and the ConPTY/SSH resize
      policies are unchanged.
- [ ] The ten `model.rs` `keep_viewport_top_*` / `default_grow_*` resize tests keep running
      against the **old** engine (R-44) until `US-0082`, alongside the engine's own
      `reflow::tests::keep_viewport_top_*`.
- [ ] `pwsh scripts/ci-local.ps1` green, raw totals recorded.
- [ ] Four GUI walks reproduced from a `fast-dev` build of this worktree, with fresh screenshots
      under `evidence/US-0081-*`: IN-0018's render walk, IN-0027's font walk, the US-0071
      local-shell walk (prompt, echo, Ctrl-C, resize reflow, CJK/emoji, exit), and IN-0028's
      Sixel walk **only if `US-0080` has landed** — otherwise recorded as a gap.
- [ ] Frame time under `yes` for 10 s and under `type` of a 10 MB file, measured before (the
      main-checkout `fast-dev` binary) and after, both recorded.
- [ ] No file outside `crates/terminal` changed except the bounded backend lines listed above,
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

Filled in at completion. One deviation is recorded up front, because it is a conflict inside the
governing docs rather than a choice:

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

- [ ] `crates/terminal/src/sync.rs` — the `FairMutex` newtype over `parking_lot::FairMutex`.
- [ ] `crates/terminal/src/engine.rs` — `Engine`, `SharedTerminal`, `new_shared_terminal`.
- [ ] `crates/terminal/src/engine_shim.rs` — `LegacySnapshot` and every value conversion.
- [ ] `crates/terminal/src/backend/` — pump on `feed` + `EventBatch`, router as a drain function,
      line accounting without `Dimensions`.
- [ ] `crates/terminal/src/model.rs` — every model operation on the new engine; the old resize
      functions and their ten tests kept `#[cfg(test)]` against the old engine (R-44).
- [ ] `crates/terminal/src/{content,search,logging}.rs` — snapshot, grid text, escape stripper.
- [ ] The two backends' bounded lines; the manifests; the graph policy.
- [ ] Docs, then verification.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Filled in after implementation.

## Handoff

Implementer: this worktree, branch `worktree-agent-a53e46421076a2d27`, based on `feat/vt-engine`
@ `458aa78` (the worktree tool based the worktree on `main` @ `c936ac0`, which has no
`crates/vt`; `git reset --hard 458aa78` was run before any file was read or written).
`US-0080` (graphics) is being implemented concurrently in another worktree; when it merges, the
snapshot's `graphics` vector and the Sixel GUI walk are wired here.
