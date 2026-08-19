//! Terminal printable-output logging settings.

use std::path::PathBuf;

use oneterm_core::{LogWriteMode, TerminalLogConfig, default_terminal_log_dir};
use serde::{Deserialize, Serialize};

/// Persisted automatic logging policy and file destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Automatically log newly created local shells.
    #[serde(default)]
    pub local: bool,
    /// Automatically log newly connected SSH terminals.
    #[serde(default)]
    pub ssh: bool,
    /// Folder that receives terminal log files.
    #[serde(default = "default_terminal_log_dir")]
    pub directory: PathBuf,
    /// Existing-file behavior.
    #[serde(default)]
    pub write_mode: LogWriteMode,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            local: false,
            ssh: false,
            directory: default_terminal_log_dir(),
            write_mode: LogWriteMode::Append,
        }
    }
}

impl LoggingConfig {
    /// Resolve the startup configuration for a local shell.
    pub fn local_config(&self) -> TerminalLogConfig {
        self.runtime_config(self.local)
    }

    /// Resolve the startup configuration for an SSH terminal.
    pub fn ssh_config(&self, enabled: bool) -> TerminalLogConfig {
        self.runtime_config(enabled)
    }

    fn runtime_config(&self, enabled: bool) -> TerminalLogConfig {
        TerminalLogConfig {
            enabled,
            directory: self.directory.clone(),
            write_mode: self.write_mode,
        }
    }
}
