//! `SshSession` — SSH client over russh + a hidden tokio runtime.
//!
//! The public API is sync (uses `block_on` for connect). The tokio runtime is
//! hidden inside (`multi_thread`, 1 worker). Write/resize/close commands are
//! sent via `async_channel` (sync→async bridge). Outgoing events also go through
//! `async_channel`.
//!
//! **Important**: `russh::client::Handle` must be kept alive — dropping it closes
//! the connection. The handle is moved into `ssh_main_task` and held until the
//! session closes.
//!
//! The SFTP channel is opened **before** spawning `ssh_main_task` (inside
//! `block_on`) and split into its own object — no handle needed afterwards. The
//! two channels (shell + sftp) share the same TCP connection, multiplexed by
//! russh.
//!
//! See `docs/terminal-backend.md` §7, `docs/sftp-browser-design.md`.

use std::sync::{Arc, Mutex};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use async_channel::Receiver;
use russh::client;
use russh::client::AuthResult;
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, load_secret_key};

use oneterm_core::SessionEvent;

use crate::config::{SshAuthMethod, SshConfig};
use crate::counting_stream::CountingStream;
use crate::handler::SshClientHandler;
use crate::listener::{Cmd, SshListener};
use crate::sftp::{SftpCmd, SftpEvent, SftpSession};
use crate::sftp_task::sftp_task;
use crate::state::{SharedState, new_shared};
use crate::task::ssh_main_task;

/// Initial PTY size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}

/// Dimensions for `Term::new` / `Term::resize`.
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

/// An SSH session (russh + a hidden tokio runtime).
pub struct SshSession {
    pub(crate) term: Arc<FairMutex<Term<SshListener>>>,
    pub(crate) listener: SshListener,
    pub(crate) event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    pub(crate) state: SharedState,
    /// Keep the `Sender` alive — the listener has its own clone.
    #[allow(dead_code)]
    pub(crate) cmd_tx: async_channel::Sender<Cmd>,
    pub(crate) _runtime: tokio::runtime::Runtime,
    pub(crate) cell_width: Mutex<f32>,
    pub(crate) line_height: Mutex<f32>,
    pub(crate) marked_text: Mutex<Option<String>>,
    /// SFTP session (None = server does not support SFTP).
    pub(crate) sftp: Mutex<Option<Arc<SftpSession>>>,
}

/// Connect over SSH to a server. Sync API — uses `block_on` for connect.
/// The `multi_thread` runtime (1 worker) keeps background tasks running after
/// `block_on()` returns.
pub fn connect(
    cfg: SshConfig,
    initial: PtySize,
    scrollback_history: usize,
) -> oneterm_core::Result<Box<dyn oneterm_core::TerminalSession>> {
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
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;

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
                return Err(anyhow::anyhow!(
                    "SSH agent auth not supported yet (roadmap)"
                ));
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

        // ── Shell integration (OSC 7 cwd) — silent, via exec ──────
        // Instead of `request_shell`, run an exec request that:
        //   1. defines a prompt hook emitting OSC 7 (cwd) + OSC 133;A,
        //   2. exports it (function + PROMPT_COMMAND) into the environment,
        //   3. `exec`s the user's interactive login shell, which inherits it.
        //
        // Steps 1–2 run in a NON-interactive shell (sshd runs `$SHELL -c <cmd>`),
        // so there is no readline/PTY echo → completely silent. Unlike the `env`
        // channel request, this does not depend on sshd `AcceptEnv`.
        //
        // bash-oriented (`export -f` + `PROMPT_COMMAND`); zsh/others: no OSC 7 but
        // harmless. `.bashrc` that overwrites `PROMPT_COMMAND` would defeat it.
        // Disable via `SshConfig::shell_integration = false`.
        if cfg.shell_integration {
            channel
                .exec(true, SHELL_INTEGRATION_EXEC)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            log::info!("SshSession: shell started via exec (shell integration)");
        } else {
            channel
                .request_shell(true)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            log::info!("SshSession: shell requested");
        }

        // ── Open SFTP channel (optional) ────────────────────────────
        // Open it BEFORE spawning ssh_main_task because the handle is moved into
        // the task. The SFTP channel is split into its own object — no handle needed.
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
        // IMPORTANT: `handle` must be moved into the task — dropping it closes the
        // connection.
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
            // multi_thread runtime: the worker thread runs spawned tasks itself.
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
            Ok(Box::new(session) as Box<dyn oneterm_core::TerminalSession>)
        }
        Err(e) => {
            log::error!("SshSession: connect failed: {e}");
            Err(oneterm_core::AppError::msg(e.to_string()))
        }
    }
}

