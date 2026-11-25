# US-0050 — DOOM-fire performance: old engine vs new render engine

Date: 2026-09-09
Old engine: `main` @ `12b3c11`, binary `target/perf-main-target/fast-dev/oneterm.exe`
New engine: `refactor/terminal-render-engine` @ `4f5988e`, binary `target/perf-new-target/fast-dev/oneterm.exe`
Both built with the `fast-dev` profile and the `terminal-diagnostics` feature. No rebuild,
no source edit and no commit was made for this measurement.

## Machine, window and grid

| Fact | Value |
| --- | --- |
| Host | Windows 11 Enterprise 10.0.26200, Intel Core i7-12700 (20 logical cores) |
| GPU / display | Intel UHD Graphics 770, 1920 x 1080, scale 1.0 |
| Window | maximized via `ShowWindow(hwnd, 3)`, client capture 1936 x 1048 px (identical for both runs) |
| Config | shared `target/terminal.json` (unmodified): Lilex 15 px, line height 1.2, padding L10/R5, gutter off, `cmd.exe` shell |
| Grid | **52 rows** (logged by both engines). Columns are not logged; the terminal area is ~1422 px wide and the measured cell width is 9.0 px (run-length histogram of one scan line of `perf-new.png`), so ~156-158 columns. Both runs used the same window, font and config, and both report 52 rows, so the grid is the same. |
| Workload | `doom-fire.exe` (identical tool in both target dirs), full-screen animation, ~40 s per run |
| Docked panels | Session list + SFTP browser open in both runs (same layout) |

## Commands used

```powershell
$env:RUST_LOG = 'info,oneterm=debug'
# OLD
$p = Start-Process -FilePath 'D:\...\target\perf-main-target\fast-dev\oneterm.exe' `
     -WorkingDirectory 'D:\TrungKFC-Research\Rust\myTerm2' -PassThru `
     -RedirectStandardError 'D:\...\target\perf-old.log'
Start-Sleep 8; $p.Refresh(); $h = $p.MainWindowHandle
[Win]::ShowWindow($h, 3); Start-Sleep 3          # SW_MAXIMIZE
# screenshot: PrintWindow($h, $hdc, 2) -> target/perf-old-prompt.png
# type the workload: one WM_CHAR (0x0102) per character, 25 ms apart, then
# WM_KEYDOWN/WM_KEYUP (0x0100/0x0101) VK 0x0D for Enter
'D:\...\target\perf-main-target\fast-dev\doom-fire.exe'
Start-Sleep 20                                    # screenshot -> target/perf-old.png
Start-Sleep 20
[Win]::PostMessage($h, 0x0102, [IntPtr]3, [IntPtr]0)   # Ctrl+C (ETX)
Start-Sleep 3; Stop-Process -Id $p.Id -Force
# NEW: identical, with perf-new-target and target/perf-new.log / perf-new*.png
```

Driver script: `<scratchpad>/run-perf.ps1` (temporary, outside the repo). The two runs were
sequential, never concurrent; an unrelated pre-existing `dist\oneterm.exe` instance (pid 15476)
was never touched.

## Screenshots

- Old, prompt: `target/perf-old-prompt.png` — fire at ~20 s: `target/perf-old.png`
- New, prompt: `target/perf-new-prompt.png` — fire at ~20 s: `target/perf-new.png`

