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
- [x] Reopened (acceptance rework)
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
      **Qualified: when the shell emits `OSC 133;D`.** `last_exit_code` is written from
      `ShellMark::OutputEnd` and nothing else, and no integration OneTerm ships emits `D`
      (`cmd` and `zsh` emit `A`+`B`, `bash` and the SSH bootstrap `A`, PowerShell none), so
      out of the box the sign stays `Class::PromptSign`. The mechanism is proved by unit
      tests and by a hand-instrumented PowerShell tab; completing the emitters is `US-0136`
      and is deliberately not in this packet.
- [x] **Stated limit:** only the most recent completed command block is tinted. A prompt
      further back in scrollback keeps an untinted sign, and the reason (the engine reports
      no row for `OSC 133;D`) is recorded in `docs/terminal-semantic-highlighting.md` §4.2
      rather than left in this packet.
- [x] The rescan scope does not grow **for the URL pass**: `url_rows_scanned` is still the
      dirty rows closed under wrap runs (`US-0092`), asserted by
      `roles_ride_the_class_rescan`. **Changed by the rework:** the *semantic* pass now also
      covers the one logical line after each changed run, because a line's role is read from
      the region its predecessor started in. The chain is exactly one line long and the two
      passes were split so only this one pays for it; `class_rows_scanned` is asserted at
      `3` where it used to be `2`.
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
- `crates/terminal-view/src/render/row_plan.rs` — `line_head_region()` (the raw reading)
  and `line_role()` (the transition rule), `class_rows_into` fills a per-row role **and**
  head region for every row of each run, `classify()` applies the tint.
- `crates/terminal-view/src/render/plan_cache.rs` — `roles`/`roles_cur` and `heads` beside
  `class_prev`/`class_cur` (same delta-and-swap, same rotation on scroll), a `scan_class`
  set that is `scan` plus one logical line forward, the URL and class passes split so only
  the second pays for it, `update_tint()`, and the exit code in the `Unchanged` guard.
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

`crates/terminal-view/src/render/row_plan.rs`, rework:
`an_a_only_shell_does_not_turn_its_output_into_prompts`,
`a_full_a_b_c_d_shell_is_classified_exactly`,
`the_viewports_first_line_is_never_a_prompt`, and
`output_left_tagged_input_by_a_shell_without_osc_133_c_is_unmarked` extended to assert that
`cmd`'s prompt still works. `an_a_only_shell_paints_no_band` for `US-0134`.
`crates/terminal-view/src/render/plan_cache.rs`, rework:
`a_prompt_at_the_top_of_the_viewport_is_unmarked_and_stays_so` (a real `OSC 133` byte stream
fed to the engine, scrolled back and forward twice),
`an_exit_code_on_an_unchanged_frame_is_not_dropped`.

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
pre-existing `BUG-0071` case, which still pass unchanged.

**Rework frame.** `us-0133-a-only-no-flood.png` re-takes the verification's `a-only-flood`
scene on the reworked build: a script writes exactly what OneTerm's bash `PROMPT_COMMAND`
and its SSH bootstrap put on the wire — `OSC 7`, then `OSC 133;A`, and nothing else — and
then five ordinary output lines. None of them carries a band, and all five are classified as
output: `ERROR` and `failed` red, `100%` a number, `-o` an option, the URL underlined, and
`done in 12s` with no invented prompt sign. The `cmd` prompts above and below it — `A`+`B`,
so the region does transition — still carry theirs. Compare
`US-0133-US-0134-verify-a-only-flood.png`, the same scene before the fix, where every one of
those rows was a banded prompt line.

Two notes from re-running the walk, because they cost time: the stripe on the row the cursor
sits on is **not** the band (it is in the pre-`US-0134` frames too), and OneTerm's `cmd` tab
has an unmarked blank line above its first prompt, so that prompt does have a predecessor
and is correctly marked. The viewport's genuine top line was checked against the library
instead, where it is deterministic.

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

### Acceptance rework after independent verification (2026-09-22)

Verified on `0dfd2070` by an independent agent; report in
`evidence/US-0133-US-0134-verify.md`. **REJECT**, one blocker plus one dead acceptance item.
What changed here:

