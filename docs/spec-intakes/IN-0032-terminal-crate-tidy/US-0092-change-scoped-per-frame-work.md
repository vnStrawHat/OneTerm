# Work: Per-frame render work scales with change, not viewport area

ID: US-0092
Intake: IN-0032
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: `IN-0032`

## Outcome

The two remaining per-frame loops that cost viewport area cost change instead.

1. The URL mask rescan in `crates/terminal-view/src/render/plan_cache.rs` stops walking every
   column of every row whenever any single row is dirty. It rescans the dirty rows **plus their
   wrap-connected neighbours** — the scope the code's own `self.wraps[r]` tracking already exists
   to define.
2. `last_content_row` in `crates/terminal/src/content.rs` stops doing three interner lookups per
   cell of an idle viewport. It skips a row the engine already says is untouched, using
   `RowRef::is_allocated()` and `RowHeader.occ`.

Frames render **identically**. The only difference is how much work produced them, and that
difference is proved by a counted-work test that fails against today's code — not by a timing
claim.

This is `DEC-0015`'s rule ("cost change, not viewport area") applied to the two places one layer
up that did not follow when the plan cache became incremental at `US-0085`.

## Scope

### In scope

- [x] **H1 — the URL mask.** `crates/terminal-view/src/render/plan_cache.rs:143-152`. Today:

  ```rust
  let any_dirty = self.dirty.iter().any(|&d| d);
  if any_dirty {
      url_masks_into(frame, &mut self.mask_cur, &mut self.wraps);   // whole viewport
      stats.url_scans += 1;
      self.mask_prev.resize_with(rows, Vec::new);
      for (r, d) in self.dirty.iter_mut().enumerate() {
          if self.mask_cur[r] != self.mask_prev[r] { *d = true; }   // whole viewport again
      }
  }
  ```

  `url_masks_into` (`crates/terminal-view/src/url/mask.rs:17`) walks every column of every row and
  tries every entry of `PREFIXES` at each unmarked column, then the loop above compares every
  row's mask. Scope this to the dirty rows and the rows their wrap runs reach. `mask_cur` /
  `mask_prev` allocation behaviour must not regress: they are `clear()` + `resize()`d and rotated
  by `shift()` with the scroll, so this is a pure CPU change with no heap effect.
- [x] **H2 — `last_content_row`.** `crates/terminal/src/content.rs:56-67` walks rows bottom-up
  until it finds a non-blank one, and `is_blank_cell` (`:35-48`) does **three interner lookups per
  cell** (`resolve_style`, `text_char`, `resolve_extras`). The fix the audit names is three lines,
  using primitives the engine already exposes and documents:

  ```rust
  let row = screen.row(top + u64::from(index));
  if !row.is_allocated() || row.occ() == 0 { continue; }
  ```

  `RowHeader.occ` is documented at `crates/vt/src/grid/row.rs:61-63` as *"the reference's
  over-approximating hint: no column at or above `occ` has been touched since the last reset"* —
  false positives allowed, never false negatives, which is exactly what a skip needs. Narrowing
  the per-row cell scan to `..occ()` as well is in scope if it falls out for free.
- [x] **The counted-work proof for each**, and the `FrameStats` field H1 needs to be measurable
  (`url_scans` counts *scans*, not rows scanned, so it cannot express the acceptance below).
- [x] **The `lock_for_render` follow-on, as a recorded finding only.** `crates/terminal/src/model.rs:168`
  routes `terminal_info` through `TerminalHandle::lock()` rather than `lock_for_render()`, so it
  does not raise the render demand and can queue behind a pump burst.
  `crates/terminal/src/handle.rs:112-115` justifies that on the explicit premise that
  `query_state` and `terminal_info` "are O(1) under the lock" — false today only because of H2.
  Fixing H2 **restores the premise**, after which no lock change is needed. Verify that this is
  what happened and record it; **do not change the lock** in this packet.

### Out of scope

- [ ] Any change to what a frame draws. Glyphs, colours, cursor, selection, URL underlining,
  graphics placement and the diagnostics overlay must be pixel-identical for the same input.
- [ ] Scanning only the dirty rows for URLs. It is the obvious fix and it is **wrong**: a changed
  row can start or end a URL that continues into an untouched row, which is why the pass is
  whole-viewport today and why `plan_cache.rs:139-140` says so. The wrap-connected neighbours are
  not optional.
- [ ] `frame.rs`'s engine-vocabulary mirror (`crates/terminal-view/src/render/frame.rs:78-290`,
  213 lines, audit §3 item 6). Adjacent, tempting, and explicitly **not recommended** by the
  audit: `row_plan.rs`, `plan_cache.rs`, `shapes.rs`, `cursor.rs` and `theme/palette.rs` all speak
  `Color` / `CellFlags`, and the mirror still flattens dim/bright into a theme lookup key.
- [ ] `TerminalContent`'s 11 forwarding accessors (audit §3 item 7, `B2`). Same file as H2, and
  also not recommended.
