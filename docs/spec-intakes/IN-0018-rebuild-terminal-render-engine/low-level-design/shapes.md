# Low-Level Design: Shape geometry (box drawing, blocks, shades, braille, powerline)

Intake: IN-0018
HLD: ../high-level-design.md
Topic: shapes
Date: 2026-09-08

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

`src/render/shapes.rs`: pure geometry that turns one code point plus a device-pixel cell size
into quads (`DeviceRect`) and, for arcs/diagonals, paths (`ShapePath`). No GPUI types, no
alacritty types, no allocation beyond the caller's output vectors. Owns the symmetry contract of
DEC-0007 item 4.

## Design

### Cell space

- The cell is `W × H` whole device pixels (`CellSizeDevicePx { w: i32, h: i32 }`, both ≥ 1),
  origin at the cell's top-left, `+x` right, `+y` down.
- The **center** is `C = (W/2, H/2)` as `f32` — an integer when the size is even, a
  half-integer when odd. All definitions are offsets from `C`.
- A **device interval** is `[lo, hi)` with integer `lo < hi`; a `DeviceRect` is the product of
  an x-interval and a y-interval, clipped to `[0, W) × [0, H)`; empty rects are dropped.

### Primitives

```rust
enum Axis { Horizontal, Vertical }

/// A centered bar. `cross` offsets the bar's centerline from `C` perpendicular to `axis`,
/// `along` offsets the bar's midpoint from `C` along `axis`; extents are half sizes.
struct Stroke { axis: Axis, cross: f32, along: f32, half_len: f32, half_thick: f32 }

/// An axis-aligned rect given by its center and half extents (used for blocks, dots, tiles).
struct CenterRect { cx: f32, cy: f32, half_w: f32, half_h: f32 }

struct DeviceRect { x: i32, y: i32, w: i32, h: i32 }    // cell-relative device pixels
```

A `Stroke` on `Axis::Horizontal` becomes `CenterRect { cx: C.x + along, cy: C.y + cross,
half_w: half_len, half_h: half_thick }`; on `Vertical`, `cx: C.x + cross, cy: C.y + along,
half_w: half_thick, half_h: half_len`.

### Symmetric snapping

`snap_interval(center: f32, half: f32, size: i32, anchor: Anchor) -> (i32, i32)`:

1. `t = max(1, round_half_away(2 * half))` — integer thickness.
2. `c = round_ties_toward_center(2 * center) / 2` — nearest integer or half-integer, with
   ties broken **toward the cell center**. `round_half_away` is not mirror covariant
   (`round(0.5) = 1`, but `7 - round(6.5) = 0`), so every rounding of a coordinate in this
   module resolves ties toward the reflection's fixed point instead. Braille dot centers,
   shade tile edges and rounded-corner stubs all land on ties at some cell sizes.
3. Parity: a symmetric interval needs `t` even when `c` is an integer and `t` odd when `c` is a
   half-integer.
   - `Anchor::Fixed` (the centerline may not move; used for anything centered on `C`): on
     mismatch `t += 1`.
   - `Anchor::Nearest` (the thickness may not change; used for rails, dots, dash segments,
     tiles): on mismatch move `c` by `±0.5` to the nearest parity-valid position, ties broken
     **away from the cell center** (so gaps between paired features never close). Exactly
     on the cell center there is no "away": `t += 1` as for `Fixed`, since moving would
     break the feature's own symmetry.
4. `lo = c - t/2` (exact integer), `hi = lo + t`. An interval that would fall outside the
   cell is **shifted in** (`lo` clamped into `[min(0, size - t), max(0, size - t)]`) and
   only then clipped to `[0, size]`: on a one or two pixel cell the rails of a double line
   and the braille dots must still draw something. The shift is symmetric, so reflection
   still commutes with snapping.

Properties (asserted by tests): the interval is symmetric about `c`; for any `v`,
`snap_interval(size - v, ...)` is the mirror of `snap_interval(v, ...)`, because ties toward
the center give `round(size - x) == size - round(x)` for integer `size`; two strokes with
the same `(center, half)` snap identically, so a horizontal line's y-interval in cell *n* equals
the one in cell *n + 1* and the joint square of `┼` is exactly `J_x × J_y`.

