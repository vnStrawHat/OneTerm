# IN-0045 research, phase 2: a coding-agent load, the glyph table, and allocation sizes

Date: 2026-09-25. Same machine and toolchain as [`memory-attribution.md`](memory-attribution.md)
(Windows 11 10.0.26200, 31.7 GB RAM, commit limit 31.7 GB, toolchain 1.96.0).

This phase answers three questions the owner raised after phase 1:

- Where do the owner's **346 MB** come from? The owner runs v0.6.3 with two tabs, each
  running the `claude` CLI, and a 10,000-line scrollback limit.
- What size should the OOM ballast be? The answer is in
  [`DEC-0020`](../../../decisions/DEC-0020-oom-ballast-size.md). This file holds the
  measurements behind it.
- Should history storage change? The answer is in
  [`history-storage-assessment.md`](history-storage-assessment.md).

Raw rows: [`measurements.csv`](measurements.csv), labels starting with `s6-`.

## 1. Build and protocol

**The build.** Main `898148e5`, release profile, with a throwaway probe. The probe was
removed before commit:

- `oom.rs` counts gross and live bytes through the global allocator and logs every
  allocation of 256 KiB or more. One extra run captured a backtrace for each new size.
- A thread prints the counters every 250 ms.
- `GlyphCache` logs its entry count and capacity every 600 frames, and on any capacity
  change.

Two build settings differ from `dist`:

- `lto = "thin"`, because fat LTO with line tables failed twice with `rustc-LLVM ERROR: out
  of memory` while another build ran on the machine.
- line-table debug info.

The probe does not move the counters. S1 on this build is 49.1 / 182.0 MB (privws /
commit). Main in phase 1 was 49.3 / 182.2.

**The backtrace run is excluded.** That run is kept only for its call sites.
Symbolizing a backtrace loads the PDB, which put S1 at 186 / 320 MB.

**S6** (new `-Mode S6` in [`measure.ps1`](measure.ps1)):

1. S1: launch at 1280x800 and idle 30 s.
2. Open a second Command Prompt tab.
3. Run [`tui-mimic.py`](tui-mimic.py) for 180 s in both tabs.
4. Switch the visible tab every 10 s, so both views render.
5. Wait for both loads to exit, idle 20 s, snapshot.

The mimic loops three phases:

- 10 s of alternate-screen frames at 30 frames/s, with 256-colour and truecolour runs and
  a changing status line.
- 10 s of Ink-style main-screen repaint: 30 frames/s of cursor-up and erase-line over a
  30-line block, which is how `claude` redraws. It adds a permanent line every 5 frames.
- A burst of 10,000 log lines of random length.

Per tab, one run wrote 90,527 history lines, with a mean length of 49.2 characters on an
89-column grid. The real `claude` CLI was not driven.

## 2. S6 results: which limit gives 346 MB

MB. Each value is the mean of 2 runs; the run values are in brackets. Two tabs. The
children's figures (OpenConsole + cmd + python, both tabs) were 7.2 privws / 11.8 commit in
every run.

| Scrollback limit | Window during load | S1 privws / commit | S6 privws | S6 commit (status bar `MEM`) |
| --- | --- | --- | --- | --- |
| 10,000 (default) | 1280x800 | 49.3 / 182.1 | 94.2 (94.4, 93.9) | **237.8** (239.2, 236.3) |
| 50,000 | 1280x800 | 51.4 / 184.2 | 154.6 (155.9, 153.2) | 298.1 (298.3, 297.8) |
| 100,000 | 1280x800 | 54.4 / 187.2 | 216.2 (218.0, 214.3) | **360.6** (361.6, 359.6) |
| 10,000 (default) | 1920x1040 | 49.3 / 182.1 | 110.3 (110.1, 110.4) | 275.7 (275.4, 276.0) |

**No tested limit gives exactly 346 MB.** The nearest are:

