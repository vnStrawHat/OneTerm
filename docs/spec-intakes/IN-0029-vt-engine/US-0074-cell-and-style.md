# Work: Cell, style and grapheme storage

ID: US-0074
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

- Change type: new capability (the first source code of the `oneterm-vt` crate)
- Risk lane: high_risk (the intake's lane; this packet ships no product-reachable behaviour yet —
  nothing depends on `oneterm-vt`)
- Spec Intake, when required: IN-0029

## Outcome

`crates/vt` (`oneterm-vt`) exists as a registered workspace crate, and its storage layer —
everything one grid position holds — is implemented and proven against
[`low-level-design/cell-and-style.md`](low-level-design/cell-and-style.md):

1. An 8-byte packed `Cell` whose zero value is a valid empty cell, with the LLD's exact bit
   layout and a compile-time size assertion.
2. Per-terminal interned `StyleSet` and `ExtrasTable`, both on the three-step no-sweep ladder
   (reuse / insert / fall back to id 0 with one warning), where **an id never moves**.
3. A `GraphemeArena` capped at `GRAPHEME_MAX_LEN = 16` codepoints per cell, with the overflow
   path written first and GC by remap implemented and proven content-preserving.
4. Width rules: `scalar_width()` reproducing today's per-scalar behaviour, and `cluster_width()`
   implemented and tested now so mode 2027 becomes a print-path packet later.
5. `HyperlinkTable` with a **per-terminal** implicit-id counter, not a process-global one.

This packet is storage only. It adds no parser, no grid, no print path and no dispatch, and no
crate in the workspace depends on it yet.

## Scope

- [ ] In scope:
  - `crates/vt/Cargo.toml` (new), `crates/vt/src/lib.rs` (module declarations + crate docs only),
    `crates/vt/src/cell.rs`, `crates/vt/src/intern.rs`, `crates/vt/src/width.rs`.
  - Workspace registration: root `Cargo.toml` members + the `oneterm-vt` path dependency + the
    five third-party workspace declarations this crate needs, `Cargo.lock`,
    `scripts/dependency-graph-policy.json`.
  - Owning-doc reconciliation: `docs/agents/structure.md` (§1 tree, §3 responsibility table),
    `docs/agents/crate-dependency-rules.md` (the layer diagram, R6/R7 wording),
    `docs/agents/dependencies.md` (§3 auxiliary crates).
- [ ] Out of scope:
  - `crates/vt/src/parser/` — `US-0073`, implemented concurrently. This packet touches
    `src/lib.rs` only to add its own `pub mod` lines.
  - The grid and the print path (`US-0075`): every wide-pair *repair* that needs a row or a
    neighbouring row, insert mode, wrapping, `Terminal::sweep_graphemes`, `FeedStats`,
    `RowFlags::HAS_GRAPHEME`. See "Evidence and Gaps" — three `cell::tests::` rows the LLD lists
    cannot be written without a grid and are re-homed there.
  - Mode 2027 itself (R-56/R-38: deferred by the LLD), the colour resolution layer
    (`NamedColor::bright`/`dim`, `ColorKey`, `Terminal::color`) which belongs to `US-0076`, and
    `GraphicData`/`Placement` which belong to `US-0080`.
  - `crates/vt/tests/corpus/` — `US-0072` data, kept untouched.

## Acceptance

- [x] `size_of::<Cell>() == 8` is a compile-time assertion, `Cell::EMPTY == Cell::default()`
      reads back as a space with `StyleId::DEFAULT` and no extras, and the bit layout matches the
      LLD field for field (content 0..21, `is_grapheme` 21, width 22..24, semantic 24..26,
      protected 26, reserved 27..32, style 32..48, extras 48..64).
- [x] There is **no `has_extras` bit** (R-34): `extras_id() != ExtrasId::NONE` is the test.
- [x] The style ladder is reuse / insert / fall back to id 0, with a single `log::warn!` per
      table and a counter; driving 70 000 distinct styles leaves every previously issued id
      resolving to the value it was issued for.
- [x] The extras ladder is the same, and one image stamped over 400 x 200 cells grows the extras
      table by exactly one entry (R-21).
- [x] Clusters longer than `GRAPHEME_MAX_LEN` are truncated and counted, identical clusters
      dedupe, `needs_sweep()` fires exactly at the documented constants
      (`GRAPHEME_SWEEP_ENTRIES = 65_536`, `GRAPHEME_SWEEP_CHARS = 1 MiB`, R-27), and a GC by
      remap preserves the text of every live cell.
- [x] The arena's own exhaustion path (the 21-bit id space) is implemented and observed in a
      test, falling back to a reserved blank cluster rather than panicking.
