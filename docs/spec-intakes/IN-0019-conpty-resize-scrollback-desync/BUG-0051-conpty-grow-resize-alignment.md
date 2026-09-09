# Work: Keep the grid aligned with conhost on a ConPTY grow-resize

ID: BUG-0051
Intake: IN-0019
Created: 2026-09-09

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
- Risk lane: normal (changes established resize behaviour of local sessions; no data, auth, or
  persistence impact)
- Spec Intake: IN-0019

## Outcome

After the viewport of a local ConPTY session grows (window maximize or split resize), shell
output and typed input appear on the rows the shell addresses; no stale rows or cursor fragments
remain after an alt-screen TUI exits; the prompt is the last visible row.

## Scope

- [ ] In scope: grow-resize policy for `SessionKind::Local` on Windows in `crates/terminal`
      (`TerminalModel::resize_grid`) and/or `crates/local-shell` (transport), a focused grid test,
      a manual repro, `docs/terminal-backend.md` update.
- [ ] Out of scope: the view crate; SSH sessions; shrink-resize semantics; vendored-fork patches
      unless policy (c) is chosen.

## Acceptance

- [ ] Repro script (`dir` to fill the screen, maximize, `echo desync-check`): the echo lands on
      the prompt row, not inside the listing (compare with
      `../IN-0018-rebuild-terminal-render-engine/evidence/rework2-bug2-resize-desync-new.png`).
- [ ] Owner flow (opentui-examples, maximize during the TUI, quit): no stale fragment, prompt on
      the last row.
- [ ] Shrink-resize and SSH resize behaviour unchanged (existing backend tests pass).
- [ ] Focused test in `crates/terminal` proves the cursor row after a grow with history matches
      the chosen policy.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` — session resize ordering (`pty_resize` before `resize_grid`).
- `docs/spec-intakes/IN-0019-conpty-resize-scrollback-desync/high-level-design.md` — policy
  candidates.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/US-0049-terminal-view.md`
  ("Acceptance Rework 2", bug 2) — triage evidence that the view is not the cause.
- `vendor/README.md` — how vendored-fork patches are made, if policy (c) is chosen.

### Documentation Action

Update required: `docs/terminal-backend.md` must describe the chosen grow policy for local
sessions and why it differs from alacritty's default.

Reason: resize semantics are an operator-visible backend contract.

### Reconciliation

Before completion, list the docs changed.

## Context

- `vendor/alacritty_terminal/src/grid/resize.rs:43-69` (`grow_lines`) pulls `from_history =
  min(history_size, lines_added)` rows and moves the cursor down by that amount.
- conhost keeps the viewport top on `ResizePseudoConsole` and addresses output with absolute
  `CUP` in its own coordinates (observed in the raw PTY dump during triage: `ESC[34;65H` while
  the grid prompt was on row 51).
- Both engines (old and new view) render the same picture; the desync is terminal state.

## Plan

- [x] Owner picked policy (a): no scrollback pull on local grow (DEC-0008).
- [ ] Implement behind `SessionKind::Local`, Windows only.
- [ ] Focused grid test + manual repro screenshots into `evidence/`.
- [ ] Update `docs/terminal-backend.md`.

## Decisions

- `docs/decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md`.

## Verification Plan

- Focused: `cargo test -p oneterm-terminal resize`
- Unit: `cargo test --workspace`
- Integration: `cargo test -p oneterm-local-shell`
- E2E: manual Windows repro (both scripts above) with `PrintWindow` screenshots.
- Gate: `pwsh scripts/ci-local.ps1`

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

## Handoff

Policy decided (DEC-0008); implementation in progress.
