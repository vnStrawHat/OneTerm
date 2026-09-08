# Work: Render core (frame model, row plans, plan cache, glyph cache, GPUI element)

ID: US-0047
Intake: IN-0018
Created: 2026-09-08

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

- Change type: new capability (new engine built beside the old one; not yet used by the view)
- Risk lane: normal
- Spec Intake: IN-0018

## Outcome

`src/render/` is a complete, testable render engine: `Frame` (the only alacritty-typed file),
`CellMetrics`/`GridGeometry`, `GlyphCache`, `RowPlan` building with shape coalescing,
`PlanCache` with damage ∪ cursor-row ∪ mask-delta candidates, scroll rotation and hash
verification, `RenderState`, `TerminalElement` (layout/prepaint/paint inside one grid layer plus a
cursor layer), cursor, overlays, and cfg-gated diagnostics — driven end to end by
`FakeTerminalSession` tests that prove dirty-frame planning and idle-frame zero work. The old
`element/` and `layout/` stay in place and in use until US-0049.

## Scope

- [ ] In scope: `src/render/{frame, metrics, glyphs, row_plan, plan_cache, state, element,
      cursor, overlay, diagnostics}.rs`, `src/render/element_tests.rs`; adapters in retained
      modules so they accept the new types: `theme/` (`TerminalTheme::color(Color) -> Hsla`
      table, `ensure_contrast` with exponent 2.4, `min_contrast` default rule — deviations 5, 6),
      `url/mask.rs` (`FrameRow` input, `url_masks_into` reuse), `highlight/overlay.rs`
      (`scan_into` reusing a class buffer). Old callers of those modules keep compiling (add
      the new entry points beside the old ones; remove the old ones in US-0049).
- [ ] Out of scope: input handling, `TerminalView`, panel/space, scrollbar, search UI, gutter
      timestamp bookkeeping (the element only formats and paints labels handed to it),
      completion, deleting old files.

## Acceptance

- [ ] HLD parity items US-0047 1–36, §2.15 (1–5, 7–9), §2.22 are implemented in `render/`
      and covered by the tests below.
- [ ] `dirty_frame_plans_rows_and_shapes` and `idle_frame_plans_nothing_but_paints` pass:
      idle frame `rows_planned == 0`, `shape_calls == 0`, `url_scans == 0`, `quads > 0`;
      dirty frame `snapshot_calls == 1`.
- [ ] `scroll_rotates_plans_and_replans_only_scrolled_in_rows` passes with `Damage::Full`
      (deviation 8).
- [ ] `block_run_coalesces_into_one_quad`: 40 `█` → 1 quad; 40 `─` → 1 quad.
- [ ] `idle_frame_allocates_nothing`: zero allocations around `PlanCache::update` + overlays on
      the second identical frame.
- [ ] `metrics_snap_cell_to_device_pixels` at 1.0 / 1.25 / 1.5 / 2.0.
- [ ] No module outside `render/frame.rs` (and the retained `theme/palette.rs` conversion of
      `TerminalPalette`) imports `alacritty_terminal` in `render/`: enforced by a test that greps
      `src/render/*.rs` for `alacritty_terminal` and expects only `frame.rs`.
