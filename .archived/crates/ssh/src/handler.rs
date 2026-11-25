//! russh client handler with fail-closed OpenSSH known-hosts verification.

use std::fmt;
use std::path::PathBuf;

use oneterm_core::{AppError, ConnectPhase, HostKeyPolicy};
use russh::client;
use russh::keys::{Algorithm, HashAlg, PublicKey};
use tokio_util::sync::CancellationToken;

use crate::agent::{AgentConnector, spawn_agent_bridge};
use crate::tunnel::{ForwardTable, forwarded_target, spawn_forwarded_tcpip};

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
    /// The host is known, but only under other key algorithms, and the
    /// presented key type is not among them. Treated like a changed key: a
    /// man-in-the-middle can always present a key type the client has never
    /// recorded, so this must never take the friendly "unknown host" path.
    HostKeyAlgorithmMismatch {
        host: String,
        port: u16,
        algorithm: String,
        known_algorithms: Vec<String>,
        fingerprint: String,
    },
    /// The server proved its identity with an OpenSSH host *certificate*.
    ///
    /// Separate from [`Self::UnknownHostKey`] on purpose: a certificate can
    /// never become trusted by approving it, because known_hosts records bare
    /// keys and OneTerm has no certificate-authority trust store. Routing it
    /// through the unknown-key path would show the user a first-use "trust this
    /// host key?" dialog whose Accept can only fail again.
    HostCertificate {
        host: String,
        port: u16,
        algorithm: String,
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
            Self::HostKeyAlgorithmMismatch {
                host,
                port,
                algorithm,
                known_algorithms,
                fingerprint,
            } => write!(
                f,
                "SSH host key changed for {host}:{port}: server presented a {algorithm} key but known_hosts only records {}; refusing connection (SHA-256 fingerprint: {fingerprint})",
                known_algorithms.join(", ")
            ),
            Self::HostCertificate {
                host,
                port,
                algorithm,
                fingerprint,
            } => write!(
                f,
                "{host}:{port} proved its identity with an OpenSSH host certificate ({algorithm}), \
                 which OneTerm does not support; refusing connection. known_hosts records bare \
                 host keys and there is no certificate authority to check this against \
                 (certified key SHA-256 fingerprint: {fingerprint})"
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
            // An algorithm mismatch shares the changed-key user path: no
            // first-use approval; the user must clean up known_hosts by hand.
            Self::ChangedHostKey {
                host,
                port,
                fingerprint,
            }
            | Self::HostKeyAlgorithmMismatch {
                host,
                port,
                fingerprint,
                ..
            } => AppError::HostKeyChanged {
                host: host.clone(),
                port: *port,
                fingerprint: fingerprint.clone(),
            },
            // A plain connect error, deliberately *not* `HostKeyUnknown`: the
            // connect dialog answers that with a first-use approval prompt, and
            // approving a certificate can never succeed.
            Self::HostCertificate { .. } => AppError::Connect {
                phase: ConnectPhase::Transport,
                message: self.to_string(),
            },
            Self::KeyStore(error) => AppError::Connect {
                phase: ConnectPhase::Transport,
                message: format!("known-hosts error: {error}"),
            },
            Self::Russh(error) => AppError::Connect {
                phase: ConnectPhase::Transport,
                message: error.to_string(),
            },
        }
    }
}

/// SSH handler carrying the connection identity and the user's host-key policy.
#[derive(Clone)]
pub(crate) struct SshClientHandler {
    host: String,
    port: u16,
    policy: HostKeyPolicy,
    known_hosts_path: Option<PathBuf>,
    /// Remote forwards this connection asked for, and the session token their
    /// relays die with; `None` for jump hops and connections without forwards.
    forwards: Option<(ForwardTable, CancellationToken)>,
    /// Set only when the session forwards the local agent; without it every
    /// agent channel the server opens is refused with
    /// `AdministrativelyProhibited` (DEC-0011). Before russh 0.63 the channel
    /// was already confirmed by the time the handler ran, so the best OneTerm
    /// could do was close it immediately (`IN-0036`).
    agent_bridge: Option<(AgentConnector, CancellationToken)>,
}

impl SshClientHandler {
    /// Create a fail-closed handler for one SSH connection.
    pub(crate) fn new(host: String, port: u16, policy: HostKeyPolicy) -> Self {
        Self {
            host,
            port,
            policy,
            known_hosts_path: None,
            forwards: None,
            agent_bridge: None,
        }
    }

