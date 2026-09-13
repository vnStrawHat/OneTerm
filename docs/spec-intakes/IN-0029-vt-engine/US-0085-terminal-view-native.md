# Work: crates/terminal-view goes native

ID: US-0085
Intake: IN-0029
Created: 2026-09-13

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (the seam's value vocabulary)
- Risk lane: high_risk
- Spec Intake, when required: `IN-0029` — VT engine rewrite

## Outcome

`crates/terminal-view` builds its frame from the engine's `RenderState` and nothing
else. Concretely:

- `render/frame.rs` builds `Frame` / `FrameRow` / `Cell` directly from `RenderRow`
  plus its resolved `StyleRun`s — no `IndexedCell` rebuild, no dense per-frame copy,
  no display-offset fallback path.
- `render/plan_cache.rs` is keyed on `(RowId, SeqNo)`: an unchanged row reuses its
  shaped plan without hashing it, a scroll shifts the cache by `RowId`, and a
  `RenderUpdate::Unchanged` frame skips candidate selection, hashing and layout
  entirely. The `FrameStats` counters are the proof.
- cursor from `RenderState::cursor()`, selection from `selection_range()`,
  hyperlinks keyed on `HyperlinkId`, graphics painted from `placements()` /
  `graphic_offset()`, modes from `ModeSnapshot`.
- `input/mouse.rs` and `crates/terminal/src/mouse_encode.rs` speak
  `oneterm_vt::SelectionKind` and `ModeSnapshot` instead of `SelectionType` /
  `TermMode`.
- `theme/palette.rs` and `crates/terminal/src/palette.rs` speak the engine's
  `Rgb` / `Color` / `NamedColor`.
- `TerminalInfo` and `TerminalQueryState` publish **display rows** instead of the
  reference's signed grid lines; `SearchMatch` carries a `RowId`.
- `crates/terminal/src/engine_shim.rs` is deleted, and the ~9 ms per-flood legacy
  `Cell` rebuild `migration.md` attributes to this packet goes with it.
- `grep -rn alacritty_terminal crates/terminal crates/terminal-view` is empty except
  the `tests/us0081_parity.rs` dev-oracle and the one `[dev-dependencies]` manifest
  line that keeps it compiling, both retiring at `US-0087`.

## Scope

- [ ] In scope: all of `crates/terminal-view/src/**`; in `crates/terminal` the
  compatibility surface only — `content.rs`, `engine_shim.rs` (deleted), `model.rs`,
  `session.rs`, `search.rs`, `mouse_encode.rs`, `palette.rs`, `osc_color.rs`,
  `color_classification.rs`, `backend/state.rs`, `lib.rs`, `test_support.rs`,
  `Cargo.toml`, `tests/us0081_parity.rs`.
- [ ] In scope (one line, listed): `crates/local-shell/src/session_tests.rs:5` —
  `use alacritty_terminal::selection::SelectionType;` becomes
  `use oneterm_terminal::SelectionKind as SelectionType;`. The three call sites are
  unchanged because the variant names match. There is no re-export that can avoid
  this: the file imports the type from the fork directly, so the alias has to move
  with the deleted parameter type. `US-0083` deletes `crates/local-shell`'s
  `alacritty_terminal` manifest line and would have had to make the same edit.
- [ ] Out of scope: anything else in `crates/local-shell` and `crates/ssh`
  (`US-0083` / `US-0084` own them, concurrently), `crates/tools`, `crates/vt`
  behaviour changes, deleting the fork (`US-0087`), the deferred deviations
  (`US-0086`).

## Acceptance

- [ ] `grep -rn alacritty_terminal crates/terminal crates/terminal-view` matches
  only `crates/terminal/tests/us0081_parity.rs` and the `[dev-dependencies]` line
  that lets it build.
- [ ] `crates/terminal/src/engine_shim.rs` does not exist.
- [ ] `plan_cache` is keyed on `(RowId, SeqNo)`; an idle frame plans no row and
  hashes none, and a scroll replans only the scrolled-in rows.
- [ ] `RenderUpdate::Unchanged` reaches the plan cache as "nothing to do": no
  candidate scan, no URL scan, no layout.
- [ ] Every test in `crates/terminal` and `crates/terminal-view` passes unchanged,
  or is consciously rewritten with the reason recorded below.
- [ ] `cargo test -p oneterm-terminal --test us0081_parity` green, with the same
  five-difference allow-list.
- [ ] `pwsh scripts/ci-local.ps1` green.
- [ ] The four GUI walks reproduced from this worktree's `fast-dev` build with
  screenshots under `evidence/` prefixed `US-0085-`: IN-0018 render walk (including
  the render-sampler pixel comparison against the binary built from `main`),
  IN-0027 font/ligature/fallback walk, IN-0028 Sixel walk (including `cls` and a
  prompt below an image), US-0071 local-shell walk.
- [ ] The flood table re-measured (the ~9 ms legacy-cell rebuild gone) and frame
  time under `yes` for 10 s and `type` of a 10 MB file, old binary versus new.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md` — the `US-0085` packet row and
  the intake-level acceptance (`every existing test green or consciously
  rewritten`, `the GUI walks reproduced`).
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` — the consumer map
  ("How today's consumers map onto the new API"), `DEC-0015`'s render contract,
  phase 13's exit criteria.
- `.../low-level-design/migration.md` — `US-0085`'s may-touch/must-not-touch row,
  the deletion list, "The adapter contract, as `US-0082` shipped it", the tests
  table, the debug-build cost table whose last row is this packet's ~9 ms.
- `.../low-level-design/damage-and-render-state.md` — tri-state, changed rows,
  `StyleRun`, the watermark, `ModeSnapshot`.
- `.../low-level-design/graphics.md` — `RenderState::placements` /
  `graphic_offset`, the release signal.
- `.../low-level-design/selection.md`, `.../events-and-api.md`.
- `.../research/api-surface.md` § 9 — what the view reads.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/` — `RowPlan`,
  `plan_cache`, the painter, the diagnostics counters.
- `docs/spec-intakes/IN-0027-font-fallbacks-ligatures/` — `CellAnchor`, the
  fallback and ligature paths that must not change.
- `docs/spec-intakes/IN-0028-sixel-graphics/` — the painting contract the walk
  checks.
- `docs/terminal-backend.md` § 5 — the pump layer and the resize policy.
- `docs/agents/{code-style,error-policy,dependencies,crate-dependency-rules}.md`.

### Documentation Action

Update required:

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` —
  the frame pipeline consumes a render state instead of a viewport copy, and the
  plan cache is keyed by row identity rather than by a row hash
  (`migration.md` § "Documentation reconciliation" schedules this edit here).

No contract change elsewhere: `damage-and-render-state.md`, `graphics.md` and
`migration.md` already describe the shapes this packet consumes; nothing about the
engine's behaviour changes.

Reason: this packet moves a consumer onto an already-accepted contract. The only
owning doc that still describes the *old* consumer shape is IN-0018's HLD.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason
remains valid.

## Context

- The adapter contract is the one `US-0082` shipped: `TerminalContent` owns the
  `RenderState` and exposes `update/rows/changed/size/render_cursor/modes/
  selection_range/placements/hyperlink/scroll_offset/row_id/display_row`.
- `RenderCursor` carries no shape; the shape comes from
  `Terminal::cursor_style().shape` and has to be copied into the frame at refill.
- The view's own vocabulary (`Cell`, `Color`, `CellFlags`, `CursorShape`,
  `Selection`, `Damage`, `FrameRow`, `GridSize`) stays: it is what `row_plan`,
  `shapes`, `overlay`, `cursor` and the URL code are written against, and keeping
  it means the conversion boundary is still exactly one file.
