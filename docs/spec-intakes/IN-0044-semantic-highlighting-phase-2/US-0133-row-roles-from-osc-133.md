# Work: Row roles come from the shell's OSC 133 marks

ID: US-0133
Intake: IN-0044
Created: 2026-09-22

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

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: `IN-0044` — `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/IN-0044.md`

## Outcome

When the shell emits OSC 133, the semantic scanner is told each visible row's role instead
of guessing it, and the prompt sign of the command that just finished is tinted by its exit
code. The prompt regex still exists and still works, but it runs only for rows the shell
said nothing about.

## Scope

- [x] In scope:
  - `crates/terminal-view/src/render/frame.rs` — expose the snapshot cell's
    `Semantic` on the frame's `Cell`/`FrameRow`, which is the only field of
    `SnapshotCell` the view does not read yet.
  - `crates/terminal-view/src/render/plan_cache.rs` — derive roles for the rescan range
    beside `fill_wraps` / `url_masks_rows_into` / `class_rows_into`, and keep them for the
    frame.
  - `crates/terminal-view/src/highlight/overlay.rs` — a setter for `row_roles`, and the
    per-row "no mark" question replacing the per-overlay `role.is_empty()` at line 74.
  - `crates/highlight/src/role.rs` — whatever `RowRoles` needs to say "this row has no
    mark" (the design leaves the representation to this packet).
  - `crates/highlight/src/scanner/prompt.rs` — the sign's class becomes `Success`/`Error`
    when an exit code applies to that prompt's block.
  - `docs/terminal-semantic-highlighting.md` §4.2, §11 and §13 Q1.
- [x] Out of scope:
  - The prompt-line background — `US-0134`.
  - Tinting prompts further back in scrollback; the limit is stated in Acceptance and in the
    intake's Open Decisions.
  - Any change to `crates/vt`. The field this packet reads is already public
    (`SnapshotCell::semantic`), so the engine, its public-API snapshot and its package
    contract are untouched.
  - The regex fallback's own correctness — `BUG-0073`.

## Acceptance

- [x] A row whose cells carry `Semantic::Prompt` is scanned in `PromptLine` state and a row
      whose cells carry `Semantic::Input` in `CommandMode`, with no prompt regex evaluated
      for either. **Qualified:** `Semantic::Input` reaches `CommandMode` through the prompt
      line's own `OSC 133;B` boundary, which is where a typed command lives. A logical line
      that *starts* in `Input` is reported unmarked instead — see Gaps and
      `docs/terminal-semantic-highlighting.md` §4.2 for `cmd.exe`'s reason.
- [x] A prompt whose cwd wraps over several rows is classified from the role of the row that
      **starts** the run, and gets the same result as the same prompt on one row.
- [x] A continuation row whose own cells carry no mark does not change the run's
      classification.
- [x] A row with no mark at all falls back to the prompt regex and is classified exactly as
      it is today — proved by the `BUG-0071` wrapped-prompt tests still passing unchanged.
- [x] "No mark" is decided per row, not per session: a session that starts unmarked and
      becomes marked (or prints unmarked output between two marked prompts) gets the right
      answer for each row.
- [x] After a command exits `0` the sign of its prompt is `Class::Success`; after a non-zero
      exit it is `Class::Error`; with no exit code reported it stays `Class::PromptSign`.
- [x] **Stated limit:** only the most recent completed command block is tinted. A prompt
      further back in scrollback keeps an untinted sign, and the reason (the engine reports
      no row for `OSC 133;D`) is recorded in `docs/terminal-semantic-highlighting.md` §4.2
      rather than left in this packet.
- [x] The rescan scope does not grow: the rows a frame derives roles for are the same dirty
      rows closed under wrap runs that the class and URL passes already scan, asserted by the
      existing `FrameStats` counters.
- [x] `cargo test -p oneterm-highlight -p oneterm-terminal-view` passes.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-semantic-highlighting.md` §4.2 (the OSC 133 fast path and the `RowRoles`
  struct), §11 (Phase 2's scope), §13 Q1 (row-level roles, the event-to-role table, and the
  `OutputStart` correction), §13 Q5 (the scan rides the existing dirty decision) — the
  contract this packet implements.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/BUG-0071-semantic-highlight-unstable-on-wrapped-lines.md`
  — its audit table and Gaps; this packet is its first named follow-up, and its rule that the
  run's **first** row supplies the role is the rule the derivation must not break.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`
  — the plan cache's phase-2 rescan, which the role derivation joins.
