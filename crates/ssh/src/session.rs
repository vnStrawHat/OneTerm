//! `SshSession` — SSH client over russh + a process-shared Tokio runtime.
//!
//! The public API is sync (uses `block_on` for connect). All sessions share one
//! bounded multi-thread runtime, so connection count does not create one worker
//! thread per tab. Write/resize/close commands and outgoing events cross the
//! sync/async boundary through `async_channel`.
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

use std::borrow::Cow;
use std::cell::Cell;
use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use async_channel::Receiver;
use russh::Pty;
use russh::client;
use russh::client::{AuthResult, KeyboardInteractiveAuthResponse};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, load_secret_key};
use russh::{MethodKind, MethodSet};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, ConnectPhase, ConnectionCancellation, SshAuthMethod, SshConfig};
use oneterm_terminal::{
    ClipboardOrigin, GridSize, OscRouter, PtySize, PtyTransport, SessionEvent, SessionEventSink,
    SharedSessionState, SharedState, TerminalSecurityPolicy,
};

use crate::counting_stream::CountingStream;
use crate::handler::SshClientHandler;
use crate::sftp::{SftpCmd, SftpSession};
use crate::sftp_task::sftp_task;
use crate::task::ssh_main_task;
use crate::transport::{Cmd, SSH_COMMAND_QUEUE_CAPACITY, SshListener, SshTransport};

/// An SSH session whose asynchronous tasks run on the shared SSH runtime.
pub struct SshSession {
    pub(crate) term: Arc<FairMutex<Term<SshListener>>>,
    pub(crate) listener: SshListener,
    pub(crate) event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    pub(crate) state: SharedState,
    pub(crate) marked_text: Mutex<Option<String>>,
    /// SFTP session (None = server does not support SFTP).
    pub(crate) sftp: Mutex<Option<Arc<SftpSession>>>,
}

impl SshSession {
    /// The SSH channel transport (write / resize / close).
    pub(crate) fn transport(&self) -> &SshTransport {
        self.listener.transport()
    }

    /// Ask the SFTP task to stop and drop the handle, so a later `close()` or
    /// drop is a no-op (no-op without SFTP).
    pub(crate) fn close_sftp(&self) {
        let sftp = self
            .sftp
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(sftp) = sftp {
            use oneterm_core::SftpBackend;
            sftp.close();
        }
    }
}

impl Drop for SshSession {
    /// Release the connection when the session is discarded without `close()`
    /// (for example when connect succeeded after the user cancelled).
    ///
    /// `ssh_main_task` holds `cmd_tx` clones through `term`/`listener`, so the
    /// command channel never closes on its own; the transport's closing flag is
    /// the task's shutdown signal. `pty_close` sets it and is idempotent, so an
    /// explicit `close()` followed by drop is fine.
    fn drop(&mut self) {
        if !self.transport().is_closing() {
            log::info!("SshSession: dropped without close — requesting close");
            if let Err(error) = self.transport().pty_close() {
                log::debug!("SshSession: close on drop not delivered: {error}");
            }
        }
        // Best effort: `sftp_task` also stops when the main task ends
        // (ARCH-28), but the UI may still hold an `Arc<SftpSession>`.
        self.close_sftp();
    }
}

const CONNECT_DEADLINE: Duration = Duration::from_secs(60);
const PHASE_DEADLINE: Duration = Duration::from_secs(20);
// Transport-level keepalive so a dead peer or a NAT that dropped the mapping is
// detected instead of leaving the tab hanging forever: one `keepalive@openssh.com`
// request every 30 s, disconnect after 3 unanswered (about 90 s of silence).
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const KEEPALIVE_MAX: usize = 3;
// Disable TTY echo while shell integration bootstraps the running shell.
const SHELL_INTEGRATION_PTY_MODES: &[(Pty, u32)] = &[(Pty::ECHO, 0)];

const SSH_RUNTIME_WORKERS: usize = 2;
static SSH_RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> =
    OnceLock::new();

