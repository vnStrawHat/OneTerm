# Full-Screen Animation Rendering Performance — OneTerm

> Investigation **and implementation record** for **optimizing** heavy full-screen TUI
> animation rendering (benchmarked with **DOOM-fire-zig**) in OneTerm's `TerminalElement`.
>
> **This is an optimization / tuning record.** The goal is to cut the per-frame cost of
> the render and PTY-parse hot paths — CPU time per frame (`paint_us` / `prepaint_us`),
> transient allocations, text-shaping (`shape_line`) calls, quad/rect counts, and parse
> throughput (MiB/s) — for the pathological *full-screen, every-cell-changes, truecolor*
> workload. It is **not** a chase for any single headline throughput number: the
> end-to-end frame-delivery rate is bounded by the **producer (DOOM-fire) + Windows
> ConPTY transport** (outside OneTerm — see
> [`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.7) and scales with window
> size (cell count) anyway. What OneTerm controls — and what these tunings move — is
> per-frame CPU, allocations, lock hold time, and parse headroom.
>
> **Original baseline**: OneTerm rendered this DOOM-fire-zig workload far more slowly than
> Windows Terminal on the same machine, and the **debug** build rendered a single frame and
> then hung ("Not Responding").
>
> **Current status (after the work in this folder was implemented)**:
> - **Debug** build now renders the fire continuously instead of hanging.
> - **Release** render cost dropped sharply: text-shaping is gone (`shapes`/`runs`
>   5389 → ~1 per frame), transient allocations are gone (~10.7k `Vec`/frame → 0), and
>   the render phase is now purely quad-emission bound (`paint_us` ≈ 15 ms,
>   `prepaint_us` ≈ 1 ms).
> - The PTY pump moved from **parse-bound (72% busy)** to **wait-bound (~44% busy)**;
>   parse capacity rose ~+70% (R1 single-pass OSC; see
>   [`09-patch-alacritty-fork.md`](09-patch-alacritty-fork.md)).
> - The remaining end-to-end limit is **measured** (see
>   [`06-results-and-ceiling.md`](06-results-and-ceiling.md)): it is the producer +
>   ConPTY transport, not OneTerm's parse or render — so no OneTerm-side change lifts it.
>   What OneTerm optimization *does* move is per-frame CPU, allocations, and parse headroom.
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
| Debug build under DOOM-fire | 1 frame → "Not Responding" | renders continuously | fixed by extending the debug `opt-level=3` overrides to the *whole* hot path |
| `shapes` / `runs` per frame | ~5389 | **~1** | Tier 1: no space-only runs for block cells |
| Per-frame `Vec` allocs (box-draw) | ~10.7k | ~0 | Tier 2: allocation-free block path + reusable buffers |
| `Output` handler delay | fixed 1 ms/batch | removed | Tier 3 |
| **Render phase cost** | shaping-dominated | `paint_us` ≈ 15 ms, `prepaint_us` ≈ 1 ms | render is now **quad-emission bound** |
| **PTY pump** | 72% busy, **parse-bound** | ~44% busy, **wait-bound** | R1 single-pass OSC; parse capacity ~40 → ~69 MiB/s (~+70%) |
| **End-to-end delivery limiter** | unknown | **producer (DOOM-fire) + ConPTY transport** | measured 4 ways (§6.7); *not* OneTerm parse/render |

**There is no single throughput target to hit here.** The end-to-end delivery rate is set
by the **producer + ConPTY** (outside OneTerm, proven in
[`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.7) and it scales with window
size (more cells ⇒ more bytes/frame). So this folder is scoped as **optimization** —
reducing per-frame CPU, allocations, parse time, and quad/lock overhead — which improves
smoothness and headroom at any window size. Full analysis in
[`06-results-and-ceiling.md`](06-results-and-ceiling.md).

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
| `shapes` / `runs` | **1** | Tier 1 succeeded — no per-cell `shape_line` (the `1` is the on-screen HUD text line). |
| `prepaint_us` | ~1000 | Layout + shaping + snapshot is cheap now. |
| `paint_us` | ~15000 | **~12k `paint_quad` calls ≈ 15 ms** — the render bottleneck. |
| pump busy | **72%** | Then parse-bound. **Later disproven as the delivery limiter** — after R1 the pump went wait-bound (~44%) yet end-to-end throughput was unchanged. The limiter is the producer + ConPTY (§6.7), not parse/render. |

Each render costs ~16 ms (`prepaint_us` + `paint_us`) and runs on the UI thread,
**decoupled** from the PTY pump (which runs on a separate thread). The two share the
`Term` lock only for the ~200 µs snapshot clone (~1% contention). See
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
| 6 | **Results, measurements, the practical ceiling & next plan** | [`06-results-and-ceiling.md`](06-results-and-ceiling.md) | current |
| 7 | **RFC — custom single-pass engine** (kept as an architecture reference; its premise — that a faster engine raises the delivery rate — is superseded, see §6.7) | [`07-custom-engine-rfc.md`](07-custom-engine-rfc.md) | superseded / reference |
| 8 | **Evaluation — `libghostty-vt`** as the engine (vs custom vs alacritty) | [`08-libghostty-vt-evaluation.md`](08-libghostty-vt-evaluation.md) | evaluation |
| 9 | **Design — patch the `alacritty_terminal` fork in place** (low-risk middle path) | [`09-patch-alacritty-fork.md`](09-patch-alacritty-fork.md) | R1 shipped · R2/R3 proposed |

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
   channel and never blocks; the render is not the delivery-rate limiter). (`view/mod.rs`)
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

After Tier 1/2/3 + R1, OneTerm's parse and render optimizations do **not** raise the
end-to-end rate at which the fire's frames are delivered. The PTY pump became
**wait-bound** on ConPTY (it spends >50% of the time blocked in `read()` waiting for
bytes), and a **bare-ConPTY probe with no OneTerm at all** — no `Term`, no parse, no
render — sees the same **~30 MiB/s / ~126 KiB per frame** the fire produces. So the
delivery limit is the **producer (DOOM-fire) + ConPTY transport**, outside OneTerm; it
also scales with window size (more cells ⇒ more bytes/frame). What the tunings *did* bank
is real and durable: `shape_line` 5389 → ~1/frame, ~10.7k `Vec` allocs/frame → 0, parse
capacity ~+70% (R1), and the debug hang fixed. The point of this folder is exactly that —
**optimize per-frame CPU / throughput / smoothness, which OneTerm controls.** The full
breakdown, the render-vs-pump decoupling, and the remaining (engine/transport) levers are
in [`06-results-and-ceiling.md`](06-results-and-ceiling.md).
