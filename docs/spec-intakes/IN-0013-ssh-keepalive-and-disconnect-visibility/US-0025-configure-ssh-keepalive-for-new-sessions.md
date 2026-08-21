# Work: Configure SSH keepalive for new sessions

ID: US-0025
Intake: IN-0013
Created: 2026-08-21

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

- Change type: new capability
- Risk lane: high-risk (additive persisted configuration and connection behavior)
- Spec Intake, when required: `IN-0013`

## Outcome

Users can enable or disable SSH keepalive, set a 5–3600 second interval, and set a 1–20 unanswered-request limit in Terminal Settings. Defaults remain enabled, 30 seconds, and three requests; a validated snapshot applies to SSH sessions opened after the change.

## Scope

- [x] In scope: `terminal.json` schema/defaults/roundtrip, live settings, Terminal Settings UI, `SessionFactory` propagation, russh configuration, tests.
- [x] Out of scope: runtime mutation of existing sessions, per-saved-session overrides, reconnect behavior.

## Acceptance

- [x] Missing SSH settings load as enabled with a 30-second interval.
- [x] The persisted interval is normalized to the inclusive 5–3600 second range.
- [x] Settings shows an SSH group with an enable switch and numeric interval field and persists changes off the UI thread.
- [x] A new connection maps enabled to `Some(interval)` and disabled to `None`.
- [x] Missing `keepalive_max` loads and persists as the backward-compatible default of three.
- [x] `keepalive_max` is normalized to `1..=20` at load, UI update, persistence, and runtime boundaries.
- [x] Settings exposes `Keepalive Max` and newly opened sessions pass the captured value to `russh::client::Config::keepalive_max`.
- [x] Existing sessions retain the policy captured when they connected.
- [x] Authentication, host-key, SFTP, and local-shell behavior are unchanged.

## Documentation

### Owning Docs Reviewed

- `docs/PROJECT.md` — backend, persistence, and verification invariants.
- `docs/agents/persistence.md` — `terminal.json` ownership and safe additive defaults.
- `docs/terminal-backend.md` — `SessionFactory`/SSH backend flow and current keepalive behavior.
- `docs/agents/structure.md` and `docs/agents/crate-dependency-rules.md` — backend-neutral factory boundary.
- `docs/agents/error-policy.md` — validation and transport failure policy.
- `docs/agents/dependencies.md` — reference-first UI component policy.

### Documentation Action

Update required: `docs/agents/persistence.md` and `docs/terminal-backend.md` must describe the new persisted group, defaults, range, new-session timing, and runtime mapping.

Reason: accepted persisted and connection behavior changes.

### Reconciliation

Updated `docs/agents/persistence.md` and `docs/terminal-backend.md` with the schema owner, defaults, validation range, new-session snapshot timing, and russh mapping.

## Context

`russh 0.61.2` defaults keepalive off. OneTerm 0.4.2 currently hard-codes enabled/30 seconds and three unanswered requests. The setting must preserve that safe default.

## Plan

- [x] Add config/default/roundtrip and SSH mapping regression tests.
- [x] Add the minimal persisted/live settings model and Settings UI group.
- [x] Pass the snapshot through `SessionFactory` and apply it in `oneterm-ssh`.
- [x] Run focused tests and reconcile owning docs.
- [x] Add acceptance-rework tests for default and out-of-range `keepalive_max` values.
- [x] Extend persisted/live/runtime policy and Settings UI with `keepalive_max`.
- [x] Apply the captured max in russh and rerun focused/full verification.

## Decisions

User accepted enabled/30 seconds, a 5–3600 second interval, and new-session-only application in the intake discussion. For acceptance rework, the user selected `keepalive_max` range `1..=20` with backward-compatible default three.

## Verification Plan

- `cargo test -p oneterm-settings`
- `cargo test -p oneterm-ssh`
- `cargo test -p oneterm-session-ui -p oneterm-app`
- Full `scripts/ci-local` gate after both intake packets integrate.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Harness verify passed: 212 focused tests across core, settings, settings UI, SSH, session UI, and app.
- `cargo test --workspace`: 948 passed, 3 ignored, 172 filtered out across 44 suites.
- `bash scripts/ci-local.sh`: passed the full required local CI gate, including fmt, clippy with warnings denied, workspace tests, dependency/UI/doc/English/catalog/notices checks.
- `git diff --check`: passed.
- Manual Settings-to-real-SSH E2E was not run; automated proof covers schema defaults/normalization, propagation compilation, and runtime interval mapping.
- Acceptance rework (configurable `keepalive_max`) verified: Harness verify passed with 216 focused tests across core, settings, settings UI, SSH, session UI, and app; full `scripts/ci-local.sh` and `git diff --check` passed after the rework. Same manual real-SSH E2E gap remains.
- Placement rework verified: the SSH group moved out of the Terminal page into its own top-level "SSH" Settings page (`crates/settings-ui/src/ssh.rs`, Network icon, Connection group) following the SFTP page pattern; Harness verify and full `scripts/ci-local.sh` re-passed.

## Handoff

Current owner: implementation session. No blocker.
