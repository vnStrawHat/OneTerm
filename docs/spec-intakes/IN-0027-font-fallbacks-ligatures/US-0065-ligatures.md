# Work: Ligatures anchored to their cells

ID: US-0065
Intake: IN-0027
Created: 2026-09-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: IN-0027

## Outcome

Ligatures are on by default and render correctly: a ligature glyph starts at its first cell
and the glyphs after it stay in their own columns. `terminal.json` `font.ligatures` and the
Settings UI "Ligatures" switch turn them off.

## Scope

- [x] In scope: `FontConfig.ligatures` + `TerminalSettings.font_ligatures` + persistence;
  `terminal_font()` derives `calt` from it (explicit `features` entries still win);
  `RowPlan::cell_starts` + `TextRunPlan.cells_start/end`; `CellAnchor` in
  `paint_shaped_line`; Settings UI switch; IN-0018 HLD risk row and README.
- [x] Out of scope: ligatures across a colour change (a colour change stays inside one run,
  so those already ligate) or style change (bold/italic break the run, as in most
  terminals); cursor-over-ligature splitting; Linux/macOS checks.

## Acceptance

- [x] A run whose shaper merged bytes 0..2 into one glyph paints the glyph at cell 0 and the
  glyph for byte 2 at cell 2 (`glyphs_are_anchored_at_their_cell`).
- [x] A combining mark keeps its offset relative to its base glyph within the cell (same
  test; `zero_width_marks_join_base_run` checks the cell map `[0, 1, 4]`).
- [x] `terminal_font()` emits `calt=1` by default, `calt=0` when `ligatures` is false
  (`font_calt_follows_the_ligatures_switch`), and an explicit `"calt"` in `features` wins
  (`font_enables_listed_features_and_overrides_calt`).
- [x] GUI (Windows, Lilex): `=>`, `->`, `!=`, `<=`, `===`, `www` render as ligatures and
  `|ab|` after them sits exactly above `|cd|` of the reference row
  (`evidence/US-0065-ligatures-on.png`); with `ligatures: false` the pairs render as plain
  glyphs (`evidence/US-0064-US-0065-fallbacks-empty-ligatures-off.png`). Checked by restart,
  not by the live switch (gap below).
- [x] `pwsh scripts/ci-local.ps1` green (2026-09-11).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0027-font-fallbacks-ligatures/high-level-design.md` — placement rule.
- `docs/spec-intakes/IN-0018-terminal-view-rewrite/high-level-design.md` § Risks (row
  "`force_width` shaping treats a wide glyph as one base") and § Frame Pipeline paint row —
  must state the cell-anchored placement.
- `crates/settings/src/terminal_config/font.rs` doc comment on `features` ("the terminal
  disables calt by default") — must change with the default.
- `README.md` § Features.

### Documentation Action

Update required: IN-0018 HLD risk row, `FontConfig` doc comment, `README.md`.

Reason: the default and the placement rule change documented behavior.

### Reconciliation

Changed: IN-0018 HLD § Risks (new row: `force_width` numbers glyphs, painter anchors by
cell), `FontConfig` doc comments (`features` no longer says calt is off by default;
`ligatures` documented), `README.md` § Terminal emulator.

## Context

- `gpui` `apply_force_width_to_layout` classifies a glyph as a new base when its shaped `x`
  exceeds the previous base by more than half a cell, then forces `x = base_count * width`.
  A ligature consumes several bytes but counts once, so later bases drift left by
  `(bytes - 1)` cells. Within-cell offsets of non-base glyphs are preserved, which the
  painter reuses.
- `ShapedGlyph.index` is the byte offset of the glyph's cluster start in the run text.
- Runs are built cell by cell in `row_plan.rs::append_to_run`; every non-forced (wide) run
  is a single cell, so `cell_starts` is only needed for forced runs.

## Plan

- [x] Settings: `ligatures` field, plumbing, test.
- [x] `terminal_font()` calt rule + test.
- [x] `RowPlan::cell_starts` filled in `append_to_run` (flattened like `colors`, so a rebuild
  allocates nothing); `CellAnchor` in `paint_shaped_line` + tests.
- [x] Settings UI switch.
- [x] Docs, GUI evidence.
- [x] CI.

## Decisions

None.

## Verification Plan

- `cargo test -p oneterm-terminal-view` — placement and font tests.
- `cargo test -p oneterm-settings` — round-trip.
- `pwsh scripts/ci-local.ps1`.
- GUI: fast-dev build, echo ligature pairs, screenshot with the switch on and off.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-settings -p oneterm-terminal-view -p oneterm-settings-ui`:
  45 / 26 / 284 passed (2026-09-11); the first cut of
  `glyphs_are_anchored_at_their_cell` had a wrong cell map (`[0, 2]` for a two-cell
  ligature) and was corrected to `[0, 1, 2]`.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- GUI: `evidence/gui-walk.md` with crops.
- Gaps: the live Settings UI switch was not driven (restart-based check); a block cursor over
  a ligature covers only its own cell while the ligature glyph spans several (unchanged
  behaviour, now visible because ligatures are on); Linux/macOS not checked.

## Handoff

None.
