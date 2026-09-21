//! `LocalSession` — spawn a local shell via `oneterm_vt::pty` on a dedicated PTY
//! owner thread (ConPTY on Windows).
//!
//! This file holds the spawn path, the struct, and its inherent helpers. The
//! session handed to the UI is `oneterm_terminal::PtySession`, which owns a
//! `LocalSession` as its `PtyOwner` — that half lives in `session_terminal.rs`.
//! See `docs/terminal-backend.md` §6.2.

use std::path::Path;
use std::sync::Mutex;

use oneterm_vt::pty::{Options, Shell, WindowSize};

use oneterm_core::config::resolve_shell;
use oneterm_core::{AppError, LocalShellConfig, TerminalLogConfig, home_dir};
use oneterm_terminal::{
    ClipboardOrigin, GridSize, OscRouter, PtySession, PtySize, PtyTransport, SessionEvent,
    SessionEventSink, SessionKind, SharedSessionState, TerminalError, TerminalSecurityPolicy,
    new_shared_terminal,
};

use crate::event_loop::ShellEventLoop;
use crate::session_terminal::local_resize_policy;
use crate::transport::{LocalListener, LocalTransport};

/// The PTY half of a local shell session: the transport and the owner thread to
/// reap. `PtySession` owns one of these and everything else a terminal session
/// does (`session_terminal.rs`).
pub struct LocalSession {
    pub(crate) listener: LocalListener,
    owner_join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl LocalSession {
    /// Spawn a shell from `cfg` with the initial size `initial`. `security` is
    /// the user's policy for terminal-controlled side effects (SEC-08).
    pub fn spawn(
        cfg: LocalShellConfig,
        initial: PtySize,
        scrollback_history: usize,
        security: TerminalSecurityPolicy,
        logging: TerminalLogConfig,
    ) -> Result<PtySession<Self>, AppError> {
        let resolved = resolve_shell(&cfg)?;
        // Kept for the spawn-failure error and logging identity: `resolved` is moved into `Options`.
        let program = resolved.program.clone();
        let program_display = resolved.program.display().to_string();
        let opts = Options {
            shell: Some(Shell::new(
                program_argument(&resolved.program),
                resolved.args,
            )),
            // From `resolved`, never from `cfg`: `resolve_shell` is where the
            // elevation guard lives, and a spawner that read one more field off
            // the original config would walk straight around it (`IN-0043`).
            working_directory: resolved.cwd.or_else(home_dir),
            env: resolved.env,
            // The engine measures cluster widths with `wcswidth`, so the console
            // host is asked for the same rule.
            glyph_width: oneterm_vt::pty::GlyphWidth::WcsWidth,
            #[cfg(unix)]
            child_signal_mask: None,
            // Escape every argument with the C-runtime rules so user-supplied
            // `Custom` args and the PowerShell `-Command` payload survive
            // `CreateProcessW` command-line re-parsing. The default `cmd /K chcp
            // 65001 >nul` tokens contain no whitespace or quotes, so escaping
            // leaves them verbatim for cmd.exe's own (non-CRT) `/K` parsing.
            #[cfg(windows)]
            escape_args: true,
        };
        let winsize = WindowSize {
            rows: initial.rows,
            cols: initial.cols,
            cell_width: 0,
            cell_height: 0,
        };

        let state = SharedSessionState::new_alive();

        let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(4096);
        let listener = OscRouter::with_security(
            LocalTransport::new(),
            SessionEventSink::new(event_tx),
            state.clone(),
            ClipboardOrigin::Local,
            security,
        );

        let size = GridSize {
            cols: initial.cols as usize,
            lines: initial.rows as usize,
        };
        let term = new_shared_terminal(size, scrollback_history);

        let (_notifier, owner_join) = ShellEventLoop::spawn_owned(
            opts,
            winsize,
            term.clone(),
            listener.clone(),
            program,
            logging,
        )
        .map_err(|error| AppError::ShellResolution {
            shell: program_display,
            reason: error.to_string(),
        })?;

        // Shell integration is injected via env vars in resolve_shell()
        // — fully silent, no temp file, no script written to the PTY.
        // See crates/core/src/config/shell.rs::resolve_shell().

        Ok(PtySession::new(
            term,
            state,
            event_rx,
            SessionKind::Local,
            local_resize_policy(),
            Self {
                listener,
                owner_join: Mutex::new(Some(owner_join)),
            },
        ))
    }

    // ── Helpers ──────────────────────────────────────────────────────

    /// The PTY transport (write / resize / shutdown).
    pub(crate) fn transport(&self) -> &LocalTransport {
        self.listener.transport()
    }

    /// Ask the PTY owner thread to shut down without waiting for it.
    ///
    /// The owner loop may be parked in `poll.wait` or blocked on the `Term`
    /// lock, so joining it here would stall the UI thread (and can deadlock
    /// with a pump that waits for the UI to drain events — CORR-10). The join
    /// handle is handed to a detached reaper thread that reports a panicked
    /// owner; the shutdown request itself is delivered synchronously.
    pub(crate) fn shutdown_owner(&self) -> Result<(), TerminalError> {
        let result = self.transport().pty_close();
        let join = self
            .owner_join
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(join) = join {
            reap_owner_thread(join);
        }
        result
    }
}

/// Join the PTY owner thread off the caller's thread and log a panic.
///
/// If the reaper thread cannot be spawned the handle is dropped, which detaches
/// the owner thread — it still exits on its own once it observes the shutdown
/// flag.
fn reap_owner_thread(join: std::thread::JoinHandle<()>) {
    let spawned = std::thread::Builder::new()
        .name("PTY owner reaper".into())
        .spawn(move || {
            if let Err(payload) = join.join() {
                log::error!("LocalSession: PTY owner thread panicked: {payload:?}");
            }
        });
    if let Err(error) = spawned {
        log::warn!(
            "LocalSession: cannot spawn PTY owner reaper ({error}); detaching the owner thread"
        );
    }
}

/// The program string handed to `oneterm_vt::pty` for `resolved.program`.
///
/// On Windows the transport joins the program and its arguments into one
/// `CreateProcessW` command line with `lpApplicationName = NULL`, so an unquoted
/// path containing spaces (`C:\Program Files\PowerShell\7\pwsh.exe`) is
/// resolved ambiguously (CWE-428). `Options::escape_args` only escapes the
/// arguments, never the program, so quote it here. Unix hands the program to
/// `execvp` verbatim, where added quotes would become part of the file name.
fn program_argument(program: &Path) -> String {
    let program = program.to_string_lossy();
    if cfg!(windows) {
        quote_windows_argument(&program)
    } else {
        program.into_owned()
    }
}

/// Quote one token with the C-runtime command-line rules that `CreateProcessW`
/// consumers use (the same rules `Options::escape_args` applies): wrap in double
/// quotes when the token is empty or contains whitespace, and double the
/// backslashes that precede an embedded or closing quote.
fn quote_windows_argument(token: &str) -> String {
    let needs_quotes = token.is_empty() || token.contains([' ', '\t']);
    let mut quoted = String::with_capacity(token.len() + 2);
    if needs_quotes {
        quoted.push('"');
    }
    let mut backslashes = 0;
    for ch in token.chars() {
        if ch == '\\' {
            backslashes += 1;
        } else {
            if ch == '"' {
                quoted.extend(std::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
        }
        quoted.push(ch);
    }
    if needs_quotes {
        quoted.extend(std::iter::repeat_n('\\', backslashes));
        quoted.push('"');
    }
    quoted
}

impl Drop for LocalSession {
    /// Request shutdown when the session is discarded without `close()`; the
    /// owner thread is reaped off-thread (see [`LocalSession::shutdown_owner`]).
    fn drop(&mut self) {
        if let Err(error) = self.transport().pty_close() {
            // Drop after an explicit `close()` reports `Closed`; nothing to do.
            log::debug!("LocalSession: close on drop not delivered: {error}");
        }
        let join = self
            .owner_join
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(join) = join {
            reap_owner_thread(join);
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;

#[cfg(all(test, windows))]
#[path = "session_orphan_tests.rs"]
mod session_orphan_tests;
