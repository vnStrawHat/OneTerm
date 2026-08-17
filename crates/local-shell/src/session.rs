//! `LocalSession` — spawn a local shell via `alacritty_terminal::tty` +
//! `EventLoop` (ConPTY on Windows).
//!
//! #11: spawn + struct + inherent methods. #12: `impl TerminalSession`
//! (mouse/selection/wheel + IME + cursor_bounds). See
//! `docs/terminal-backend.md` §6.2 + freya `handle.rs`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{Options, Shell};
use async_channel::Receiver;

use oneterm_core::config::resolve_shell;
use oneterm_core::{AppError, LocalShellConfig, home_dir};
use oneterm_terminal::{PtySize, SessionEvent, TerminalError};

use crate::event_loop::ShellEventLoop;
use crate::listener::LocalListener;
use crate::state::{SharedState, new_shared};

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

/// A local shell session.
pub struct LocalSession {
    pub(crate) term: Arc<FairMutex<Term<LocalListener>>>,
    pub(crate) listener: LocalListener,
    pub(crate) event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    pub(crate) state: SharedState,
    pub(crate) config: LocalShellConfig,
    /// Pixel cell metrics (set by the UI via `set_cell_size`) — for `cursor_bounds`.
    pub(crate) cell_width: Mutex<f32>,
    pub(crate) line_height: Mutex<f32>,
    /// IME marked text (compose buffer).
    pub(crate) marked_text: Mutex<Option<String>>,
    owner_join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl LocalSession {
    /// Spawn a shell from `cfg` with the initial size `initial`.
    pub fn spawn(
        cfg: LocalShellConfig,
        initial: PtySize,
        scrollback_history: usize,
    ) -> Result<Self, AppError> {
        let resolved = resolve_shell(&cfg)?;
        let opts = Options {
            shell: Some(Shell::new(
                program_argument(&resolved.program),
                resolved.args,
            )),
            working_directory: cfg.cwd.clone().or_else(home_dir),
            drain_on_exit: false,
            env: resolved.env,
            // Escape every argument with the C-runtime rules so user-supplied
            // `Custom` args and the PowerShell `-Command` payload survive
            // `CreateProcessW` command-line re-parsing. The default `cmd /K chcp
            // 65001 >nul` tokens contain no whitespace or quotes, so escaping
            // leaves them verbatim for cmd.exe's own (non-CRT) `/K` parsing.
            #[cfg(windows)]
            escape_args: true,
        };
        let winsize = WindowSize {
            num_lines: initial.rows,
            num_cols: initial.cols,
            cell_width: 0,
            cell_height: 0,
        };

        let state = new_shared();
        state.lock().unwrap().alive = true;

        let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(4096);
        let listener = LocalListener::new(event_tx, state.clone());

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

        let (_notifier, owner_join) = ShellEventLoop::spawn_owned(
            opts,
            winsize,
            term.clone(),
            listener.clone(),
            state.clone(),
        )
        .map_err(|e| AppError::msg(e.to_string()))?;

        // Shell integration is injected via env vars in resolve_shell()
        // — fully silent, no temp file, no script written to the PTY.
        // See crates/core/src/config/shell.rs::resolve_shell().

        Ok(Self {
            term,
            listener,
            event_rx: Mutex::new(Some(event_rx)),
            state,
            config: cfg,
            cell_width: Mutex::new(0.0),
            line_height: Mutex::new(0.0),
            marked_text: Mutex::new(None),
            owner_join: Mutex::new(Some(owner_join)),
        })
    }

    /// UI sets pixel cell metrics (after measuring the font) for `cursor_bounds`.
    pub fn set_cell_size(&self, cell_width: f32, line_height: f32) {
        *self.cell_width.lock().unwrap() = cell_width;
        *self.line_height.lock().unwrap() = line_height;
    }

    /// The config this session was spawned with.
    pub fn config(&self) -> &LocalShellConfig {
        &self.config
    }

    // ── Helpers ──────────────────────────────────────────────────────

    /// Get a `TerminalModel` adapter for the shared terminal-model operations.
    /// Cheap to create — just wraps the existing `Arc<FairMutex<Term>>`.
    pub(crate) fn model(&self) -> oneterm_terminal::model::TerminalModel<LocalListener> {
        oneterm_terminal::model::TerminalModel::new(self.term.clone())
    }

    /// Shut down and join the dedicated PTY owner thread.
    pub(crate) fn shutdown_owner(&self) -> Result<(), TerminalError> {
        let result = self.listener.pty_shutdown();
        let join_result = self
            .owner_join
            .lock()
            .unwrap()
            .take()
            .map(|join| join.join());
        if let Err(error) = join_result.unwrap_or(Ok(())) {
            return Err(TerminalError::Transport(format!(
                "PTY owner thread panicked: {error:?}"
            )));
        }
        result
    }
}

/// The program string handed to alacritty for `resolved.program`.
///
/// On Windows alacritty joins the program and its arguments into one
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
/// consumers use (the same rules alacritty applies to arguments): wrap in double
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
    fn drop(&mut self) {
        let _ = self.listener.pty_shutdown();
        if let Some(join) = self.owner_join.get_mut().unwrap().take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;
