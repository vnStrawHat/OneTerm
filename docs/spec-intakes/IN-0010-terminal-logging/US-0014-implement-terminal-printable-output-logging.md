# Work: Implement terminal printable-output logging

ID: US-0014
Intake: IN-0010
Created: 2026-08-19

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

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

- Change type: new capability
- Risk lane: high-risk (plaintext user terminal data and overwrite behavior)
- Spec Intake: `IN-0010`

## Outcome

Local and SSH terminal instances can automatically or manually record line-oriented printable output in the configured folder, with persisted policy, correct SSH override precedence, safe state transitions, and accurate UI controls/indicators.

## Scope

- [x] In scope: global Local/SSH automatic logging choices, folder picker/input, Append/Overwrite, fixed filename and record formats, per-saved-SSH inherit/on/off, per-terminal Start/Stop, parser-level printable line capture, failure notifications, and single-/multi-Space indicators.
- [x] Out of scope: user-editable format strings, log rotation/retention, encryption/redaction, input/keystroke logging, historical scrollback export, reconstructing in-place terminal screens, and SFTP activity logging.

## Acceptance

- [x] New local and SSH terminals resolve automatic logging from `terminal.json`; saved SSH `on`/`off` override global SSH and `inherit` uses it.
- [x] Default directory is `<user_home>/.OneTerm/logs`; Settings shows only a normal-color gpui-component `Input` for the folder. The Input is not editable, clicking it opens the native OS directory selector, and the selected folder updates the Input and persists.
- [x] Start opens `%n_%Y-%m-%d_%H-%M-%S.log`; local `%n` is `<process>_<pid>`, SSH `%n` is `<user>_<host>_<port>`, and unsafe filename characters cannot escape the folder.
- [x] Append appends to a collision; Overwrite truncates once on Start. Both create missing directories.
- [x] Only printable text is logged, one record per LF/CR-delimited non-empty message using `[%Y-%m-%d %H:%M:%S] %msg`; ANSI, OSC, DCS, and other controls are excluded.
- [x] Stop and terminal close flush a final unterminated message. A logging I/O failure stops logging, not terminal output, and is surfaced to the user.
- [x] Context menu has a `Log` submenu immediately above a separator and `Close Terminal Tab`; Start/Stop disabled states match the right-clicked terminal.
- [x] A logging single-Space tab shows a red record icon before its title. Multi-Space tabs omit that tab icon and each logging terminal Space shows a small red top-right overlay.
- [x] Existing persisted files without logging fields load with non-logging defaults and retain all existing data.

## Documentation

### Owning Docs Reviewed

- `docs/PROJECT.md` — persistence, security boundaries, quality gate.
- `docs/terminal-backend.md` — shared pump and backend/session façade.
- `docs/terminal-split.md` and `docs/terminal-split/05-rendering-theme.md` — tab/Space ownership and rendering.
- `docs/ssh-client-connect.md` — saved session and connect flows.
- `docs/agents/persistence.md` — schema ownership, background writes, compatibility.
- `docs/agents/crate-dependency-rules.md` — no UI/backend dependency and lowest shared layer.
- `reference/gpui-component/crates/ui/src/menu/popup_menu.rs` — pinned submenu/disabled/separator APIs.
- `reference/gpui-component/crates/ui/src/input/` — pinned Input rendering, disabled appearance, focus, and pointer behavior for the folder-field acceptance rework.

### Documentation Action

Update required: create `docs/terminal-logging.md`; update `docs/terminal-backend.md`, `docs/ssh-client-connect.md`, `docs/agents/persistence.md`, and the current architecture/index docs where they enumerate configuration or terminal responsibilities.

Reason: this adds a persisted schema surface, filesystem side effect, shared terminal output path, SSH behavior, and visible controls not described by existing contracts.

### Reconciliation

Created `docs/terminal-logging.md` and updated the terminal backend, SSH connect, persistence, architecture, and documentation-index contracts to describe the implemented ownership, precedence, and lifecycle.

## Context

`TerminalPump` is the only backend-neutral location that sees every transport byte before repaint coalescing. Local process pid is available only on the owner thread after `tty::new`; SSH identity comes from `SshConfig`. All persisted settings writes continue through existing owner queues/atomic-write paths. Runtime log files are user content, not JSON schema documents, and use direct buffered file I/O.

## Plan

- [x] Add tested logging domain/config/controller and connect it to the shared pump and session capability.
- [x] Pass identity/startup policy through local/SSH construction without adding UI→backend edges.
- [x] Persist global logging settings and saved SSH tri-state with compatibility tests.
- [x] Replace the separate Log Folder rows with a reusable single-row Input-based `FolderPicker` and native directory prompt.
- [x] Move the ellipsis button outside the Input border while retaining a single horizontal row.
- [x] Remove `FolderPicker` and its button; render only a normal-color, non-editable Input that opens the native folder selector on click.
- [x] Re-run focused settings UI checks and the full CI-local gate, then reconcile proof for the acceptance rework.

## Decisions

- [`DEC-0003`](../../decisions/DEC-0003-define-terminal-logging-capture-and-override-semantics.md) — line framing, SSH tri-state, and overwrite collision semantics.

## Verification Plan

- Focused: `cargo test -p oneterm-settings-ui`, plus the existing logging tests in terminal, settings, session-ui, and terminal-view.
- Integration/regression: affected crate suites for core, terminal, local-shell, ssh, settings, session-ui, settings-ui, and terminal-view.
- Platform/manual: Windows run verifying native folder picker, local pid filename, SSH override, context-menu state, and split indicators.
- Quality gate: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

The dedicated `FolderPicker` component and module were removed. The Log Folder field now renders only a normal, non-disabled gpui-component Input. It is excluded from keyboard tab focus and covered by a transparent pointer target, preventing direct editing while opening GPUI's native directory selector on left click. A selected path updates the Input and persists through the existing settings owner.

Focused `oneterm-settings-ui` build/tests and workspace clippy pass. The Harness verify command supplies the complete `pwsh scripts/ci-local.ps1` quality-gate evidence below.

Gap: no interactive Windows GUI run was performed, so native dialog appearance, pointer behavior, and normal-color presentation have compile/API evidence but not manual platform proof in this session.

## Handoff

No cross-session handoff planned.
