//! Local, dynamic (SOCKS5), and remote forwards against an in-process server
//! that echoes direct-tcpip channels and can open forwarded-tcpip channels.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use oneterm_core::{SecretString, SshAuthMethod};
use russh::ChannelMsg;
use russh::server::Auth;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::route::authenticate;
use crate::test_support::{TempKnownHosts, loopback_fingerprint, spawn_server};

/// Echoes every direct-tcpip channel; accepts every remote forward request and
/// hands the test a server handle to open forwarded channels with.
#[derive(Clone)]
struct EchoServer {
    server_handle: Arc<Mutex<Option<russh::server::Handle>>>,
    forwards_requested: Arc<Mutex<Vec<(String, u32)>>>,
}

impl EchoServer {
    fn new() -> Self {
        Self {
            server_handle: Arc::default(),
            forwards_requested: Arc::default(),
        }
    }
}

impl russh::server::Server for EchoServer {
    type Handler = Self;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        self.clone()
    }
}

impl russh::server::Handler for EchoServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        _host_to_connect: &str,
        _port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        tokio::spawn(async move {
            let stream = channel.into_stream();
            let (mut reader, mut writer) = tokio::io::split(stream);
            let _ = tokio::io::copy(&mut reader, &mut writer).await;
        });
        Ok(true)
    }

    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        if *port == 0 {
            *port = 40_000;
        }
        self.forwards_requested
            .lock()
            .expect("requests")
            .push((address.to_string(), *port));
        *self.server_handle.lock().expect("server handle") = Some(session.handle());
        Ok(true)
    }
}

struct Fixture {
    /// Shared with the owner task, which serves handle requests the way the
    /// select arm of `ssh_main_task` does.
    handle: Arc<Handle<SshClientHandler>>,
    _known_hosts: TempKnownHosts,
    server_task: tokio::task::JoinHandle<()>,
    owner_task: tokio::task::JoinHandle<()>,
    table: ForwardTable,
    ctx: ForwardContext,
    events: async_channel::Receiver<SessionEvent>,
    server: EchoServer,
}

impl Fixture {
    async fn start(specs: &[PortForward]) -> (Self, Vec<SocketAddr>) {
        let server = EchoServer::new();
        let (address, server_task) = spawn_server(server.clone()).await;
        let known_hosts = TempKnownHosts(std::env::temp_dir().join(format!(
            "oneterm-tunnel-known-hosts-{}-{}",
            std::process::id(),
            address.port()
        )));
        let fingerprint = loopback_fingerprint(address, &known_hosts).await;
        let handler = SshClientHandler::new(
            address.ip().to_string(),
            address.port(),
            oneterm_core::HostKeyPolicy::AcceptNewFingerprint(fingerprint),
        )
        .with_known_hosts_path(known_hosts.0.clone());
        let table: ForwardTable = Arc::default();
        let shutdown = CancellationToken::new();
        let handler = handler.with_forwards(table.clone(), shutdown.clone());
        let mut handle =
            russh::client::connect(Arc::new(russh::client::Config::default()), address, handler)
                .await
                .expect("connect");
        authenticate(
            &mut handle,
            "user",
            SshAuthMethod::Password {
                password: SecretString::new("secret"),
            },
        )
        .await
        .expect("auth");

        // The handle stays with the test (the real owner is ssh_main_task);
        // a small task serves the requests the way its select arm does.
        let (open_tx, mut open_rx) = mpsc::channel(HANDLE_REQUEST_CAPACITY);
        let (event_tx, events) = async_channel::bounded(16);
        let ctx = ForwardContext {
            open_tx,
            shutdown: shutdown.clone(),
            notify: event_tx,
        };
        let bound = start_forwards(specs, &handle, &table, &ctx).await;
        let handle = Arc::new(handle);
        let owner_task = tokio::spawn({
            let handle = handle.clone();
            async move {
                while let Some(request) = open_rx.recv().await {
                    serve_handle_request(&handle, request).await;
                }
            }
        });
        let fixture = Self {
            handle,
            _known_hosts: known_hosts,
            server_task,
            owner_task,
            table,
            ctx,
            events,
            server,
        };
        (fixture, bound)
    }

    async fn finish(self) {
        self.ctx.shutdown.cancel();
        self.owner_task.abort();
        drop(self.handle);
        self.server_task.abort();
    }
}

async fn echo_round_trip(tcp: &mut tokio::net::TcpStream, payload: &[u8]) {
    tcp.write_all(payload).await.expect("write");
    let mut back = vec![0u8; payload.len()];
    tokio::time::timeout(Duration::from_secs(5), tcp.read_exact(&mut back))
        .await
        .expect("echo within 5 s")
        .expect("read");
    assert_eq!(back, payload);
}

