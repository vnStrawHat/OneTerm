# Work: Render core (frame model, row plans, plan cache, glyph cache, GPUI element)

ID: US-0047
Intake: IN-0018
Created: 2026-09-08

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

- [x] In scope: `src/render/{frame, metrics, glyphs, row_plan, plan_cache, state, element,
      cursor, overlay, diagnostics}.rs`, `src/render/element_tests.rs`; adapters in retained
      modules so they accept the new types: `theme/` (`TerminalTheme::color(Color) -> Hsla`
      table, `ensure_contrast` with exponent 2.4, `min_contrast` default rule — deviations 5, 6),
      `url/mask.rs` (`FrameRow` input, `url_masks_into` reuse), `highlight/overlay.rs`
      (`scan_into` reusing a class buffer). Old callers of those modules keep compiling (add
      the new entry points beside the old ones; remove the old ones in US-0049).
- [x] Out of scope: input handling, `TerminalView`, panel/space, scrollbar, search UI, gutter
      timestamp bookkeeping (the element only formats and paints labels handed to it),
      completion, deleting old files.

## Acceptance

- [x] HLD parity items US-0047 1–36, §2.15 (1–5, 7–9), §2.22 are implemented in `render/`
      and covered by the tests below (26 scrollbar and 28 bell badge stay with the view).
- [x] `dirty_frame_plans_rows_and_shapes` and `idle_frame_plans_nothing_but_paints` pass:
      idle frame `rows_planned == 0`, `shape_calls == 0`, `url_scans == 0`, `quads > 0`;
      dirty frame `snapshot_calls == 1`.
- [x] `scroll_rotates_plans_and_replans_only_scrolled_in_rows` passes with `Damage::Full`
      (deviation 8) — a `plan_cache` unit test driven by `FrameBuilder` (the fake session cannot
      scroll).
- [x] `block_run_coalesces_into_one_quad`: 40 `█` → 1 quad; 40 `─` → 1 quad.
- [x] `idle_frame_allocates_nothing`: zero allocations around `PlanCache::update` + overlays +
      cursor resolution on the second identical frame (counting `#[global_allocator]` with
      thread-local counters, self-checked against a deliberate `vec!`).
- [x] `metrics_snap_cell_to_device_pixels` at 1.0 / 1.25 / 1.5 / 2.0.
- [x] No module outside `render/frame.rs` (and the retained `theme/palette.rs` conversion of
      `TerminalPalette`) imports `alacritty_terminal` in `render/`: enforced by a test that greps
      `src/render/*.rs` for `alacritty_terminal` and expects only `frame.rs`.
