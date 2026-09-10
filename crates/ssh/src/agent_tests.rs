//! Agent identity loop against an in-process SSH agent and SSH server.

use std::sync::{Arc, Mutex};

use russh::keys::agent::client::AgentClient;
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::Auth;
use russh::{MethodKind, MethodSet};

use super::*;
use crate::test_support::{connect_trusting_loopback, spawn_server};

/// Accepts exactly `accepted` (by key data) and records every key offered.
#[derive(Clone)]
struct PublicKeyServer {
    accepted: Option<PublicKey>,
    offered: Arc<Mutex<Vec<PublicKey>>>,
    /// After a rejection, tell the client only `password` remains.
    drop_publickey: bool,
}

impl PublicKeyServer {
    fn decide(&self, key: &PublicKey) -> Auth {
        if self.accepted.as_ref().map(PublicKey::key_data) == Some(key.key_data()) {
            Auth::Accept
        } else if self.drop_publickey {
            Auth::Reject {
                proceed_with_methods: Some(MethodSet::from(&[MethodKind::Password][..])),
                partial_success: false,
            }
        } else {
            Auth::reject()
        }
    }
}

impl russh::server::Server for PublicKeyServer {
    type Handler = Self;

    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
        self.clone()
    }
}

impl russh::server::Handler for PublicKeyServer {
    type Error = russh::Error;

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.offered
            .lock()
            .expect("offered keys")
            .push(public_key.clone());
        Ok(self.decide(public_key))
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(self.decide(public_key))
    }
}

fn ed25519_keys(count: usize) -> Vec<PrivateKey> {
    (0..count)
        .map(|_| PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("key generation"))
        .collect()
}

/// An in-process agent (russh's own agent server over an in-memory duplex
/// stream) holding `keys`, in that order.
async fn spawn_agent(keys: &[PrivateKey]) -> AgentClient<tokio::io::DuplexStream> {
    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let connections = futures::stream::iter(vec![Ok::<_, std::io::Error>(server_side)]);
    tokio::spawn(async move {
        if let Err(error) = russh::keys::agent::server::serve(connections, ()).await {
            eprintln!("test SSH agent stopped: {error}");
        }
    });
    let mut agent = AgentClient::connect(client_side);
    for key in keys {
        agent.add_identity(key, &[]).await.expect("add identity");
    }
    agent
}

async fn run(
    keys: &[PrivateKey],
    server: PublicKeyServer,
) -> (oneterm_core::Result<AuthResult>, Vec<PublicKey>) {
    let offered = server.offered.clone();
    let (address, server_task) = spawn_server(server).await;
    let (mut handle, _known_hosts) = connect_trusting_loopback(address).await;
    let mut agent = spawn_agent(keys).await;

    let result = authenticate_with_agent_client(&mut handle, "user", &mut agent).await;

    drop(handle);
    server_task.abort();
    let offered = offered.lock().expect("offered keys").clone();
    (result, offered)
}

