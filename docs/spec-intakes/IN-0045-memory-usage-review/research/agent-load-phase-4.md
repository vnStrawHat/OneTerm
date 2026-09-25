# IN-0045 research, phase 4: fragmentation or a leak

Date: 2026-09-25. Same machine and toolchain as [`agent-load-phase-3.md`](agent-load-phase-3.md)
(Windows 11 10.0.26200, two 1920x1080 displays at 100 %; this time both were connected, so
the maximized client was **1920x1032**, as in phase 3's 20-minute runs).

## 0. The question

Phase 3 § 6: under the rich two-tab load, commit keeps rising by 0.3 to 0.5 MB a minute for
40+ minutes, spread over several process-heap segments. Fragmentation (commit grows, the
allocator's live bytes stay flat) or a leak (both grow)? The owner's 346 MB session still
had 47 MB unexplained.

## 1. Answer in one paragraph

**A leak in one structure, not fragmentation.** Over 120 minutes the allocator's live bytes
rise by 43.8 MB and commit by 50.4 MB; commit minus live bytes moves by only 6.6 MB, and
that is the heap's per-block overhead on the half-million small blocks the leak adds. The
structure is the VT engine's per-terminal **hyperlink table and extras table**: every OSC 8
link without `id=` gets a new entry in both, and the rich mimic emits one every 50th burst
line. Both tables reach their 65,535-entry bound after about **110 minutes**; from then on
live bytes and commit are flat (84.5 / 229.5 MB at minutes 110 and 120), and every later
link, **and every later Sixel image**, in those terminals is dropped. At the bound each
terminal holds 21.0 MB (measured headless), so two tabs hold 42 MB: close to the owner's 47
MB, **if** the owner's `claude` emitted OSC 8 links (it does when it sees an inherited
`WT_SESSION`, phase 3 § 7). Without links nothing else grows: every other size class is flat
after minute 2. `BUG-0079` is real and is now written; no allocator change is warranted.

## 2. The probe build

Throwaway, **not committed**. Main `0d09b584` with `crates/app/src/oom.rs` changed so that
`OomResilientAlloc` also:

- keeps relaxed atomic counters of live bytes and live allocations per size class (0-64,
  65-512, 513-4K, 4K-64K, above 64K bytes; `realloc` moves a block between classes) and a
  running allocation total;
- when `ONETERM_ALLOC_LOG` is set, writes them to that file every 30 s from its own thread;
- when `ONETERM_ALLOC_BT=<class>` is set, samples blocks of that class by address hash
  (`ONETERM_ALLOC_BT_RATE`, default 1 in 1,000), captures a `std::backtrace` on allocation,
  drops the sample on free, and every 5 minutes writes the live samples aggregated by their
  first six non-`std` frames. A thread-local guard keeps the probe's own allocations out.

The OOM ballast is allocated from `System` directly, so live bytes exclude it.

Build: release profile with `debug = "line-tables-only"`, `strip = "none"` and **thin** LTO
(env overrides). Fat LTO with line tables ran `rustc` out of memory twice on the final
link. Only the allocator and code layout differ from the shipped profile.

The counters do not move the process counters measurably: S1 on the probe build is 49.2 /
110.4 MB (private working set / commit), phase 3's main S1 was 49.4 / 110.5. The
**backtrace** runs do: resolving symbols makes dbghelp load the 131 MB PDB, and those runs
read 340 to 350 MB of commit at minute 30. They are used for attribution only.

Protocol: `measure.ps1 -Mode S7 -Rich -AllocLog` (this is "S9"; `-AllocLog` adds `live_mb`
and `live_count` columns and writes to [`measurements-phase4.csv`](measurements-phase4.csv)).
Raw probe logs: [`phase4-alloc/s9-120min.csv`](phase4-alloc/s9-120min.csv) and
[`phase4-alloc/osc8-5min.csv`](phase4-alloc/osc8-5min.csv) (`t_s` counts from process start;
the load starts about 45 s later).

## 3. S9: two tabs, rich load, 120 minutes

Probe build, two tabs, `tui-mimic.py --rich --minutes 120`, maximized 1920x1032, 10,000
lines of history, run alone. MB = 2^20 B.

| Minute | Private WS | Commit | Live bytes | Live count | Commit - live | Commit / live |
| --- | --- | --- | --- | --- | --- | --- |
| S1 (idle, 1280x800) | 49.2 | 110.4 | 7.1 | 5,563 | 103.3 | 15.6 |
| 1 | 96.0 | 179.2 | 40.6 | 38,225 | 138.6 | 4.41 |
| 10 | 105.9 | 189.6 | 49.7 | 86,145 | 139.9 | 3.81 |
| 20 | 109.3 | 193.3 | 52.8 | 134,142 | 140.5 | 3.66 |
| **30** | 114.4 | **199.6** | **58.7** | **182,142** | 140.9 | 3.40 |
| 40 | 80.3 | 201.3 | 59.6 | 226,946 | 141.7 | 3.38 |
| 50 | 61.0 | 207.2 | 65.3 | 277,342 | 141.9 | 3.17 |
| **60** | 63.2 | **212.5** | **70.2** | **322,145** | 142.3 | 3.03 |
| 70 | 65.6 | 214.1 | 71.3 | 372,542 | 142.8 | 3.00 |
| 80 | 68.0 | 215.8 | 72.3 | 420,539 | 143.5 | 2.98 |
| **90** | 70.2 | **217.3** | **73.2** | **465,348** | 144.1 | 2.97 |
| 100 | 81.4 | 227.8 | 83.5 | 513,345 | 144.3 | 2.73 |
| 110 | 83.8 | 229.5 | 84.5 | 560,545 | 145.0 | 2.72 |
| **120** | 83.9 | **229.6** | **84.4** | **563,157** | 145.2 | 2.72 |
| 20 s after the load | 83.6 | 229.3 | 84.4 | 563,157 | 144.9 | 2.72 |

What the table says:

- **Live bytes follow commit.** Minute 1 to 120: commit +50.4 MB, live bytes +43.8 MB, live
  allocations +525,000. The gap (commit - live) grows by 6.6 MB, 2.3 of it in the first 30
  minutes. The Windows heap keeps a header of 8 to 16 bytes per block plus rounding, and
  525,000 new blocks account for all of it. There is no fragmentation to speak of: a
  fragmenting heap would show the gap growing while live bytes stay flat.
- **The growth stops by itself** at about minute 110, when both tabs' tables are full.
  `stderr.log` of the run: `extras table is full at 65535 entries` and `hyperlink table is
  full` for both terminals at 12:30:52Z and 12:31:04Z, 110 minutes into the load. Minute 110
  to 120: live bytes -0.1 MB, commit +0.1 MB.
- **The steps** (minute 30 to 40 flat, then +5.9 at 50, +9.4 at 100) are table doublings: a
  `Vec` or hash map of a few MB is reallocated at twice its size and the old block freed.
  This is why phase 3 saw the growth "move between heap segments": a doubled table lands in
  a new segment, and its old block leaves a hole that later small blocks fill.
- **Private working set** fell from 114 to 61 MB between minutes 30 and 50 while commit kept
  rising: the working set was trimmed (other builds were running on the machine then). It
  climbs back as pages are touched. It does not bear on the question.
- **The first minute** (+33.5 MB live) is the history filling: the 513-4K class goes to 27.1
  MB (21,184 rows of about 1.3 KB, two full 10,000-line histories) and stays there for two
  hours.

**Per size class** (live MB, from the probe log):

| Minute (t_s) | 0-64 B | 65-512 B | 513 B-4K | 4K-64K | above 64K |
| --- | --- | --- | --- | --- | --- |
| 10 (600) | 1.2 (50,693) | 1.2 (9,183) | 26.0 (21,184) | 2.8 (246) | 18.3 (36) |
| 60 (3600) | 6.2 (289,094) | 1.2 (9,185) | 26.0 (21,184) | 2.8 (246) | 33.9 (36) |
| 120 (7200) | 11.3 (532,573) | 1.2 (9,183) | 26.0 (21,184) | 2.8 (246) | 43.2 (36) |

(count in brackets). Three classes are flat for two hours to the allocation. The two that
grow are 0-64 B (about 2,400 new blocks every 30 s, four per link) and above 64K (constant
count, growing by reallocation).

**Region walks.** The `vmregions.ps1 -Csv` walks at minutes 1, 60 and 120 **were not
written**: `pwsh -File measure.ps1 -VmAt 1,30,60,90,120` hands PowerShell the string
`1,30,60,90,120`, which it converts to the single integer 13060, so no minute matched.
`measure.ps1` now takes `-VmAt` as a comma-separated string. The walks at minutes 1 and 30
of the two attribution runs (section 4) exist, but those runs carry the dbghelp overhead
and are not comparable. The walks are not needed for the verdict: the live-byte counter
answers what they could only suggest.

## 4. Attribution: which structure

Two more 30-minute runs of the same load, concurrent, one per display (attribution only):

- **0-64 B, 1 sample in 1,000.** Live samples at 4.5 minutes and at 29.5 minutes:

  | Stack (first two frames past the allocator) | 4.5 min | 29.5 min |
  | --- | --- | --- |
  | `oneterm_vt::intern::HyperlinkTable::intern` <- `dispatch::Handler::set_hyperlink` | 13 samples | **133 samples** |
  | everything else (gpui input state, a regex, the product name) | 3 | 5 |

- **Above 64K, every block.** Live bytes by stack, same two moments:

  | Stack | 4.5 min | 29.5 min | Bound |
  | --- | --- | --- | --- |
  | `HyperlinkTable::intern` (entry vector and index map) | 0.5 MB | **5.3 MB** | 65,535 links |
  | `Interner::extras` from `set_hyperlink` | 0.3 MB | **3.7 MB** | 65,535 entries |
  | `Interner::style` from `sgr` | 4.1 MB | 6.3 MB | 65,535 styles, full in minutes (random 24-bit colours) |
  | `GlyphCache::shape` | 0.2 MB | 0.8 MB | 4,096 entries |
  | PTY pipe ring, local-shell read buffer, grid screens, channels | flat | flat | fixed |

Everything that grows goes through `set_hyperlink`. The style table grows too, but only
until it is full, which the probe log shows well before minute 10.

## 5. OSC 8 measured directly (`BUG-0079`)

### 5.1 Headless, to the bound

A throwaway `oneterm-vt` example (release, not committed) with a counting allocator feeds one
terminal (158 x 40) the repaint `tui-mimic.py --link-repaint` sends: cursor up 3 rows, then
three lines, the middle one holding `OSC 8 ; ; https://docs.anthropic.com/en/docs/claude-code`
around the word `docs`, with no `id=` and the same URI every time.

| Repaints | Hyperlink entries | Extras entries | Live bytes added |
| --- | --- | --- | --- |
| 5,000 | 5,000 | 5,001 | 2.0 MB |
| 9,000 (5 min at 30/s) | 9,000 | 9,001 | 3.6 MB |
| 30,000 | 30,000 | 30,001 | 10.4 MB |
| 65,534 | 65,534 | 65,535 (full) | 21.0 MB |
| 65,535 | 65,535 (full) | 65,535 | 21.0 MB |
| 70,000 | 65,535 | 65,535 | 21.0 MB |

- **One entry per repaint in both tables.** About 102 B per link in the steady state plus the
  doublings of the entry vector and the index maps: 320 B per link on average, **21.0 MB per
  terminal** at the bound. Phase 3's estimate was 14 MB plus 2.5 MB of extras.
- **Links stop resolving.** After 1,000 repaints the link's 4 cells resolve. After 70,000
  the same line has **0 linked cells**: from the 65,535th repaint on, the extras table (full
  one step before the hyperlink table, since its id 0 is the default) drops the attribute.
- **The damage is wider than links.** After the fill, a new link with an explicit `id=` gets
  0 linked cells, and a Sixel image gets **0 image cells**; the same Sixel in a fresh
  terminal gets its cells. A full extras table disables everything that needs an extras
  entry, for the life of the terminal. `RIS` clears the hyperlink table but not the extras
  table (read from `dispatch.rs`, not run), so new images do not come back even then.

### 5.2 Live, two tabs, 5 minutes

Probe build, two tabs, `tui-mimic.py --link-repaint --minutes 5` in both, maximized
1920x1032, run alone:

| Minute | Private WS | Commit | Live bytes | Live count |
| --- | --- | --- | --- | --- |
| S1 | 49.2 | 110.3 | 7.1 | 5,545 |
| 1 | 56.1 | 137.9 | 13.1 | 27,580 |
| 3 | 59.6 | 141.6 | 15.4 | 58,931 |
| 5 | 61.8 | 144.4 | 18.4 | 88,253 |
| end | 62.3 | 144.9 | 18.4 | 88,336 |

- 0-64 B blocks rise by about 7,450 every 30 s: four per link, 31 links a second per tab.
  Over the 5 minutes: about 9,400 links per tab, live bytes +7.4 MB (with two doublings in
  the above-64K class), commit +6.5 MB from minute 1 to 5.
- At 31 repaints a second a tab reaches the bound in **35 minutes**. The table did not fill
  within 5 minutes, so no warning was logged; section 5.1 shows what happens when it does,
  and S9 hit it in the app itself (section 3).

**Verdict: `BUG-0079` is real.** Written as
[`BUG-0079`](../BUG-0079-implicit-osc8-links-reinterned-on-every-repaint.md). No code change
in this phase.

## 6. What it means for the owner's 346 MB

- The creep phase 3 measured was **the mimic's own OSC 8 links** (one per 50 burst lines,
  about 10 a second per tab), bounded at 21 MB per tab. It is not a general high-water creep.
