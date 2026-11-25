# Low-Level Design: Shape geometry (box drawing, blocks, shades, braille, powerline)

Intake: IN-0018
HLD: ../high-level-design.md
Topic: shapes
Date: 2026-09-08 (curves reworked 2026-09-09; parity snapping amended 2026-09-09, see DEC-0007 item 4; stroke weight added 2026-09-09, US-0052)

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

`src/render/shapes.rs`: pure geometry that turns one code point plus a device-pixel cell size
and a font weight into quads (`DeviceRect`, each carrying a coverage `alpha`). Solid families emit opaque rects;
arcs, diagonals and powerline are rasterized here into anti-aliased coverage rects. No GPUI
types, no alacritty types, no allocation beyond the caller's output vector. Owns the symmetry
contract of DEC-0007 item 4.

## Design

### Cell space

- The cell is `W × H` whole device pixels (`CellSizeDevicePx { w: i32, h: i32 }`, both ≥ 1),
  origin at the cell's top-left, `+x` right, `+y` down.
- The **center** is `C = (W/2, H/2)` as `f32` — an integer when the size is even, a
  half-integer when odd. All definitions are offsets from `C`.
- A **device interval** is `[lo, hi)` with integer `lo < hi`; a `DeviceRect` is the product of
  an x-interval and a y-interval, clipped to `[0, W) × [0, H)`, plus a coverage `alpha` in
  `(0, 1]` shared by every pixel of the rect; empty rects are dropped.

### Primitives

```rust
enum Axis { Horizontal, Vertical }

/// A centered bar. `cross` offsets the bar's centerline from `C` perpendicular to `axis`,
/// `along` offsets the bar's midpoint from `C` along `axis`; extents are half sizes.
struct Stroke { axis: Axis, cross: f32, along: f32, half_len: f32, half_thick: f32 }

/// An axis-aligned rect given by its center and half extents (used for blocks, dots, tiles).
struct CenterRect { cx: f32, cy: f32, half_w: f32, half_h: f32 }

struct DeviceRect { x: i32, y: i32, w: i32, h: i32, alpha: f32 }    // cell-relative device pixels
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
3. Parity: a centered interval needs `t` even when `c` is an integer and `t` odd when `c` is a
   half-integer. **The thickness never changes for a stroke** (DEC-0007 item 4, amendment
   2026-09-09).
   - `Anchor::Fixed` (a stroke on a cell axis: joints `J_x`/`J_y`, full-length lines, arm
     cross-sections): on mismatch move `c` by **−0.5, toward the top/left**, for every code
     point alike. Exact centring whenever the grid allows it; the same half-pixel bias
     everywhere when it does not, so joins stay seamless and `─` is as thick as `│`.
   - `Anchor::Nearest` (an off-axis feature whose thickness may not change: rails, dots,
     dash segments, tiles): on mismatch move `c` by `±0.5` to the nearest parity-valid
     position, ties broken **away from the cell center** (so gaps between paired features
     never close). Exactly on the cell center there is no "away": `t += 1`, since moving
     would break the feature's own symmetry. This branch is reached only by *lengths* (the
     middle dash segment, the `▬` bar height), never by a thickness.
4. `lo = c - t/2` (exact integer), `hi = lo + t`. An interval that would fall outside the
   cell is **shifted in** (`lo` clamped into `[min(0, size - t), max(0, size - t)]`) and
   only then clipped to `[0, size]`: on a one or two pixel cell the rails of a double line
   and the braille dots must still draw something.

The **stroke position** `C'` of an axis is the midpoint of `J(t_l)`: `C` when `(size − t_l)`
is even, `C − 0.5` otherwise (`Geometry::axis_center`). Rails and the rounded corner are laid
out around `C'`, not `C`, so they stay concentric with the strokes they meet.

