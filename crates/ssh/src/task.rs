//! Main tokio task for the SSH session — reads data from the channel + handles commands.
//!
//! **`handle` must be kept alive** — dropping `russh::client::Handle` closes the
//! connection. The handle is moved into the task and held until the session closes.

use std::sync::Arc;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::Parser as VteParser;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use russh::ChannelMsg;

use oneterm_core::SessionEvent;
use oneterm_core::terminal::default_color_for_index;
use oneterm_core::terminal::osc::{Osc133Kind, OscPayload, OscSink, parse_cwd_url};

use crate::handler::SshClientHandler;
use crate::listener::{Cmd, SshListener};
use crate::session::TermSize;
use crate::state::SharedState;

/// Main tokio task: reads data from the SSH channel + receives commands from the
/// main thread. Feeds bytes to `Term` (via ansi::Processor) + `OscSink` (via
/// vte::Parser).
///
/// **`handle` must be kept alive** — dropping it closes the SSH connection.
pub(crate) async fn ssh_main_task(
    _handle: russh::client::Handle<SshClientHandler>,
    mut channel: russh::Channel<russh::client::Msg>,
    term: Arc<FairMutex<Term<SshListener>>>,
    listener: SshListener,
    state: SharedState,
    cmd_rx: async_channel::Receiver<Cmd>,
) {
    log::info!("ssh_main_task: started");
    let mut processor = Processor::<StdSyncHandler>::new();
    let mut vte_parser = VteParser::new();
    let mut osc_sink = OscSink::default();

    loop {
        tokio::select! {
            // ── Read data from the SSH channel ────────────────────────
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        let bytes: &[u8] = data.as_ref();
                        log::debug!("ssh_main_task: recv {} bytes", bytes.len());
                        // Track network stats — bytes received from server.
                        state.lock().unwrap().rx_bytes += bytes.len() as u64;
                        // Feed Term (ansi::Processor).
                        {
                            let mut term = term.lock();
                            processor.advance(&mut *term, bytes);

                            // Track absolute line count.
                            let total_after = term.total_lines();
                            let screen_lines = term.screen_lines();
                            let (mut absolute, mut prev_total) = {
                                let st = state.lock().unwrap();
                                (st.absolute_line_count, st.prev_total_lines)
                            };
                            if total_after > prev_total {
                                absolute += total_after - prev_total;
                            } else if total_after == prev_total
                                && total_after > screen_lines
                            {
                                let nl = bytes.iter().filter(|&&b| b == b'\n').count();
                                absolute += nl;
                            } else if total_after < prev_total {
                                absolute = total_after;
                            }
                            prev_total = total_after;
                            drop(term);

                            let mut st = state.lock().unwrap();
                            st.absolute_line_count = absolute;
                            st.prev_total_lines = prev_total;
                        }

                        // Answer OSC 10/11/12 color queries collected during
                        // advance: read the current color from `Term`, fall back
                        // to the theme default, then write the reply to the channel.
                        let queries = listener.take_color_queries();
                        if !queries.is_empty() {
                            let (def_fg, def_bg, def_cursor, def_ansi) = {
                                let st = state.lock().unwrap();
                                (
                                    st.default_foreground,
                                    st.default_background,
                                    st.default_cursor,
                                    st.default_ansi,
                                )
                            };
                            let mut replies: Vec<String> = Vec::new();
                            {
                                let term = term.lock();
                                for q in queries {
                                    let color = term.colors()[q.index].or_else(|| {
                                        default_color_for_index(
                                            q.index,
                                            def_fg,
                                            def_bg,
                                            def_cursor,
                                            def_ansi.as_ref(),
                                        )
                                    });
                                    if let Some(color) = color {
                                        replies.push((q.format)(color));
                                    }
                                }
                            }
                            for reply in replies {
                                listener.pty_write(reply.as_bytes());
                            }
                        }

                        // Feed OscSink (vte::Parser) — in parallel.
                        vte_parser.advance(&mut osc_sink, bytes);
                        while let Some(payload) = osc_sink.take() {
                            handle_osc(&payload, &state, &listener);
                        }

                        // Screen was just cleared (clear/RIS) → bump clear_epoch so
                        // the UI resets per-line timestamps (gutter).
                        if osc_sink.take_clear() {
                            state.lock().unwrap().clear_epoch += 1;
                        }

                        // Notify UI.
                        listener.forward(SessionEvent::Output);
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        let code = exit_status as i32;
                        log::info!("ssh_main_task: exit status = {code}");
                        {
                            let mut st = state.lock().unwrap();
                            st.alive = false;
                            st.exit_code = Some(code);
                        }
                        listener.forward(SessionEvent::Exited(Some(code)));
                    }
                    Some(ChannelMsg::Eof) => {
                        log::info!("ssh_main_task: EOF received");
                        {
                            let mut st = state.lock().unwrap();
                            st.alive = false;
                        }
                        listener.forward(SessionEvent::Closed);
                        break;
                    }
                    Some(ChannelMsg::Close) => {
                        log::info!("ssh_main_task: channel closed");
                        {
                            let mut st = state.lock().unwrap();
                            st.alive = false;
                        }
                        listener.forward(SessionEvent::Closed);
                        break;
                    }
                    None => {
                        log::info!("ssh_main_task: channel.wait() = None (disconnected)");
                        {
                            let mut st = state.lock().unwrap();
                            st.alive = false;
                        }
                        listener.forward(SessionEvent::Closed);
                        break;
                    }
                    Some(other) => {
                        log::debug!("ssh_main_task: unhandled msg: {other:?}");
                    }
                }
            }
            // ── Receive commands from the main thread ─────────────────
            cmd = cmd_rx.recv() => {
                match cmd {
                    Ok(Cmd::Write(bytes)) => {
                        log::debug!(
                            "ssh_main_task: write {} bytes: {:?}",
                            bytes.len(),
                            String::from_utf8_lossy(&bytes)
                        );
                        // Track network stats — bytes sent to server.
                        state.lock().unwrap().tx_bytes += bytes.len() as u64;
                        if let Err(e) = channel.data(&bytes[..]).await {
                            log::warn!("ssh_main_task: channel.data fail: {e}");
                        }
                    }
                    Ok(Cmd::Resize(rows, cols)) => {
                        log::info!("ssh_main_task: resize {cols}x{rows}");
                        if let Err(e) = channel
                            .window_change(cols as u32, rows as u32, 0, 0)
                            .await
                        {
                            log::warn!("ssh_main_task: window_change fail: {e}");
                        }
                        term.lock().resize(TermSize {
                            cols: cols as usize,
                            lines: rows as usize,
                        });
                    }
                    Ok(Cmd::Close) => {
                        log::info!("ssh_main_task: close requested");
                        let _ = channel.close().await;
                        {
                            let mut st = state.lock().unwrap();
                            st.alive = false;
                        }
                        break;
                    }
                    Err(_) => {
                        log::info!("ssh_main_task: cmd_rx closed — session dropped");
                        break;
                    }
                }
            }
        }
    }
    log::info!("ssh_main_task: exiting");
}