fn shared_runtime() -> oneterm_core::Result<&'static tokio::runtime::Runtime> {
    match SSH_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(SSH_RUNTIME_WORKERS)
            .enable_all()
            .thread_name("ssh-runtime")
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(runtime) => Ok(runtime),
        Err(error) => Err(AppError::Connect {
            phase: ConnectPhase::Transport,
            message: format!("failed to initialize shared SSH runtime: {error}"),
        }),
    }
}

/// `AppError::Connect` for a transport-level failure in `phase`.
fn phase_error(phase: ConnectPhase, error: impl std::fmt::Display) -> AppError {
    AppError::Connect {
        phase,
        message: error.to_string(),
    }
}

/// Runs each step of a connection attempt under the per-phase deadline and
/// the user's cancellation, remembering which phase is in flight so the
/// overall deadline can name it.
struct ConnectPhases {
    cancellation: ConnectionCancellation,
    current: Cell<ConnectPhase>,
}

impl ConnectPhases {
    fn new(cancellation: ConnectionCancellation) -> Self {
        Self {
            cancellation,
            current: Cell::new(ConnectPhase::Transport),
        }
    }

    /// The phase most recently started.
    fn current(&self) -> ConnectPhase {
        self.current.get()
    }

    async fn run<T, F>(&self, phase: ConnectPhase, future: F) -> oneterm_core::Result<T>
    where
        F: Future<Output = oneterm_core::Result<T>>,
    {
        self.run_with_deadline(phase, future, PHASE_DEADLINE).await
    }

    /// Await `future` as `phase`: a timeout becomes `AppError::Connect`
    /// naming the phase, a user cancellation `AppError::Cancelled` (woken by
    /// `ConnectionCancellation::cancelled`, no polling — PERF-22).
    async fn run_with_deadline<T, F>(
        &self,
        phase: ConnectPhase,
        future: F,
        deadline: Duration,
    ) -> oneterm_core::Result<T>
    where
        F: Future<Output = oneterm_core::Result<T>>,
    {
        self.current.set(phase);
        tokio::select! {
            result = tokio::time::timeout(deadline, future) => {
                result.map_err(|_| AppError::Connect {
                    phase,
                    message: format!("timed out after {} s", deadline.as_secs()),
                })?
            }
            _ = self.cancellation.cancelled() => Err(AppError::Cancelled),
        }
    }
}

