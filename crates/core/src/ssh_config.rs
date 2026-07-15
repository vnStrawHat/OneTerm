//! SSH connection config — shared connect parameters.
//!
//! `SshConfig` holds host info + credentials, built from the UI store + the
//! connect dialog and consumed by the SSH backend (`oneterm-ssh`). It lives in
//! this leaf crate so the UI feature crates and the backend can share it without
//! a UI→backend dependency edge. Passwords are **not** persisted to disk — they
//! live in RAM only for the duration of the connection.

use std::fmt::{self, Debug, Formatter};
use std::path::PathBuf;

/// SSH authentication method.
#[derive(Clone)]
pub enum SshAuthMethod {
    /// No authentication (for servers that require no password).
    None,
    /// Password authentication.
    Password {
        /// Plaintext password (RAM only — never logged or serialized).
        password: String,
    },
    /// Private key file authentication.
    PrivateKey {
        /// Path to the private key file.
        key_path: PathBuf,
        /// Passphrase to decrypt the key (if the key is encrypted).
        passphrase: Option<String>,
    },
    /// Authentication via SSH agent.
    Agent,
}

/// SSH connection config.
///
/// Built from `SshSession` info (UI store) + credentials the user enters in the
/// connect dialog. Passed to the SSH backend's `connect`.
#[derive(Clone)]
pub struct SshConfig {
    /// Hostname or IP.
    pub host: String,
    /// SSH port (default 22).
    pub port: u16,
    /// SSH username.
    pub username: String,
    /// Authentication method.
    pub auth: SshAuthMethod,
    /// Inject shell integration (OSC 7 cwd + OSC 133 prompt markers) into the
    /// remote shell right after the shell starts. Enables the SFTP "sync to
    /// terminal cwd" button on servers whose shell does not emit OSC 7 by default.
    /// Idempotent + shell-guarded (bash/zsh); no-op on unrecognized shells.
    pub shell_integration: bool,
}

// ── Debug impl (mask password) ──────────────────────────────────────

impl Debug for SshAuthMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password { .. } => f
                .debug_struct("Password")
                .field("password", &"***")
                .finish(),
            Self::None => f.write_str("None"),
            Self::PrivateKey { key_path, .. } => f
                .debug_struct("PrivateKey")
                .field("key_path", key_path)
                .field("passphrase", &"***")
                .finish(),
            Self::Agent => f.write_str("Agent"),
        }
    }
}

impl Debug for SshConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth) // password masked via the impl above
            .finish()
    }
}
