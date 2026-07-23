//! SSH connection config — shared connect parameters.
//!
//! `SshConfig` holds host info + credentials, built from the UI store + the
//! connect dialog and consumed by the SSH backend (`oneterm-ssh`). It lives in
//! this leaf crate so the UI feature crates and the backend can share it without
//! a UI→backend dependency edge. Credentials are **not** persisted to disk and
//! are zeroized when their final in-memory owner is dropped.

use std::fmt::{self, Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::{Zeroize, ZeroizeOnDrop};

/// An in-memory secret that clears its backing allocation when dropped.
///
/// The wrapper deliberately does not implement `Display`, serialization, or
/// transparent debug output. Cloning is supported because connection configs
/// cross the app/backend boundary, but callers should avoid retaining clones.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretString(String);

impl SecretString {
    /// Wrap a plaintext secret.
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// Borrow the plaintext only for the operation that needs it.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    /// Return whether this secret is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Debug for SecretString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Cooperative cancellation handle for one SSH connection attempt.
#[derive(Clone, Debug, Default)]
pub struct ConnectionCancellation(Arc<AtomicBool>);

impl ConnectionCancellation {
    /// Request cancellation. The SSH backend checks this during every phase.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Return whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
/// Policy used for a server key that is not already in OpenSSH `known_hosts`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Require a matching key in `~/.ssh/known_hosts`; unknown keys fail closed.
    #[default]
    Strict,
    /// Trust and persist exactly this SHA-256 fingerprint once.
    ///
    /// The SSH handler still rejects a key changed from an existing entry. This
    /// variant is set only after the user confirms the fingerprint shown by the
    /// first failed connection attempt.
    AcceptNewFingerprint(String),
}

/// SSH authentication method.
#[derive(Clone)]
pub enum SshAuthMethod {
    /// No authentication (for servers that require no password).
    None,
    /// Password authentication.
    Password {
        /// Zeroizing password (RAM only — never logged or serialized).
        password: SecretString,
    },
    /// Private key file authentication.
    PrivateKey {
        /// Path to the private key file.
        key_path: PathBuf,
        /// Zeroizing passphrase to decrypt the key, if encrypted.
        passphrase: Option<SecretString>,
    },
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
    /// Cooperative cancellation owned by the initiating UI operation.
    pub cancellation: ConnectionCancellation,
    /// Server host-key verification policy.
    pub host_key_policy: HostKeyPolicy,
    /// Inject shell integration (OSC 7 cwd + OSC 133 prompt markers) into the
    /// remote shell right after the shell starts. Enables the SFTP "sync to
    /// terminal cwd" button on servers whose shell does not emit OSC 7 by default.
    /// Idempotent + shell-guarded (bash/zsh); no-op on unrecognized shells.
    pub shell_integration: bool,
}

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
        }
    }
}

impl Debug for SshConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth)
            .field("host_key_policy", &self.host_key_policy)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_output_is_masked() {
        let secret = SecretString::new("do-not-print");
        assert_eq!(format!("{secret:?}"), "***");
    }

    #[test]
    fn config_debug_output_masks_password() {
        let config = SshConfig {
            host: "example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            auth: SshAuthMethod::Password {
                password: SecretString::new("do-not-print"),
            },
            cancellation: ConnectionCancellation::default(),
            host_key_policy: HostKeyPolicy::Strict,
            shell_integration: true,
        };

        let output = format!("{config:?}");
        assert!(!output.contains("do-not-print"));
        assert!(output.contains("***"));
    }
}