- `query_line_range_cells` is the damage-free path (URL hover, completion). It
  cannot go through a `RenderState` — that copies a whole viewport per pointer
  move — so it keeps an owned per-cell shape, rebuilt on the engine's cell.
- `FakeTerminalSession` fabricated the compatibility surface by hand. `RenderState`
  has no public constructor for rows, so the fake gets a real `oneterm-vt`
  terminal behind it and feeds bytes into it. That is smaller than the fabrication
  it replaces and makes the fake's frames real frames.
- `tests/us0081_parity.rs` compares the two engines through the **old**
  `TerminalContent` shape. It absorbs the deleted conversion verbatim, so both
  sides are still compared field by field with the same five-difference allow-list,
  and the fork's manifest line moves to `[dev-dependencies]` for it.

## Plan

- [ ] Packet + harness row (this file) before any code.
- [ ] `crates/terminal`: delete the compatibility surface; the parity test absorbs
  the conversion; the fake session gets a real engine.
- [ ] `crates/terminal-view`: `frame.rs` on `RenderRow`, `plan_cache` on
  `(RowId, SeqNo)`, mouse/palette/URL/completion/graphics follow.
- [ ] Reconcile IN-0018's HLD.
- [ ] Measure and walk; record evidence.

## Decisions

No new decision. `DEC-0015` (absolute row ids and an incremental render state) is
the contract this packet finishes consuming; `DEC-0014` allows call sites to change
anywhere in the workspace.

## Verification Plan

- `cargo test -p oneterm-terminal` and `cargo test -p oneterm-terminal-view` —
  every suite, with the rewrites listed in Evidence.
- `cargo test -p oneterm-terminal --test us0081_parity` — the differential, same
  allow-list.
- `pwsh scripts/ci-local.ps1`.
- `cargo test -p oneterm-terminal --test us0081_parity flood_bench -- --ignored
  --nocapture` under `fast-dev`, old versus new.
- Frame time under `yes` for 10 s and a 10 MB `type`, read off the diagnostics
  counters, against a binary built from `main`.
- The four GUI walks, from this worktree's `fast-dev` build, own process only.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable,
partial, or failing.

## Handoff

Worktree `agent-ad731760c1d949455`. The tool based it on `main @c936ac0`, which has
no `crates/vt`; `git reset --hard d3c537b` was run before anything was read or
written, so the branch is `feat/vt-engine @ d3c537b` plus this packet's commits.

`US-0083` (`crates/local-shell`) and `US-0084` (`crates/ssh`) run concurrently.
The only file this packet touches in their crates is
`crates/local-shell/src/session_tests.rs:5` (one import line) — flagged above, and
flagged to `US-0083`, which deletes that crate's `alacritty_terminal` manifest line.

`US-0087` inherits: `crates/terminal/tests/us0081_parity.rs` and the
`[dev-dependencies] alacritty_terminal` line that builds it.
