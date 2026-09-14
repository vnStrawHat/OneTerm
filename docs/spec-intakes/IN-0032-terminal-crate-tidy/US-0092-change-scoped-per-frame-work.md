# Work: Per-frame render work scales with change, not viewport area

ID: US-0092
Intake: IN-0032
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] **H1 — the URL mask.** `crates/terminal-view/src/render/plan_cache.rs:143-152`. Today:

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
- [ ] **H2 — `last_content_row`.** `crates/terminal/src/content.rs:56-67` walks rows bottom-up
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
- [ ] **The counted-work proof for each**, and the `FrameStats` field H1 needs to be measurable
  (`url_scans` counts *scans*, not rows scanned, so it cannot express the acceptance below).
- [ ] **The `lock_for_render` follow-on, as a recorded finding only.** `crates/terminal/src/model.rs:168`
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

- [ ] **H1 counted-work test, failing before and passing after.** In a 45×160 viewport with one
  changed row that starts no wrapped URL, the URL pass visits **1 row**, not 45. Against today's
  code the same test reports 45 and must fail. Add a `url_rows_scanned` counter to `FrameStats`
  (`crates/terminal-view/src/render/diagnostics.rs:14-36`) for this; the existing `url_scans`
  counts scan *events*, not rows. The counter is production code, consistent with every other
  field of that struct, and the diagnostics overlay already renders them.
- [ ] **H1 wrap correctness is pinned by a test that would fail under a naive dirty-rows-only
  fix.** A URL wrapping across three rows, where only the middle row changes, must produce the
  same mask as a full rescan — and the test must assert the mask, not just the row count.
- [ ] **H1's existing assertions still hold untouched**: `url_scans` is 1 on the first frame and 0
  on an idle frame (`crates/terminal-view/src/render/element_tests.rs:308, :335`;
  `plan_cache.rs:340, :347, :426`; `terminal_view/view_tests.rs:306`). None of these may be
  edited: a scan still happens exactly when it happened before, it is just smaller.
- [ ] **H2 counted-work test, failing before and passing after.** On a viewport that is blank
  except for row 0 — the common idle case, and `last_content_row`'s worst case — the cells
  examined drop from about `rows × cols` to at most one row's worth. Assert the scaling directly:
  **doubling the column count must not change the work**. Against today's code it doubles, so the
  test fails. A `#[cfg(test)]`-only visit counter is the right mechanism here; do not add a
  production counter to `crates/terminal/src/content.rs` for it.
- [ ] **`last_content_row` returns the same value for every input it does today.** The gutter
  renders up to the last non-blank line and `line_times` stamps to the same row, so an
  off-by-one here shows as `[--:--:--]` on a visible line. Pin at least: an all-blank screen
  (returns 0), content on the last row, content on row 0 only, a row whose only content is a wide
  spacer, a row containing only a hyperlink cell, and a row that was written and then cleared
  (which is precisely where `occ` over-approximates and the skip must **not** fire).
- [ ] **No test lost, no existing assertion edited.** Baselines recorded for
  `cargo test -p oneterm-terminal-view` and `-p oneterm-terminal`; after-counts greater than or
  equal to baseline plus the new tests.
- [ ] **`vt-bench` tier 3 does not regress.** Record `tier3_render` (µs/frame at 7 200 cells)
  before and after. **This is a guard, not the proof**: tier 3 measures the engine's render tier,
  and neither H1 (in `crates/terminal-view`) nor H2 (in `crates/terminal`) is on its path. The
  audit says so outright (§(d).1): *"No benchmark measures the view … my claims are complexity
  arguments from reading, not measurements."* Tier 3 exists here to catch an accidental engine
  regression from touching `content.rs`. The improvement is proved by the counted-work tests, and
  the packet must not claim a timing win it did not measure.
- [ ] **GUI smoke check on Windows** — see Verification Plan for the process-safety rule.
- [ ] **`pwsh scripts/ci-local.ps1` exits 0**, with totals recorded.

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

- [ ] Record branch-point baselines: `cargo test -p oneterm-terminal-view` and
  `-p oneterm-terminal` counts, and `vt-bench` tier 3.
- [ ] Add `url_rows_scanned` to `FrameStats` and write the H1 counted-work test plus the
  three-row wrapped-URL mask test. Confirm the first **fails** at 45 rows.
- [ ] Write the H2 counted-work test with its `#[cfg(test)]` visit counter, plus the six
  correctness cases. Confirm the scaling assertion **fails**.
- [ ] Implement H2 first — it is three lines and independent. Re-run its tests.
- [ ] Implement H1: scope `url_masks_into` to a row range, walk the wrap runs outward from the
  dirty rows, and narrow the mask delta loop to the rescanned rows. Re-run its tests plus the
  untouched `url_scans` assertions.
- [ ] Update the four comments named in Documentation Action.
- [ ] `vt-bench` tier 3 after; `cargo test --workspace`; `pwsh scripts/ci-local.ps1`.
- [ ] GUI smoke check, under the process rule below.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
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

## Handoff

Independent of `US-0090`, `US-0091` and `US-0093`; shares no file with any of them and may run in
parallel. One coupling to flag: this packet gives `oneterm_vt::RowRef` an external consumer in
`crates/terminal/src/content.rs`. If it lands before `US-0090`, `RowRef` must **not** be dropped
from `crates/vt/src/lib.rs`'s re-export block — `US-0090`'s Context already carries that warning.