- 100,000 lines at 1280x800 gives 360.6 MB.
- Interpolating linearly between 50,000 and 100,000 puts 346 MB at about 88,000 lines
  per tab.

**At the owner's 10,000 lines the mimic gives 238 MB.** That is about 108 MB short.

- A larger window closes 38 MB of the gap: 275.7 MB at 1920x1040.
  - About 16 MB of that is live heap: the grid, the snapshot, and render state that
    scales with the number of cells.
  - The rest is a new 32.3 MB committed block that is not resident. It is outside the
    Rust allocator: the probe never saw it. Its size matches four 1920x1040x4-byte
    frame buffers, so it is probably swap-chain or composition surfaces. At 1280x800
    the matching block is 15.7 MB, of which 1.0 MB is resident.
- Nothing else in S6 grows with time. In the 250 ms trace, commit is flat within 5 MB
  from 30 s to 180 s. It rises only while each tab's history fills.

**Candidate causes for the remaining ~70 MB.** These are not measured. They are the
questions to put to the owner:

- **A larger or HiDPI window.** Frame buffers scale with physical pixels: about 60 MB of
  commit for four 2560x1440 buffers. History rows scale with the column count.
- **Split panes or more views.** Each costs 26.6 MB of commit idle (phase 1), plus 7 to
  8 MB resident once busy.
- **A long session.** It adds heap fragmentation, and history filled with wider lines
  than the mimic's 55 %.

A per-tab breakdown at the default limit, 1280x800 (S6 minus S1, then minus phase 1's
idle second tab of +2.0 privws / +26.6 commit):

- Busy load on two tabs adds about 43 MB privws and 29 MB commit.
- The glyph tables become 6.7 to 8.3 MB resident each (`vmregions.ps1`).
- History is about 7.6 MB per tab: 10,000 rows × 760 B, see the history assessment.
- The rest is the style interner, the snapshot, the frame, and heap slack.

`vmregions.ps1` on the default-limit S6 process: 230.4 MB committed, 91.9 MB resident.

| Allocation | Commit MB | Resident MB |
| --- | --- | --- |
| OOM ballast | 64.0 | 0.0 |
| Heap segment | 31.4 | 10.2 |
| Glyph table, tab 1 | 23.6 | 7.8 |
| Glyph table, tab 2 | 23.6 | 7.0 |
| Frame-sized block (see above) | 15.7 | 1.0 |
| Heap segments | 13.3, 12.8, 12.7, 10.5 | 12.7, 12.8, 10.0, 10.3 |
| Other heap segments | 8.0, 6.7, 4.1 | 8.0, 6.7, 4.1 |

At 100,000 lines the same walk shows 352.3 MB committed and 215.2 MB resident. The extra
~120 MB is spread over the resident heap segments. No single new large block appears.

## 3. For BUG-0078: does the glyph table double under a chatty TUI?

The hypothesis was this. A TUI that produces many distinct shaped runs makes the per-view
glyph `HashMap` grow by doubling:

- 16,384 buckets would be 49 MB.
- 32,768 buckets would be 99 MB.

Two views at 99 MB would explain 346 MB.

**Not reproduced.**

**What the code bounds** (`crates/terminal-view/src/render/glyphs.rs`):

- `GlyphCache::new` calls `HashMap::with_capacity(4096)`. hashbrown rounds that up to
  8,192 buckets, which hold 7,168 entries before the table grows. At 3,024 B per bucket
  that is 24,780,816 B.
- On an insert with `len >= 4096`, the cache runs `retain` and keeps only entries used in
  the last three generations (`GRACE = 2`). The generation advances once per prepaint
  (`crates/terminal-view/src/render/element.rs:138`).
- hashbrown never shrinks on `retain` or `clear`.
- So the table grows past 8,192 buckets only when **more than 7,168 distinct runs are
  shaped within three consecutive frames**. A run is one row's text between font changes
  (bold, italic, forced width). Colour does not split a run.