/// Connect over SSH to a server. Sync API — uses the shared runtime for connect.
/// The runtime's bounded worker pool keeps all sessions' background tasks running.
/// `security` is the user's policy for terminal-controlled side effects (SEC-08).
pub fn connect(
    mut cfg: SshConfig,
    initial: PtySize,
    scrollback_history: usize,
    security: TerminalSecurityPolicy,
) -> oneterm_core::Result<Box<dyn oneterm_terminal::TerminalSession>> {
    log::info!(
        "SshSession::connect: host={}, port={}, user={}, rows={}, cols={}",
        cfg.host,
        cfg.port,
        cfg.username,
        initial.rows,
        initial.cols
    );

    let runtime = shared_runtime()?;
    let phases = ConnectPhases::new(cfg.cancellation.clone());

    // Input must preserve FIFO ordering without dropping keystrokes when the UI
    // produces a short burst. Control-flow failures remain observable when the
    // receiver closes; tests use bounded transports to exercise saturation.
    let (cmd_tx, cmd_rx) = async_channel::bounded::<Cmd>(SSH_COMMAND_QUEUE_CAPACITY);
    let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(4096);
    let state = SharedSessionState::new_alive();

    // SSH is remote: OSC 52 clipboard reads/writes default off.
    let listener = OscRouter::with_security(
        SshTransport::new(cmd_tx),
        SessionEventSink::new(event_tx),
        state.clone(),
        ClipboardOrigin::Remote,
        security,
    );
    // Cancelled by `ssh_main_task` on exit so the SFTP task dies with the
    // connection (ARCH-28).
    let sftp_shutdown = CancellationToken::new();

    let size = GridSize {
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
        let operation = async {
            let addr = format!("{}:{}", cfg.host, cfg.port);
            log::info!("SshSession: connecting to {addr}");
            let handler =
                SshClientHandler::new(cfg.host.clone(), cfg.port, cfg.host_key_policy.clone());
            let mut client_cfg = russh::client::Config {
                keepalive_interval: Some(KEEPALIVE_INTERVAL),
                keepalive_max: KEEPALIVE_MAX,
                ..Default::default()
            };
            client_cfg.preferred.key = Cow::Owned(handler.preferred_key_algorithms());

            let mut handle = phases
                .run(ConnectPhase::Transport, async {
                    client::connect(Arc::new(client_cfg), addr, handler)
                        .await
                        .map_err(|error| error.to_app_error())
                })
                .await?;
            log::info!("SshSession: TCP connected");

            // ── Authenticate ──────────────────────────────────────────────
            // Move authentication material out of the long-lived config so it is
            // zeroized as soon as authentication completes.
            let auth = std::mem::replace(&mut cfg.auth, SshAuthMethod::None);
            let auth_result = match auth {
                SshAuthMethod::None => {
                    log::info!("SshSession: authenticating with none (no password)");
                    phases
                        .run(ConnectPhase::Authentication, async {
                            handle
                                .authenticate_none(&cfg.username)
                                .await
                                .map_err(|e| phase_error(ConnectPhase::Authentication, e))
                        })
                        .await?
                }
                SshAuthMethod::Password { password } => {
                    log::info!("SshSession: authenticating with password");
                    phases
                        .run(ConnectPhase::Authentication, async {
                            authenticate_with_password(
                                &mut handle,
                                &cfg.username,
                                password.expose_secret(),
                            )
                            .await
                        })
                        .await?
                }
                SshAuthMethod::PrivateKey {
                    key_path,
                    passphrase,
                } => {
                    log::info!("SshSession: authenticating with key {}", key_path.display());
                    // Key files are read and decrypted on the blocking pool so
                    // the two SSH runtime workers keep serving other sessions
                    // (CORR-17).
                    let key = phases
                        .run(ConnectPhase::Authentication, async {
                            tokio::task::spawn_blocking(move || {
                                load_private_key(
                                    &key_path,
                                    passphrase.as_ref().map(|secret| secret.expose_secret()),
                                )
                            })
                            .await
                            .unwrap_or_else(|join_error| {
                                Err(phase_error(ConnectPhase::Authentication, join_error))
                            })
                        })
                        .await?;
                    // RSA keys must not sign with the legacy SHA-1 `ssh-rsa`
                    // (OpenSSH >= 8.8 rejects it). Ask the server which
                    // `rsa-sha2-*` it supports (RFC 8308 `server-sig-algs`);
                    // when it does not say, prefer SHA-512.
                    let hash_alg = if key.algorithm().is_rsa() {
                        let advertised = phases
                            .run(ConnectPhase::Authentication, async {
                                handle
                                    .best_supported_rsa_hash()
                                    .await
                                    .map_err(|e| phase_error(ConnectPhase::Authentication, e))
                            })
                            .await?;
                        rsa_hash_alg(advertised)
                    } else {
                        None
                    };
                    let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);
                    phases
                        .run(ConnectPhase::Authentication, async {
                            handle
                                .authenticate_publickey(&cfg.username, key_with_alg)
                                .await
                                .map_err(|e| phase_error(ConnectPhase::Authentication, e))
                        })
                        .await?
                }
            };
            log::info!("SshSession: auth result = {auth_result:?}");
            if let AuthResult::Failure {
                remaining_methods,
                partial_success,
            } = auth_result
            {
                return Err(AppError::Connect {
                    phase: ConnectPhase::Authentication,
                    message: authentication_failure_message(&remaining_methods, partial_success),
                });
            }

            // ── Open channel + pty + shell ──────────────────────────────
            let channel = phases
                .run(ConnectPhase::ChannelOpen, async {
                    handle
                        .channel_open_session()
                        .await
                        .map_err(|e| phase_error(ConnectPhase::ChannelOpen, e))
                })
                .await?;
            log::info!("SshSession: channel opened");

            let pty_modes: &[(Pty, u32)] = if cfg.shell_integration {
                SHELL_INTEGRATION_PTY_MODES
            } else {
                &[]
            };

            phases
                .run(ConnectPhase::PtyRequest, async {
                    channel
                        .request_pty(
                            false,
                            "xterm-256color",
                            initial.cols as u32,
                            initial.rows as u32,
                            0,
                            0,
                            pty_modes,
                        )
                        .await
                        .map_err(|e| phase_error(ConnectPhase::PtyRequest, e))
                })
                .await?;
            log::info!(
                "SshSession: pty requested ({}x{})",
                initial.cols,
                initial.rows
            );

            // ── Shell integration (OSC 7 cwd) — shell login + bootstrap ──────
            // Keep `request_shell(true)` so sshd/PAM still prints the normal login
            // banner and MOTD / "Last login" output. When enabled, we request the PTY
            // with ECHO off, then inject a bootstrap command that installs the OSC 7
            // prompt hook in the running shell and re-enables echo.
            phases
                .run(ConnectPhase::ShellRequest, async {
                    channel
                        .request_shell(true)
                        .await
                        .map_err(|e| phase_error(ConnectPhase::ShellRequest, e))
                })
                .await?;
            log::info!("SshSession: shell requested");

            if cfg.shell_integration {
                phases
                    .run(ConnectPhase::ShellIntegration, async {
                        send_shell_integration_bootstrap(&channel, &state).await
                    })
                    .await?;
                log::info!("SshSession: shell integration bootstrap sent");
            }

            // ── Open SFTP channel (optional) ────────────────────────────
            // Open it BEFORE spawning ssh_main_task because the handle is moved into
            // the task. The SFTP channel is split into its own object — no handle needed.
            let sftp_session = match phases
                .run(ConnectPhase::SftpSetup, async {
                    open_sftp(&handle, &state, sftp_shutdown.clone()).await
                })
                .await
            {
                Ok(sftp) => {
                    log::info!("SshSession: SFTP channel opened");
                    Some(sftp)
                }
                Err(AppError::Cancelled) => return Err(AppError::Cancelled),
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
                cmd_rx,
                sftp_shutdown,
            ));
            log::info!("SshSession: main task spawned");

            Ok::<_, AppError>(sftp_session)
        };
        tokio::time::timeout(CONNECT_DEADLINE, operation)
            .await
            .unwrap_or_else(|_| {
                Err(AppError::Connect {
                    phase: phases.current(),
                    message: format!(
                        "connection attempt timed out after {} s",
                        CONNECT_DEADLINE.as_secs()
                    ),
                })
            })
    });

    match connect_result {
        Ok(sftp_session) => {
            log::info!("SshSession: connect successful");
            let session = SshSession {
                term,
                listener,
                event_rx: Mutex::new(Some(event_rx)),
                state,
                marked_text: Mutex::new(None),
                sftp: Mutex::new(sftp_session),
            };
            Ok(Box::new(session) as Box<dyn oneterm_terminal::TerminalSession>)
        }
        Err(e) => {
            log::error!("SshSession: connect failed: {e}");
            Err(e)
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
/// 5. Create the command channel `(cmd_tx, cmd_rx)`
/// 6. `tokio::spawn(sftp_task(...))` — runs in the background
/// 7. Return `Arc<SftpSession>` — bridge for the UI to call synchronously
async fn open_sftp(
    handle: &russh::client::Handle<SshClientHandler>,
    state: &SharedState,
    shutdown: CancellationToken,
) -> oneterm_core::Result<Arc<SftpSession>> {
    let sftp_error = |step: &str, error: &dyn std::fmt::Display| {
        phase_error(ConnectPhase::SftpSetup, format!("{step}: {error}"))
    };
    // 1. Open a new channel on the same handle.
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| sftp_error("channel_open_session", &e))?;

    // 2. Request SFTP subsystem.
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| sftp_error("request_subsystem", &e))?;

    // 3. Convert channel → stream (AsyncRead + AsyncWrite).
    //    Wrap with CountingStream to count rx/tx bytes — merged into the same
    //    SharedState as the SSH shell channel → total network traffic.
    let stream = CountingStream::new(channel.into_stream(), state.clone());

    // 4. SFTP handshake — create the SftpChannel.
    let sftp_channel = russh_sftp::client::SftpSession::new(stream)
        .await
        .map_err(|e| sftp_error("handshake", &e))?;

    // 5. Create the command channel bridging sync (UI) → async (tokio task);
    //    `alive` is the task's observable lifecycle state.
    let (sftp_cmd_tx, sftp_cmd_rx) = async_channel::bounded::<SftpCmd>(64);
    let alive = Arc::new(AtomicBool::new(true));

    // 6. Spawn sftp_task — runs in the background on the same tokio runtime.
    tokio::spawn(sftp_task(
        sftp_channel,
        sftp_cmd_rx,
        alive.clone(),
        shutdown,
    ));

    // 7. Return Arc<SftpSession> — the UI holds this handle.
    Ok(SftpSession::new(sftp_cmd_tx, alive))
}

