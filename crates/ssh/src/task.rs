//! Main tokio task for the SSH session — reads data from the channel + handles commands.
//!
//! **`handle` must be kept alive** — dropping `russh::client::Handle` closes the
//! connection. The handle is moved into the task and held until the session closes.

use std::sync::Arc;

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use russh::ChannelMsg;
use tokio_util::sync::CancellationToken;

use oneterm_terminal::TerminalPump;

use crate::handler::SshClientHandler;
use crate::transport::{Cmd, SshListener, SshTransport};

/// Main tokio task: reads data from the SSH channel + receives commands from the
/// main thread. Feeds bytes to `Term` through the shared [`TerminalPump`] in a
/// **single pass**; OSC 7/9/133 and screen clears arrive via `Event::Osc` /
/// `Event::ClearScreen` (OneTerm alacritty fork) and are handled by the shared
/// `OscRouter` — no second parser.
///
/// The `Term` grid is resized by the UI thread (`TerminalSession::resize`);
/// this task only forwards the coalesced size to the remote PTY (CORR-21).
///
/// **`handle` must be kept alive** — dropping it closes the SSH connection.
/// When the task ends it cancels `sftp_shutdown` so the SFTP task dies with the
/// connection (ARCH-28).
pub(crate) async fn ssh_main_task(
    _handle: russh::client::Handle<SshClientHandler>,
    mut channel: russh::Channel<russh::client::Msg>,
    term: Arc<FairMutex<Term<SshListener>>>,
    listener: SshListener,
    cmd_rx: async_channel::Receiver<Cmd>,
    sftp_shutdown: CancellationToken,
) {
    log::info!("ssh_main_task: started");
    let mut pump = TerminalPump::new(listener);
    let transport: SshTransport = pump.router().transport().clone();
    let state = pump.state().clone();

    // Every exit path breaks with a reason and falls through to the single
    // teardown block below (CORR-11), so `Closed` always follows the deferred
    // reliable events and the SFTP task always dies with the connection.
    let reason: &'static str = loop {
        // If close was requested (even if Cmd::Close was dropped due to a
        // full queue), honor the closing flag immediately.
        if transport.is_closing() {
            break "closing flag set";
        }
        if let Some((rows, cols)) = transport.take_pending_resize() {
            log::info!("ssh_main_task: resize {cols}x{rows}");
            if let Err(error) = channel.window_change(cols as u32, rows as u32, 0, 0).await {
                log::warn!("ssh_main_task: window_change fail: {error}");
            }
        }
        tokio::select! {
            // ── Read data from the SSH channel ────────────────────────
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        let bytes: &[u8] = data.as_ref();
                        log::debug!("ssh_main_task: recv {} bytes", bytes.len());
                        state.add_rx_bytes(bytes.len() as u64);
                        // Parse under the Term lock, answer OSC colour queries,
                        // then (lock released) flush deferred reliable events
                        // and post the repaint hint.
                        pump.process_chunk(&term, bytes);
                        pump.finish_batch(true).await;
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        let code = exit_status as i32;
                        log::info!("ssh_main_task: exit status = {code}");
                        // Keep draining until the server closes the channel
                        // so a late `Data` chunk (exit banner) still renders.
                        pump.publish_exit(Some(code)).await;
                    }
                    Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                        log::info!("ssh_main_task: exit signal = {signal_name:?}");
                        pump.publish_exit(None).await;
                    }
                    Some(ChannelMsg::Eof) => break "EOF received",
                    Some(ChannelMsg::Close) => break "channel closed",
                    None => break "disconnected",
                    Some(other) => {
                        log::debug!("ssh_main_task: unhandled msg: {other:?}");
                    }
                }
            }
            // ── Receive commands from the main thread ─────────────────
            cmd = cmd_rx.recv() => {
                match cmd {
                    Ok(Cmd::Write(bytes)) => {
                        log::debug!("ssh_main_task: write {} bytes", bytes.len());
                        state.add_tx_bytes(bytes.len() as u64);
                        if let Err(e) = channel.data(&bytes[..]).await {
                            log::warn!("ssh_main_task: channel.data fail: {e}");
                        }
                        transport.release_write_bytes(bytes.len());
                    }
                    Ok(Cmd::Resize) => {}
                    Ok(Cmd::Close) => break "close requested",
                    Err(_) => {
                        // Unreachable while the task runs: `term` and `listener`
                        // both hold a `cmd_tx` clone. The closing flag (checked at
                        // the top of the loop) is the shutdown signal —
                        // `SshSession::drop` sets it.
                        log::warn!("ssh_main_task: cmd_rx closed unexpectedly");
                        break "command channel closed";
                    }
                }
            }
        }
    };

    // ── Single teardown block (CORR-11 / ARCH-28) ────────────────────────
    log::info!("ssh_main_task: {reason} — tearing down");
    let _ = channel.close().await;
    pump.publish_closed().await;
    sftp_shutdown.cancel();
    log::info!("ssh_main_task: exiting");
}
