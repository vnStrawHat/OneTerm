# Work: Prevent Unicode prefix-match crash

ID: BUG-0011
Intake: IN-0009
Created: 2026-08-14

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
- Spec Intake, when required: `IN-0009`

## Outcome

Terminal completion prefix matching handles arbitrary valid UTF-8 candidate and prefix strings without panicking, while preserving existing case-sensitive and ASCII-case-insensitive matching behavior.

## Scope

- [x] In scope: boundary-safe shared prefix matching and a focused regression for the reported `❯`/one-byte-prefix crash.
- [x] Out of scope: changing history capture, Unicode case folding, fuzzy matching, ranking, UI behavior, or persisted data.

## Acceptance

- [x] Matching a candidate beginning with `❯` against a different one-byte prefix returns `false` without panic.
- [x] Existing empty, matching, case-sensitive, and ASCII-case-insensitive prefix behavior remains covered and passing.
- [x] Completion crate tests and mandatory workspace quality gates pass.

## Documentation

### Owning Docs Reviewed

- `docs/auto-completion/02-data-sources.md` — history and catalogs provide the candidate strings.
- `docs/auto-completion/04-suggestion-engine.md` — family-aware prefix matching contract.
- `docs/agents/error-policy.md` — recoverable runtime data must not crash OneTerm.
- `docs/agents/code-style.md` — regression-test and Rust implementation rules.

### Documentation Action

No contract change.

Reason: the owning completion design already requires prefix matching and does not permit valid Unicode candidate data to terminate the process. The fix restores that contract without changing matching semantics.

### Reconciliation

The no-contract-change reason remains valid. No owning product contract changed; this packet, `IN-0009`, and its high-level design retain the crash context and fix boundary.

## Context

Before this fix, `prefix_match` checked byte lengths and then indexed `&haystack[..prefix.len()]`. Rust string indexing panics when the endpoint is not a character boundary, as in a one-byte typed prefix against a candidate beginning with the three-byte `❯` character. The helper is shared by catalog/token and whole-line-history matching.

## Plan

- [x] Add a regression test that reproduces the invalid UTF-8 boundary panic.
- [x] Replace unchecked slicing with the minimum boundary-checked standard-library operation.
- [x] Run focused and workspace verification, then reconcile this packet.

## Decisions

None; the existing prefix contract and Rust string invariants determine the fix.

## Verification Plan

- Focused: run the reported regression test in `oneterm-completion`.
- Unit: run `cargo test -p oneterm-completion`.
- Regression: run `cargo test --workspace` if practical and record any platform/tooling blocker.
- Required release gates: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Before the implementation change, `cargo test -p oneterm-completion prefix_match_rejects_non_character_boundary -- --nocapture` failed with the reported panic path, proving the regression reproduced the defect.
- After the change, the same focused command passed: 1 test, 56 filtered out.
- `cargo test -p oneterm-completion` — passed, 57 tests across 2 suites.
- `cargo test --workspace` — passed, 599 tests; 2 ignored and 76 filtered out across 41 suites.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `git diff --check` — passed.
- Manual GUI reproduction was not run. The reported panic is isolated in the gpui-free prefix helper and is covered directly plus through the full workspace test suite.

## Handoff

Single-session implementation; no handoff or blocker.