**What S6 measured.** All 10 S6 runs, 20 views, at both window sizes:

| Measure | Value |
| --- | --- |
| Largest entry count seen in a view | 1,435 (sampled every 600 frames) |
| Capacity | 7,168 in every sample |
| Capacity-change events | 0 (these are logged on every insert, so a transient growth could not be missed) |
| Table size (`vmregions.ps1`) | 23.6 MB committed per view, 6.7 to 8.3 MB resident |
| 49 MB or 99 MB blocks | none, in any run |

The `retain` path was never reached: 1,435 < 4,096.

**What doubling would take.** About 2,400 distinct runs per frame, for three frames in a
row. A claude-like screen is 40 to 60 rows of one to three runs each. Doubling would need
bold or italic alternating on nearly every word across a full-screen grid, redrawn with
new text every frame. That is not what the mimic or the `claude` CLI draws.

**What this means for BUG-0078.**

- The table costs 23.6 MB of commit per view from the moment the view opens.
- Under this load only 7 to 8 MB of it becomes resident.
- An entry-sized design (no preallocation, entry behind an `Arc`) removes about 23 MB of
  commit per view. It also removes about 7 MB of private working set per busy view:
  1,435 entries × 3 KB is the resident part measured here.
- There is a CPU cliff: once `len >= 4096`, every cache miss runs an O(n) `retain`. S6
  never reached it, but a bounded design should evict without a full scan per miss.

## 4. Allocation sizes and rates (input to DEC-0020)

**Largest single allocations** through the Rust allocator, S1 to S6, all runs. Call sites
come from the backtrace run.

| Bytes | What | Call site |
| --- | --- | --- |
| 24,780,816 | Glyph table, once per view (gone after BUG-0078) | `GlyphCache::new` ← `RenderState::new` ← `TerminalView::new` |
| 6,291,456 / 3,145,728 / 786,432 | History ring at 100,000 / 50,000 / 10,000 lines, once per tab | `Screen::new` ← `TerminalGrid::new` ← `Terminal::new` |
| 2,228,240, 1,114,128 | Style interner hash table rehash (bounded: ids are `u16`) | `Interner::style` ← `Handler::sgr` |
| 1,048,576 | PTY read buffer per tab; gpui arena chunk | `event_loop.rs`; `gpui::arena::Chunk::new` |
| 917,504 | Style interner entry vector growth | `Interner::style` |
| 688,128, 344,064 | gpui scene paint-operation vector growth | `Scene::push_layer`, `Scene::insert_primitive` |
| 647,168 | ConPTY output pipe buffer growth | `pty::windows::pipe::push` |

**Sixel images** can reach 64 MiB each (`crates/vt/src/graphics/mod.rs`). They were not
exercised here.

**Peak rate during the load**, over any 3 s window after the first 40 s:

| | 1280x800, all limits | 1920x1040 |
| --- | --- | --- |
| Gross bytes allocated | 224 to 240 MiB | 154 to 158 MiB |
| Net rise of the live heap | ≤ 9.6 MiB | ≤ 15.1 MiB |
| Net rise of commit (250 ms trace) | ≤ 10.2 MB | ≤ 15.9 MB |

Gross is churn: the heap reuses freed blocks without new commit, so it is not what a
retry needs. The net rise is. At both sizes it comes from the history filling in the
first cycle. Once history is full, net growth is close to zero.

## 5. Gaps

- The real `claude` CLI was not measured. The mimic's history is 55 % of the grid width.
  Its style count and its redraw pattern are approximations.
- The owner's window size, DPI, pane count and session length are unknown. They are the
  likely remainder of the gap at 10,000 lines.
- The 32.3 MB and 15.7 MB frame-sized blocks are attributed only by their size and by
  being invisible to the Rust allocator.
- Two runs per configuration. The runs differ by at most 3 MB.
