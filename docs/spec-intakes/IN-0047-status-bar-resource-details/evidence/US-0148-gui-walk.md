# US-0148 GUI walk — CPU/MEM hover table

> **2026-09-28 acceptance rework**: the owner rejected the first cut below ("the
> information inside the tooltip does not update with the interval"). The section
> [Acceptance rework: the tooltip stays live while open](#acceptance-rework-the-tooltip-stays-live-while-open)
> at the end of this file supersedes the "freezes at the sample taken when the pointer
> entered" note in the first walk — the table now tracks the item's own 2 s cadence for as
> long as the tooltip stays open. The first walk is kept for its OS-counter comparison,
> which is still valid.

Windows 11 Enterprise 10.0.26200, `fast-dev` profile build of this branch
(`target/fast-dev/oneterm.exe`), private `USERPROFILE`/`HOME` scratch directory, window
forced to 1280x800 at screen (40,40), one `Command Prompt` local-shell tab (the default),
`zed-one-dark` theme (the app's default). Driven with `PrintWindow(hwnd, dc, 2)` (renders
DirectX-composited content) and `SetCursorPos`, targeting only the pid this walk started
(9860); nothing else was enumerated, signalled, or closed.

## Steps

1. Launched `oneterm.exe` with `USERPROFILE`/`HOME` pointed at a scratch directory; pid
   9860.
2. Moved and sized its window with `SetWindowPos` to (40,40)-(1320,840) (1280x800 client,
   borderless).
3. Baseline: moved the cursor away from the window (50,900) and captured
   [`US-0148-no-hover-dark.png`](US-0148-no-hover-dark.png) — no tooltip, the CPU/MEM item
   shows only its own text (`CPU 0.0%  MEM 53.2 MB`) at the bottom right.
4. Moved the cursor onto the CPU/MEM item (client (1170,778), i.e. screen (1210,818)),
   waited 500 ms for the kit tooltip's enter animation (150 ms), and captured
   [`US-0148-tooltip-dark.png`](US-0148-tooltip-dark.png).
5. Immediately after the screenshot, read `GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS_EX2`,
   same struct the widget itself reads) and `Get-Process -Id 9860` for the same pid.

## Tooltip contents (step 4), in the order the item shows them

```
Memory
Private working set            53.3 MB
Working set                   167.7 MB
Commit (private bytes)        112.9 MB
Peak working set              171.6 MB
CPU
Usage              0.3% of 20 logical cores
CPU time (user + kernel)         7.8 s
Threads                             14
Uptime                          2m 53s
```

## Comparison with the OS counters for the same pid

Read a few seconds after the tooltip's underlying 2 s sample (the tooltip freezes at the
sample taken when the pointer entered — confirmed separately: hovering continuously across
27 s of wall clock left the tooltip's figures unchanged while the item's own `MEM` text
ticked from 53.6 to 53.7 MB), so the OS read is expected to be slightly ahead, not behind.

| Figure | Tooltip | `GetProcessMemoryInfo` (same struct, read after the shot) | `Get-Process -Id 9860` | Agreement |
| --- | --- | --- | --- | --- |
| Private working set | 53.3 MB | 53.4 MB (`PrivateWorkingSetSize`) | n/a (not exposed) | within 0.1 MB |
| Working set | 167.7 MB | 167.9 MB (`WorkingSetSize`) | 167.9 MB (`WorkingSet64`) | within 0.2 MB |
| Commit (private bytes) | 112.9 MB | 113.1 MB (`PrivateUsage`) | 113.1 MB (`PrivateMemorySize64`) | within 0.2 MB |
| Peak working set | 171.6 MB | 171.6 MB (`PeakWorkingSetSize`) | n/a (not exposed) | exact |
| CPU time (user + kernel) | 7.8 s | n/a | 7.97 s (`TotalProcessorTime`) | within 0.2 s (2 s sample lag) |
| Threads | 14 | n/a | 14 (`Threads.Count`) | **exact** |
| Uptime | 2m 53s (173 s) | n/a | 176 s (`Now - StartTime`) | within 3 s (2 s sample lag) |

