# 1. Diagnosis & Measurement

> How to read the per-frame stats, what the numbers mean for the DOOM-fire
> workload, and how to re-measure after each optimization.
>
> **UPDATE:** the render stats line now also logs `prepaint_us` and `paint_us`
> (phase timing), and a separate `[PTY pump]` line reports pump throughput
> (MiB/s parsed, parse vs wait time, % busy). Together they distinguish
> *render-bound* from *pump-bound*. Post-implementation readings and the
> re-measurement recipe are in
> [`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.2 and §6.6.

---

## 1.1. The built-in per-frame counters

`element/paint.rs` maintains a `RowLayoutCache::stats` struct and logs it every 60
frames:

```rust
if cache.stats.frame_count % 60 == 0 {
    log::debug!(
        "[TerminalElement] frame={} lines={} dirty={} quads={} bg_rects={} shapes={} runs={} hashes={}",
        ...
    );
}
```

| Counter | Where it is incremented | What it measures |
|---|---|---|
| `total_lines` (`lines`) | `layout/cache.rs` | Number of display rows in the viewport. |
| `dirty_lines` (`dirty`) | `layout/cache.rs` | Rows re-laid-out this frame. |
| `paint_quad_calls` (`quads`) | `element/paint.rs` | Total `window.paint_quad` calls. |
| `bg_rect_count` (`bg_rects`) | `element/paint.rs` | Background rects painted. |
| `shape_line_calls` (`shapes`) | `element/prepaint.rs` | `text_system().shape_line` calls. |
| `text_run_paints` (`runs`) | `element/paint.rs` | `ShapedLine::paint` calls. |
| `hash_calls` (`hashes`) | `layout/cache.rs` | Per-line hash computations. |

To see them, run the debug binary (it keeps the console) with `RUST_LOG=debug` (or at
least `oneterm_ui=debug`).

---

## 1.2. Reference frame breakdown

```
frame=180 lines=45 dirty=45 quads=13216 bg_rects=5359 shapes=5389 runs=5389 hashes=0
```

### `dirty == lines` → full damage every frame

The fire changes every cell every frame, so `damage = TermDamageInfo::Full` and every
row is invalidated. The row-layout cache and the per-line hash short-circuit
(`layout/cache.rs`) provide **zero benefit** here — they are designed for the common
case where only a few lines change.

### `bg_rects ≈ cell count` → background merge is defeated

`layout/row.rs` merges horizontally adjacent cells that share a background color into a
single `LayoutRect` (via `num_cells`). With a fire gradient, adjacent cells almost
never share a color, so merging collapses to ~1 rect per cell. 5359 rects ⇒ the grid is
roughly `45 rows × ~119 cols`.

### `shapes == runs ≈ cell count` → the dominant cost

`shape_line` is called once per text run, and there is ~1 run per cell. Text shaping is
substantially more expensive than a fill quad, so **~5389 shape calls per frame is the
biggest single contributor to frame time**. The root cause (space-only runs emitted for
block cells) is analyzed in [`02-shape-line-batching.md`](02-shape-line-batching.md).

### `quads = 13216`

Approximate composition:

```
1     base background quad
5359  cell background rects
~7855 box-drawing / block primitive rects  (13216 − 5359 − 1 base − 1 cursor)
1     cursor
```

The ~7855 box-draw quads come from ~5359 block cells (some block glyphs emit more than
one rect). Details and the allocation cost in
[`03-quads-and-allocations.md`](03-quads-and-allocations.md).

---

## 1.3. How to re-measure after a change

1. Build and run the debug (or release) binary with logging enabled.
2. Run DOOM-fire-zig inside the terminal until it reaches steady state.
3. Capture a `[TerminalElement] frame=…` line.
4. Compare `shapes`, `quads`, and `bg_rects` against the baseline above.

Expected targets after the Tier 1/2 changes:

| Metric | Baseline | Target after Tier 1 | Target after Tier 1+2 |
|---|---|---|---|
| `shapes` | 5389 | ~0 (pure-block lines) | ~0 |
| `runs` | 5389 | ~0 | ~0 |
| `quads` | 13216 | 13216 (unchanged) | reduced via merge/instancing |
| `bg_rects` | 5359 | 5359 | 5359 (color-limited) |

`shapes`/`runs` dropping to near zero is the primary signal that Tier 1 worked.

---

## 1.4. Optional: add phase timing

The current counters are call counts, not wall-clock time. To attribute time between
`prepaint` (layout + shaping) and `paint` (quad emission), add two `std::time::Instant`
measurements around the `update_row_cache` + shaping block in `prepaint.rs` and around
the quad loops in `paint.rs`, and log them alongside the counters. This confirms whether
shaping or quad emission dominates on a given machine before investing in Tier 2.
