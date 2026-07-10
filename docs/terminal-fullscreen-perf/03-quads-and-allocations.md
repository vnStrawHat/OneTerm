# 3. Tier 2 — Per-Cell Allocations & Quad Count

> **STATUS: §3.1 ✅ IMPLEMENTED, §3.2 ✅ (bg + block run-merge), §3.3 ⏳ OPEN.** The
> allocation-free box-drawing path (`block::rects_into`, `box_drawing_rects_into`,
> `has_box_geometry`) + reusable probe/paint buffers are done (~10.7k `Vec` allocs/frame
> → 0). **Quad-count reduction via run-merging is now done on both sides**: background
> rects (existing) *and* full-width band block glyphs (`▀▁▂▃▄▅▆▇█▔`) — consecutive
> same-glyph/same-colour cells coalesce into one stretched rect (`BoxDrawCell.num_cells`,
> gated by `block::is_full_width_band`). This cuts `paint_quad` calls for any block/bar
> content with colour runs (btop, borders, progress bars); for DOOM-fire the per-cell
> gradient limits merging, so its gain depends on adjacent-cell colour coherence
> (re-measure `quads`/`paint_us`). True GPU **quad instancing** (§3.3) is still **not**
> done — it is the only thing that would cut `paint_us` when colours genuinely vary
> per cell. Note (06 §6.3): render is decoupled from the PTY pump / delivered throughput,
> so this mainly buys render smoothness / CPU headroom.
> See [`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.6.B.
>
> After Tier 1 removes the shaping cost, the next bottleneck is ~13216
> `paint_quad` calls per frame plus thousands of transient `Vec` allocations
> in the box-drawing path.

---

## 3.1. Problem A — `box_drawing_rects` allocates a `Vec` per cell, twice

### In paint (`element/paint.rs`)

```rust
for bd in &cache.rows[i].box_draws {
    ...
    for (rx, ry, rw, rh) in box_drawing_rects(bd.c, cw_d, lh_d) { // ← new Vec per cell
        window.paint_quad(fill(Bounds::new(pos, sz), bd.color));
    }
}
```

`box_drawing_rects` (`box_drawing/drawing.rs`) returns a freshly allocated
`Vec<(i32, i32, i32, i32)>` on every call. With ~5359 block cells that is ~5359 heap
allocations per frame inside the paint hot loop.

### In layout, only to check emptiness (`layout/row.rs`)

```rust
if is_box_drawing(cell.c)
    && (is_rounded_corner(cell.c) || !box_drawing_rects(cell.c, 16, 16).is_empty())
```

Here the returned `Vec` is discarded immediately — it exists only to test
`is_empty()`. That is another ~5359 allocations per frame (full damage re-lays out every
row).

### Proposed changes

1. **Add an allocation-free probe** used by layout:

```rust
/// True if `c` has custom primitive geometry (block / box-drawing / powerline /
/// rounded / shade). Pure match, no allocation. Mirrors the arms of
/// `box_drawing_rects` + the rounded/AA path.
pub(crate) fn has_box_geometry(c: char) -> bool {
    is_rounded_corner(c)
        || matches!(c,
            '\u{2500}'..='\u{257F}'   // box drawing (minus the unsupported diagonals)
            | '\u{2580}'..='\u{259F}' // block elements
            | '\u{25AC}'              // horizontal bar
            | '\u{E0B0}'..='\u{E0BF}' // powerline
        ) && !is_unsupported_box(c)   // diagonals U+2571/2572/2573, quad-dash, etc.
}
```

Then in `row.rs`:

```rust
if is_box_drawing(cell.c) && has_box_geometry(cell.c) { ... }
```

2. **Write rects into a reusable buffer** instead of returning a `Vec`. Change the
   signature to append into a caller-owned scratch buffer:

```rust
pub(crate) fn box_drawing_rects_into(
    out: &mut Vec<(i32, i32, i32, i32)>,
    c: char, cw_d: i32, lh_d: i32,
) { out.clear(); /* push into out */ }
```

Keep a single `Vec` (or `SmallVec<[_; 8]>`) in `LayoutState`/paint scope, `clear()` it
per cell, and reuse the backing allocation across all cells and frames. This removes the
per-cell allocation while keeping the exact same geometry.

> `SmallVec<[(i32,i32,i32,i32); 8]>` is a good fit: the vast majority of glyphs emit
> ≤ 8 rects, so most cells stay entirely on the stack even without a shared buffer.

---

## 3.2. Problem B — 2 quads per block cell (overdraw)

For a `▀` cell the paint path emits:

- one **full-cell background** quad (the lower-half color, from `LayoutRect`), then
- one **upper-half** primitive quad (the foreground color, from `box_draws`).

That is ~2 quads per cell (~10700 for 5359 cells) with the upper half overdrawing the
top of the background quad. Correct, but doubles the quad count.

### Options

- **Half-quad instead of full-cell background for known half-blocks.** For `▀`/`▄` the
  background could be drawn as only the complementary half, so the two quads tile the
  cell without overlap. This does not reduce the count (still 2) but removes overdraw;
  low value on its own.
- **Treat pure-fill blocks (`█ ▀ ▄ ▌ ▐` …) as background rects.** Fold them into the
  same rect list as cell backgrounds so they flow through one code path. Colors are
  unique per fire cell, so this does **not** reduce the quad count, but it simplifies the
  structure and removes the separate `box_draws` traversal for the common blocks.

Neither materially cuts the quad count for a gradient, because the limiting factor is
**unique color per cell** (no merge possible). Real reduction needs §3.3.

---

## 3.3. Problem C — no batching / instancing for primitives

`bg_rects` already merges horizontally adjacent same-color cells (`row.rs`), but a fire
gradient makes every cell a different color, so merge yields ~1 rect/cell. Box-draw
rects are never merged at all.

The structural fix, matching how Windows Terminal AtlasEngine and the GPU-side of GPUI
handle this, is **instanced quads / a glyph atlas**:

- Cache each block glyph shape once (per cell size) and draw it as an instanced,
  color-parameterized quad, so the CPU cost per cell is a single instance write rather
  than building and submitting an individual `paint_quad` primitive.
- This is the only approach that scales to "every cell is a block, every color unique".

This is a larger change (touches how primitives are submitted to GPUI's scene) and
should be scoped separately. Tier 1 + the allocation fixes in §3.1 are expected to close
most of the gap first; measure before committing to instancing.

---

## 3.4. Expected impact

| Change | Effect |
|---|---|
| §3.1 allocation-free probe + reusable buffer | Removes ~10.7k transient `Vec` allocs/frame. Large win in debug, moderate in release. |
| §3.2 overdraw removal | Removes overdraw; quad count unchanged. |
| §3.3 instancing/atlas | The only path that reduces the ~13k quad count for gradients. Larger effort. |

Re-measure `quads` and per-phase timing (see
[`01-diagnosis.md`](01-diagnosis.md) §1.4) after §3.1 to decide whether §3.3 is worth it.
