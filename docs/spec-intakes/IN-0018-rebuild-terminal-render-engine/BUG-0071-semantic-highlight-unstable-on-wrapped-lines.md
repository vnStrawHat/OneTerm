# Work: Semantic highlighting follows the logical line across a wrap

ID: BUG-0071
Intake: IN-0018
Created: 2026-09-21

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

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: `IN-0018` — `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/IN-0018.md`

`US-0047` of this intake built the render core, including the semantic overlay hook
(`classify`) and the per-row plan cache. The classification defect reported here is in that
hook, so the fix belongs to this intake. The highlight engine itself
(`crates/highlight`) predates the harness and has no intake of its own; its owning
contract is `docs/terminal-semantic-highlighting.md`, which this packet reconciles.

## Source

Owner report (2026-09-21): "Semantic highlighting is not stable with long lines that wrap."
Follow-up the same day: "review everything, but the most visible case is when the cwd is too
long and wraps: the highlight becomes broken/intermittent."

## Outcome

A logical line is classified once, as one line, whatever the window width. A quoted string, a
keyword, a path, a URL or a prompt that straddles a wrap boundary gets the same classes it
would get if the same text fitted one row, and changing any row of a wrapped line
re-classifies the whole line — so narrowing the window, widening it, or typing into a wrapped
prompt never leaves a row coloured from a stale or truncated scan.

## Scope

- [ ] In scope:
  - `crates/terminal-view/src/render/row_plan.rs` — `classify`, which scans one visual row
    with `SemanticOverlay::scan_into` and so restarts the scanner's state at every wrap
    boundary.
  - `crates/terminal-view/src/render/plan_cache.rs` — the rebuild trigger, which closes dirty
    rows under wrap runs for the URL mask only; the class bytes need the same closure once
    they depend on neighbouring rows.
  - `crates/terminal-view/src/highlight/overlay.rs` — `scan_into`'s per-visual-row signature
    and its `display_row` role lookup.
  - `docs/terminal-semantic-highlighting.md` §8 (the render-path integration and its cache
    key) and §13 Q5 if the cache key changes.
- [ ] Out of scope:
  - Populating `RowRoles` from the OSC 133 stream (§4.2 "Phase 2"). It is unimplemented
    today — `SemanticOverlay::row_roles` is never assigned, so every row is scanned as
    `RowRole::Output` and the prompt regex fallback decides. Making the fallback correct
    across a wrap is this packet; sourcing roles from the shell is a separate outcome.
  - Painting the prompt-line background (§8 item 6). `ClassStyles::prompt_line_bg` is parsed
    from the theme asset but never read by the render path, so there is no wrapped-prompt
    background to fix yet. Recorded as a gap, not repaired here.
  - The scanner's rule set: which words are keywords, what counts as a path, the permission
    block. Only *where the line starts and ends* changes.
  - Reflow of scrollback on resize, which the engine owns (`docs/terminal-backend.md`).

## Acceptance

- [ ] A `cmd.exe` or PowerShell prompt whose cwd pushes it past the row width is highlighted
      exactly like a short prompt: the path carries `Path`, the `>` carries `PromptSign`, and
      the typed command after it carries `Command`/`Option` — whichever visual row each part
      lands on.
- [ ] A command being typed that wraps keeps one classification across the boundary: a quoted
      string opened on row 1 stays `String` on row 2, and a URL or path split by the wrap is
      one run, not two differently coloured halves.
- [ ] Printed output that wraps behaves the same.
- [ ] Editing any row of a wrapped line re-classifies every row of that line, so no row keeps
      classes from the text it had before.
- [ ] A row that does **not** carry the wrap flag is never joined to the next row: a hard
      newline still ends the logical line.
- [ ] Narrowing and widening the window so the wrap point moves produces the same colours as
      the unwrapped line, with no row left behind.
- [ ] The performance budget of §10 holds: the rows a frame classifies stay the dirty rows
      closed under their wrap runs, never the viewport, and the number of scanner invocations
      per frame does not rise.
