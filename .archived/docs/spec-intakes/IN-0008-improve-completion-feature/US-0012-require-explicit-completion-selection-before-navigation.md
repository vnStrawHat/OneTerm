# Work: Require explicit completion selection before navigation

ID: US-0012
Intake: IN-0008
Created: 2026-08-13

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: tiny
- Spec Intake, when required: `IN-0008`

## Outcome

The completion overlay requires explicit selection before navigation or acceptance: first Tab selects row 0, second Tab or Enter accepts it, and navigation keys are forwarded to the shell until a row has been selected.

## Scope

- [x] In scope: Tab, Enter, Up, Down, Ctrl+N, and Ctrl+P behavior while suggestions are visible.
- [x] In scope: regression coverage for the unselected and selected states.
- [x] Out of scope: suggestion matching/ranking, overlay visuals, mouse behavior, completion casing, history, and settings schema.

## Acceptance

- [x] With visible suggestions and no selection, first Tab selects item 0, consumes the key, and does not apply/dismiss the suggestion.
- [x] With a selected item, a subsequent Tab applies it through the existing acceptance path.
- [x] With a selected item, Enter applies it; without a selected item, Enter continues to the shell.
- [x] Up/Down and Ctrl+N/Ctrl+P return to the shell without changing selection while no item is selected.
- [x] After Tab selects item 0, Down/Ctrl+N move down and Up/Ctrl+P move up using existing clamped navigation.
- [x] When `accept_tab` is disabled, Tab continues to the shell and does not select an item.

## Documentation

### Owning Docs Reviewed

- `docs/auto-completion/05-ui.md` — key table and selection interaction contract.
- `docs/auto-completion/09-roadmap-risks.md` — run-first selection behavior.
- `docs/auto-completion/11-implementation-plan.md` — implemented terminal-view key milestone.
- `docs/agents/code-style.md` — localized changes and regression-test requirements.

### Documentation Action

Update required: revise the three auto-completion owning docs above to specify Tab-first selection and navigation forwarding before selection.

Reason: this is an intentional user-visible interaction contract change, not a restoration of the previous behavior.

### Reconciliation

Updated `docs/auto-completion/05-ui.md`, `09-roadmap-risks.md`, and `11-implementation-plan.md` with the two-step Tab interaction and selection-gated navigation behavior.

## Context

`CompletionController::recompute` already initializes `selected = None`, and dismiss also clears it. `LocalTerminalView::completion_handle_key` currently makes Up/Down create a selection and makes one Tab both select and accept. Returning `false` from this handler already forwards the key through the normal terminal keyboard path.

## Plan

- [x] Add focused, context-independent key-transition logic and tests.
- [x] Wire the view handler to select on first Tab, accept on later Tab/Enter, and gate navigation on existing selection.
- [x] Update owning auto-completion docs.
- [x] Run focused proof and required workspace quality gates.

## Decisions

No standalone decision record. This behavior is fully owned by the completion interaction contract and the user's explicit request.

## Verification Plan

- `cargo test -p oneterm-terminal-view completion`
- `cargo test -p oneterm-terminal-view`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-terminal-view completion` — passed, 25 tests (84 filtered out), including explicit first/second Tab, Enter, navigation forwarding, Ctrl aliases, and disabled-Tab transitions.
- `cargo test -p oneterm-terminal-view` — passed, 109 tests across 2 suites.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `git diff --check` — passed.
- Manual GUI/real-shell E2E was not run. Focused transition tests prove whether each key is consumed or forwarded, and controller tests prove navigation cannot create a selection before first Tab.

## Handoff

Single-session implementation; no handoff or blocker.
