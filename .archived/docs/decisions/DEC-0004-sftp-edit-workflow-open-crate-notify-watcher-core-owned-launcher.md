# DEC-0004 SFTP edit workflow: open crate, notify watcher, core-owned launcher

Date: 2026-08-19

## Status

accepted

## Context

The SFTP remote-file edit workflow (IN-0011) needs three cross-cutting choices
that later work must inherit: how to open a file in the OS default application,
how to detect a local save, and where the launcher logic lives. Each choice adds
a pinned dependency or fixes a crate boundary, so it belongs in one durable
record rather than being re-decided per work packet.

## Decision

Future SFTP edit work must:

- open files in the OS default application through the **`open` crate** (pinned
  once in the root `Cargo.toml` `[workspace.dependencies]`), not a hand-rolled
  per-OS `Command`. Custom editor commands are still spawned directly via
  `std::process::Command` with an explicit argv (the file path as a separate
  argument, never a shell string);
- detect local saves with the **`notify` crate** file watcher (pinned once in
  the root `Cargo.toml`), running off the UI thread and forwarding debounced
  events to the UI thread over `async_channel`;
- place the editor launcher in **`crates/core`**
  (`oneterm_core::editor_launcher`) as gpui-free, unit-testable logic. It takes
  an owned `EditorChoice` value, so `core` gains no dependency on `settings`;
  `crates/sftp-ui` maps `EditorConfig` → `EditorChoice`.

## Alternatives

- [x] Selected approach described above.
- [ ] Hand-rolled per-OS opener (`cmd /C start`, `open`, `xdg-open`): rejected
  because the `open` crate already handles the Windows `start` title-argument
  pitfall and per-OS quirks, reducing bespoke unsafe-ish shell handling.
- [ ] Poll the temp file's mtime on the existing timer instead of `notify`:
  rejected because it adds up to one poll interval of latency and re-purposes the
  panel poll timer; the user asked for prompt save detection.
- [ ] Launcher in `crates/sftp-ui`: rejected because process-spawn/OS-default
  logic is easier to unit-test and reuse when it stays UI-free in `core`.

## Consequences

- [ ] Benefit to confirm: OS-default open is cross-platform with minimal bespoke
  code; saves are detected promptly; launch resolution is unit-testable.
- [ ] Tradeoff to address: two new pinned third-party dependencies (`open`,
  `notify`) enter the workspace; both must be declared once and reviewed like
  any other dependency (`docs/agents/dependencies.md`).
- [ ] Tradeoff to address: the `notify` watcher runs a background thread whose
  events must be marshalled to the UI thread; the edit-session registry owns the
  watcher lifetime so a dropped session stops watching.
