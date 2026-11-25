# IN-0045 research: memory attribution

Date: 2026-09-25. Machine: Windows 11 Enterprise 10.0.26200, 31.7 GB RAM (15 GB free during
the runs). Toolchain 1.96.0 (`rust-toolchain.toml`). Every number below comes from runs
made for this record, unless the row says "estimate" or "read from code".

Raw data: [`measurements.csv`](measurements.csv) (one row per scenario and run). Scripts:
[`measure.ps1`](measure.ps1) (the protocol), [`vmregions.ps1`](vmregions.ps1) (walks the
private committed regions of one pid and says how much of each is resident).

## 1. Which number the owner saw

The status bar's `MEM` value is **commit charge** (`PrivateUsage`, "Private Bytes"), not
what Task Manager shows by default. `crates/workspace/src/widgets/resource.rs` formats
`sysinfo::Process::virtual_memory()`, which on Windows is `PrivateUsage`. The S3 screenshot
of the owner's own `dist/` build (0.6.3) shows `MEM 244.8 MB` in the status bar, and the
same scenario recorded `commit 244.8` for that pid.

| Counter (measure.ps1 column) | Win32 field | Where it appears | main, S1 |
| --- | --- | --- | --- |
| `privws` | `PrivateWorkingSetSize` | Task Manager "Memory (active private working set)", in both the Processes and Details tabs | 49.3 MB |
| `ws` | `WorkingSetSize` | Task Manager "Working set (memory)"; `sysinfo` `memory()` | 93.8 MB |
| `commit` | `PrivateUsage` | Task Manager "Commit size"; **OneTerm's status bar `MEM`** | 182.2 MB |

So "350 MB" is almost certainly the status bar (or "Commit size"), i.e. commit. Task
Manager's default column would show 50 to 90 MB for the same process. The owner should
confirm which one he read (open question in the intake).

Commit includes pages that were committed but never touched. On main at S1, 64 MiB of it is
the OOM ballast and 23.6 MiB is the first tab's glyph cache table, both untouched (section
4). Neither is in the private working set.

## 2. Protocol

`measure.ps1` launches one executable with `USERPROFILE` and `HOME` set to a fresh scratch
directory. In a release build, `oneterm_core::config_dir()` is `%USERPROFILE%\.OneTerm`
(`crates/core/src/config/shell.rs:109`), so the owner's config is never read or written.
The daily update check is turned off in that private config unless `-UpdateCheck` is given.
The window is set to 1280x800. Input is posted only to the window of the pid the script
launched. It reads `GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS_EX2`) for that pid, and
separately the sum over its descendants (OpenConsole, cmd), which are found by parent pid.