/// Password authentication with a keyboard-interactive fallback.
///
/// Servers configured with `PasswordAuthentication no` but
/// `KbdInteractiveAuthentication yes` (the PAM default on several distributions)
/// reject `password` and advertise `keyboard-interactive` instead. The fallback
/// runs a single round: every prompt of the first info request is answered with
/// the same password; a second info request or a prompt that echoes (so it is
/// not a password prompt) aborts with an explicit error instead of guessing.
async fn authenticate_with_password(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    password: &str,
) -> oneterm_core::Result<AuthResult> {
    let auth_error = |e: russh::Error| phase_error(ConnectPhase::Authentication, e);
    let result = handle
        .authenticate_password(username, password)
        .await
        .map_err(auth_error)?;
    let AuthResult::Failure {
        remaining_methods, ..
    } = &result
    else {
        return Ok(result);
    };
    if !remaining_methods.contains(&MethodKind::KeyboardInteractive) {
        return Ok(result);
    }
    log::info!("SshSession: password rejected; falling back to keyboard-interactive");
    let response = handle
        .authenticate_keyboard_interactive_start(username, None)
        .await
        .map_err(auth_error)?;
    let prompts = match response {
        KeyboardInteractiveAuthResponse::Success => return Ok(AuthResult::Success),
        KeyboardInteractiveAuthResponse::Failure {
            remaining_methods,
            partial_success,
        } => {
            return Ok(AuthResult::Failure {
                remaining_methods,
                partial_success,
            });
        }
        KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => prompts,
    };
    if prompts.iter().any(|prompt| prompt.echo) {
        return Err(phase_error(
            ConnectPhase::Authentication,
            "keyboard-interactive authentication asked for input other than a password; interactive prompts are not supported",
        ));
    }
    let responses = prompts.iter().map(|_| password.to_string()).collect();
    match handle
        .authenticate_keyboard_interactive_respond(responses)
        .await
        .map_err(auth_error)?
    {
        KeyboardInteractiveAuthResponse::Success => Ok(AuthResult::Success),
        KeyboardInteractiveAuthResponse::Failure {
            remaining_methods,
            partial_success,
        } => Ok(AuthResult::Failure {
            remaining_methods,
            partial_success,
        }),
        KeyboardInteractiveAuthResponse::InfoRequest { .. } => Err(phase_error(
            ConnectPhase::Authentication,
            "keyboard-interactive authentication requested a second round of prompts; interactive prompts are not supported",
        )),
    }
}