- `docs/terminal-backend.md` — how OSC 133 reaches the view and what the backend keeps
  (`prompt_count`, `last_exit_code`).

### Documentation Action

Update required:

- `docs/terminal-semantic-highlighting.md` §4.2 and §13 Q1 — the source of `RowRoles`. Both
  specify rebuilding it "in the pump ... alongside the grid snapshot" from the event stream.
  The engine `IN-0029` shipped stores the region on the cell
  (`crates/vt/src/cell.rs:83`, `SnapshotCell::semantic`), so the roles are derived from the
  frame instead, and survive scroll and reflow because the content does. The event-to-role
  table stays correct as a description of what the marks mean; what changes is who keeps
  the bookkeeping, which is now nobody.
- `docs/terminal-semantic-highlighting.md` §4.2 — the exit-code tint's reach, and why.
- `docs/terminal-semantic-highlighting.md` §11 — Phase 2 stops being "planned".

Reason: the contract names a mechanism the current engine makes unnecessary. Implementing
it as written would rebuild, by hand, a fact the snapshot already carries.

### Reconciliation

Changed: `docs/terminal-semantic-highlighting.md` §4.2 (rewritten: where the roles come
from, the four-way reduction table, the mixed-region rule and the `input_at` boundary, the
`Input`-headed exception, the tint and its stated limit), §11 Phase 2 (shipped, with what
differs from the plan), §13 Q1 (the "rebuilt in the pump" clause superseded, the glyph probe
demoted to the fallback, the per-row `exit_code` column dropped with its reason).

No change needed: §8 (the render-path integration already describes the wrap-run-closed
rescan this pass joins), §9 (the merge policy is untouched), §10 (the rescan bound is
unchanged — `US-0135` owns it), §13 Q5 (the scan still rides the existing dirty decision).

## Context

- `SemanticOverlay::row_roles` is assigned by nothing today, so `scan_into`
  (`crates/terminal-view/src/highlight/overlay.rs:67-80`) always takes the
  `role.is_empty()` branch and passes `RowRole::Output`.
- `Semantic` is two bits of `Cell` (`crates/vt/src/cell.rs:83`), written from the cell
  template by `dispatch.rs:1014`, copied into `SnapshotCell::semantic` by
  `snapshot/row.rs:105`. Because it is part of the cell, it moves with its content: no
  anchor, no replay, and nothing to reconcile after a reflow.
- The engine also registers an `AnchorKind::Mark` per `PromptStart`/`OutputStart`
  (`dispatch.rs:1036`, bounded by `MARK_MAX`), but `Terminal` exposes no accessor for the
  anchor list and `Anchors` is not re-exported, so the anchors are not a source the view can
  use. They are not needed.
- `OSC 133;D`'s exit code reaches `crates/terminal/src/backend/state.rs:59` as
  `last_exit_code` and carries no row.

## Plan

- [x] Expose `Semantic` on the frame's cell, and write the role reduction with a
      hand-written table test first — no render change, no overlay wiring.
- [x] Give `RowRoles` a per-row "no mark" answer and make `scan_into` ask it per row.
- [x] Build the roles in the plan cache's existing rescan and hand them to the overlay.
- [x] Synthetic mark streams: a fixture that builds a `Frame` with a chosen `Semantic` per
      cell, so a wrapped prompt, an unmarked continuation row, an unmarked session and a
      marked/unmarked mix are all testable without a live shell.
- [x] The tint: locate the most recent completed `Input` block and substitute the sign's
      class.
- [x] Reconcile §4.2, §11 and §13 Q1.

## Decisions

None expected. The two choices this packet makes — deriving roles from the snapshot rather
than the event stream, and tinting only the most recent block — are recorded in
`docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/high-level-design.md` and in the
contract itself; neither binds future work to a rule it could not see there.

## Verification Plan

- Unit: the role reduction table; the wrapped-prompt run; the unmarked continuation row; the
  per-row absent case; the tint's two signs and its absent case.
