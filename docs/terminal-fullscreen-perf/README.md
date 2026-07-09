# Full-Screen Animation Rendering Performance — OneTerm

> Investigation **and implementation record** for rendering high-frame-rate,
> full-screen TUI animations (benchmarked with **DOOM-fire-zig**) in OneTerm's
> `TerminalElement`.
>
> **Original baseline**: OneTerm ~160-170 fps vs Windows Terminal ~400 fps on the
> same DOOM-fire-zig workload. The **debug** build rendered one frame and then hung
> ("Not Responding").
>
> **Current status (after the work in this folder was implemented)**:
> - **Debug** build now renders the fire smoothly (~260 fps counter) instead of hanging.
> - **Release** build ~235-260 fps counter (up from ~197).
> - The `shape_line` bottleneck is gone (`shapes`/`runs` 5389 → 46 per frame).
> - The remaining ceiling is now understood and **measured** (see
>   [`06-results-and-ceiling.md`](06-results-and-ceiling.md)): DOOM-fire's fps counter
>   is **pump-bound** by `alacritty_terminal`'s per-frame grid mutation (a pinned
>   dependency), not by the renderer.
>
> **Primary implementation files**:
> - `crates/ui/src/views/terminal/layout/row.rs`
> - `crates/ui/src/views/terminal/box_drawing/{drawing.rs,block.rs,mod.rs}`
> - `crates/ui/src/views/terminal/element/{paint.rs,prepaint.rs}`
> - `crates/ui/src/views/terminal/layout/types.rs`
> - `crates/ui/src/views/terminal/view/mod.rs`
> - `crates/core/src/terminal/{content.rs,session.rs}`
> - `crates/local/src/{session_terminal.rs,event_loop.rs}` + `crates/ssh/src/session_terminal.rs`
> - workspace `Cargo.toml` (`[profile.dev.package]`)

---

## TL;DR — what changed and what we learned

| Item | Before | After | Notes |
|---|---|---|---|
| Debug build under DOOM-fire | 1 frame → "Not Responding" | renders ~260 fps | fixed by extending the debug `opt-level=3` overrides to the *whole* hot path |
| `shapes` / `runs` per frame | ~5389 | **46** | Tier 1: no space-only runs for block cells |
| Per-frame `Vec` allocs (box-draw) | ~10.7k | ~0 | Tier 2: allocation-free block path + reusable buffers |
| `Output` handler delay | fixed 1 ms/batch | removed | Tier 3 |
| Release fps counter | ~197 | ~235-260 | Tier 1 + Tier 3 |
| **Render phase cost** | shaping-dominated | `paint_us`≈15 ms, `prepaint_us`≈1.1 ms | render is now **quad-emission bound** |
| **fps-counter bottleneck** | unknown | **pump parse/grid-mutation, 72% busy @ 29 MiB/s** | measured; alacritty grid mutation dominates |

**The 500 fps target is not reachable by tuning OneTerm** — it is bounded by the
pinned `alacritty_terminal` grid-mutation cost plus ConPTY. Full analysis + the
options to go further are in [`06-results-and-ceiling.md`](06-results-and-ceiling.md).

---

## Why DOOM-fire is the worst case

DOOM-fire-zig paints fire using the upper-half block glyph `▀` (U+2580): the upper
half shows the foreground color, the lower half shows the background color, so each
cell encodes two vertical "pixels". This means:

1. **Almost every cell on screen is a block element**, not regular text.
2. **Every cell changes color every frame** (the fire is a gradient that shifts
   continuously) → full-screen damage on every frame.
3. Colors are **truecolor and unique per cell** → run/rect batching cannot merge
   adjacent cells.

This directly violates the assumption stated in
[`../terminal-rendering-optimization.md`](../terminal-rendering-optimization.md) §14:
*"the number of box-drawing/block cells on screen is usually tiny compared to regular
text."* DOOM-fire inverts that assumption, so all cost concentrates on the per-cell
primitive-render and text-shaping paths.

---

## Evidence (from the built-in per-frame stats)

`element/paint.rs` logs cache stats every 60 frames.

**Original (before optimization)** — shaping dominated:

```
[TerminalElement] frame=180 lines=45 dirty=45 quads=13216 bg_rects=5359 shapes=5389 runs=5389 hashes=0
```

**After Tier 1/2/3** — shaping is gone; quad emission dominates:

```
[TerminalElement] frame=300 lines=45 dirty=45 quads=13124 bg_rects=5267 shapes=1 runs=1 hashes=0 prepaint_us=1025 paint_us=15161
[PTY pump] 29.2 MiB/s parsed | parse=1449ms wait=553ms over 2.0s | pump 72% busy (parse-bound)
```

Interpretation:

| Metric | Value | Meaning |
|---|---|---|
| `shapes` / `runs` | **1** | Tier 1 succeeded — no per-cell `shape_line`. |
| `prepaint_us` | ~1000 | Layout + shaping + snapshot is cheap now. |
| `paint_us` | ~15000 | **~12k `paint_quad` calls ≈ 15 ms** — the render bottleneck. |
| pump busy | **72%** | The **fps counter** is limited by the PTY pump's parse/grid-mutation, not the renderer. |