/// User-facing message for a rejected authentication attempt, naming the
/// methods the server still accepts so a wrong method choice is diagnosable.
fn authentication_failure_message(remaining_methods: &MethodSet, partial_success: bool) -> String {
    let mut message = String::from("rejected by the server");
    if partial_success {
        message.push_str(" (the server accepted this method but requires another one)");
    }
    if remaining_methods.is_empty() {
        return message;
    }
    let methods: Vec<&str> = remaining_methods.iter().map(<&str>::from).collect();
    message.push_str("; the server accepts: ");
    message.push_str(&methods.join(", "));
    message
}

/// Bootstrap command injected after `request_shell(true)` to install the
/// OSC 7 prompt hook in the running shell without showing the script itself.
const SHELL_INTEGRATION_BOOTSTRAP: &str = r#"__oneterm_osc7() { printf '\x1b]7;file://%s%s\x1b\\' "${HOSTNAME:-$(hostname)}" "$PWD"; printf '\x1b]133;A\x1b\\'; }; case ";${PROMPT_COMMAND:-};" in *";__oneterm_osc7;"*) ;; *) PROMPT_COMMAND="__oneterm_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;; esac; __oneterm_osc7; stty echo 2>/dev/null"#;

/// Send the shell-integration bootstrap after the shell is open.
async fn send_shell_integration_bootstrap(
    channel: &russh::Channel<russh::client::Msg>,
    state: &SharedState,
) -> oneterm_core::Result<()> {
    let payload = format!("{SHELL_INTEGRATION_BOOTSTRAP}\r");
    channel
        .data(payload.as_bytes())
        .await
        .map_err(|e| phase_error(ConnectPhase::ShellIntegration, e))?;
    state.add_tx_bytes(payload.len() as u64);
    Ok(())
}