- Integration: plan-cache tests that the role pass scans the same rows as the class pass and
  that a role change replans exactly the rows of its run.
- E2E: a Windows `fast-dev` walk with a PowerShell tab carrying an OSC 133 prompt function
  and a `cmd` tab without one, before and after frames under `evidence/`.
- Platform: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What was built

- `crates/highlight/src/role.rs` — `RowRoles.role` is `Vec<Option<RowRole>>`: `None` is
  "this row carries no mark", per row. `last_completed_prompt(&wraps)` names the prompt run
  the exit code belongs to. The per-row `exit_code` column is gone (the engine attaches no
  row to `OSC 133;D`).
- `crates/highlight/src/scanner/mod.rs` — `scan_line{,_into}` take
  `role: Option<RowRole>` and `input_at: Option<usize>`. `None` is the regex fallback;
  `Some(Output)` is output mode with **no** regex.
- `crates/highlight/src/scanner/prompt.rs` — `marked_sign()` (the sign is the last prompt
  glyph, else the last non-space character, inside `0..input_at`) and the exported
  `tint_prompt_sign()`.
- `crates/terminal-view/src/render/frame.rs` — `Cell::semantic`, `append_text_into` also
  emits the per-char region, `FrameBuilder::mark()` for synthetic mark streams.
- `crates/terminal-view/src/render/row_plan.rs` — `line_role()` (the reduction),
  `class_rows_into` fills a per-row role for every row of each run, `classify()` applies the
  tint.
- `crates/terminal-view/src/render/plan_cache.rs` — `roles`/`roles_cur` beside
  `class_prev`/`class_cur` (same delta-and-swap, same rotation on scroll), `update_tint()`.
- `crates/terminal/{model,session,backend/state,test_support}.rs` —
  `TerminalInfo::last_exit_code`, wired in `terminal_view/render.rs` and suppressed while
  the viewport is scrolled.

### Commands

- `cargo test -p oneterm-highlight` — 94 passed.
- `cargo test -p oneterm-terminal-view` — 378 passed, 3 ignored.
- `cargo test --workspace` — green.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `python scripts/vt-public-api.py --check --no-doc` — "public API surface unchanged".
- `pwsh scripts/ci-local.ps1` — see the final entry below.

### Tests

`crates/highlight/src/role.rs`: `absence_is_per_row_not_per_viewport`,
`last_completed_prompt_is_the_one_above_the_newest`,
`a_wrapped_prompt_is_tinted_as_a_whole_run`,
`a_running_command_tints_the_previous_prompt_not_its_own`,
`two_prompts_on_adjacent_rows_are_two_runs`, `a_single_prompt_on_screen_is_not_tinted`.

`crates/highlight/src/scanner/scanner_tests.rs`:
`a_marked_prompt_with_a_space_in_it_still_finds_its_sign`,
`a_marked_prompt_without_a_boundary_uses_the_whole_line`,
`a_marked_prompt_with_an_unknown_glyph_uses_its_last_character`,
`a_marked_command_line_is_command_mode`, `marked_output_never_runs_the_prompt_regex`,
`the_exit_code_tints_the_sign`, `an_untinted_sign_keeps_its_class`.

`crates/terminal-view/src/render/row_plan.rs`:
`a_wrapped_prompt_takes_its_role_from_the_marks`,
`a_marked_prompt_is_classified_the_same_wrapped_or_not`,
`an_unmarked_continuation_row_keeps_the_runs_role`,
`a_session_with_no_marks_falls_back_to_the_regex`,
`marks_appearing_mid_session_are_decided_per_row`,
`output_left_tagged_input_by_a_shell_without_osc_133_c_is_unmarked`,
`a_marked_output_row_is_not_read_as_a_prompt`.

`crates/terminal-view/src/render/plan_cache.rs`: `roles_ride_the_class_rescan`,
`roles_are_read_from_the_marks`, `a_role_change_replans_its_run`,
`the_tint_lands_on_the_completed_block`, `a_single_prompt_is_never_tinted`.

### GUI walk (Windows, `fast-dev`)

Frames under `evidence/`, from one `target/fast-dev/oneterm.exe` run:

- `us-0133-cmd-marked-prompt-wrapped.png` — the default local shell. `cmd.exe` **does** get
  shell integration from OneTerm (`CMD_OSC7_PROMPT`, `crates/core/src/config/shell.rs`,
  emits `OSC 133;A` and `;B`), which is the opposite of what the packet assumed. The prompt
  wraps, its whole cwd is `Path`, the sign comes from the `;B` boundary, `echo` and `dir`
  are `Command` and `/b` is `Option` — and the output below it, which `cmd` leaves tagged
  `Input` because it never emits `;C`, is classified as output (`'notacommand'` as a
  string), not as a command line. That is the `Input`-headed rule doing its job.
- `us-0133-powershell-osc133-prompt.png` — a PowerShell tab whose prompt function emits the
  full `A`/`B`/`C`/`D` set. `PS C:\...>` gets its sign from the boundary; the in-row glyph
  probe §13 Q1 originally specified stops at the space after `PS` and would have found none.
- `us-0133-tint-success.png` — `Write-Output "C:\src -> C:\dst"` under `OSC 133;C` is drawn
  as output (path, arrow) and **not** as a prompt, and the prompt of the block that exited
  `0` carries a green sign.
- `us-0133-tint-error.png` — after `cmd /c exit 3` the same sign is red, and the previous
  prompt's sign has gone back to untinted: only the most recent completed block is tinted.

The regex-fallback path is the unmarked one and is covered by the unit tests plus every
pre-existing `BUG-0071` case, which still pass unchanged; no local shell OneTerm ships
leaves a prompt unmarked, so it has no frame of its own.

### Deviations from the plan, and why

1. **`SemanticOverlay` does not hold `RowRoles`.** The roles are frame-derived state with
   exactly the lifetime of the URL masks and the classes, so the plan cache owns them and
   hands the scanner the role of the line it is scanning. The overlay keeps the one fact
   that is *not* in the frame: `last_exit_code`.
2. **The tint is applied in `classify()`, not inside the scanner.** Which prompt the code
   belongs to is a whole-viewport fact, and the scan is deliberately restricted to the
   rescan range; deciding it inside the scan would have made the roles and the classes
   mutually dependent. The rule itself still lives in `scanner/prompt.rs`
   (`tint_prompt_sign`), and a tint change dirties only the rows it moves off and on to —
   with no rescan, asserted by `the_tint_lands_on_the_completed_block`.
3. **The sign on a marked line is found from the `OSC 133;B` boundary, not by an in-row
   glyph probe** (§13 Q1's original answer). The probe stops at the first space, so it
   found no sign at all in `PS C:\src>` or `[user@host ~]$` — i.e. the fast path would have
   been *worse* than the regex for the two most common Windows prompts. The boundary is
   already in the cells; using it is cheaper than the probe and exact.

### Gaps

- **`RowRole::Command` is not produced by the derivation.** A logical line that starts in
  `Semantic::Input` is reported unmarked, because `cmd.exe`'s built-in `PROMPT` emits
  `OSC 133;A`/`;B` and never `;C`, leaving every output line tagged `Input`. Command mode is
  still entered — through the prompt line's own boundary, which is where a typed command
  actually lives — so the acceptance clause about `Semantic::Input` rows is met *within* a
  prompt line rather than as a standalone row role. The variant stays in the API for a shell
  that emits a multi-line command region after a `;C`.
- **The tint reaches one prompt.** Stated in §4.2 and asserted; history needs an engine API
  change (`IN-0044` Open Decisions).
- **The tint is suppressed while scrolled up** (`display_offset > 0`), because the newest
  prompt on screen is then not the live one. Not in the packet's original acceptance; it is
  the honest reading of "the most recent completed block".
- **Roles are derived inside the rescan, but the tint decision reads the whole viewport.**
  `roles` is authoritative for every row (the same contract as `mask_prev`/`class_prev`), so
  this costs no extra scanning; `roles_ride_the_class_rescan` asserts the rescan scope did
  not grow.
- The `cfg(unix)` half of shell integration (bash/zsh `PROMPT_COMMAND`/`PS1`) is not
  exercisable on this machine; the marked paths are proved with synthetic mark streams and
  with `cmd.exe`, which is the one local shell OneTerm gives OSC 133 to.

## Handoff

Implemented; `US-0134` builds the prompt-line background on the roles this packet derives.