- [ ] `cargo test -p oneterm-highlight -p oneterm-terminal-view` passes.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-semantic-highlighting.md` §4 (the scanner's two states), §4.2 (the OSC 133
  row-role path), §8 (render-path integration: the per-visible-row `scan_line` loop and the
  `line_text_hash` cache key), §10 (performance budget), §13 Q4/Q5. §8 is the clause that is
  wrong: it specifies the scan per *visible row*, which is what loses the scanner state at a
  wrap. **Update required.**
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`
  — the snapshot → row plan → paint pipeline, the plan cache and the URL wrap-run rescan that
  the class pass will reuse. Read to confirm the rescan contract before extending it.
- `docs/terminal-backend.md` — how a soft wrap is represented: the row's wrap flag, row
  identity across scroll, and what reflow does on resize. Read to confirm the wrap flag is the
  only continuation signal the view gets, and that it is per row, not per cell.
- `docs/architecture.md` — the current ownership of the highlight crate and the view's
  highlight module.

### Documentation Action

Update required:

- `docs/terminal-semantic-highlighting.md` §8 — the scan loop becomes per *logical line*
  (the wrap-connected run of visual rows, joined), with the resulting class run sliced back
  per visual row; the cache clause becomes the wrap-run closure the render path already uses
  for URL masks.
- `docs/terminal-semantic-highlighting.md` §13 Q5 — the cache key answer, if the key changes.

### Reconciliation

Before completion, list the docs changed here.

## Context

- `crates/highlight` is line-oriented by construction: `scan_line_into(line, …)` takes one
  `&str` and owns the whole state machine, including the `String` begin/end mini-state and
  the prompt/command mode transition. It has no notion of a row. Feeding it a logical line
  instead of a visual row needs no change to the engine.
- The view already has the machinery this fix needs, built for URLs by `US-0092`:
  `fill_wraps` reads the per-row wrap flags, `url_masks_rows_into` extends a URL across them,
  and `PlanCache::mark_scan_runs` closes the dirty rows under wrap runs so a change on one row
  rescans the whole run. The class pass is the same shape and should reuse it rather than
  invent a second mechanism.
- `FrameRow::wraps()` is the continuation signal; `FrameRow::text_into` already yields the
  row's scanner text plus the column of every char, which is what lets a joined scan be
  sliced back per row.

## Plan

- [ ] Reproduce in a fast-dev build: wrapped prompt (long cwd), wrapped command being typed,
      wrapped output; resize so the wrap point moves; scroll the line partly out of view.
      Capture frames.
- [ ] Audit every place the highlighter or the overlay iterates rows and record each one that
      assumes a logical line fits one row.
- [ ] Classify per logical line: join a wrap run's rows, scan once with the run's first row's
      role, scatter the classes back per row and column.
- [ ] Key the class cache by the logical line, by moving the class pass into the same
      wrap-run-closed rescan the URL masks use, so continuation rows invalidate together.
- [ ] Unit-test with synthetic grids: 20-column vs 80-column equivalence for a string, a
      keyword and a URL straddling rows; a change on row 1 re-classifies row 2; a row without
      the wrap flag is not joined.
- [ ] Reconcile §8 (and Q5).

## Decisions

None. The fix follows the wrap-run contract `US-0092` already established for URL masks; it
introduces no choice future work must inherit.

## Verification Plan

- Unit: `cargo test -p oneterm-highlight -p oneterm-terminal-view`, including the new
  synthetic-grid cases listed under Plan.
- Integration: the plan-cache tests that assert which rows are rebuilt, extended to a wrapped
  line so the rescan set is proved to be the wrap run and not the viewport (§10).
- E2E: a Windows fast-dev walk with `cmd` and PowerShell tabs — before/after frames in
  `evidence/BUG-0071-*.png`.
- Platform: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To be filled after implementation.

## Handoff

Implementer: this session. No blockers.
