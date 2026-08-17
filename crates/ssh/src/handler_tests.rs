//! Host-key verification tests for the SSH client handler.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";
// A throwaway 2048-bit RSA public key: RSA generation is too slow for a unit test.
const RSA_KEY: &str = "AAAAB3NzaC1yc2EAAAADAQABAAABAQC4Kg9GOOXC0ZZmtj1/X6A/qRPw+6v8c57k24G+gNZhrQKpnPlmQUvTsih6JBmtEGOFgsIiDctBZEHKEnTDMJUIg6TTP44oN8FGuEsGLDSmmCYV+h7MUtPXbSm574RJAVgBJyQkn9KvGdJykNpKxJcJqghMZl7yvorG/klLLl6OXbWH2qe8nr3fz645YmTVds1eRyToWYFJm4c0m865kbEvHiVM5pb7eeYXcF3UkDA7Y8QVEaKyP+uuNy1qgcyf1ega3XpaOJkOQDJ625ng3Sy4Qdyq39cVM8H7nMtR8GehSEuYcC7sZQrdNqlcawWaKD7OAw272gHYAabzfowNHUAd";

fn temporary_known_hosts() -> PathBuf {
    // Distinct per call so tests running in parallel never share a known_hosts
    // file, even when the wall clock is too coarse to separate two calls (as on
    // macOS). The atomic counter guarantees uniqueness within the process; the
    // pid and timestamp keep names unique across processes and test runs.
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "oneterm-known-hosts-{}-{nonce}-{sequence}",
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

fn random_public_key(algorithm: russh::keys::Algorithm) -> PublicKey {
    russh::keys::PrivateKey::random(&mut rand::rng(), algorithm)
        .unwrap()
        .public_key()
        .clone()
}

fn ecdsa_p256() -> russh::keys::Algorithm {
    russh::keys::Algorithm::Ecdsa {
        curve: russh::keys::EcdsaCurve::NistP256,
    }
}

#[test]
fn key_of_a_different_algorithm_for_a_known_host_is_a_mismatch_not_unknown() {
    let path = temporary_known_hosts();
    let recorded = russh::keys::parse_public_key_base64(KEY_A).unwrap();
    russh::keys::known_hosts::learn_known_hosts_path("example.com", 22, &recorded, &path).unwrap();
    let presented = random_public_key(ecdsa_p256());
    // Even a first-use approval for the presented fingerprint must not
    // silently add a second key type to a host that is already known.
    let handler = SshClientHandler::new(
        "example.com".to_string(),
        22,
        HostKeyPolicy::AcceptNewFingerprint(SshClientHandler::fingerprint(&presented)),
    )
    .with_known_hosts_path(path.clone());

    let error = handler.verify_server_key(&presented).unwrap_err();
    match &error {
        SshHandlerError::HostKeyAlgorithmMismatch {
            algorithm,
            known_algorithms,
            ..
        } => {
            assert_eq!(algorithm, "ecdsa-sha2-nistp256");
            assert_eq!(known_algorithms, &["ssh-ed25519".to_string()]);
        }
        other => panic!("expected an algorithm mismatch, got {other:?}"),
    }
    assert!(matches!(
        error.to_app_error(),
        AppError::HostKeyChanged { port: 22, .. }
    ));
    let strict = SshClientHandler::new("example.com".to_string(), 22, HostKeyPolicy::Strict)
        .with_known_hosts_path(path.clone());
    assert!(matches!(
        strict.verify_server_key(&presented),
        Err(SshHandlerError::HostKeyAlgorithmMismatch { .. })
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_matching_entry_among_several_key_types_is_trusted() {
    let path = temporary_known_hosts();
    let ed25519 = russh::keys::parse_public_key_base64(KEY_A).unwrap();
    let ecdsa = random_public_key(ecdsa_p256());
    russh::keys::known_hosts::learn_known_hosts_path("example.com", 22, &ed25519, &path).unwrap();
    russh::keys::known_hosts::learn_known_hosts_path("example.com", 22, &ecdsa, &path).unwrap();
    let handler = SshClientHandler::new("example.com".to_string(), 22, HostKeyPolicy::Strict)
        .with_known_hosts_path(path.clone());

    assert!(handler.verify_server_key(&ed25519).unwrap());
    assert!(handler.verify_server_key(&ecdsa).unwrap());
    let _ = std::fs::remove_file(path);
}

#[test]
fn preferred_key_algorithms_put_recorded_types_first() {
    use russh::keys::Algorithm;

    let defaults = russh::Preferred::DEFAULT.key.to_vec();
    assert_eq!(preferred_key_algorithms(&[]), defaults);

    let ecdsa = random_public_key(ecdsa_p256());
    let preferred = preferred_key_algorithms(std::slice::from_ref(&ecdsa));
    assert_eq!(preferred[0], ecdsa_p256());
    assert_eq!(preferred.len(), defaults.len());

    let rsa = russh::keys::parse_public_key_base64(RSA_KEY).unwrap();
    let preferred = preferred_key_algorithms(std::slice::from_ref(&rsa));
    assert!(
        preferred[..3]
            .iter()
            .all(|algorithm| matches!(algorithm, Algorithm::Rsa { .. }))
    );
    assert_eq!(preferred[3], Algorithm::Ed25519);
}

#[test]
fn handler_prefers_the_recorded_host_key_type() {
    let path = temporary_known_hosts();
    let ecdsa = random_public_key(ecdsa_p256());
    russh::keys::known_hosts::learn_known_hosts_path("example.com", 22, &ecdsa, &path).unwrap();
    let handler = SshClientHandler::new("example.com".to_string(), 22, HostKeyPolicy::Strict)
        .with_known_hosts_path(path.clone());
    assert_eq!(handler.preferred_key_algorithms()[0], ecdsa_p256());
    let _ = std::fs::remove_file(path);
}