- [ ] `theme` tests: `contrast_ratio_uses_wcag_exponent` (black/white = 21.0 ± 0.01;
      #777777 on white ≈ 4.48), `min_contrast_zero_keeps_theme_default`,
      `min_contrast_one_disables`.
- [ ] `pwsh scripts/ci-local.ps1` green; old view still renders (manual smoke run).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` — frame pipeline, caches, invalidation rules, interfaces, deviations 4–8, 10, performance budget.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md` — the contract this packet implements.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/shapes.md` — `DeviceRect`/`ShapePath` consumed here.
- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md` — items 1, 2, 5.
- `docs/PROJECT.md` — invariants; `docs/architecture.md` — crate boundaries (engine stays in `crates/terminal`).
- `docs/agents/crate-dependency-rules.md` — R7 (view maps engine types to GPUI).
- `docs/terminal-rendering-optimization.md` — historical after this intake; read for PERF tags referenced by the inventory only.

### Documentation Action

Update required: `render-pipeline.md` if any struct/field or the plan-cache pseudo-code
changes during implementation; `high-level-design.md` Cross-frame State table if a new buffer
is added.

Reason: the LLD is the owning contract for the engine's data structures; implementation drift
must be reflected there so US-0048/0049 build on the real shapes.

### Reconciliation

Before completion, diff `render-pipeline.md` type listings against `src/render/*.rs`
signatures and record the result.

## Context

- GPUI 0.3.3 phase rules: `insert_hitbox` prepaint-only; `paint_quad`/`paint_glyph`/
  `handle_input`/`set_cursor_style` paint-only; `shape_line_by_hash` legal in prepaint; scene
  kind order inside a layer is Quad → Path → Underline → Sprite regardless of call order; quads
  keep insertion order within a layer.
- `snapshot_into` consumes damage — call it exactly once per frame from prepaint; use
  `query_state`/`terminal_info` elsewhere.
- `force_width` shaping breaks after a wide glyph: wide chars are their own run with
  `force_width: None`.
- `FakeTerminalSession` (`oneterm-terminal/test-support`) drives the element tests; use
  `FakeSessionProbe::set_text`/`set_cursor`/`snapshot_calls`.
- `FrameStats` and `LatencySamples` are `cfg(any(test, feature = "terminal-diagnostics"))`.
- Retained modules gain new entry points beside old ones; keep `cargo clippy -D warnings` clean
  with `#[allow(dead_code)]` only on the new module roots.

## Plan

- [ ] `frame.rs`: `Frame`, `FrameRow`, `Cell`, `Color`, `CellFlags`, `CursorShape`, `Cursor`,
      `Selection`, `Damage`, conversions, row hash, `text_into`, tests for conversions and
      selection mapping.
- [ ] `metrics.rs`: `CellMetrics`, `measure`, `GridGeometry`, `grid_size_for`, `pixel_to_grid`,
      tests (device snapping at four scales, gutter/padding subtraction, ≥ 1×1).
- [ ] `theme/`: `Color → Hsla` table rebuilt on palette change; WCAG 2.4 luminance;
      `min_contrast` rule; tests.
- [ ] `glyphs.rs`: `FontKey`, `GlyphCache::{begin_frame, shape}`, eviction; tests.
- [ ] `highlight/overlay.rs` `scan_into`; `url/mask.rs` `url_masks_into(&Frame, ..)`; keep old
      functions until US-0049.
- [ ] `row_plan.rs`: style resolution, bg spans, text runs (wide/zero-width/hidden rules),
      shapes + coalescing, paths, decorations, class merge; tests listed in the LLD.
- [ ] `plan_cache.rs`: `StyleKey`, candidates, rotation, hash verify, mask delta; tests.
- [ ] `state.rs`: `RenderState`, `RenderInputs`, `Scratch`, overlays, gutter buffers.
- [ ] `overlay.rs`: selection/search rects, mask entry; `cursor.rs`: `resolve` + paint.
- [ ] `diagnostics.rs`: `FrameStats`, `LatencySamples`, throttled log.
- [ ] `element.rs`: `TerminalElementSpec`, `TerminalElement`, prepaint/paint per the LLD table.
- [ ] `element_tests.rs`: headless `gpui::test` window rendering the element with
      `FakeTerminalSession`; counting allocator test.
- [ ] Manual smoke: `cargo run -p oneterm-app --profile fast-dev` still renders via the old view.
- [ ] `pwsh scripts/ci-local.ps1`.

## Decisions

- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md` (items 1, 2, 5).

## Verification Plan

- Focused: `cargo test -p oneterm-terminal-view render` — `frame_*`, `metrics_*`,
  `glyph_cache_*`, `row_plan_*` (`bg_spans_merge_adjacent_same_color`,
  `inverse_swaps_colors_and_forces_bg`, `dim_reduces_alpha`, `hidden_cells_have_no_text_run`,
  `undercurl_maps_to_wavy`, `class_underline_only_without_ansi_underline`,
  `url_mask_forces_url_class`, `wide_char_occupies_two_columns_one_run`,
  `zero_width_marks_join_base_run`, `overflow_slot_keeps_background`,
  `block_run_coalesces_into_one_quad`), `plan_cache_*`
  (`scroll_rotates_plans_and_replans_only_scrolled_in_rows`,
  `cursor_row_replans_on_undamaged_change`, `selection_change_does_not_replan`,
  `style_key_change_replans_all`, `resize_replans_all_and_resizes_session`),
  `cursor_*`, `element_tests::{dirty_frame_plans_rows_and_shapes,
  idle_frame_plans_nothing_but_paints, idle_frame_allocates_nothing,
  glyph_cache_hits_across_rows, gutter_labels_use_fallbacks}`, `render_has_single_alacritty_file`.
- `cargo test -p oneterm-terminal-view theme` — `contrast_ratio_uses_wcag_exponent`,
  `min_contrast_zero_keeps_theme_default`, `min_contrast_one_disables`, existing 7 theme tests.
- Regression: `cargo test --workspace`.
- Gate: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

## Handoff

Depends on US-0046. Blocks US-0048 (hit-test contract) and US-0049 (element consumer).
