# Work: Shape geometry library (box drawing, blocks, shades, braille, powerline)

ID: US-0046
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

- [x] In scope: `src/render/mod.rs` (new, declared in `lib.rs` under `#[allow(dead_code)]`),
      `src/render/shapes.rs`, `src/render/shapes_tests.rs`; `smallvec` only if it is already a
      workspace dependency, otherwise a fixed-size array for `ShapePath.ops`.
- [x] Out of scope: any GPUI type, any change under the old `box_drawing/`, row planning,
      painting, sextants (U+1FB00+), font fallback decisions.

## Acceptance

- [x] HLD parity items US-0046 37–42 and braille are satisfied by geometry (verified by the
      tests below, not by screenshots yet).
- [x] Every code point in the five ranges returns `is_shape_char == true` and emits geometry
      (U+2800 exempt from emitting).
- [x] Mirror-pair, self-symmetry, and `build(transform(def)) == transform(build(def))` tests
      pass on cell sizes `7×15, 8×16, 9×19, 14×29, 18×38, 36×76, 1×1`.
- [x] `┼ == ─ ∪ │`, `╋ == ━ ∪ ┃`, adjacent `─` cells share the same y-interval, `╬` leaves the
      center empty (from 5 × 5 device pixels up).
- [x] `╒ ≠ ╘`, `╓ ≠ ╙` (wart 3); all 16 powerline glyphs have real geometry and the eight
      fills are pairwise distinct (wart 1); shades emit ≤ 24 rects at every size with
      25/50/75 % ± 13 points of coverage (wart 2). Two amendments to the LLD, below.
- [x] Per-cell quad budgets hold: Family A/C ≤ 8 (amended from 6: `╬` needs eight),
      braille ≤ 8, shades ≤ 24.
- [x] `snap_interval` property test (10k samples) confirms symmetry for both anchors.
- [ ] `pwsh scripts/ci-local.ps1` green — run by the orchestrator over the whole branch.

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

`low-level-design/shapes.md` amended in this packet (implementation forced the change, so the
LLD follows the code):

1. **Rounding.** `round_half_away` does not commute with reflection at ties
   (`round(0.5) = 1`, `7 - round(6.5) = 0`), which broke braille dot centers and shade tile
   edges. Every coordinate rounding now breaks ties **toward** the cell center, which does
   commute; `Anchor::Nearest` exactly on the cell center widens instead of moving, and an
   interval that would fall outside the cell is shifted in before clipping so that rails and
   dots still draw on a one pixel cell.
2. **`╒` / `╓` worked example.** The rects listed for `╒` are `╓`'s (U+2552 is DOWN SINGLE AND
   RIGHT DOUBLE, i.e. horizontal double); both are now listed with their Unicode names.
3. **Family C single arms.** `J⊥ := O` for every single arm crossing a double axis would put
   the stem of `╤` through both rails; it now stops at the near rail when it is a stub and at
   the far rail when it closes a corner or runs through (`╪`, `╫`).
4. **Quad budget.** Family A/C is ≤ 8, not ≤ 6: `╬` is two split rails per axis and no two of
   them are contiguous. The HLD performance budget row was corrected to match.
5. **Shades.** Rect counts for 9×19 are `░` 8, `▒` 11, `▓` 10 (the LLD said 14 / 11), and the
   density tolerance is ±13 points: an odd tile grid with three columns makes `░` and `▓` run
   up to 12.5 points heavy, and a finer grid would break the 24-quad budget.
6. **Smaller wording fixes.** One-eighth blocks clamp to at least one pixel; the outer `┄`
   segments end within one dash gap of the edge rather than always touching it; the four
   powerline corner outlines are two distinct diagonals by definition (E0B9 == E0BF,
   E0BB == E0BD), so distinctness is asserted over the eight fills.

Thickness table values were confirmed against the implementation for 7/8/9/14/18 px widths;
no numeric change was needed there.

## Context

- Old geometry lives in `src/box_drawing/` and must not be read as a reference for structure; the
  inventory describes its behavior and warts only.
- `DeviceRect` coordinates are cell-relative device pixels; row planning (US-0047) offsets them.
- `round_half_away` is `f32::round` in Rust (ties away from zero) — use it directly.
- No GPUI import: keep the module compilable under plain `cargo test` without a window.

## Plan

- [x] Add `pub(crate) mod render;` (`#[allow(dead_code)]`) to `lib.rs` and `render/mod.rs` with `pub(crate) mod shapes;`.
- [x] Implement primitives (`CellSizeDevicePx`, `DeviceRect`, `Stroke`, `CenterRect`, `Axis`, `Anchor`, `snap_interval`, transforms).
- [x] Implement the thickness table and joint/rail intervals.
- [x] Implement Family A (arm-set table + builder), then B (dashes), C (double/mixed rails), E (blocks), F (shades), G (braille).
- [x] Implement paths: D (diagonals, rounded) and H (powerline), `ShapePath`/`PathOp`.
- [x] Implement `is_shape_char`, `shape_quads`, `shape_paths`, `stroke_thickness`.
- [x] Write `shapes_tests.rs` per the LLD verification list; run `cargo test -p oneterm-terminal-view shapes`.
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
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (branch `refactor/terminal-render-engine`, Windows):

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean.
- `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` —
  `Finished dev profile`, no warnings.
- `cargo test -p oneterm-terminal-view shapes` — `22 passed; 0 failed; 1 ignored;
  193 filtered out`.
- `cargo test -p oneterm-terminal-view` — `215 passed, 1 ignored` (193 pre-existing + 22 new;
  the ignored one is `shape_bitmaps_for_visual_review`, an ASCII dump for eyeballing joints).
- Visual review: `cargo test -p oneterm-terminal-view shape_bitmaps -- --ignored --nocapture`
  at 9×19 and 8×16 for `┌ ┼ ╔ ╬ ╒ ╘ ╤ ╟ ▚ ░ ▒ ▓ ⣿`; joints meet, rails cross correctly,
  corners are symmetric, `╬` keeps its center hole, `╒`/`╘` differ.

Files: `src/render/mod.rs` (3 lines), `src/render/shapes.rs` (1289),
`src/render/shapes_tests.rs` (792), plus four lines in `src/lib.rs`.

Gaps:

- Nothing renders through this module yet; US-0047 wires it and removes `#[allow(dead_code)]`
  in US-0050.
- No screenshot evidence: parity items 37–42 are proved by geometry tests only, as scoped.
- `pwsh scripts/ci-local.ps1` not run here (whole-branch gate, owned by the orchestrator).

## Handoff

Blocks US-0047 (row planning consumes `shape_quads`/`shape_paths`).
