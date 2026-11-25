# High-Level Design: SSH keepalive and disconnect visibility

Intake: IN-0013
Lane: high_risk
Date: 2026-08-21

## Idea

Keep the existing safe keepalive behavior as the default while making its enablement, interval, and unanswered-request limit explicit user settings. Snapshot that policy when a new SSH connection starts. Separately, fold a remote `Closed` lifecycle event into terminal-view state so the previous screen remains available read-only while a toast and persistent banner make the closure visible.

## Diagram

```text
terminal.json                 Settings / Terminal / SSH
     |                                  |
     +---- oneterm-settings live model -+
                        |
                        | snapshot on connect
                        v
session-ui -> SessionFactory -> app -> oneterm-ssh -> russh::client::Config

russh channel ends -> ssh_main_task -> SessionEvent::Closed
                                           |
                                           v
                                terminal-view closed state
                                  |                 |
                                  v                 v
                             one toast       persistent banner
                                                + input gate
```

## UI Wireframe

```text
+-----------------------------------------------------------+
| Settings > Terminal                                       |
+-----------------------------------------------------------+
| SSH                                                       |
| Keep SSH connections detectable across idle network paths.|
|                                                           |
| Enable Keepalive                              [ ON ]       |
| Keepalive Interval                           [ 30 ] seconds|
| Keepalive Max                                 [  3 ] requests|
| Applies to newly opened SSH sessions.                     |
+-----------------------------------------------------------+

+----------------------- SSH Terminal ----------------------+
| previous terminal output remains scrollable/selectable    |
|                                                           |
|                                                           |
+-----------------------------------------------------------+
| ! SSH connection closed. Input is disabled.               |
+-----------------------------------------------------------+

One application toast is also shown when the close event arrives:
"SSH connection closed."
```

## Data Flow

1. `TerminalConfig` loads `ssh.keepalive_enabled`, `ssh.keepalive_interval_secs`, and `ssh.keepalive_max`, using default-compatible values when the group or fields are absent.
2. The Settings UI validates the interval to 5–3600 seconds and max to 1–20, updates the live model, and schedules persistence off the UI thread.
3. `session-ui` snapshots the policy with the other terminal settings when a connection starts and passes it through `SessionFactory`.
4. `oneterm-ssh` maps enabled to `Some(Duration)` and disabled to `None`, and applies the captured unanswered-request limit.
5. When the SSH task ends, the existing `SessionEvent::Closed` reaches terminal-view.
6. For SSH only, terminal-view records a closed state, queues one toast, stops cursor blinking/completion, and renders a persistent banner. Existing output interactions remain available; outbound terminal writes are ignored once the session reports not alive.

## Detail Design

- [x] Detail design: required (high-risk)
- Reason: additive persisted configuration and established runtime/session UI behavior require explicit boundary and failure-mode design.
- `low-level-design/ssh-liveness.md`
- `low-level-design/disconnect-visibility.md`
