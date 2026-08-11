# Work: Store and recover multiple crash reports

ID: US-003
Created: 2026-08-11

## Classification

- Change type: existing-contract change and concurrency bug prevention
- Risk lane: normal
- Spec Intake: IN-0001 follow-up intake #3

## Outcome

Each OneTerm instance writes to a collision-resistant report identity under `crashes/`; restart recovery safely handles multiple reports newest-first, Dismiss removes only the current report, and storage retains at most 20 completed reports.

## Scope

- In scope: `crashes/` directory; UTC-time/PID/random naming; per-instance panic destination and native staging file; dead-process native staging promotion; legacy singleton import; newest-first loading; sequential dialogs; current-report cleanup; 20-report retention.
- Out of scope: a crash history browser, manual bulk-delete UI, cross-device synchronization, and preserving more than 20 completed reports.

## Acceptance

- Completed reports use `crashes/YYYYMMDDTHHMMSSmmmZ-p<PID>-<8 hex random>.crash.txt`.
- Native capture uses the same identity with `.native.tmp`, chosen before handler installation; no randomness, allocation, or path construction occurs in the compromised callback.
- Concurrent live instances never write the same path and do not consume or delete one another's active native staging file.
- A dead instance's non-empty staging file is promoted to its matching completed report; if a Rust panic report already exists for that identity, both diagnostics are retained in one report.
- Existing `pending-crash-report.txt` and `native-crash-report.txt` are imported once so current users do not lose a pending diagnostic.
- Completed reports are sanitized, sorted newest-first by the sortable identity, and pruned to the newest 20.
- Startup opens the newest report. Dismiss deletes only that report and opens the next report. Copy/Create Issue retain the current report. Closing by X/Escape/overlay retains the current and remaining reports without advancing.
- Cleanup accepts only paths discovered from the crash store and deletes the selected report plus its backup.

## Documentation

### Owning Docs Reviewed

- `docs/crash-reporting.md` — capture storage, promotion, privacy, recovery actions, and retention contract.
- `docs/agents/persistence.md` — blocking I/O and atomic-write constraints.
- `docs/agents/error-policy.md` — startup and cleanup failure visibility.
- `docs/agents/code-style.md` — concurrency clarity, error handling, and regression tests.
- `docs/PROJECT.md` — standing project invariants.
- `docs/work/US-002-redact-user-paths-and-capture-native-crashes.md` — previous singleton staging and safety decisions.

### Documentation Action

Update required: revise `docs/crash-reporting.md` and the intake to replace singleton storage/lifecycle with the accepted multi-report queue and retention contract.

Reason: storage names, retention, and per-action deletion semantics are user-visible operational behavior.

### Reconciliation

Updated `docs/crash-reporting.md` with the `crashes/` layout, identity format, concurrent staging ownership, legacy import, newest-first queue, per-report actions, and 20-report retention. Updated IN-0001 with follow-up intake #3 and removed singleton assumptions.

## Context

The user selected sequential newest-first display, per-report Dismiss, a 20-report cap, and UTC-time/PID/random names. PID is part of the identity both for readability and to avoid simultaneous-instance collisions; randomness protects PID reuse and same-time collisions. Native staging must remain separate because the file is pre-opened for compromised-context writes. Startup must skip staging owned by a currently live PID so one instance cannot unlink another instance's crash destination.

## Plan

1. Introduce crash-store identities and paths; migrate singleton artifacts, promote inactive staging, sanitize/load/sort, and prune.
2. Pass structured pending reports into the window and settings UI.
3. Implement sequential per-report Dismiss without re-entering `Root` during its startup update.
4. Add focused concurrency/lifecycle tests and run workspace gates.

## Decisions

No separate decision record: the user-selected queue, deletion, naming, and retention behavior is recorded in the owning crash contract.

## Verification Plan

- Focused crash store tests for unique names, sorting, legacy import, staging behavior, sanitization, and retention.
- Focused dialog queue tests for report ordering/advance state where separable from GPUI rendering.
- `cargo test -p oneterm-app -p oneterm-settings-ui`
- `python scripts/verify-dependency-graph.py`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- `cargo test --workspace`

## Evidence and Gaps

- `cargo test -p oneterm-app crash_report::tests` — passed: 10 focused tests covering identities, legacy import, newest-first retention, native promotion/combination, empty cleanup, redaction, and path validation.
- `cargo test -p oneterm-settings-ui crash_report_dialog::tests` — passed: 3 tests covering queue order, URL encoding, and issue content.
- `cargo test -p oneterm-app -p oneterm-settings-ui` — passed: 18 tests across 5 suites.
- Manual bounded startup with two completed fixtures under `target/crashes/` — OneTerm initialized and remained stable until the 12-second timeout; no nested `Root` panic occurred. Both completed reports remained, and the running instance created a distinct empty native staging name. Test artifacts were removed afterward.
- `python scripts/verify-dependency-graph.py` — passed for 18 workspace packages and explicit members.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `cargo test --workspace` — passed: 564 passed, 2 ignored, 64 filtered out across 41 suites.
- `srcwalk review` and `git diff --check` — completed with no whitespace errors.
- Automated UI clicking was unavailable, so Dismiss-to-next rendering is source-verified against gpui-component's immediate close/open support (`Root::open_dialog` preserves pending focus restoration) and the queue transition has a focused unit test. The initial multi-report startup path was exercised manually.

## Handoff

Single-session implementation; no blocker.