- [ ] `RowsScrolled` / `RowsTrimmed` / `GraphicReleased` being materialised then dropped (audit
  §4 H3). Measured at about 10 000 pushes of a 24-byte enum into a warm `Vec` per second at
  10 000 lines/s, with no steady-state allocation. The audit's verdict is explicit: **do not
  optimise this.**
- [ ] `shapes.rs` (1 341 lines) and `row_plan.rs` (1 038), which the audit did not read (§(d).6).
- [ ] Anything in `US-0090`, `US-0091` or `US-0093`. This packet shares no file with any of them
  and may run in parallel.

## Acceptance

- [x] **H1 counted-work test, failing before and passing after.** In a 45×160 viewport with one
  changed row that starts no wrapped URL, the URL pass visits **1 row**, not 45. Against today's
  code the same test reports 45 and must fail. Add a `url_rows_scanned` counter to `FrameStats`
  (`crates/terminal-view/src/render/diagnostics.rs:14-36`) for this; the existing `url_scans`
  counts scan *events*, not rows. The counter is production code, consistent with every other
  field of that struct, and the diagnostics overlay already renders them.
- [x] **H1 wrap correctness is pinned by a test that would fail under a naive dirty-rows-only
  fix.** A URL wrapping across three rows, where only the middle row changes, must produce the
  same mask as a full rescan — and the test must assert the mask, not just the row count.
- [x] **H1's existing assertions still hold untouched**: `url_scans` is 1 on the first frame and 0
  on an idle frame (`crates/terminal-view/src/render/element_tests.rs:308, :335`;
  `plan_cache.rs:340, :347, :426`; `terminal_view/view_tests.rs:306`). None of these may be
  edited: a scan still happens exactly when it happened before, it is just smaller.
- [x] **H2 counted-work test, failing before and passing after.** On a viewport that is blank
  except for row 0 — the common idle case, and `last_content_row`'s worst case — the cells
  examined drop from about `rows × cols` to at most one row's worth. Assert the scaling directly:
  **doubling the column count must not change the work**. Against today's code it doubles, so the
  test fails. A `#[cfg(test)]`-only visit counter is the right mechanism here; do not add a
  production counter to `crates/terminal/src/content.rs` for it.
- [x] **`last_content_row` returns the same value for every input it does today.** The gutter
  renders up to the last non-blank line and `line_times` stamps to the same row, so an
  off-by-one here shows as `[--:--:--]` on a visible line. Pin at least: an all-blank screen
  (returns 0), content on the last row, content on row 0 only, a row whose only content is a wide
  spacer, a row containing only a hyperlink cell, and a row that was written and then cleared
  (which is precisely where `occ` over-approximates and the skip must **not** fire).
- [x] **No test lost, no existing assertion edited.** Baselines recorded for
  `cargo test -p oneterm-terminal-view` and `-p oneterm-terminal`; after-counts greater than or
  equal to baseline plus the new tests.
- [x] **`vt-bench` tier 3 does not regress.** Record `tier3_render` (µs/frame at 7 200 cells)
  before and after. **This is a guard, not the proof**: tier 3 measures the engine's render tier,
  and neither H1 (in `crates/terminal-view`) nor H2 (in `crates/terminal`) is on its path. The
  audit says so outright (§(d).1): *"No benchmark measures the view … my claims are complexity
  arguments from reading, not measurements."* Tier 3 exists here to catch an accidental engine
  regression from touching `content.rs`. The improvement is proved by the counted-work tests, and
  the packet must not claim a timing win it did not measure.
- [x] **GUI smoke check on Windows** — see Verification Plan for the process-safety rule.
      Partial: the prompt, the gutter timestamps, the wrapped URL and the scroll-and-return leg
      are all evidenced; the Ctrl+click, drag-selection and full-screen-TUI legs could not be
      driven from this non-interactive session (see Evidence and Gaps).
- [x] **`pwsh scripts/ci-local.ps1` exits 0**, with totals recorded.

## Documentation

### Owning Docs Reviewed

- `IN-0029/low-level-design/damage-and-render-state.md` — owns the damage model this packet
  consumes: row keys, `RenderUpdate`'s `Unchanged` case, and `DEC-0015`'s "cost change, not
  viewport area" premise. This packet extends that premise one layer up rather than changing it.
  Check whether it documents the URL mask as deliberately whole-viewport; if it does, that is the
  sentence to update.
- `crates/vt/src/grid/row.rs:61-63` — `RowHeader.occ`'s contract: over-approximating, false
  positives allowed, never false negatives. H2's correctness rests entirely on this sentence, so
  it must be quoted in the code comment that uses it.
- `crates/terminal-view/src/render/plan_cache.rs:139-140` — the comment stating why the URL pass
  is whole-viewport (a changed row can start or end a URL that continues into an untouched row).
  It is correct about the hazard and wrong about the necessary scope. **Must change**, because it
  is the exact comment a future reader would use to revert this packet.
- `crates/terminal-view/src/url/mask.rs:1-16` — `url_masks_into`'s doc contract: masks for every
  display row, wrap extension, trailing-punctuation stripping after extension, caller-owned
  buffers. If the function gains a row-range parameter, this contract changes.
