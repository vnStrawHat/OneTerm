# Work: Duplicate terminal sessions

ID: US-0010
Intake: IN-0003
Created: 2026-08-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Keep operational status in `harness.db`.

## Classification

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: `docs/spec-intakes/IN-0003-duplicate-terminal-sessions/IN-0003.md`

## Outcome

A terminal Space context menu offers **Duplicate Session** immediately below **New Terminal**. The action creates a new terminal tab using the source session type and live cwd. Local sessions preserve their complete shell launch configuration. SSH sessions reopen a prefilled authentication dialog, require credentials again, reconnect to the same endpoint/default remote shell, and request the source cwd.

## Scope

- In scope: per-view non-secret launch metadata; local duplicate tab creation; SSH duplicate authentication dialog and reconnect; initial focus on the SSH secret input; shell-default behavior when cwd is unavailable; Windows cmd/PowerShell/pwsh OSC 7 prompt integration; menu placement; a user-configurable Duplicate Session key binding with no built-in default; focused tests; acceptance rework of the Windows PowerShell PTY test's startup deadline and timeout diagnostics.
- Out of scope: persisting duplicate metadata, retaining SSH passwords/passphrases, cloning terminal scrollback/process state, duplicating all Spaces in a split tab, guaranteeing cwd command syntax for unsupported remote shells, and changing production shell startup behavior for this test-only timing failure.

## Acceptance

- **Duplicate Session** appears directly below **New Terminal** in a populated terminal Space menu and is exposed in Key Bindings under `Terminal Context Menu` with no built-in default keystroke.
- Invoking it from a local session creates a new sibling terminal tab with the same `LocalShellConfig`, overriding `cwd` with the source session's live cwd; when unavailable, `cwd` is cleared so the shell/backend chooses its normal default.
- Invoking it from an SSH session opens an authentication dialog prefilled with host, port, username, auth method, and non-secret key path where applicable; password/passphrase fields remain empty, the applicable secret field receives initial focus, and `Save to SSH Sessions` is not offered.
- Successful SSH duplication creates a new tab connected to the same endpoint/default remote shell and requests the source cwd when known; when unavailable, no `cd` command is sent.
- OneTerm's generated prompt integration emits OSC 7 for Windows cmd, PowerShell, and pwsh sessions without overriding user-supplied cmd `PROMPT`; unsupported/custom prompts may still need user configuration.
- The Windows PowerShell PTY regression allows a bounded 15-second cold-start window and reports the terminal snapshot if cwd is still unavailable.
- The source session remains running and unchanged.
- No persisted schema changes and no new crate dependency violations are introduced.

## Documentation

### Owning Docs Reviewed

- `docs/PROJECT.md` — brownfield boundaries, invariants, and verification commands.
- `docs/architecture.md` — feature ownership and app-composed command boundary.
- `docs/agents/structure.md` and `docs/agents/crate-dependency-rules.md` — UI/backend and feature dependency constraints.
- `docs/agents/code-style.md` and `docs/agents/error-policy.md` — code, tests, and recoverable UI failure handling.
- `docs/agents/persistence.md` — confirms no persisted schema should be involved.
- `docs/agents/dependencies.md` — local-reference-first GPUI/gpui-component research.
- `docs/terminal-backend.md` — session factory, cwd, SSH, and secret lifetime contract.
- `docs/terminal-split/04-context-menu.md` and `06-integration.md` — menu ordering and active-Space targeting.

### Documentation Action

Update required: `docs/terminal-split/04-context-menu.md` must define Duplicate Session ordering and behavior; `docs/terminal-backend.md` must define non-secret duplication metadata, SSH reauthentication, and cwd behavior.

Reason: the capability and its security/lifecycle behavior are not in the accepted terminal contracts. For the acceptance rework, the reviewed `docs/terminal-backend.md` contract remains accurate because only the test's bounded wait and failure diagnostics change; no production behavior or owning contract changes.

### Reconciliation

Updated `docs/terminal-split/04-context-menu.md` with menu placement, one-shot secret focus, and shell-default cwd behavior, and `docs/terminal-backend.md` with non-secret launch metadata, reauthentication, cwd semantics, and Windows cmd/PowerShell/pwsh OSC 7 integration. Added the accepted credential-lifetime decision at `docs/decisions/0002-ssh-duplicate-auth.md`. The reviewed architecture and persistence contracts remain accurate; no persisted schema or dependency ownership changed.

## Context

