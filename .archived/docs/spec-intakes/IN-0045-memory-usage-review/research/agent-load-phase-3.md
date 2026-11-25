# IN-0045 research, phase 3: a maximized window, a richer load, and session length

Date: 2026-09-25. Same machine and toolchain as [`memory-attribution.md`](memory-attribution.md)
(Windows 11 10.0.26200, Intel UHD Graphics 770, two 1920x1080 displays at 100 % scale).

## 0. The question

The owner's data point: v0.6.3, one 1920x1080 display at 100 %, window maximized, no split
panes, two tabs each running the `claude` CLI for a long session, 10,000 lines of
scrollback. The status bar, which then showed commit, read **346 MB**. Phase 2's nearest
point was 1920x1040, 10,000 lines and 180 s of mimic: **276 MB**. About 70 MB was
unexplained. Three hypotheses were to be separated:

- **H1, session length.** Something grows slowly over tens of minutes.
- **H2, glyph diversity.** Real `claude` output has emoji, box drawing, braille spinners,
  Vietnamese text, attribute mixes and 24-bit colour, which the phase-2 mimic does not.
- **H3, what the mimic never did.** Alternate-screen switches, bracketed paste, window
  title and cursor-shape changes, focus reporting, and a resize (maximize after launch).

Raw rows: [`measurements.csv`](measurements.csv), labels starting with `s7-`. Load:
[`tui-mimic.py`](tui-mimic.py) `--rich --minutes N`. Protocol: `measure.ps1 -Mode S7`.

## 1. Answer in one paragraph

On v0.6.3 the maximized, rich, two-tab load reaches **298.7 MB** of commit after 20
minutes. That is 23 MB above phase 2, and it still leaves **47 MB** of the owner's 346 MB
unexplained. The maximized window explains almost nothing by itself (+3 MB). The rich
load and the first ten minutes explain the rest of the 23 MB, and the growth slows after
ten minutes (+3.7 MB from minute 10 to minute 20). A 40-minute run on main shows the
growth does not stop: after the first ten minutes commit keeps rising by 0.3 to 0.5 MB a
minute (2.7 to 5.2 MB per ten minutes), spread over several process-heap segments, with no
single structure growing. At that rate a session of about two hours would cover the
remaining 47 MB, but no run here was that long, and whether the growth is live heap or
fragmentation is not known. On main, with this week's fixes, the same 20-minute load reads
**111.5 MB** in the status bar (private working set, `US-0137`) and 195.1 MB of commit.

## 2. Build and protocol

- **Builds.** Main `d27f06ca` (includes `BUG-0078`, `US-0137`, `US-0141`), release
  profile, built in the worktree's own target directory. v0.6.3 is the owner's
  `dist/oneterm-x86_64-pc-windows-msvc/oneterm.exe`.
- **S7** (new `-Mode S7` in [`measure.ps1`](measure.ps1)):
  1. S1: launch at 1280x800 and idle 30 s.
  2. Open a second Command Prompt tab.
  3. Maximize with a posted `WM_SYSCOMMAND` / `SC_MAXIMIZE`, the path a title-bar
     double-click takes. The client area is then **1920x1032** physical pixels on both
     displays (the taskbar takes 48). With the default right dock open, the grid is 158
     columns.
  4. Run `tui-mimic.py --minutes N` in both tabs (with `--rich` for the rich runs),
     switching the visible tab every 10 s.
  5. Every 60 s, write a row `S7-t<min>`. At minutes 1, 10 and 20, write every private
     allocation with `vmregions.ps1 -Csv`.
- **`--rich`** adds, over the phase-2 mode:
  - glyphs: emoji (some with VS16), the rounded input box in box drawing, braille and star
    spinners, Vietnamese words with diacritics (30 % of words), tree and bullet marks;
  - styles: bold, dim, italic, underline, curly underline and strike mixes; 24-bit
    foreground from an 8-colour theme palette (20 % random); diff-style 24-bit backgrounds;
    underline colour;
  - modes: synchronized output around every frame; bracketed paste and focus reporting
    on; a short alternate-screen excursion every 3 s in the repaint phase; a window title
    (OSC 0) every second; a cursor shape (DECSCUSR) every 2 s; a bell every 60 s; an
    OSC 8 hyperlink with a new URL on every 50th burst line.