Properties (asserted by tests): the interval is symmetric about `c` (or `c − 0.5`); ties
toward the center give `round(size - x) == size - round(x)` for integer `size`, so for
`Nearest` and for a parity-matched `Fixed`, `snap_interval(size - v, ...)` is the exact mirror
of `snap_interval(v, ...)`; for a mismatched `Fixed` it is the mirror translated by exactly one
pixel toward the top/left (both intervals carry the same bias). Two strokes with the same
`(center, half)` snap identically, so a horizontal line's y-interval in cell *n* equals the one
in cell *n + 1* and the joint square of `┼` is exactly `J_x × J_y`.

Worked examples (`t_l = 1`): at **9 × 18** `C = (4.5, 9)`; `J_x = [4, 5)` (exact), `J_y`:
`c = 9` is an integer and `t = 1` odd, so `c' = 8.5` and `J_y = [8, 9)` — `─` occupies row 8
only, `│` column 4, `┼` is their union, `━` (`t_h = 3`) is rows `[7, 10)`; the double rails
sit at `C'.y ± 1 = 7.5, 9.5` → rows 7 and 9 with the light line running exactly between them.
At **8 × 16** `C = (4, 8)`, both axes mismatch: `J_x = [3, 4)`, `J_y = [7, 8)`, `┼` is column 3
and row 7, rails at columns 2 / 4 and rows 6 / 8. At **9 × 19** nothing shifts.

Consequence to accept: a stroke is exactly centered only when the cell dimension and the
stroke thickness have the same parity; otherwise the whole box-drawing family sits half a
device pixel toward the top/left of the cell centre. Reflection is then off by one pixel
(`build(mirror_y(def)) == mirror_y(build(def))` translated by 1 px toward the top, arms that
reach the cell edge still reaching it). Light and heavy strokes of different parity (e.g.
`t_l = 2`, `t_h = 5` at 14 px) can have centres 0.5 px apart on the same axis; the light
interval is always inside the heavy one, so joints remain unions. This replaces the earlier
"widen by one pixel" rule, which made `─` 2 px and `│` 1 px at 9 × 18 (owner complaint,
acceptance rework 2 of US-0046).

### Transforms

Applied to **definitions**, never to emitted rects:

| Transform | `Stroke` (axis H) | `Stroke` (axis V) | `CenterRect` | path point `(x, y)` |
| --- | --- | --- | --- | --- |
| `mirror_x` | `along = -along` | `cross = -cross` | `cx = W - cx` | `(-u, v)` |
| `mirror_y` | `cross = -cross` | `along = -along` | `cy = H - cy` | `(u, -v)` |
| `rotate_cw` | applies only to arm sets (slot permutation below); never to strokes, rects, or regions | | | |

The last column is for the rasterized families: a region is a predicate over sample points
`(u, v)` given as offsets from `C`, and a mirrored code point evaluates the canonical predicate
on the negated offset (see Family D).

Arm sets (below) are transformed by permuting arm slots: `mirror_x` swaps `left ↔ right`,
`mirror_y` swaps `up ↔ down`, `rotate_cw` maps `up → right → down → left → up` (the builder
re-derives thickness per axis, so rotation is valid on non-square cells). The code-point table
stores a canonical definition plus a transform; the test
`build(transform(def)) == transform_rects(build(def))` proves the builder is symmetric.

### Thickness rules (device px; `W` = device cell width, `weight` = font weight)

Strokes follow the font weight (US-0052, owner request 2026-09-09: bold text next to a box frame
that stayed hairline). `weight` is the CSS weight `100..900` (`gpui::FontWeight.0`); the scale
`k = clamp(weight / 400, 1, 2.25)` is applied to the **nominal** thickness before snapping, so
DEC-0007 item 4 (snapping never changes a thickness) is untouched. `k == 1.0` exactly for every
weight at or below 400, which therefore draws the weight-independent geometry bit for bit.

