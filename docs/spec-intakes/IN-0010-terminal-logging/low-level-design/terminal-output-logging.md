# Low-Level Design: terminal-output-logging

Intake: IN-0010
HLD: high-level-design.md
Topic: terminal-output-logging
Date: 2026-08-19

## Concern

Define the runtime capture, file, identity, and state-transition mechanics shared by local and SSH sessions.

## Design

- `TerminalLogConfig` is a backend-neutral value containing `enabled`, `directory`, and `LogWriteMode`. The fixed filename/content templates are constants, not persisted editable strings.
- `TerminalLogController` is an `Arc`-shareable object owned by the session and pump. A mutex protects only logger state; each transport pump is the sole byte producer while UI/background tasks may start or stop it.
- `start` creates the directory, sanitizes the identity to filename-safe characters, opens the timestamped file, and atomically replaces the stopped state only after all fallible setup succeeds. Append uses `create + append`; Overwrite uses `create + write + truncate` once.
- A `vte::Parser` with a small `Perform` implementation receives each transport chunk. `print(char)` appends printable Unicode. LF and CR commit a non-empty message. Backspace removes the latest character and tab appends `\t`; other C0/C1, CSI, OSC, and DCS content is ignored by the parser callbacks.
- Each committed message is written as `[{local time}] {message}\n`. Invalid UTF-8 is handled by VTE parser semantics rather than lossy raw-byte conversion.
- Stop commits an unterminated message, flushes, and drops the writer. Drop/terminal shutdown does the same. A write failure records an error, disables the logger, and emits a reliable logging-state event for UI notification without interrupting the terminal parser.
- The controller publishes `Stopped`, `Running { path }`, or `Failed { message }`. Start is idempotently rejected while running; Stop is harmless while stopped.
- Local identity is completed on the PTY owner thread from the resolved executable basename and the PTY child pid (`Pty::child().id()` on Unix, `Pty::child_watcher().pid()` on Windows) before the read loop starts. SSH identity is known from `SshConfig`.
- Automatic logging setup runs before output pumping begins. Manual Start uses the GPUI background executor for filesystem setup and then notifies the view.

## Interfaces

```rust
pub enum LogWriteMode { Append, Overwrite }
pub struct TerminalLogConfig { pub enabled: bool, pub directory: PathBuf, pub write_mode: LogWriteMode }
pub enum TerminalLogState { Stopped, Running { path: PathBuf }, Failed { message: String } }
pub struct TerminalLogController { /* shared state */ }

impl TerminalLogController {
    pub fn start(&self, config: TerminalLogConfig) -> Result<PathBuf, TerminalLogError>;
    pub fn stop(&self) -> Result<(), TerminalLogError>;
    pub fn state(&self) -> TerminalLogState;
    pub(crate) fn process(&self, bytes: &[u8]);
}
```

`TerminalCapabilities` exposes the controller. `SessionFactory::{spawn_local, connect_ssh}` receives `TerminalLogConfig`; the SSH config carries only connection identity and its already-resolved effective logging policy is passed separately.

Persisted fragments:

```json
"logging": {
  "local": false,
  "ssh": false,
  "directory": "<user_home>/.OneTerm/logs",
  "write_mode": "append"
}
```

```json
"logging": "inherit"
```

## Edge Cases and Failure Modes

- [x] Folder does not exist: create it recursively on Start.
- [x] Folder cannot be created/file cannot be opened: remain stopped and notify the user.
- [x] Invalid host/user/shell characters: replace with `_`; never allow path separators or `..` traversal through `%n`.
- [x] Same-second collision: Append reuses and appends; Overwrite truncates once at Start as explicitly accepted.
- [x] Partial line on Stop/close: commit once, then flush.
- [x] ANSI/OSC/DCS payloads: excluded because only VTE `print` callbacks enter the message.
- [x] Logging write failure: stop logging and surface the error; terminal parsing/rendering continues.
- [x] Multiple Spaces: logging state remains per terminal, not per tab.

## Verification

- [ ] Unit tests for printable parsing across chunks, LF/CR/backspace/tab, control stripping, partial-line flush, and write failure state.
- [ ] Filesystem tests for default naming, sanitization, Append, Overwrite, and folder creation in temporary directories.
- [ ] Settings and SSH persistence round-trip/default compatibility tests.
- [ ] Terminal panel/view tests for state-based controls and single-/multi-Space indicator predicates.
- [ ] Full workspace quality gate.