    /// Bridge server-opened agent channels to the local agent.
    pub(crate) fn with_agent_forwarding(
        mut self,
        connector: AgentConnector,
        shutdown: CancellationToken,
    ) -> Self {
        self.agent_bridge = Some((connector, shutdown));
        self
    }

    pub(crate) fn host(&self) -> &str {
        &self.host
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Accept `forwarded-tcpip` channels for the remote forwards in `table`.
    pub(crate) fn with_forwards(
        mut self,
        table: ForwardTable,
        shutdown: CancellationToken,
    ) -> Self {
        self.forwards = Some((table, shutdown));
        self
    }

    #[cfg(test)]
    pub(crate) fn with_known_hosts_path(mut self, path: PathBuf) -> Self {
        self.known_hosts_path = Some(path);
        self
    }

    fn fingerprint(server_key: &PublicKey) -> String {
        server_key.fingerprint(HashAlg::Sha256).to_string()
    }

    /// Every known_hosts entry recorded for this host and port.
    fn known_host_keys(&self) -> Result<Vec<PublicKey>, russh::keys::Error> {
        let entries = match self.known_hosts_path.as_deref() {
            Some(path) => {
                russh::keys::known_hosts::known_host_keys_path(&self.host, self.port, path)
            }
            None => russh::keys::known_hosts::known_host_keys(&self.host, self.port),
        }?;
        Ok(entries.into_iter().map(|(_, key)| key).collect())
    }

    fn learn_known_host(&self, server_key: &PublicKey) -> Result<(), russh::keys::Error> {
        match self.known_hosts_path.as_deref() {
            Some(path) => russh::keys::known_hosts::learn_known_hosts_path(
                &self.host, self.port, server_key, path,
            ),
            None => russh::keys::known_hosts::learn_known_hosts(&self.host, self.port, server_key),
        }
    }

    /// Host-key algorithms to offer during key exchange, with the algorithms
    /// already recorded in known_hosts for this host moved to the front.
    ///
    /// Servers usually hold one key per type and pick the first client-preferred
    /// type they have, so preferring the recorded types keeps a host known by an
    /// older key type (for example RSA) matching after the server gains newer
    /// keys, instead of tripping the algorithm-mismatch refusal. A known_hosts
    /// read failure keeps the default order; `check_server_key` reports it.
    pub(crate) fn preferred_key_algorithms(&self) -> Vec<Algorithm> {
        match self.known_host_keys() {
            Ok(known) => preferred_key_algorithms(&known),
            Err(error) => {
                log::warn!(
                    "SshClientHandler: known_hosts lookup for host-key preference failed: {error}"
                );
                russh::Preferred::DEFAULT.key.to_vec()
            }
        }
    }

    fn verify_server_key(&self, server_key: &PublicKey) -> Result<bool, SshHandlerError> {
        let fingerprint = Self::fingerprint(server_key);
        let known = self.known_host_keys().map_err(SshHandlerError::KeyStore)?;
        if known.iter().any(|recorded| recorded == server_key) {
            return Ok(true);
        }
        let algorithm = server_key.algorithm();
        if known
            .iter()
            .any(|recorded| recorded.algorithm() == algorithm)
        {
            return Err(SshHandlerError::ChangedHostKey {
                host: self.host.clone(),
                port: self.port,
                fingerprint,
            });
        }
        if !known.is_empty() {
            let mut known_algorithms: Vec<String> = known
                .iter()
                .map(|key| key.algorithm().to_string())
                .collect();
            known_algorithms.sort();
            known_algorithms.dedup();
            return Err(SshHandlerError::HostKeyAlgorithmMismatch {
                host: self.host.clone(),
                port: self.port,
                algorithm: algorithm.to_string(),
                known_algorithms,
                fingerprint,
            });
        }
        match &self.policy {
            HostKeyPolicy::AcceptNewFingerprint(expected) if expected == &fingerprint => {
                self.learn_known_host(server_key)
                    .map_err(SshHandlerError::KeyStore)?;
                Ok(true)
            }
            HostKeyPolicy::Strict | HostKeyPolicy::AcceptNewFingerprint(_) => {
                Err(SshHandlerError::UnknownHostKey {
                    host: self.host.clone(),
                    port: self.port,
                    algorithm: algorithm.to_string(),
                    fingerprint,
                })
            }
        }
    }
}

/// Reorder russh's default host-key algorithm list so every algorithm compatible
/// with a recorded key comes first, keeping the default relative order within
/// both groups. RSA entries match any RSA signature flavour (`rsa-sha2-*`,
/// `ssh-rsa`) because known_hosts records the bare key type.
fn preferred_key_algorithms(known: &[PublicKey]) -> Vec<Algorithm> {
    let is_known = |candidate: &Algorithm| {
        known.iter().any(|key| match candidate {
            Algorithm::Rsa { .. } => key.algorithm().is_rsa(),
            other => key.algorithm() == *other,
        })
    };
    let (known_first, rest): (Vec<Algorithm>, Vec<Algorithm>) = russh::Preferred::DEFAULT
        .key
        .iter()
        .cloned()
        .partition(is_known);
    known_first.into_iter().chain(rest).collect()
}

impl client::Handler for SshClientHandler {
    type Error = SshHandlerError;

