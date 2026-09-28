# US-0148 GUI walk — CPU/MEM hover table

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