fn local_spec(bind_port: u16) -> PortForward {
    PortForward::Local {
        bind: oneterm_core::loopback(),
        bind_port,
        target_host: "echo.invalid".into(),
        target_port: 80,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_forward_relays_bytes_through_the_session() {
    let (fixture, bound) = Fixture::start(&[local_spec(0)]).await;
    assert_eq!(bound.len(), 1);

    let mut tcp = tokio::net::TcpStream::connect(bound[0])
        .await
        .expect("connect to forward");
    echo_round_trip(&mut tcp, b"ping through the tunnel").await;

    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dynamic_forward_speaks_socks5_connect_only() {
    let spec = PortForward::Dynamic {
        bind: oneterm_core::loopback(),
        bind_port: 0,
    };
    let (fixture, bound) = Fixture::start(&[spec]).await;
    let proxy = bound[0];

    // CONNECT to an IPv4 target: greeting, request, reply, then echo.
    let mut tcp = tokio::net::TcpStream::connect(proxy)
        .await
        .expect("connect");
    tcp.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut reply = [0u8; 2];
    tcp.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [0x05, 0x00]);
    tcp.write_all(&[0x05, 0x01, 0x00, 0x01, 10, 0, 0, 1, 0x00, 0x50])
        .await
        .unwrap();
    let mut connect_reply = [0u8; 10];
    tcp.read_exact(&mut connect_reply).await.unwrap();
    assert_eq!(connect_reply[0..2], [0x05, 0x00], "{connect_reply:?}");
    echo_round_trip(&mut tcp, b"socks payload").await;

    // BIND is refused with "command not supported".
    let mut bind = tokio::net::TcpStream::connect(proxy)
        .await
        .expect("connect");
    bind.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    bind.read_exact(&mut reply).await.unwrap();
    bind.write_all(&[0x05, 0x02, 0x00, 0x01, 10, 0, 0, 1, 0x00, 0x50])
        .await
        .unwrap();
    bind.read_exact(&mut connect_reply).await.unwrap();
    assert_eq!(connect_reply[1], 0x07);

    // A client without the no-auth method is refused outright.
    let mut auth_only = tokio::net::TcpStream::connect(proxy)
        .await
        .expect("connect");
    auth_only.write_all(&[0x05, 0x01, 0x02]).await.unwrap();
    auth_only.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [0x05, 0xFF]);

    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_forward_reaches_the_local_target_and_unknown_ports_are_dropped() {
    let target = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let target_port = target.local_addr().unwrap().port();
    let spec = PortForward::Remote {
        bind_host: "127.0.0.1".into(),
        bind_port: 9000,
        target_host: "127.0.0.1".into(),
        target_port,
    };
    let (fixture, bound) = Fixture::start(&[spec]).await;
    assert!(bound.is_empty(), "remote forwards bind nothing locally");
    assert_eq!(
        fixture.server.forwards_requested.lock().unwrap().as_slice(),
        &[("127.0.0.1".to_string(), 9000)]
    );
    assert_eq!(
        forwarded_target(&fixture.table, "127.0.0.1", 9000),
        Some(("127.0.0.1".to_string(), target_port))
    );
    let server_handle = fixture
        .server
        .server_handle
        .lock()
        .unwrap()
        .clone()
        .expect("server handle");

    // The server opens a forwarded channel: bytes reach the local target.
    let mut channel = server_handle
        .channel_open_forwarded_tcpip("127.0.0.1", 9000, "10.0.0.9", 40001)
        .await
        .expect("forwarded channel");
    let (mut accepted, _) = tokio::time::timeout(Duration::from_secs(5), target.accept())
        .await
        .expect("target accepts within 5 s")
        .expect("accept");
    channel.data(&b"from the server"[..]).await.expect("data");
    let mut received = [0u8; 15];
    accepted.read_exact(&mut received).await.expect("read");
    assert_eq!(&received, b"from the server");
    accepted.write_all(b"reply").await.expect("write");
    let mut got_reply = false;
    while let Some(message) = tokio::time::timeout(Duration::from_secs(5), channel.wait())
        .await
        .expect("channel message")
    {
        if let ChannelMsg::Data { data } = message {
            assert_eq!(data.as_ref(), b"reply");
            got_reply = true;
            break;
        }
    }
    assert!(got_reply);

    // A port OneTerm never asked for is dropped: the target sees nothing.
    let stray = server_handle
        .channel_open_forwarded_tcpip("127.0.0.1", 9001, "10.0.0.9", 40002)
        .await
        .expect("stray channel");
    let nothing = tokio::time::timeout(Duration::from_millis(500), target.accept()).await;
    assert!(
        nothing.is_err(),
        "no local connection for an unknown forward"
    );
    drop(stray);

    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bind_failure_warns_and_the_rest_still_start() {
    let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let taken = occupied.local_addr().unwrap().port();
    let (fixture, bound) = Fixture::start(&[local_spec(taken), local_spec(0)]).await;

    assert_eq!(bound.len(), 1, "only the free port is bound");
    let event = fixture.events.recv().await.expect("warning event");
    match event {
        SessionEvent::Notification(text) => {
            assert!(text.contains(&format!("127.0.0.1:{taken}")), "{text}");
            assert!(text.contains("not started"), "{text}");
        }
        other => panic!("expected a notification, got {other:?}"),
    }

    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_the_session_frees_the_bound_port() {
    let (fixture, bound) = Fixture::start(&[local_spec(0)]).await;
    let address = bound[0];
    assert!(tokio::net::TcpListener::bind(address).await.is_err());

    fixture.finish().await;

    let mut freed = false;
    for _ in 0..50 {
        if tokio::net::TcpListener::bind(address).await.is_ok() {
            freed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        freed,
        "the listener must release {address} after cancellation"
    );
}