Consequence to accept: on an even cell dimension a nominal 1 px centered stroke becomes 2 px
(`Fixed` parity), and when `W` and `H` have different parity `─` and `│` can differ by one
device pixel. This is the price of exact symmetry and is recorded in the HLD risks.

### Transforms

Applied to **definitions**, never to emitted rects:

| Transform | `Stroke` (axis H) | `Stroke` (axis V) | `CenterRect` | path point `(x, y)` |
| --- | --- | --- | --- | --- |
| `mirror_x` | `along = -along` | `cross = -cross` | `cx = W - cx` | `(W - x, y)` |
| `mirror_y` | `cross = -cross` | `along = -along` | `cy = H - cy` | `(x, H - y)` |
| `rotate_cw` | applies only to arm sets (slot permutation below); never to strokes, rects, or paths | | | |

Arm sets (below) are transformed by permuting arm slots: `mirror_x` swaps `left ↔ right`,
`mirror_y` swaps `up ↔ down`, `rotate_cw` maps `up → right → down → left → up` (the builder
re-derives thickness per axis, so rotation is valid on non-square cells). The code-point table
stores a canonical definition plus a transform; the test
`build(transform(def)) == transform_rects(build(def))` proves the builder is symmetric.

### Thickness rules (device px; `W` = device cell width)

| Name | Value | 7 px | 8 px | 9 px | 14 px | 18 px |
| --- | --- | --- | --- | --- | --- | --- |
| light `t_l` | `max(1, round(W / 8))` | 1 | 1 | 1 | 2 | 2 |
| heavy `t_h` | `max(t_l + 2, round(W / 3))` | 3 | 3 | 3 | 5 | 6 |
| double rail `t_d` | `t_l` | 1 | 1 | 1 | 2 | 2 |
| double gap `g` | `max(1, t_l)` | 1 | 1 | 1 | 2 | 2 |
| rail offset `d` | `(t_d + g) / 2` (nominal, `Nearest`-snapped) | 1 | 1 | 1 | 2 | 2 |
| dash gap `g_dash` | `max(1, round(W / 8))` | 1 | 1 | 1 | 2 | 2 |

Nominal values pass through `snap_interval`, so the painted thickness may be one pixel more
(parity). Height uses the same table with `W` (not `H`) so horizontal and vertical strokes start
from the same nominal.

Joint intervals: `J_x = snap_interval(C.x, t/2, W, Fixed)`, `J_y = snap_interval(C.y, t/2,
H, Fixed)` for `t ∈ {t_l, t_h}`. Double rails: `R±_x = snap_interval(C.x ± d, t_d/2, W,
Nearest)`, likewise `R±_y`; the **outer interval** `O_x = [R-_x.lo, R+_x.hi)`.

### Family A — lines, corners, tees, crosses, half lines (U+2500–254B, U+2574–257F)

Every code point is an **arm set**: `ArmSet { up, down, left, right: Option<Weight> }`,
`Weight ∈ { Light, Heavy }`. Construction:

1. **Opposite arms of equal weight merge** into one full-length centered `Stroke` (`along = 0`,
   `half_len = W/2` or `H/2`, `half_thick = t/2`, `Fixed`).
2. **Every other arm** is a rect from the cell edge to the joint: right arm `[J⊥.lo, W) × J_y(w)`,
   left arm `[0, J⊥.hi) × J_y(w)`, up `J_x(w) × [0, J⊥.hi)`, down `J_x(w) × [J⊥.lo, H)`, where
   `w` is the arm's own weight and `J⊥` is the joint interval on the perpendicular axis using
   the **heaviest weight present on that axis** (falls back to the arm's own weight when the
   perpendicular axis has no arms, so `╶` + `╴` in adjacent cells still meet).
3. Arm sets for the 76 + 12 code points are a literal table in the source (`(char, ArmSet)`),
   generated by hand from the Unicode chart names (`BOX DRAWINGS HEAVY DOWN AND LIGHT RIGHT` →
   `down: Heavy, right: Light`). The table lists canonical entries for one orientation and
   `(canonical, transform)` for mirrored names, e.g. `┐ = mirror_x(┌)`, `┘ = mirror_y(┐)`,
   `┤ = mirror_x(├)`, `┴ = mirror_y(┬)`, `╴ = mirror_x(╶)`.

