//! SSH connection config — input for [`crate::SshSession::connect`].
//!
//! `SshConfig` holds host info + credentials. Passwords are **not** persisted
//! to disk — they live in RAM only for the duration of the connection.

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
/// connect dialog. Passed to [`crate::SshSession::connect`].
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