/// Open an SFTP channel on the same `handle` + spawn `sftp_task`.
///
/// Flow:
/// 1. `handle.channel_open_session()` → new channel (same TCP connection)
/// 2. `channel.request_subsystem("sftp")` → request the SFTP subsystem
/// 3. `channel.into_stream()` → convert into `AsyncRead + AsyncWrite`
/// 4. `russh_sftp::client::SftpSession::new(stream)` → SFTP handshake
/// 5. Create channels `(cmd_tx, cmd_rx)` + `(event_tx, event_rx)`
/// 6. `tokio::spawn(sftp_task(...))` — runs in the background
/// 7. Return `Arc<SftpSession>` — bridge for the UI to call synchronously
async fn open_sftp(
    handle: &russh::client::Handle<SshClientHandler>,
    state: &SharedState,
) -> anyhow::Result<Arc<SftpSession>> {
    // 1. Open a new channel on the same handle.
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
    //    Wrap with CountingStream to count rx/tx bytes — merged into the same
    //    SharedState as the SSH shell channel → total network traffic.
    let stream = CountingStream::new(channel.into_stream(), state.clone());

    // 4. SFTP handshake — create the SftpChannel.
    let sftp_channel = russh_sftp::client::SftpSession::new(stream)
        .await
        .map_err(|e| anyhow::anyhow!("SFTP handshake: {e}"))?;

    // 5. Create channels bridging sync (UI) ↔ async (tokio task).
    let (sftp_cmd_tx, sftp_cmd_rx) = async_channel::bounded::<SftpCmd>(64);
    let (sftp_event_tx, sftp_event_rx) = async_channel::bounded::<SftpEvent>(40);
    let alive = Arc::new(Mutex::new(true));

    // 6. Spawn sftp_task — runs in the background on the same tokio runtime.
    tokio::spawn(sftp_task(
        sftp_channel,
        sftp_cmd_rx,
        sftp_event_tx,
        alive.clone(),
    ));

    // 7. Return Arc<SftpSession> — the UI holds this handle.
    Ok(SftpSession::new(sftp_cmd_tx, sftp_event_rx, alive))
}

/// Exec command used instead of `request_shell` to enable shell integration
/// **silently**. sshd runs it as `$SHELL -c <cmd>` (non-interactive → no echo):
///
/// 1. define `__oneterm_osc7` — emits **OSC 7** (cwd) + **OSC 133;A** (prompt marker),
/// 2. `export -f __oneterm_osc7` + `PROMPT_COMMAND=__oneterm_osc7` → inherited by the child shell,
/// 3. print the MOTD (which sshd/PAM only shows for `request_shell`, not `exec`),
/// 4. `exec` the user's interactive login shell.
///
/// Step 3 restores the login banner: PAM caches the dynamic MOTD to
/// `/run/motd.dynamic` (Ubuntu) plus static `/etc/motd`; we print both, guarded so
/// missing files are skipped. Absent on non-Ubuntu → simply nothing printed.
///
/// bash `printf` expands `\x1b...\x1b\\` (ESC ... ST) at runtime. This is
/// bash-oriented (`export -f`/`PROMPT_COMMAND`); zsh and other shells simply
/// won't emit OSC 7 (harmless). A `.bashrc` that overwrites `PROMPT_COMMAND`
/// would defeat it. Disable via `SshConfig::shell_integration = false`.
const SHELL_INTEGRATION_EXEC: &str = r#"__oneterm_osc7() { printf '\x1b]7;file://%s%s\x1b\\' "${HOSTNAME:-$(hostname)}" "$PWD"; printf '\x1b]133;A\x1b\\'; }; export -f __oneterm_osc7 2>/dev/null; export PROMPT_COMMAND='__oneterm_osc7'; [ -f /run/motd.dynamic ] && cat /run/motd.dynamic 2>/dev/null; [ -r /etc/motd ] && cat /etc/motd 2>/dev/null; exec "${SHELL:-/bin/bash}" -il"#;

/// Load a private key from a file, decrypting with the passphrase if needed.
fn load_private_key(
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> anyhow::Result<PrivateKey> {
    let key = load_secret_key(path, passphrase)
        .map_err(|e| anyhow::anyhow!("Failed to load key {}: {e}", path.display()))?;
    Ok(key)
}