- `crates/terminal/src/content.rs:30-34, :50-55` — `is_blank_cell`'s "read straight off the
  engine's packed cell" note and `last_content_row`'s doc explaining the `line_times` gutter
  dependency. The second is the reason the correctness cases above matter.
- `crates/terminal/src/handle.rs:112-115` — the lock policy premise that `terminal_info` is O(1)
  under the plain lock. This packet makes it true. Worth a sentence saying so.
- `docs/terminal-backend.md` — the render path description, to confirm it does not state the
  per-frame cost in terms this packet changes.

### Documentation Action

Update required:

- `crates/terminal-view/src/render/plan_cache.rs` — the `:139-140` comment must state the hazard
  **and** the scope that answers it (dirty rows plus wrap-connected neighbours), naming
  `self.wraps`. A comment that only names the hazard invites the whole-viewport scan back.
- `crates/terminal/src/content.rs` — the skip must cite `RowHeader.occ`'s over-approximation
  contract by file and line, because that contract is the only reason the skip is safe.
- `crates/terminal/src/handle.rs:112-115` — note that the premise now holds, so a future reader
  does not "fix" `terminal_info` onto `lock_for_render` unnecessarily.
- `IN-0029/low-level-design/damage-and-render-state.md` — only if it records the URL pass as
  deliberately whole-viewport.

Reason: no behavioural contract changes — same frames, same `last_content_row` values, same
`url_scans` event counts. But two of the comments this packet touches are the documented reasons
the slow shape existed, and leaving them as they are would make the next reader revert the change
in good faith.

### Reconciliation

Before completion, list the docs and comments changed, and confirm the no-change reason for
`docs/PROJECT.md`, `docs/terminal-backend.md` and `docs/agents/crate-dependency-rules.md` (no
crate edge moves; `crates/terminal` gains no dependency, and the hints H2 uses are already public
on `RowRef`).

**Done.** Changed:

- `crates/terminal-view/src/render/plan_cache.rs` — the phase 2 comment now states the hazard
  **and** the scope that answers it (dirty rows closed under the wrap runs, naming `self.wraps`),
  plus why `mask_prev` stays authoritative for every row. `shift`'s mask comment now covers
  `wraps_prev` travelling with the rows too.
- `crates/terminal-view/src/url/mask.rs` — `url_masks_rows_into`'s doc replaces
  `url_masks_into`'s: same wrap-extension and punctuation contract, plus the caller obligation
  that `range` be closed under wrap runs. `fill_wraps` is documented as O(rows).
- `crates/terminal/src/content.rs` — the skip quotes `RowHeader.occ`'s over-approximation
  contract and cites `crates/vt/src/grid/row.rs:61-63`, because that sentence is the only reason
  the skip is safe.
- `crates/terminal/src/handle.rs` — the `lock_for_render` premise now records that H2 made it
  true, so a future reader does not move `terminal_info` onto `lock_for_render`.
- `crates/terminal-view/src/render/diagnostics.rs` — `url_scans` gains a one-line note that it
  counts events, `url_rows_scanned` is new, and the throttled log line prints both.
- `IN-0029/low-level-design/damage-and-render-state.md` — it did **not** record the URL pass as
  deliberately whole-viewport (grep for "url" in that file returns only the hyperlink-interning
  edge case at `:379`), so nothing was stale to correct. One paragraph was added instead, saying
  `DEC-0015`'s premise binds the consumers above `render_update` and naming the two loops this
  packet brought into line — so a future widening reads as contradicting the LLD.

No change, with reasons:

- `docs/PROJECT.md` — no project fact or standing invariant moves; same frames, same
  `last_content_row` values, same `url_scans` event counts.
- `docs/terminal-backend.md` — its render-path description states *what* the path does, never its
  per-frame cost in the terms this packet changes.
- `docs/agents/crate-dependency-rules.md` — no crate edge moves. `crates/terminal` gains no
  dependency; `RowRef::occ()` / `is_allocated()` were already `pub` on an already-imported type.

## Context

Why these two and nothing else: the audit read the whole hot path — `TerminalPump::advance`,
`finish_batch*`, `TerminalHandle`, both read loops, `TerminalContent::refill`, `PlanCache::update`,
`build_row_plan` and the PTY ring — and found the big things already right and measured. No double
copy of output bytes on either path; zero steady-state per-frame allocation; the yield rule
recorded at 22.6 ms → 2.25 ms worst-case frame wait while throughput *rose* 41.4 → 54.2 MiB/s; and
engine throughput of 41.4–201.9 MiB/s parse+grid against a real ConPTY producer ceiling of about
1.2 MiB/s for `cmd.exe`. H1 and H2 are what is left.

**H2's worst case is the common case, which is why nobody noticed.** A fresh shell has a prompt on
row 0 and blank rows below it, so the bottom-up loop scans the entire viewport before returning:
45 × 160 cells × 3 hash lookups, every frame. Under sustained output the cursor sits near the
bottom and the loop returns on the first row. The call runs once per rendered frame, twice on a
scrollbar-drag frame, via `terminal-view/src/terminal_view/render.rs:219` and `:236` →
`session.rs:525-528` → `model.rs:167-176` → `crate::last_content_row(&term)`.