Because every rect is derived from symmetric joint intervals, `┼ == ─ ∪ │` pixel for pixel and
adjacent `─` cells share `J_y`.

### Family B — dashes (U+2504–250B, U+254C–254F)

`n ∈ {2, 3, 4}` segments of a light/heavy stroke: segment pitch `p = W/n` (or `H/n`), segment
half-length `(p - g_dash)/2`, centers at `C + (k + 0.5 - n/2) * p` for `k in 0..n`; the middle
segment of odd `n` is `Fixed`, others `Nearest`. Along-extents are clipped so segments never
overlap (`hi_k ≤ lo_{k+1}`); if snapping closes a gap the later segment's `lo` is raised by one
pixel and its `hi` kept, and the symmetric partner is adjusted identically by construction.
Cross-extent = `J_y(w)` / `J_x(w)`.

### Family C — double and mixed (U+2550–256C)

Arm sets with `Weight::Double` allowed. A double arm is **two rails** (side `−` and side `+`),
each a rect on `R±` of the arm's cross axis. Along-extent rules for a rail on cross side `s` of a
horizontal right arm (all others follow by transform):

| Perpendicular (vertical) arms present | Rail `s` starts at |
| --- | --- |
| an arm on side `s` (e.g. `s = −` ⇔ `up`) | `R+_x.lo` when the vertical arm is Double, `J_x.lo` when Light/Heavy |
| no arm on side `s`, but one on the other side | `R-_x.lo` (Double) / `J_x.lo` (single) — closes the outer corner |
| no vertical arms | `O_x.lo` (Double) / `J_x.lo` (single) — a stub that meets its neighbor |

Rails are emitted per arm and then merged when they share a cross interval and touch, so no
separate merge rule is needed: the two rails of `═` come out full length and the four rails of
`╬` come out split, from the same table.

A single (light or heavy) arm whose perpendicular axis is Double stops at the **near** rail when
it is a stub — no opposite arm, perpendicular arms on both sides (`╤`, `╧`, `╟`, `╢`) — and at
the far rail (`J⊥ := O`) otherwise, which closes the outer corner of `╒`/`╓` and lets the
through-going stems of `╪`/`╫` cross both rails.

Worked results on 9 × 19 (`t_l = 1`, `R-_x = [3,4)`, `R+_x = [5,6)`, `R-_y = [8,9)`,
`R+_y = [10,11)`):

- `╔`: `[3,9)×[8,9)`, `[3,4)×[8,19)`, `[5,9)×[10,11)`, `[5,6)×[10,19)`.
- `╬`: rails `[0,4)×[8,9)`, `[5,9)×[8,9)`, `[0,4)×[10,11)`, `[5,9)×[10,11)` and the vertical
  counterparts; the center `[4,5)×[9,10)` stays empty.
- `╓` U+2553 (down Double, right Light): `[3,9)×[9,10)`, `[3,4)×[9,19)`, `[5,6)×[9,19)`;
  `╙ = mirror_y(╓)`: `[3,9)×[9,10)`, `[3,4)×[0,10)`, `[5,6)×[0,10)`.
- `╒` U+2552 (right Double, down Light — the transpose of `╓`): `[4,9)×[8,9)`,
  `[4,9)×[10,11)`, `[4,5)×[8,19)`; `╘ = mirror_y(╒)` differs from it (fixes inventory
  wart 3).

### Family D — diagonals and rounded corners (U+256D–2573)

Paths only (`ShapePath`), stroked with width `t_l` (device px, converted to logical at paint):

- `╱` U+2571: line `(0, H) → (W, 0)`; `╲` = `mirror_x`; `╳` = both.
- `╭` U+256D (arc down and right): radius `r = min(W, H) / 2`, arc from `(C.x, C.y + r)` to
  `(C.x + r, C.y)` around `(C.x + r, C.y + r)` as one cubic with `κ = 0.5523`:
  `P1 = (C.x, C.y + r − κr)`, `P2 = (C.x + r − κr, C.y)`. Straight stubs to the edges are
  quads: `J_x × [C.y + r, H)` and `[C.x + r, W) × J_y` (empty when `r` reaches the edge).
  `╮ = mirror_x(╭)`, `╰ = mirror_y(╭)`, `╯ = mirror_x(mirror_y(╭))`.

