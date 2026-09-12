# Work: Selection

ID: US-0078
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

- Change type: new capability (the engine's selection concern; nothing in `crates/vt` selected
  anything before this packet, and the public API the High-Level Design exposes —
  `selection_start` / `selection_update` / `selection_range` / `selection_text` /
  `selection_clear` / `select_all` — had no design until
  [`low-level-design/selection.md`](low-level-design/selection.md)).
- Risk lane: high_risk (the intake's lane). Selection is a user-visible, clipboard-facing
  behaviour: a wrong range copies the wrong text. Nothing in the workspace depends on
  `oneterm-vt` yet, so nothing user-reachable changes with this packet.
- Spec Intake, when required: IN-0029

## Outcome

`crates/vt` owns selection, against
[`low-level-design/selection.md`](low-level-design/selection.md):

1. **Four kinds** — `Simple`, `Block`, `Semantic`, `Lines`, each with the LLD's `to_range` rule,
   including reversed anchors, the side rules, and the "never empty" rule for `Semantic` and
   `Lines`.
2. **Anchors are tracked anchors** — both endpoints are `AnchorKind::SelectionStart` /
   `SelectionEnd` entries in the engine's one anchor list, so a selection rotates through scroll,
   `IL`/`DL`, a trim and a reflow by the mechanism that already exists, and the reference's
   `Selection::rotate` has no counterpart here.
3. **The invalidation matrix**, stated once in code, kind-scoped where the LLD says so.
4. **Text extraction** — wide chars once, spacers never, graphemes whole, tab runs collapsed to
   the next tab stop, trailing blanks trimmed by the reference's `line_length` policy, wrapped
   rows joined without a newline, hard newlines between rows, `Block` as a rectangle.
5. **The render hook** — `RenderState::selection()`, refreshed every update from
   `EngineView::selection`, which is what [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md)
   names and what the migration gap table assigns to this packet.

## Scope

- [ ] In scope:
  - `crates/vt/src/selection/` — new: `mod.rs` (the types, the anchor plumbing, `to_range`,
    `hit_test`, the invalidation matrix), `expand.rs` (the semantic, bracket and logical-line
    walks), `text.rs` (extraction), `selection_tests.rs`, `selection_props.rs`.
  - `crates/vt/src/lib.rs` — the `pub mod selection;` declaration and its re-exports.
  - `crates/vt/src/render/state.rs` — additive: one `EngineView` field, one `RenderState` field,
    one accessor, and the `selection_changed` term in the `Unchanged` test (a drag over static
    content must still produce a frame).
- [ ] Out of scope:
  - `crates/vt/src/grid/screen.rs` and `crates/vt/src/grid/terminal_grid.rs` — **not touched.**
    The invalidation matrix is a predicate here rather than a `kill_selection` call sprinkled
    through the erase paths; see reading 4. A concurrent packet is reworking the blanking paths.
  - `crates/vt/src/terminal/` — a concurrent packet. `Terminal::selection_*` are thin wrappers
    over this module; see reading 1.
  - `crates/vt/src/grid/anchor.rs` — nothing needed; `register` / `get` / `set` / `release` /
    `shift_region` / `remap` / `trim` / `kill_selection` are all already there from `US-0075`
    and `US-0077`.
  - The click-count to `SelectionKind` mapping, which stays in the view
    ([`low-level-design/migration.md`](low-level-design/migration.md), `US-0085`).
  - `crates/terminal` and `crates/terminal-view` — the adapter packets (`US-0081`, `US-0085`).

## Acceptance

- [x] Every test the LLD's Verification section names exists and passes, under
  `cargo test -p oneterm-vt selection::`.
- [x] Each kind's `to_range` on a fixed grid, including reversed anchors.
- [x] Semantic selection over escape chars, wide chars and wrapped rows; bracket matching when
  the anchors coincide.
- [x] `Lines` across a wrapped logical line; `Block` with wide chars at both edges.
- [x] Rotation: a selection survives a scroll into history, `IL`/`DL` inside a region moves it,
  a trim kills it, the invalidation matrix cases kill it, and a column reflow round trip
  (narrow then widen) is compared by `selection_text` before and after.
- [x] Text extraction: trailing blanks trimmed per policy, a wide char once, a spacer never,
  wrapped rows joined without a newline, hard newlines between rows, an empty selection yields
  empty text.
- [x] A property test: for random grids and random anchors, `to_range` is ordered and within
  bounds, and `selection_text` length equals the covered visible cells' text.
- [x] `pwsh scripts/ci-local.ps1` green; no `unsafe`; no new dependency.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md` — this packet's contract:
  the four kinds, tracked anchors, fractional hit testing, the `to_range` table, semantic
  expansion and its escape chars, `selection_text`, the invalidation matrix, `select_all`, the
  edge cases and the verification list.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` — the canonical
  `AnchorKind` list, the lane rule that makes `Anchors::trim` screen-scoped, `RowFlags::WRAPPED`
  as a **row** flag (deviation G1, which replaces the reference's per-cell `WRAPLINE` in every
  wrapped-line rule here), and the statement that selection invalidation is specified in
  `selection.md` rather than restated there.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/reflow-and-resize.md` — the anchor API,
  `Anchors::remap`, the `sync_anchors` ordering, trap 28, and the recorded exception that
  `Anchors::kill_selection` matches on **kind**, not lane.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` — the
  `selection: Option<SelectionRange>` field and the `selection()` accessor on `RenderState`, and
  the rule that selection and the cursor are refreshed every update rather than carried by row
  damage.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the R-28
  integrity budget and the trap map; trap 28 is reflow's row and stays there.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` — the view-needs gap table
  row "Selection range — `start`, `end`, `is_block`", which assigns this packet the render-state
  hook, and the `US-0085` row that keeps the click-count mapping in the view.
- `docs/spec-intakes/IN-0029-vt-engine/research/engine-semantics.md` § 2.18 and § 8 — the
  reference behaviour and the traps.
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — row ids name
  positions, not content, which is the whole reason both endpoints are anchors.
- `docs/terminal-backend.md` — the backend's selection surface today
  (`TerminalModel::{start_selection, update_selection, selection_text, has_selection,
  clear_selection, select_all}`), which this module must be able to reproduce.
- `vendor/alacritty_terminal/src/selection.rs`, `src/term/mod.rs:544-645`,
  `src/term/search.rs:465-620` — the reference implementation being replaced.

### Documentation Action

No contract change. `selection.md` describes the behaviour implemented here, and the shape
differences below are all consequences of types the intake's earlier packets already shipped
(`Terminal` does not exist yet; `Config` does not exist yet; `WRAPPED` is a row flag; `Pos`
carries a `RowId` rather than a viewport `Line`). They are recorded as readings for the design
owner rather than as edits to the LLD, which this packet must not modify.

Reason: the LLD is written against a `Terminal` that `US-0076` will introduce. Rewriting it now
would pre-empt that packet's own shape. Everything observable — the four kinds, the side rules,
the escape chars, the matrix, the extraction policy — is implemented exactly as written.

### Reconciliation

Docs changed: none. The no-change reason above still holds at completion. The readings and the
two deviations are recorded here, in this packet, which is where the intake's other packets
record theirs.

## Context

### Shape differences from the LLD

1. **No `Terminal`.** The LLD's interfaces are `impl Terminal`. `crates/vt/src/terminal/` is
   being built by a concurrent packet, so this module is written against the types that do
   exist and the `Terminal` methods become one-line wrappers:

   | LLD | This packet |
   | --- | --- |
   | `Terminal::selection_start(pos, side, kind)` | `Selection::new(&mut TerminalGrid, kind, pos, side)` |
   | `Terminal::selection_update(pos, side)` | `Selection::update(&mut self, &mut TerminalGrid, pos, side)` |
   | `Terminal::selection_range()` | `Selection::to_range(&self, &TerminalGrid, escape_chars) -> Option<SelectionRange>` |
   | `Terminal::selection_text()` | `Selection::text(&self, &TerminalGrid, &Interner, escape_chars) -> Option<String>` |
   | `Terminal::selection_clear()` | `Selection::release(self, &mut TerminalGrid)` |
   | `Terminal::select_all()` | `Selection::all(&mut TerminalGrid) -> Selection` |
   | `Terminal::hit_test(row, col)` | `selection::hit_test(&TerminalGrid, row, col) -> (Pos, Side)` |

   `Terminal` will hold `selection: Option<Selection>` and route each method through it. That is
   also why `release` consumes `self`: the anchors must be released exactly once, and the
   type system says so.

2. **No `Config`.** `Config::semantic_escape_chars` does not exist (`US-0076` owns the mode and
   config table), so the escape set is a `&str` parameter on the two entry points that need it,
   with the LLD's default published as `selection::SEMANTIC_ESCAPE_CHARS`.

3. **`WRAPPED` is a row flag** (deviation G1, `grid-and-scrollback.md`). Every reference rule
   phrased as "the cell at the last column carries `WRAPLINE`" is implemented as
   `screen.row(id).wrapped()`. This is strictly better defined: the reference reads a flag off
   a cell that an `EL` can erase.

4. **The invalidation matrix is a predicate, not a call site.** `Selection::invalidated_by(grid,
   Invalidation)` answers the matrix for the seven operations that blank or replace content
   without moving a row. The row-**moving** half of the matrix (`SU` / `SD` / `IL` / `DL` / `RI`,
   a scroll into history, a resize) needs no code at all: those primitives already call
   `Anchors::shift_region`, `Anchors::trim` and `Anchors::remap`, and a dead anchor makes
   `to_range` return `None`, which is the LLD's "the selection is cleared". Putting the erase
   half in the same place keeps one mechanism and keeps this packet out of `screen.rs`, whose
   blanking paths a concurrent packet is reworking.

5. **`contains_cell` takes `Option<Pos>` for the block cursor.** The reference takes a
   `CursorShape`, which is `US-0076`'s type. `Some(pos)` means "a block-shaped cursor is at
   `pos`"; `None` means every other shape. The quirk itself — a block cursor sitting exactly on
   a selection corner is not inverted — is reproduced unchanged.

### Readings for the design owner

1. The `Terminal` wrappers above are the only thing `US-0076` owes this module.
2. `SEMANTIC_ESCAPE_CHARS` is byte-for-byte the reference's `,│`|:"' ()[]{}<>\t`, as the LLD
   requires. It is a `&str` and matching is by `char`, so a multi-byte escape char such as `│`
   works.
3. `Selection::all` registers `(oldest, 0)` and `(newest, cols - 1)` on the **active** screen's
   run, not on the primary's. On the alternate screen that is the alternate lane, which is the
   only sensible reading of "covering history and viewport" when the alternate screen has no
   history.
4. The matrix's `EL`/`ED 0`/`ED 1` rows are evaluated **before** the operation runs, because
   each is about the rows the operation is going to touch and the cursor row is the input. The
   enum's doc comment says so.
5. The reference's `line_to_string` tail rule — "if a wide char is not part of the selection but
   its leading spacer is, include it" — reads
   `self.grid[line - 1i32][Column(0)].c`, which is the row **above**; a `LeadingWideSpacer` at
   the end of a row belongs to the glyph on the row **below**. This packet implements the rule
   the way it is described rather than the way it is coded, and pins it with
   `text_includes_a_wide_glyph_whose_leading_spacer_ends_the_selection`.
6. `semantic_search_right` steps back one cell without skipping spacers, where
   `semantic_search_left` steps forward one cell **and** skips them. The asymmetry is the
   reference's; it is not observable in the extracted text, because a range whose end lands on
   a `WideSpacer` still covers its `Wide` partner at `end - 1`. Reproduced rather than
   "fixed", and named here so the choice is visible.

## Plan

- [x] Write this packet and mirror the story row into `harness.db`.
- [x] `selection/mod.rs` — types, anchor plumbing, `to_range` dispatch, `hit_test`, the matrix.
- [x] `selection/expand.rs` — the cell walker, `semantic_search_*`, `line_search_*`,
  `bracket_search`.
- [x] `selection/text.rs` — `line_text`, `bounds_text`, the block rectangle.
- [x] `selection/selection_tests.rs`, `selection/selection_props.rs`.
- [x] `render/state.rs` — the selection field, the accessor and the `Unchanged` term.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — clause 1 (row ids
  name positions) is why both endpoints are anchors; clause 2 (the incremental render state) is
  why the selection range is a field on `RenderState` rather than a question the painter asks
  the engine a second time.

No new decision. Nothing here is a choice future work must inherit that the LLD does not
already state.

## Verification Plan

- `cargo test -p oneterm-vt selection::` — the LLD's named tests plus the intake's outcome list.
- `cargo test -p oneterm-vt render::` — the render-state hook, including the new
  `a_selection_change_alone_reports_a_frame`.
- `VT_PROPTEST_CASES=10000 cargo test -p oneterm-vt selection::props` — the ordering, bounds and
  text-length property at depth.
- `pwsh scripts/ci-local.ps1` — the full gate, raw `test result:` totals recorded below.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Base correction

The worktree was created on `main` (`c936ac0`), which has no `crates/vt` at all. It was
`git reset --hard 4b833a0` — the tip of `feat/vt-engine`, `US-0077`'s last commit — before any
file was read or written. Same correction `US-0079` recorded.

### Commands

*(filled in after the run — see the table below)*

### Gaps

*(filled in after the run)*

## Handoff

Branch `worktree-agent-a3a509ddbf185b930`, off `feat/vt-engine` @ `4b833a0`. Not merged, not
pushed. Next owner: the integrator for `IN-0029`, then `US-0076` (`Terminal` wrappers) and
`US-0085` (the view's `SelectionKind`).
