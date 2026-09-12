# Work: Dispatch and modes

ID: US-0076
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
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
  - The seven `Terminal::selection_*` wrappers and the `invalidated_by`-before-operation
    obligation — added to scope mid-packet when `US-0078` landed (`selection.md` § Interfaces
    assigns them here).
  - Reflow itself (`US-0077`, merged): this packet only calls `TerminalGrid::resize`.
  - Additive features assigned to `US-0086` by the deviation table (D7 DECXCPR, D8 XTVERSION,
    D10 `modifyOtherKeys`, D12 reverse wrap) — but see Gaps: the CSI table, the answers table and
    the HLD's `US-0076` exit criteria all require D7/D8/D10, so they are implemented here and the
    packet-column conflict is recorded rather than resolved.

## Acceptance

- [x] `vt-corpus check --engine new` green on all 45 recordings.
- [x] `crates/tools/tests/corpus_check.rs` runs the new engine inside `cargo test --workspace`.
- [x] Every difference is covered by a declared `expected-diffs.json` window naming a correction
      id; no stale window. **Measured: there is no difference at all, so no file was written.**
- [x] Every test the LLD's Verification list names exists and passes (61 `terminal::tests`).
- [x] The trap-map rows owned by `dispatch` (12, 20, 21, 22, 25, 26, 38, 39, 40, 42, 43) pass.
- [x] `vt-diff` feeds the same bytes to both engines and diffs in `grid.expect` form; run over
      all 45 recordings and over the bench fixtures, with the result reported.
- [x] No `unsafe`, no new external dependency.
- [x] `pwsh scripts/ci-local.ps1` green.
- [x] The seven `Terminal::selection_*` wrappers and the `invalidated_by`-before-operation
      obligation (`selection.md`, added mid-packet by `US-0078`).

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

Changed: `docs/osc-sequences-checklist.md` — the three stale statements `migration.md` line 241
assigns here. There is one VT parser and not two (the checklist said OneTerm parses OSC 7 and 133
"in parallel" via a second parser), the type is `OscRouter<T>` and not `OscSink`, and the FIFO
promise is a property of the byte-ordered event batch rather than of a queue. The OSC 52 line now
says the engine decodes and the **policy** stays in `security_policy.rs`, and the OSC 133 line
records that `oneterm-vt` also holds the mark as a cell `Semantic` plus a tracked anchor.

The low-level designs are unchanged, per the brief. Every place this packet reads differently from
them is in Gaps below, not edited into the contract.

## Context

