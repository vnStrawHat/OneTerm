//! Backend-neutral terminal logging configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::home_dir;

/// Fixed terminal log filename template.
pub const LOG_FILE_NAME_FORMAT: &str = "%n_%Y-%m-%d_%H-%M-%S.log";
/// Fixed terminal log record template.
pub const LOG_CONTENT_FORMAT: &str = "[%Y-%m-%d %H:%M:%S] %msg";

/// How an existing terminal log file is opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogWriteMode {
    /// Preserve existing content and write new records at the end.
    #[default]
    Append,
    /// Truncate the file once when logging starts.
    Overwrite,
}

/// Startup configuration resolved for one terminal session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLogConfig {
    /// Start logging before terminal output is pumped.
    pub enabled: bool,
    /// Folder that receives the log file.
    pub directory: PathBuf,
    /// Existing-file behavior.
    pub write_mode: LogWriteMode,
}

impl Default for TerminalLogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: default_terminal_log_dir(),
            write_mode: LogWriteMode::Append,
        }
    }
}

/// Default terminal-log folder, independent of debug configuration storage.
pub fn default_terminal_log_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".OneTerm")
        .join("logs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_log_folder_ends_in_oneterm_logs() {
        assert!(default_terminal_log_dir().ends_with(PathBuf::from(".OneTerm").join("logs")));
    }
}
