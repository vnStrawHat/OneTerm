# Work: Dispatch and modes

ID: US-0076
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (the semantic layer of the new VT engine)
- Risk lane: high_risk — this is the parity gate of the whole engine rewrite; a silent
  behaviour loss here is invisible until a user hits it in a real session.
- Spec Intake, when required: IN-0029

## Outcome

`oneterm-vt` gains a `Terminal` type that is parser + dispatch + grid + render hand-off, and
**the 45-recording parity gate is green**: every vendored alacritty reference recording replays
through the new engine and matches the frozen old-engine `grid.expect` and `state.expect`
cell-exactly, except cells covered by an `expected-diffs.json` window naming a correction id
(C1-C14). An undeclared difference, or a declared window that stops differing, fails.

## Scope

- [ ] In scope:
  - `crates/vt/src/terminal/` — `Terminal`, `Config`, the `Dispatch` implementation, the mode
    table, the colour model, the OSC registry, the answer set, the title stack, the kitty
    keyboard flag stack, `RIS` / `DECSTR`, charsets, tab stops, `DECSC`/`DECRC`, the sync flag,
    cursor style, `lines_produced`, `render_update`.
  - `crates/vt/src/lib.rs` — module line and re-exports.
  - `crates/vt/src/events/vt_event.rs` — `VtEvent::ColorQuery` (the LLD's event list carries it;
    `US-0079` deferred it to this packet) and `FeedStats::hyperlink_table_exhausted`.
  - `crates/vt/src/intern.rs` — the `HyperlinkTable` bound ladder and `clear()` for `RIS`; the
    `implicit` flag an implicit-id link needs so the parity encoder can renumber it the way the
    old engine's `_alacritty` suffix is renumbered. The LLD assigns the ladder to this packet.
  - `crates/vt/Cargo.toml` — the one-line `vt-paranoid` feature entry (`testing-and-bench.md`
    M12).
  - `crates/tools/` — `vt-corpus check --engine new`, the new-engine replay, and the `vt-diff`
    old-versus-new differential binary.
  - `crates/vt/tests/corpus/alacritty-ref/<name>/expected-diffs.json` for exactly the recordings
    the deviation tables list, measured.
- [ ] Out of scope:
  - Sixel / graphics (`US-0078`): `DCS` is parsed, dropped and counted.
  - Selection (`US-0078`), `pub mod testing` / `pub mod strip` (`US-0080`+), the adapter swap
    (`US-0081`+).
  - Reflow itself (`US-0077`, merged): this packet only calls `TerminalGrid::resize`.
  - Additive features assigned to `US-0086` by the deviation table (D7 DECXCPR, D8 XTVERSION,
    D10 `modifyOtherKeys`, D12 reverse wrap) — but see Gaps: the CSI table, the answers table and
    the HLD's `US-0076` exit criteria all require D7/D8/D10, so they are implemented here and the
    packet-column conflict is recorded rather than resolved.

## Acceptance

- [ ] `vt-corpus check --engine new` green on all 45 recordings.
- [ ] `crates/tools/tests/corpus_check.rs` runs the new engine inside `cargo test --workspace`.
- [ ] Every difference is covered by a declared `expected-diffs.json` window naming a correction
      id; no stale window.
- [ ] Every test the LLD's Verification list names exists and passes.
- [ ] The trap-map rows owned by `dispatch` (12, 20, 21, 22, 25, 26, 38, 39, 40, 42, 43) pass.
- [ ] `vt-diff` feeds the same bytes to both engines and diffs in `grid.expect` form; run over
      all 45 recordings and over the bench fixtures, with the result reported.