- **Concurrency.** To save time, two runs shared the machine: one on each display, both
  maximized and visible. Pairs: v0.6.3 rich 20 min with main rich 20 min; main rich 10 min
  with v0.6.3 plain 10 min. Main plain 10 min ran alone. The 40-minute run ran alone,
  after the physical displays had been disconnected (the session then had one
  1872x921 virtual display, see section 6).
- **One run per configuration**, plus a 10-minute repeat of main rich.

## 3. S7 time series

MB; private working set / commit (status bar `MEM`: commit on v0.6.3, private working set
on main). Two tabs, 10,000 lines, maximized 1920x1032.

**20-minute runs, rich load:**

| Minute | v0.6.3 | main |
| --- | --- | --- |
| S1 (1280x800, idle) | 49.4 / 182.3 | 49.4 / 110.5 |
| 1 | 116.5 / 281.4 | 96.2 / 179.3 |
| 5 | 136.0 / 289.7 | 103.0 / 186.3 |
| 10 | 150.5 / 295.0 | 109.2 / 192.6 |
| 15 | 155.3 / 297.7 | 109.2 / 193.3 |
| 20 | **157.6 / 298.7** | **111.5 / 195.1** |
| 20 s after the load ends | 157.8 / 298.8 | 111.1 / 194.6 |

**10-minute runs (the repeat and the plain controls):**

| Minute | main rich, repeat | main plain | v0.6.3 plain |
| --- | --- | --- | --- |
| 1 | 82.4 / 165.5 | 86.5 / 169.5 | 103.6 / 271.4 |
| 5 | 85.8 / 169.1 | 92.0 / 174.6 | 122.0 / 278.1 |
| 10 | 87.7 / 170.9 | 93.0 / 175.9 | 132.8 / 279.2 |

What the tables say:

- **Run-to-run spread is about 14 to 22 MB.** The two main rich runs differ by 13.8 MB of
  commit at minute 1 and 21.7 MB at minute 10. The walk shows why: the first run had one
  more ~13 MB heap segment from the first minute on. Heap high-water marks depend on how
  far the renderer fell behind a burst, which depends on timing.
- **H2 (rich against plain).** On main the plain control (175.9) sits between the two rich
  runs (170.9 and 192.6): no effect beyond the spread. On v0.6.3, rich is 15.8 MB of
  commit and 17.7 MB of private working set above plain at minute 10. Most of the private
  part is the v0.6.3 glyph table becoming resident (section 4), which `BUG-0078` removed.
- **H3 (maximize and modes).** v0.6.3 plain at 1920x1032 after 10 minutes is 279.2 MB,
  3.5 MB above phase 2's 1920x1040 after 3 minutes. Title, cursor-shape, bracketed-paste,
  focus and alternate-screen traffic add nothing measurable: the rich and plain runs of
  main agree within the spread.
- **H1 (session length).** Commit rises 13 to 16 MB in the first ten minutes and 2.7 to
  3.7 MB in the next ten. The growth slows, but it has not stopped at 20 minutes.

**The owner's number, decomposed** (v0.6.3, commit):

| Step | MB | Delta |
| --- | --- | --- |
| Phase 2: 1920x1040, plain, 3 min | 275.7 | |
| Maximized 1920x1032, plain, 10 min | 279.2 | +3.5 |
| Rich, 10 min | 295.0 | +15.8 |
| Rich, 20 min | 298.7 | +3.7 |
| Owner | 346 | **+47 unexplained** |

## 4. What grew between minute 1 and minute 20

`vmregions.ps1` walks, private committed allocations whose commit or resident size moved
by 0.5 MB or more (MB):

**main, rich, 20 min** (total commit 170.9 → 187.0, resident 94.0 → 109.6):

| Allocation | Commit | Resident | What it is |
| --- | --- | --- | --- |
| heap segment A | 10.2 → 14.8 (+4.6) | 8.1 → 12.7 | process heap, see below |
| new heap segment | 0 → 5.5 | 0 → 5.2 | process heap |
| heap segment | 8.8 → 12.9 (+4.2) | 8.8 → 12.8 | process heap |
| heap segments (two) | +1.2, +0.9 | same | process heap |

**v0.6.3, rich, 20 min** (total commit 272.8 → 290.6, resident 114.3 → 156.0):

