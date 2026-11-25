# Work: Omit crash report from GitHub issue URL

ID: BUG-003
Intake: IN-0001
Created: 2026-08-11

## Classification

- Change type: bug
- Risk lane: tiny
- Spec Intake: IN-0001

## Outcome

Create Issue opens a GitHub new-issue draft without embedding the crash report in the URL, preventing long reports from exceeding browser/GitHub URL limits.

## Scope

- In scope: Create Issue URL construction, its focused tests, and crash recovery documentation.
- Out of scope: uploading reports, GitHub API submission, attachments, or automatically copying the report.

## Acceptance

- Create Issue opens the repository's new-issue URL with a prefilled title only.
- The URL contains no `body` query parameter and no crash report content.
- Copy remains the explicit action for placing report text on the clipboard.

## Documentation

### Owning Docs Reviewed

- `docs/crash-reporting.md` — recovery action and external side-effect contract.
- `crates/settings-ui/src/crash_report_dialog.rs` — GitHub URL construction and focused tests.

### Documentation Action

Update required: revise Create Issue behavior and privacy text in `docs/crash-reporting.md`.

Reason: the external browser action no longer transports report content through the URL.

### Reconciliation

Updated `docs/crash-reporting.md`: Create Issue now prefills only the title, report content remains local, and Copy is the explicit transfer action.

## Context

GitHub/browser URL length limits make a query-encoded crash report unreliable. Users can copy the local report through the existing Copy action and paste it into the issue manually.

## Plan

1. Update the owning crash recovery contract.
2. Remove the body parameter and report argument from issue URL construction.
3. Update focused tests and run required gates.

## Decisions

No separate decision record; this narrows an existing external action.

## Verification Plan

- `cargo test -p oneterm-settings-ui crash_report_dialog::tests`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`

## Evidence and Gaps

- `cargo test -p oneterm-settings-ui crash_report_dialog::tests` — passed: 3 tests; the issue URL test asserts exact title-only output and absence of `body=`.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.

## Handoff

Single-session implementation; no blocker.

## Harness Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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
