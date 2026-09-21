# Work: Semantic highlighting follows the logical line across a wrap

ID: BUG-0071
Intake: IN-0018
Created: 2026-09-21

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

- [x] In scope:
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
- [x] Out of scope:
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

- [x] A `cmd.exe` or PowerShell prompt whose cwd pushes it past the row width is highlighted
      exactly like a short prompt: the path carries `Path`, the `>` carries `PromptSign`, and
      the typed command after it carries `Command`/`Option` — whichever visual row each part
      lands on.
- [x] A command being typed that wraps keeps one classification across the boundary: a quoted
      string opened on row 1 stays `String` on row 2, and a URL or path split by the wrap is
      one run, not two differently coloured halves.
- [x] Printed output that wraps behaves the same.
- [x] Editing any row of a wrapped line re-classifies every row of that line, so no row keeps
      classes from the text it had before.
- [x] A row that does **not** carry the wrap flag is never joined to the next row: a hard
      newline still ends the logical line.
- [x] Narrowing and widening the window so the wrap point moves produces the same colours as
      the unwrapped line, with no row left behind.
- [x] The performance budget of §10 holds: the rows a frame classifies stay the dirty rows
      closed under their wrap runs, never the viewport, and the number of scanner invocations
      per frame does not rise.
- [x] `cargo test -p oneterm-highlight -p oneterm-terminal-view` passes.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Changed:

- `docs/terminal-semantic-highlighting.md` §8 — item 3 (the cache key) and the pipeline
  pseudocode now state the scan unit as the logical line and the rescan scope as the dirty
  rows closed under wrap runs, with a prose paragraph naming what the per-visual-row loop
  got wrong.
- `docs/terminal-semantic-highlighting.md` §13 Q5 — an "Amended by `BUG-0071`" paragraph on
  the cache key and a rewritten Implementation bullet.

Unchanged, and why:

- `docs/terminal-backend.md` — the wrap flag and row identity are unchanged; the view now
  reads the flag it already published.
- `low-level-design/render-pipeline.md` — the pipeline shape is unchanged; the class pass
  joins the same wrap-run rescan the URL pass already documents there.

## Audit — every row iteration that assumed one logical line fits one row

The owner asked for a full review, so every place the highlighter or the overlay walks rows
was checked:

| Site | Assumed one row = one line? | Outcome |
| --- | --- | --- |
| `crates/terminal-view/src/render/row_plan.rs` `classify` | **Yes** — scanned `row.text_into(...)`, one visual row per `scan_line_into` | The defect. Replaced by `class_rows_into`, which scans the wrap run. |
| `crates/terminal-view/src/render/plan_cache.rs` `update` phase 2/3 | **Yes** — the wrap-run closure existed but fed only the URL masks; the class bytes were recomputed inside `build_row_plan` from that row alone | Fixed: the class pass runs in the same wrap-run-closed rescan, with the same previous/current delta. |
| `crates/terminal-view/src/highlight/overlay.rs` `scan_into(line, display_row, …)` | **Yes** — `display_row` indexed `RowRoles` as if the row were the line | Fixed: the parameter is the logical line's first row, and it is the run's first row that supplies the role. |
| `crates/terminal-view/src/url/mask.rs` `url_masks_rows_into` | **No** — extends a URL across `WRAPLINE` already (`US-0092`) | Correct; it is the pattern the fix reuses. |
| `crates/terminal-view/src/url/detect.rs` (`URL_WINDOW`) | **No** — reads a window of rows around the pointer | Correct. |
| `crates/highlight/src/scanner/**` | Line-oriented by construction; takes a `&str`, has no row concept | No change needed — feeding it a longer string is the whole fix. |
| `crates/highlight/src/role.rs` `RowRoles` | **Yes** by design (§4.2 stores a role *per display row*) | Never populated today (see Gaps), so it is inert. Left as is; sourcing roles from OSC 133 is out of scope and must, when implemented, key off the row that starts the logical line. |
| `ClassStyles::prompt_line_bg` (§8 item 6) | Would be per row | Parsed from the theme asset, read by nothing. No prompt-line background is painted at all, wrapped or not (see Gaps). |

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

