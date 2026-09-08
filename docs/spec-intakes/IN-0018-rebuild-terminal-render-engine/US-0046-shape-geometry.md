# Work: Shape geometry library (box drawing, blocks, shades, braille, powerline)

ID: US-0046
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

- Change type: new capability (first module of the rewrite; not wired into rendering yet)
- Risk lane: normal
- Spec Intake: IN-0018

## Outcome

`crates/terminal-view/src/render/shapes.rs` exists and turns any code point in U+2500–257F,
U+2580–259F, U+25AC, U+2800–28FF, U+E0B0–E0BF plus a device-pixel cell size into symmetric,
seam-free quads (and paths for arcs/diagonals) exactly as specified in
`low-level-design/shapes.md`, with the exhaustive test suite green. Nothing renders through it
yet; the module root carries `#[allow(dead_code)]` until US-0047 wires it.

## Scope

- [ ] In scope: `src/render/mod.rs` (new, declared in `lib.rs` under `#[allow(dead_code)]`),
      `src/render/shapes.rs`, `src/render/shapes_tests.rs`; `smallvec` only if it is already a
      workspace dependency, otherwise a fixed-size array for `ShapePath.ops`.
- [ ] Out of scope: any GPUI type, any change under the old `box_drawing/`, row planning,
      painting, sextants (U+1FB00+), font fallback decisions.

## Acceptance

- [ ] HLD parity items US-0046 37–42 and braille are satisfied by geometry (verified by the
      tests below, not by screenshots yet).
- [ ] Every code point in the five ranges returns `is_shape_char == true` and emits geometry
      (U+2800 exempt from emitting).
- [ ] Mirror-pair, self-symmetry, and `build(transform(def)) == transform(build(def))` tests
      pass on cell sizes `7×15, 8×16, 9×19, 14×29, 18×38, 36×76, 1×1`.
- [ ] `┼ == ─ ∪ │`, `╋ == ━ ∪ ┃`, adjacent `─` cells share the same y-interval, `╬` leaves the
      center empty.
- [ ] `╒ ≠ ╘`, `╓ ≠ ╙` (wart 3); all 16 powerline glyphs have distinct real geometry (wart 1);
      shades emit ≤ 24 rects at 36×76 and at 9×19 with 25/50/75 % ± 12 % coverage (wart 2).
- [ ] Per-cell quad budgets hold: Family A/C ≤ 6, braille ≤ 8, shades ≤ 24.
- [ ] `snap_interval` property test (10k samples) confirms symmetry for both anchors.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` — module map, deviations 1–3, 7, 9.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/shapes.md` — the geometry contract this packet implements.
- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md` — items 3 and 4.
- `docs/PROJECT.md` — invariants (no hard-coded colors: this module carries no colors).
- `docs/architecture.md` — crate boundaries (no new crate, no new edge).
- `docs/agents/code-style.md` — sibling `*_tests.rs` convention.

### Documentation Action

No contract change: `shapes.md` already specifies this module; if implementation forces a
numeric change (thickness table, shade pitch), update `shapes.md` in the same commit.

Reason: the LLD was written for this packet; the code must match it, not the other way round.

### Reconciliation

Before completion, confirm `shapes.md` tables (thickness, worked 9×19 / 14×29 values) match the
test constants, or amend the LLD.

## Context

- Old geometry lives in `src/box_drawing/` and must not be read as a reference for structure; the
  inventory describes its behavior and warts only.
- `DeviceRect` coordinates are cell-relative device pixels; row planning (US-0047) offsets them.
- `round_half_away` is `f32::round` in Rust (ties away from zero) — use it directly.
- No GPUI import: keep the module compilable under plain `cargo test` without a window.

## Plan

- [ ] Add `pub(crate) mod render;` (`#[allow(dead_code)]`) to `lib.rs` and `render/mod.rs` with `pub(crate) mod shapes;`.
- [ ] Implement primitives (`CellSizeDevicePx`, `DeviceRect`, `Stroke`, `CenterRect`, `Axis`, `Anchor`, `snap_interval`, transforms).
- [ ] Implement the thickness table and joint/rail intervals.
- [ ] Implement Family A (arm-set table + builder), then B (dashes), C (double/mixed rails), E (blocks), F (shades), G (braille).
- [ ] Implement paths: D (diagonals, rounded) and H (powerline), `ShapePath`/`PathOp`.
- [ ] Implement `is_shape_char`, `shape_quads`, `shape_paths`, `stroke_thickness`.
- [ ] Write `shapes_tests.rs` per the LLD verification list; run `cargo test -p oneterm-terminal-view shapes`.
- [ ] Run `pwsh scripts/ci-local.ps1`.

## Decisions

- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md` (items 3, 4).

## Verification Plan

- Focused: `cargo test -p oneterm-terminal-view shapes` — expected tests:
  `every_supported_code_point_emits_geometry`, `all_rects_within_cell_bounds`,
  `all_path_points_within_cell_bounds`, `mirror_x_pairs_match`, `mirror_y_pairs_match`,
  `self_symmetric_glyphs`, `builder_commutes_with_transforms`,
  `horizontal_line_abuts_across_cells`, `cross_equals_union_of_lines`,
  `corner_arms_meet_at_joint`, `double_corner_up_and_down_differ`, `dash_segment_counts`,
  `block_eighth_fractions_monotone`, `quadrant_union_is_full_block`, `braille_dot_slots`,
  `powerline_triangle_apex_at_center_height`, `shade_density_and_budget`,
  `snap_interval_is_even_about_center`, `heavy_thicker_than_light`, `rails_disjoint_with_gap`,
  `rounded_corner_meets_neighbors`, `per_cell_quad_budget`.
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

Blocks US-0047 (row planning consumes `shape_quads`/`shape_paths`).