/// Pick the RSA signature hash from the server's `server-sig-algs` answer
/// (`russh::client::Handle::best_supported_rsa_hash`).
///
/// `None` = extension not advertised, `Some(None)` = advertised without any
/// `rsa-sha2-*`. Both fall back to SHA-512 — passing `None` to
/// `PrivateKeyWithHashAlg` would sign with legacy SHA-1 `ssh-rsa`, which modern
/// servers refuse and which is no longer considered safe.
fn rsa_hash_alg(advertised: Option<Option<HashAlg>>) -> Option<HashAlg> {
    advertised.flatten().or(Some(HashAlg::Sha512))
}

/// Load a private key from a file, decrypting with the passphrase if needed.
fn load_private_key(
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> oneterm_core::Result<PrivateKey> {
    load_secret_key(path, passphrase).map_err(|e| {
        phase_error(
            ConnectPhase::Authentication,
            format!("failed to load key {}: {e}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn phase_wait_stops_when_cancelled() {
        let cancellation = ConnectionCancellation::default();
        cancellation.cancel();

        let phases = ConnectPhases::new(cancellation);
        let error = phases
            .run_with_deadline(
                ConnectPhase::ChannelOpen,
                async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok(())
                },
                Duration::from_secs(1),
            )
            .await
            .expect_err("cancelled phase must fail");

        assert!(matches!(error, AppError::Cancelled), "{error}");
    }

    /// PERF-22: a cancellation raised while a phase is in flight wakes the
    /// phase promptly (no 25 ms poll loop) and reports `Cancelled`.
    #[tokio::test]
    async fn phase_wait_wakes_on_late_cancellation() {
        let cancellation = ConnectionCancellation::default();
        let phases = ConnectPhases::new(cancellation.clone());
        let canceller = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                cancellation.cancel();
            }
        });

        let started = std::time::Instant::now();
        let error = phases
            .run_with_deadline(
                ConnectPhase::PtyRequest,
                async {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    Ok(())
                },
                Duration::from_secs(5),
            )
            .await
            .expect_err("cancelled phase must fail");
        canceller.await.expect("canceller task");

        assert!(matches!(error, AppError::Cancelled), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cancellation must not wait for the deadline"
        );
    }

    #[test]
    fn shared_runtime_is_reused_across_many_workers() {
        let runtime = shared_runtime().expect("shared SSH runtime must initialize");
        let expected = runtime as *const tokio::runtime::Runtime as usize;
        let handles: Vec<_> = (0..32)
            .map(|_| {
                std::thread::spawn(|| {
                    let runtime = shared_runtime().expect("shared SSH runtime must exist");
                    runtime.block_on(async { tokio::task::yield_now().await });
                    runtime as *const tokio::runtime::Runtime as usize
                })
            })
            .collect();

        for handle in handles {
            assert_eq!(handle.join().expect("runtime worker must finish"), expected);
        }
    }

    use crate::handler::SshHandlerError;

    /// Deletes a temporary known_hosts file when the test ends, even when an
    /// assertion fails (ERR-15).
    struct TempKnownHosts(std::path::PathBuf);

    impl Drop for TempKnownHosts {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_file(&self.0) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("failed to remove {}: {error}", self.0.display());
                }
            }
        }
    }

    fn detached_session() -> (SshSession, async_channel::Receiver<Cmd>) {
        let (cmd_tx, cmd_rx) = async_channel::bounded::<Cmd>(4);
        let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(4);
        let state = SharedSessionState::new_alive();
        let listener = OscRouter::new(
            SshTransport::new(cmd_tx),
            SessionEventSink::new(event_tx),
            state.clone(),
            ClipboardOrigin::Remote,
        );
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &GridSize {
                cols: 80,
                lines: 24,
            },
            listener.clone(),
        )));
        let session = SshSession {
            term,
            listener,
            event_rx: Mutex::new(Some(event_rx)),
            state,
            marked_text: Mutex::new(None),
            sftp: Mutex::new(None),
        };
        (session, cmd_rx)
    }

    /// CORR-06: a session discarded without `close()` must still tell the task
    /// to shut down (closing flag + `Cmd::Close`).
    #[test]
    fn dropping_an_unclosed_session_requests_close() {
        let (session, cmd_rx) = detached_session();
        let transport = session.transport().clone();
        assert!(!transport.is_closing());

        drop(session);

        assert!(transport.is_closing());
        assert!(matches!(cmd_rx.try_recv(), Ok(Cmd::Close)));
    }

    /// CORR-06: drop after an explicit `close()` does not enqueue a second close.
    #[test]
    fn dropping_a_closed_session_is_idempotent() {
        use oneterm_terminal::TerminalLifecycle;

        let (session, cmd_rx) = detached_session();
        session.close().unwrap();
        assert!(matches!(cmd_rx.try_recv(), Ok(Cmd::Close)));

        drop(session);

        assert!(cmd_rx.try_recv().is_err());
    }

    /// SEC-02: RSA keys never fall back to SHA-1 `ssh-rsa`.
    /// A server that only accepts keyboard-interactive: `password` is rejected
    /// while advertising keyboard-interactive, whose single prompt must be
    /// answered with `secret`.
    #[derive(Clone)]
    struct KeyboardInteractiveServer;

    impl russh::server::Server for KeyboardInteractiveServer {
        type Handler = Self;

        fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
            self.clone()
        }
    }

    impl russh::server::Handler for KeyboardInteractiveServer {
        type Error = russh::Error;

        async fn auth_password(
            &mut self,
            _user: &str,
            _password: &str,
        ) -> Result<russh::server::Auth, Self::Error> {
            Ok(russh::server::Auth::Reject {
                proceed_with_methods: Some(MethodSet::from(&[MethodKind::KeyboardInteractive][..])),
                partial_success: false,
            })
        }

        async fn auth_keyboard_interactive(
            &mut self,
            _user: &str,
            _submethods: &str,
            response: Option<russh::server::Response<'_>>,
        ) -> Result<russh::server::Auth, Self::Error> {
            let Some(mut response) = response else {
                return Ok(russh::server::Auth::Partial {
                    name: "".into(),
                    instructions: "".into(),
                    prompts: vec![("Password: ".into(), false)].into(),
                });
            };
            let answered_correctly = response.next().as_deref() == Some(b"secret".as_slice());
            if answered_correctly {
                Ok(russh::server::Auth::Accept)
            } else {
                Ok(russh::server::Auth::reject())
            }
        }
    }

    async fn spawn_keyboard_interactive_server()
    -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        use russh::server::Server as _;

        let private_key =
            russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        let server_config = Arc::new(russh::server::Config {
            keys: vec![private_key],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let mut server = KeyboardInteractiveServer;
            // The test aborts this task when done; a run error is expected then.
            if let Err(error) = server.run_on_socket(server_config, &listener).await {
                eprintln!("test SSH server stopped: {error}");
            }
        });
        (address, server_task)
    }

    async fn connect_trusting_loopback(
        address: std::net::SocketAddr,
    ) -> (client::Handle<SshClientHandler>, TempKnownHosts) {
        let known_hosts = TempKnownHosts(std::env::temp_dir().join(format!(
            "oneterm-kbd-known-hosts-{}-{}",
            std::process::id(),
            address.port()
        )));
        // Learn the loopback key with a probe connection so the real
        // connection can run under the strict policy without touching the
        // user's known_hosts.
        let probe = client::connect(
            Arc::new(client::Config::default()),
            address,
            SshClientHandler::new(
                address.ip().to_string(),
                address.port(),
                oneterm_core::HostKeyPolicy::Strict,
            )
            .with_known_hosts_path(known_hosts.0.clone()),
        )
        .await;
        let fingerprint = match probe {
            Err(SshHandlerError::UnknownHostKey { fingerprint, .. }) => fingerprint,
            other => panic!("expected an unknown host key, got {:?}", other.err()),
        };
        let handle = client::connect(
            Arc::new(client::Config::default()),
            address,
            SshClientHandler::new(
                address.ip().to_string(),
                address.port(),
                oneterm_core::HostKeyPolicy::AcceptNewFingerprint(fingerprint),
            )
            .with_known_hosts_path(known_hosts.0.clone()),
        )
        .await
        .expect("loopback connect");
        (handle, known_hosts)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn password_falls_back_to_keyboard_interactive() {
        let (address, server_task) = spawn_keyboard_interactive_server().await;
        let (mut handle, _known_hosts) = connect_trusting_loopback(address).await;

        let result = authenticate_with_password(&mut handle, "user", "secret")
            .await
            .expect("keyboard-interactive fallback must not error");
        assert!(matches!(result, AuthResult::Success), "{result:?}");

        drop(handle);
        server_task.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wrong_password_reports_remaining_methods() {
        let (address, server_task) = spawn_keyboard_interactive_server().await;
        let (mut handle, _known_hosts) = connect_trusting_loopback(address).await;

        let result = authenticate_with_password(&mut handle, "user", "wrong")
            .await
            .expect("a rejected password is a result, not an error");
        let AuthResult::Failure {
            remaining_methods, ..
        } = result
        else {
            panic!("wrong password must be rejected");
        };
        assert!(remaining_methods.contains(&MethodKind::KeyboardInteractive));

        drop(handle);
        server_task.abort();
    }

    #[test]
    fn authentication_failure_message_lists_remaining_methods() {
        let none = MethodSet::empty();
        assert_eq!(
            authentication_failure_message(&none, false),
            "rejected by the server"
        );

        let methods =
            MethodSet::from(&[MethodKind::PublicKey, MethodKind::KeyboardInteractive][..]);
        assert_eq!(
            authentication_failure_message(&methods, false),
            "rejected by the server; the server accepts: publickey, keyboard-interactive"
        );
        assert_eq!(
            authentication_failure_message(&methods, true),
            "rejected by the server (the server accepted this method but requires another one); the server accepts: publickey, keyboard-interactive"
        );
    }

    #[test]
    fn rsa_hash_prefers_server_choice_and_never_sha1() {
        assert_eq!(
            rsa_hash_alg(Some(Some(HashAlg::Sha256))),
            Some(HashAlg::Sha256)
        );
        assert_eq!(rsa_hash_alg(None), Some(HashAlg::Sha512));
        assert_eq!(rsa_hash_alg(Some(None)), Some(HashAlg::Sha512));
    }

    #[tokio::test]
    async fn phase_wait_has_a_deadline() {
        let phases = ConnectPhases::new(ConnectionCancellation::default());
        let error = phases
            .run_with_deadline(
                ConnectPhase::ShellRequest,
                async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok(())
                },
                Duration::from_millis(1),
            )
            .await
            .expect_err("expired phase must fail");

        assert!(
            matches!(
                error,
                AppError::Connect {
                    phase: ConnectPhase::ShellRequest,
                    ..
                }
            ),
            "{error}"
        );
        assert_eq!(
            error.to_string(),
            "SSH shell request failed: timed out after 0 s"
        );
        assert_eq!(phases.current(), ConnectPhase::ShellRequest);
    }
}
