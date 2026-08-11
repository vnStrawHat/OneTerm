# Work: Open startup crash dialog without re-entering Root

ID: BUG-001
Created: 2026-08-11

## Classification

- Change type: bug
- Risk lane: normal
- Spec Intake: IN-0001 (existing crash recovery capability)

## Outcome

Restarting OneTerm with a pending crash report opens the recovery dialog without panicking from a nested update of `gpui_component::Root`.

## Scope

- In scope: startup recovery-dialog presentation and a regression guard around the direct Root mutation path.
- Out of scope: crash-report capture format, report actions, native crash capture, and unrelated startup behavior.

## Acceptance

- Startup code does not invoke `WindowExt::open_alert_dialog` while the main `Root` entity is already inside `WindowHandle<Root>::update`.
- The recovery dialog is added directly through the already-borrowed `Root` and retains all existing buttons and retention behavior.
- Startup without a pending report is unchanged.
- Focused tests and mandatory workspace quality gates pass.

## Documentation

### Owning Docs Reviewed

- `docs/crash-reporting.md` — recovery lifecycle and retention contract.
- `docs/agents/code-style.md` — localized regression fix and verification requirements.
- `.pi/skills/gpui/references/async.md` — warns against re-entering an entity already being updated.
- `.pi/skills/gpui/references/context.md` — context and deferred-update behavior.
- `vendor/gpui-component/src/window_ext.rs` — confirms `open_alert_dialog` delegates to `Root::update`.
- `vendor/gpui-component/src/root.rs` — confirms `Root::open_dialog` can mutate the already-borrowed Root directly.

### Documentation Action

No product contract change: `docs/crash-reporting.md` already describes the intended startup behavior. The implementation violated that contract through GPUI entity reentrancy.

Reason: this is an internal scheduling/borrowing correction with no user-visible semantic change.

### Reconciliation

No owning product documentation change was required. The direct `Root::open_dialog` implementation now matches the existing recovery lifecycle contract.

## Context

The observed panic is `cannot update gpui_component::root::Root while it is already being updated`. `open_window` called `show_crash_report` from `WindowHandle<Root>::update`; `WindowExt::open_alert_dialog` then called `Root::update`, creating the forbidden nested update. Deferring through `Window::defer` is insufficient because it also runs its callback from a root-handle update. The safe path is to call `Root::open_dialog` on the `&mut Root` already supplied to the startup update closure.

## Plan

1. Change the crash dialog entry point to accept the already-borrowed `Root` and its `Context<Root>`.
2. Build the alert dialog through `Root::open_dialog` rather than `WindowExt::open_alert_dialog`.
3. Pass the Root from `open_window`, format, review, and run focused/workspace verification.

## Decisions

No durable decision record required; this follows GPUI's existing entity borrowing contract.

## Verification Plan

- `cargo test -p oneterm-app -p oneterm-settings-ui`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- `cargo test --workspace`
- Manual restart with an existing pending report if GUI execution is available.

## Evidence and Gaps

- Reproduced pre-fix from the supplied trace: `WindowExt::open_alert_dialog` nested `Root::update` inside startup `WindowHandle<Root>::update`.
- `cargo test -p oneterm-app -p oneterm-settings-ui` — passed: 8 tests across 5 suites.
- Manual startup smoke check with an existing `target/pending-crash-report.txt`: OneTerm remained running for the 12-second observation period with no Root reentrancy panic; the process was then intentionally terminated by `timeout` (exit 124/child exit 143).
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `cargo test --workspace` — passed: 554 passed, 2 ignored, 64 filtered out across 41 suites.
- `srcwalk review` and `git diff --check` — passed.
- The automated GPUI suite does not expose a focused assertion for Root reentrancy; the compile proof, source-path review, and pending-report startup smoke check cover this regression boundary.

## Handoff

Single-session fix; no blocker.