| Allocation | Commit | Resident | What it is |
| --- | --- | --- | --- |
| heap segment | 0.4 → 8.6 (+8.2) | 0.3 → 6.9 | process heap |
| heap segment A | 10.2 → 15.6 (+5.3) | 8.1 → 13.1 | process heap, as on main |
| new heap segment | 0 → 4.3 | 0 → 4.3 | process heap |
| glyph table, tab 1 | 23.6 → 23.6 | **6.9 → 20.4** | `GlyphCache` table (`BUG-0078`, fixed on main) |
| glyph table, tab 2 | 23.6 → 23.6 | **7.0 → 20.0** | same |

Nothing outside the heap grew. No new large block appeared, and the window-sized blocks
(section 5) did not change after the maximize.

**Heap segment A** is the one allocation that grows in every run, on both builds, rich or
plain: 10.2 → 12.0 → 14.8 MB (main rich) and 10.2 → 12.0 → 15.6 MB (v0.6.3 rich) at
minutes 1, 10, 20; +2.8 MB in 6 to 10 minutes in each plain run. A dump of it from a
held run (6 minutes of plain load, 12.1 MB committed):

- 49 % of its pages are all zero, and they are resident: freed or zero-initialised
  blocks that the heap keeps committed;
- 0.125 MB is raw terminal text (burst lines with their escape sequences, OSC 7);
- the rest is binary heap data.

It is a segment of the process heap that the Rust allocator (`HeapAlloc`) uses, not a
buffer of its own. Which allocations live in it cannot be read from a walk; that needs
the allocator probe of phase 2 (section 7).

**The 2.1 MB segment** that appeared in the same held run holds high-entropy data, and
its size, 2.1 MiB, is the style interner's hash-table rehash that phase 2's allocation log
recorded (2,228,240 B). The mimic's random 24-bit colours fill a tab's style table; see the
bound below.

## 5. The window-size blocks (H3, resize)

| State | Window-sized allocations (commit / resident MB) |
| --- | --- |
| S1, 1280x800, main idle | 28.9 / 7.9 (one allocation, two regions) |
| Maximized 1920x1032 | 32.3 / 0.0 and 7.8 / 0.0 (two allocations); the 28.9 block is gone |

The code (`gpui-pre-windows` 0.3.3, `directx_renderer.rs`):

- `recreate_resources` creates, per window, a path intermediate texture of W×H×4 bytes and
  a 4× multisampled texture of W×H×4×4 bytes (`PATH_MULTISAMPLE_COUNT = 4`). At 1920x1032
  that is 7.6 MiB and 30.2 MiB; with row alignment they are the 7.8 and 32.3 MB blocks.
- The three swap-chain buffers (`BUFFER_COUNT = 3`) are owned by DXGI and composition, and
  do not show as private committed memory of the process.
- `resize` drops and recreates both textures. The walk confirms the 1280x800 block is
  freed: a resize leaves nothing behind.
- They are GPU textures in shared system memory on this integrated GPU. They count
  against commit and are never in the private working set.

So the maximized window costs about **+11 MB of commit and 0 of private working set**
over 1280x800, per window, not per tab. Phase 2's "15.7 MB frame-sized block" is not
frame-sized: a 15.0 to 15.7 MB allocation of six regions, 0.6 to 1.2 MB resident, is
present at every window size.

## 6. The 40-minute run

Main, rich, two tabs, 10,000 lines, 40 minutes, run alone. The physical displays had been
disconnected by then, so the session had one 1872x921 virtual display and the maximized
client was **1872x873**. The slope, not the level, is what this run is for.

| Minute | 1 | 5 | 10 | 15 | 20 | 25 | 30 | 35 | 40 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Private WS | 92.8 | 98.2 | 104.2 | 106.7 | 108.6 | 112.9 | 112.5 | 114.8 | 115.9 |
| Commit | 169.7 | 176.1 | 181.7 | 184.7 | 186.5 | 190.4 | 191.7 | 193.6 | 194.4 |

Commit per ten minutes: +12.0, +4.8, +5.2, +2.7. The walks at minutes 1, 10, 20, 30 and 40
(total private commit 161.6 → 172.9 → 178.1 → 183.3 → 185.9):

