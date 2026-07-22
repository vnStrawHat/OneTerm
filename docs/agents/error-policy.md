# Runtime error policy — OneTerm

This policy keeps failures observable without turning recoverable user and transport
errors into process-wide panics.

## Decision table

| Failure class | Required behavior | Examples |
|---|---|---|
| User input or action | Return/stop the operation and show a notification with a corrective message. | Invalid host, invalid path, rejected host key, unavailable destination. |
| Transport closure or cancellation | Return a typed error or terminal/session state; do not retry implicitly. | Closed PTY/channel, cancelled connect, full command queue. |
| Persistence read/write | Preserve the previous file, use a documented default only when safe, quarantine invalid JSON, and report recovery. | Invalid `terminal.json`, atomic write failure, concurrent layout update. |
| Optional telemetry/UI refresh | Log at `debug`/`warn` with the operation and continue only when the value is genuinely optional. | Failed best-effort flush, stale status widget, dropped repaint request. |
| Initialization invariant | Fail fast with a precise `expect`/panic only when continuing would violate a guaranteed startup invariant. | Missing required global after startup registration. |
| Test/build setup | `unwrap`/`expect` is acceptable when the test or build fixture is the subject of the assertion. | Temporary fixture creation, known-good test JSON, resource generation. |

## Review rules

- Do not use `let _ =` for a runtime operation unless the line has a nearby comment
  explaining why the result is intentionally best effort.
- Prefer `Result` and typed domain errors at backend and persistence boundaries.
- UI action handlers convert user-action failures to notifications; they must not
  hide a failed mutation behind an empty/default view.
- Defaults must be accompanied by a recovery log when malformed persisted data was
  replaced or quarantined.
- Keep `unwrap`/`expect` out of network, PTY, SFTP, and persistence error paths.
- When an operation is intentionally best effort, include the operation name in its
  log message so failures can be diagnosed without reproducing the UI action.

The workspace dependency and lint checks enforce structural rules. This policy is a
review contract for runtime behavior and is intentionally supplemented by focused
unit tests for persistence, transport lifecycle, and action-state transitions.