**H1's scope, precisely.** `self.wraps[r]` is already filled by `url_masks_into` for every row.
A URL reaching the last column of a row with `WRAPLINE` continues on the next row, so a dirty
row's mask can only be affected by, and can only affect, the rows in the same maximal wrap run.
Walk outward from each dirty row while `wraps[r]` says the run continues. The mask delta loop
(`mask_cur[r] != mask_prev[r]`) then only needs to run over the rows actually rescanned, since
every other row's mask is unchanged by construction.

**Neither cost has ever been measured** (audit §(d).1): there is no criterion bench anywhere in
the workspace, `crates/tools`'s five tiers stop at `render_update`, and `FrameStats` holds
counters, not timers. The audit's own framing is that both fixes are small enough that "measure
after" is cheaper than "measure first", and that neither should be sold as a known win. This
packet therefore accepts on **counted work**, which is a fact, and treats `vt-bench` tier 3 as a
regression guard rather than evidence.

## Plan

- [x] Record branch-point baselines: `cargo test -p oneterm-terminal-view` and
  `-p oneterm-terminal` counts, and `vt-bench` tier 3.
- [x] Add `url_rows_scanned` to `FrameStats` and write the H1 counted-work test plus the
  three-row wrapped-URL mask test. Confirm the first **fails** at 45 rows.
- [x] Write the H2 counted-work test with its `#[cfg(test)]` visit counter, plus the six
  correctness cases. Confirm the scaling assertion **fails**.
- [x] Implement H2 first — it is three lines and independent. Re-run its tests.
- [x] Implement H1: scope `url_masks_into` to a row range, walk the wrap runs outward from the
  dirty rows, and narrow the mask delta loop to the rescanned rows. Re-run its tests plus the
  untouched `url_scans` assertions.
- [x] Update the four comments named in Documentation Action.
- [x] `vt-bench` tier 3 after; `cargo test --workspace`; `pwsh scripts/ci-local.ps1`.
- [x] GUI smoke check, under the process rule below.

## Decisions

None expected. Both fixes use primitives that already exist and are already documented
(`self.wraps`, `RowRef::is_allocated()`, `RowHeader.occ`), so there is no consequential choice
future work must inherit. If H1's correct scope turns out to need something the wrap tracking
cannot express, stop and report — a wrong scope here silently mis-underlines URLs, which is a
behaviour change, not an optimisation.

## Verification Plan

Focused proof:

- The two counted-work tests, each confirmed **failing** against the branch point before the fix
  lands. A test that passes both ways proves nothing here.
- The wrapped-URL mask test and the six `last_content_row` correctness cases.
- The existing `url_scans` assertions, unedited:
  `crates/terminal-view/src/render/element_tests.rs:308, :335`,
  `plan_cache.rs:340, :347, :426`, `terminal_view/view_tests.rs:306`.

Regression:

- `cargo test -p oneterm-terminal-view`, `cargo test -p oneterm-terminal`, then
  `cargo test --workspace`.
- `cargo test -p oneterm-vt --features vt-paranoid` — H2 reads engine row hints, so the
  whole-history integrity walk is part of this packet's proof.
- `cargo clippy --workspace --all-targets -- -D warnings`.

Benchmark:

- `vt-bench` tier 3 (`tier3_render`, µs/frame at 7 200 cells) before and after, both recorded,
  same machine, same invocation. Baseline reference from `evidence/US-0087-verify.md:144-148`:
  the render tier ran 1.2–22.6 µs/frame. Report it as a **non-regression guard** and say in the
  same sentence that it does not measure either changed site.

E2E — manual GUI smoke check, Windows, and the only one in this intake:

