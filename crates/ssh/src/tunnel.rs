//! Port forwarding (IN-0023 / US-0059): local and dynamic (SOCKS5) listeners,
//! remote forwards, and the relays between them and the SSH connection.
//!
//! `russh::client::Handle` is owned by `ssh_main_task`, so every direct-tcpip
//! open goes through a [`HandleRequest`] served by that task
//! ([`serve_handle_request`]). Every listener and relay shares the session's
//! `CancellationToken` and dies with it (DEC-0011).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use russh::Channel;
use russh::client::{Handle, Msg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use oneterm_core::PortForward;
use oneterm_terminal::SessionEvent;

use crate::handler::SshClientHandler;

/// Work that needs the connection handle, sent to the task that owns it.
pub(crate) enum HandleRequest {
    /// Open a direct-tcpip channel to `host:port` for a local connection from
    /// `originator`.
    OpenDirectTcpip {
        host: String,
        port: u32,
        originator: SocketAddr,
        reply: oneshot::Sender<Result<Channel<Msg>, russh::Error>>,
    },
}

/// Pending direct-tcpip opens; back-pressures the accept loops when full.
pub(crate) const HANDLE_REQUEST_CAPACITY: usize = 32;

/// Remote forwards the server accepted: `(bind_host, bind_port)` as the server
/// names them in `forwarded-tcpip` opens, to the local target.
pub(crate) type ForwardTable = Arc<Mutex<HashMap<(String, u32), (String, u16)>>>;

/// What every listener and relay needs from the session.
#[derive(Clone)]
pub(crate) struct ForwardContext {
    pub(crate) open_tx: mpsc::Sender<HandleRequest>,
    pub(crate) shutdown: CancellationToken,
    /// Warnings reach the terminal as toasts through the session event channel.
    pub(crate) notify: async_channel::Sender<SessionEvent>,
}

impl ForwardContext {
    fn warn(&self, message: String) {
        log::warn!("SshSession: {message}");
        if let Err(error) = self.notify.try_send(SessionEvent::Notification(message)) {
            log::debug!("SshSession: forward warning not delivered: {error}");
        }
    }
}

/// Serve one request on the connection handle (called from `ssh_main_task`).
pub(crate) async fn serve_handle_request(
    handle: &Handle<SshClientHandler>,
    request: HandleRequest,
) {
    match request {
        HandleRequest::OpenDirectTcpip {
            host,
            port,
            originator,
            reply,
        } => {
            let result = handle
                .channel_open_direct_tcpip(
                    host,
                    port,
                    originator.ip().to_string(),
                    u32::from(originator.port()),
                )
                .await;
            if reply.send(result).is_err() {
                log::debug!("ssh_main_task: direct-tcpip requester went away");
            }
        }
    }
}

/// Start every forward of `specs`; a forward that cannot start produces one
/// warning and is skipped (DEC-0011). Returns the local addresses the `Local`
/// and `Dynamic` listeners bound, in spec order.
pub(crate) async fn start_forwards(
    specs: &[PortForward],
    handle: &Handle<SshClientHandler>,
    table: &ForwardTable,
    ctx: &ForwardContext,
) -> Vec<SocketAddr> {
    let mut bound = Vec::new();
    for spec in specs {
        match spec {
            PortForward::Local {
                bind,
                bind_port,
                target_host,
                target_port,
            } => {
                let relay = RelayKind::Fixed {
                    host: target_host.clone(),
                    port: *target_port,
                };
                if let Some(address) = listen(spec, (*bind, *bind_port), relay, ctx).await {
                    bound.push(address);
                }
            }
            PortForward::Dynamic { bind, bind_port } => {
                if let Some(address) = listen(spec, (*bind, *bind_port), RelayKind::Socks5, ctx).await {
                    bound.push(address);
                }
            }
            PortForward::Remote {
                bind_host,
                bind_port,
                target_host,
                target_port,
            } => match handle
                .tcpip_forward(bind_host.clone(), u32::from(*bind_port))
                .await
            {
                Ok(chosen) => {
                    let port = if *bind_port == 0 {
                        chosen
                    } else {
                        u32::from(*bind_port)
                    };
                    log::info!(
                        "SshSession: remote forward {} listening on {bind_host}:{port}",
                        spec.summary()
                    );
                    table
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert((bind_host.clone(), port), (target_host.clone(), *target_port));
                }
                Err(error) => ctx.warn(format!(
                    "Remote forward {bind_host}:{bind_port} refused by the server: {error}. The session stays connected."
                )),
            },
        }
    }
    bound
}

#[derive(Clone)]
enum RelayKind {
    /// A `Local` forward: every connection goes to this remote target.
    Fixed { host: String, port: u16 },
    /// A `Dynamic` forward: the target comes from the SOCKS5 request.
    Socks5,
}

async fn listen(
    spec: &PortForward,
    bind: (std::net::IpAddr, u16),
    relay: RelayKind,
    ctx: &ForwardContext,
) -> Option<SocketAddr> {
    let listener = match TcpListener::bind(bind).await {
        Ok(listener) => listener,
        Err(error) => {
            ctx.warn(format!(
                "Port forward {} not started: {error}. The session stays connected.",
                spec.summary()
            ));
            return None;
        }
    };
    let address = listener.local_addr().ok()?;
    log::info!(
        "SshSession: forward {} listening on {address}",
        spec.summary()
    );
    tokio::spawn(accept_loop(listener, relay, ctx.clone()));
    Some(address)
}

async fn accept_loop(listener: TcpListener, relay: RelayKind, ctx: ForwardContext) {
    loop {
        tokio::select! {
            _ = ctx.shutdown.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((tcp, peer)) => {
                    tokio::spawn(relay_connection(tcp, peer, relay.clone(), ctx.clone()));
                }
                Err(error) => {
                    log::debug!("SshSession: forward accept failed: {error}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
        }
    }
    log::info!("SshSession: forward listener closed");
}

async fn relay_connection(
    mut tcp: TcpStream,
    peer: SocketAddr,
    relay: RelayKind,
    ctx: ForwardContext,
) {
    let socks = matches!(relay, RelayKind::Socks5);
    let (host, port) = match relay {
        RelayKind::Fixed { host, port } => (host, port),
        RelayKind::Socks5 => match socks5_connect_target(&mut tcp).await {
            Ok(target) => target,
            Err(error) => {
                log::debug!("SshSession: SOCKS5 handshake from {peer} rejected: {error}");
                let _ = tcp.shutdown().await;
                return;
            }
        },
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let request = HandleRequest::OpenDirectTcpip {
        host: host.clone(),
        port: u32::from(port),
        originator: peer,
        reply: reply_tx,
    };
    let opened = match ctx.open_tx.send(request).await {
        Ok(()) => reply_rx.await.ok().and_then(Result::ok),
        Err(_) => None,
    };
    let Some(channel) = opened else {
        log::debug!("SshSession: direct-tcpip to {host}:{port} for {peer} failed");
        if socks {
            // General SOCKS server failure; the client sees a clean refusal.
            let _ = tcp.write_all(&socks5_reply(0x01)).await;
        }
        let _ = tcp.shutdown().await;
        return;
    };
    if socks && tcp.write_all(&socks5_reply(0x00)).await.is_err() {
        return;
    }
    let mut remote = channel.into_stream();
    tokio::select! {
        _ = ctx.shutdown.cancelled() => {}
        result = tokio::io::copy_bidirectional(&mut tcp, &mut remote) => {
            if let Err(error) = result {
                log::debug!("SshSession: relay {peer} -> {host}:{port} ended: {error}");
            }
        }
    }
}

/// Bridge a `forwarded-tcpip` channel the server opened to the local target.
pub(crate) fn spawn_forwarded_tcpip(
    channel: Channel<Msg>,
    target: (String, u16),
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let mut tcp = match TcpStream::connect((target.0.as_str(), target.1)).await {
            Ok(tcp) => tcp,
            Err(error) => {
                log::debug!(
                    "SshSession: remote forward target {}:{} unreachable: {error}",
                    target.0,
                    target.1
                );
                return;
            }
        };
        let mut remote = channel.into_stream();
        tokio::select! {
            _ = shutdown.cancelled() => {}
            result = tokio::io::copy_bidirectional(&mut remote, &mut tcp) => {
                if let Err(error) = result {
                    log::debug!("SshSession: remote forward relay ended: {error}");
                }
            }
        }
    });
}

/// The `forwarded-tcpip` target for a server-side listener, when OneTerm asked
/// for it.
pub(crate) fn forwarded_target(
    table: &ForwardTable,
    address: &str,
    port: u32,
) -> Option<(String, u16)> {
    table
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(address.to_string(), port))
        .cloned()
}

// ── SOCKS5 (RFC 1928): no authentication, CONNECT only ──────────────────

const SOCKS5_HANDSHAKE_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub(crate) enum SocksError {
    #[error("not SOCKS5")]
    Version,
    #[error("client offers no no-authentication method")]
    NoAuthMethod,
    #[error("only CONNECT is supported")]
    Command,
    #[error("unknown address type")]
    AddressType,
    #[error("handshake timed out")]
    Timeout,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Run the SOCKS5 greeting and CONNECT request on `tcp` and return the
/// destination as text (domains are resolved on the remote side). The success
/// reply is sent by the caller once the channel is open; refusals are sent
/// here before the error is returned.
pub(crate) async fn socks5_connect_target(
    tcp: &mut TcpStream,
) -> Result<(String, u16), SocksError> {
    match tokio::time::timeout(SOCKS5_HANDSHAKE_DEADLINE, socks5_handshake(tcp)).await {
        Ok(result) => result,
        Err(_) => Err(SocksError::Timeout),
    }
}

async fn socks5_handshake(tcp: &mut TcpStream) -> Result<(String, u16), SocksError> {
    let mut head = [0u8; 2];
    tcp.read_exact(&mut head).await?;
    if head[0] != 0x05 {
        return Err(SocksError::Version);
    }
    let mut methods = vec![0u8; usize::from(head[1])];
    tcp.read_exact(&mut methods).await?;
    if !methods.contains(&0x00) {
        tcp.write_all(&[0x05, 0xFF]).await?;
        return Err(SocksError::NoAuthMethod);
    }
    tcp.write_all(&[0x05, 0x00]).await?;

    let mut request = [0u8; 4];
    tcp.read_exact(&mut request).await?;
    if request[0] != 0x05 {
        return Err(SocksError::Version);
    }
    // Read the whole request before judging the command: closing a socket
    // with unread bytes makes Windows send a reset instead of the reply.
    let host = match request[3] {
        0x01 => {
            let mut ipv4 = [0u8; 4];
            tcp.read_exact(&mut ipv4).await?;
            std::net::Ipv4Addr::from(ipv4).to_string()
        }
        0x03 => {
            let len = usize::from(tcp.read_u8().await?);
            let mut name = vec![0u8; len];
            tcp.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        0x04 => {
            let mut ipv6 = [0u8; 16];
            tcp.read_exact(&mut ipv6).await?;
            std::net::Ipv6Addr::from(ipv6).to_string()
        }
        _ => {
            tcp.write_all(&socks5_reply(0x08)).await?;
            return Err(SocksError::AddressType);
        }
    };
    let port = tcp.read_u16().await?;
    if request[1] != 0x01 {
        tcp.write_all(&socks5_reply(0x07)).await?;
        return Err(SocksError::Command);
    }
    Ok((host, port))
}

/// A SOCKS5 reply with `rep` and an all-zero IPv4 bind address.
fn socks5_reply(rep: u8) -> [u8; 10] {
    [0x05, rep, 0x00, 0x01, 0, 0, 0, 0, 0, 0]
}

// Forward tests against an in-process echo server live in a sibling file (see code-style.md).
#[cfg(test)]
#[path = "tunnel_tests.rs"]
mod tunnel_tests;