- [x] Reproduce in a fast-dev build: wrapped prompt (long cwd), wrapped command being typed,
      wrapped output; resize so the wrap point moves; scroll the line partly out of view.
      Capture frames.
- [x] Audit every place the highlighter or the overlay iterates rows and record each one that
      assumes a logical line fits one row.
- [x] Classify per logical line: join a wrap run's rows, scan once with the run's first row's
      role, scatter the classes back per row and column.
- [x] Key the class cache by the logical line, by moving the class pass into the same
      wrap-run-closed rescan the URL masks use, so continuation rows invalidate together.
- [x] Unit-test with synthetic grids: 20-column vs 80-column equivalence for a string, a
      keyword and a URL straddling rows; a change on row 1 re-classifies row 2; a row without
      the wrap flag is not joined.
- [x] Reconcile §8 (and Q5).

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Reproduced

A `fast-dev` build of `main` @ `39a6d853`, launched with its own scratch `USERPROFILE` so the
cwd is long enough to wrap, cmd.exe tab, window 1000x640:

- `evidence/BUG-0071-before-01-wrapped-prompt.png` — the very first frame. The prompt wraps
  over three rows; rows 1 and 2 of the path are `Path` blue, and `fore` — the tail of the
  **same** path, on row 3 — is default white. One path, two colours, split exactly at the
  wrap boundary.