| Name | Value |
| --- | --- |
| scale `k` | `clamp(weight / 400, 1, 2.25)` |
| light `t_l` | `max(1, round(W / 8 · k))` |
| heavy `t_h` | `max(t_l + 2, round(W / 3 · k))` |
| double rail `t_d` | `t_l` |
| double gap `g` | `t_l` (so `2d = t_d + g` is even: the rails stay parity-exact and abut `J(t_l)` at every weight; a fixed gap would let the rails overlap the light joint at e.g. 9 px / 900) |
| rail offset `d` | `(t_d + g) / 2` (nominal, `Nearest`-snapped) |
| dash gap `g_dash` | `max(1, round(W / 8))` — the weight-400 light thickness, so a heavier dash keeps its segment lengths instead of turning into dots |

`t_l / t_h` by cell width and weight (`thickness_table_for_docs` prints it):

| `W` | 400 | 600 (`k` 1.5) | 700 (`k` 1.75) | 900 (`k` 2.25) |
| --- | --- | --- | --- | --- |
| 7 px | 1 / 3 | 1 / 4 | 2 / 4 | 2 / 5 |
| 8 px | 1 / 3 | 2 / 4 | 2 / 5 | 2 / 6 |
| 9 px | 1 / 3 | 2 / 5 | 2 / 5 | 3 / 7 |
| 14 px | 2 / 5 | 3 / 7 | 3 / 8 | 4 / 11 |
| 18 px | 2 / 6 | 3 / 9 | 4 / 11 | 5 / 14 |

`t_d = g = t_l` in every cell; `g_dash` is 1 px up to 9 px and 2 px at 14 / 18 px regardless of
the weight. The **effective weight** of a cell is chosen by row planning
(`render-pipeline.md`): the settings weight, or `min(900, settings + 300)` for a cell with
`CellFlags::BOLD` or a bold class style, so SGR bold on a normal font draws at 700 and the
strokes get heavier together with the text. Fills (blocks, triangles, half discs), shades and
braille dots have no stroke and do not change with the weight.

Nominal values pass through `snap_interval`; the painted thickness equals the nominal on both
axes (a stroke shifts, it never widens). Height uses the same table with `W` (not `H`) so
horizontal and vertical strokes are equally thick. A heavier weight can reach the parity bias
on a cell the base weight did not: at 7 px / 700 (`t_l = 2`) the rails of `║` are `[0, 2)` and
`[4, 6)`, half a pixel left of centre like every stroke of the family; the mirror tests skip
the far-edge line on such an axis (`Bitmap::without_far_edge`), the only place where "reflect
and move one pixel toward the top/left" is ambiguous.

Joint intervals: `J_x = snap_interval(C.x, t/2, W, Fixed)`, `J_y = snap_interval(C.y, t/2,
H, Fixed)` for `t ∈ {t_l, t_h}`. Double rails: `R±_x = snap_interval(C'.x ± d, t_d/2, W,
Nearest)` around the stroke position `C'`, likewise `R±_y`; since `2d = t_d + g` is even the
rails always land parity-exact, equidistant from `J(t_l)` and abutting it (`R-.hi == J.lo`,
`J.hi == R+.lo`). The **outer interval** `O_x = [R-_x.lo, R+_x.hi)`.

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
half-length `(p - g_dash)/2`, centers at `C + (k + 0.5 - n/2) * p` for `k in 0..n`; every
segment is `Nearest` (the middle one of an odd `n` sits on the cell center and widens by a
pixel rather than shifting, so the gaps stay symmetric). Along-extents are clipped so segments never
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

Coverage-rasterized (no paths). The rasterizer and its symmetry argument:

