# Work: Glyph cache preallocates a 24 MB table per terminal view

ID: BUG-0078
Intake: IN-0045
Created: 2026-09-25

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug (shipped behavior, memory regression since v0.5.0)
- Risk lane: normal. Render-internal; no public contract, no persisted data.
- Spec Intake, when required: `IN-0045` (measurement and attribution). The cache itself came
  from `US-0047` of `IN-0018` (`d21c8cad`).

## Reported by

- `IN-0045` measurement, 2026-09-25: 23.6 MiB of commit per tab and per pane at idle, resident
  once output fills the table.
- Owner data point, 2026-09-25: two tabs running the `claude` TUI, 10,000-line scrollback,
  status bar `MEM` (commit) 346 MB. That is about 140 MB above the two-tab estimate. The
  hypothesis was that the table doubles under a chatty TUI (16384 buckets = 49 MB, 32768 = 99 MB
  per view). It was tested here (Evidence, "TUI load") and **not reproduced**: under the load the
  map stays at 4096 entries and 8192 buckets, in both window sizes tried.

## Root cause

Measured in [`research/memory-attribution.md` § 4.1](research/memory-attribution.md).
`GlyphCache::new` (`crates/terminal-view/src/render/glyphs.rs:111` on `main` @898148e5) calls
`HashMap::with_capacity(4096)`. hashbrown rounds that to 8192 buckets. Each bucket is a
`(RunKey, Entry)` of 3,024 bytes, because `Entry` holds gpui's `ShapedLine` by value and its
`SmallVec<[DecorationRun; 32]>` is inline. The result is one allocation of 24,780,816 bytes per
`TerminalView`, so per tab and per split pane. It is committed at creation and becomes resident
once output fills it.

The terminal painter never reads a `ShapedLine`'s decoration runs or text. It paints
`LineLayout::runs` with the plan's own color spans (`render/element.rs` `paint_shaped_line`,
`render/cursor.rs` `CursorPaint::paint`), and the other readers use only `width` and `len`.

## Outcome

- `GlyphCache` starts empty and grows on demand.
- It stores and returns gpui's `Arc<LineLayout>` from `WindowTextSystem::layout_line_by_hash`.
  That is the same `Arc` gpui's own line-layout cache holds, and the one `shape_line_by_hash`
  wraps. A bucket is 48 bytes, and a hit is one hash lookup plus an `Arc` bump. It used to be
  that plus a 3 KB `SmallVec` copy.
- Hit semantics are unchanged: the same key, the same shaping call and the same arguments.
- The eviction policy is unchanged. There is a soft cap of `CAPACITY = 4096` entries. When an
  insert finds the map at the cap, entries unused for `GRACE = 2` generations are dropped
  first. The map grows past 4096 only while more distinct runs than that were shaped in the
  last three frames. That is bounded by the viewport, not by time, so **there is no unbounded
  growth and no new bound was added**. Under the TUI load below, the maximum was 4096 entries.
- Paint output is unchanged: the same glyphs at the same positions in the same colors.

## Scope

- [x] In scope: `render/glyphs.rs` and the three holders of its result (`TextRunPlan.line`,
  `GutterLabel.line`, `CursorPaint.glyph`), plus the painter's parameter type.
- [x] In scope: a `Tui` mode in `research/measure.ps1` (plus `research/tui_mimic.py`) so that
  the owner's TUI hypothesis can be re-measured.
- [x] Out of scope: one cache per window instead of per view; the rest of `IN-0045`
  (`US-0137..0140`, `DEC-0020`); the window-size-dependent 32 MB region (Gaps).

## Acceptance

- [x] `GlyphCache::new` allocates nothing (`map.capacity() == 0`, unit test).
- [x] A cache filled to `CAPACITY` holds its table plus the per-entry `LineLayout` headers in
  under 1 MiB: 8192 buckets × 48 B + control bytes = 401,408 B, plus 4096 × 64 B = 262,144 B,
  about 0.63 MiB in total. This is computed from `size_of` in the unit test. The glyph vectors
  are gpui's shaped data, shared with gpui's own cache, and are not counted.
- [x] The same key returns the same `Arc` (pointer-equal), and a miss shapes once.
- [x] Existing terminal-view tests pass unchanged (eviction, idle frame allocates nothing,
  paint tests).