- The same mechanism explains the remaining 47 MB **only if** the owner's `claude` emitted
  OSC 8 links, which it does when it believes the terminal supports them (an inherited
  `WT_SESSION`, phase 3 § 7). Two tabs at the bound add about 42 MB of live heap and 50 MB
  of commit, in 35 minutes to 2 hours depending on how often links are repainted.
- If the owner's `claude` did not emit links, phase 4 found nothing else that grows: after
  the first two minutes the other size classes are flat to the allocation for two hours,
  and heap overhead added 4.3 MB in the last 90 minutes. The 47 MB would then be the burst
  high-water spread of phase 3 § 3 (14 to 22 MB between identical runs) plus whatever the
  real CLI does that the mimic does not.
- The open question to the owner stays the useful one: was OneTerm started from a Windows
  Terminal shell? A visible symptom would also answer it: after a long session, `claude`'s
  links stop being clickable, and images stop drawing, in that tab.

## 7. A global allocator (candidate `DEC-0021`): not now

Phase 3 § 10 made a global-allocator trial conditional on fragmentation. The condition is
not met: commit minus live bytes grew 6.6 MB in two hours, most of it block headers. For the
record, the assessment:

- **Dependency policy** ([`docs/agents/dependencies.md`](../../../agents/dependencies.md) § 3):
  `mimalloc` (MIT) is not in the graph; it would add `mimalloc` and `libmimalloc-sys` (a C
  build through `cc`, which is already in `Cargo.lock`), a row in the auxiliary-crates table,
  `deny.toml` and `THIRD-PARTY-NOTICES.md` updates, and a C toolchain on every CI runner
  (present today for `ring`).
