//! `SshSession` — SSH client qua russh + tokio runtime ẩn.
//!
//! API lộ ra ngoài là sync (dùng `block_on` cho connect). Tokio runtime ẩn
//! bên trong (`multi_thread` 1 worker). Lệnh ghi/resize/close gửi qua
//! `async_channel` (bridge sync→async). Event ra ngoài cũng qua `async_channel`.
//!
//! **Quan trọng**: `russh::client::Handle` phải giữ sống — drop = đóng kết nối.
//! Handle được move vào `ssh_main_task` và giữ đến khi session đóng.
//!
//! Tham chiếu `docs/terminal-backend.md` §7.

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
use crate::handler::SshClientHandler;
use crate::listener::{Cmd, SshListener};
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

        Ok::<_, anyhow::Error>(())
    });

    match connect_result {
        Ok(()) => {
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
            };
            Ok(Box::new(session) as Box<dyn myterm2_core::TerminalSession>)
        }
        Err(e) => {
            log::error!("SshSession: connect failed: {e}");
            Err(myterm2_core::AppError::msg(e.to_string()))
        }
    }
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