| Scenario | What happens |
| --- | --- |
| S1 | Launch with the default single local shell (cmd.exe), idle 30 s |
| S2 | Two more cmd tabs from the tab bar "+" menu, idle 20 s |
| S3 | In the active tab: `cmd /c "for /l %i in (1,1,300000) do @echo line %i"`. Wait for that cmd to exit, then idle 20 s. The 10,000-line default scrollback is now full. |
| S4 | Close the two extra tabs (S3's tab is one of them), idle 20 s |
| S5 | Idle 120 s more |

"Full" mode runs S1 to S5. "Bisect" mode runs S1, then S3 in the single tab.

## 3. Results

### 3.1 main, full protocol

Release build, commit `19237c8d`, 2 runs (values are means; runs differ by at most
0.5 MB). S2 to S4 have three tabs.

| Scenario | ws | privws | commit | children privws | children commit |
| --- | --- | --- | --- | --- | --- |
| S1 (1 tab idle) | 93.8 | 49.3 | 182.2 | 3.5 | 5.8 |
| S2 (3 tabs idle) | 98.1 | 53.3 | 235.3 | 10.6 | 23.1 |
| S3 (one of 3 tabs full) | 131.7 | 86.5 | 245.1 | 10.6 | 17.5 |
| S4 (back to 1 tab) | 96.5 | 51.2 | 183.9 | 3.5 | 5.8 |
| S5 (+2 min idle) | 96.7 | 51.4 | 184.0 | 7.3 | 12.5 |

The owner's dist 0.6.3 build gives the same figures (1 run): 49.1 / 53.4 / 86.3 / 51.1 /
51.4 privws and 181.9 / 235.7 / 244.8 / 183.7 / 184.3 commit.

What the table shows:

- **Per idle tab:** +2.0 MB private working set, **+26.6 MB commit**, and +3.5 MB private
  working set in the tab's own OpenConsole + cmd processes.
- **Filling one tab's scrollback:** +33 MB private working set.
- **Closing tabs gives the memory back.** S4 is within 2 MB of S1.
- **No growth over time.** S5 is S4 + 0.1 to 0.2 MB. The S5 children count includes
  short-lived `git` processes spawned by the status bar.

### 3.2 Per release

Release profile (`lto = "fat"`, `codegen-units = 1`), built in this worktree from each tag
with `git checkout --detach <tag>`. v0.4.2 is the owner's `dist/` build. Bisect mode,
2 runs each (v0.4.2: 1 run). S3 is one tab with its scrollback full.

| Build | Commit | S1 privws | S1 commit | S3 privws | S3 commit |
| --- | --- | --- | --- | --- | --- |
| v0.4.2 (dist) | (release zip) | 46.6 | 159.2 | 69.1 | 181.1 |
| v0.5.0 | 8bc9cb79 | 48.5 | 184.8 | 96.3 | 208.5 |
| v0.5.2 | 5c351d2e | 48.9 | 185.3 | 95.6 | 207.8 |
| v0.6.0 | fef4b866 | 49.4 | 182.4 | 81.7 | 190.9 |
| v0.6.1 | d07ba629 | 49.6 | 182.4 | 82.1 | 191.2 |
| v0.6.2 | 04cc3bb3 | 49.2 | 182.1 | 82.2 | 191.4 |
| main | 19237c8d | 49.4 | 182.2 | 82.0 | 191.2 |
| gpui + gpui-component floor | 19237c8d (throwaway example) | 41.7 | 84.1 | n/a | n/a |

All five tags built on this toolchain. Build times were 6 to 9 minutes each; main took
12.7 minutes from a cold target directory.

- **v0.4.2 to v0.5.0 is the only step up:** +25.6 MB commit at idle and +27 MB private
  working set with a full tab. Section 4.1 pins it to one allocation.
- **v0.5.2 to v0.6.0 went down:** −14 MB private working set and −17 MB commit with a full
  tab. That is the window in which the `oneterm-vt` engine replaced the previous one
  (IN-0029).
- **v0.6.0 to main is flat** within 0.5 MB in every column. IN-0043 (elevation), IN-0044
  (semantic highlighting phase 2), BUG-0071 and the SFTP fixes add nothing measurable.

The floor is a hello-world window: `gpui_platform::application()`, `gpui_component::init`,
and one `Root` with a label and a button. It was built from the same `Cargo.lock` in the
same release profile, then removed. OneTerm at S1 is floor + 7.7 MB private working set and
floor + 98 MB commit.

### 3.3 Toggles on main

Bisect mode, 1 run each. The baseline is main in Bisect mode, 2 runs: S1 49.4 / 182.2,
S3 82.0 / 191.2.

| Config change (private `terminal.json`) | S1 privws | S1 commit | S3 privws | S3 commit |
| --- | --- | --- | --- | --- |
| `semantic_highlighting: "off"` | 49.8 | 182.5 | 81.7 | 190.9 |
| `ligatures: false` | 49.0 | 181.9 | 83.1 | 192.3 |
| `completion.enabled: false` | 49.4 | 182.2 | 82.0 | 191.2 |
| `scrollback_history: 1000` | 49.2 | 182.1 | 75.4 | 184.6 |
| `scrollback_history: 100000` | 54.8 | 187.5 | 150.0 | 259.6 |
| update check on (`-UpdateCheck`) | 49.8 | 182.6 | n/a | n/a |

Highlighting, ligatures, completion and the update check (reqwest, rustls, native roots)
are each within about 1 MB, which is run-to-run noise. Scrollback is the only toggle that
moves memory:

- **Per history row:** 0.75 KB. (150.0 − 75.4) MB over 99,000 rows.
- **Ring preallocation:** at 100,000 lines the ring costs 5.5 MB before any output.

## 4. Attribution on main

### 4.1 The glyph cache table: 23.6 MiB committed per terminal view

This was found by rebuilding main (fast-dev profile, throwaway) with the global allocator
logging every allocation of 1 MiB or more, with a backtrace, then running the full
protocol. The only allocation above 1.1 MB was `alloc 24780816` bytes, three times: one per
tab. Its call chain:

`TerminalView::new` (`crates/terminal-view/src/terminal_view/view.rs:285`) →
`RenderState::new` (`render/state.rs:186`) → `GlyphCache::new`
(`render/glyphs.rs:111`) → `HashMap::with_capacity(4096)`

- **Why 24.78 MB:** hashbrown rounds 4096 up to 8192 buckets. Each bucket holds a
  `(RunKey, Entry)` of 3,024 bytes, because `Entry` stores gpui's `ShapedLine` by value. A
  `ShapedLine` carries an inline `SmallVec<[DecorationRun; 32]>`
  (`gpui-pre-0.3.3/src/text_system/line.rs:43`). 8192 × 3024 + 8208 control bytes =
  24,780,816.
- **Idle cost:** at idle the table is committed but untouched. `vmregions.ps1` shows these
  as single-region 23.6 MB allocations with 0.0 MB resident.
- **Cost under output:** a burst of distinct text (S3: 300,000 different `line N` runs)
  fills the table. The same region is then 23.6 MB resident. `scrollback_history: 1000` S3
  is still +26 MB private working set over S1, with only about 0.8 MB of grid (below), so
  most of S3's jump is this table, not the scrollback.
- **When:** the table was added in `d21c8cad` (2026-09-08, "render core — frame model, row
  plans, glyph cache"). That is inside the v0.4.2 → v0.5.0 window where the per-release
  table steps up by 25.6 MB commit.
- **Scope:** it is one table per `TerminalView`, so every tab **and every split pane**
  pays it.
- **Cap:** the cap is soft (`retain` of entries older than two generations,
  `glyphs.rs:183`), so a table that is full of recent entries stays at its full size.

### 4.2 The OOM ballast: 64 MiB of commit, 0 resident

`crates/app/src/oom.rs:70` allocates 64 MiB with `System.alloc` and never writes to it.
The Windows heap serves a block that large with `VirtualAlloc(MEM_COMMIT)`. `vmregions.ps1`
shows it on every build since v0.4.1 (`05df6ffa`, 2026-08-19): one 64.0 MB region, 0.0 MB
resident. It adds exactly 64 MiB to commit and to the status bar `MEM`, and nothing to
Task Manager's "Memory". Committed is its purpose: it reserves commit headroom for the
retry path. So shrinking it or dropping it is a design decision, not a bug fix.

### 4.3 Scrollback: 0.75 KB per history row

Measured, section 3.3. The code says the same:

- each row is `vec![template; cols]` of 8-byte cells, full width even when the line is
  `line 123` (`crates/vt/src/grid/row.rs:97`);
- each row also takes a 48-byte ring slot;
- the ring is preallocated to `(limit + 1024).next_power_of_two()` slots
  (`crates/vt/src/grid/screen.rs:291,314`) and every slot is written with `None`, so it is
  resident: 0.75 MB at 10,000 lines, 6 MB at 100,000, about 50 MB at the 1,000,000 maximum.

**Estimate:** at the default 10,000 lines, a full tab holds about 7.5 MB of grid at this
window width. The throwaway allocation probe measured 7.2 MiB for 10,038 rows at 88 columns.

History is freed when a row is trimmed and when a tab closes (S4 returns to S1). There is
no leak.

### 4.4 Other per-tab costs (read from code and the allocation log)

| Item | Size | Source |
| --- | --- | --- |
| PTY read buffer, `vec![0u8; 1 MiB]` (zeroed, touched only as far as reads go) | 1 MiB commit per tab | `crates/local-shell/src/event_loop.rs:275`, seen in the allocation log |
| Threads: "PTY owner", conout, conin | reserve only; the rest of the 2 MiB default stacks is not committed | `event_loop.rs:186`, `crates/vt/src/pty/windows/pipe.rs:171,271` |
| Plan cache, row plans, `LineMark`, wrap-run scans | viewport-sized, rotated on scroll, not keyed by history | `render/plan_cache.rs:140-255` |
| Gutter timestamps | 4 bytes per history line, trimmed with the scrollback | `terminal_view/gutter_timestamps.rs:113-125` |
| Completion catalog | lazily parsed, per tab, 87 files / 35 KB of JSON in total | `crates/terminal-view/src/completion/controller.rs:50` |
| OpenConsole + cmd (separate processes) | 3.5 MB private working set per tab | measured, children columns |

### 4.5 Per-process items

| Item | Finding | Evidence |
| --- | --- | --- |
| gpui + gpui-component | 41.7 MB private working set, 84.1 MB commit | measured floor |
| OneTerm above the floor at S1, beyond the ballast and one glyph table | about 7.7 MB private working set, about 10 MB commit | measured, by subtraction |
| Highlight rules (1 Aho-Corasick + 3 regexes + 5 prompt regexes, `LazyLock`) | one per process; turning highlighting off changes nothing measurable | `crates/highlight/src/rules.rs:188`, toggle |
| Themes: 24 JSON files, 197 KB, parsed eagerly | estimate under 1 MB | `crates/theme/src/theme.rs:179-188` |
| Font fallbacks | not loaded eagerly; `all_font_names()` runs only in Settings | `crates/settings-ui/src/terminal/font.rs:143` |
| Update check (reqwest, rustls, native roots) | only when due, at most daily; +0.4 MB, +2 threads | toggle |
| SSH tokio runtime | created on first SSH use (`OnceLock`) | `crates/ssh/src/session.rs:117-135` |
| Git status bar | spawns `git` at most every 2 s; nothing resident in-process | `crates/workspace/src/widgets/git_status.rs:32,100` |
| Elevation (IN-0043) | a token read and a `OnceLock`; nothing measurable | `crates/app/src/elevation.rs` |
| Logging | `env_logger` to stderr, no in-memory buffer | `crates/app/src/lib.rs:110-117` |
| Sixel | up to 64 MiB RGBA per image, 256 placements, **no total byte cap** | `crates/vt/src/graphics/mod.rs:55-67`; not exercised by these scenarios |

### 4.6 What 350 MB of commit could be made of

**Estimate, not measured.** Using the per-unit costs above: 182 MB for the first tab, plus
26.6 MB for each further tab or pane, plus about 8 MB for each tab with a full 10,000-line
history, plus the glyph table turning resident in busy tabs.

- Six tabs with three busy ones: 182 + 5 × 26.6 + 3 × 8 ≈ 339 MB commit.
- The same load on v0.4.2 (159 + 5 × 3 + 3 × 8): about 198 MB.

That matches the owner's "350 now, about 200 before" without any leak. It is still an
estimate until the owner confirms his tab count and which number he read.

## 5. Ranked table

MB are on main, release build, 2026-09-25. "Commit" is what the status bar shows.
"Private WS" is what Task Manager shows.

### Cheap and safe

| # | Component | MB on main | First release | Proposed reduction | Expected saving | Effort | Risk | Packet |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Glyph cache table, `GlyphCache::new` | 23.6 commit per view (measured); up to 23.6 private WS per busy view (measured) | v0.5.0 (`d21c8cad`) | Do not preallocate (`HashMap::new()`), and store `Arc<ShapedLine>` (or the `Arc<LineLayout>` plus the run) so a bucket is about 40 B instead of 3,024 B. Optionally one cache per window instead of per view. | 23 MB commit per tab and per pane at idle; about 20 MB private WS per busy tab (estimate: 4096 entries × about 3 KB → bucket plus boxed entry only while in use) | S | Low: hit rate unchanged; the render bench guards frame time | `BUG-0078` |
| 2 | Status bar `MEM` reads commit, not the number Task Manager shows | Shows 182 where Task Manager shows 49 (measured) | before v0.4.2 | Show the private working set (`PrivateWorkingSetSize`), or label the value as "commit" | Perception: 130+ MB off the displayed number, no real change | S | Low (one widget); the label wording needs the owner | `US-0137` |

### Needs a decision

| # | Component | MB on main | First release | Proposed reduction | Expected saving | Effort | Risk | Packet |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 4 | OOM ballast | 64 commit, 0 private WS (measured) | v0.4.1 (`05df6ffa`) | Keep it (it works as designed), shrink it to 16 MiB, or reserve it and commit only on the first failure. That last option gives up the headroom the ballast exists to give. | 0 to 64 MB commit, 0 private WS | S | Medium: survival under a system-wide OOM spike | `DEC-0020` |
| 5 | Scrollback rows stored at full width | 0.75 KB per row: 7.5 MB per full tab at 10,000 lines, 75 MB at 100,000 (measured) | v0.6.0 (engine); similar before | Store history rows trimmed to their last occupied cell (`occ` is already in `RowHeader`), padding on read | about 80 % of history for short lines such as S3's (estimate: 11 of 88 cells used); nothing for full-width output | M | Medium: `oneterm-vt` is an external contract (reflow, selection, search, snapshot) | `US-0138` |
| 6 | Ring preallocated to the scrollback limit | 0.75 MB at 10,000 lines, 5.5 MB at 100,000 (measured), about 50 MB at 1,000,000 (estimate) | v0.6.0 | Grow the ring with the history (amortized doubling up to the limit) | Same as the column to its left, per tab, until history fills | S-M | Low-medium: `oneterm-vt` internals and the vt-paranoid integrity walk | `US-0139` |
| 7 | Sixel images have no total byte cap | 0 in these scenarios; up to 64 MiB per image (read from code) | v0.5.x (IN-0028) | A per-terminal byte budget that evicts the oldest image | Bounds a worst case; nothing on typical use | S-M | Medium: behaviour change for image-heavy tools | `US-0140` |

Not worth a packet on this evidence:

- semantic highlighting, ligatures and completion: each under 1 MB (toggles above);
- themes: under 1 MB (estimate);
- the update check: +0.4 MB, and it runs at most daily;
- the git status bar, elevation, and logging;
- the 1 MiB PTY read buffer (1 MB commit per tab). It is kept on purpose: shrinking it spun
  a test binary at 100 % CPU (`crates/local-shell/src/event_loop.rs:53-55`).

## 6. Gaps

- The per-release table has one idle scenario and one output scenario. S2, S4 and S5 were
  run on main, dist 0.6.3 and v0.4.2 only. The v0.4.2 S2, S4 and S5 rows are invalid,
  because its tab bar has no "+" at the scripted point (marked in the CSV).
- The column count (about 88) is inferred from the window width and the probe. The
  script does not read it.
- The saving estimates for items 1, 5 and 6 are arithmetic from the measured sizes. They
  are proven only when the packets land and `measure.ps1` is re-run.
- SSH sessions, SFTP transfers, split panes, and Sixel output were not measured.
- The allocation log came from a fast-dev build. Its absolute numbers are much higher
  (debug code). Only the call sites and sizes from it are used here.