- Open a local shell; confirm the prompt renders and the gutter timestamps are not `[--:--:--]`
  on any visible line (H2's failure mode).
- Run a full-screen TUI on the `fast-dev` profile; confirm it renders and scrolls normally.
- Print text containing a URL that wraps across a row boundary near the right margin; confirm the
  underline covers the whole URL and that clicking it opens the right target — then scroll it
  partly off-screen and back, which is where a wrap-scope bug shows.
- Drag a selection across a wrapped URL; confirm the selection and the underline agree.

**Process safety, mandatory.** The owner runs their agent session inside an OneTerm window.
Launch only with `Start-Process -PassThru`, and inspect or close **only that pid and the
descendants you observed it spawn** (`Win32_Process` `ParentProcessId`, BFS from your own pid).
Never match a process by name or by window title, and never touch a pre-existing `oneterm.exe`
— enumerate them before launching so you can prove you left them alone. Close your own window
with `CloseMainWindow` first and `Stop-Process -Id <your pid>` only as a fallback. This is
`DEC-0005`'s rule; `BUG-0054`'s evidence section is the worked example.

Platform:

- `pwsh scripts/ci-local.ps1`, exit 0, totals recorded.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record: the branch point and commit; both counted-work tests' before
(failing) and after numbers, with the viewport geometry used; the H2 scaling result at two column
counts; `vt-bench` tier 3 before and after with the machine and invocation; the test counts before
and after; the GUI smoke result with the pid discipline actually followed; and `ci-local.ps1`'s
totals.

Expected standing gaps, to be confirmed rather than discovered: no timing measurement of either
changed site exists, before or after, because nothing in the workspace benchmarks the view; the
GUI check is manual and Windows-only; and the `lock_for_render` question is recorded as resolved
by H2 rather than changed.

### Branch point

`4e83f31` (`docs(terminal): IN-0032 terminal crate tidy — intake, design and four packets`),
reached by `git reset --hard main` in the packet's worktree
(`.claude/worktrees/agent-a7148a16449276544`). Every number below is from that worktree,
`CARGO_BUILD_JOBS=3` throughout (two sibling worktrees were building concurrently).

### H1 — URL rescan, counted rows

New `FrameStats::url_rows_scanned`. Both numbers are from
`render::plan_cache::tests::url_pass_scans_the_changed_rows_not_the_viewport`, a 45 × 160
viewport with no wrapped rows.

| frame | before (whole-viewport scan) | after |
| --- | ---: | ---: |
| first frame (nothing cached) | 45 | 45 |
| one row rewritten | **45** | **1** |
| idle frame (`Unchanged`) | 0 | 0 |

`url_scans` is unchanged at 1 / 1 / 0 — a scan still happens exactly when it happened before, it
is just smaller. The two wrap-scope tests:

| test | before | after |
| --- | ---: | ---: |
| `url_pass_rescans_the_whole_wrap_run_of_a_changed_row` (5 rows, a 3-row wrap run, middle row rewritten) | 5 | **3** |
| `url_pass_rescans_a_continuation_row_whose_wrap_was_dropped` (3 rows) | 3 | **2** |

The first also asserts the resulting mask equals a full `url_masks_into` rescan of the same
frame, row by row — the row count alone would not catch a wrong scope. The "before" column was
produced by temporarily forcing `mark_scan_runs` to mark every row, which is exactly the old
whole-viewport shape; all three tests failed in that state and pass now.

**Scope, precisely.** Rows `r` and `r + 1` are connected when `wraps[r] || wraps_prev[r]` — the
*union* of this frame's and the last frame's `WRAPLINE` flags, not just this frame's. A row that
has just **lost** its wrap flag is dirty, but the continuation row that inherited its mask is
not, and only the union pulls that row back into the rescan. Dropping `wraps_prev` and using this
frame's flags alone leaves a stale underline on the old continuation row; the second test above
exists to pin that and fails without it.

### H2 — `last_content_row`, counted cells

`#[cfg(test)]`-only `CELLS_EXAMINED` thread-local; no production counter was added.
`last_content_row_cost_follows_the_content_not_the_viewport`: a 45-row viewport, blank except for
a two-character prompt on row 0 — the idle case, and this function's worst case.

| columns | before | after |
| ---: | ---: | ---: |
| 40 | 1 800 (45 × 40) | **2** |
| 80 | 3 600 (45 × 80) | **2** |

Before, doubling the columns doubled the work, and every one of those cells cost three interner
lookups (`resolve_style`, `text_char`, `resolve_extras`). After, the work is the two cells the
prompt actually occupies and does not move with the geometry at all. The failing "before" run was
produced by removing the `is_allocated()` / `occ() == 0` skip and the `..occ` narrowing while
keeping the counter: `left: 1800, right: 3600`.

`last_content_row_pins_the_blank_definition` covers the six correctness cases: all-blank screen
(0), content on the last row, content on row 0 only, a wide pair (its `WideSpacer` column is
content), a row whose only content is an OSC 8 link on a space cell, and a row written and then
cleared with `\x1b[2K` — which returns 0 **and** is shown to have been examined rather than
skipped, because `occ` over-approximates exactly there.

### lock_for_render (recorded finding, no change)

Confirmed as the packet predicted. `crates/terminal/src/handle.rs:112-115` justifies routing
`terminal_info` through the plain `lock()` on the premise that it is O(1) under the lock;
`crates/terminal/src/model.rs:176` calls `last_content_row`, which was the single reason that
premise was false. H2 restores it, so **no lock change was made**. A sentence saying so was added
to the `lock_for_render` doc comment.

### Test counts

| suite | before | after |
| --- | --- | --- |
| `cargo test -p oneterm-terminal-view` | 288 passed, 3 ignored | 291 passed, 3 ignored |
| `cargo test -p oneterm-terminal` | 270 passed | 272 passed |

No test was deleted and no existing assertion edited. The `url_scans` assertions named in the
Verification Plan (`element_tests.rs:308, :335`, `plan_cache.rs:340, :347, :426`,
`view_tests.rs:306`) are untouched and green.

### vt-bench tier 3 — non-regression guard only

`cargo run -q -p oneterm-tools --release --bin vt-bench -- all --mib 2`, same machine
(Windows 11, the packet's worktree), before and after, µs/frame at 7 200 cells:

| fixture | before | after |
| --- | ---: | ---: |
| `plain_ascii` | 5.3 | 4.6 |
| `long_lines` | 3.0 | 2.3 |
| `heavy_sgr` | 1.4 | 1.3 |
| `tui_redraw` | 20.0 | 17.9 |
| `scroll_region` | 33.0 | 21.5 |
| `cjk_wide` | 2.0 | 1.7 |
| `dense_cells` | 2.0 | 1.7 |
| `scrolling` | 2.7 | 2.3 |
| `sixel` | 6.3 | 5.6 |
| `osc_9_7` | 6.1 | 5.6 |

No regression. **This is not evidence of the improvement**: tier 3 measures the engine's render
tier, and neither H1 (in `crates/terminal-view`) nor H2 (in `crates/terminal`) is on its path.
The uniform downward drift is machine load, not this change. The improvement is the counted-work
tables above.

### GUI smoke check (Windows, manual, this worktree only)

`cargo build -p oneterm-app --profile fast-dev`; D: had 61.6 GB free before the run.
`target/terminal.json` was written with `layout.show_gutter: true` and the local shell's `args`
set to `&& type <200-URL fixture>` so the URL-heavy output is on screen at startup; it was
deleted again afterwards (`target/` is gitignored and holds no committed state).

Process safety, as required: `Get-Process oneterm` was enumerated **before** each launch and
recorded — pids `2504`, `14804` and, on the last run, `7488` (the owner's own OneTerm windows,
one of which runs their agent session). None was touched. Every launch used
`Start-Process -PassThru`; only that pid was acted on; shutdown was `CloseMainWindow` first with
`Stop-Process -Id <my pid>` as the fallback. No process was ever matched by name or window title.
After every run the pre-existing pids were re-enumerated and all were still alive, and my own pid
was gone.

| run | my pid | result |
| --- | ---: | --- |
| 1 | 13828 | window up, prompt renders; `CopyFromScreen` refused (no screen DC in this session) |
| 2 | 7844 / 15720 | switched to `PrintWindow(…, PW_RENDERFULLCONTENT)`; capture works |
| 3 | 16464 / 18408 | URL fixture on screen after fixing the config (see below) |
| 4 | 16180 | synthetic wheel via `mouse_event` does not reach the window |
| 5 | 4636 | posted `WM_MOUSEWHEEL` does scroll; all three screenshots captured |

Evidence, in `evidence/`:

- `US-0092-url-heavy-output.png` — 200 URL-bearing lines. Every URL is underlined; the gutter
  shows a real timestamp (`[19:50:57]`) on **every** visible line including the blank lines below
  the prompt, which is H2's failure mode (`[--:--:--]`) absent. The long URL wraps from display
  row 201 onto 202 and the underline is continuous across the row boundary, ending at `-end`;
  `after the wrapped url` on row 203 is not underlined.
- `US-0092-scrolled-back.png` — scrolled ~30 lines into history. Underlines intact; this is the
  `shift()` path, where `mask_prev` and the new `wraps_prev` rotate with their rows.
- `US-0092-scrolled-forward.png` — scrolled back down. The wrapped URL returns with its underline
  still spanning both rows, which is where a wrap-scope bug would show.

Also worth recording: printing 202 lines into a 45-row viewport scrolls ~180 times, so the
whole-viewport fallback (every row dirty → every row rescanned) and the rotation path were both
exercised heavily in the same run, and the final frame is correct.

Two notes on driving the app from this session, for whoever runs the next GUI check here:
`Graphics.CopyFromScreen` fails with "The handle is invalid" and neither `SendKeys` nor
`mouse_event` reaches the window — the session has no interactive desktop input. `PrintWindow`
with `PW_RENDERFULLCONTENT` (flag 2) captures a GPUI window correctly, and `PostMessage` of
`WM_MOUSEWHEEL` does drive the scroll. Also, `terminal.json` is quarantined as invalid unless
`shell` carries **all** of `kind`, `program`, `args`, `env`, `cwd`, `utf8` — only `utf8` has a
serde default.

### ci-local.ps1

`pwsh scripts/ci-local.ps1` → `ci-local: all checks passed.` (exit 0). Workspace test totals,
summed over both cargo test invocations the script runs:

```
sections: 60  passed: 1940  failed: 0  ignored: 14
```

Against the 60 / 1935 / 0 / 14 baseline, +5 passed = the three new `plan_cache` tests and the two
new `content_tests` tests. `cargo fmt --all` and
`cargo clippy --workspace --all-targets -- -D warnings` are clean.

**After the rework** (merge with `main` at `4bb088d` + the eleven adopted verification tests + the
two fixes), re-run in full:

```
ci-local: all checks passed.        (exit 0)
sections: 60  passed: 1956  failed: 0  ignored: 14
```

Per crate: `oneterm-terminal` 276 passed, `oneterm-terminal-view` 299 passed / 3 ignored. The
+16 over the previous run is the eleven adopted tests plus `US-0090`'s own net test delta from
the merge.

### harness.db row

No `harness.db` exists in this worktree, so the status row is recorded here as the snippet to
apply against the authoritative database rather than written directly:

```python
import sqlite3
db = sqlite3.connect("harness.db")
db.execute(
    "UPDATE work SET status = ?, proof_unit = 1, proof_integration = 1, "
    "proof_e2e = 1, proof_platform = 1, proof_verify = 1 WHERE id = ?",
    ("implemented", "US-0092"),
)
db.commit()
```

### Independent verification — FAIL, then rework

The first implementation (`f963a94`) was reviewed by an independent verifier, whose full report is
[`evidence/US-0092-verify.md`](evidence/US-0092-verify.md) with its own screenshots
(`US-0092-verify-a-url-heavy.png` … `-f-ctrlclick.png`). Verdict: **FAIL**, two behaviour
regressions of exactly the class this packet's Acceptance forbids. Both are now fixed on top of a
merge with `main` at `4bb088d` (which brought `US-0090` in — `Engine` newtype deleted,
`take_render_demand` gone, `Demand` moved; the only conflict was `handle.rs`, resolved by keeping
`main`'s signature and re-appending this packet's `lock_for_render` note).

| # | Severity | Site | What was wrong | Fix |
| --- | --- | --- | --- | --- |
| 1 | **Major** | `content.rs` | `occ == 0` was read as "blank". `Row::reset` fills the row with the erase template and *then* zeroes `occ` (`crates/vt/src/grid/row.rs:161-171`), so after `CSI 44 m` + `ED` — or any scroll / `IL` / `DL` under a non-default background, which is what every full-screen TUI does — the cells carry `bg = Blue` with `occ == 0`. The old code called that content; the new code skipped it, so the gutter lost timestamps on painted lines: H2's own stated failure mode. | `reset` also sets `flags = DIRTY \| flags_for(template)`, so the row itself records that the template was not plain. The skip now also requires `!flags.intersects(STYLED \| HAS_EXTRAS \| HAS_GRAPHEME)`, and the `..occ` narrowing is gated on the same condition — with a styled erase template the cells *above* `occ* are that template, not blanks, so narrowing was wrong there too. |
| 2 | **Major (stale render)** | `plan_cache.rs` | A wrapped URL whose head scrolls above the viewport top left a stale underline on its continuation rows. Display row 0's mask is extended into it from the row above, and a scroll moves the viewport boundary with **no** row's `(RowId, SeqNo)` changing — so no row is dirty, no run is rescanned, and the mask that a full rescan would now empty survives. `wraps_prev` cannot express it: the run walk has no `connected(-1)`. | On a `Partial { scrolled != 0 }` frame, display row 0 is seeded into `self.scan` (**not** `self.dirty`) and closed under its wrap run like any other seed. The existing mask-delta compare then decides whether a plan is rebuilt, so a scrolled frame costs one extra rescanned row and no extra plan. |
| 3 | Minor (note) | comments + LLD | The paraphrase "a row the engine says was never written" is not what `occ` means after a `reset`. | Corrected in `content.rs` and in `damage-and-render-state.md`, which now carries both traps as standing notes for the next consumer of these hints. |

**Tests adopted verbatim from the verifier's worktree** (`git diff` of
`crates/terminal/src/content_tests.rs` and `crates/terminal-view/src/render/plan_cache.rs`,
applied with `git apply --3way`). Four of the eleven failed on `f963a94` and pass now:

| test | file | on `f963a94` |
| --- | --- | --- |
| `last_content_row_sees_a_background_erased_row` | `content_tests.rs` | **FAIL** (0, want 4) |
| `last_content_row_sees_a_background_erased_scroll_in` | `content_tests.rs` | **FAIL** (2, want 3) |
| `last_content_row_default_erase_stays_blank` | `content_tests.rs` | pass (control) |
| `url_v2_scrolling_the_viewport_keeps_the_masks_exact` | `plan_cache.rs` | **FAIL** (defect 2) |
| `url_v2_streaming_output_keeps_the_masks_exact` | `plan_cache.rs` | **FAIL** (defect 2) |
| `url_v2_first_row_of_a_three_row_url_changes` | `plan_cache.rs` | pass |
| `url_v2_last_row_of_a_three_row_url_changes` | `plan_cache.rs` | pass |
| `url_v2_delete_and_insert_line_inside_a_wrapped_url` | `plan_cache.rs` | pass |
| `url_v2_clear_screen_and_alt_screen_swap` | `plan_cache.rs` | pass |
| `url_v2_resize_rewraps_the_url` | `plan_cache.rs` | pass |
| `url_v2_wrap_dropped_in_a_frame_that_was_never_rendered` | `plan_cache.rs` | pass |

plus the helper `assert_masks_match_a_full_rescan`, which compares **every** row against a
from-scratch `url_masks_into` of the same frame — a much stronger invariant than the row counts,
and the reason defect 2 was caught at all.

**The counted-work numbers are unchanged.** Every figure in the H1 and H2 tables above still
holds, asserted by the same `assert_eq!`s:

- H1's 45 → 1, 5 → 3 and 3 → 2 are all measured on `Partial { scrolled: 0 }` frames (a row
  rewritten in place), so the seam seeding does not fire and adds nothing.