- [x] `theme` tests: `contrast_ratio_uses_wcag_exponent` (black/white = 21.0 ± 0.01;
      #777777 on white ≈ 4.48), `min_contrast_zero_keeps_theme_default`,
      `min_contrast_one_disables`.
- [ ] `pwsh scripts/ci-local.ps1` green; old view still renders (manual smoke run) — left to
      the orchestrator (whole-branch gate); focused fmt/clippy/test evidence below.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` — frame pipeline, caches, invalidation rules, interfaces, deviations 4–8, 10, performance budget.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md` — the contract this packet implements.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/shapes.md` — `DeviceRect`/`ShapePath` consumed here.
- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md` — items 1, 2, 5.
- `docs/PROJECT.md` — invariants; `docs/architecture.md` — crate boundaries (engine stays in `crates/terminal`).
- `docs/agents/crate-dependency-rules.md` — R7 (view maps engine types to GPUI).

### Documentation Action

Update required: `render-pipeline.md` if any struct/field or the plan-cache pseudo-code
changes during implementation; `high-level-design.md` Cross-frame State table if a new buffer
is added.

Reason: the LLD is the owning contract for the engine's data structures; implementation drift
must be reflected there so US-0048/0049 build on the real shapes.

### Reconciliation

Before completion, diff `render-pipeline.md` type listings against `src/render/*.rs`
signatures and record the result.

Result (2026-09-08): `render-pipeline.md` amended to the implemented shapes — `RowPlan` with
flattened `colors` / `path_ops`, `PlanContext` fields, `build_row_plan` signature, word-sized
text runs, `Vec` open-rect scratch, four-phase `PlanCache::update` (URL scan gated on
hash-verified changes), `FontSet`, `CursorConfig` / `ResolvedCursor` / `CursorPaint::build`,
`url::url_masks_into(frame, masks, wraps)`, `SearchHighlight` in `overlay.rs`, always-compiled
`FrameStats` (+ `glyph_errors`), `DiagnosticsLog`, the element background quads, geometry
written in prepaint, the allocation plan and the `layers == 1` test note; its Edge Cases and
Verification boxes are ticked. `high-level-design.md` Cross-frame State table lists the new
buffers (`dirty`, `wraps`, `fonts`, cached metrics, gutter labels) and the Invalidation Rules
row for the URL mask states the hash-verified gating. `shapes.md` unchanged.

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

- [x] `frame.rs`: `Frame`, `FrameRow`, `Cell`, `Color`, `CellFlags`, `CursorShape`, `Cursor`,
      `Selection`, `Damage`, conversions, row hash, `text_into`, tests for conversions and
      selection mapping.
- [x] `metrics.rs`: `CellMetrics`, `measure`, `GridGeometry`, `grid_size_for`, `pixel_to_grid`,
      tests (device snapping at four scales, gutter/padding subtraction, ≥ 1×1).
- [x] `theme/`: `Color → Hsla` table rebuilt on palette change; WCAG 2.4 luminance;
      `min_contrast` rule; tests.
- [x] `glyphs.rs`: `FontKey`, `GlyphCache::{begin_frame, shape}`, eviction; tests.
- [x] `highlight/overlay.rs` `scan_into`; `url/mask.rs` `url_masks_into(&Frame, ..)`; keep old
      functions until US-0049.
- [x] `row_plan.rs`: style resolution, bg spans, text runs (wide/zero-width/hidden rules),
      shapes + coalescing, paths, decorations, class merge; tests listed in the LLD.
- [x] `plan_cache.rs`: `StyleKey`, candidates, rotation, hash verify, mask delta; tests.
- [x] `state.rs`: `RenderState`, `RenderInputs`, `Scratch`, overlays, gutter buffers.
- [x] `overlay.rs`: selection/search rects, mask entry; `cursor.rs`: `resolve` + paint.
- [x] `diagnostics.rs`: `FrameStats`, `LatencySamples`, throttled log.
- [x] `element.rs`: `TerminalElementSpec`, `TerminalElement`, prepaint/paint per the LLD table.
- [x] `element_tests.rs`: headless `gpui::test` window rendering the element with
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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (branch `refactor/terminal-render-engine`, Windows, 2026-09-08):

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean.
- `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` — clean (no output).
- `cargo clippy --workspace --all-targets -- -D warnings` — exit 0.
- `cargo clippy -p oneterm-terminal-view --all-targets --features terminal-diagnostics -- -D warnings`
  — fails on a **pre-existing** dead-code error in the old view
  (`view/local_view.rs:350 render_diagnostics` is `cfg(any(test, feature))` but only called from
  tests); the new `render/` code is clean under the feature. Left untouched (US-0049 deletes it).
- `cargo test -p oneterm-terminal-view` — `274 passed, 1 ignored` (215 pre-existing + 59 new).
- `cargo test -p oneterm-terminal-view render` — `81 passed, 1 ignored` (59 new + 22 shapes).
- `cargo test -p oneterm-terminal-view theme` — `13 passed`.
- `FrameStats` from `element_tests` (`--nocapture`, 1024×768 test window → 69×128 grid):
  - first frame: `snapshot_calls: 1, rows_total: 69, rows_candidate: 69, rows_planned: 69,
    shape_calls: 3, url_scans: 1, quads: 3, glyphs: 31, layers: 1`
  - dirty frame (`set_text`): `snapshot_calls: 1, rows_candidate: 69, rows_planned: 4,
    shape_calls: 1, glyph_hits: 1, url_scans: 1, quads: 1, glyphs: 10, layers: 1`
  - idle frame: `snapshot_calls: 1, rows_candidate: 1, rows_planned: 0, shape_calls: 0,
    url_scans: 0, quads: 3, glyphs: 8, layers: 1`
  - `idle_frame_allocates_nothing`: 0 allocations across `update_plans` + `compute_overlays` +
    `resolve_cursor`.

New tests (59): `frame` 6, `metrics` 6, `glyphs` 3, `row_plan` 13, `plan_cache` 7, `overlay` 3,
`cursor` 4, `state` 2, `diagnostics` 1, `element_tests` 7, `theme` 4, `highlight/overlay` 1,
`url/mask` 2.

Files: `src/render/{frame 872, metrics 317, glyphs 263, row_plan 928, plan_cache 460, overlay
215, cursor 290, state 504, diagnostics 151, element 460, element_tests 381, mod 17}.rs`;
adapted `theme/{palette, terminal_theme, contrast, mod, tests}.rs`, `highlight/overlay.rs`,
`url/{mask, mod}.rs`; `crates/highlight/src/lib.rs` (+ `scan_line_into` re-export).

Deviations from the LLD/HLD (all recorded in `render-pipeline.md`):

1. `FrameStats` counters are always compiled; only `LatencySamples`, timers and the log are
   cfg-gated. `glyph_errors` added.
2. `RowPlan` flattens `ColorSpan`s and `PathOp`s per row (`color_start/end`, `op_start/end`)
   so a rebuild never allocates a per-run `Vec`; `ShapePathPlan` carries `style` instead of a
   `ShapePath`.
3. `Scratch.open: SmallVec` → `open_prev` / `open_cur: Vec<usize>` (no `smallvec`); `paths`
   scratch added; `label` doubles as the cursor glyph text.
4. `PlanCache::update` runs in four phases; the URL scan is gated on hash-verified changes,
   not on candidates (the cursor row is a candidate every frame, so the LLD order would rescan
   on every idle frame). `dirty` bitset and `wraps` scratch added.
5. `url_masks_into(frame, masks, wraps)` lives in `url/mask.rs` (packet scope), `overlay.rs`
   only holds selection/search; `SearchHighlight` is defined in `overlay.rs`.
6. Text runs split at blank cells (word-sized runs) — better glyph-cache reuse; output
   identical because `force_width` places glyphs per cell.
7. Cursor resolution is split into a pure `resolve` (unit-tested without a window) and
   `CursorPaint::build`; a block cursor covers two columns over a wide char.
8. `state.geometry` is written in prepaint (input handlers see it before paint).
9. The element paints its own `theme.bg` background quad and the `gutter_bg` quad; overlays
   are painted after all rows' bg/shape quads.
10. `TerminalTheme.colors: ColorTable` is a field rebuilt by `build_terminal_theme` /
    `apply_color_overrides` / `apply_dynamic_colors` (269 entries incl. dim ANSI and
    bright/dim fg); the two theme test literals gained the field.
11. `oneterm_highlight::scan_line_into` is re-exported at the highlight crate root (it existed
    but was not exported) so `SemanticOverlay::scan_into` can reuse its buffer.
12. `layers == 1` in the dirty/idle tests needs cursor-less inputs (the fake reports a visible
    block cursor at (0, 0)); `cursor_layer_and_gutter_paint` covers the two-layer path.
13. `FrameRow::hash` reads the engine cell directly (no `Cell` conversion) because
    `Damage::Full` hashes every row.

Gaps:

- Nothing renders through `render/element.rs` in the app yet (US-0049 wires the view);
  `#[allow(dead_code)]` stays on `pub(crate) mod render` until US-0050.
- No screenshot / manual smoke evidence from this packet; the old view is untouched apart from
  the two shared theme behavior changes (deviations 5 and 6 of the HLD).
- `pwsh scripts/ci-local.ps1` not run here (whole-branch gate, owned by the orchestrator).
- `terminal-diagnostics` feature clippy blocked by the pre-existing old-view dead code above.

## Handoff

For US-0048 (input): read `RenderState.geometry: Option<GridGeometry>` (written in prepaint)
through the shared `Rc<RefCell<RenderState>>`; `GridGeometry::{cell_at, pixel_to_grid,
cell_origin}` is the hit-test contract; `state.frame` holds the last snapshot (`selection()`,
`cursor()`, `app_cursor()`, `alt_screen()`, `row(r).hyperlink_uri(col)`) — never call
`snapshot_into` outside the element.

For US-0049 (view): before building `TerminalElement`, refresh `state.inputs: RenderInputs`
in place (`theme: Rc<TerminalTheme>` replaced only when the palette/overrides/dynamic colors
change, `font`, `font_size`, `line_height_factor`, `cell_width_override`, `padding`,
`show_gutter`, `cursor: CursorConfig { shape, color, focused, blink_visible }`, `gutter:
GutterInputs { times, base, absolute_line_count }`, `search` refilled in place, `semantic`
(owned there: `set_enabled` / `set_profile`), `url_hovering`); build
`TerminalElementSpec { id, session, state, ime }` where `ime` is
`Some(Box::new(move |bounds, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, view), cx)))`
only while focused (the element calls it once, in paint, after releasing the state borrow).
Diagnostics: `state.stats` is the last frame's `FrameStats`.



Depends on US-0046. Blocks US-0048 (hit-test contract) and US-0049 (element consumer).
