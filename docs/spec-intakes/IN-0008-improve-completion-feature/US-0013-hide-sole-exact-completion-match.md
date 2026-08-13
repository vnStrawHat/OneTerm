# Work: Hide sole exact completion match

ID: US-0013
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

A completion overlay is not shown when its only suggestion already exactly equals the text the user entered for that suggestion; actionable or ambiguous suggestions remain visible.

## Scope

- [x] In scope: sole-result suppression using `Suggestion::replace_from` and cursor position.
- [x] In scope: command/token and whole-line history replacement ranges.
- [x] Out of scope: engine ranking/dedup, multi-result filtering, casing acceptance, key interaction, and settings.

## Acceptance

- [x] Input `ls` with sole suggestion `ls` produces no visible completion overlay.
- [x] A sole prefix extension such as input `l` and suggestion `ls` remains visible.
- [x] A sole Cmd/PowerShell case-different suggestion such as input `LS` and suggestion `ls` remains visible for exact-casing acceptance.
- [x] Multiple suggestions remain visible even when one exactly equals the typed text.
- [x] Whole-line history uses its replacement boundary correctly and does not regress existing filtered-history completion.

## Documentation

### Owning Docs Reviewed

- `docs/auto-completion/04-suggestion-engine.md` — suppression and empty-results contract.
- `docs/auto-completion/05-ui.md` — overlay lifecycle.
- `docs/agents/code-style.md` — regression-test requirement.

### Documentation Action

Update required: add the sole exact-match suppression rule to the suggestion-engine and UI lifecycle contracts.

Reason: this intentionally changes when an otherwise valid result is presented.

### Reconciliation

Updated `docs/auto-completion/04-suggestion-engine.md` and `05-ui.md` with the sole byte-exact result suppression rule and its preserved actionable cases.

## Context

The engine can return catalog candidates equal to a fully typed token. `CompletionController::recompute` owns overlay-visible results and already has line/cursor context plus each suggestion's replacement boundary, making it the smallest correct suppression seam.

## Plan

- [x] Add focused exact/prefix/case/multiple-result regression tests.
- [x] Add a replacement-range helper and suppress only a sole byte-exact result.
- [x] Update owning docs.
- [x] Run focused proof and required quality gates.

## Decisions

No separate decision record. Byte-exact equality preserves the accepted behavior that case-insensitive Windows-family suggestions apply their displayed casing.

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

- `cargo test -p oneterm-terminal-view completion` — passed, 27 tests (84 filtered out), including sole exact, prefix, case-different, and multi-result regressions.
- `cargo test -p oneterm-terminal-view` — passed, 111 tests across 2 suites.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `git diff --check` — passed.
- Manual GUI E2E was not run; controller-level visibility tests cover the requested overlay condition.

## Handoff

Single-session implementation; no blocker.
