//! Host-key verification tests for the SSH client handler.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";

fn temporary_known_hosts() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oneterm-known-hosts-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn strict_policy_rejects_unknown_key() {
    let path = temporary_known_hosts();
    let key = russh::keys::parse_public_key_base64(KEY_A).unwrap();
    let handler = SshClientHandler::new("example.com".to_string(), 22, HostKeyPolicy::Strict)
        .with_known_hosts_path(path.clone());

    assert!(matches!(
        handler.verify_server_key(&key),
        Err(SshHandlerError::UnknownHostKey { .. })
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn approved_fingerprint_is_persisted_and_then_matches_strictly() {
    let path = temporary_known_hosts();
    let key = russh::keys::parse_public_key_base64(KEY_A).unwrap();
    let fingerprint = SshClientHandler::fingerprint(&key);
    let approving = SshClientHandler::new(
        "example.com".to_string(),
        22,
        HostKeyPolicy::AcceptNewFingerprint(fingerprint),
    )
    .with_known_hosts_path(path.clone());
    assert!(approving.verify_server_key(&key).unwrap());

    let strict = SshClientHandler::new("example.com".to_string(), 22, HostKeyPolicy::Strict)
        .with_known_hosts_path(path.clone());
    assert!(strict.verify_server_key(&key).unwrap());
    let _ = std::fs::remove_file(path);
}

#[test]
fn changed_key_is_never_approved() {
    let path = temporary_known_hosts();
    let first = russh::keys::parse_public_key_base64(KEY_A).unwrap();
    let changed = russh::keys::parse_public_key_base64(KEY_B).unwrap();
    russh::keys::known_hosts::learn_known_hosts_path("example.com", 22, &first, &path).unwrap();
    let handler = SshClientHandler::new(
        "example.com".to_string(),
        22,
        HostKeyPolicy::AcceptNewFingerprint(SshClientHandler::fingerprint(&changed)),
    )
    .with_known_hosts_path(path.clone());

    assert!(matches!(
        handler.verify_server_key(&changed),
        Err(SshHandlerError::ChangedHostKey { .. })
    ));
    let _ = std::fs::remove_file(path);
}
#[test]
fn unknown_key_with_wrong_approval_fingerprint_is_rejected() {
    let path = temporary_known_hosts();
    let key = russh::keys::parse_public_key_base64(KEY_A).unwrap();
    let handler = SshClientHandler::new(
        "example.com".to_string(),
        2222,
        HostKeyPolicy::AcceptNewFingerprint("SHA256:not-the-server-key".into()),
    )
    .with_known_hosts_path(path.clone());
    assert!(matches!(
        handler.verify_server_key(&key),
        Err(SshHandlerError::UnknownHostKey { port: 2222, .. })
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_known_hosts_does_not_trust_the_server_key() {
    let path = temporary_known_hosts();
    std::fs::write(&path, b"not a known_hosts line\n").unwrap();
    let key = russh::keys::parse_public_key_base64(KEY_A).unwrap();
    let handler = SshClientHandler::new("example.com".into(), 22, HostKeyPolicy::Strict)
        .with_known_hosts_path(path.clone());
    assert!(matches!(
        handler.verify_server_key(&key),
        Err(SshHandlerError::UnknownHostKey { .. })
    ));
    let _ = std::fs::remove_file(path);
}
#[derive(Clone)]
struct LoopbackServer;

impl russh::server::Server for LoopbackServer {
    type Handler = Self;

    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
        self.clone()
    }
}

impl russh::server::Handler for LoopbackServer {
    type Error = russh::Error;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_handshake_rejects_then_persists_an_approved_host_key() {
    use russh::server::Server as _;

    let private_key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    let fingerprint = private_key
        .public_key()
        .fingerprint(HashAlg::Sha256)
        .to_string();
    let server_config = Arc::new(russh::server::Config {
        keys: vec![private_key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let mut server = LoopbackServer;
        server.run_on_socket(server_config, &listener).await
    });
    let known_hosts = temporary_known_hosts();
    let client_config = Arc::new(russh::client::Config::default());

    let strict = SshClientHandler::new(
        address.ip().to_string(),
        address.port(),
        HostKeyPolicy::Strict,
    )
    .with_known_hosts_path(known_hosts.clone());
    let rejected = russh::client::connect(client_config.clone(), address, strict).await;
    assert!(matches!(
        rejected,
        Err(SshHandlerError::UnknownHostKey { .. })
    ));

    let approving = SshClientHandler::new(
        address.ip().to_string(),
        address.port(),
        HostKeyPolicy::AcceptNewFingerprint(fingerprint),
    )
    .with_known_hosts_path(known_hosts.clone());
    let approved = russh::client::connect(client_config.clone(), address, approving)
        .await
        .expect("approved loopback host key should connect");
    drop(approved);

    let strict = SshClientHandler::new(
        address.ip().to_string(),
        address.port(),
        HostKeyPolicy::Strict,
    )
    .with_known_hosts_path(known_hosts.clone());
    let trusted = russh::client::connect(client_config, address, strict)
        .await
        .expect("persisted loopback host key should connect strictly");
    drop(trusted);

    server_task.abort();
    let _ = server_task.await;
    let _ = std::fs::remove_file(known_hosts);
}
