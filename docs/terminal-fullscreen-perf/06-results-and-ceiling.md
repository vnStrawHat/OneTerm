# 6. Results, Measurements, the 500 fps Ceiling & Next Plan

> What was implemented, what the numbers say after implementation, why the
> DOOM-fire fps counter caps at ~250, and the concrete options to go further.

---

## 6.1. What was implemented

| Change | Files | Effect |
|---|---|---|
| **Tier 1** — block cells flush the text batch instead of emitting an invisible space-only run | `layout/row.rs` | `shapes`/`runs` 5389 → ~1 per frame; ~5389 `String` clones/frame → 0 |
| **Tier 2 §3.1** — allocation-free box-drawing (`block::rects_into`, `box_drawing_rects_into`, `has_box_geometry`) + reusable probe/paint buffers | `box_drawing/{drawing.rs,block.rs,mod.rs}`, `element/paint.rs`, `layout/row.rs` | ~10.7k transient `Vec` allocs/frame → 0 |
| **Tier 3** — removed the fixed 1 ms `Output`-handler delay | `view/mod.rs` | −1 ms/frame latency; relies on `drain_coalesced_events` + GPUI frame scheduling |
| **Debug profile** — `opt-level=3` for the full hot path | `Cargo.toml` | debug renders normally instead of hanging (see §6.5) |
| **Damage-reset fix** — `snapshot_query()` for non-render reads | `core/session.rs`, `core/content.rs`, `{local,ssh}/session_terminal.rs`, `ui/.../handlers/*` | non-render snapshots no longer discard the renderer's dirty-row info |
| **Instrumentation** — `prepaint_us`/`paint_us` + `[PTY pump]` line | `layout/types.rs`, `element/{prepaint,paint}.rs`, `local/event_loop.rs` | made the real bottleneck measurable |

Correctness of the box-drawing refactor is locked by unit tests
(`rects_into_matches_vec_variant`, `has_box_geometry_matches_old_predicate`,
`rects_into_clears_previous_contents`, `upper_half_block_is_upper_half`).

---

## 6.2. Measured results

DOOM-fire fps counter (the number shown by DOOM-fire itself):

| Build | Before | After |
|---|---|---|
| Debug (`cargo run`) | 1 frame, then "Not Responding" | **~260 fps**, renders normally |
| Release | ~197 fps | **~235-260 fps** |

Per-frame render stats (release, steady state):

```
[TerminalElement] frame=300 lines=45 dirty=45 quads=13124 bg_rects=5267 shapes=1 runs=1 hashes=0 prepaint_us=1025 paint_us=15161
```

- `shapes`/`runs` = **1** → Tier 1 confirmed (the `1` is the on-screen `mem: … fps` text line).
- `prepaint_us` ≈ **1.0 ms** (layout + shaping + snapshot).
- `paint_us` ≈ **15 ms** (~12-13k `paint_quad` calls) → the render phase is now
  entirely **quad-emission bound**.

PTY pump throughput (the busy pump; a second idle session logs `0% busy`):

```
[PTY pump] 29.2 MiB/s parsed | parse=1449ms wait=553ms over 2.0s | pump 72% busy (parse-bound)
```

- **72% busy** parsing + mutating the grid, **28%** waiting for PTY data.
- ~29 MiB/s of escape stream at ~250 fps ≈ ~116-190 KB/frame for a ~45×160 grid of
  truecolor `▀` cells.

---

## 6.3. Root cause: render is decoupled from the fps counter

Two independent pipelines:

```
 PTY reader thread                         Main (UI) thread
 ─────────────────                         ────────────────
 ConPTY → read() → alacritty               Output event → notify() → GPUI paint
 Processor::advance (parse + mutate         prepaint: snapshot()  ── locks Term ~200µs
 ~5.4k cells) + OSC vte_parser              paint: ~12k paint_quad (~15 ms, lock-free)
        │                                          │
        └── try_send(Output) into a ───────────────┘
            4096-slot channel (never blocks; drops if full)
```

Key facts established by the instrumentation:

1. The **render** runs at ~62 fps (`1 ms + 15 ms`), but the **fps counter shows
   ~250** — they are different numbers. The counter is DOOM-fire's own
   write/flush rate, gated by how fast the **pump** drains the PTY.
2. The render holds the `Term` lock **only** during the ~200 µs snapshot clone.
   At 62 render-fps that is ~1% contention — the render does **not** gate the pump.
3. The event channel is **bounded at 4096** and the pump uses non-blocking
   `try_send`, so a slow/backed-up renderer never throttles the pump. (This is why
   the Tier-3 channel-backpressure idea was unnecessary.)

**Conclusion:** optimizing `paint_us` improves *display smoothness* and frees
main-thread CPU, but it does **not** move the fps counter. The counter is the pump.

---

## 6.4. The 500 fps ceiling (why it is not a tuning gap)

The pump's 72% busy time decomposes as:

1. **`alacritty_terminal::Processor::advance`** — VT parse **plus mutating ~5.4k grid
   cells every frame** (SGR state, per-cell fg/bg, damage tracking). This is the bulk.
