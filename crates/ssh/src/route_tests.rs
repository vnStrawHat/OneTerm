//! Jump-host route against two in-process servers: one relays direct-tcpip
//! channels (the bastion), the other only accepts session channels (the target).

use std::net::SocketAddr;

use oneterm_core::{HostKeyPolicy, SecretString};
use russh::server::Auth;

use super::*;
use crate::test_support::{TempKnownHosts, loopback_fingerprint, spawn_server};

/// Accepts any password; relays direct-tcpip channels to their destination
/// when `relay` is set, refuses them otherwise; accepts session channels.
#[derive(Clone)]
struct RelayServer {
    relay: bool,
}

impl russh::server::Server for RelayServer {
    type Handler = Self;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        self.clone()
    }
}

impl russh::server::Handler for RelayServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<russh::server::Msg>,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        if !self.relay {
            return Ok(false);
        }
        let mut tcp =
            tokio::net::TcpStream::connect((host_to_connect, port_to_connect as u16)).await?;
        tokio::spawn(async move {
            let mut remote = channel.into_stream();
            if let Err(error) = tokio::io::copy_bidirectional(&mut remote, &mut tcp).await {
                eprintln!("test relay ended: {error}");
            }
        });
        Ok(true)
    }
}

fn password() -> SshAuthMethod {
    SshAuthMethod::Password {
        password: SecretString::new("secret"),
    }
}

fn known_hosts(tag: &str) -> TempKnownHosts {
    TempKnownHosts(std::env::temp_dir().join(format!(
        "oneterm-route-known-hosts-{tag}-{}",
        std::process::id()
    )))
}

fn handler(
    address: SocketAddr,
    policy: HostKeyPolicy,
    known_hosts: &TempKnownHosts,
) -> SshClientHandler {
    SshClientHandler::new(address.ip().to_string(), address.port(), policy)
        .with_known_hosts_path(known_hosts.0.clone())
}

/// TCP to the bastion, authenticated, with its key learned into `known_hosts`.
async fn bastion_handle(
    address: SocketAddr,
    known_hosts: &TempKnownHosts,
) -> Handle<SshClientHandler> {
    let fingerprint = loopback_fingerprint(address, known_hosts).await;
    let handler = handler(
        address,
        HostKeyPolicy::AcceptNewFingerprint(fingerprint),
        known_hosts,
    );
    let config = client_config(&handler, SshKeepaliveConfig::default());
    let mut handle = open_transport(
        None,
        &address.ip().to_string(),
        address.port(),
        handler,
        config,
    )
    .await
    .expect("bastion transport");
    authenticate(&mut handle, "ops", password())
        .await
        .expect("bastion auth");
    handle
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hop_carries_the_target_handshake_and_session_channel() {
    let (bastion, bastion_task) = spawn_server(RelayServer { relay: true }).await;
    let (target, target_task) = spawn_server(RelayServer { relay: false }).await;
    let known_hosts = known_hosts("hop");
    let bastion_handle = bastion_handle(bastion, &known_hosts).await;
    let target_fingerprint = loopback_fingerprint(target, &known_hosts).await;

    let handler = handler(
        target,
        HostKeyPolicy::AcceptNewFingerprint(target_fingerprint),
        &known_hosts,
    );
    let config = client_config(&handler, SshKeepaliveConfig::default());
    let mut target_handle = open_transport(
        Some(&bastion_handle),
        &target.ip().to_string(),
        target.port(),
        handler,
        config,
    )
    .await
    .expect("target transport through the bastion");
    authenticate(&mut target_handle, "deploy", password())
        .await
        .expect("target auth through the bastion");
    let channel = target_handle
        .channel_open_session()
        .await
        .expect("session channel on the target");

    drop(channel);
    drop(target_handle);
    drop(bastion_handle);
    bastion_task.abort();
    target_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bastion_that_refuses_forwarding_names_the_next_hop() {
    let (bastion, bastion_task) = spawn_server(RelayServer { relay: false }).await;
    let known_hosts = known_hosts("refuse");
    let bastion_handle = bastion_handle(bastion, &known_hosts).await;

    let handler = handler(bastion, HostKeyPolicy::Strict, &known_hosts);
    let config = client_config(&handler, SshKeepaliveConfig::default());
    let error = open_transport(Some(&bastion_handle), "127.0.0.1", 1, handler, config)
        .await
        .map(drop)
        .expect_err("a refused direct-tcpip channel must fail");

    let text = error.to_string();
    assert!(
        text.contains("direct-tcpip channel to 127.0.0.1:1"),
        "{text}"
    );
    assert!(
        matches!(
            error,
            AppError::Connect {
                phase: ConnectPhase::Transport,
                ..
            }
        ),
        "{error:?}"
    );
    drop(bastion_handle);
    bastion_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_key_behind_the_bastion_names_the_target() {
    let (bastion, bastion_task) = spawn_server(RelayServer { relay: true }).await;
    let (target, target_task) = spawn_server(RelayServer { relay: false }).await;
    let known_hosts = known_hosts("unknown");
    let bastion_handle = bastion_handle(bastion, &known_hosts).await;

    let handler = handler(target, HostKeyPolicy::Strict, &known_hosts);
    let config = client_config(&handler, SshKeepaliveConfig::default());
    let error = open_transport(
        Some(&bastion_handle),
        &target.ip().to_string(),
        target.port(),
        handler,
        config,
    )
    .await
    .map(drop)
    .expect_err("the target key is not in known_hosts");

    match error {
        AppError::HostKeyUnknown { host, port, .. } => {
            assert_eq!(host, target.ip().to_string());
            assert_eq!(port, target.port());
        }
        other => panic!("expected an unknown host key, got {other:?}"),
    }
    drop(bastion_handle);
    bastion_task.abort();
    target_task.abort();
}

#[test]
fn hop_error_prefixes_connect_errors_only() {
    let connect = hop_error(
        "ops@bastion:22",
        AppError::Connect {
            phase: ConnectPhase::Authentication,
            message: "rejected by the server".into(),
        },
    );
    assert_eq!(
        connect.to_string(),
        "SSH authentication failed: jump host ops@bastion:22: rejected by the server"
    );

    let host_key = hop_error(
        "ops@bastion:22",
        AppError::HostKeyUnknown {
            host: "bastion".into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:x".into(),
        },
    );
    assert!(matches!(host_key, AppError::HostKeyUnknown { .. }));
    assert!(matches!(
        hop_error("ops@bastion:22", AppError::Cancelled),
        AppError::Cancelled
    ));
}
