# Work: Recover from a previous OneTerm crash

ID: US-001
Created: 2026-08-11

## Classification

- Change type: new capability
- Risk lane: high-risk
- Spec Intake: IN-0001

## Outcome

When OneTerm terminates because of a Rust panic, it preserves diagnostic text and offers it on the next successful main-window launch so the user can dismiss it, copy it, or review a prefilled GitHub issue draft.

## Scope

- In scope: Rust panic hook capture; one pending local report; hidden tenth-click About icon panic; startup recovery dialog; Dismiss, Copy, and Create Issue actions; retention on every non-Dismiss close path.
- Out of scope: native faults/access violations, OS kill/power loss, automatic issue submission, telemetry upload, report history, and cleanup on Copy/Create Issue.

## Acceptance

- A panic writes a report containing app/version/platform/panic/location/backtrace diagnostics without suppressing the previous panic hook.
- Clicking the About app icon ten times triggers an intentional panic and resets the counter.
- A pending report opens a startup dialog after the main window exists.
- The dialog has a title, a multiline report textarea, and Dismiss, Copy, and Create Issue buttons.
- Dismiss closes the dialog and deletes the pending report; deletion failure is logged and retains the report.
- Copy writes the report to the clipboard and retains the pending report.
- Create Issue opens a GitHub new-issue URL prefilled with title and body and retains the pending report.
- Closing the dialog by any other route retains the pending report for the next launch.
- No-report startup and normal About behavior remain unchanged.

## Documentation

### Owning Docs Reviewed

- `AGENTS.md` — required workflow and quality gates.
- `docs/agents/code-style.md` — Rust style, panic exception rationale, and test requirements.
- `docs/agents/structure.md` — app/settings-ui ownership and dependency layers.
- `docs/agents/crate-dependency-rules.md` — no new upward or cross-feature dependency.
- `docs/agents/error-policy.md` — panic versus recoverable failure behavior.
- `docs/agents/persistence.md` — persistence lifecycle and UI-thread I/O boundary.
- `docs/agents/dependencies.md` — local GPUI reference-first requirement.

### Documentation Action

Update required: add `docs/crash-reporting.md` as the owning lifecycle and support contract; update `docs/PROJECT.md` with confirmed project context created by Harness initialization.

Reason: crash report scope, privacy, retention, and native-fault limitations are product behavior future work must preserve.

### Reconciliation

Added `docs/crash-reporting.md` as the owning behavior, lifecycle, limitation, and privacy contract. Populated `docs/PROJECT.md` with confirmed project facts and verification commands. The reviewed architecture and policy docs remain accurate and require no edits.

## Context

- Release profile uses `panic = "unwind"`.
- The app crate is the earliest common startup owner and already depends on settings-ui.
- `AlertDialog` defaults to non-overlay-closable and no close button; explicit non-Dismiss close handling must never delete the report.
- The report textarea is display-only to prevent accidental mutation of the diagnostic submitted/copied.

## Plan

1. Add tested app-owned panic report formatting, atomic replacement, loading, deletion, and hook installation.
2. Wire report discovery into startup and recovery dialog display after opening the main window.
3. Add settings-ui recovery dialog with clipboard and percent-encoded GitHub issue draft actions.
4. Add the hidden click counter to the About identity icon.
5. Run focused tests and all mandatory workspace quality gates.

## Decisions

No separate decision record: the choices are local to this new owning contract and recorded in `docs/crash-reporting.md`.

## Verification Plan

- Focused app tests cover formatting, write/load/delete, malformed UTF-8 handling, and URL-independent lifecycle behavior.
- Focused settings-ui tests cover tenth-click behavior and GitHub URL percent encoding.
- `cargo test -p oneterm-app -p oneterm-settings-ui`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- Manual crash/restart UI flow if the GUI environment is available; otherwise report it as unverified.

## Evidence and Gaps

- `cargo test -p oneterm-app -p oneterm-settings-ui` — passed: 8 tests across 5 suites.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `cargo test --workspace` — passed: 554 passed, 2 ignored, 64 filtered out across 41 suites.
- `srcwalk review` and `git diff --check` — completed without whitespace errors; structural review covered all changed implementation files.
- Manual GUI ten-click crash/restart, clipboard, browser, and non-Dismiss close flows were not run in this headless coding session. Their wiring is compile-checked and pure lifecycle/encoding behavior has focused unit coverage.

## Handoff

Single-session implementation; no blocker.
