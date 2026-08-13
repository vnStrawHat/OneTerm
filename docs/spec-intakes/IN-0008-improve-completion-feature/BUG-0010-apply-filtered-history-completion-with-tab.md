# Work: Apply filtered history completion with Tab

ID: BUG-0010
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

- Change type: bug
- Risk lane: tiny
- Spec Intake, when required: `IN-0008`

## Outcome

When Cmd/PowerShell in-session history contains `cd Project`, accepting the displayed suggestion after `cd `, `cd p`, or `cd P` produces exactly `cd Project`. Unix remains case-sensitive, so `cd p` does not match `cd Project`, while `cd P` appends the exact remainder.

## Scope

- [x] In scope: controller acceptance-prefix calculation for suggestions with a non-token replacement boundary.
- [x] In scope: exact-casing correction for case-insensitive Cmd/PowerShell prefix matches using terminal Backspace bytes.
- [x] In scope: focused regression tests covering Cmd, PowerShell, and Unix case behavior.
- [x] Out of scope: fuzzy/non-prefix acceptance, path completion, history persistence, ranking, redaction, overlay presentation, and shell-native completion.

## Acceptance

- [x] With Cmd-family history `cd Project`, `cd p` displays the history candidate and acceptance emits one Backspace followed by `Project`, producing exactly `cd Project`.
- [x] PowerShell follows the same case-insensitive exact-suggestion behavior as Cmd.
- [x] With Cmd/PowerShell history `cd Project`, `cd P` appends `roject` without unnecessary Backspace bytes.
- [x] Unix remains case-sensitive: `cd p` does not suggest `cd Project`; `cd P` accepts by appending `roject`.
- [x] Existing token completion such as `di` -> `dir` still accepts as `r`, and fuzzy/non-prefix acceptance remains disabled by default.

## Documentation

### Owning Docs Reviewed

- `docs/auto-completion/04-suggestion-engine.md` — accepted append-only remainder contract.
- `docs/auto-completion/05-ui.md` — Tab accepts the selected suggestion when `accept_tab` is enabled.
- `docs/auto-completion/09-roadmap-risks.md` — fuzzy acceptance remains disabled by default.
- `docs/PROJECT.md` — brownfield project context and verification gaps.
- `docs/agents/code-style.md` — localized changes and regression-test requirements.

### Documentation Action

Update required: `docs/auto-completion/04-suggestion-engine.md` and `docs/auto-completion/05-ui.md` must describe the accepted case-preserving behavior for case-insensitive shell families. `docs/auto-completion/09-roadmap-risks.md` must distinguish bounded case correction from fuzzy replacement.

Reason: the user's acceptance feedback intentionally changes the earlier append-only expectation for case-only differences on Cmd/PowerShell while retaining the prohibition on fuzzy/non-prefix replacement.

### Reconciliation

Updated `docs/auto-completion/04-suggestion-engine.md`, `05-ui.md`, `09-roadmap-risks.md`, and `11-implementation-plan.md` to distinguish bounded Windows-family casing correction from fuzzy replacement and preserve Unix exact-case behavior.

## Context

`Engine::gather_history_whole_line` matches the entire line prefix and emits `Suggestion { replace_from: 0 }`. Before this fix, `CompletionController` discarded that boundary and used only `ParsedLine::token` for acceptance; therefore `cd p` was compared against full suggestion `cd Project`, failed the safe-prefix check, and Tab dismissed without writing. The empty-token fallback happened to use `cd `, which explains why that case already succeeded. Acceptance now slices the typed input from the selected suggestion's replacement boundary while retaining `typed_prefix` for overlay anchoring.

## Plan

- [x] Replace the string-only remainder with an internal acceptance byte payload capable of bounded Backspace correction.
- [x] Add regression tests for Cmd, PowerShell, and Unix exact-casing behavior.
- [x] Update the owning auto-completion contracts.
- [x] Run focused tests and all required workspace quality gates.

## Decisions

The user's acceptance feedback is authoritative for this work packet: case-insensitive Windows-family matches reproduce the displayed suggestion exactly; Unix remains case-sensitive. No separate cross-project decision record is needed because the behavior is owned by the auto-completion contract.

## Verification Plan

- `cargo test -p oneterm-terminal-view completion`
- `cargo test -p oneterm-completion`
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

- `cargo test -p oneterm-terminal-view completion` — passed, 21 tests (84 filtered out), including Cmd, PowerShell, and Unix case-behavior regressions.
- `cargo test -p oneterm-completion` — passed, 56 tests across 2 suites.
- `cargo test -p oneterm-terminal-view` — passed, 105 tests across 2 suites.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `git diff --check` — passed.
- Manual GUI/real-shell E2E was not run. Controller proof validates the exact PTY payload (`0x7f` + `Project`) and the view writes that payload unchanged through the existing session interface.

## Handoff

Single-session implementation; no handoff or blocker.