- [x] `is_blank()`, `is_erasable()` and `text_char()` are three different functions with the
      LLD's three different rules, including the `'\t'` cell (R-12) and the graphic cell (R-13).
- [x] `cluster_width()` returns 2 for the ZWJ family emoji, the skin-tone sequences and the flag
      sequences the research lists as visibly broken today, while `scalar_width()` still pins
      today's per-scalar behaviour.
- [x] No `unsafe` anywhere in the crate.
- [x] `pwsh scripts/ci-local.ps1` is green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md` — the contract this
  packet implements: bit layout, both ladders, the arena, the predicates, the width rules.
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` — crate layout (`crates/vt` =
  `oneterm-vt`, leaf, no OneTerm dependency, no gpui), the flat module layout (R-49), the
  dependency table, and the P2/P4/P18 problem rows this packet closes.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` — `Row`/
  `RowFlags` consume `Cell`; `WRAPPED` is a row flag (deviation G1), so it is not a cell bit.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md` — `GraphicId(u64)`, and the
  one-extras-entry-per-image rule (R-21).
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` — the render
  hand-off copies **resolved values, never ids**, which is why style ids may never move.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` — the public surface
  re-exports `Cell, CellContent, CellWidth, Color, NamedColor, Style, Attrs, Rgb` from `cell`,
  and `FeedStats` owns the two counters this packet feeds.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the integrity
  budget (R-28): O(1) checks on mutating methods, full walks only at feed/resize boundaries.
- `docs/spec-intakes/IN-0029-vt-engine/research/engine-semantics.md` § 2.3, `research/prior-art.md`
  § 9.1 — the reference print path and the cell-layout survey the design derives from.
- `docs/decisions/DEC-0014-oneterm-owns-its-vt-engine.md` — why a first-party engine crate exists
  at all.
- `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md`,
  `docs/agents/dependencies.md` — the registration surface for a new crate (R11) and the layering
  rules it must satisfy (R1, R2, R6, R7).
- `docs/agents/code-style.md` — flat modules, tests beside the code, no `unwrap` in production,
  small public API.
- `docs/agents/error-policy.md` — terminal input is untrusted: never panic, degrade and count.

### Documentation Action

Update required:

- `docs/agents/structure.md` — §1 tree (`crates/vt/` is no longer "data only"; it gains a
  manifest and three source files) and §3 responsibility table (a new `vt` row).