    /// known_hosts is read (and possibly appended) on the blocking pool so
    /// the two shared SSH runtime workers never stall on disk I/O (CORR-17).
    ///
    /// russh 0.63 can present an OpenSSH host *certificate* here instead of a
    /// bare key. OneTerm refuses one: it advertises no `*-cert-v01@openssh.com`
    /// host-key algorithm (`Preferred::host_key_certificates` is empty in
    /// `Preferred::DEFAULT`, and OneTerm builds no custom `Preferred`), so a
    /// conforming server never sends one — and there is no certificate-authority
    /// trust store to check one against, because known_hosts records bare keys.
    async fn check_server_key(
        &mut self,
        server_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let server_key = match server_key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            russh::keys::PublicKeyOrCertificate::Certificate(certificate) => {
                return Err(SshHandlerError::HostCertificate {
                    host: self.host.clone(),
                    port: self.port,
                    algorithm: certificate.algorithm().to_string(),
                    fingerprint: format!(
                        "cert:{}",
                        certificate.public_key().fingerprint(HashAlg::Sha256)
                    ),
                });
            }
        };
        let handler = self.clone();
        tokio::task::spawn_blocking(move || handler.verify_server_key(&server_key))
            .await
            .unwrap_or_else(|join_error| {
                Err(SshHandlerError::KeyStore(russh::keys::Error::IO(
                    std::io::Error::other(join_error),
                )))
            })
    }

    /// The server wants the local agent. Bridged only when this session turned
    /// forwarding on; otherwise `reply` is dropped, which refuses the channel
    /// with `AdministrativelyProhibited` (russh 0.63 no longer confirms a
    /// server-initiated channel before calling the handler).
    async fn server_channel_open_agent_forward(
        &mut self,
        channel: russh::Channel<client::Msg>,
        reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        match &self.agent_bridge {
            Some((connector, shutdown)) => {
                // Confirm before the bridge writes anything to the channel.
                reply.accept().await;
                spawn_agent_bridge(channel, connector.clone(), shutdown.clone());
            }
            None => {
                log::warn!(
                    "SshClientHandler: the server opened an agent channel but forwarding is off for this session"
                );
            }
        }
        Ok(())
    }

    /// A connection to one of this session's remote forwards. Channels for
    /// listeners OneTerm never asked for are refused, never bridged: dropping
    /// `reply` sends `AdministrativelyProhibited` (`DEC-0011`).
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let target = self
            .forwards
            .as_ref()
            .and_then(|(table, _)| forwarded_target(table, connected_address, connected_port));
        match (target, &self.forwards) {
            (Some(target), Some((_, shutdown))) => {
                log::debug!(
                    "SshClientHandler: forwarded-tcpip {connected_address}:{connected_port} from {originator_address}:{originator_port} -> {}:{}",
                    target.0,
                    target.1
                );
                // Confirm before the bridge writes anything to the channel.
                reply.accept().await;
                spawn_forwarded_tcpip(channel, target, shutdown.clone());
            }
            _ => {
                log::warn!(
                    "SshClientHandler: refusing a forwarded-tcpip channel for {connected_address}:{connected_port} that was never requested"
                );
            }
        }
        Ok(())
    }
}

// Substantial host-key tests live in a sibling `handler_tests.rs` (see code-style.md).
#[cfg(test)]
#[path = "handler_tests.rs"]
mod handler_tests;
