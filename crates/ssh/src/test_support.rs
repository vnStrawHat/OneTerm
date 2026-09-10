//! Test-only helpers: an in-process `russh` server on a loopback port and a
//! client connection that trusts that server through a temporary `known_hosts`
//! file, so tests never touch the developer's real one.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::client;
use russh::server::Server;

use crate::handler::{SshClientHandler, SshHandlerError};

/// Deletes a temporary known_hosts file when the test ends, even when an
/// assertion fails (ERR-15).
pub(crate) struct TempKnownHosts(pub(crate) std::path::PathBuf);

impl Drop for TempKnownHosts {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("failed to remove {}: {error}", self.0.display());
        }
    }
}

/// Run `server` on an ephemeral loopback port with a fresh Ed25519 host key
/// and no authentication back-off. The test aborts the returned task when done.
pub(crate) async fn spawn_server<S>(mut server: S) -> (SocketAddr, tokio::task::JoinHandle<()>)
where
    S: Server + Send + 'static,
{
    let private_key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
            .expect("host key generation");
    let server_config = Arc::new(russh::server::Config {
        keys: vec![private_key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("loopback listener");
    let address = listener.local_addr().expect("listener address");
    let server_task = tokio::spawn(async move {
        // The test aborts this task when done; a run error is expected then.
        if let Err(error) = server.run_on_socket(server_config, &listener).await {
            eprintln!("test SSH server stopped: {error}");
        }
    });
    (address, server_task)
}

/// Connect to a loopback test server under the strict host-key policy after
/// learning its key with a probe connection into a temporary `known_hosts`.
pub(crate) async fn connect_trusting_loopback(
    address: SocketAddr,
) -> (client::Handle<SshClientHandler>, TempKnownHosts) {
    let known_hosts = TempKnownHosts(std::env::temp_dir().join(format!(
        "oneterm-test-known-hosts-{}-{}",
        std::process::id(),
        address.port()
    )));
    let probe = client::connect(
        Arc::new(client::Config::default()),
        address,
        SshClientHandler::new(
            address.ip().to_string(),
            address.port(),
            oneterm_core::HostKeyPolicy::Strict,
        )
        .with_known_hosts_path(known_hosts.0.clone()),
    )
    .await;
    let fingerprint = match probe {
        Err(SshHandlerError::UnknownHostKey { fingerprint, .. }) => fingerprint,
        other => panic!("expected an unknown host key, got {:?}", other.err()),
    };
    let handle = client::connect(
        Arc::new(client::Config::default()),
        address,
        SshClientHandler::new(
            address.ip().to_string(),
            address.port(),
            oneterm_core::HostKeyPolicy::AcceptNewFingerprint(fingerprint),
        )
        .with_known_hosts_path(known_hosts.0.clone()),
    )
    .await
    .expect("loopback connect");
    (handle, known_hosts)
}
