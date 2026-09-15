//! The render handshake, driven against the real `ssh_main_task`.
//!
//! `crates/vt/src/render/render_tests.rs:785` proves the property on a bare
//! engine with two threads; this is the same shape against the loop that ships:
//! a loopback server floods the shell channel, the task pumps it, and a second
//! task asks for the engine the way `TerminalModel::snapshot` does.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use russh::server::{Auth, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId};
use tokio_util::sync::CancellationToken;

use oneterm_terminal::{
    ClipboardOrigin, DEFAULT_SCROLLBACK_LINES, GridSize, OscRouter, PtyTransport, SessionEvent,
    SessionEventSink, SharedSessionState, new_shared_terminal,
};

use super::ssh_main_task;
use crate::route::JumpHandles;
use crate::session::authenticate_with_password;
use crate::test_support::{connect_trusting_loopback, spawn_server};
use crate::transport::{Cmd, SSH_COMMAND_QUEUE_CAPACITY, SshTransport};
use crate::tunnel::HANDLE_REQUEST_CAPACITY;

/// Writes to the shell channel as fast as the client drains it, until the test
/// stops it — the sustained-output shape the handshake exists for.
#[derive(Clone)]
struct FloodingServer {
    stop: Arc<AtomicBool>,
}

impl Server for FloodingServer {
    type Handler = Self;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        self.clone()
    }
}

impl Handler for FloodingServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let handle = session.handle();
        let stop = Arc::clone(&self.stop);
        tokio::spawn(async move {
            let chunk = "sustained output from the remote side\r\n"
                .repeat(100)
                .into_bytes();
            while !stop.load(Ordering::Relaxed) {
                if handle.data(channel, chunk.clone()).await.is_err() {
                    break;
                }
            }
        });
        Ok(())
    }
}

/// The pump's half of the handshake (R-37): under sustained output the loop asks
/// for the flag at its chunk boundary, so a frame gets the engine promptly and
/// the flag does not stay raised.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_task_yields_the_engine_to_a_waiting_frame() {
    let stop = Arc::new(AtomicBool::new(false));
    let (address, server_task) = spawn_server(FloodingServer { stop: stop.clone() }).await;
    let (mut handle, _known_hosts) = connect_trusting_loopback(address).await;
    authenticate_with_password(&mut handle, "user", "secret")
        .await
        .expect("password auth must succeed");
    let channel = handle
        .channel_open_session()
        .await
        .expect("session channel must open");
    channel
        .request_shell(false)
        .await
        .expect("the shell request must start the flood");

    let (cmd_tx, cmd_rx) = async_channel::bounded::<Cmd>(SSH_COMMAND_QUEUE_CAPACITY);
    // Held for the whole test: a dropped receiver would turn every repaint hint
    // into a send failure instead of the coalescing the loop expects.
    let (event_tx, _event_rx) = async_channel::bounded::<SessionEvent>(4096);
    let state = SharedSessionState::new_alive();
    let listener = OscRouter::new(
        SshTransport::new(cmd_tx),
        SessionEventSink::new(event_tx),
        state.clone(),
        ClipboardOrigin::Remote,
    );
    let transport = listener.transport().clone();
    let term = new_shared_terminal(
        GridSize {
            cols: 80,
            lines: 24,
        },
        DEFAULT_SCROLLBACK_LINES,
    );
    let (_open_tx, open_rx) = tokio::sync::mpsc::channel(HANDLE_REQUEST_CAPACITY);
    let task = tokio::spawn(ssh_main_task(
        handle,
        JumpHandles::default(),
        channel,
        term.clone(),
        listener,
        cmd_rx,
        open_rx,
        CancellationToken::new(),
    ));

    // Let the loop reach its steady state before asking for the lock.
    let deadline = Instant::now() + Duration::from_secs(20);
    while state.net_stats().rx_bytes < 256 * 1024 {
        assert!(
            Instant::now() < deadline,
            "the flood never reached the task ({} bytes)",
            state.net_stats().rx_bytes
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let render = tokio::task::spawn_blocking({
        let term = Arc::clone(&term);
        move || {
            let waiting = Instant::now();
            drop(term.lock_for_render());
            let waited = waiting.elapsed();
            // The loop clears the flag by asking for it; the renderer never
            // raises it again here, so a flag still up after the frame means
            // the chunk boundary never asked.
            let give_up = Instant::now() + Duration::from_secs(5);
            while term.render_demand_raised() {
                if Instant::now() > give_up {
                    return (waited, false);
                }
                std::thread::yield_now();
            }
            (waited, true)
        }
    });
    let (waited, taken) = render.await.expect("the render task must not panic");

    stop.store(true, Ordering::Relaxed);
    transport.pty_close().expect("close must be accepted");
    let _ = tokio::time::timeout(Duration::from_secs(10), task).await;
    server_task.abort();

    assert!(
        waited < Duration::from_secs(2),
        "the frame waited {waited:?} for the engine under sustained output"
    );
    assert!(taken, "the loop never took the render demand");
}
