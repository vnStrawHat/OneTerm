//! `SshSession` — SSH client qua russh + tokio runtime ẩn.
//!
//! API lộ ra ngoài là sync (dùng `block_on` cho connect). Tokio runtime ẩn
//! bên trong (`multi_thread` 1 worker). Lệnh ghi/resize/close gửi qua
//! `async_channel` (bridge sync→async). Event ra ngoài cũng qua `async_channel`.
//!
//! **Quan trọng**: `russh::client::Handle` phải giữ sống — drop = đóng kết nối.
//! Handle được move vào `ssh_main_task` và giữ đến khi session đóng.
//!
//! SFTP channel được mở **trước khi** spawn `ssh_main_task` (trong `block_on`),
//! tách ra object riêng — không cần handle nữa. Hai channel (shell + sftp)
//! chia sẻ cùng 1 TCP connection, multiplex bởi russh.
//!
//! Tham chiếu `docs/terminal-backend.md` §7, `docs/sftp-browser-design.md`.

use std::sync::{Arc, Mutex};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use async_channel::Receiver;
use russh::client;
use russh::client::AuthResult;
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, load_secret_key};

use myterm2_core::SessionEvent;

use crate::config::{SshAuthMethod, SshConfig};
use crate::counting_stream::CountingStream;
use crate::handler::SshClientHandler;
use crate::listener::{Cmd, SshListener};
use crate::sftp::{SftpCmd, SftpEvent, SftpSession};
use crate::sftp_task::sftp_task;
use crate::state::{SharedState, new_shared};
use crate::task::ssh_main_task;

/// Kích thước PTY ban đầu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}

/// Dimensions cho `Term::new` / `Term::resize`.
pub(crate) struct TermSize {
    pub(crate) cols: usize,
    pub(crate) lines: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Một session SSH (russh + tokio runtime ẩn).
pub struct SshSession {
    pub(crate) term: Arc<FairMutex<Term<SshListener>>>,
    pub(crate) listener: SshListener,
    pub(crate) event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    pub(crate) state: SharedState,
    /// Giữ `Sender` sống — listener có clone riêng.
    #[allow(dead_code)]
    pub(crate) cmd_tx: async_channel::Sender<Cmd>,
    pub(crate) _runtime: tokio::runtime::Runtime,
    pub(crate) cell_width: Mutex<f32>,
    pub(crate) line_height: Mutex<f32>,
    pub(crate) marked_text: Mutex<Option<String>>,
    /// SFTP session (None = server không hỗ trợ SFTP).
    pub(crate) sftp: Mutex<Option<Arc<SftpSession>>>,
}

/// Kết nối SSH tới server. API sync — dùng `block_on` cho connect.
/// Runtime `multi_thread` (1 worker) giữ background task chạy sau khi
/// `block_on()` trả về.
pub fn connect(
    cfg: SshConfig,
    initial: PtySize,
    scrollback_history: usize,
) -> myterm2_core::Result<Box<dyn myterm2_core::TerminalSession>> {
    log::info!(
        "SshSession::connect: host={}, port={}, user={}, rows={}, cols={}",
        cfg.host,
        cfg.port,
        cfg.username,
        initial.rows,
        initial.cols
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .thread_name("ssh-runtime")
        .build()
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;

    let (cmd_tx, cmd_rx) = async_channel::bounded::<Cmd>(64);
    let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(4096);
    let state = new_shared();
    state.lock().unwrap().alive = true;

    let listener = SshListener::new(event_tx, cmd_tx.clone(), state.clone());

    let size = TermSize {
        cols: initial.cols as usize,
        lines: initial.rows as usize,
    };
    let term_config = Config {
        scrolling_history: scrollback_history,
        ..Default::default()
    };
    let term = Arc::new(FairMutex::new(Term::new(
        term_config,
        &size,
        listener.clone(),
    )));

    // ── Connect (block_on) ──────────────────────────────────────────
    let connect_result = runtime.block_on(async {
        let addr = format!("{}:{}", cfg.host, cfg.port);
        log::info!("SshSession: connecting to {addr}");
        let client_cfg = russh::client::Config::default();
        let handler = SshClientHandler;

        let mut handle = client::connect(Arc::new(client_cfg), addr, handler)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        log::info!("SshSession: TCP connected");

        // ── Authenticate ──────────────────────────────────────────────
        let auth_result = match &cfg.auth {
            SshAuthMethod::None => {
                log::info!("SshSession: authenticating with none (no password)");
                handle
                    .authenticate_none(&cfg.username)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
            }
            SshAuthMethod::Password { password } => {
                log::info!("SshSession: authenticating with password");
                handle
                    .authenticate_password(&cfg.username, password)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
            }
            SshAuthMethod::PrivateKey {
                key_path,
                passphrase,
            } => {
                log::info!("SshSession: authenticating with key {}", key_path.display());
                let key = load_private_key(key_path, passphrase.as_deref())?;
                let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key), None);
                handle
                    .authenticate_publickey(&cfg.username, key_with_alg)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
            }
            SshAuthMethod::Agent => {
                return Err(anyhow::anyhow!("SSH agent auth chưa hỗ trợ (roadmap)"));
            }
        };
        log::info!("SshSession: auth result = {auth_result:?}");
        if !matches!(auth_result, AuthResult::Success) {
            return Err(anyhow::anyhow!("SSH authentication failed"));
        }

        // ── Open channel + pty + shell ──────────────────────────────
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        log::info!("SshSession: channel opened");

        channel
            .request_pty(
                false,
                "xterm-256color",
                initial.cols as u32,
                initial.rows as u32,
                0,
                0,
                &[],
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        log::info!(
            "SshSession: pty requested ({}x{})",
            initial.cols,
            initial.rows
        );

        channel
            .request_shell(true)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        log::info!("SshSession: shell requested");

        // ── Mở SFTP channel (optional) ──────────────────────────────
        // Mở TRƯỚC khi spawn ssh_main_task vì handle sẽ bị move vào task.
        // SFTP channel tách ra object riêng — không cần handle nữa.
        let sftp_session = match open_sftp(&handle, &state).await {
            Ok(sftp) => {
                log::info!("SshSession: SFTP channel opened");
                Some(sftp)
            }
            Err(e) => {
                log::warn!("SshSession: SFTP not available: {e} — terminal only");
                None
            }
        };

        // ── Spawn main SSH task ──────────────────────────────────────
        // QUAN TRỌNG: `handle` phải move vào task — drop handle = đóng kết nối.
        tokio::spawn(ssh_main_task(
            handle,
            channel,
            term.clone(),
            listener.clone(),
            state.clone(),
            cmd_rx,
        ));
        log::info!("SshSession: main task spawned");

        Ok::<_, anyhow::Error>(sftp_session)
    });

    match connect_result {
        Ok(sftp_session) => {
            log::info!("SshSession: connect successful");
            // multi_thread runtime: worker thread tự chạy spawned tasks.
            let session = SshSession {
                term,
                listener,
                event_rx: Mutex::new(Some(event_rx)),
                state,
                cmd_tx,
                _runtime: runtime,
                cell_width: Mutex::new(0.0),
                line_height: Mutex::new(0.0),
                marked_text: Mutex::new(None),
                sftp: Mutex::new(sftp_session),
            };
            Ok(Box::new(session) as Box<dyn myterm2_core::TerminalSession>)
        }
        Err(e) => {
            log::error!("SshSession: connect failed: {e}");
            Err(myterm2_core::AppError::msg(e.to_string()))
        }
    }
}

/// Mở SFTP channel trên cùng `handle` + spawn `sftp_task`.
///
/// Flow:
/// 1. `handle.channel_open_session()` → channel mới (cùng TCP connection)
/// 2. `channel.request_subsystem("sftp")` → yêu cầu SFTP subsystem
/// 3. `channel.into_stream()` → convert thành `AsyncRead + AsyncWrite`
/// 4. `russh_sftp::client::SftpSession::new(stream)` → SFTP handshake
/// 5. Tạo channels `(cmd_tx, cmd_rx)` + `(event_tx, event_rx)`
/// 6. `tokio::spawn(sftp_task(...))` — task chạy nền
/// 7. Return `Arc<SftpSession>` — bridge cho UI gọi sync
async fn open_sftp(
    handle: &russh::client::Handle<SshClientHandler>,
    state: &SharedState,
) -> anyhow::Result<Arc<SftpSession>> {
    // 1. Mở channel mới trên cùng handle.
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| anyhow::anyhow!("SFTP channel_open_session: {e}"))?;

