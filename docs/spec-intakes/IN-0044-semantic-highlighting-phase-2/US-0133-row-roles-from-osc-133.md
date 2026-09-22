# Work: Row roles come from the shell's OSC 133 marks

ID: US-0133
Intake: IN-0044
Created: 2026-09-22

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

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: `IN-0044` — `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/IN-0044.md`

## Outcome

When the shell emits OSC 133, the semantic scanner is told each visible row's role instead
of guessing it, and the prompt sign of the command that just finished is tinted by its exit
code. The prompt regex still exists and still works, but it runs only for rows the shell
said nothing about.

## Scope

- [ ] In scope:
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
- [ ] Out of scope:
  - The prompt-line background — `US-0134`.
  - Tinting prompts further back in scrollback; the limit is stated in Acceptance and in the
    intake's Open Decisions.
  - Any change to `crates/vt`. The field this packet reads is already public
    (`SnapshotCell::semantic`), so the engine, its public-API snapshot and its package
    contract are untouched.
  - The regex fallback's own correctness — `BUG-0073`.

## Acceptance

- [ ] A row whose cells carry `Semantic::Prompt` is scanned in `PromptLine` state and a row
      whose cells carry `Semantic::Input` in `CommandMode`, with no prompt regex evaluated
      for either.
- [ ] A prompt whose cwd wraps over several rows is classified from the role of the row that
      **starts** the run, and gets the same result as the same prompt on one row.
- [ ] A continuation row whose own cells carry no mark does not change the run's
      classification.
- [ ] A row with no mark at all falls back to the prompt regex and is classified exactly as
      it is today — proved by the `BUG-0071` wrapped-prompt tests still passing unchanged.
- [ ] "No mark" is decided per row, not per session: a session that starts unmarked and
      becomes marked (or prints unmarked output between two marked prompts) gets the right
      answer for each row.
- [ ] After a command exits `0` the sign of its prompt is `Class::Success`; after a non-zero
      exit it is `Class::Error`; with no exit code reported it stays `Class::PromptSign`.
- [ ] **Stated limit:** only the most recent completed command block is tinted. A prompt
      further back in scrollback keeps an untinted sign, and the reason (the engine reports
      no row for `OSC 133;D`) is recorded in `docs/terminal-semantic-highlighting.md` §4.2
      rather than left in this packet.
- [ ] The rescan scope does not grow: the rows a frame derives roles for are the same dirty
      rows closed under wrap runs that the class and URL passes already scan, asserted by the
      existing `FrameStats` counters.
- [ ] `cargo test -p oneterm-highlight -p oneterm-terminal-view` passes.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

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

- [ ] Expose `Semantic` on the frame's cell, and write the role reduction with a
      hand-written table test first — no render change, no overlay wiring.
- [ ] Give `RowRoles` a per-row "no mark" answer and make `scan_into` ask it per row.
- [ ] Build the roles in the plan cache's existing rescan and hand them to the overlay.
- [ ] Synthetic mark streams: a fixture that builds a `Frame` with a chosen `Semantic` per
      cell, so a wrapped prompt, an unmarked continuation row, an unmarked session and a
      marked/unmarked mix are all testable without a live shell.
- [ ] The tint: locate the most recent completed `Input` block and substitute the sign's
      class.
- [ ] Reconcile §4.2, §11 and §13 Q1.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
