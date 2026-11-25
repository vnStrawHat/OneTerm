# Work: Show closed SSH sessions as read-only

ID: US-0026
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

- Change type: existing-contract change
- Risk lane: high-risk (established terminal lifecycle and public user-visible behavior)
- Spec Intake, when required: `IN-0013`

## Outcome

When an SSH session emits `Closed`, the terminal immediately shows one error toast and a persistent bottom banner, preserves read-only access to prior output, and no longer sends outbound terminal input.

## Scope

- [x] In scope: SSH `Closed` event folding, one toast, persistent banner, cursor/completion stop, outbound input gating, focused tests.
- [x] Out of scope: reconnect, root-cause classification, retained release diagnostics, local-shell exit presentation, changing `SessionEvent` shape.

## Acceptance

- [x] The first SSH `Closed` event marks the view closed and queues exactly one error toast.
- [x] A persistent banner reads `SSH connection closed. Input is disabled.`
- [x] Prior terminal content remains renderable, scrollable, searchable, selectable, and copyable.
- [x] Keyboard, paste, IME, mouse-reporting, completion acceptance, and generated terminal replies do not write once `TerminalSession::alive()` is false.
- [x] Repeated close and tab shutdown remain idempotent.
- [x] Local-shell `Exited`/`Closed` behavior is unchanged.

## Documentation

### Owning Docs Reviewed

- `docs/PROJECT.md` — `TerminalSession` public contract and verification boundary.
- `docs/terminal-backend.md` — lifecycle event flow and terminal-view responsibilities.
- `docs/agents/error-policy.md` — transport closure must be observable and not silently retried.
- `reference/gpui-component/crates/ui/src/alert.rs` — local pinned alert/banner API.
- `crates/terminal-view/src/view/local_view.rs`, `view/render.rs`, and relevant tests — authoritative brownfield behavior.

### Documentation Action

Update required: `docs/terminal-backend.md` must describe the closed SSH presentation and read-only behavior.

Reason: current docs stop at delivery of `SessionEvent::Closed` and do not describe the accepted UI state.

### Reconciliation

Updated `docs/terminal-backend.md` with the SSH-only closed-session presentation, retained read-only interactions, and shared outbound-write alive gate.

## Context

The backend already sets shared `alive = false` before forwarding `Closed`. The current view only ends agent state, so release users receive no visible lifecycle feedback and failed input is only logged.

## Plan

- [x] Add regression tests for one-time close folding, SSH-only presentation, and input gating.
- [x] Add the smallest view state and persistent banner/toast behavior.
- [x] Gate the shared outbound write seam without disabling read-only actions.
- [x] Run focused tests and reconcile owning docs.

## Decisions

User accepted banner plus one toast, persistent read-only history, and disabled outbound input. Root-cause text is intentionally generic until the backend owns a durable reason contract.

## Verification Plan

- Focused `oneterm-terminal-view` lifecycle/input tests.
- `cargo test -p oneterm-terminal-view`
- `cargo test -p oneterm-terminal -p oneterm-ssh`
- Full `scripts/ci-local` gate after both intake packets integrate.
- Manual Windows release close presentation if a GUI environment is available.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Harness verify passed: 468 focused tests across terminal, SSH, and terminal-view.
- Regression proof covers one-time SSH close state/toast, persistent banner construction, completion dismissal, and a dead SSH session rejecting input without touching its transport.
- `cargo test --workspace`: 948 passed, 3 ignored, 172 filtered out across 44 suites.
- `bash scripts/ci-local.sh`: passed the full required local CI gate, including fmt, clippy with warnings denied, workspace tests, dependency/UI/doc/English/catalog/notices checks.
- `git diff --check`: passed.
- Manual Windows GUI/real-server close E2E was not run, so exact banner/toast appearance remains an explicit visual verification gap.

## Handoff

Current owner: implementation session. No blocker.