- On a genuinely scrolled frame the seam costs **one extra rescanned row**, closed under its run.
  `scroll_shifts_plans_and_replans_only_scrolled_in_rows` still asserts `rows_planned == 2` and
  `rows_candidate == 2`, because row 0 goes into `scan`, not `dirty`, and its mask did not change.
- H2's 1 800 / 3 600 → 2 / 2 is unchanged: a fresh shell's blank rows carry no content hints, so
  the added `flags()` condition never fires on them.

**One cross-crate change was needed.** `RowRef::flags()` had been narrowed to `pub(crate)` by
`US-0090` (it was `pub` at this packet's branch point, which is what the verifier's suggested fix
assumed). It is `pub` again, with a comment naming `last_content_row` as the consumer and why the
hints — not `occ` — are what answers "is this row blank". `RowFlags` itself was already `pub` and
reachable at `oneterm_vt::grid::RowFlags`; no other visibility moved, and no crate edge changed.

### Re-run of the defect-2 GUI leg

Same process discipline: `Get-Process oneterm` enumerated first — pids `2504` and `14804`, the
owner's own windows, one of which runs their agent session — neither touched, never matched by
name or title; `Start-Process -PassThru` with `-WorkingDirectory` set to this worktree; my pids
`16828` and `18216`; `CloseMainWindow()` then `Stop-Process -Id <my pid>` as fallback; both
pre-existing pids re-checked alive afterwards and mine gone. `target/terminal.json` written for
the run and deleted after.