- [x] `measure.ps1` S1 commit drops by 24 MB, to 157 MB (v0.4.2 was 159). S2 adds 3.1 MB of
  commit per tab, under the intake's 4 MB stop condition. S3 private WS drops by 21.6 MB.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`
  — § Row plan, § Glyph cache, § Cursor and the allocation plan. They describe `ShapedLine`
  as the cached value, and a clone as "an `Arc` bump plus an inline `SmallVec` copy".
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` — the module
  table (`glyphs.rs`) and the render state table (`glyphs`, `gutter`).
- `docs/spec-intakes/IN-0045-memory-usage-review/high-level-design.md` — the budget row "Glyph
  cache: table ≤ 1 MB up front; entries ≤ 4096". The fix meets it (0 up front, about 0.63 MiB
  full), so no change is needed.
- `docs/gui-layout.md` and `docs/terminal-backend.md` do not mention glyph cache internals
  (`terminal-backend.md` only lists `render/` in the tree). No change.

### Documentation Action

Update required: the IN-0018 LLD and HLD lines that name `ShapedLine` as the cached and
planned value change to `Arc<LineLayout>`. The allocation-plan sentence about a clone changes
too, and the LLD records the soft-cap bound.

Reason: the cached value's type is part of the documented design.

### Reconciliation

Changed:

- `IN-0018/high-level-design.md`: three rows.
- `IN-0018/low-level-design/render-pipeline.md`: the row plan, shape step, glyph cache block,
  a new paragraph on the value and the bound, the cursor struct and the allocation plan.
- `IN-0045/research/measure.ps1`: the `Tui` mode, `-W`/`-H` and the `T1`/`T2` header lines.
- `IN-0045/research/tui_mimic.py`: new file.
- `IN-0045/research/measurements.csv`: this packet's rows, `note = BUG-0078`.

The IN-0045 HLD budget row needs no change.

## Context

- In gpui-pre 0.3.3, `WindowTextSystem::shape_line_by_hash` is `layout_line_by_hash(...)` plus a
  one-run `SmallVec` of decoration runs and an empty `text`. `layout_line_by_hash` is public and
  returns `Arc<LineLayout>`, and its fields (`runs`, `width`, `len`) are public.
- hashbrown never shrinks after `retain`, so a table keeps its peak size. That is why the bucket
  size matters even under the soft cap.

## Plan

- [x] Build the release binary on `main` and measure it (before).
- [x] Change `GlyphCache` to `HashMap::new()` and `Arc<LineLayout>`; adapt the holders.
- [x] Add the size/identity test.
- [x] Rebuild, measure (after), and run the frame-time measurement before and after.
- [x] Measure the owner's TUI hypothesis (the `Tui` mode) before and after, with a temporary
  entry-count and hit-rate log (not committed).
- [x] Update the IN-0018 docs and run the gates.

## Decisions

None.

## Verification Plan

- Unit: the new `glyph_cache_full_table_stays_small_and_hits_share_the_layout` in `glyphs.rs`,
  and the existing `glyph_cache_evicts_stale_generation` and
  `element_tests::idle_frame_allocates_nothing`.
- Frame time: `element_tests::frame_time_under_output` (an ignored measurement), before and
  after.
- Integration: `research/measure.ps1 -Mode Full` and `-Mode Tui` on release builds, 2 runs each
  before and after.
- Gates: fmt, clippy, `cargo test -p oneterm-terminal-view`, `check-doc-paths.py`,
  `check-english.py`, and the full `scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=4`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Setup: release builds from this worktree. `before` is `main` @898148e5 and `after` is this
branch. Windows 11, and `measure.ps1` with a private `USERPROFILE`. MB = 2^20 B. Raw rows are
in [`research/measurements.csv`](research/measurements.csv) (labels `bug0078-*`).

### Entry size

| | Bucket `(RunKey, Entry)` | Table up front | Table at 4096 entries |
| --- | --- | --- | --- |
| before | 3,024 B | 24,780,816 B (8192 buckets) | the same 24.8 MB |
| after | 48 B | 0 | 401,408 B + 262,144 B of `LineLayout` headers |

### S1 / S2 / S3 (`-Mode Full`, 1280x800)

privws / commit, MB. Before runs 1-2 are the quiet re-runs (`bug0078-before-q`).

| Scenario | before r1 | before r2 | after r2 | after r3 | Δ (mean) |
| --- | --- | --- | --- | --- | --- |
| S1, 1 tab idle | 48.1 / 180.8 | 48.6 / 181.4 | 48.0 / 157.0 | 48.5 / 157.4 | −0.1 / **−23.9** |
| S2, 3 tabs idle | 52.1 / 234.3 | 52.8 / 235.1 | 52.0 / 163.2 | 52.5 / 163.8 | −0.2 / **−71.2** |
| S3, one of 3 full | 82.9 / 243.9 | 84.2 / 244.9 | 62.7 / 173.9 | 61.2 / 172.3 | **−21.6** / −71.3 |
| S4, back to 1 | 49.9 / 182.5 | 51.1 / 183.8 | 50.7 / 159.7 | 49.8 / 158.8 | −0.3 / −23.9 |
| S5, +2 min | 50.5 / 183.2 | 51.1 / 183.8 | 50.9 / 160.0 | 50.2 / 159.1 | −0.3 / −24.0 |

- **S2 commit per added tab:** before 26.8 MB (26.75, 26.85); after 3.1 MB (3.1, 3.2).
- **After run 1** was excluded as a first-launch outlier: S1 58.4 / 167.4, S2 62.3 / 173.4, S3
  71.7 / 182.6. Its S2 delta is still 3.0 MB per tab. The first launch of each new binary
  (`before` in an earlier batch, `after`, and the instrumented build) read about 10 MB higher in
  both counters and `ws` about 142 MB, where later runs read 90 MB.
- An earlier `before` batch ran while a release build was linking. Its run 2 S4 lost the 64 MB
  ballast (commit 118.9, presumably the OOM retry path under commit pressure), so it was not used.

### TUI load (`-Mode Tui`, owner hypothesis)

`tui_mimic.py 150` runs in tab 1, then in a second tab, each while visible. The load is 2 s of
alternate-screen colour redraws at about 60 fps, a third of the rows new text every frame, and
then a 300-line scrolling burst, repeated. That gives about 8,400 frames and 22,500 lines per
tab. T2 has two tabs, both loaded. privws / commit in MB:

| | T1, 1 tab loaded | T2, 2 tabs loaded | Largest cache regions (`vmregions.ps1`) |
| --- | --- | --- | --- |
| before r1 | 88.6 / 199.3 | 123.7 / 237.6 | 2 × 23.6 MB, 22.3 and 23.6 resident |
| before r2 | 88.3 / 198.8 | 123.9 / 237.6 | 2 × 23.6 MB, 23.6 and 22.6 resident |
| after r1 | 62.1 / 172.0 | 72.2 / 183.4 | none ≥ 8 MB belongs to the cache |
| after r2 | 62.0 / 172.0 | 72.1 / 183.3 | same |
| Δ at T2 | | **−51.7 / −54.3** | |

A temporary log in `GlyphCache` (entry count, `HashMap::capacity`, hits and misses, printed
every 600 frames; built once and not committed) gave the following under the same load:

- 1280x800: the maximum was 4096 entries per view. The capacity peaked at 7168, which is 8192
  buckets. The hit rate was 96.2% (2,603,494 hits and 103,257 misses in tab 1).
- 1920x1040 (one tab; the scripted "+" click misses at this width): the maximum was 4094
  entries, again 8192 buckets. The hit rate was 98.5%. `before` at the same size has exactly one
  23.6 MB table: privws / commit 98.5 / 231.6 before, 71.4 / 201.6 after.
- The table never doubled, so the soft cap holds. No bound was added and the eviction is the
  same code, so the hit rate is unchanged by construction.

### Frame time

`cargo test -p oneterm-terminal-view --lib frame_time_under_output -- --ignored --nocapture`,
dev profile, 2 runs each. The load is 1 MiB of scrolling output in 4 KiB chunks, 257 frames,
and prepaint plus paint is timed.

| | flood avg / p50 / p95 | idle avg / p95 |
| --- | --- | --- |
| before | 34.9 / 34.3 / 45.8 ms; 33.7 / 32.2 / 45.4 ms | 6.4 / 10.5 ms; 5.4 / 7.2 ms |
| after | 32.6 / 30.7 / 44.2 ms; 32.1 / 29.5 / 42.9 ms | 5.6 / 6.8 ms; 5.6 / 7.4 ms |

There is no regression. Flood is about 5% faster, consistent with a hit no longer copying 3 KB.
Rows planned are identical (13,868 of 17,733).

### Gates

- `cargo fmt --all -- --check`: pass.
- `python scripts/check-doc-paths.py`: pass (207 paths).
- `python scripts/check-english.py`: pass.
- `pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=4`), final line: `ci-local: all checks passed.` (terminal-view lib: 389 passed, 0 failed, 3 ignored). `check-doc-paths.py` and `check-english.py` were re-run after the last packet edit: pass.

### Gaps

- **The owner's 346 MB is still not fully explained.** This fix removes about 47 MB of commit
  from two busy tabs, and the table-doubling hypothesis is not reproduced. One lead was seen
  but not attributed. At 1920x1040 the process commits a second large region of 32.3 MB, 0 MB
  resident, in both builds. It is absent at 1280x800, and S1 commit is +20 MB at that size. It
  is window-size-dependent, so it is probably GPU or swap-chain staging, not OneTerm code. It
  belongs to a follow-up under IN-0045, and the owner's window size and pane count would narrow
  it.
- The mimic is not the real `claude` TUI. It uses no inline (non-alternate-screen) redraws of a
  growing region, as Ink does.
- Split panes and SSH were not measured. Every pane has its own `GlyphCache`, so the per-pane
  saving is the same by construction.
- The frame-time numbers come from the dev profile in a headless test window, not from the
  release app.
- Known and unchanged: when the map is at the cap and every entry is recent, each further miss
  runs `retain` over the whole map (O(n) per miss). That is a CPU cost only, and it predates
  this packet.