    // 2. Request SFTP subsystem.
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| anyhow::anyhow!("SFTP request_subsystem: {e}"))?;

    // 3. Convert channel → stream (AsyncRead + AsyncWrite).
    //    Wrap với CountingStream để đếm bytes rx/tx — gộp vào cùng
    //    SharedState với SSH shell channel → tổng network traffic.
    let stream = CountingStream::new(channel.into_stream(), state.clone());

    // 4. SFTP handshake — tạo SftpChannel.
    let sftp_channel = russh_sftp::client::SftpSession::new(stream)
        .await
        .map_err(|e| anyhow::anyhow!("SFTP handshake: {e}"))?;

    // 5. Tạo channels bridge sync (UI) ↔ async (tokio task).
    let (sftp_cmd_tx, sftp_cmd_rx) = async_channel::bounded::<SftpCmd>(64);
    let (sftp_event_tx, sftp_event_rx) = async_channel::bounded::<SftpEvent>(40);
    let alive = Arc::new(Mutex::new(true));

    // 6. Spawn sftp_task — chạy nền trên cùng tokio runtime.
    tokio::spawn(sftp_task(
        sftp_channel,
        sftp_cmd_rx,
        sftp_event_tx,
        alive.clone(),
    ));

    // 7. Return Arc<SftpSession> — UI giữ handle này.
    Ok(SftpSession::new(sftp_cmd_tx, sftp_event_rx, alive))
}

/// Tải private key từ file, giải mã bằng passphrase nếu cần.
fn load_private_key(
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> anyhow::Result<PrivateKey> {
    let key = load_secret_key(path, passphrase)
        .map_err(|e| anyhow::anyhow!("Failed to load key {}: {e}", path.display()))?;
    Ok(key)
}