The render (`~1 ms + 15 ms` ≈ 62 fps) is **decoupled** from the fps counter
(~250 fps): the pump runs on a separate thread and they share the `Term` lock only
for the ~200 µs snapshot (~1% contention). See
[`06-results-and-ceiling.md`](06-results-and-ceiling.md).

---

## Document index

| # | Topic | File | Status |
|---|---|---|---|
| 1 | Diagnosis & measurement (log breakdown, how to re-measure) | [`01-diagnosis.md`](01-diagnosis.md) | reference |
| 2 | **Tier 1** — eliminate per-cell `shape_line` (space runs for block cells) | [`02-shape-line-batching.md`](02-shape-line-batching.md) | ✅ implemented |
| 3 | **Tier 2** — kill per-cell allocations + reduce quad count | [`03-quads-and-allocations.md`](03-quads-and-allocations.md) | ✅ §3.1 done; §3.3 open |
| 4 | **Tier 3** — notify cadence & channel backpressure | [`04-notify-and-backpressure.md`](04-notify-and-backpressure.md) | ✅ timer removed |
| 5 | Why the debug build hangs but release runs steadily | [`05-debug-vs-release.md`](05-debug-vs-release.md) | ✅ fixed (see below) |
| 6 | **Results, measurements, the 500 fps ceiling & next plan** | [`06-results-and-ceiling.md`](06-results-and-ceiling.md) | current |
| 7 | **RFC — custom single-pass engine** (the only path toward 500 fps) | [`07-custom-engine-rfc.md`](07-custom-engine-rfc.md) | proposal |

---

## Priority summary (status)

1. ✅ **Tier 1 — removed the space-only text runs for block cells.** `shape_line`
   dropped from ~5389/frame to ~1. Highest leverage, low risk. **Done.**
   (`layout/row.rs`)
2. ✅ **Tier 2 §3.1 — removed per-cell `Vec` allocations** in the box-drawing path
   (allocation-free `block::rects_into` / `box_drawing_rects_into` + reusable probe
   and paint scratch buffers). **Done.** ⏳ **§3.3 (quad instancing) — not done**;
   it is the only thing that would cut the ~12k-quad `paint_us`.
   (`box_drawing/{drawing.rs,block.rs}`, `element/paint.rs`, `layout/row.rs`)
3. ✅ **Tier 3 — dropped the fixed 1 ms delay** in the `Output` handler. **Done.**
   Channel backpressure was **not** needed (the pump `try_send`s into a 4096-slot
   channel and never blocks; the render is not the fps limiter). (`view/mod.rs`)
4. ✅ **Debug ergonomics — `[profile.dev.package]` `opt-level=3`** now covers the
   **whole** hot path (`oneterm-ui`, `oneterm-core`, `oneterm-local`, `oneterm-ssh`,
   `alacritty_terminal`), not just the UI crate. This is what actually fixed the
   debug hang: the ~5.4k-cell snapshot clone (`oneterm-core`) and the PTY pump
   (`oneterm-local`) were still unoptimized. (`Cargo.toml`)

Extra work done during the investigation (not in the original plan):

5. ✅ **Latent damage-reset bug fixed.** `snapshot()` consumes/resets `Term` damage;
   several non-render callers (`cursor_bounds`/IME, mouse, URL, keyboard, mode
   checks) were also calling it and silently discarding the renderer's dirty-row
   info. Added `snapshot_query()` (damage-free, `TerminalContent::from_query`) and
   routed all non-render reads through it. (`core/session.rs`, `core/content.rs`,
   `local/ssh session_terminal.rs`, `ui/.../handlers/*`)
6. ✅ **Instrumentation.** Added `prepaint_us` / `paint_us` to the render stats and a
   `[PTY pump]` throughput line (MiB/s parsed, parse vs wait time, % busy) so the
   bottleneck is measurable. (`layout/types.rs`, `element/{prepaint,paint}.rs`,
   `local/event_loop.rs`)

---

## Quality gate

All changes pass the required gate:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace   # + --release
cargo test --workspace    # incl. new box_drawing equivalence tests
```

The box-drawing refactor is covered by `rects_into_matches_vec_variant`
(byte-for-byte equivalence to the old geometry across sizes/glyphs) and
`has_box_geometry_matches_old_predicate` (the layout guard is unchanged), so no glyph
flips between primitive and font rendering.

---

## The ceiling, in one paragraph

After Tier 1/2/3, DOOM-fire's fps counter (~250) is gated by the PTY pump spending
**72% of its time** parsing + mutating the alacritty grid (29 MiB/s, ~5.4k cell writes
per frame). That grid mutation lives in `alacritty_terminal`, a **rev-pinned**
dependency (locked to gpui's fork — see `docs/agents/dependencies.md`), so it cannot
be optimized here. At 250 fps alacritty already mutates ~1.35M cells/s; 500 fps needs
~2.7M cells/s, which exceeds its single-threaded rate for this cell count. **500 fps is
therefore an architectural limit, not a tuning gap.** The full breakdown, the render
vs counter decoupling, and the concrete paths that *could* raise it are in
[`06-results-and-ceiling.md`](06-results-and-ceiling.md).
