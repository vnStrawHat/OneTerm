# Work: Compile native crash helpers only where used

ID: BUG-004
Intake: IN-0001
Created: 2026-08-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Keep operational status in `harness.db`.

## Classification

- Change type: bug
- Risk lane: tiny
- Spec Intake, when required: existing crash-reporting capability `IN-0001`; no new intake required.

## Outcome

`oneterm-app` compiles with warnings denied on targets where hexadecimal native crash context and Windows-only filesystem test helpers are not used.

## Scope

- In scope: conditionally compile the hexadecimal buffer helper and Windows-only test import on the targets that use them.
- Out of scope: changing crash report content, callback behavior, persistence, platform coverage, or dependencies.

## Acceptance

- `push_hex` remains available for tests, Windows exception formatting, and macOS exception formatting, but does not produce dead-code diagnostics on other production targets.
- The filesystem test import remains available to the Windows simulation test without producing an unused-import diagnostic on other test targets.
- Focused app tests and all required workspace release gates pass with warnings denied.

## Documentation

### Owning Docs Reviewed

- `docs/PROJECT.md` — crash reports remain local diagnostics and the workspace release gates are mandatory.
- `docs/crash-reporting.md` — native callback capture content, supported platform coverage, and verification contract.
- `docs/spec-intakes/IN-0001-persistent-crash-reporting-and-recovery/US-002-redact-user-paths-and-capture-native-crashes.md` — original callback-safety acceptance and platform-test intent.
- `docs/agents/code-style.md` — warning-free Clippy, localized changes, and test conventions.
- `docs/agents/error-policy.md` — native capture remains best effort without changing runtime failure handling.

### Documentation Action

No contract change: the accepted native crash behavior and supported platform formatting remain unchanged; this change only aligns helper compilation with existing target-specific call sites.

Reason: `push_hex` is used in Windows/macOS formatters and tests, while `std::fs` is used only by the Windows simulation test. Target-specific compilation removes warnings without altering behavior.

### Reconciliation

No owning contract changes were required. The implementation only narrows compilation of existing helpers to the test and platform configurations that already call them; crash capture behavior and report content are unchanged.

## Context

The Linux non-test build has no `push_hex` caller because its signal context is decimal. Non-Windows test builds do not compile the simulated exception test that uses `std::fs`.

## Plan

1. Add the narrow target/test `cfg` attributes at the helper definition and test import.
2. Run focused app tests and the mandatory workspace release gates.
3. Review the diff and reconcile this packet with verification evidence.

## Decisions

No decision record is needed; no architectural or behavioral choice is introduced.

## Verification Plan

- `cargo test -p oneterm-app native_crash::tests`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- `git diff --check`

## Evidence and Gaps

- `cargo test -p oneterm-app native_crash::tests` — passed: 2 tests across 2 suites, with 12 filtered out.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues on the final rerun.
- `harness story verify BUG-004` — passed using the warning-denied workspace Clippy command.
- `cargo build --workspace` — passed.
- `git diff --check` — passed.
- `srcwalk review --scope crates/app/src/native_crash.rs` — reviewed both target-configuration hunks; no function body or runtime flow changed.
- Platform gap: only the current host target was compiled. Windows/macOS call sites were source-reviewed, and the existing `cfg(test)` path keeps hexadecimal formatting covered by the focused test on the current host.

## Handoff

Single-session change; no handoff expected.

## Harness Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Harness Proof

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->