**Coverage rasterization.** A shape is a region predicate `inside(u, v)` over offsets from the
cell center. Every device pixel `(x, y)` is sampled on an `n × n` grid, `n = 4` (sixteen
levels, as the old engine's rounded corners), at `x + (i + 0.5) / n`, `y + (j + 0.5) / n`. As an
offset from `C.x = W / 2` the sample is the odd numerator `(2nx + 2i + 1 − nW) / 2n`: exactly
representable in `f32`, and reflecting the pixel to `W − 1 − x` with sample `n − 1 − i` gives
`(2n(W − 1 − x) + 2(n − 1 − i) + 1 − nW) = −(2nx + 2i + 1 − nW)`, the exact negation. A
mirrored code point evaluates the canonical predicate on `(−u, v)` (or `(u, −v)`, or both), and
every predicate is built from `abs`, squares, products and comparisons of `u`, `v` and constants,
so the coverage count of pixel `(x, y)` for `mirror_x(def)` is **bit-identical** to the count of
pixel `(W − 1 − x, y)` for `def` — no rounding tolerance anywhere. Sample offsets are odd
multiples of `1 / 2n` while every boundary in this module lies on a multiple of `1 / 2`, so no
sample ever sits on a boundary and `<=` versus `<` never matters.

Emission: `alpha = count / n²`; along each scanline, neighbouring pixels of equal count merge
into one run; a run whose `x`, `w` and `alpha` equal a rect ending on the row above extends that
rect downwards (so a straight stub is one rect); zero-coverage runs are dropped. Run-length
merging commutes with reflection because maximal runs of a mirrored coverage image are the
mirrored runs. Cost is `W · H · n²` predicate calls per glyph, only when its row is replanned;
above `64 × 128` device pixels per cell (`AA_PIXEL_CAP`) the rasterizer falls back to `n = 1`
(one centered sample, no anti-aliasing) so a row of gigantic cells cannot stall a replan.

**Strokes** are distance bands `|d| ≤ t / 2` around an ideal curve, so both edges of a stroke
are equidistant from it and the stroke is symmetric by construction; `t = t_l` from the
thickness table (the snapped joint width where a band has to meet a snapped stub, see the
rounded corner). **Fills** are inside tests against the ideal boundary.

- `╲` U+2572 (canonical): band of half width `t_l / 2` around the line through `C` with
  direction `(W, H)`: `|u · H − v · W| ≤ (t_l / 2) · √(W² + H²)`. `╱` = `mirror_x`; `╳` is the
  union of both bands (one predicate, self-symmetric).
- `╭` U+256D (canonical, arc down and right). Let `hx = |J_x| / 2`, `hy = |J_y| / 2` (half the
  snapped light joint widths, `Fixed`), and let offsets be taken from the **stroke position**
  `C'` (the rasterizer subtracts `C' − C` from every sample *before* reflecting, so all four
  orientations share the biased stub columns/rows); `r = max(0, min(W − C'.x − hx,
  H − C'.y − hy))`. The region is the union of
  - the vertical stub `v ≥ r ∧ |u| ≤ hx` (to the bottom edge),
  - the horizontal stub `u ≥ r ∧ |v| ≤ hy` (to the right edge; empty when `r = C.x − hx`),
  - the arc: with `(du, dv) = (u − r, v − r)`, `du ≤ 0 ∧ dv ≤ 0` and inside the elliptical band
    between semi-axes `(r − hx, r − hy)` (the hole; absent when either is `≤ 0`) and
    `(r + hx, r + hy)`.

  The band is `2hx` wide where it meets the vertical stub and `2hy` where it meets the
  horizontal one, so it blends into both stubs, and into `│` below and `─` to the right in the
  neighbouring cells, with neither a gap nor an overshoot; when `|J_x| ≠ |J_y|` (parity) the
  thickness interpolates along the arc, and the band's midline is the circle of radius `r`
  about `(r, r)`. `r` is the largest radius whose outer band edge still touches the cell edge,
  so the tangent ends fall on whole pixels and the joins are solid. `╮ = mirror_x(╭)`,
  `╰ = mirror_y(╭)`, `╯ = mirror_x(mirror_y(╭))`. Diagonals and the powerline strokes are
  anchored to the cell corners and edges, meet no box line, and are anti-aliased, so they
  keep the true centre `C` (a shift would clip half a pixel at one edge and leave a gap at
  the other); fills keep `C` too.

Worked example, `╭` at 9 × 19 (`t_l = 1`, `J_x = [4, 5)`, `J_y = [9, 10)`, `hx = hy = 0.5`,
`r = min(4.5 − 0.5, 9.5 − 0.5) = 4`, arc center `C + (4, 4) = (8.5, 13.5)`, band radii 3.5 … 4.5).
Coverage per pixel (`.` = 0, `#` = 1, digit = tenths), rows 9–18; rows 0–8 are empty:

```text
row  9  .....058#      rects: (5,9) 1/16  (6,9) 8/16  (7,9) 14/16  (8,9) 16/16
row 10  ....0861.             (4,10) 1/16  (5,10) 13/16  (6,10) 11/16  (7,10) 2/16
row 11  ....56...             (4,11) 8/16  (5,11) 11/16
row 12  ....81...             (4,12) 14/16  (5,12) 2/16
row 13  ....#....      one rect [4,5) × [13,19) alpha 1: the arc's lower end (row 13)
  …     ....#....      and the vertical stub merged
row 18  ....#....
```

13 rects. Pixel `(8, 9)` is solid, so `─` in the next cell (`[0, 9) × [9, 10)`) continues the
stroke; pixel `(4, 18)` is solid, so `│` below continues it. `╮` at the same size is the exact
mirror: `(0, 9)` solid, `(3, 9) 14/16`, `(2, 9) 8/16`, …. At 9 × 18 (`C'.y = 8.5`) the same
picture starts one row higher (`.....058#` on row 8, stub on column 4 from row 12), meeting
`─` on row 8 and `│` on column 4 exactly; `╮` there is the mirror moved one pixel left.

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
| `▬` U+25AC | `Stroke { H, cross 0, along 0, half_len W/2, half_thick H/4 }`, snapped `Nearest` (a bar height is a length: it widens by a pixel on parity mismatch and stays exactly centered) |

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

### Family H — powerline (U+E0B0–E0BF), coverage-rasterized

Canonical regions in offsets from `C` (`t = t_l`, `h = t / 2`); every other code point is a
reflection of one of these and inherits exact symmetry from the rasterizer.

| Code | Kind | Canonical region |
| --- | --- | --- |
| E0B0 | fill | triangle, base on the left edge, apex at `(W, C.y)`: `\|v\| · W ≤ C.y · (C.x − u)` |
| E0B1 | stroke | both legs of that triangle: distance from `(u, \|v\|)` to the segment `(−C.x, C.y) → (C.x − h, 0)` is `≤ h`; the apex is inset by `h` so the round join stays inside the cell |
| E0B2 / E0B3 | | `mirror_x` of E0B0 / E0B1 |
| E0B4 | fill | half ellipse centered on the left edge, semi-axes `(W, C.y)`: `((u + C.x) / W)² + (v / C.y)² ≤ 1` |
| E0B5 | stroke | band between the ellipses with semi-axes `(W − t, C.y − t)` and `(W, C.y)` (the ideal curve inset by `h`, so the outline stays inside the cell) |
| E0B6 / E0B7 | | `mirror_x` of E0B4 / E0B5 |
| E0B8 | fill | lower-left triangle: `v · W ≥ u · H` |
| E0B9 | stroke | the `╲` band (`Family D`) |
| E0BA / E0BB | | `mirror_x` of E0B8 / E0B9 |
| E0BC / E0BD | | `mirror_y` of E0B8 / E0B9 |
| E0BE / E0BF | | `mirror_x(mirror_y)` of E0B8 / E0B9 |

Fixes inventory wart 1: all sixteen have real geometry and all eight fills are distinct
coverage maps. The four corner outlines are only two distinct diagonals by definition
(E0B9 == E0BF, E0BB == E0BD, and E0B9 == `╲` pixel for pixel), which is what the
powerline-extra font draws as well.

## Interfaces

```rust
pub(crate) struct CellSizeDevicePx { pub w: i32, pub h: i32 }
/// Cell-relative device pixels; every pixel of the rect has coverage `alpha` in (0, 1]
/// (a multiple of 1/16; exactly 1.0 for the solid families A/B/C/E/F/G).
pub(crate) struct DeviceRect { pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub alpha: f32 }

/// True for every code point this module draws (row planning skips the font for them).
pub(crate) fn is_shape_char(c: char) -> bool;
/// Append the quads of `c` drawn at `font_weight` (CSS scale 100..900, `gpui::FontWeight.0`);
/// returns false (and appends nothing) when `c` is not a shape char (and for U+2800, which
/// draws nothing).
pub(crate) fn shape_quads(c: char, cell: CellSizeDevicePx, font_weight: f32, out: &mut Vec<DeviceRect>) -> bool;
/// Exposed for tests and metrics: light/heavy thickness for a cell width and font weight.
pub(crate) fn stroke_thickness(cell: CellSizeDevicePx, font_weight: f32) -> StrokeThickness { light, heavy, rail, gap, dash_gap }
```

The module stays GPUI-free: the weight is a plain `f32` on the CSS scale, which is exactly what
`gpui::FontWeight` wraps.

Rects are appended in a deterministic order (arms in `up, down, left, right` order, rails
`−` then `+`; rasterized shapes top to bottom, left to right), which row planning relies on only
for reproducibility, not correctness. `shape_quads` is pure and allocation-free apart from
`out`; the rasterizer keeps no buffer (runs are merged on the fly and the downward merge scans
the rects already in `out`). Row planning multiplies `alpha` into the quad color's `a`, so the
painter stays a plain `paint_quad`, and coalesces across cells only when the final colors
(hence alphas) are equal.

## Edge Cases and Failure Modes

- [ ] `W == 1` or `H == 1`: every interval clips or shifts into the cell; nothing panics;
      `┼` is one pixel, and so is every double corner (the `╬` center hole and the
      `╒`/`╘` distinction need at least 5 × 5).
- [ ] Cell dimension and stroke thickness of different parity: the stroke keeps its
      thickness and sits half a pixel toward the top/left; `┼` is still the exact union of
      `─` and `│`, and mirrored code points are one pixel off their reflection on that axis.
- [ ] `W` even and `H` odd (or `t_l` and `t_h` of different parity): `─` and `│` are still
      equally thick; only the bias differs per axis / per weight.
- [ ] Rails on a narrow cell (`W ≤ 4`): `Nearest` snapping keeps `R-` and `R+` disjoint; when
      impossible (`W < 2 t_d + 1`) the rails may touch but never cross.
- [ ] Dash gaps at tiny widths (`W < 2n`): segments degrade to 1 px each, gaps may vanish; the
      segment count test only runs for `W ≥ 8`.
- [ ] Shade tiles overhanging the cell are clipped symmetrically because `n` is odd.
- [ ] Unknown code point in a family range (none exist in U+2500–259F; U+E0B0–E0BF fully
      covered) returns `false`.
- [ ] Braille U+2800 draws nothing but is a shape char (no font fallback).
- [ ] Cells above 64 × 128 device pixels: curves and diagonals rasterize without
      anti-aliasing (one sample per pixel); still symmetric, still bounded in cost.
- [ ] `╭` on a 1 × 1 or 2 × 2 cell: `r = 0`, the arc degenerates to a blob, something is
      still drawn.

## Verification

`src/render/shapes_tests.rs`, all pure `#[test]`s, run on cell sizes `{7×15, 8×16, 8×17, 9×18,
9×19, 14×29, 18×38, 36×76, 1×1}` unless stated (`PARITY_SIZES = {9×18, 8×16, 9×19, 8×17,
14×28, 14×29}` for the parity-specific tests). The symmetry, joint, thickness and budget
suites (`mirror_x_pairs_match`, `mirror_y_pairs_match`, `self_symmetric_glyphs`,
`builder_commutes_with_transforms`, `horizontal_line_abuts_across_cells`,
`cross_equals_union_of_lines`, `corner_arms_meet_at_joint`, `heavy_thicker_than_light`,
`stroke_thickness_uniform_across_axes`, `double_rails_equidistant_from_joint`,
`rails_disjoint_with_gap`, `rounded_corner_matches_straight_stubs`, `per_cell_quad_budget`)
run at `WEIGHTS = {400, 700}`; everything else at 400:

- [ ] `every_supported_code_point_emits_geometry`: for each code point in U+2500–257F,
      U+2580–259F, U+25AC, U+2800–28FF, U+E0B0–E0BF, `is_shape_char` is true and
      `shape_quads` appends something (U+2800 exempt from the emit check).
- [ ] `all_rects_within_cell_bounds`: inside the cell, `0 < alpha ≤ 1`.
- [ ] `solid_families_unchanged_and_opaque`: rect sets of `┌ ┼ ╬ ╤ ┄ ┋ ▄ ▚ ░ ▒ ▓ ⣿ ▬ ━ ╟` at
      9 × 18, 9 × 19 and 14 × 29 equal the snapshot (taken before the curves moved to coverage
      rasterization, regenerated 2026-09-09 for the parity amendment and reviewed by eye via
      `shape_bitmaps_for_visual_review`; 9 × 19 unchanged); every rect of every non-D/H code
      point has `alpha == 1.0` at every size.
- [ ] `curved_edges_have_fractional_coverage`: `╭`, E0B4, E0B0, `╱` each emit rects with
      `0 < alpha < 1` at 9 × 19 and 14 × 29, and every alpha is a multiple of 1/16.
- [ ] `mirror_x_pairs_match`: `┌/┐ ┗/┛ ├/┤ ╭/╮ ╔/╗ ╠/╣ ▌/▐ ▏/▕ ▘/▝ ╱/╲ E0B0/E0B2 E0B4/E0B6
      E0B8/E0BA` and braille `0x01/0x08` — coverage maps equal pixel for pixel including
      alpha, and the mirrored rect set equals the other glyph's rect set, on every axis whose
      parity matches the glyph's stroke; on a mismatched axis (`(size − t) odd`, `t` the
      glyph's nominal — `t_h` for heavy glyphs, none for blocks / braille / fills / diagonals)
      the coverage map equals the reflection moved exactly one pixel toward the top/left
      (`Bitmap::shifted`: the far edge keeps its own pixels). The shift is computed from
      `(size, t)` in `assert_mirrored`, never inferred, so a shift the other way or by more
      than one pixel fails.
- [ ] `mirror_y_pairs_match`: `┌/└ ┬/┴ ╭/╰ ╒/╘ ╓/╙ ╔/╚ ▀/▄ ▔/▁ ▘/▖ E0B8/E0BC` and braille
      `0x01/0x40`, same comparison.
- [ ] `self_symmetric_glyphs`: `─ │ ┼ ━ ┃ ╋ ═ ║ ╬ █ ╳ ▒ ▬` equal their own mirror_x and
      mirror_y coverage maps (and rect sets where exact), same comparison.
- [ ] `builder_commutes_with_transforms`: for every Family A/C arm set and reflection, every
      rect of `build(mirror(def))` has exactly one counterpart in `mirror_rects(build(def))`
      whose interior boundaries on a mirrored axis are equal or moved one pixel toward the
      top/left (cell-edge boundaries stay); a shift is only allowed on an axis where `t_l`
      or `t_h` cannot be centered (mixed-weight glyphs shift per rect).
- [ ] `stroke_thickness_uniform_across_axes` (PARITY_SIZES): `─` and `│` are one rect each,
      `t_l` thick; `━` and `┃` are `t_h` thick; each is centered on `C` or on `C − 0.5`,
      never `C + 0.5`.
- [ ] `double_rails_equidistant_from_joint` (all sizes ≥ 5 × 5): on both axes
      `R-.hi ≤ J.lo`, `J.hi ≤ R+.lo`, `J.lo − R-.hi == R+.lo − J.hi`, equal rail lengths;
      `╪ == ═ ∪ │` and `╫ == ║ ∪ ─` pixel for pixel.
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
      along-intervals, and the rect set is its own exact mirror along the dash axis at every
      size (lengths never shift), for `W ≥ 8`.
- [ ] `block_eighth_fractions_monotone`: heights of `▁..█` non-decreasing, `█` full, `▄` ==
      `H − (H+1)/2 .. H`; widths of `▏..▉` likewise.
- [ ] `quadrant_union_is_full_block`: `▘ ∪ ▝ ∪ ▖ ∪ ▗` covers every pixel.
- [ ] `braille_dot_slots`: each single bit yields exactly one rect in its slot ordering (columns
      left < right, rows top < bottom); U+28FF yields 8 disjoint rects.
- [ ] `powerline_triangle_apex_at_center_height`: the base column of E0B0 / E0B2 is covered on
      every row and solid on the center row; the apex column is symmetric about `C.y` and
      peaks there; every one of the sixteen emits quads, the eight fills are pairwise distinct
      coverage maps, and E0B9 equals `╲`.
- [ ] `shade_density_and_budget`: rect count ≤ 24 at every size; covered area within ±13
      points of 25 / 50 / 75 % (for cells ≥ 7 px wide); each shade's rect set is
      self-symmetric at every size.
- [ ] `snap_interval_is_even_about_center`: property test over 10k `(center, half, size)`
      triples for both anchors: exact mirror for `Nearest` and parity-matched `Fixed`; for a
      mismatched `Fixed` the mirror translated by one pixel toward the top/left (or exact,
      where the cell edge clamps), with the nominal thickness and the midpoint at exactly
      `c − 0.5` when the interval is interior.
- [ ] `heavy_thicker_than_light`, `rails_disjoint_with_gap` for all sizes `W ≥ 5`.
- [ ] `rounded_corner_matches_straight_stubs`: for `╭` at every size, rows below `C'.y + r` are
      exactly `J_x` at alpha 1 and nothing else, columns right of `C'.x + r` are exactly `J_y`
      at alpha 1 (no overshoot); the join pixels on the bottom and right edges are solid (cells
      ≥ 3 × 3); every row from the arc's top and every column from its left edge has coverage
      (no gap); some coverage lies off both stub axes (it is an arc, not a mitre).
- [ ] `per_cell_quad_budget`: Family A/C ≤ 8 rects, braille ≤ 8, shades ≤ 24 (`╬` needs
      eight: two split rails per axis, and no two of them are contiguous); rasterized shapes
      ≤ 5 H (`╳`, two strokes, ≤ 6 H).
- [ ] `coverage_quad_counts`: measured counts recorded as upper bounds — `╭` 13 / 22, E0B4
      39 / 61, E0B0 42 / 66, `╱` 41 / 77 rects at 9 × 19 / 14 × 29. The arc glyph is well under
      2 H; a filled or diagonal shape needs a solid run plus one or two edge pixels per
      scanline and lands between 2 H and 2.7 H.
- [ ] `weight_400_matches_baseline_geometry`: for every code point and size, weights 100, 300
      and 399 emit exactly the weight-400 rect set, and the weight-400 table is the width-only
      rule (`(t_l, t_h)` = 1/3, 1/3, 1/3, 2/5, 2/6 at 7, 8, 9, 14, 18 px).
- [ ] `heavier_weight_thickens_strokes` (9 × 19, 14 × 29): at 700 vs 400 `─ │ ━ ┃` are
      thicker, the `═` rails are thicker and still disjoint, the `╭` band covers more pixels
      and its stub is wider, the `╲` / E0B1 / E0B5 bands cover more; `█ ▚ ▒ ⣿` E0B0 E0B4 are
      identical; `dash_gap` equals the 400 light thickness, `rail == gap == light`; weights
      above 900 clamp to the 900 scale.