- The worktree was branched from `main` (`c936ac0`) without `crates/vt`. It was
  `git reset --hard 0163e86` (feat/vt-engine, parser core), and then, when `US-0077` landed
  mid-read, `git reset --hard 4b833a0` — both before any file was written. `TerminalGrid::resize(size,
  ResizePolicy) -> ResizeOutcome` is therefore present and the resize dispatch targets it
  directly; no `resize_rows` placeholder was needed. Two further rebases followed as upstream
  moved: `02962f4` (the `US-0075` blanking rework) and `cc14802` (`US-0078` selection, which added
  `EngineView::selection` and the seven wrappers to this packet's scope).
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
- [x] Write the packet and mirror the story row into `harness.db`.
- [x] `crates/vt/src/terminal/{mod,mode,color,osc,dispatch}.rs`, plus `terminal_tests.rs`.
- [x] New-engine corpus replay in `crates/tools`, `--engine new`, `vt-diff`.
- [x] Get `selective_erasure` green (first run), then the rest: 44 of 45 on the first full run,
      45 of 45 after the `row_mut` fix.
- [x] Measure the corrections. **No `expected-diffs.json` file is needed**; the measurement is in
      the evidence file instead, so the reasoning survives even though there is nothing to declare.
- [x] The seven selection wrappers, after `US-0078` landed mid-packet.
- [x] `pwsh scripts/ci-local.ps1`; parity gate output saved to
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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence

Parity gate output, per recording: [`evidence/US-0076-parity-gate.md`](evidence/US-0076-parity-gate.md).

**The exit criterion is met.** `vt-corpus check --engine new`: **45 recordings, 45 passed, 0
failed**, and **no `expected-diffs.json` file was needed at all** — every recording matches the
frozen `grid.expect` and `state.expect` cell-exactly. `vt-diff` old-against-new: **45 of 45
identical** over the corpus and **10 of 10 identical** over the `vt-bench` fixtures at 160x45,
256 KiB each. `vt-corpus check --engine old` is still 45/45, so the expectations did not move.

`pwsh scripts/ci-local.ps1` green. Raw totals over 57 `test result:` sections:
**1518 passed / 0 failed / 8 ignored**. `cargo test -p oneterm-vt --lib`: 329 passed / 0 failed /
2 ignored in 0.60 s, of which **61 are `terminal::tests`**; `cargo test -p oneterm-tools --test
corpus_check`: 2 passed (old engine and new, both against the frozen files).

**The corrections, measured rather than assumed.** The design tables predicted C9 (`DECSTR` in
`grid_reset`) as "the one certain diff in this table". It is not: `grid_reset` does send `CSI ! p`,
`terminal::tests::decstr_soft_reset_scope` proves the sequence is implemented rather than dropped,
and the soft reset's effects are all overwritten by what follows it in that recording. C1-C8,
C10-C12 are likewise free, C13-C15 are unreachable from a corpus that never resizes and carries no
selection. The full table is in the evidence file.

**Three real defects the gate found**, each fixed with a named regression test:

1. `Screen::row_mut` materialised an unwritten ring slot with the cursor's **erase** cell, so the
   first glyph landing on a fresh row repainted every untouched column with the live
   background-erase colour. `sgr` showed it as 810 cells over six rows carrying `48;5;1`. An
   unwritten slot already reads as plain blanks; deliberate background-erase blanking is
   `blank_row` / `blank_slot`, which the scroll and reset paths call explicitly. (`US-0075` code.)
2. The `WRAPPED` row flag (deviation G1) outlived the cell that carried it. The reference keeps
   `WRAPLINE` on the last **cell** and wipes it with any write there, so a TUI repainting over the
   end of a wrapped row cleared it; the row flag did not, and a later reflow would have rejoined
   two rows that are not one logical line. Found by the differential on the `tui_redraw` bench
   fixture — **no corpus recording reaches it**, which is why the fixture run is part of the gate.
   Fixed with one `clear_wrap_at` call on each `RowMut` write path.
   (`terminal::tests::overwriting_the_last_cell_clears_the_wrap_flag`.)
3. The OSC accumulator ate the separator that closes the sixteenth parameter, so P9's "re-split
   parameter 16 on `;`" — which `parser.md` assigns to this packet — was not actually reversible: a
   bulk `OSC 4` palette set would have mis-paired every colour past the eighth. `params_full()` now
   opens the last slot one parameter earlier, so every separator inside the join survives.
   (`terminal::tests::osc_parameters_past_the_sixteenth_are_re_split`, and
   `parser::tests::osc_params_past_sixteen_join_into_the_last`'s expectation moves from
   `"op;q;r;s;t"` to `"o;p;q;r;s;t"`.)

**Files.** New: `crates/vt/src/terminal/{mod,dispatch,mode,color,osc,terminal_tests}.rs`;
`crates/tools/src/corpus_replay_new.rs`; `crates/tools/src/bin/vt-diff.rs`;
`evidence/US-0076-parity-gate.md`. Edited: `crates/vt/src/lib.rs` (module line and re-exports),
`crates/vt/Cargo.toml` (`vt-paranoid`), `crates/vt/src/events/vt_event.rs`
(`VtEvent::ColorQuery`, `FeedStats::hyperlink_table_exhausted`), `crates/vt/src/intern.rs` (the
hyperlink ladder, `clear`, `Hyperlink::implicit`), `crates/tools/src/{lib,corpus}.rs`,
`crates/tools/src/bin/vt-corpus.rs`, `crates/tools/tests/corpus_check.rs`,
`crates/tools/Cargo.toml`, `scripts/dependency-graph-policy.json`,
`docs/osc-sequences-checklist.md`.

**Edits outside this packet's file scope**, each listed because the brief limits them to additive
one-line accessors:

| File | Change | Why here |
| --- | --- | --- |
| `parser/mod.rs` | re-export `ParamGroups` | the CSI argument reader needs to name the type |
| `parser/osc.rs` | `params_full()` opens the last slot one parameter earlier | defect 3 above; P9 is assigned to this packet |
| `parser/parser_tests.rs` | one expectation moves with it | same |
| `grid/screen.rs` | `set_region_raw`, three lines | the reference's one-based `DECSTBM` test can produce an **empty** region, which `set_region`'s `top < bottom` contract cannot express |
| `grid/screen.rs` | `row_mut` allocates with `Cell::EMPTY` | defect 1 above |
| `grid/row.rs` | `clear_wrap_at` plus five call sites | defect 2 above |
| `intern.rs`, `intern_tests.rs`, `render/render_tests.rs` | `HyperlinkTable::intern` returns `Option` | the ladder the LLD assigns to this packet; the two test files are mechanical call-site updates |

## Gaps

Ambiguities and conflicts found while implementing. None is edited into the designs, per the brief.

1. **D7 / D8 / D10 are assigned to `US-0086` by the deviation table but required here** by the CSI
   table, the answers table and the HLD's `US-0076` exit row (P19, and
   `dispatch::tests::da1_da2_dsr_xtversion_answers` is in this file's own verification list). They
   are implemented here — DECXCPR, XTVERSION and `modifyOtherKeys` are a dozen lines between them
   and the harness discards answers — and the packet-column conflict is recorded rather than
   resolved. **D12 (reverse wrap `? 45`) is not implemented**: it is a print-path change in
   `Screen::backspace`, which is `US-0075`'s file; the mode is recognised, stored and reported.
2. **DECALN does not home the cursor.** `dispatch-and-modes.md` says "fill the screen with `E`,
   home the cursor"; the reference does not move the cursor, and three recordings send `ESC # 8`.
   The reference behaviour is reproduced, because homing would be an undeclared grid difference
   with no correction id to name it. `terminal::tests::decaln_fills_the_screen_with_default_styled_e`
   pins the choice.
3. **`SUB` (`0x1A`) is a no-op**, where the LLD says "write a replacement glyph at the cursor". Same
   reason; **no recording sends it**, so the choice is unmeasurable either way and the reference
   is the safe default.
4. **`? 47` / `? 1047` / `? 1048` (correction C8)** are implemented as "the `? 1049` swap" and
   "`DECSC` / `DECRC`". The LLD says only "implemented"; xterm distinguishes them by whether the
   cursor is saved and whether the alternate screen is wiped on the way out, and `TerminalGrid::swap_alt`
   does both unconditionally. **No recording exercises any of the three**, so the distinction is
   free to make later; the code says so.
5. **`Repaint` is emitted whenever the batch dispatched anything**, not when something actually
   changed. `events-and-api.md`'s `repaint_is_absent_when_nothing_changed` is in `US-0079`'s
   verification list, not this one, and an exact answer needs a `changed` flag threaded through
   every grid primitive. `feed(b"")` still appends nothing.
6. **`pub mod testing` and `pub mod strip` are not created.** `events-and-api.md` places them in the
   public surface; nothing in this packet needs them and no downstream crate exists yet. The test
   helpers live in `terminal_tests.rs`.
7. **DCS is dropped and counted.** Sixel is `US-0078`'s successor packet (`graphics.md`); the
   `sixel` bench fixture is byte-identical between the two engines today only because both drop it.
8. **`NamedColor::bright()` / `dim()`** (named in the colour-model section) are not added: nothing
   in the dispatch layer needs them, and the renderer's bold-to-bright mixing stays in
   `crates/terminal/src/palette.rs` as the same section says.
9. **`vt-paranoid` is declared but not yet consumed.** The manifest entry is this packet's
   (`testing-and-bench.md` M12); the `#[cfg(feature = "vt-paranoid")]` gate belongs on the property
   tests in `grid/` and `reflow/`, which are other packets' files.
10. **No integration, E2E or platform proof beyond the corpus.** Nothing in the workspace depends
    on `oneterm-vt` except `oneterm-tools`; the application still runs the old engine (R-44) and the
    first application-level proof is `US-0081`.
11. **`Terminal::prune_selection` is this packet's invention.** A reflow kills selection anchors
    where it stands and a history trim kills anything that fell off the oldest end, neither of which
    can reach the `Selection` value that owns the two anchor entries; without the prune they leak
    one pair per drag-resize. It reads as "no selection" through `selection_range()` either way.
12. **`Invalidation` is also evaluated for `DECCOLM` and `DECALN`**, which `selection.md` does not
    list among the four. Both blank the whole screen, so leaving a selection over them is exactly
    the bug the obligation exists to prevent.

## Handoff

Ready for verification. Branch `worktree-agent-a134c2d2c50be5a81`, on `feat/vt-engine` @ `cc14802`,
not merged and not pushed. The three defects in Evidence are the rows a verifier should re-derive
first: each is reproducible by reverting one hunk and re-running
`cargo run -p oneterm-tools --bin vt-corpus -- check --engine new` or `vt-diff --fixtures`.
