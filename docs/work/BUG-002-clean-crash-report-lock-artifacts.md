# Work: Clean crash report lock artifacts

ID: BUG-002
Created: 2026-08-11

## Classification

- Change type: bug
- Risk lane: normal
- Spec Intake: IN-0001 follow-up observation

## Outcome

The unique-file crash store contains only completed reports and active/native staging artifacts; crash-specific atomic-write `.lock` and `.bak` files are neither created by new crash writes nor left behind from earlier builds.

## Scope

- In scope: panic persistence, native promotion, legacy import, redaction rewrite, retention/Dismiss cleanup, and startup reconciliation of crash-specific lock/backup artifacts.
- Out of scope: changing `oneterm_core::atomic_write` semantics or cleaning persistent lock files belonging to JSON/configuration documents.

## Acceptance

- New panic reports use create-new plus write/sync on their already unique path and do not invoke shared atomic persistence.
- Native promotion and legacy import use direct durable writes because claim/identity ownership already serializes them.
- A second panic targeting the same per-process path does not truncate the first captured report.
- Startup removes orphan `.<report>.crash.txt.lock` and `<report>.crash.bak` artifacts created by previous builds.
- Loading, retention, and Dismiss remove the selected report's legacy lock/backup siblings.
- Focused tests prove no crash lock/backup artifacts remain.

## Documentation

### Owning Docs Reviewed

- `docs/crash-reporting.md` — crash store and lifecycle owner.
- `docs/agents/persistence.md` — persistent `.lock` files are intentional for shared atomic documents.
- `docs/agents/error-policy.md` — crash persistence failures remain observable.
- `docs/work/US-003-store-and-recover-multiple-crash-reports.md` — unique path and claim invariants.
- `crates/core/src/persistence.rs` — `atomic_write` intentionally acquires a persistent sibling lock and creates backups.

### Documentation Action

Update required: state in `docs/crash-reporting.md` that unique crash text files deliberately use direct durable writes rather than shared-document atomic persistence, and document legacy crash artifact cleanup.

Reason: the observed file is expected from `atomic_write`, but that mechanism is unnecessary for collision-resistant per-instance crash files and pollutes the user-facing crash directory.

### Reconciliation

Updated `docs/crash-reporting.md` to distinguish uniquely owned crash text writes from shared-document atomic persistence and to define startup cleanup of legacy crash-specific `.lock`/`.bak` artifacts. Shared persistence policy remains unchanged.

## Context

`oneterm_core::atomic_write` creates `.<filename>.lock` as a persistent coordination artifact by design and creates `<stem>.bak` when replacing a file. Crash reports are not shared JSON documents: panic destinations are unique per process, and native staging is atomically claimed by one recovering process. Their ownership already removes the need for inter-process locking and backup replacement.

## Plan

1. Add crash-owned direct create/overwrite durable-write helpers.
2. Replace crash-store calls to `atomic_write` while preserving first-panic and native-claim behavior.
3. Reconcile and delete crash-specific legacy lock/backup artifacts.
4. Add focused tests and run workspace gates.

## Decisions

No separate decision record: this restores the documented clean crash-store invariant without changing shared persistence policy.

## Verification Plan

- Focused crash report tests for first-write preservation and artifact cleanup.
- `cargo test -p oneterm-app -p oneterm-settings-ui`
- `python scripts/verify-dependency-graph.py`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- `cargo test --workspace`

## Evidence and Gaps

- Root cause confirmed in `crates/core/src/persistence.rs`: `atomic_write` intentionally acquires a persistent `.<filename>.lock` and creates replacement backups.
- `srcwalk discover atomic_write --scope crates/app/src/crash_report.rs` — zero remaining crash-store uses.
- `cargo test -p oneterm-app crash_report::tests` — passed: 12 focused tests, including first-report preservation, absence of newly created lock/backup artifacts, startup orphan cleanup, promotion cleanup, and loaded-report cleanup.
- `cargo test -p oneterm-app -p oneterm-settings-ui` — passed: 20 tests across 5 suites.
- `python scripts/verify-dependency-graph.py` — passed for 18 workspace packages and explicit members.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `cargo test --workspace` — passed: 566 passed, 2 ignored, 64 filtered out across 41 suites.
- `srcwalk review` and `git diff --check` — completed without whitespace errors.

## Handoff

Single-session implementation; no blocker.
