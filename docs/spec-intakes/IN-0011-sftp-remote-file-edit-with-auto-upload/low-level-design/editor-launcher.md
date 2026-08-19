# Low-Level Design: Editor launcher

Intake: IN-0011
HLD: ../high-level-design.md
Topic: editor-launcher
Date: 2026-08-19

## Concern

Launching a local file in an editor from `EditorConfig`: the OS-default opener,
the custom-command path, argv safety, and where this helper lives.

## Design

### Home crate

The launcher is pure, gpui-free logic (config in → spawned process out) and
benefits from unit tests, so it lives in **`crates/core`** as
`crates/core/src/editor_launcher.rs`, re-exported from `oneterm_core`. This keeps
`crates/sftp-ui` free of process-spawning details and lets the resolution logic
be tested without a UI. `crates/core` already owns `config_dir()` and other
process/filesystem helpers, so this fits its role.

> Note: `EditorConfig` is defined in `crates/settings`. To avoid a
> `core → settings` dependency (settings depends on core, not the reverse), the
> launcher takes a small **owned parameter struct** rather than importing
> `EditorConfig` directly. The caller (`crates/sftp-ui`, which depends on both)
> maps `EditorConfig` → the launcher's parameter type.

```rust
// crates/core/src/editor_launcher.rs

/// What editor to launch — a UI/settings-agnostic view of the editor config.
pub enum EditorChoice {
    /// Use the OS default application for the file's type.
    OsDefault,
    /// Spawn `program` with `args` followed by the file path.
    Custom { program: String, args: Vec<String> },
}

/// Launch `path` in the chosen editor. Returns once the process has been
/// spawned (fire-and-forget); the editor keeps running independently.
pub fn launch_editor(choice: &EditorChoice, path: &Path) -> Result<()> {
    match choice {
        EditorChoice::Custom { program, args } if !program.trim().is_empty() => {
            spawn_custom(program, args, path)
        }
        // OsDefault, or Custom with an empty program → OS default.
        _ => open_with_os_default(path),
    }
}
```

### Argv safety

Both paths use `std::process::Command` with **separate arguments** — never a
single shell string:

```rust
fn spawn_custom(program: &str, args: &[String], path: &Path) -> Result<()> {
    Command::new(program)
        .args(args)          // pre-file arguments from config
        .arg(path)           // the temp path as its own argv entry
        .spawn()             // do not wait; editor outlives this call
        .map(|_child| ())
        .map_err(|e| AppError::msg(format!("failed to launch editor '{program}': {e}")))
}
```

Because the path is a distinct `arg`, a file name containing spaces, quotes, or
shell metacharacters cannot break out into a command. `.spawn()` (not
`.output()`/`.status()`) so we do not block the UI waiting for the editor.

### OS-default opener

Use the small, widely used **`open` crate** (pinned in
`[workspace.dependencies]`, DEC-0004), which wraps the per-OS launcher and
already handles the Windows `start` title-argument pitfall:

```rust
fn open_with_os_default(path: &Path) -> Result<()> {
    // `open::that` spawns the OS default handler for `path` and returns without
    // waiting for it to exit.
    open::that(path)
        .map_err(|e| AppError::msg(format!("failed to open with OS default: {e}")))
}
```

`open::that` (non-blocking) is preferred over `open::that_in_background` unless
profiling shows the spawn blocks the caller; either keeps the editor running
independently. The choice does not affect `launch_editor`'s signature.

### Failure surfacing

`launch_editor` returns `Result`. `crates/sftp-ui` maps an error to a user
notification ("Could not open the editor …") and **aborts that edit session**
(drops the watcher, deletes the temp copy) so a failed launch leaves no orphaned
watcher or temp file. A Custom mode with an empty `program` transparently falls
back to OS default (per `editor-config.md`).

## Interfaces

```rust
// oneterm_core
pub enum EditorChoice { OsDefault, Custom { program: String, args: Vec<String> } }
pub fn launch_editor(choice: &EditorChoice, path: &Path) -> Result<()>;
```

## Edge Cases and Failure Modes

- [ ] Custom `program` not found on PATH → `spawn` errors → notify + abort the
  session.
- [ ] Empty custom `program` → fall back to OS default.
- [ ] File name with spaces/`&`/`;` → passed as one argv element; no
  injection, no splitting.
- [ ] No OS default association (Windows/Linux) → `start`/`xdg-open` may error or
  pop a "choose an app" dialog; the error path notifies the user.
- [ ] Editor is a GUI app that detaches immediately → still fine; the watcher,
  not the process handle, drives the workflow.

## Verification

- [ ] Unit: `EditorChoice::Custom { program: "" }` resolves to the OS-default
  branch; `Custom` with a program builds a `Command` whose args are
  `[..config args.., path]` (assert via a testable command-builder split out
  from the spawn call).
- [ ] Unit: an argv builder keeps a metacharacter-laden path as a single
  element.
- [ ] `cargo test -p oneterm-core`.
- [ ] Manual (Windows primary): OS default opens the associated app; a custom
  command (e.g. `code -n`) opens the file; a bogus program surfaces a
  notification and cleans up.
