# High-Level Design: Terminal printable-output logging

Intake: IN-0010
Lane: high_risk
Date: 2026-08-19

## Idea

Attach one backend-neutral logging controller to each terminal session. The shared terminal pump feeds transport bytes through an independent VTE parser that emits only printable characters and line boundaries, so escape/OSC/control payloads are excluded without changing terminal rendering. Global startup policy is persisted in `terminal.json`; saved SSH sessions can inherit, force on, or force off. UI controls operate on the same controller and render its current state.

## Diagram

```text
terminal.json ── global Local/SSH policy, folder, write mode
ssh_session.json ── Use global / On / Off
             │
             ▼
SessionFactory startup config
             │
 Local PTY / SSH channel bytes
             │
             ▼
 TerminalPump ───────────────► existing alacritty parser/rendering
             │
             └───────────────► TerminalLogController
                                 ├─ VTE printable-line collector
                                 ├─ timestamp formatter
                                 └─ buffered log file
                                          │
                      context menu / indicators / error notification
```

## Data Flow

1. A new terminal resolves the effective startup policy from global settings and, for a saved SSH session, its tri-state override.
2. The backend constructs a per-session `TerminalLogController` with a sanitized identity: `<shell_process_name>_<pid>` or `<user>_<host>_<port>`.
3. If startup logging is enabled, the controller creates the configured folder and opens `%n_%Y-%m-%d_%H-%M-%S.log` using Append or one-time Overwrite semantics.
4. Every transport read is offered to the controller before/alongside the existing terminal parser. A separate VTE parser retains printable characters only, ends a message at LF or CR, applies `[%Y-%m-%d %H:%M:%S] %msg`, and writes it through a buffered file.
5. Start/Stop context-menu actions transition that controller. Start and Stop are disabled according to the controller state. Filesystem errors become UI notifications; terminal output continues.
6. A single-Space tab shows a red record icon before its title. A multi-Space tab suppresses that title icon and each logging Space shows a small red top-right overlay.
7. Closing a terminal flushes and closes its log. A final unterminated printable line is flushed on Stop/close.

## Detail Design

- [x] Detail design: required (high-risk) and added at `low-level-design/terminal-output-logging.md`.
- Reason: parser framing, filesystem failure behavior, process identity, and cross-thread control need explicit mechanics.
