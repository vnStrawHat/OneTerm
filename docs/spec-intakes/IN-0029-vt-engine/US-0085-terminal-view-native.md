# Work: crates/terminal-view goes native

ID: US-0085
Intake: IN-0029
Created: 2026-09-13

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

- [x] In scope: all of `crates/terminal-view/src/**`; in `crates/terminal` the
  compatibility surface only — `content.rs`, `engine_shim.rs` (deleted), `model.rs`,
  `session.rs`, `search.rs`, `mouse_encode.rs`, `palette.rs`, `osc_color.rs`,
  `color_classification.rs`, `backend/state.rs`, `lib.rs`, `test_support.rs`,
  `Cargo.toml`, `tests/us0081_parity.rs`, **and the `*_tests.rs` modules of the
  files above** (`content_tests.rs`, `model_tests.rs`), which are the same
  compilation units.
- [x] In scope (listed): `crates/local-shell/src/session_tests.rs:5` —
  `use alacritty_terminal::selection::SelectionType;` becomes
  `use oneterm_terminal::SelectionKind as SelectionType;`. The three call sites are
  unchanged because the variant names match. There is no re-export that can avoid
  this: the file imports the type from the fork directly, so the alias has to move
  with the deleted parameter type. `US-0083` deletes `crates/local-shell`'s
  `alacritty_terminal` manifest line and would have had to make the same edit.
- [x] Out of scope: anything else in `crates/local-shell` and `crates/ssh`
  (`US-0083` / `US-0084` own them, concurrently), `crates/tools`, `crates/vt`
  behaviour changes, deleting the fork (`US-0087`), the deferred deviations
  (`US-0086`).

## Acceptance

- [x] `grep -rn alacritty_terminal crates/terminal crates/terminal-view` matches
  only `crates/terminal/tests/us0081_parity.rs`, the `[dev-dependencies]` line
  that lets it build, and four comments citing the fork. No **code** in either
  crate names it.
- [x] `crates/terminal/src/engine_shim.rs` does not exist.
- [x] `plan_cache` is keyed on `(RowId, SeqNo)`; an idle frame plans no row and
  hashes none (there is no hash left), and a scroll replans only the scrolled-in
  rows.
- [x] `RenderUpdate::Unchanged` reaches the plan cache as "nothing to do": no
  candidate scan, no URL scan, no layout — `FrameStats::frames_unchanged`.
- [x] Every test in `crates/terminal` and `crates/terminal-view` passes unchanged,
  or is consciously rewritten with the reason recorded in
  [`evidence/US-0085-verify.md`](evidence/US-0085-verify.md) § 2.
- [x] `cargo test -p oneterm-terminal --test us0081_parity` green, with the same
  five-difference allow-list.
- [x] `pwsh scripts/ci-local.ps1` green.
- [ ] **NOT MET.** The four GUI walks could not be reproduced: `quser` reports
  session 1 **`Disc`** for the whole packet, so `PrintWindow` returns an all-black
  bitmap and `CopyFromScreen` fails. No `US-0085-` screenshot exists. The gap and
  what stands in for it are in `evidence/US-0085-verify.md` § 3.
- [x] The flood table re-measured, before and after, on this machine
  ([`evidence/US-0085-measurements.md`](evidence/US-0085-measurements.md)): the
  snapshot column drops 61 -> 36 ms in `fast-dev` and 38 -> 12 ms in release, more
  than the ~9 ms `migration.md` attributed to the `Cell` rebuild alone. Frame time
  under sustained output is measured headless, from the same `FrameStats`
  counters; **the old-binary comparison for it is part of the GUI gap.**

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