| Allocation | t1 | t10 | t20 | t30 | t40 |
| --- | --- | --- | --- | --- | --- |
| heap segment, new after t1 | 0 | 4.3 | 4.3 | 10.4 | 10.4 |
| heap segment | 6.3 | 9.1 | 8.9 | 9.2 | 11.2 |
| heap segment A | 10.2 | 12.0 | 15.6 | 14.5 | 14.5 |
| heap segment | 12.2 | 13.5 | 14.5 | 14.5 | 14.8 |
| heap segment | 13.7 | 13.9 | 14.8 | 14.6 | 15.0 |

(commit MB; resident follows commit within 0.5 to 2.6 MB in every row.)

On this virtual display the 4× path texture (25.0 MB at 1872x873) was 17.5 MB
**resident**, where on the physical displays it was never resident. The disconnected
session probably renders through a software adapter. This does not change the slope.

- The growth does not stop within 40 minutes, but it slows: 0.5 MB a minute between minute
  20 and 30, 0.27 between 30 and 40.
- It is spread over five process-heap segments, and it moves between them: segment A
  **shrank** by 1.1 MB between minute 20 and 30 while a new segment grew by 6.1 MB.
  A leak in one structure would grow in place. This looks like the heap's high-water mark
  creeping as blocks of changing sizes are freed and reallocated. That is an inference;
  only a live-bytes counter in the allocator can tell fragmentation (commit grows, live
  bytes flat) from a leak (both grow).
- Extrapolated at 0.3 to 0.5 MB a minute, the owner's missing 47 MB is 1.5 to 2.5 hours of
  this load. The owner described "a long session".

## 7. Attribution: what can grow, and its bound

Read from the code on main. "Per tab" means per terminal; a split pane is a terminal.

| Structure | Where | Bound | Per | Worst case | Reached by S7? |
| --- | --- | --- | --- | --- | --- |
| Style interner (`InternTable<Style>`) | `crates/vt/src/intern.rs` | 65,535 ids, then id 0 (default colours) for the life of the terminal | tab | about 3 MB (2.1 MiB table + 1 MiB entry vector) | Yes: random 24-bit colours fill it in minutes. `claude` uses a small palette |
| Extras (`InternTable<Extras>`) | same | 65,535 ids, never swept | tab | about 2.5 MB | No |
| Hyperlinks (`HyperlinkTable`) | same | 65,535 links, then links render as plain text; cleared only by `RIS` | tab | about 14 MB (each link keeps id and URI twice: entry and key) | No: about 8,000 links per tab in 20 minutes |
| Grapheme arena | same | swept at 65,536 entries or 1 Mi codepoints | tab | a few MB | No |
| Title stack | `crates/vt/src/terminal/mode.rs` | 16 | tab | small | No |
| Glyph cache | `crates/terminal-view/src/render/glyphs.rs` | 4,096 entries, then entries unused for 3 frames are dropped | view | 4,096 shaped runs (`Arc<LineLayout>`) | Yes, in v0.6.3's table (resident 20 MB of 23.6) |
| Notification queue | `crates/terminal-view/src/terminal_view/view.rs` | `MAX_QUEUED_NOTIFICATIONS` | view | small | No |
| Gutter timestamps | `crates/terminal-view/src/terminal_view/gutter_timestamps.rs` | popped with the rows | view | one `u32` per row | Yes |
| Image store | `crates/terminal-view/src/render/graphics.rs` | 64 images, 64 MiB | view | 64 MiB | No |
| Contrast cache | `crates/terminal-view/src/theme/contrast.rs` | 4,096 entries | thread | small | Yes |
| gpui glyph atlas | `gpui-pre-windows` `directx_atlas.rs` | 1024x1024 pages, glyph tiles never evicted; bounded by distinct glyphs × sizes × subpixel variants | process | a few pages (1 MiB mono, 4 MiB colour emoji) | GPU memory, not in the walk |
| gpui line layout cache | `gpui-pre` text system | swapped every frame | window | one frame of lines | Yes |
| Window textures | `directx_renderer.rs` | W×H×20 bytes | window | 38 MB at 1920x1032 | Yes, commit only |

None of these grows without a bound. The one that can cost real memory with `claude`
is the hyperlink table, and only if `claude` emits links:

- `claude` wraps URLs in OSC 8 **without an `id=`**, and only when it decides the terminal
  supports hyperlinks. The check (read from the installed CLI's bundled JavaScript)
  accepts a known `TERM_PROGRAM` list, `WT_SESSION`, JetBrains, kitty and similar.
  OneTerm sets `TERM_PROGRAM=OneTerm`, which is not in the list. But a OneTerm started
  from a Windows Terminal shell inherits `WT_SESSION`, and then links are on.
- `HyperlinkTable::intern` gives every link without `id=` a new entry. A TUI that repaints
  a line holding a link, as Ink does, adds one entry per repaint. At 30 repaints a second
  that fills the table in about 36 minutes: about 14 MB plus 2.5 MB of extras per tab,
  after which every link in that terminal is plain text.

This is an estimate from the entry layout, not a measurement.

## 8. S8 (the real `claude` CLI): skipped

`claude` is installed (`~/.bun/bin/claude.exe`). S8 was not run:

- The protocol launches OneTerm with a private `%USERPROFILE%`, so `claude` inside it has
  no credentials. It would show first-run onboarding, not the idle prompt.
- Pointing it at the real profile would make `claude` write `~/.claude.json` and project
  state outside the worktree, which this task must not do.

## 9. Conclusions

- **H3, the maximized window:** +11 MB of commit over 1280x800 (GPU path textures), 0
  private working set, and +3.5 MB over phase 2's 1920x1040. Closed: it does not explain
  the gap.
- **H2, glyph diversity:** on v0.6.3, +15.8 MB of commit and +17.7 MB of private working
  set, most of it the glyph table that `BUG-0078` removed. On main, within the run-to-run
  spread.
- **H1, session length:** real. Commit keeps rising after the first ten minutes, by 0.3 to
  0.5 MB a minute through minute 40, spread over the process heap. Every structure OneTerm
  owns that could grow has a bound (section 7), and none of them is near it.
- **Remaining 47 MB at v0.6.3:** not reproduced in 20 minutes. The best candidate is H1
  over 1.5 to 2.5 hours. Others: bursts larger than the mimic's (heap high-water marks
  differ by up to 22 MB between identical runs), and hyperlinks (only if the owner's
  OneTerm inherits `WT_SESSION`).
- **What the owner sees now:** the same load on main reads 111.5 MB in the status bar
  after 20 minutes (115.9 MB after 40), against 346 MB before.

## 10. Proposed next steps

| Item | Kind | Expected saving | Effort | Risk |
| --- | --- | --- | --- | --- |
| **Phase 4: a two-hour S7 on a probe build** that logs the allocator's live bytes (phase 2's throwaway probe in `crates/app/src/oom.rs`) next to commit. Live bytes flat while commit grows means fragmentation; live bytes growing means a leak, and the probe's large-allocation log names it | research in this intake | none by itself; decides the next row | S (one build, two hours of machine time) | none (probe not committed) |
| If fragmentation: try a global allocator that returns memory (for example mimalloc) behind a measurement | packet, only after phase 4 | unknown; the growth measured here is 0.3 to 0.5 MB a minute | M (new dependency, `cargo deny`, notices) | medium |
| If a leak: a `BUG` for the structure the probe names | packet, only after phase 4 | the leak's rate | depends | depends |
| **`BUG-0079` candidate: implicit OSC 8 links are re-interned on every repaint.** Give an un-`id=`-ed link the same entry when the same URI is reopened on the same row run, or sweep unreferenced hyperlink and extras ids the way the grapheme arena is swept | packet (engine, `oneterm-vt`) | up to about 16 MB per tab, and links that keep working after 65,535 repaints; only for programs that emit OSC 8 without `id=` (`claude` when it sees `WT_SESSION`) | M | medium: `HyperlinkId` identity is visible to embedders; needs an LLD note first |
| Window path textures created on first use | upstream `gpui` issue, or a vendor patch | 38 MB of commit at 1920x1032, 0 private working set | S to M | medium (vendored renderer) |

The first row is the one worth doing now: it decides between two different fixes and
costs no code. The hyperlink row should start with a failing `oneterm-vt` test that
repaints one link 70,000 times and checks the table size and that the last link resolves.
The texture row saves nothing the status bar shows since `US-0137`.