Fixture: 200 plain lines, one 706-character URL (`WRAPHEAD https://wrap.test/segment-…tail-end`,
wrapping onto **five** display rows, 201–205), then 44 plain lines.

- `US-0092-fixed-scroll-back.png` — wheeled back 6 clicks: all five rows of the URL on screen,
  underline continuous across 201→205 and stopping exactly at `tail-end`. The plain `filler` and
  `trailer` lines around it are not underlined.
- `US-0092-fixed-scroll-forward.png` — then wheeled **forward** 5 clicks, so rows 201–203
  (including the `https://` head) pass above the viewport top and rows 204–205 sit at the top of
  the screen. They render as **plain text, no underline** — which is what a whole-viewport rescan
  of that frame produces, and what the code did before this packet. This is the exact position the
  verifier captured as `US-0092-verify-d-stale-underline.png` with three rows still underlined and
  no `https://` anywhere on screen.

### Gaps

- **No timing measurement of either changed site exists, before or after.** Nothing in the
  workspace benchmarks the view; `crates/tools`'s five tiers stop at `render_update` and
  `FrameStats` holds counters, not timers. Both fixes are accepted on counted work, which is a
  fact, and this packet claims no timing win. Confirmed as expected, not discovered.
- **The GUI check is manual, Windows-only, and partial.** Covered across this packet and the
  verification: URL-heavy output, the gutter, a wrapped URL across a row boundary, a re-wrap onto
  five rows after a resize, scroll back **and** scroll forward past the seam, and a drag-select
  that reached the app. Not covered: the full-screen TUI (`doom-fire` is not built here, and the
  verifier ran out of budget), and Ctrl+click.
