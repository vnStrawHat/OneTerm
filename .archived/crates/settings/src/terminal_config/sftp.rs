//! SFTP group: the `sftp` block in `terminal.json`.
//!
//! Configures the SFTP browser's "Edit" workflow: which editor opens a remote
//! file locally and the maximum file size opened without a confirmation prompt.
//! Each struct is `#[serde(default)]` so an old `terminal.json` without an
//! `sftp` block — or with only some fields — loads the rest from `Default`, the
//! same pattern as `CompletionConfig` / `LoggingConfig`.

use serde::{Deserialize, Serialize};

/// Default maximum edit file size (bytes) opened without a confirmation: 1 MiB.
pub const DEFAULT_EDIT_MAX_FILE_SIZE: u64 = 1024 * 1024;

/// How the "Edit" action opens a remote file locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorMode {
    /// Open with the operating system's default application for the file type.
    #[default]
    OsDefault,
    /// Open with a user-specified command.
    Custom,
}

/// Editor configuration for the SFTP "Edit" workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    /// Which launcher to use.
    pub mode: EditorMode,
    /// Custom editor program (only used when `mode == Custom`). Empty = unset,
    /// which falls back to the OS default.
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
    /// Editor used by the "Edit" action.
    pub editor: EditorConfig,
    /// Maximum remote file size (bytes) the "Edit" action opens without a
    /// confirmation prompt. `0` = no limit. Default = 1 MiB.
    pub edit_max_file_size: u64,
}

impl Default for SftpConfig {
    fn default() -> Self {
        Self {
            editor: EditorConfig::default(),
            edit_max_file_size: DEFAULT_EDIT_MAX_FILE_SIZE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_os_default_editor_and_one_mib_limit() {
        let cfg = SftpConfig::default();
        assert_eq!(cfg.editor.mode, EditorMode::OsDefault);
        assert!(cfg.editor.program.is_empty());
        assert!(cfg.editor.args.is_empty());
        assert_eq!(cfg.edit_max_file_size, DEFAULT_EDIT_MAX_FILE_SIZE);
    }

    #[test]
    fn missing_block_deserializes_to_default() {
        // An empty JSON object must fill every field from Default.
        let cfg: SftpConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg, SftpConfig::default());
    }

    #[test]
    fn partial_block_fills_remaining_fields_from_default() {
        // Only the editor mode is present; program/args/limit come from Default.
        let cfg: SftpConfig = serde_json::from_str(r#"{"editor":{"mode":"custom"}}"#).unwrap();
        assert_eq!(cfg.editor.mode, EditorMode::Custom);
        assert!(cfg.editor.program.is_empty());
        assert_eq!(cfg.edit_max_file_size, DEFAULT_EDIT_MAX_FILE_SIZE);
    }

    #[test]
    fn explicit_values_round_trip() {
        let cfg = SftpConfig {
            editor: EditorConfig {
                mode: EditorMode::Custom,
                program: "code".into(),
                args: vec!["-n".into(), "--wait".into()],
            },
            edit_max_file_size: 5 * 1024 * 1024,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SftpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }
}