- **MAJ-1 (blocker) — the A-only flood.** The OSC 133 region is a sticky cell attribute, and
  OneTerm's bash `PROMPT_COMMAND` (`crates/core/src/config/shell.rs`) and its SSH bootstrap
  (`crates/ssh/src/session.rs`, on by default for **every** remote session) emit `OSC 133;A`
  and nothing else. Every line printed afterwards was therefore `Prompt`-tagged, so the
  first reading — "the role is the region of the line's first marked character" — turned a
  whole screen of output into prompt lines, with invented signs, command colouring and a
  US-0134 band on every row. The packet's own Gaps made this invisible by asserting that
  `cmd.exe` was "the one local shell OneTerm gives OSC 133 to", which is wrong; that
  sentence is replaced below and the real table is now in §4.2.

  **Rule as implemented:** a logical line is a marked `Prompt` only when the region
  **transitions into** `Prompt` — that is, when the previous logical line's own
  first-marked-character region is *known* and is not `Prompt`. No previous line inside the
  viewport is *unknown*, not "not a prompt", so the viewport's top line is never a marked
  prompt. The first-marked-character rule and the `Input`-headed rule are unchanged; the
  verification confirmed the alternative ("`Input`-headed and the previous line was a
  prompt → `Command`") regresses every `cmd` command, so it was not taken.

  Consequences, all asserted: `A`-only shells get no marked prompt and no band, and fall
  back to the regex, which is what they did before the fast path existed; `A`/`B` (`cmd`,
  `zsh`) still get the prompt, because it follows an `Input`-tagged line; `A`/`B`/`C`/`D`
  is exact. A line's role now depends on the line above it, so the **semantic** rescan
  covers one logical line more than the dirty runs. The URL pass was split out of the shared
  loop so it keeps the narrower `US-0092` bound unchanged.

- **MAJ-2 — the tint cannot fire out of the box.** No shipped integration emits `OSC 133;D`.
  The acceptance item above is qualified rather than dropped: the mechanism is real, tested
  and demonstrated, and completing the emitters is `US-0136`.

- **MIN-1** — an exit code arriving on an `SnapshotUpdate::Unchanged` frame was dropped
  until something else dirtied a row. The early return now also compares the exit code
  (`an_exit_code_on_an_unchanged_frame_is_not_dropped`).
- **MIN-2** — `last_completed_prompt` no longer builds a `Vec` of every prompt run per
  frame; two locals give the same answer.
- **MIN-3** — `marked_sign`'s last-resort rule skipped only ASCII spaces, so an unknown sign
  glyph followed by a tab or a no-break space tagged an invisible cell. It uses
  `char::is_whitespace`.
- **MIN-4** — §4.2 cited `crates/core/src/terminal/osc.rs`, which has not existed since
  `IN-0029`; §4.2 and §13 Q1 now say `crates/vt`, and §4.2 carries the per-shell mark table.

Not changed, with reasons: the `Input`-headed rule (the verification confirmed it is right);
`self.tint`'s row range is not rotated by `shift()` (over-invalidation only, never under);
`prompt_line_bg`'s explicit override ignores `reverse_video` (unreachable — nothing ships an
override).

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
- **The viewport's top line is never a marked prompt**, because its predecessor is off
  screen and therefore unknown. A prompt scrolled to the top edge loses its band. The answer
  is deterministic rather than flickering (asserted by
  `a_prompt_at_the_top_of_the_viewport_is_unmarked_and_stays_so`), and reading the row above
  the viewport needs an engine read path the view does not have — the same viewport-only
  limit §13 Q5 already states for the class and URL passes.
- **The exit-code tint is unreachable with every shipped integration** (MAJ-2). `US-0136`.
- The `cfg(unix)` half of shell integration (bash `PROMPT_COMMAND`, zsh `PS1`) and a live
  SSH remote are not exercisable on this machine. The mark streams they put on the wire are
  reproduced exactly — as synthetic frames in the unit tests and as raw bytes in the GUI
  walk — but no live bash and no real remote was driven.

## Handoff

Implemented; `US-0134` builds the prompt-line background on the roles this packet derives.