The menu receives a `SplitContext` that identifies the owning panel and Space. `TerminalPanel` can add a sibling tab through its `TabPanel`. SSH UI cannot be imported by terminal-view, so the request must cross `WorkspaceCommands`, wired only by `oneterm-app`. SSH credentials are currently short-lived and zeroized after connect; duplication metadata must not contain secrets. Local cwd is available only after the shell emits OSC 7; the existing cmd prompt emitted OSC 133 but not OSC 7, while PowerShell/pwsh had no cwd prompt hook. Follow-up runtime feedback showed that embedded double quotes in the PowerShell `-Command` argument survived direct process probes but were stripped by the Windows PTY command-line path, producing a parser error at `[Console]::Write($e]7;...)`; the startup command must avoid nested double quotes and be verified through `LocalSession`.

## Plan

0. Reconcile code-style review findings: minimize visibility, correct API documentation, model quick-connect mode explicitly, remove unnecessary cloning, and add action-dispatch/rendered-visibility regression coverage.
1. Add a low-layer, non-secret launch descriptor and attach it to each terminal view.
2. Add `TerminalPanel` duplication behavior and place the menu item below New Terminal.
3. Add a prefilled SSH duplicate-auth dialog and app command wiring.
4. Focus the duplicate SSH dialog's applicable secret input once after dialog layout.
5. Add Windows prompt hooks that emit OSC 7 for cmd, PowerShell, and pwsh.
6. Expose the same active-Space operation as a bindable `Duplicate Session` action, registered without a default keystroke.
7. Add focused tests, including key-binding registration and a Windows `LocalSession` PowerShell/pwsh OSC 7 regression, run regression/release gates, and reconcile this packet.
8. Acceptance rework: preserve the PTY-level assertion, extend only its bounded cold-start allowance from 5 to 15 seconds, include the terminal snapshot on timeout, then repeat the focused Windows test before the quality gates.

## Decisions

- `docs/decisions/0002-ssh-duplicate-auth.md`

## Verification Plan

- Focused unit tests for launch-descriptor sanitization, shell-default cwd behavior, remote cwd command escaping, duplicate-dialog initial focus, rendered duplicate-dialog save-option visibility, unbound Duplicate Session action registration and dispatch to the active Space, and Windows prompt integration, plus a Windows PTY-level PowerShell/pwsh OSC 7 regression.
- Repeat `cargo test -p oneterm-local-shell windows_powershell_prompt_emits_cwd_without_parser_errors` to exercise the acceptance-rework path and run `cargo test -p oneterm-local-shell --lib` for backend regression coverage.
- `cargo test -p oneterm-terminal-view`
- `cargo test -p oneterm-session-ui`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- Manual local and SSH context-menu flows if a GUI and SSH target are available; otherwise record the gap explicitly.

## Evidence and Gaps

- Acceptance rework trigger: one Windows PowerShell PTY run exhausted the original 5-second cwd wait. The unchanged focused test then passed 10/10 immediate reruns in 0.64-0.76 seconds and the full local-shell suite passed, so the failure was treated as a transient cold-start/load timeout rather than a reproducible prompt parser defect.
- Rework proof: after extending only the test deadline to 15 seconds and adding the terminal snapshot to timeout output, 10/10 focused Windows PowerShell PTY reruns passed; `cargo test -p oneterm-local-shell --lib` passed all 32 tests.
- Rework quality gates passed: `cargo test --workspace` reported 599 passed and 2 ignored across 41 suites; format check, clippy with warnings denied, workspace build, English contributor-text check, architecture path check, and `git diff --check` passed.
- Production shell startup and OSC 7 behavior are unchanged. No GUI/manual verification was needed for this test-only timing and diagnostics adjustment.
- Harness focused verification passed: core credential sanitization and Windows prompt integration; 2 terminal-view config/cwd tests; real GPUI `DuplicateSession` action dispatch selecting the active Space; settings-ui registration with no default keystroke; session-ui remote cwd escaping, 2 secret-focus tests, one-shot focus lifecycle, and duplicate-dialog save-option element construction.
- Windows PTY-level regressions spawned both Windows PowerShell 5.1 and pwsh through production `LocalSession`; both emitted a cwd through OSC 7 and rendered no parser error. The quote-sensitive interpolated string was replaced with single-quoted concatenation so the Windows PTY command-line path cannot strip required nested double quotes.
- `cargo test --workspace` passed: 584 passed, 2 ignored, 76 filtered out across 41 suites.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed with no issues.
- `cargo build --workspace` passed.
- `python scripts/verify-dependency-graph.py` passed for 18 workspace packages and 18 explicit members.
- `python scripts/check-doc-paths.py` passed for 58 current paths.
- `git diff --check` passed.
- Manual desktop context-menu verification was not run in this non-interactive session. SSH end-to-end proof additionally requires a reachable SSH target and user credentials. The remote cwd command follows the existing POSIX-shell integration scope; unsupported remote shells may reject it and remain at their default cwd. Windows PowerShell/pwsh prompt integration is covered through production PTYs; cmd prompt expansion remains unit-tested and requires desktop PTY confirmation.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.

## Harness Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
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
