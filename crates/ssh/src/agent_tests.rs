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
