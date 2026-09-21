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

    /// The one place both callers resolve through, so the elevation gate is
    /// written once (`IN-0043` MAJ-4).
    fn runtime_config(&self, enabled: bool) -> TerminalLogConfig {
        Self::gated(
            TerminalLogConfig {
                enabled,
                directory: self.directory.clone(),
                write_mode: self.write_mode,
            },
            oneterm_core::elevation::is_restricted(),
        )
    }

    /// M4 (`DEC-0019`): an elevated window writes no configuration — and a
    /// terminal log is a file it would create, at a path `terminal.json` chose,
    /// under an administrator token. With `LogWriteMode::Overwrite` that is a
    /// create-or-**truncate** primitive at an arbitrary path, which is a
    /// destructive capability on its own and not something an elevated window
    /// needs. Logging is therefore off there regardless of what the file says.
    ///
    /// `restricted` is a parameter so the rule has a unit test that neither
    /// needs Windows nor flips the process global under the other tests.
    fn gated(config: TerminalLogConfig, restricted: bool) -> TerminalLogConfig {
        if restricted {
            return TerminalLogConfig {
                enabled: false,
                ..config
            };
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MAJ-4`: a `terminal.json` asking for logging gets none in an elevated
    /// window, whichever caller resolved it and whatever write mode it chose.
    #[test]
    fn an_elevated_window_never_opens_a_terminal_log_file() {
        let asked = TerminalLogConfig {
            enabled: true,
            directory: PathBuf::from(r"C:\Windows\System32\drivers\etc"),
            write_mode: LogWriteMode::Overwrite,
        };
        assert!(
            !LoggingConfig::gated(asked.clone(), true).enabled,
            "an elevated window must open no log file at a config-chosen path"
        );
        assert!(
            LoggingConfig::gated(asked, false).enabled,
            "an ordinary window keeps logging exactly as before"
        );
    }

    #[test]
    fn both_callers_route_through_the_same_gate() {
        let config = LoggingConfig {
            local: true,
            ssh: true,
            ..LoggingConfig::default()
        };
        // Off Windows and unelevated here, so both are simply the configured
        // value; the gate itself is covered by the test above.
        assert!(config.local_config().enabled);
        assert!(config.ssh_config(true).enabled);
        assert!(!config.ssh_config(false).enabled);
    }
}
