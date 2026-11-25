# Terminal output logging

> **Status:** Current implementation contract (2026-08).

OneTerm can record printable output from local-shell and SSH terminals. Logging is per terminal: the shared terminal pump feeds a dedicated VTE parser before advancing the visible terminal, and only printable text produced by that parser is written. Input, escape sequences, OSC/DCS payloads, and other control bytes are not logged.

## Configuration and precedence

`terminal.json` owns the global `logging` group:

- `local`: automatically start logging new local shells;
- `ssh`: automatically start logging new SSH terminals;
- `directory`: destination folder, defaulting to `<user_home>/.OneTerm/logs`;
- `write_mode`: `append` or `overwrite`.

The Terminal Settings page exposes both automatic-start switches, a normal-color non-editable Log Folder Input that opens the native OS directory selector when clicked, and the write mode. File-name and record formats are fixed in this release.

A saved SSH session in `ssh_session.json` has a `logging` value of `inherit`, `on`, or `off`. Missing values deserialize as `inherit`. `on` and `off` override global SSH automatic logging; `inherit` uses the global value. Quick Connect is not a saved-session override and uses the global SSH value.

The terminal context menu's **Log** submenu starts or stops the right-clicked terminal. Manual Start uses the current global destination and write mode, regardless of the automatic-start policy.

## Files and records

Starting logging creates missing destination directories and opens:

```text
%n_%Y-%m-%d_%H-%M-%S.log
```

`%n` is `<process>_<pid>` for a local shell and `<user>_<host>_<port>` for SSH. Characters outside ASCII letters, digits, `.`, `-`, and `_` are replaced, so the identity cannot escape the configured directory.

- `append` preserves a colliding file and writes at its end.
- `overwrite` truncates a colliding file once when Start opens it.

Each non-empty LF- or CR-delimited printable message is written as:

```text
[%Y-%m-%d %H:%M:%S] %msg
```

Stopping or dropping the final logger flushes an unterminated message. A setup or write failure changes that terminal's logging state to failed, closes the active writer, and is surfaced as an error notification; terminal rendering and transport continue.

## UI state

- A logging tab containing one Space shows a red recording marker before its title.
- A tab containing multiple Spaces omits the tab marker; each logging terminal Space shows a red marker at its top-right.
- Context-menu Start/Stop disabled states are derived from the right-clicked terminal's logging controller.

## Ownership

- `oneterm-core`: backend-neutral runtime config and fixed format constants.
- `oneterm-settings`: persisted global logging schema.
- `oneterm-session-ui`: persisted saved-SSH tri-state and precedence resolution.
- `oneterm-terminal`: parser, file lifecycle, state, and shared controller.
- `oneterm-local-shell` / `oneterm-ssh`: identity and automatic-start lifecycle wiring.
- `oneterm-terminal-view` / `oneterm-settings-ui`: manual controls, settings, notifications, and indicators.

The accepted capture, precedence, and collision semantics are recorded in [`DEC-0003`](decisions/DEC-0003-define-terminal-logging-capture-and-override-semantics.md).
