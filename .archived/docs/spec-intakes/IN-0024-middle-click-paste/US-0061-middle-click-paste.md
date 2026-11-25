# Work: Middle-click paste with a Terminal Settings switch

ID: US-0061
Intake: IN-0024
Created: 2026-09-11

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
- Spec Intake, when required: IN-0024

## Outcome

A middle click on the terminal pastes the clipboard through the existing paste path, unless
Settings › Terminal › Mouse › "Middle-Click Paste" (default on, persisted as
`terminal.json` `mouse.middle_click_paste`) is off or a program in mouse mode owns the
click (Shift overrides).

## Scope

- [x] In scope: `MouseConfig.middle_click_paste`; `TerminalSettings.middle_click_paste` with
  apply/persist mapping; the Mouse settings switch; `TerminalSession::is_mouse_mode` (macro +
  fake); `MouseInputs.middle_click_paste`; `MouseOutcome::Paste`; the view handler; tests; README
  and backend-doc updates.
- [x] Out of scope: pasting the X11 primary selection (Windows has none; the system clipboard
  is used); a key binding for the same action (Ctrl+Shift+V exists).

## Acceptance

- [x] Middle click on a terminal at the prompt pastes the clipboard text (unit: `Paste` outcome; GUI: pasted into cmd with the default, nothing pasted with the file set to false).
- [x] Settings › Terminal › Mouse shows "Middle-Click Paste", on by default (switch built like Copy on Select; persist round-trip tested); the off state forwards the click (unit-tested).
- [x] With a program in mouse mode, middle click reaches the program; Shift+middle pastes (unit-tested against the fake session's mode).
- [x] A `terminal.json` without the field loads with the switch on (serde default test).
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0024-middle-click-paste/high-level-design.md` — flow and seams.
- `docs/terminal-backend.md` §5 — `TerminalSession` method list (gains `is_mouse_mode`).
- `docs/agents/persistence.md` — `terminal.json` row is generic (no field list); no change.
- `README.md` § Terminal emulator — feature bullet.
- `crates/terminal-view/src/input/mouse.rs` module docs — the outcome list.

### Documentation Action

- Update required: `README.md` (feature bullet), `docs/terminal-backend.md` (`is_mouse_mode`
  in the trait listing).

Reason: the trait listing enumerates every `TerminalSession` method; the README lists mouse
features.

### Reconciliation

- `README.md` — Terminal emulator bullet names middle-click paste and its switch.
- `docs/terminal-backend.md` — `is_mouse_mode` in the `TerminalSession` listing.
- `docs/agents/persistence.md` — reviewed, unchanged: the `terminal.json` row does not list mouse fields.
- `crates/terminal-view/src/input/mouse.rs` — the outcome enum documents `Paste` in place.

## Context

- `MouseState::down` already returns `CopySelection` for the view to run an edit command;
  `Paste` follows the same shape (`apply_mouse_outcome` in `terminal_view/input.rs`).
- `paste_clipboard(session, origin, window, cx)` in `input/edit.rs` is the single paste path
  (sanitising, bracketed paste, broadcast fan-out, error toast).
- `TerminalModel::mode()` exposes `TermMode`; `is_alt_screen` is the precedent for a mode
  accessor on the trait (`crates/terminal/src/session.rs`, macro at the bottom of the file,
  fake in `test_support.rs`).
- Settings pattern: `crates/settings-ui/src/terminal/mouse.rs` (`SettingField::switch`).

## Plan

- [x] `MouseConfig.middle_click_paste` + default/round-trip tests; `TerminalSettings` field,
  `apply.rs`, `persist.rs`, persist tests.
- [x] `TerminalSession::is_mouse_mode` (trait, macro, fake) + `TerminalModel::is_mouse_mode`.
- [x] `MouseInputs.middle_click_paste`, `MouseOutcome::Paste`, the `down` branch, view handler
  (also clears the bell indicator like typed input).
- [x] Mouse tests: on (`Paste`), mouse mode (forwarded), Shift override (`Paste`), off (forwarded);
  the pre-existing middle-click test became the setting-off case.
- [x] Settings switch; README + backend doc; gate.

## Decisions

- None.

## Verification Plan

- `cargo test -p oneterm-settings -p oneterm-terminal -p oneterm-terminal-view`
- `pwsh scripts/ci-local.ps1`
- Manual: middle-click into `cmd` with text on the clipboard; toggle the switch.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, branch `feat/middle-click-paste`, 2026-09-11):

- `rtk proxy cargo test -p oneterm-settings -p oneterm-terminal -p oneterm-terminal-view -p oneterm-settings-ui`
  — all passed after the old `middle_click_forwards_to_session` test was turned into the
  setting-off case (with the default on, a middle click is a paste, not forwarded input).
- `rtk proxy cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `rtk proxy pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`, 1107 tests.
- GUI: `evidence/US-0061-gui-walk.md` (GUI: pasted into cmd with the default, nothing pasted with the file set to false).

Deviations from the HLD: none.

Gaps: the Settings window is a separate GPUI window; its switch was verified through the
shared `SettingField::switch` pattern and the persist round-trip rather than a screenshot
unless the walk notes say otherwise.

## Handoff

Implemented, gate green, not committed.