Every memory figure agrees within a few MB (mostly under 0.5 MB), the thread count is
exact, and CPU time/uptime are within the 2 s sampler's own lag — consistent with
`US-0148`'s acceptance criterion.

## Note on an earlier, uncontrolled hover

Before this controlled pass, the very first screenshot taken after positioning the window
(no deliberate hover) already showed the tooltip open — the window's `SetWindowPos` moved
its status bar under a cursor that happened to already be resting there, which GPUI's hover
tracking picked up as a real enter. The baseline screenshot in step 3 (cursor moved away)
confirms the tooltip is hover-driven, not a stuck or permanent overlay.

## Acceptance rework: the tooltip stays live while open

Owner ruling, 2026-09-28: "the information inside the tooltip does not update with the
interval" — rejected the first cut above. Fix: `DetailsTooltip`, a small entity nested in
the tooltip's content that observes the `StatusText` indicator entity
(`cx.observe(&source, |_, _, cx| cx.notify())`) and re-renders on every tick, instead of a
`Vec<Section>` baked into the tooltip's builder closure (which the kit only calls once per
hover). See `crates/workspace/src/widgets/status_text.rs` and
[`high-level-design.md`](../high-level-design.md).

Same setup as the walk above (`fast-dev` build of this branch after the fix, private
`USERPROFILE`/`HOME`, 1280x800 at (40,40), pid this walk started only — 22316). The cursor
was moved onto the CPU/MEM item **once** (client (1170,778)) and held there, motionless,
for the whole sequence; `PrintWindow` captures were taken at the pointer-enter moment
(t=0) and again 3 s and 6 s later, with no further input in between.

| Capture | Wall clock | CPU time (user + kernel) | Uptime | Threads |
| --- | --- | --- | --- | --- |
| [t=0](US-0148-live-t0-dark.png) | 11:19:47 | 1.8 s | 29.0 s | 18 |
| [t=3s](US-0148-live-t3-dark.png) | 11:19:50 | 1.9 s | 33.0 s | 18 |
| [t=6s](US-0148-live-t6-dark.png) | 11:19:53 | 2.0 s | 35.0 s | 18 |

CPU time and Uptime both advance between every capture (the memory figures move too: private
working set 52.4 -> 53.3 -> 53.2 MB, commit 111.8 -> 112.9 -> 112.7 MB) while the cursor never
left the item — the table is now live, matching the item's own 2 s cadence, not frozen at the
sample the tooltip opened with.

### F1: the sample moved off the UI thread

Verifier finding F1 (Major, 2026-09-28): the 2 s sampler ran synchronously inside
`StatusText::tick`, on gpui's foreground executor (the UI thread) — the `sysinfo` refresh
plus the Windows `CreateToolhelp32Snapshot` thread-count walk together cost 10-16 ms on a
machine with the verifier's process count, most of a 60 Hz frame, every 2 s. Fix: the whole
sample (`sample_label`, in `resource.rs`) now runs on `cx.background_executor()`; the tick
closure only swaps an `AtomicBool` and reads a `Mutex<Option<Label>>` slot — the same
stale-while-revalidate shape `git_status.rs` already uses for its own background `git`
calls — and posts nothing back onto the foreground beyond what `StatusText::tick` already
does with the returned `Label`.

Measured on this machine (`cargo test -p oneterm-workspace --lib -- --ignored --nocapture`,
a throwaway probe removed before this commit; own process, 8 samples each):

| | Before (synchronous, on the UI thread) | After (foreground part of the tick only) |
| --- | --- | --- |
| Samples | 10.0, 10.5, 10.7, 11.0, 11.4, 11.5, 11.9, 14.1 ms | 0.2, 0.2, 0.2, 0.3, 0.6, 1, 1, 41.6 µs |
| Median | ~11.2 ms | ~0.3 µs (all 8 sub-millisecond) |

The "before" numbers reproduce the verifier's finding (~10-16 ms) on this machine; the
"after" numbers are the foreground tick's own cost once the refresh and the thread-count
walk moved to the background executor — every sample sub-millisecond, most sub-microsecond
(the one 41.6 µs sample is the first call's cache/branch warm-up).
