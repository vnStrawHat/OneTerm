# Work: Phase 2 — structural refactors

ID: WK-0003
Intake: IN-0010
Created: 2026-08-17

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: refactor
- Risk lane: high-risk
- Spec Intake, when required: `IN-0010`

## Outcome

The Phase 2 items in `docs/review-refresh-2026-08/09-remediation-plan.md` land as behaviour-preserving
refactors, each guarded by the existing test suite plus the new pure-function tests listed in TEST-16 /
TEST-17 / TEST-19.

## Scope

- [x] In scope: ARCH-01, ARCH-02, ARCH-21..ARCH-27, PERF-01..PERF-11 (PERF-12..17 opportunistically),
  ARCH-13..ARCH-17 (+ HYG-01), ARCH-28, ARCH-29, ARCH-30, ARCH-31, ARCH-33, ARCH-34, ARCH-35, ARCH-06,
  PERF-18, ARCH-09, ARCH-10, ARCH-40, PERF-19, PERF-20, PERF-31; tests TEST-01/02 (as far as the
  transport seam allows), TEST-16, TEST-17, TEST-19.
- [x] Out of scope: BUILD-21 (upstreaming the gpui-component patches is an external PR the agent must
  not open unasked — tracked as a follow-up), Phase 3 polish (WK-0004).

## Acceptance

- [x] No behaviour change observable from tests: the full workspace suite still passes; new tests
  cover the extracted pure functions.
- [x] `oneterm-ssh` / `oneterm-local-shell` contain transport code only; shared pump/listener/OSC/colour
  logic lives in `oneterm-terminal`.
- [x] Workspace gate green on the integration branch (fmt, clippy -D warnings, test, build, Python checks,
  cargo deny) — 922 tests, 2026-08-18.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md`, `docs/architecture.md`, `docs/gui-layout.md`, `docs/sftp-browser-design.md`,
  `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md`, `docs/agents/code-style.md`,
  `docs/agents/persistence.md`.

### Documentation Action

Update required: `docs/terminal-backend.md` (shared pump in `oneterm-terminal`), `docs/agents/structure.md`
(module moves), `docs/sftp-browser-design.md` (state grouping, transfer pipelining), `docs/architecture.md`
(`AppServices` single registry).

### Reconciliation

Updated: `docs/terminal-backend.md` (shared pump layer §5.3, transport/teardown §7, trait split §9, file
layout §11), `docs/agents/structure.md` (state/terminal/ssh/local-shell/terminal-view/sftp-ui/theme trees,
crate table), `docs/architecture.md` (`AppServicesBuilder` phases, init contract, exit save),
`docs/gui-layout.md` (`TerminalPanel::open(PanelSpec)`), `docs/agents/persistence.md` (owner table,
sessions v2, load outcomes), `docs/sftp-browser-design.md` (state grouping, pipelined transfers, realpath),
`docs/ssh-client-connect.md` (`ConnectPhases`, typed errors, host parser policy), `docs/ssh-authentication.md`,
`docs/agent-panel-display.md` (nav in `AgentRegistry`), `docs/agents/crate-dependency-rules.md`.

## Plan

- [x] Wave C (parallel, disjoint ownership): C1 backends + terminal pump lift; C2 terminal-view
  structure; C3 state/settings/theme/app/workspace registries; C4 sftp-ui/session-ui/core sftp+errors.
- [x] Wave D/F: terminal-view render hot path (PERF-01..11) + `TerminalSession` split (ARCH-02) after C2 lands.

## Verification Plan

Per-crate `cargo test -p`, then full workspace gate on the merged branch.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof (Windows local; Linux/macOS via CI)
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Merges into `fix/review-remediation-phase-0-1`: 21f61bd (C3 registries/settings), ddd704e (C2 terminal-view
  structure), C1 backend pump lift (c7f3a09..1801efb), 0875e5d (C4 SFTP/session-ui/errors), 419c022 (post-merge
  fixes), 93dab0d (D1 render hot path, ARCH-13/20, SEC-08), 22441f5 (D2 backends), 8e2bea8 (D3), c625308 (D4),
  F1 `TerminalSession` split (9c3676a, 52d8350), F2 sweep (fcdc748..e9e069f).
- Gate on the merged branch (2026-08-18): fmt, clippy -D warnings, build, 922 tests, verify-dependency-graph,
  check-ui-fork, check-doc-paths, check-english (+unit tests), completion-catalog validate, benchmark-scale
  --list, third-party-notices --check, cargo deny.
- Gaps: BUILD-21 (external upstream PR); PERF-19 remainder needs a vendored alacritty grid counter; PERF-07/08/10
  larger cache work; TEST-01/02 partial (russh types not abstracted; real-shell e2e tests kept); ARCH-14
  `AppState.dock_area` remains (needs explicit workspace context in session-ui); `SharedState.prompt_count`/
  `last_exit_code` kept because router tests read them.