- **Coexistence with the OOM ballast.** `OomResilientAlloc` wraps `System`; it would wrap
  `mimalloc::MiMalloc` instead, with the same retry loop. The ballast would come from
  mimalloc's large-object path (a direct OS allocation), so freeing it still returns commit
  to the system. The OOM path needs a new test: mimalloc returns NULL on commit failure
  rather than aborting, which is what the wrapper needs, but that has to be proved on
  Windows.
- **Expected effect.** On this load: at most the 6.6 MB of overhead growth, less its own
  per-segment reservations, which are larger than the NT heap's. It would not touch the 44
  MB of live growth, which is data the engine keeps on purpose.
- **Risk.** Medium: a process-wide change under gpui, DirectWrite callbacks and the ConPTY
  threads, measured only by a long run.

Reopen only if a future long run shows commit - live growing by more than about 10 MB an
hour with live bytes flat.

## 8. Conclusions

- **Leak, not fragmentation:** commit +50.4 MB, live bytes +43.8 MB, commit - live +6.6 MB
  over two hours; commit / live falls from 4.41 to 2.72 because the leak is live data.
- **The structure:** `oneterm_vt` `HyperlinkTable` and `ExtrasTable`, via
  `Handler::set_hyperlink`, one entry each per OSC 8 link without `id=`. Bounded at 65,535:
  21.0 MB per terminal, reached in 110 minutes by the rich mimic and in 35 minutes by a
  30-per-second link repaint.
- **After the bound**, links and Sixel images stop working in that terminal until it is
  closed. That is a functional bug, not only a memory one.
- **Next packet:** `BUG-0079` (engine, `oneterm-vt`; LLD note first, then a failing test that
  repaints one link 70,000 times). No `DEC-0021`.