/// Handle OSC payload — update state + forward events.
fn handle_osc(payload: &OscPayload, state: &SharedState, listener: &SshListener) {
    match payload {
        OscPayload::Cwd(url) => {
            let cwd = parse_cwd_url(url);
            {
                let mut st = state.lock().unwrap();
                st.cwd = Some(cwd.clone());
            }
            listener.forward(SessionEvent::Cwd(cwd));
        }
        OscPayload::ShellIntegration(kind) => {
            {
                let mut st = state.lock().unwrap();
                match kind {
                    Osc133Kind::PromptStart => {
                        st.prompt_count = st.prompt_count.saturating_add(1);
                    }
                    Osc133Kind::OutputEnd { exit_code } => {
                        st.last_exit_code = *exit_code;
                    }
                    _ => {}
                }
            }
            listener.forward(SessionEvent::ShellIntegration(*kind));
        }
        OscPayload::Clipboard { query, .. } => {
            // Set (query=false) is handled by alacritty's ClipboardStore.
            // Read (query=true) → ask the UI to reply with the clipboard.
            if *query {
                listener.forward(SessionEvent::ClipboardRead);
            }
        }
        OscPayload::Notification(msg) => {
            listener.forward(SessionEvent::Notification(msg.clone()));
        }
        OscPayload::Progress(progress) => {
            listener.forward(SessionEvent::Progress(*progress));
        }
    }
}