- **Ctrl+click is not testable from an agent session — this is a harness limit, not a gap in the
  change.** The verifier established the mechanism: GPUI reads modifier state from the real
  keyboard via `GetKeyState`, which a posted `WM_KEYDOWN VK_CONTROL` does not set, so a
  posted-message driver cannot deliver a modified click. It posts cleanly and nothing happens (no
  browser process appears, no URL-open attempt is logged). Nothing about the change is in
  question; the input path is simply unreachable without a human at the keyboard.
- **`lock_for_render` is resolved by H2, not changed.** Recorded above; no lock change was made,
  which is what the packet asked for.
- **`vt-bench` tier 3 does not cover either changed site.** Recorded as a guard only.

## Handoff

Independent of `US-0090`, `US-0091` and `US-0093`; shares no file with any of them and may run in
parallel. One coupling to flag: this packet gives `oneterm_vt::RowRef` an external consumer in
`crates/terminal/src/content.rs`. If it lands before `US-0090`, `RowRef` must **not** be dropped
from `crates/vt/src/lib.rs`'s re-export block — `US-0090`'s Context already carries that warning.

**Confirmed on landing, and one item re-widened after the merge.**
`crates/terminal/src/content.rs` now calls four `pub` items on `oneterm_vt::RowRef` —
`is_allocated()`, `occ()`, `cells()` and `flags()` — and uses `oneterm_vt::grid::RowFlags`.
`US-0090` landed first and had narrowed `RowRef::flags()` to `pub(crate)`; the rework restores it
to `pub` with a comment naming this consumer, because the content hints are the only thing that
distinguishes a blank `occ == 0` row from one painted by a non-default erase template. Keep all
four `pub`, and keep `RowRef` re-exported from `crates/vt/src/lib.rs`. `RowHeader.occ`'s doc comment (`crates/vt/src/grid/row.rs:61-63`) is now
cited by name and line from `content.rs`; if that comment moves, the citation needs updating.
Nothing else new crosses the crate boundary: `fill_wraps` and `url_masks_rows_into` are
`pub(crate)` inside `oneterm-terminal-view`, and `url_masks_into` is now `#[cfg(test)]` there.