- `docs/agents/crate-dependency-rules.md` — the L0 layer line and R6/R7 must name `oneterm-vt`
  (the HLD's "Rule impact" section).
- `docs/agents/dependencies.md` § 3 — the auxiliary-crate table gains the VT engine's row
  (`unicode-width`, `unicode-segmentation`, `bitflags`, `rustc-hash`; `proptest` dev-only).

Reason: a new workspace crate is exactly the change R11 says must appear in the member list, the
policy file and two doc rows, and the dependency table gains a category (Unicode width and
segmentation) it did not have.

### Reconciliation

Changed: `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md`,
`docs/agents/dependencies.md`, `scripts/dependency-graph-policy.json`, root `Cargo.toml`.
Not changed and deliberately so: `docs/PROJECT.md` (the "terminal engine is `alacritty_terminal`"
invariant is still true — nothing depends on `oneterm-vt` yet; the intake's documentation plan
gives that edit to the last packet), and every IN-0029 design document (the intake forbids
implementers editing them; deviations are recorded below instead).

## Context

- The crate is a leaf: no OneTerm dependency, no gpui, no lock, no atomic, no interior
  mutability. `oneterm-vt` is reachable from nothing, so `THIRD-PARTY-NOTICES.md` (rendered from
  what `oneterm-app` reaches) does not move.
- Every third-party crate this packet declares already resolves in `Cargo.lock`, so the
  dependency graph does not grow: `bitflags 2.13.0`, `rustc-hash 2.1.2`, `unicode-width 0.2.2`,
  `unicode-segmentation 1.13.3`, `proptest 1.11.0`, `log 0.4.32`.
- `US-0073` (parser) is implemented concurrently in `crates/vt/src/parser/`. Both packets create
  `crates/vt/Cargo.toml` and `crates/vt/src/lib.rs`; this packet keeps both files as small and as
  mechanical as possible so the merge is a two-line union.

## Plan

- [x] Write this packet, mirror the story row into `harness.db`.
- [x] Create the crate: manifest, lints, edition, workspace member, policy entry.
- [x] `cell.rs`: `Cell` + the packed accessors, `CellContent`, `CellWidth`, `Semantic`, `Style`,
      `Attrs`, `Color`, `NamedColor`, `Rgb`, the three predicates.
- [x] `intern.rs`: `StyleId`/`ExtrasId`/`GraphemeId`/`HyperlinkId`/`GraphicId`, `StyleSet`,
      `ExtrasTable`, `GraphemeArena` (overflow path first), `HyperlinkTable`, `Interner`.
- [x] `width.rs`: `scalar_width`, `cluster_width`.
- [x] Tests: every row the LLD's Verification section names that does not require a grid, plus
      the ladder-exhaustion and proptest round-trip tests the packet brief adds.
- [x] Reconcile the three agent docs, run `pwsh scripts/ci-local.ps1`.

## Decisions

- [`DEC-0014`](../../decisions/DEC-0014-oneterm-owns-its-vt-engine.md) — OneTerm owns its VT
  engine.
- [`DEC-0015`](../../decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md) — the
  render hand-off carries resolved values, which is the reason the style ladder may not renumber.

No new decision: every choice this packet makes is already recorded in the LLD.

## Verification Plan

- `cargo test -p oneterm-vt` — the unit suite (`cell::`, `intern::`, `width::`), including the
  proptest round trips and both ladders driven to exhaustion.
- `pwsh scripts/ci-local.ps1` — fmt, clippy `-D warnings` over all targets, the whole workspace
  test suite, the dependency-graph policy (proves the new crate is registered and is a leaf), the
  doc-path check (proves the new doc rows point at real paths), English-only, and the
  third-party-notices check.
- `cargo tree -p oneterm-vt -e normal` — R6/R7: no `oneterm-*`, no `gpui*`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Commands

- `pwsh scripts/ci-local.ps1` — green on 2026-09-12, every step.
- `cargo test --workspace` raw totals: **52 `test result:` sections, 1188 passed / 0 failed /
  5 ignored**. The `US-0072` baseline was 50 sections and 1156 passed, so this packet adds two
  sections (the `oneterm-vt` unit binary and its empty doc-test) and exactly the 32 tests below.
- `cargo test -p oneterm-vt` — **32 passed / 0 failed / 0 ignored, 0.07 s**
  (`cell::tests` 15, of which 3 are proptest properties; `intern::tests` 13; `width::tests` 4).
  The whole crate's suite is well inside `testing-and-bench.md`'s 60 s debug budget.
- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, with no new `#[allow]`.
- `python scripts/verify-dependency-graph.py` — "policy passed for 20 workspace packages and 20
  explicit members" (19 before), which is the proof that `oneterm-vt` is registered and is a leaf.
- `python scripts/check-doc-paths.py`, `python -m unittest scripts/test_check_english.py`,
  `python scripts/check-english.py`, `python scripts/completion-catalog.py validate`,
  `python scripts/third-party-notices.py --check` — all pass. The notices file does not move,
  because nothing reachable from `oneterm-app` changed and every new dependency was already in
  `Cargo.lock`: the lock file grows by exactly 12 lines, the new package entry.
- `grep -rn "unsafe" crates/vt/src` — no match.

### Deviations from the LLD (implemented as the best reading, LLD not edited)

1. **`is_erasable(self, ex: &ExtrasTable)` takes the whole `&Interner` instead.** The rule the
   LLD states ("default fg/bg, none of `INVERSE`, any underline, `STRIKEOUT`") cannot be
   evaluated from an `ExtrasTable`: those live in the `Style` behind the cell's `style_id`. The
   signature is `is_erasable(self, interner: &Interner) -> bool`.
2. **`text_char()` takes `&GraphemeArena`.** `text_char(self) -> char` cannot answer for a
   grapheme cell, whose content bits hold an id, not a scalar. The signature is
   `text_char(self, graphemes: &GraphemeArena) -> char` and it returns the cluster's base scalar;
   callers that need the whole cluster use `GraphemeArena::resolve`.
3. **Three `cell::tests::` rows are re-homed to `US-0075`**, because they test the *print path*,
   which this packet explicitly does not own and which has no grid to run against:
   `wide_char_at_last_column_wrap_on_and_off` (trap 5 — needs wrapping and a second row),
   `insert_mode_over_wide_char_repairs_the_pair` (trap 7, correction C4 — needs the row shift),
   and `zero_width_at_column_zero_attaches_to_column_zero` (trap 8 — needs a cursor). The
   cell-level halves that *can* be tested without a grid are:
   `wide_pair_repair_on_overwrite` (trap 6, the two same-row sub-cases, via
   `repair_wide_pair_in_row`) and `width_enum_covers_every_wide_pair_shape`. The cross-row
   sub-case of trap 6 (the previous row's trailing `LeadingWideSpacer`) is re-homed with the
   others.
4. **The grapheme arena has a fourth ladder step the LLD does not name.** The LLD's answer to
   arena growth is the sweep, but the sweep is `Terminal::sweep_graphemes` and there is no
   `Terminal` yet, so nothing stops the 21-bit id space from filling. `GraphemeArena::intern`
   therefore falls back to a reserved id 0 (a single-space cluster interned at construction),
   counts it in `arena_exhausted` and warns once — the same shape as the style ladder's step 3,
   degraded but never a panic and never a lost cell. It is unreachable in practice once
   `US-0075` wires the sweep.
5. **`FeedStats::style_table_exhausted` / `grapheme_truncated` do not exist yet** (they belong to
   `events-and-api.md`, i.e. `US-0079`). The counters live on the tables
   (`StyleSet::exhausted()`, `GraphemeArena::truncated()`, `GraphemeArena::arena_exhausted()`)
   and `US-0075`/`US-0079` reads them into `FeedStats`.
6. **`Terminal::sweep_graphemes -> SweepStats` is not implemented** — it is a `Terminal` method
   and belongs to `US-0075`. Its arena half is: `GraphemeArena::sweep(live) -> GraphemeRemap`,
   which steals the arena, re-interns only the ids it is handed and returns the old → new map the
   caller rewrites cells with. `grapheme_gc_preserves_every_live_cell` drives it over a synthetic
   `Vec<Cell>` stand-in for a grid.
7. **`unicode-width` is `0.2.x`, not "2.x"** as the LLD's Width section says. The HLD's
   dependency table and `Cargo.lock` both say `0.2`; the crate has no 2.x release.
8. **`NamedColor::bright()` / `dim()` and `ColorKey` are not implemented.** They are the colour
   *resolution* layer (`dispatch-and-modes.md`), owned by `US-0076`. `Color`, `NamedColor` and
   `Rgb` are defined here only because `Style` cannot be typed without them, and the public
   surface in `events-and-api.md` re-exports them from `cell`.
9. **File names follow the LLD (`cell.rs`, `intern.rs`, `width.rs`), not a `style.rs` /
   `grapheme.rs` split.** The LLD names two files; `width.rs` is the third because the LLD's own
   verification list names `width::tests::`. The HLD's R-49 module layout is flat files, and
   `events-and-api.md` re-exports `Style`/`Attrs`/`Color` from `cell`.

### Gaps

- **No micro-benchmark.** The LLD asks for none, and `testing-and-bench.md`'s five tiers have no
  style-lookup or intern tier. Interning is an `FxHashMap` lookup and resolution is a `Vec`
  index; the number that will matter is tier 1 (parse + grid throughput), measured at
  `US-0075`/`US-0076` against the `US-0072` baseline. Not measured here.
- **The 21-bit id-space exhaustion test injects a small limit** through a crate-private
  `GraphemeArena::with_id_limit`, rather than interning 2 097 152 real clusters (~2 s and
  ~200 MB in a debug build, in the CI gate). The production constructor uses the real constant
  and a test pins that it does.
- **Integrity assertions are the O(1) tier only** (`debug_assert!` on every mutating table
  method: the index and the entry vector agree in length, id 0 is never overwritten, every span
  lies inside the arena). The full two-screen walk `testing-and-bench.md` describes has no screen
  to walk yet; it arrives with `US-0075`.
- **Nothing consumes this code.** There is no integration, E2E or platform proof, and there
  cannot be one until `US-0081` puts the engine behind the seam. The proof here is the unit suite
  plus the compile-time size assertion.
- **An implicit OSC 8 link is one table entry per occurrence**, as the reference produces and as
  the LLD's "per-terminal counter" wording implies, so a stream of unique un-`id=`-ed links grows
  `HyperlinkTable` without bound. That is a dispatch-side concern (`US-0076` owns OSC 8, RIS and
  reset, which is where the table is cleared); this packet does not cap it.
- **The disk filled during verification.** `cargo test --workspace` failed twice with
  `LNK1106: invalid file or disk full` and one rustc `STATUS_STACK_BUFFER_OVERRUN` while `D:` had
  under 1 GB free, with 98 GB of it in the main checkout's `target/`. It was re-run to completion
  after `cargo clean` in this worktree; the totals above are from that clean run. Nothing about
  the failures was code-related.
- Branch `worktree-agent-ae2a4340bbcef8125` off `feat/vt-engine` @ `f3cf1a5`. Not merged, not
  pushed.

## Handoff

`US-0075` (grid) picks this up: it owns the print path (and with it traps 5, 7, 8 and the
cross-row half of trap 6), `RowFlags::HAS_GRAPHEME`, `Terminal::sweep_graphemes` over
`GraphemeArena::sweep`, and reading the three counters listed in deviation 5 into `FeedStats`.
`US-0076` owns `NamedColor::bright`/`dim` and `ColorKey`. `US-0080` owns `GraphicData` and
re-exports `GraphicId` from `graphics`.
