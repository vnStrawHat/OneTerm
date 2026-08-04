//! russh client handler with fail-closed OpenSSH known-hosts verification.

use std::fmt;
use std::path::PathBuf;

use oneterm_core::{AppError, HostKeyPolicy};
use russh::client;
use russh::keys::{HashAlg, PublicKey};

/// Handler errors that preserve enough host-key information for the UI to ask
/// for explicit first-use approval.
#[derive(Debug)]
pub(crate) enum SshHandlerError {
    /// A key was not found in known_hosts.
    UnknownHostKey {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    /// A key differs from an existing known_hosts entry.
    ChangedHostKey {
        host: String,
        port: u16,
        fingerprint: String,
    },
    /// The known_hosts file could not be read or updated.
    KeyStore(russh::keys::Error),
    /// An ordinary russh connection error.
    Russh(russh::Error),
}

impl fmt::Display for SshHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownHostKey {
                host,
                port,
                algorithm,
                fingerprint,
            } => write!(
                f,
                "unknown SSH host key for {host}:{port} ({algorithm}), SHA-256 fingerprint: {fingerprint}"
            ),
            Self::ChangedHostKey {
                host,
                port,
                fingerprint,
            } => write!(
                f,
                "SSH host key changed for {host}:{port}; refusing connection (SHA-256 fingerprint: {fingerprint})"
            ),
            Self::KeyStore(error) => write!(f, "SSH known-hosts error: {error}"),
            Self::Russh(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SshHandlerError {}

impl From<russh::Error> for SshHandlerError {
    fn from(error: russh::Error) -> Self {
        Self::Russh(error)
    }
}

impl SshHandlerError {
    /// Convert a handler failure to the shared application error boundary.
    pub(crate) fn to_app_error(&self) -> AppError {
        match self {
            Self::UnknownHostKey {
                host,
                port,
                algorithm,
                fingerprint,
            } => AppError::HostKeyUnknown {
                host: host.clone(),
                port: *port,
                algorithm: algorithm.clone(),
                fingerprint: fingerprint.clone(),
            },
            Self::ChangedHostKey {
                host,
                port,
                fingerprint,
            } => AppError::HostKeyChanged {
                host: host.clone(),
                port: *port,
                fingerprint: fingerprint.clone(),
            },
            Self::KeyStore(error) => AppError::msg(format!("SSH known-hosts error: {error}")),
            Self::Russh(error) => AppError::msg(error.to_string()),
        }
    }
}

/// SSH handler carrying the connection identity and the user's host-key policy.
pub(crate) struct SshClientHandler {
    host: String,
    port: u16,
    policy: HostKeyPolicy,
    known_hosts_path: Option<PathBuf>,
}

impl SshClientHandler {
    /// Create a fail-closed handler for one SSH connection.
    pub(crate) fn new(host: String, port: u16, policy: HostKeyPolicy) -> Self {
        Self {
            host,
            port,
            policy,
            known_hosts_path: None,
        }
    }

    #[cfg(test)]
    fn with_known_hosts_path(mut self, path: PathBuf) -> Self {
        self.known_hosts_path = Some(path);
        self
    }

    fn fingerprint(server_key: &PublicKey) -> String {
        server_key.fingerprint(HashAlg::Sha256).to_string()
    }

    fn check_known_host(&self, server_key: &PublicKey) -> Result<bool, russh::keys::Error> {
        match self.known_hosts_path.as_deref() {
            Some(path) => {
                russh::keys::check_known_hosts_path(&self.host, self.port, server_key, path)
            }
            None => russh::keys::check_known_hosts(&self.host, self.port, server_key),
        }
    }

    fn learn_known_host(&self, server_key: &PublicKey) -> Result<(), russh::keys::Error> {
        match self.known_hosts_path.as_deref() {
            Some(path) => russh::keys::known_hosts::learn_known_hosts_path(
                &self.host, self.port, server_key, path,
            ),
            None => russh::keys::known_hosts::learn_known_hosts(&self.host, self.port, server_key),
        }
    }

    fn verify_server_key(&self, server_key: &PublicKey) -> Result<bool, SshHandlerError> {
        let fingerprint = Self::fingerprint(server_key);
        match self.check_known_host(server_key) {
            Ok(true) => Ok(true),
            Ok(false) => match &self.policy {
                HostKeyPolicy::Strict => Err(SshHandlerError::UnknownHostKey {
                    host: self.host.clone(),
                    port: self.port,
                    algorithm: server_key.algorithm().to_string(),
                    fingerprint,
                }),
                HostKeyPolicy::AcceptNewFingerprint(expected) if expected == &fingerprint => {
                    self.learn_known_host(server_key)
                        .map_err(SshHandlerError::KeyStore)?;
                    Ok(true)
                }
                HostKeyPolicy::AcceptNewFingerprint(_) => Err(SshHandlerError::UnknownHostKey {
                    host: self.host.clone(),
                    port: self.port,
                    algorithm: server_key.algorithm().to_string(),
                    fingerprint,
                }),
            },
            Err(russh::keys::Error::KeyChanged { .. }) => Err(SshHandlerError::ChangedHostKey {
                host: self.host.clone(),
                port: self.port,
                fingerprint,
            }),
            Err(error) => Err(SshHandlerError::KeyStore(error)),
        }
    }
}

impl client::Handler for SshClientHandler {
    type Error = SshHandlerError;

    async fn check_server_key(
        &mut self,
        server_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        self.verify_server_key(server_key)
    }
}

// Substantial host-key tests live in a sibling `handler_tests.rs` (see code-style.md).
#[cfg(test)]
#[path = "handler_tests.rs"]
mod handler_tests;