### Family E — blocks (U+2580–259F except shades)

Edge-anchored rects; `half_w = (W + 1) / 2`, `half_h = (H + 1) / 2` (integer division, i.e.
ceil), eighths `e_k(S) = clamp(round_half_away(k * S / 8), 1, S)` — a one-eighth block that
rounds to zero pixels would drop the character entirely on a small cell.

| Code points | Rect |
| --- | --- |
| `▁▂▃▄▅▆▇█` U+2581–2588 (lower k/8) | `[0, W) × [H − e_k(H), H)` |
| `▀` U+2580, `▔` U+2594 | `mirror_y` of `▄`, `▁` |
| `▉▊▋▌▍▎▏` U+2589–258F (left 7/8 … 1/8) | `[0, e_k(W)) × [0, H)` |
| `▐` U+2590, `▕` U+2595 | `mirror_x` of `▌`, `▏` |
| `▘` U+2598 | `[0, half_w) × [0, half_h)`; `▝`/`▖`/`▗` by `mirror_x`/`mirror_y`/both |
| `▚` U+259A, `▞` U+259E | `▘ ∪ ▗`, `▝ ∪ ▖` |
| `▙ ▛ ▜ ▟` U+2599, 259B, 259C, 259F | three quadrants (all but `▝`, `▗`, `▖`, `▘`) |
| `▬` U+25AC | `Stroke { H, cross 0, along 0, half_len W/2, half_thick H/4 }` |

Sextants (U+1FB00–1FB3B) are out of scope.

### Family F — shades (U+2591–2593)

A dot-grid whose pitch scales with the cell: `p = max(2, round(W / 3))`, `n_x = ceil(W / p) | 1`,
`n_y = ceil(H / p) | 1` (forced odd so reflection maps tile `i → n − 1 − i` and preserves parity).
Tile `(i, j)` covers `[C.x + (i − n_x/2) p, C.x + (i + 1 − n_x/2) p) × …` with edges rounded
ties-toward-center (as everywhere else in this module: tile edges land on ties whenever `p` is
even and the cell dimension odd) and clipped. "On" tiles: `░`: `(i + j) even ∧ j even`; `▒`: `(i + j) even`; `▓`:
`¬((i + j) even ∧ j odd)`. Rects are emitted per tile row with horizontally adjacent on-tiles
merged, giving ≤ `n_y · ceil(n_x / 2)` rects — for 9 × 19 (`p = 3`, 3 × 7 tiles): `░` 8, `▒` 11,
`▓` 10; the same at 36 × 76 (`p = 12`), and 15 / 23 / 17 at 7 × 15 (`p = 2`, 5 × 9 tiles), which
is the worst case for the budget. No size cliff (fixes wart 2).

Because `n_x` must be odd, a three-column grid has two of its three columns "on" for `░`, so the
light and dark patterns run up to 13 percentage points heavier than their nominal 25 % / 75 %
(`▒` stays at 50 %). Making the grid finer would fix the density but break the 24-quad budget on
tall cells, so the density tolerance is ±13 points.

### Family G — braille (U+2800–28FF)

Dot bit → slot: `0x01 (0,0) 0x02 (0,1) 0x04 (0,2) 0x08 (1,0) 0x10 (1,1) 0x20 (1,2) 0x40 (0,3)
0x80 (1,3)` as `(column, row)`. Dot diameter `s = max(1, round(min(W/2, H/4) * 0.6))`; centers
`cx = C.x ± W/4`, `cy = C.y + (row − 1.5) * H/4`; each dot is `CenterRect` snapped `Nearest` on
both axes. U+2800 emits nothing (still a shape char: no font glyph, keeps bg).

9 × 19: `s = 3`; x centers `2.5 → [1,4)`, `6.5 → [5,8)`; y centers `2.5, 7.5, 11.5, 16.5 →
[1,4) [6,9) [10,13) [15,18)`.

### Family H — powerline (U+E0B0–E0BF), all paths

