# Spec Intake: Persistent crash reporting and recovery

ID: IN-0001
Date: 2026-08-11
Type: new_spec
Lane: high_risk

## Source

User request: after unexplained crashes reported in release 0.3.6, preserve crash diagnostics, add a hidden ten-click crash trigger to the About icon, and show a retained recovery dialog on the next launch.

## Requested Outcome

Capture Rust panic diagnostics and supported platform-native fatal crash context, redact the current user's home-directory prefix, persist diagnostics across restart, provide a hidden ten-click About panic trigger, and present a recovery dialog with retention-aware actions.

## Project Impact

- Product surfaces: process startup, main window startup, About icon, recovery dialog.
- Data: up to 20 completed local crash-report text files plus per-live-instance native staging files under `crashes/`, owned by the app binary.
- External system: opening a prefilled issue URL in the system browser; OneTerm does not submit the issue itself.
- Existing startup and About behavior must remain unchanged when no report exists and before the tenth icon click.

## Candidate Product Contracts

| Contract | Purpose | Source or owner |
| --- | --- | --- |
| Crash recovery lifecycle | Define capture, retention, cleanup, and startup presentation | `docs/crash-reporting.md` / app owner |
| Runtime error policy | Bound panic capture to unrecoverable Rust panics | `docs/agents/error-policy.md` |
| Persistence policy | Keep blocking file I/O off UI action handlers | `docs/agents/persistence.md` |

## Candidate Work Packets

| Packet | Outcome | Dependencies |
| --- | --- | --- |
| US-001 | A prior panic is discoverable and actionable after restart | IN-0001 |
| US-002 | User-home paths are redacted and native crashes are captured | IN-0001 follow-up #2 |
| US-003 | Concurrent instances retain unique reports and recovery handles a newest-first queue | IN-0001 follow-up #3 |

## Architecture and Boundary Questions

- Runtime and owning boundary: `oneterm-app` installs the process panic hook and owns the report file; `oneterm-settings-ui` renders About and recovery UI.
- Data ownership and lifecycle: each process has a UTC-time/PID/random identity; completed reports are loaded newest-first and capped at 20; Dismiss deletes only the current report and advances, while Copy, Create Issue, escape/overlay close, and window close retain reports.
- Auth, security, privacy, or audit: the report may contain file paths and panic text; opening GitHub only creates a draft in the browser so the user reviews before submission.
- External systems and side effects: clipboard write and browser navigation occur only after explicit user actions.
- Public interfaces and compatibility: one narrow settings-ui entry point accepts structured report paths/content and a per-path cleanup callback; singleton legacy artifacts are imported into the directory store.

## Validation Shape

| Layer | Expected proof |
| --- | --- |
| Focused | Unit tests for report formatting, persistence lifecycle, URL encoding, and ten-click counter |
| Unit | Relevant app/settings-ui crate tests |
| Integration | Startup wiring compiles and dialog opens from the created main window |
| E2E | Manual ten-click panic then restart workflow (reported as manual if not run) |
| Platform / Release | Required workspace fmt, clippy, and build gates |

## Open Decisions and Questions

- Follow-up intake #2 adds native access violations and fatal signal/exception capture through `crash-handler` 0.8.0; symbolized native stacks/minidumps and forced OS termination remain out of scope.
- Current user-home prefixes are redacted as `<USER_HOME>`; other potentially sensitive report values still require user review.
- Report cleanup failures are logged and leave the report available for retry.
- Follow-up intake #3 stores reports in `crashes/` using sortable UTC-time/PID/random names, displays them sequentially newest-first, dismisses one report at a time, and retains the newest 20.

## First Action or Handoff

Implement US-001 after reviewing startup, About, GPUI dialog/input APIs, and persistence/error policies; stop if a new third-party dependency or native crash handler becomes necessary.

## Harness Delta

None.