Changed:

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md`
  — idea 1 amended, idea 3 renamed off the fork, the diagram, two module-map rows,
  the `RenderState` field table, the whole Invalidation Rules table and deviation
  8. Commit `0a8b7f3`.

Confirmed unchanged, and why:

- `.../low-level-design/{migration,damage-and-render-state,graphics,selection,
  events-and-api}.md` — this packet consumed the shapes they already specify;
  nothing about the engine's behaviour changed. `migration.md`'s `US-0085` row,
  its deletion list and its cost table all describe what was done.
- `docs/terminal-backend.md` — § 5's pump layer and § 5.3's resize policy are
  untouched: no engine, transport or lock path changed.
- `docs/agents/structure.md`, `crate-dependency-rules.md`, `dependencies.md` — no
  crate was added or removed and no dependency edge changed. `crates/terminal`'s
  `alacritty_terminal` moved from `[dependencies]` to `[dev-dependencies]` and
  `crates/terminal-view` dropped it; both lines are `US-0087`'s to delete, and the
  dependency-graph script passes as it stands.

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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Full record: [`evidence/US-0085-verify.md`](evidence/US-0085-verify.md) and
[`evidence/US-0085-measurements.md`](evidence/US-0085-measurements.md).

Independently verified — **merge after minor fixes**, no blocker, no correctness
defect ([`evidence/US-0085-independent-verify.md`](evidence/US-0085-independent-verify.md)).
Its § 4 is the packet's strongest evidence and covers what `us0081_parity`
cannot: the **old** view conversion (`engine_shim::write_row` +
`Cell::from_indexed`, copied from `d3c537b` and built against the real fork
types) against the **new** `FrameRow::cell` over 61 streams — **2 822 154 cells,
0 differences** — with the style, colour and width maps proved pointwise equal in
isolation. Its five minors (M1-M5) are applied in the commit that carries it.

```text
cargo test --workspace                                   green (exit 0)
cargo test -p oneterm-terminal                           245 passed
cargo test -p oneterm-terminal --test us0081_parity        5 passed, 2 ignored
cargo test -p oneterm-terminal-view                      288 passed, 3 ignored
cargo clippy --workspace --all-targets -- -D warnings     green
pwsh scripts/ci-local.ps1                                 green
cargo build -p oneterm-app --profile fast-dev             Finished in 1m 45s
```

### Gaps

1. **The four GUI walks were not reproduced, and no screenshot exists.** `quser`
   reports session 1 `Disc` (idle 12:03) throughout. An instance was launched from
   this worktree's `fast-dev` binary (pid 15516; the owner's 27376 recorded first
   and never signalled), created a window and stayed alive, but `PrintWindow`
   returns an all-black bitmap on a disconnected session — the same result the
   `US-0081` independent verifier recorded. The walks, and with them the
   **render-sampler pixel comparison against a binary built from `main`**, remain
   owed and need an Active desktop. `evidence/US-0085-verify.md` § 3 lists what
   stands in for them (`us0081_parity` over 81 streams through the native path;
   the headless GPUI draw tests including a real Sixel painted and released) and
   says plainly how far that goes: what a frame *contains* is proven, what a GPU
   draws from it is not.
2. **No old-versus-new frame time from the app's diagnostics log**, for the same
   reason. The engine-side half of that question — the per-frame snapshot cost —
   is measured before and after on this machine.
3. `crates/local-shell/src/session_tests.rs` was edited (seven one-line reads of
   deleted fields plus one import) in a crate `US-0083` owns. **Verified clean:**
   the independent verifier ran `git merge-tree` against `US-0083`'s branch
   (`worktree-agent-a20da8012abddb9c7` @ `5f9cc1e`) — no conflict, and the two
   branches share no changed file at all; that packet never touches
   `session_tests.rs`. The line-by-line list stays in `US-0085-verify.md` § 2 in
   case that branch moves before it merges.
4. One additive engine accessor: `oneterm_vt::Terminal::interner_mut`. Nothing on
   the engine's own paths uses it; it exists so an embedder **test** can write a
   styled cell into a real grid instead of the render state growing a fabrication
   API for a downstream test.

## Handoff

Worktree `agent-ad731760c1d949455`, branch
`worktree-agent-ad731760c1d949455`. The tool based it on `main @c936ac0`, which has
no `crates/vt`; `git reset --hard d3c537b` was run before anything was read or
written, so the branch is `feat/vt-engine @ d3c537b` plus this packet's commits.
**Not merged, not pushed.**

**To `US-0083` and `US-0084`.** Nothing in `crates/ssh` was touched.
`crates/local-shell/src/session_tests.rs` needed seven one-line edits (listed in
`US-0085-verify.md` § 2); everything else in both crates is untouched, and the
seam now gives you what you asked for:

- `impl_pty_terminal_session!` expands **`$crate::` paths only** — no
  `::oneterm_vt` reaches the caller, so neither backend needs a dependency on
  `oneterm-vt` to use the macro.
- its `$resize_policy` argument accepts **either** `oneterm_terminal::ResizePolicy`
  or `oneterm_vt::ResizePolicy`: both `From` directions exist. `resize_policy()`
  still reads back as `oneterm_terminal::ResizePolicy`, because that is the only
  one `crates/local-shell` can name (the HLD's crate layout gives it `core`,
  `terminal`, `pty`). `local_session_grow_policy_matches_conpty` and
  `ssh_session_keeps_the_default_grow_policy` compile unchanged.
- `set_default_colors` takes `$crate::Rgb` and `mouse_down` takes
  `$crate::SelectionKind`, so both crates can drop their `alacritty_terminal`
  manifest line whenever they like.
- `oneterm_terminal` re-exports the engine vocabulary (`Rgb`, `SelectionKind`,
  `RowId`, `ModeSnapshot`, `CursorShape`, `RenderRow`, …) for exactly this.

**To `US-0087`.** You inherit `crates/terminal/tests/us0081_parity.rs` — which now
also carries the legacy-snapshot conversion this packet deleted from the product —
and the `[dev-dependencies] alacritty_terminal` line that builds it. Deleting the
test deletes the line and the last mention of the fork in these two crates.
**Done at `c8d84ff`:** the test, the dev-dependency and the fork are gone.

**Still owed:** the four GUI walks, on an Active desktop. See Gaps.