| Code | Definition (device px, unsnapped) |
| --- | --- |
| E0B0 | fill `(0,0) → (W, C.y) → (0,H)`; **apex at `C.y`** |
| E0B1 | stroke polyline `(0,0) → (W, C.y) → (0,H)` |
| E0B2 / E0B3 | `mirror_x` of E0B0 / E0B1 |
| E0B4 | fill: `(0,0)`, cubic arc through `(W, C.y)` to `(0,H)` (two cubics, `κ`), close |
| E0B5 | stroke of the same arc |
| E0B6 / E0B7 | `mirror_x` of E0B4 / E0B5 |
| E0B8 | fill `(0,0) → (W,H) → (0,H)` (lower-left triangle) |
| E0B9 | stroke `(0,0) → (W,H)` |
| E0BA / E0BB | `mirror_x` of E0B8 / E0B9 |
| E0BC / E0BD | `mirror_y` of E0B8 / E0B9 |
| E0BE / E0BF | `mirror_x(mirror_y)` of E0B8 / E0B9 |

Fills use `PathStyle::Fill`; strokes use width `t_l`. Fixes inventory wart 1: all sixteen have
real geometry and all eight fills are distinct shapes. The four corner outlines are only two
distinct diagonals by definition (E0B9 == E0BF and E0BB == E0BD as point sets), which is what
the powerline-extra font draws as well.

## Interfaces

```rust
pub(crate) struct CellSizeDevicePx { pub w: i32, pub h: i32 }
pub(crate) struct DeviceRect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

pub(crate) enum PathOp { Move(DevicePoint), Line(DevicePoint), Cubic(DevicePoint, DevicePoint, DevicePoint), Close }
pub(crate) struct DevicePoint { pub x: f32, pub y: f32 }
pub(crate) enum PathStyle { Fill, Stroke { width: f32 } }
pub(crate) struct ShapePath { pub style: PathStyle, pub ops: SmallVec<[PathOp; 8]> }

/// True for every code point this module draws (row planning skips the font for them).
pub(crate) fn is_shape_char(c: char) -> bool;
/// Append the quads of `c`; returns false (and appends nothing) when `c` is not a shape char.
pub(crate) fn shape_quads(c: char, cell: CellSizeDevicePx, out: &mut Vec<DeviceRect>) -> bool;
/// Append the paths of `c` (rounded corners, diagonals, powerline); false when it has none.
pub(crate) fn shape_paths(c: char, cell: CellSizeDevicePx, out: &mut Vec<ShapePath>) -> bool;
/// Exposed for tests and metrics: light/heavy thickness for a cell width.
pub(crate) fn stroke_thickness(cell: CellSizeDevicePx) -> StrokeThickness { light, heavy, rail, gap }
```

Rects are appended in a deterministic order (arms in `up, down, left, right` order, rails
`−` then `+`), which row planning relies on only for reproducibility, not correctness.
`shape_quads` and `shape_paths` are pure and allocation-free apart from `out`.

## Edge Cases and Failure Modes

- [ ] `W == 1` or `H == 1`: every interval clips or shifts into the cell; nothing panics;
      `┼` is one pixel, and so is every double corner (the `╬` center hole and the
      `╒`/`╘` distinction need at least 5 × 5).
- [ ] Even cell dimension: `Fixed` parity widens a nominal 1 px stroke to 2 px (documented).
- [ ] `W` even and `H` odd: `─` and `│` differ by 1 px; `┼` is still their exact union.
- [ ] Rails on a narrow cell (`W ≤ 4`): `Nearest` snapping keeps `R-` and `R+` disjoint; when
      impossible (`W < 2 t_d + 1`) the rails may touch but never cross.
- [ ] Dash gaps at tiny widths (`W < 2n`): segments degrade to 1 px each, gaps may vanish; the
      segment count test only runs for `W ≥ 8`.
- [ ] Shade tiles overhanging the cell are clipped symmetrically because `n` is odd.
- [ ] Unknown code point in a family range (none exist in U+2500–259F; U+E0B0–E0BF fully
      covered) returns `false`.
- [ ] Braille U+2800 draws nothing but is a shape char (no font fallback).

## Verification

`src/render/shapes_tests.rs`, all pure `#[test]`s, run on cell sizes `{7×15, 8×16, 9×19, 14×29,
18×38, 36×76, 1×1}` unless stated:

- [ ] `every_supported_code_point_emits_geometry`: for each code point in U+2500–257F,
      U+2580–259F, U+25AC, U+2800–28FF, U+E0B0–E0BF, `is_shape_char` is true and
      `shape_quads || shape_paths` (U+2800 exempt from the emit check).
- [ ] `all_rects_within_cell_bounds`, `all_path_points_within_cell_bounds` (±0.5 px).
- [ ] `mirror_x_pairs_match`: `┌/┐ ┗/┛ ├/┤ ╭/╮ ╔/╗ ╠/╣ ▌/▐ ▏/▕ ▘/▝ ╱/╲ E0B0/E0B2 E0B4/E0B6
      E0B8/E0BA` and braille `0x01/0x08`.
- [ ] `mirror_y_pairs_match`: `┌/└ ┬/┴ ╭/╰ ╒/╘ ╓/╙ ╔/╚ ▀/▄ ▔/▁ ▘/▖ E0B8/E0BC` and braille
      `0x01/0x40`.
- [ ] `self_symmetric_glyphs`: `─ │ ┼ ━ ┃ ╋ ═ ║ ╬ █ ╳ ▒ ▬` equal their own mirror_x and
      mirror_y rect sets.
- [ ] `builder_commutes_with_transforms`: for every Family A/C arm set,
      `build(mirror(def)) == mirror_rects(build(def))`.
- [ ] `horizontal_line_abuts_across_cells`: `─` y-interval identical for the same cell size;
      `x = 0` and `x + w = W`; same for `━` and the `═` rails. The outer `┄` segments end
      within one dash gap of the edge, symmetrically (they touch it only when the snapped
      pitch happens to reach it).
- [ ] `cross_equals_union_of_lines`: pixel set of `┼` == `─ ∪ │`; `╋` == `━ ∪ ┃`; `╬` ⊂
      `═ ∪ ║` and leaves the center empty.
- [ ] `corner_arms_meet_at_joint`: `┌` pixel set == `╶ ∪ ╷` minus nothing (arms overlap at
      `J_x × J_y`).
- [ ] `double_corner_up_and_down_differ`: `╒ ≠ ╘`, `╓ ≠ ╙`, `╕ ≠ ╛`, `╖ ≠ ╜` (regression for
      wart 3), for cells of at least 5 × 5 device pixels — below that a double line has no
      room for two rails and every corner collapses onto the same pixels.
- [ ] `dash_segment_counts`: `╌ 2`, `┄ 3`, `┈ 4` (and heavy/vertical variants) distinct
      along-intervals, symmetric about the center, for `W ≥ 8`.
- [ ] `block_eighth_fractions_monotone`: heights of `▁..█` non-decreasing, `█` full, `▄` ==
      `H − (H+1)/2 .. H`; widths of `▏..▉` likewise.
- [ ] `quadrant_union_is_full_block`: `▘ ∪ ▝ ∪ ▖ ∪ ▗` covers every pixel.
- [ ] `braille_dot_slots`: each single bit yields exactly one rect in its slot ordering (columns
      left < right, rows top < bottom); U+28FF yields 8 disjoint rects.
- [ ] `powerline_triangle_apex_at_center_height`: E0B0 apex `(W, H/2)`, E0B2 apex `(0, H/2)`;
      every one of the sixteen emits a path and the eight fills are pairwise distinct.
- [ ] `shade_density_and_budget`: rect count ≤ 24 at every size; covered area within ±13
      points of 25 / 50 / 75 % (for cells ≥ 7 px wide); each shade's rect set is
      self-symmetric at every size.
- [ ] `snap_interval_is_even_about_center`: property test over 10k `(center, half, size)`
      triples for both anchors.
- [ ] `heavy_thicker_than_light`, `rails_disjoint_with_gap` for all sizes `W ≥ 5`.
- [ ] `rounded_corner_meets_neighbors`: `╭` arc endpoints lie on `J_x`/`J_y` centerlines.
- [ ] `per_cell_quad_budget`: Family A/C ≤ 8 rects, braille ≤ 8, shades ≤ 24. `╬` needs
      eight: two split rails per axis, and no two of them are contiguous.