Both screenshots show the fire filling the whole terminal pane with the workload's own status
line at the bottom (`316.85 fps` old, `307.06 fps` new — that is doom-fire's internal loop, not
OneTerm's frame rate). Rendering is visually equivalent; no corruption, no missing rows.

## Results (medians over the 9 fire-period diagnostics lines per engine)

| Metric | OLD (`element::paint`) | NEW (`render::diagnostics`) | Delta |
| --- | --- | --- | --- |
| Prepaint per frame | 7 681 us | 7 334 us | -4.5 % |
| Paint per frame | 9 210 us | **493 us** | **-94.6 %** |
| Prepaint + paint per frame | 16 930 us | **7 838 us** | **-53.7 %** |
| Frame latency p95 | 19 888 us | **8 438 us** | -57.6 % |
| Frame latency p99 | 21 885 us | **9 015 us** | -58.8 % |
| Rendered frames / s | 30.0 (absolute `frame=` counter, 150 per 5 s) | ~60 (208 -> 508 samples in 5 s during ramp-up; the sample ring caps at 512 so later lines cannot show it) | x2 |
| Quads per frame | 9 572 | 9 704 | +1.4 % |
| Rows planned / laid out per frame | 52 of 52 (`row_layouts`, `dirty=52`) | 47 of 52 candidate (`rows_planned`) | -10 % |
| Shape calls per frame | 1 (`shapes`) | 0 (`shape_calls`) | — |
| Layers per frame | not counted | **1** | — |
| Snapshot per frame | 73 us (`snapshot_us`, p95 87 us) | not logged (see note) | not comparable |
| PTY throughput | 39.4 MiB/s (11.5 -> 39.7) | 37.5 MiB/s (10.9 -> 37.9) | -4.8 % |
| PTY pump busy | 45 % (wait-bound) | 43 % (wait-bound) | — |
| PTY `Term` lock hold p95 / p99 | 1 540 / 1 745 us | 1 541 / 1 729 us | ~equal |

Counters that exist on only one side (not comparable): OLD `bg_rects` (median 4 779),
`runs` (1), `hashes` (0), `info_us` (0), `snapshot_us` / snapshot percentiles;
NEW `glyph_hits` (16), `glyphs` (52), `paths` (0), `url_scans` (1), `layers` (1),
`rows_candidate` (52). Neither engine logs allocation counts, so allocations per frame
were not measured. `snapshot_calls` is tracked by the new engine but is **not** in its log
line; single-snapshot-per-frame is instead established by the code (one
`state.frame.snapshot(...)` call site in `render/element.rs:121`) and by the unit tests
asserting `snapshot_calls == 1` (`render/element_tests.rs`, `terminal_view/view_tests.rs`).

## Raw diagnostics — OLD (`target/perf-old.log`)

```
[01:13:56Z] [TerminalElement] frame=25 lines=52 dirty=1 row_layouts=1 quads=1 bg_rects=0 shapes=3 runs=3 hashes=0 info_us=25 snapshot_us=110 prepaint_us=339 paint_us=14 | snapshot p95=130us p99=178us | frame p95=2792us p99=22344us samples=25
[01:14:01Z] [TerminalElement] frame=158 lines=52 dirty=52 row_layouts=52 quads=9341 bg_rects=4721 shapes=1 runs=1 hashes=0 info_us=0 snapshot_us=80 prepaint_us=8870 paint_us=10747 | snapshot p95=130us p99=178us | frame p95=20897us p99=22510us samples=158
[01:14:06Z] [TerminalElement] frame=308 lines=52 dirty=52 row_layouts=52 quads=9608 bg_rects=4779 shapes=1 runs=1 hashes=0 info_us=0 snapshot_us=76 prepaint_us=7692 paint_us=9015 | snapshot p95=117us p99=178us | frame p95=21196us p99=22344us samples=308
[01:14:11Z] [TerminalElement] frame=458 ... quads=9389 bg_rects=4704 shapes=1 runs=1 snapshot_us=73 prepaint_us=7569 paint_us=9157 | frame p95=21045us p99=22344us samples=458
[01:14:16Z] [TerminalElement] frame=608 ... quads=9792 bg_rects=4930 shapes=1 runs=1 snapshot_us=72 prepaint_us=7639 paint_us=9000 | frame p95=20914us p99=21885us samples=512
[01:14:21Z] [TerminalElement] frame=757 ... quads=9921 bg_rects=4939 shapes=1 runs=1 snapshot_us=73 prepaint_us=7720 paint_us=9210 | frame p95=19652us p99=22584us samples=512
[01:14:26Z] [TerminalElement] frame=907 ... quads=9411 bg_rects=4710 shapes=1 runs=1 snapshot_us=72 prepaint_us=7681 paint_us=9919 | frame p95=19435us p99=20893us samples=512
[01:14:31Z] [TerminalElement] frame=1057 ... quads=9775 bg_rects=4892 shapes=1 runs=1 snapshot_us=73 prepaint_us=7900 paint_us=10679 | frame p95=19888us p99=21480us samples=512
[01:14:36Z] [TerminalElement] frame=1208 ... quads=9400 bg_rects=4709 shapes=1 runs=1 snapshot_us=73 prepaint_us=7599 paint_us=9133 | frame p95=19782us p99=21069us samples=512
[01:14:41Z] [TerminalElement] frame=1358 ... quads=9572 bg_rects=4790 shapes=1 runs=1 snapshot_us=76 prepaint_us=7631 paint_us=10649 | frame p95=19847us p99=21270us samples=512
[01:14:10Z] [PTY pump] 39.7 MiB/s parsed | parse=904ms wait=1095ms over 2.0s | pump 45% busy (wait-bound: ConPTY/producer is the limiter) | lock p95=1512us p99=1693us samples=660
```

## Raw diagnostics — NEW (`target/perf-new.log`)

```
[01:15:17Z] terminal render: rows 1/52 candidate, 0 planned, 0 shaped, 0 glyph hits, 0 url scans, 1 quads, 0 paths, 17 glyphs, 1 layers, prepaint 95 us, paint 6 us, p95 3518 us, p99 15259 us over 24 frames
[01:15:22Z] terminal render: rows 52/52 candidate, 45 planned, 2 shaped, 14 glyph hits, 1 url scans, 9519 quads, 0 paths, 52 glyphs, 1 layers, prepaint 7047 us, paint 483 us, p95 8317 us, p99 9049 us over 208 frames
[01:15:27Z] terminal render: rows 52/52 candidate, 46 planned, 1 shaped, 15 glyph hits, 1 url scans, 9507 quads, 0 paths, 52 glyphs, 1 layers, prepaint 7180 us, paint 488 us, p95 8438 us, p99 9116 us over 508 frames
[01:15:32Z] terminal render: rows 52/52 candidate, 46 planned, 0 shaped, 16 glyph hits, 1 url scans, 9798 quads, 0 paths, 52 glyphs, 1 layers, prepaint 6873 us, paint 496 us, p95 8494 us, p99 8919 us over 512 frames
[01:15:37Z] terminal render: rows 52/52 candidate, 47 planned, 1 shaped, ..., 9310 quads, 0 paths, 52 glyphs, 1 layers, prepaint 7618 us, paint 473 us, p95 8443 us, p99 8895 us over 512 frames
[01:15:42Z] terminal render: rows 52/52 candidate, 51 planned, 0 shaped, ..., 9519 quads, 0 paths, 52 glyphs, 1 layers, prepaint 7525 us, paint 482 us, p95 8518 us, p99 9372 us over 512 frames
[01:15:47Z] terminal render: rows 52/52 candidate, 49 planned, 1 shaped, ..., 9704 quads, 0 paths, 52 glyphs, 1 layers, prepaint 7438 us, paint 501 us, p95 8489 us, p99 9952 us over 512 frames
[01:15:52Z] terminal render: rows 52/52 candidate, 48 planned, 0 shaped, ..., 9734 quads, 0 paths, 52 glyphs, 1 layers, prepaint 7507 us, paint 509 us, p95 8383 us, p99 9015 us over 512 frames
[01:15:57Z] terminal render: rows 52/52 candidate, 46 planned, 0 shaped, ..., 9771 quads, 0 paths, 52 glyphs, 1 layers, prepaint 6829 us, paint 493 us, p95 8397 us, p99 9015 us over 512 frames
[01:16:02Z] terminal render: rows 52/52 candidate, 47 planned, 0 shaped, ..., 9812 quads, 0 paths, 52 glyphs, 1 layers, prepaint 7334 us, paint 504 us, p95 8397 us, p99 8758 us over 512 frames
[01:15:39Z] [PTY pump] 37.3 MiB/s parsed | parse=866ms wait=1133ms over 2.0s | pump 43% busy (wait-bound: ConPTY/producer is the limiter) | lock p95=1565us p99=1753us samples=619
```

## Verdict

The new engine meets the HLD budget on this workload. Per-frame cost is not worse than the old
crate — it is **53.7 % lower** (7.84 ms vs 16.93 ms prepaint+paint median, p95 8.44 ms vs
19.89 ms, p99 9.02 ms vs 21.89 ms), and the app renders the fire at ~60 fps where the old engine
sustained a flat 30 fps at the same grid, same window and the same PTY throughput (37.5 vs
39.4 MiB/s, both wait-bound on ConPTY with identical `Term` lock-hold percentiles, so the input
side is unchanged and the whole win is in the render path — almost all of it in paint, 493 us vs
9 210 us). "Plans only changed rows" holds: at idle the log shows `rows 1/52 candidate, 0
planned`, and even under a full-screen animation the planner rebuilds 47 of 52 rows instead of
the old engine's unconditional 52 `row_layouts`. "Block runs coalesced" holds as far as the
counters can show: 0-2 shape calls per frame with 16 glyph-cache hits over ~8 200 cells, and
9 704 quads — 1.4 % more than the old engine's 9 572, which is the expected floor for DOOM fire,
where adjacent cells almost never share a colour (the screenshot's scan-line histogram is
dominated by single-cell 9 px runs, with a minority of 18 px two-cell runs, i.e. the run builder
does coalesce where the data allows). "One grid layer" holds: `layers = 1` on every fire frame
(a second layer appears only when the cursor is visible, as at the idle prompt). The one budget
item not observable from the log is "one snapshot per frame": the new log line omits
`snapshot_calls`; it is covered by the single call site in `render/element.rs:121` and by unit
tests. Anomalies: none affecting the comparison — both runs emit the same benign
`gpui_windows::directx_devices` DXGI-debug-interface warning at startup, neither run hung, and
no allocation counter exists in either build.
