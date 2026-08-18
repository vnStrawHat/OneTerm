# 5. Why Debug Hangs but Release Runs Steadily

> **STATUS: ✅ FIXED.** Remedy A (per-package `opt-level=3` in debug) was applied
> **and extended** to the full hot path — `oneterm-ui`, `oneterm-core`,
> `oneterm-local`, `oneterm-ssh`, `alacritty_terminal`. The snapshot clone
> (`oneterm-core`) and PTY pump (`oneterm-local`) had been overlooked and were still
> at `opt-level=0`. Remedy B (Tier 1/2) also landed. Debug now renders continuously.
> See [`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.5.
>
> **Update (2026-08, BUILD-09):** the first-party `opt-level = 3` overrides now live in
> the named `[profile.fast-dev]` (`cargo run -p oneterm-app --profile fast-dev`) rather
> than in `dev`, so plain `dev`/`test` builds keep OneTerm's crates at `opt-level = 0`
> (precise debugging, fast incremental rebuilds). Third-party hot crates (`gpui`,
> `gpui_platform`, `smol`, `alacritty_terminal`) stay optimized in both profiles. Use
> `fast-dev` for DOOM-fire-class workloads in a debug build.
>
> The debug build renders one frame and then goes "Not Responding"; release
> renders steadily. This is **not a logic bug** (a deadlock would hang release
> too) — it is a performance threshold being crossed.

---

## 5.1. Debug builds the hot path at `opt-level = 0`

From the workspace `Cargo.toml`:

```toml
[profile.dev.package]
gpui = { opt-level = 3 }
gpui_platform = { opt-level = 3 }
smol = { opt-level = 3 }
```

Only `gpui`, `gpui_platform`, and `smol` are optimized in debug. Everything else stays
at `opt-level = 0`, including:

- **`oneterm-ui`** — all of `layout_row`, `box_drawing_rects` (a match with hundreds of
  arms), the ~13216-quad paint loop, building ~5389 runs, and thousands of `Vec` /
  `String` allocations per frame.
- **`alacritty_terminal`** — `Term::advance()` parsing the fire's large escape stream on
  the pump thread.
- **Overflow checks are on.** `[profile.dev]` does not set `overflow-checks = false`, so
  it defaults to `true`. Every integer device-pixel computation in `box_drawing_rects` /
  snapping / paint (run ~13k times per frame) carries an overflow check.

`shape_line` itself lives in `gpui` (optimized), so shaping is not the worst-hit
primitive — but the ~5389 calls per frame plus the unoptimized glue and allocations
around them are.

Net effect: the heaviest per-frame work runs roughly **20-100× slower** than release. A
frame that costs ~6 ms in release costs hundreds of milliseconds to seconds in debug.

---

## 5.2. Why that turns into a permanent hang

Rendering runs on the **main / UI thread**. On Windows, a window is flagged
"Not Responding" when its thread does not pump the OS message queue for ~5 seconds.

DOOM-fire is a **continuous, high-rate producer** — it emits output every frame without
pausing. The sequence:

1. The **first frame** paints (before the fire's stream floods in) — this is the "one
   frame" you see.
2. The fire starts streaming continuously. Each `SessionEvent::Output` schedules a
   `notify()` → a render.
3. In debug, one render takes hundreds of ms to seconds. By the time it finishes,
   `drain_coalesced_events` has already coalesced a fresh batch of `Output` events into
   another pending render → the thread **immediately renders again**.
4. Because render time (debug) ≫ the fire's frame interval, the main thread is
   **permanently behind**: it finishes one render only to find the next already queued.
   It never returns to the OS message pump long enough → Windows marks the window
   "Not Responding".

There is no backpressure on the event channel (see
[`04-notify-and-backpressure.md`](04-notify-and-backpressure.md)), so the consumer can
never converge — the backlog only grows.

---

## 5.3. Why release is fine

With `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, and
`overflow-checks = false`, each render is ~6 ms. The main thread finishes
quickly and returns to the message pump every frame → the window stays responsive.
Consumption (~6 ms/frame) keeps pace with production (coalescing merges multiple
`Output` events into one render). The workload is identical; only the per-frame CPU
cost differs by the debug/release factor.

---

## 5.4. Two independent remedies

### A. Make debug fast enough to test (ergonomics)

Add per-package optimization for the two hot crates so DOOM-fire is usable in debug
without sacrificing debuggability of the app-logic crates:

```toml
[profile.dev.package]
gpui = { opt-level = 3 }
gpui_platform = { opt-level = 3 }
smol = { opt-level = 3 }
# Hot paths for full-screen rendering — keep usable under debug.
oneterm-ui = { opt-level = 3 }
alacritty_terminal = { opt-level = 3 }
```

Optionally also disable overflow checks for these packages, or accept the default. This
does not change release behavior and keeps `oneterm-core` / `oneterm-local` /
`oneterm-ssh` at `opt-level = 0` for debugging.

> Trade-off: `opt-level = 3` on `oneterm-ui` makes stepping through UI code in a
> debugger less precise. Apply only if debug-time DOOM-fire testing is needed; the
> Tier 1/2 algorithmic fixes are the real solution.

### B. Reduce the per-frame cost (the real fix)

The Tier 1/2 changes ([`02-shape-line-batching.md`](02-shape-line-batching.md),
[`03-quads-and-allocations.md`](03-quads-and-allocations.md)) help **debug
disproportionately**, because they remove exactly the unoptimized work:

- ~5389 `shape_line` calls + ~5389 `String` clones per frame → ~0.
- ~10.7k transient `Vec` allocations per frame → ~0.

Once per-frame cost drops below the point where the main thread can return to the
message pump each frame, the debug hang disappears and release renders the fire
comfortably.

---

## 5.5. Summary

| | Debug (before fixes) | Release |
|---|---|---|
| Hot crates opt-level | 0 (except gpui/smol) | 3 |
| Overflow checks | on | off |
| Per-frame time | hundreds of ms – seconds | ~6 ms |
| Main thread returns to message pump | effectively never under load | every frame |
| Result | one frame, then "Not Responding" | steady, responsive |

The hang is a symptom of frame time crossing the message-pump-starvation threshold, not
a concurrency bug. Fix per-frame cost (Tier 1/2) and/or optimize the hot crates in debug
(§5.4.A).