- `evidence/BUG-0071-before-02-wrapped-command.png` — a typed command after that prompt.
  `"error log upload"` is not one `String` run, and `C:\Temp\report\error-2026.log` is torn
  into blue `C:\Temp\report\` + red `error` + white `-2026.log`, because the line was scanned
  in `OutputMode`: the prompt was never detected, so nothing after `>` was a command.
- `evidence/BUG-0071-before-03-wrapped-output.png` — a quoted string that opens on row 2 and
  closes on row 5 is *not* `String` on rows 3 and 4; those rows are scanned as fresh output
  lines and come out `Path` blue, and row 5 opens a second string of its own.
- `evidence/BUG-0071-before-04-narrowed.png` — the same text at a narrower width. `error`
  inside the quotes is now red where it was white, and the path tail is now blue where it was
  white. Nothing changed but the window size. This is the "intermittent" the owner saw.

### Root cause

`crates/terminal-view/src/render/row_plan.rs:457` (pre-fix `classify`) called
`overlay.scan_into(&scratch.line_text, row.index(), …)` with **one visual row's** text, and
`crates/terminal-view/src/highlight/overlay.rs:74` handed that row straight to
`scan_line_into`. The scanner owns the whole line state — the quote begin/end mini-state,
the prompt-sign detection, the prompt -> command mode switch — so every soft wrap reset it.
Concretely, for a wrapped cmd prompt: `crates/highlight/src/profile.rs:99` anchors
`PROMPT_CMD` at `^[A-Za-z]:…>`, so the first visual row (no `>`) and the last (`fore>`, no
drive letter) both fail `looks_like_prompt`
(`crates/highlight/src/scanner/prompt.rs:34`) and are scanned as output.

The cache half: `crates/terminal-view/src/render/plan_cache.rs` decides a rebuild from
`RowKey = (RowId, SeqNo)` per display row, and applied the wrap-run closure
(`mark_scan_runs`) only to the URL masks. Once a class depends on the whole logical line, a
continuation row must be replanned when a *neighbouring* row changes, which no per-row key
can express.

### Fixed

- `class_rows_into` / `scan_logical_line` in `row_plan.rs` join a wrap run into one string,
  scan it once with the run's first row's role, and scatter the classes back by
  `(row, col)`; `FrameRow::append_text_into` records the row each char came from.
- `plan_cache.rs` runs that pass inside the existing wrap-run-closed rescan and keeps
  `class_prev` beside `mask_prev`, so a row is replanned on a class delta as well as a mask
  delta, and both rotate with their rows on scroll.
- `classify` now only copies the precomputed classes and lays `Class::Url` on top.

After-frames, same walk on the fixed build:
`evidence/BUG-0071-after-01-wrapped-prompt.png` (the path tail `ter` is blue like the rest,
`>` is the prompt sign), `-after-02-wrapped-command.png` (the command, its option and the
quoted string classify as they do on a short prompt; the torn path is whole),
`-after-03-wrapped-output.png` (the four-row quoted string is one run),
`-after-04-narrowed.png` (the same colours at the narrower width — resizing no longer
changes the classification).

### Commands

- `cargo test -p oneterm-highlight -p oneterm-terminal-view` — 71 + 358 passed, 0 failed.
- New tests: `render::row_plan::tests::wrapped_line_classifies_like_the_same_text_unwrapped`
  (a 20-column grid vs an 80-column grid, byte for byte),
  `a_prompt_that_wraps_keeps_its_sign_and_command`,
  `a_string_that_straddles_a_wrap_is_one_run`, `a_hard_newline_is_not_joined`,
  `editing_one_row_reclassifies_the_whole_logical_line`, and
  `render::plan_cache::tests::class_delta_replans_the_continuation_row`.
- `pwsh scripts/ci-local.ps1` — "ci-local: all checks passed."

### Performance (§10)

The budget is "only the visible viewport is lexed, and the hash cache re-lexes only what
changed". Both still hold, and the scan got cheaper rather than dearer:

- The rescan scope is unchanged — `mark_scan_runs` already closed the dirty rows under wrap
  runs for the URL masks, and the class pass reuses exactly that range. It is never the
  viewport; `class_delta_replans_the_continuation_row` asserts the scanned-row count stays
  at the wrap run (2 rows) when one row of a wrapped line is rewritten.
- One wrap run is now **one** `scan_line_into` call instead of one per row, over the same
  total characters, so the number of scanner invocations per frame falls for wrapped lines
  and is unchanged (one per row) for unwrapped ones. The per-line fixed costs — the
  `byte_to_char` map, the prompt regex, the Aho-Corasick start — are paid once per logical
  line instead of once per visual row.
- Added state is one `Vec<u8>` per display row (`class_prev`/`class_cur`), sized like the
  existing `Vec<bool>` URL masks, reused across frames by swap so a steady frame allocates
  nothing.
- No benchmark harness covers the render path (`crates/tools` has the DOOM-fire workload and
  a corpus replay for the VT engine, no `cargo bench`), so this is an argument from the
  rescan scope and the call count, not a measured number. Recorded as a gap.

### Gaps

- **Not measured.** There is no render-path benchmark in the repo, so the §10 claim above
  rests on the unchanged rescan scope and the reduced call count rather than a timing run.
- **A logical line whose head has scrolled above the top of the viewport** is classified from
  its first *visible* row, because the view only holds the viewport. Its colours can change
  as it scrolls. This is the viewport-only contract of §10/Q5 and is the same limit the URL
  pass has had since `US-0092`; it is not new, and it is not addressed here.
- **`RowRoles` is still never populated** — `SemanticOverlay::row_roles` has no setter, so
  the OSC 133 fast path of §4.2 is inert and every line falls back to the prompt regex. Out
  of scope; it deserves its own packet.
- **The prompt-line background (§8 item 6) is not painted at all.**
  `ClassStyles::prompt_line_bg` is parsed from `assets/highlight/default.json` and read by
  nothing in the render path. There is therefore no wrapped-prompt background defect to fix,
  and none to regress. Out of scope; its own packet.
- **The GUI walk is driven by posted messages**, so a couple of the `cmd.exe` steps ran into
  each other (`clsecho`, `clscd` in the frames). The frames are still a valid before/after
  pair because both runs used the identical script and the defect is in how the *rendered*
  text is coloured.
- **cmd.exe only.** The same code path serves PowerShell and Unix profiles and the unit
  tests cover the Unix profile, but no PowerShell tab was captured in the walk.

## Handoff

Implementer: this session. No blockers. Two follow-ups worth their own packets: populate
`RowRoles` from the OSC 133 stream (§4.2 Phase 2), and paint the prompt-line background
(§8 item 6) across every row of a wrapped prompt.