fn key_data(keys: &[PublicKey]) -> Vec<russh::keys::ssh_key::public::KeyData> {
    keys.iter().map(|key| key.key_data().clone()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn second_agent_identity_is_accepted_after_the_first_is_rejected() {
    let keys = ed25519_keys(2);
    let server = PublicKeyServer {
        accepted: Some(keys[1].public_key().clone()),
        offered: Arc::default(),
        drop_publickey: false,
    };

    let (result, offered) = run(&keys, server).await;

    assert!(matches!(result, Ok(AuthResult::Success)), "{result:?}");
    // The agent lists identities in its own order, so the accepted key is
    // offered first or second; the loop stops on it either way.
    let agent_keys = key_data(&[keys[0].public_key().clone(), keys[1].public_key().clone()]);
    let offered = key_data(&offered);
    assert!(!offered.is_empty() && offered.len() <= 2, "{offered:?}");
    assert!(offered.iter().all(|key| agent_keys.contains(key)));
    assert_eq!(offered.last(), Some(&agent_keys[1]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_agent_is_an_explicit_error() {
    let server = PublicKeyServer {
        accepted: None,
        offered: Arc::default(),
        drop_publickey: false,
    };

    let (result, offered) = run(&[], server).await;

    let error = result.expect_err("no identities must fail").to_string();
    assert!(error.contains("holds no identities"), "{error}");
    assert!(offered.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_loop_stops_when_the_server_stops_accepting_publickey() {
    let keys = ed25519_keys(3);
    let server = PublicKeyServer {
        accepted: None,
        offered: Arc::default(),
        drop_publickey: true,
    };

    let (result, offered) = run(&keys, server).await;

    let error = result
        .expect_err("rejected identities must fail")
        .to_string();
    assert_eq!(offered.len(), 1, "{error}");
    assert!(error.contains("the server accepts: password"), "{error}");
    assert!(
        error.contains("none of the 1 agent identity offered was accepted"),
        "{error}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn at_most_six_identities_are_offered() {
    let keys = ed25519_keys(MAX_AGENT_IDENTITIES + 1);
    let server = PublicKeyServer {
        accepted: None,
        offered: Arc::default(),
        drop_publickey: false,
    };

    let (result, offered) = run(&keys, server).await;

    let error = result
        .expect_err("rejected identities must fail")
        .to_string();
    assert_eq!(offered.len(), MAX_AGENT_IDENTITIES, "{error}");
    assert!(
        error.contains("none of the 6 agent identities offered was accepted"),
        "{error}"
    );
}

// ── Agent forwarding (US-0060) ──────────────────────────────────────────

use std::sync::atomic::{AtomicUsize, Ordering};

use russh::ChannelMsg;
use tokio_util::sync::CancellationToken;

/// Accepts any password and session channel; answers the agent-forwarding
/// request with `accept` and keeps a server handle so the test can open agent
/// channels afterwards.
#[derive(Clone)]
struct AgentForwardServer {
    accept: bool,
    handle: Arc<Mutex<Option<russh::server::Handle>>>,
}

impl russh::server::Server for AgentForwardServer {
    type Handler = Self;

    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
        self.clone()
    }
}

impl russh::server::Handler for AgentForwardServer {
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

    async fn agent_request(
        &mut self,
        channel: russh::ChannelId,
        session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        *self.handle.lock().expect("server handle") = Some(session.handle());
        if self.accept {
            session.channel_success(channel)?;
        } else {
            session.channel_failure(channel)?;
        }
        Ok(self.accept)
    }
}

/// An in-memory "agent" that echoes; counts how often it was connected to.
fn echo_agent_connector(connections: Arc<AtomicUsize>) -> AgentConnector {
    Arc::new(move || {
        let connections = connections.clone();
        Box::pin(async move {
            connections.fetch_add(1, Ordering::SeqCst);
            let (ours, theirs) = tokio::io::duplex(4096);
            tokio::spawn(async move {
                let (mut reader, mut writer) = tokio::io::split(theirs);
                let _ = tokio::io::copy(&mut reader, &mut writer).await;
            });
            Ok(Box::new(ours) as BoxedAgentStream)
        })
    })
}

async fn agent_forward_fixture(
    accept: bool,
    connector: Option<AgentConnector>,
) -> (
    Handle<SshClientHandler>,
    Channel<Msg>,
    AgentForwardServer,
    tokio::task::JoinHandle<()>,
    crate::test_support::TempKnownHosts,
) {
    use crate::route::authenticate;
    use crate::test_support::{TempKnownHosts, loopback_fingerprint};

    let server = AgentForwardServer {
        accept,
        handle: Arc::default(),
    };
    let (address, server_task) = spawn_server(server.clone()).await;
    let known_hosts = TempKnownHosts(std::env::temp_dir().join(format!(
        "oneterm-agentfwd-known-hosts-{}-{}",
        std::process::id(),
        address.port()
    )));
    let fingerprint = loopback_fingerprint(address, &known_hosts).await;
    let mut handler = SshClientHandler::new(
        address.ip().to_string(),
        address.port(),
        oneterm_core::HostKeyPolicy::AcceptNewFingerprint(fingerprint),
    )
    .with_known_hosts_path(known_hosts.0.clone());
    if let Some(connector) = connector {
        handler = handler.with_agent_forwarding(connector, CancellationToken::new());
    }
    let mut handle =
        russh::client::connect(Arc::new(russh::client::Config::default()), address, handler)
            .await
            .expect("connect");
    authenticate(
        &mut handle,
        "user",
        oneterm_core::SshAuthMethod::Password {
            password: oneterm_core::SecretString::new("secret"),
        },
    )
    .await
    .expect("auth");
    let channel = handle
        .channel_open_session()
        .await
        .expect("session channel");
    (handle, channel, server, server_task, known_hosts)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_agent_forwarding_request_is_reported_not_fatal() {
    let (handle, mut channel, _server, server_task, _known_hosts) =
        agent_forward_fixture(false, None).await;

    let accepted = request_agent_forwarding(&mut channel)
        .await
        .expect("a refusal is a result");
    assert!(!accepted);

    drop(channel);
    drop(handle);
    server_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_accepted_request_bridges_agent_channels_to_the_local_agent() {
    let connections = Arc::new(AtomicUsize::new(0));
    let (handle, mut channel, server, server_task, _known_hosts) =
        agent_forward_fixture(true, Some(echo_agent_connector(connections.clone()))).await;

    assert!(
        request_agent_forwarding(&mut channel)
            .await
            .expect("request")
    );
    let server_handle = server
        .handle
        .lock()
        .unwrap()
        .clone()
        .expect("server handle");
    let mut agent_channel = server_handle
        .channel_open_agent()
        .await
        .expect("agent channel");
    agent_channel
        .data(&b"sign this"[..])
        .await
        .expect("data to the agent");
    let mut echoed = false;
    while let Some(message) =
        tokio::time::timeout(std::time::Duration::from_secs(5), agent_channel.wait())
            .await
            .expect("agent channel message")
    {
        if let ChannelMsg::Data { data } = message {
            assert_eq!(data.as_ref(), b"sign this");
            echoed = true;
            break;
        }
    }
    assert!(
        echoed,
        "bytes must round-trip through the local agent bridge"
    );
    assert_eq!(connections.load(Ordering::SeqCst), 1);

    drop(agent_channel);
    drop(channel);
    drop(handle);
    server_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_switch_a_server_opened_agent_channel_is_closed_unanswered() {
    let connections = Arc::new(AtomicUsize::new(0));
    // The server accepts the request, but this client never asked: no bridge.
    let (handle, mut channel, server, server_task, _known_hosts) =
        agent_forward_fixture(true, None).await;
    assert!(
        request_agent_forwarding(&mut channel)
            .await
            .expect("request")
    );
    let server_handle = server
        .handle
        .lock()
        .unwrap()
        .clone()
        .expect("server handle");

    let mut agent_channel = server_handle
        .channel_open_agent()
        .await
        .expect("the open itself succeeds");
    let mut closed = false;
    while let Some(message) =
        tokio::time::timeout(std::time::Duration::from_secs(5), agent_channel.wait())
            .await
            .expect("agent channel message")
    {
        match message {
            ChannelMsg::Eof | ChannelMsg::Close => {
                closed = true;
                break;
            }
            ChannelMsg::Data { .. } => panic!("no data may flow without the switch"),
            _ => {}
        }
    }
    assert!(
        closed,
        "the client must close the unrequested agent channel"
    );
    assert_eq!(connections.load(Ordering::SeqCst), 0);

    drop(channel);
    drop(handle);
    server_task.abort();
}