2. **OSC double-parse** (`vte_parser` + `OscSink`) — the same bytes are parsed a
   second time to capture OSC 7/9/52/133 and to detect `CSI 2J/3J` screen clears.
3. Read syscalls + `Term` lock acquisition — minor.

Why #1 cannot be optimized here: `alacritty_terminal` is a **rev-pinned** dependency,
locked to the exact revision gpui's fork uses (see `docs/agents/dependencies.md` §1).
Changing it breaks the lock and the shared type compatibility with gpui.

The arithmetic:

- At ~250 fps, alacritty mutates ~250 × 5.4k ≈ **1.35M cells/s**, consuming most of the
  1.45 s/2 s of pump-busy time.
- 500 fps would require ~**2.7M cells/s**, i.e. roughly **2×** the grid-mutation
  throughput, on a single thread, for this exact worst-case (every cell truecolor,
  full-screen damage).
- That exceeds what alacritty's single-threaded `advance` delivers for this cell
  count, **before** adding OSC parse and ConPTY overhead.

Windows Terminal reaches ~400 fps on the same workload because it uses a fundamentally
different stack (its own DirectWrite/AtlasEngine renderer + a bespoke VT parser +
tighter ConPTY integration), not because of a tunable OneTerm parameter.

**Therefore 500 fps on the counter is an architectural limit for this
workload, not a remaining optimization.**

---

## 6.5. Why the debug fix worked

The original debug hang (file 5) was addressed by `opt-level=3` overrides — but the
first attempt only covered `gpui`/`alacritty_terminal`/`oneterm-ui`. Debug still
misbehaved because the **render hot path spans more crates**:

- `oneterm-core` — `TerminalContent::from()` clones ~5.4k cells per snapshot **under
  the `Term` FairMutex**. Unoptimized, this held the lock long enough to starve the
  pump and stall the render.
- `oneterm-local` / `oneterm-ssh` — the PTY pump loop + OSC side-parser + state locks.

Extending the debug overrides to all four crates (plus `alacritty_terminal`) made the
whole pipeline fast in debug, so it now renders like release. Trade-off: stepping
through the terminal backend in a debugger is less precise; drop the specific crate
back to `opt-level=0` temporarily if you need to.

---

## 6.6. Next plan (options, in order of recommendation)

### A. Accept the ceiling (recommended)

~250-260 fps is close to what `alacritty_terminal` + ConPTY allow for this
pathological full-screen truecolor workload. Normal TUIs (mostly static, few block
cells) were never the problem and are unaffected. The high-leverage, low-risk wins
(Tier 1/2/3 + debug + damage fix) are already banked.

### B. Reduce render `paint_us` for display smoothness (does *not* raise the counter)

The render is ~62 fps because of ~12k `paint_quad` calls. Options:

- **Two-stop-gradient half-blocks** — paint `▀`/`▄` as **one** quad with a hard 50/50
  vertical gradient (`gpui::linear_gradient`, stops at ~0.499/0.501) instead of a
  full-cell bg quad + an upper-half fg quad. Halves the quads for the dominant glyph
  (~12k → ~7k), so `paint_us` ≈ 15 ms → ~9 ms, render ~110 fps. **Risk:** the hard
  edge depends on shader behaviour with near-coincident stops; must be verified
  visually (fire must stay crisp, not blurry). Only helps `▀`/`▄`.
- **§3.3 quad instancing / glyph atlas** — the real fix for `paint_us`, but a large,
  separately-scoped change to how primitives are submitted to GPUI's scene.

Neither moves the fps counter (render is decoupled — §6.3).

### C. Raise the counter (large / risky)

- **Remove the OSC double-parse** (~+25%, to ~310 fps). **Blocked by** clear
  detection: `OscSink` needs to see `CSI 2J/3J` to reset gutter timestamps, and
  DOOM-fire's stream is entirely CSI, so the second parser cannot be cheaply/safely
  gated without decoupling clear-detection first (risk to the gutter-timestamp
  feature). Still would not reach 500.
- **Replace the terminal engine** with a custom single-pass parser+renderer (parse +
  grid-mutate + OSC + clear in one pass, replacing both alacritty's `Processor` and
  the OSC `vte_parser`). This is the only path that could genuinely approach 500 fps,
  but it breaks the alacritty rev-lock and is a major undertaking. **Full design in
  [`07-custom-engine-rfc.md`](07-custom-engine-rfc.md).**
- **A non-ConPTY local backend** — would only address the ~28% `wait` component.

### How to re-measure after any change

Run with `RUST_LOG=info` and capture both lines at steady state:

```
[TerminalElement] frame=… shapes=… paint_us=… prepaint_us=…
[PTY pump] … MiB/s parsed | parse=…ms wait=…ms | pump …% busy (…)
```

- `paint_us` ↓ → render-side change worked (smoothness).
- `[PTY pump]` % busy ↓ **and** MiB/s ↑ → counter-side change worked.
- If `[PTY pump]` stays ~72% busy, the change did not touch the actual limiter.