- [ ] No `unsafe`, no new external dependency.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` — this packet's
  contract: every CSI/ESC/OSC default, the mode table and its DECRQM answers, the answer set, the
  colour model, the OSC registration table, the hyperlink ladder, the title and keyboard stacks,
  `RIS`/`DECSTR`, and the deviation and correction tables.
- `.../low-level-design/parser.md` — the `Dispatch` trait, `DEL` is execute-no-op, and the
  "`US-0076` must re-split parameter 16 on `;`" note (P9).
- `.../low-level-design/grid-and-scrollback.md` — the grid primitives this layer calls, and
  corrections C1-C4, C8, C10, C12-C14 with their measured recordings.
- `.../low-level-design/cell-and-style.md`, `.../damage-and-render-state.md`,
  `.../events-and-api.md` — `Style`/`Attrs`, `EngineView`, `RenderState::begin_update`,
  `EventBatch`/`VtEvent`, `SyncState`, `ModeSnapshot`, the `feed` contract.
- `.../low-level-design/testing-and-bench.md` — the parity harness, `expected-diffs.json`, the
  state snapshot fields, the trap-map rows, the `vt-diff` differential, and the `vt-paranoid`
  manifest entry this packet owns.
- `.../low-level-design/reflow-and-resize.md` — `TerminalGrid::resize(size, ResizePolicy)`.
- `.../research/engine-semantics.md` §2 and §8, `.../research/api-surface.md` §3 — every
  sequence and event OneTerm relies on.
- `.../high-level-design.md` — P19, P20, P27-P30 and the `US-0076` phase row.
- `docs/osc-sequences-checklist.md` — the checklist the byte-feed tests must not drift from.
- `docs/terminal-backend.md` — the backend contract the engine must keep answerable.
- `docs/decisions/DEC-0014-*`, `DEC-0015-*` — the typed-key and absolute-row-id contracts.
- `docs/agents/{code-style,error-policy,dependencies}.md`, `AGENTS.md`, `docs/PROJECT.md`.

### Documentation Action

- Update required: `docs/osc-sequences-checklist.md` — `migration.md` line 241 assigns its
  "real coverage, and the three stale statements fixed" to this packet.
- No contract change for the LLDs: they are the accepted contract and this packet implements
  them. Ambiguities and conflicts found while implementing are recorded in Gaps, not edited into
  the design (the brief forbids editing IN-0029.md, the HLD or any LLD).

Reason: the engine's OSC coverage becomes real in this packet, so the checklist that documents it
is the one owning doc whose statements change.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- The worktree was branched from `main` (`c936ac0`) without `crates/vt`. It was
  `git reset --hard 0163e86` (feat/vt-engine, parser core), and then, when `US-0077` landed
  mid-read, `git reset --hard 4b833a0` — before any file was written. `TerminalGrid::resize(size,
  ResizePolicy) -> ResizeOutcome` is therefore present and the resize dispatch targets it
  directly; no `resize_rows` placeholder is needed.
- The parity encoding is the constraint that shapes the colour model: `state.expect` carries
  `palette.{index}` over the old engine's 269-slot table (0-255 indexed, 256 foreground, 257
  background, 258 cursor, 259-266 dim, 267 bright foreground, 268 dim foreground). `ColorKey`
  is the typed surface (D1); the storage is that same index space so the frozen files compare
  without a translation table.
- `grid.expect` lifts `WRAPLINE` from the last cell to the row (deviation G1). The reference
  *clears* the flag whenever that cell is overwritten or erased, because it assigns the whole
  template `flags`; the new row flag is cleared only on reset. Any divergence is measured.
- `CUU`/`CUD`/`CNL`/`CPL` route through the reference's `goto`, so under `DECOM` they are
  re-offset by the region top and clamped to its bottom. Reproduced with i32 arithmetic in this
  layer, because `Screen::goto_origin` cannot represent a negative intermediate line.

## Plan

- [x] Read the owning docs, the existing `crates/vt` surface, the corpus harness, and the two
      vendored reference files.
- [ ] Write the packet and mirror the story row into `harness.db`.
- [ ] `crates/vt/src/terminal/{mod,mode,color,osc,dispatch}.rs`, plus `terminal_tests.rs`.
- [ ] New-engine corpus replay in `crates/tools`, `--engine new`, `vt-diff`.
- [ ] Get `selective_erasure` green, then the rest, keeping a per-recording table.
- [ ] Measure and write the `expected-diffs.json` files.
- [ ] `pwsh scripts/ci-local.ps1`; save the parity gate output to
      `evidence/US-0076-parity-gate.md`.

## Decisions

- `DEC-0014` — the intake's own decision record.
- `DEC-0015` — absolute row ids and the incremental render state.

## Verification Plan

- Unit: `cargo test -p oneterm-vt terminal::` — every test named in the LLD's Verification list,
  one byte-feed test per sequence marked supported in `docs/osc-sequences-checklist.md`, and the
  dispatch-owned trap-map rows.
- Integration: `cargo test -p oneterm-tools` — the 45-recording parity gate against the frozen
  expectations, through the new engine.
- Differential: `vt-diff` over all 45 recordings and over the bench fixtures.
- Regression: `pwsh scripts/ci-local.ps1` in full.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To be filled after implementation. Parity gate output: `evidence/US-0076-parity-gate.md`.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
