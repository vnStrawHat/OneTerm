# Low-Level Design: SFTP editor configuration

Intake: IN-0011
HLD: ../high-level-design.md
Topic: editor-config
Date: 2026-08-19

## Concern

The persisted editor configuration and its Settings UI: a new `sftp` group in
`terminal.json`, the `EditorConfig` schema, defaults, and how the "SFTP"
settings page reads and writes it.

## Design

### Config group

Follow the existing per-group pattern in `crates/settings/src/terminal_config/`
(see `completion.rs`, `logging.rs`): a `#[serde(default)]` struct that loads from
`Default` when the group or a field is missing, so an old `terminal.json` keeps
working.

New file `crates/settings/src/terminal_config/sftp.rs`:

```rust
//! SFTP group: the `sftp` block in `terminal.json`.

use serde::{Deserialize, Serialize};

/// How the "Edit" action opens a remote file locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorMode {
    /// Open with the operating system's default application for the file type.
    OsDefault,
    /// Open with a user-specified command.
    Custom,
}

impl Default for EditorMode {
    fn default() -> Self {
        EditorMode::OsDefault
    }
}

/// Editor configuration for the SFTP "Edit" workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    /// Which launcher to use.
    pub mode: EditorMode,
    /// Custom editor program (only used when `mode == Custom`). Empty = unset.
    pub program: String,
    /// Extra arguments passed before the file path (argv, not a shell string).
    pub args: Vec<String>,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            mode: EditorMode::OsDefault,
            program: String::new(),
            args: Vec::new(),
        }
    }
}

/// The `sftp` config group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SftpConfig {
    pub editor: EditorConfig,
    /// Maximum remote file size (bytes) the "Edit" action opens without a
    /// confirmation prompt. `0` = no limit. Default = 1 MiB.
    pub edit_max_file_size: u64,
}

impl Default for SftpConfig {
    fn default() -> Self {
        Self {
            editor: EditorConfig::default(),
            edit_max_file_size: 1024 * 1024, // 1 MiB
        }
    }
}
```

Wire-in points (mirror `completion`):

- `crates/settings/src/terminal_config/document.rs`:
  - add `pub sftp: SftpConfig` (with `#[serde(default)]`) to `TerminalConfig`;
  - re-export `SftpConfig`, `EditorConfig`, `EditorMode` from the module and
    from `crates/settings/src/lib.rs`.
- No migration code: `#[serde(default)]` supplies the group for old files, and
  `serialize_document` writes it out on the next save (same as other groups).

### Live settings global

The Settings UI currently reads/writes `TerminalSettings` (terminal.json). The
editor config is read at *edit time*, not per-keystroke, so the simplest correct
source is to load `TerminalConfig` when starting an edit and read
`config.sftp.editor`. If a live global is already threaded for the Terminal
page, reuse it; otherwise a direct `TerminalConfig::load()` at edit start is
acceptable (edits are infrequent). The exact accessor is chosen to match how the
Terminal settings page persists today — no new global is introduced unless the
existing page already relies on one.

### Settings page

New file `crates/settings-ui/src/sftp.rs` exposing `pub(crate) fn page(cx) ->
SettingPage`, registered in `SettingsPanel::pages` after `terminal::page()`.

Group "Editor":

- A choice/segmented field for `EditorMode` (OS Default | Custom).
- A text field for `program`, enabled only when mode is Custom.
- A text field for `args` (space-separated, parsed to `Vec<String>`), enabled
  only when mode is Custom.

Group "Edit":

- A number field for the **maximum edit file size in MB** (getter divides
  `edit_max_file_size` bytes by 1 MiB; setter multiplies back). `0` means no
  limit. Default shows `1`. Use `NumberFieldOptions { min: 0.0, .. }`, matching
  the existing number-field pattern in `general.rs`.

Getter closures read the current `SftpConfig`; setter closures mutate it and
persist through the same `terminal.json` save path the Terminal page uses. When
mode is OS Default, the program/args fields are shown disabled (or hidden) so
the OS-default behavior is the clear default.

## Interfaces

```rust
// crates/settings/src/terminal_config/sftp.rs
pub enum EditorMode { OsDefault, Custom }
pub struct EditorConfig { pub mode: EditorMode, pub program: String, pub args: Vec<String> }
pub struct SftpConfig { pub editor: EditorConfig, pub edit_max_file_size: u64 }

// crates/settings/src/lib.rs (re-export)
pub use terminal_config::{EditorConfig, EditorMode, SftpConfig, /* … */};

// crates/settings/src/terminal_config/document.rs
pub struct TerminalConfig { /* … */ pub sftp: SftpConfig }

// crates/settings-ui/src/sftp.rs
pub(crate) fn page(cx: &App) -> SettingPage;
```

## Edge Cases and Failure Modes

- [ ] `terminal.json` has no `sftp` block → `SftpConfig::default()` (OS default,
  1 MiB edit limit).
- [ ] `edit_max_file_size = 0` → the Edit action opens any size without the
  size confirmation (enforced in `edit-session-lifecycle.md`); the config layer
  simply stores `0`.
- [ ] MB field shows a fractional value for a non-MiB-aligned stored byte count;
  the getter/setter round-trips through bytes so the stored value is exact.
- [ ] Mode is Custom but `program` is empty → treat as unconfigured; the launch
  step falls back to OS default and surfaces a notification suggesting the user
  set an editor (handled in `editor-launcher.md`, but the config layer must not
  reject an empty program at save time — the user may be mid-edit of the field).
- [ ] `args` with quoted tokens → v1 uses simple whitespace split; document the
  limitation (no shell quoting). A quoted-arg parser is out of scope for v1.
- [ ] Round-trip: saving then reloading `terminal.json` yields an identical
  `SftpConfig` (idempotent serialize, matching the existing document test style).

## Verification

- [ ] Unit (in `sftp.rs` `#[cfg(test)]` or `document_tests.rs`): default when
  absent; explicit round-trip; idempotent re-serialize; unknown/legacy document
  still loads with OS-default editor.
- [ ] `cargo test -p oneterm-settings`.
- [ ] Manual: the "SFTP" page appears, toggling mode enables/disables the custom
  fields, and edits persist across a settings-window reopen.
