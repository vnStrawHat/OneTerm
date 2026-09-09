# Work: Font weight scales the self-drawn strokes

ID: US-0052
Intake: IN-0018
Created: 2026-09-09

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

- Change type: existing-contract change (`shape_quads` / `stroke_thickness` gain a weight input; the geometry at weight 400 is unchanged)
- Risk lane: normal
- Spec Intake: IN-0018

## Outcome

Owner request (2026-09-09): "Font weight has no effect on the self-drawn strokes." The quad-drawn
box / block / braille / powerline glyphs derive their stroke thickness from the cell width alone,
so with the terminal font at weight 600/700 (`TerminalSettings.font_weight`) or a cell under SGR
bold (`CellFlags::BOLD`) the text gets heavier but a box frame does not, and a bold frame looks
thinner than the bold text next to it. The old engine had the same gap; this is a small new
capability of the shape geometry, not a regression.

After this packet the light and heavy stroke thickness (and everything derived from it: rails,
arc band, diagonal and powerline outline bands) scales with the effective font weight of the
cell — the settings weight, raised by 300 for a bold cell — while every weight at or below 400
produces today's geometry bit for bit. Fills, blocks, shades and braille dots do not change.

## Scope

- [x] In scope: `crates/terminal-view/src/render/shapes.rs` (thickness rule takes a font
      weight; `shape_quads` / `stroke_thickness` / `Geometry::new` signatures),
      `shapes_tests.rs` (weight-parameterised helpers, new tests, symmetry suites at 400 and
      700), `row_plan.rs` (`PlanContext.font_weight`, per-cell effective weight, tests),
      `plan_cache.rs` (weight-change replan test), `state.rs` (pass the settings weight),
      owning docs listed below, visual evidence.
- [x] Out of scope: any change to text shaping or to `FontSet`'s bold variant
      (`max(base, 700)` stays), braille dot size, block / shade geometry, settings schema
      (`font_weight` already exists and already flows into `RenderInputs.font`), the old engine.

## Acceptance

- [x] `stroke_thickness(cell, weight)`: `k = clamp(weight / 400, 1, 2.25)`,
      `t_l = max(1, round(W/8 * k))`, `t_h = max(t_l + 2, round(W/3 * k))`, `t_d = t_l`; at
      weight 400 (and any lower weight) every value equals today's; the table in `shapes.md`
      lists W = 7, 8, 9, 14, 18 at 400 / 600 / 700 / 900.
- [x] `shape_quads(c, cell, weight, out)`: for every code point and cell size, weight 400 and
      weight 100 emit exactly the rect set the current code emits (`SOLID_SNAPSHOT` and every
      existing test pass unchanged at 400: `weight_400_matches_baseline_geometry`,
      `solid_families_unchanged_and_opaque`).
- [x] At 9 × 19 and 14 × 29, weight 700 vs 400: `─` thicker, `━` and `┃` thicker, the `╭` band
      thicker, the `═` rails thicker and still disjoint (`heavier_weight_thickens_strokes`).
- [x] The symmetry and joint invariants hold at weight 700 as well as 400:
      `mirror_x_pairs_match`, `mirror_y_pairs_match`, `self_symmetric_glyphs`,
      `builder_commutes_with_transforms`, `cross_equals_union_of_lines`,
      `horizontal_line_abuts_across_cells`, `double_rails_equidistant_from_joint`,
      `rails_disjoint_with_gap`, `rounded_corner_matches_straight_stubs`, and the quad budget
      (`per_cell_quad_budget`) at 700.
- [x] Row planning: a cell with `CellFlags::BOLD` (or class-style bold) is drawn at
      `min(900, base + 300)`; a `┌` cell with BOLD yields a different, thicker rect set than
      the same cell without (`bold_cell_uses_heavier_strokes`); `PlanContext.font_weight = 700`
      thickens `─` (`settings_weight_scales_strokes`); coalescing across cells compares rects,
      so quads of different weights never merge.
- [x] A settings weight change replans every row (`weight_change_replans_all`).
- [x] Visual: in the running app a bold box frame is visibly heavier than a plain one and still
      joins cleanly (`evidence/US-0052-weight.png`, `US-0052-weight-zoom.png`).
- [x] Gates: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo clippy -p oneterm-terminal-view --all-targets --features terminal-diagnostics
      -- -D warnings`, `cargo test -p oneterm-terminal-view`, `cargo test --workspace`,
      `python scripts/check-english.py`, `python scripts/check-doc-paths.py`.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` —
  principle 2 (shapes are geometry), deviation 9 (thickness table), the shape-geometry row of
  the performance budget, the parity-snapping risk row.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/shapes.md` —
  thickness rules table, `Interfaces` (`shape_quads`, `stroke_thickness`), Family D/H
  (`t = t_l` bands), verification list.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`
  — `PlanContext`, row plan step 2 (style / shape), shape coalescing, `StyleKey`
  (`weight_bits` already present, invalidation table).
- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md` —
  items 3 and 4: quads, symmetric construction, snapping never changes a stroke's thickness.
  The weight scale is applied to the *nominal* thickness before snapping, so item 4 is
  untouched; no amendment needed.
- `crates/settings/src/terminal_settings/settings.rs` — `font_weight: FontWeight` (settings
  contract, unchanged).

### Documentation Action

Update required:

- `low-level-design/shapes.md`: thickness table becomes a function of `(W, weight)` with the
  scale rule and the W × weight table; `Interfaces` signatures; a note that the gap of a double
  line follows `t_l` (so `2d` stays even and the rails keep abutting the light joint) while the
  dash gap stays at the weight-400 light thickness; verification list additions.
- `low-level-design/render-pipeline.md`: `PlanContext.font_weight`; row plan step 2 computes
  the effective per-cell weight and passes it to `shape_quads`; coalescing note.
- `high-level-design.md`: deviation 9 and the shape-geometry budget row mention the weight
  scale; the US-0052 line in the parity checklist.
- `IN-0018.md`: US-0052 in the candidate packets list.

Reason: `shapes.md` is the owning contract for the thickness rule and `render-pipeline.md` for
what row planning feeds `shape_quads`; both currently state thickness depends on the cell width
only.

### Reconciliation

Done (2026-09-09) — docs changed by this packet:

- `low-level-design/shapes.md`: "Thickness rules" rewritten as a function of `(W, weight)` with
  the `k` rule, the 5 × 4 table, the double-gap / dash-gap note, effective-weight note;
  `Interfaces` signatures; verification list (`heavier_weight_thickens_strokes`,
  `weight_400_matches_baseline_geometry`, weights 400 / 700 on the symmetry and joint suites).
- `low-level-design/render-pipeline.md`: `PlanContext.font_weight`, row-plan step 2 "shape"
  bullet (effective weight, `shape_quads(ch, device, weight, …)`), coalescing note
  (comparison is by rect, so different weights never merge), `StyleKey` paragraph (a settings
  weight change replans every row; `weight_change_replans_all`).
- `high-level-design.md`: deviation 9 (weight scale), performance-budget shape row (budget also
  asserted at weight 700), parity checklist entry for US-0052.
- `IN-0018.md`: US-0052 added to Candidate Work Packets.
- `DEC-0007`: reviewed, no change (item 4 concerns snapping; the scale is applied before it).

## Context

- `RenderInputs.font.weight` already carries the settings weight and `StyleKey.weight_bits`
  already includes it, so a weight change already invalidates every plan; this packet only has
  to feed the weight into `PlanContext` and `shape_quads`.
- `shapes.rs` stays GPUI-free: the weight is a plain `f32` on the CSS scale (100..900), which
  is exactly `gpui::FontWeight.0`.
- Bold text uses `FontSet`'s bold variant (`max(base, 700)`); strokes use `min(900, base + 300)`
  so a bold cell at a 600/700 base still gets visibly heavier strokes than a plain one, which
  the text variant would not give (`max(600, 700) = 700`).
- Deviation from the brief, recorded here: the brief asked for "gaps unchanged". The **dash**
  gap stays at the weight-400 light thickness. The **double-line** gap follows the scaled
  `t_l` (as the code does today: `gap = light`): with a fixed gap and a thicker rail `2d =
  t_d + g` can be odd and, at e.g. 9 px / 900 or 14 px / 900, the rails overlap the light
  joint, breaking `double_rails_equidistant_from_joint` and the `R-.hi == J.lo` invariant in
  `shapes.md`. Scaling the gap keeps the rails abutting the joint and the double line readable
  (gap ≥ today's).

## Plan

- [x] Packet (this file) and intake list entry.
- [x] `shapes.rs`: `stroke_thickness(cell, font_weight)` with the `k` scale and a `dash_gap`
      field; `Geometry::new(cell, font_weight)`; `shape_quads(c, cell, font_weight, out)`;
      dashes use `dash_gap`.
- [x] `shapes_tests.rs`: `quads_at` / `Bitmap::of_at` / `assert_mirrored_at` with a weight,
      `WEIGHTS = [400, 700]` loops on the symmetry / joint / budget suites, new tests.
- [x] `row_plan.rs`: `PlanContext.font_weight: f32`; `push_shape(ch, col, color, bold)`
      computes the effective weight; tests.
- [x] `state.rs`: `font_weight: inputs.font.weight.0`; `plan_cache.rs` test harness field and
      `weight_change_replans_all`.
- [x] Docs (see Documentation Action), visual evidence, gates.

## Decisions

- DEC-0007 (items 3, 4) — unchanged; the weight scale is applied to the nominal thickness
  before snapping.

## Verification Plan

- Focused: `cargo test -p oneterm-terminal-view shapes` (new: `heavier_weight_thickens_strokes`,
  `weight_400_matches_baseline_geometry`; existing suites now also at weight 700),
  `cargo test -p oneterm-terminal-view row_plan` (`bold_cell_uses_heavier_strokes`,
  `settings_weight_scales_strokes`), `cargo test -p oneterm-terminal-view plan_cache`
  (`weight_change_replans_all`).
- Regression: `cargo test -p oneterm-terminal-view`, `cargo test --workspace`, both clippy
  invocations, `check-english.py`, `check-doc-paths.py`.
- Manual (Windows, `--profile fast-dev`): print a plain box and the same box under SGR bold,
  plus `\x1b[1m╭──╮ ═══ ━━━\x1b[0m`; capture `PrintWindow` full and 4× zoom into
  `evidence/US-0052-weight*.png`; optionally `font_weight: 700` in `target/terminal.json`
  (backed up, restored byte-identical).

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (branch `refactor/terminal-render-engine`, Windows, 2026-09-09):

- `cargo fmt --all` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — `Finished dev profile`, no warnings
  (`clippy exit 0`).
- `cargo clippy -p oneterm-terminal-view --all-targets --features terminal-diagnostics -- -D
  warnings` — no issues.
- `cargo test -p oneterm-terminal-view` — `270 passed, 2 ignored` (`shapes`: 29 passed, 2
  ignored — `shape_bitmaps_for_visual_review`, `thickness_table_for_docs`).
- `cargo test --workspace` — 44 suites, 1037 passed, 0 failed (`test exit 0`).
- `python scripts/check-english.py` — `English contributor-text check passed for 545 files.`
- `python scripts/check-doc-paths.py` — `Doc path check passed for 149 current paths in 10
  documents.`
- Thickness table (from `thickness_table_for_docs`): 9 px `t_l / t_h` = 1/3 at 400, 2/5 at
  600, 2/5 at 700, 3/7 at 900; full table in `shapes.md`.

Tests added: `shapes_tests::{heavier_weight_thickens_strokes,
weight_400_matches_baseline_geometry, thickness_table_for_docs (ignored)}`; the symmetry /
joint / thickness / budget suites now iterate `WEIGHTS = [400, 700]`
(`mirror_x_pairs_match`, `mirror_y_pairs_match`, `self_symmetric_glyphs`,
`builder_commutes_with_transforms`, `horizontal_line_abuts_across_cells`,
`cross_equals_union_of_lines`, `corner_arms_meet_at_joint`, `heavy_thicker_than_light`,
`stroke_thickness_uniform_across_axes`, `double_rails_equidistant_from_joint`,
`rails_disjoint_with_gap`, `rounded_corner_matches_straight_stubs`, `per_cell_quad_budget`);
`row_plan::tests::{bold_cell_uses_heavier_strokes, settings_weight_scales_strokes}`;
`plan_cache::tests::weight_change_replans_all`.

Visual (fast-dev build 10:45:35, `target/fast-dev/oneterm.exe`, pids 20452 / 9332 started and
stopped by the driver; the owner's `dist` build pid 15476 untouched; workstation locked, input
posted with `PostMessage`, capture `PrintWindow(hwnd, hdc, 2)`; Lilex 15 px, cell 9 × 18):

- `evidence/US-0052-weight.png` — a plain and an SGR-bold row of `┌ ╔ ┏ ╭` frames with tees
  and crosses, plus `╭──╮ ═══ ━━━ ╱╲╳ ┄┄┄ E0B1 E0B5` bold and plain and a `───` plain / bold /
  plain run. Bold frames are visibly heavier and join cleanly.
- `evidence/US-0052-weight-zoom.png` — 4× nearest-neighbour zoom of the same region: bold light
  strokes 2 px (plain 1 px), bold rails 2 px, bold heavy 5 px (plain 3 px), thicker arc and
  diagonal bands, dashes keep their 1 px gaps, tees / crosses meet without gaps, the mixed run
  changes thickness at the bold cell only.
- `evidence/US-0052-weight-700.png` — `target/terminal.json` `font.weight = "bold"` (backed up,
  restored byte-identical, verified with `SequenceEqual`): the settings row now draws 2 px
  light strokes, and SGR bold on top (capped at 900) 3 px.

Test-helper change worth knowing: at 7 px / 700 (`t_l = 2`) the `║` rails are `[0, 2)` and
`[4, 6)` — the family's half-pixel-left bias reaches the cell edge. The mirror helper used to
assume a far-edge pixel always continues into the next cell and so expected a 3 px rail; the
comparison now ignores the far-edge line on a parity-mismatched axis (`Bitmap::without_far_edge`,
`shift_interval` accepts both edge outcomes). Edge reach is still asserted by
`horizontal_line_abuts_across_cells` at both weights. Geometry unchanged.

Gaps:

- Deviation from the brief: the double-line gap scales with `t_l` (see Context); the dash gap
  stays fixed as asked.
- `pwsh scripts/ci-local.ps1` not run (instructed); `bash vendor/refresh.sh --check` and
  `cargo deny` not run.
- The `cargo test --workspace` run included unrelated uncommitted changes present in the working
  tree (`crates/terminal`, `crates/ssh`, `crates/local-shell`, IN-0019 work by another agent).
- No screenshot at a HiDPI scale; the 14 × 29 and 36 × 76 cases are covered by the geometry
  tests only.

## Handoff

None: single-session packet.
