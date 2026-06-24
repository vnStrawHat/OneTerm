//! `LocalSession` - spawn shell cục bộ qua `alacritty_terminal::tty` +
//! `EventLoop` (ConPTY trên Windows).
//!
//! #11: spawn + struct + inherent methods. #12: `impl TerminalSession`
//! (mouse/selection/wheel + IME + cursor_bounds). Tham chiếu
//! `docs/terminal-backend.md` §6.2 + freya `handle.rs`.

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty::{self, Options, Shell};
use async_channel::Receiver;

use myterm2_core::config::resolve_shell;
use myterm2_core::{AppError, LocalShellConfig, SessionEvent};

use crate::event_loop::ShellEventLoop;
use crate::listener::LocalListener;
use crate::state::{SharedState, new_shared};

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

/// Một session shell cục bộ.
pub struct LocalSession {
    pub(crate) term: Arc<FairMutex<Term<LocalListener>>>,
    pub(crate) listener: LocalListener,
    pub(crate) event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    pub(crate) state: SharedState,
    pub(crate) config: LocalShellConfig,
    /// Pixel cell metrics (UI set qua `set_cell_size`) - cho `cursor_bounds`.
    pub(crate) cell_width: Mutex<f32>,
    pub(crate) line_height: Mutex<f32>,
    /// IME marked text (compose buffer).
    pub(crate) marked_text: Mutex<Option<String>>,
}

impl LocalSession {
    /// Spawn shell theo `cfg` với kích thước ban đầu `initial`.
    pub fn spawn(
        cfg: LocalShellConfig,
        initial: PtySize,
        scrollback_history: usize,
    ) -> Result<Self, AppError> {
        let resolved = resolve_shell(&cfg)?;
        let opts = Options {
            shell: Some(Shell::new(
                resolved.program.to_string_lossy().into_owned(),
                resolved.args,
            )),
            working_directory: cfg.cwd.clone(),
            drain_on_exit: false,
            env: resolved.env,
            ..Default::default()
        };
        let winsize = WindowSize {
            num_lines: initial.rows,
            num_cols: initial.cols,
            cell_width: 0,
            cell_height: 0,
        };
        let pty = tty::new(&opts, winsize, 0).map_err(|e| AppError::msg(e.to_string()))?;

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

        let (event_loop, notifier) =
            ShellEventLoop::new(pty, term.clone(), listener.clone(), state.clone())
                .map_err(|e| AppError::msg(e.to_string()))?;
        listener.set_notifier(notifier);
        let _join = event_loop.spawn();

        // Shell integration được inject qua env vars trong resolve_shell()
        // - hoàn toàn silent, không temp file, không viết script ra PTY.
        // Xem crates/core/src/config/shell.rs::resolve_shell().

        Ok(Self {
            term,
            listener,
            event_rx: Mutex::new(Some(event_rx)),
            state,
            config: cfg,
            cell_width: Mutex::new(0.0),
            line_height: Mutex::new(0.0),
            marked_text: Mutex::new(None),
        })
    }

    /// UI set pixel cell metrics (sau khi measure font) cho `cursor_bounds`.
    pub fn set_cell_size(&self, cell_width: f32, line_height: f32) {
        *self.cell_width.lock().unwrap() = cell_width;
        *self.line_height.lock().unwrap() = line_height;
    }

    /// Config đã spawn.
    pub fn config(&self) -> &LocalShellConfig {
        &self.config
    }

    // ── Helpers ──────────────────────────────────────────────────────
    /// Chuyển (row, col) pixel-cell → (Point, Side) để thao tác selection.
    fn point_and_side(term: &Term<LocalListener>, row: f32, col: f32) -> (Point, Side) {
        let col = col.max(0.0);
        let row_idx = (row.max(0.0) as usize).min(term.screen_lines().saturating_sub(1));
        let column = (col as usize).min(term.columns().saturating_sub(1));
        let line = row_idx as i32 - term.grid().display_offset() as i32;
        let side = if col.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (Point::new(Line(line), Column(column)), side)
    }

    pub(crate) fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// Bắt đầu selection (khi không ở mouse mode).
    pub(crate) fn start_selection(&self, row: f32, col: f32, sel: SelectionType) {
        let mut term = self.term.lock();
        let (point, side) = Self::point_and_side(&term, row, col);
        term.selection = Some(Selection::new(sel, point, side));
    }

    /// Cập nhật selection đang có (khi kéo).
    pub(crate) fn update_selection(&self, row: f32, col: f32) {
        let mut term = self.term.lock();
        // Compute point/side (immutable borrow) trước, rồi mới mutate selection.
        let (point, side) = Self::point_and_side(&term, row, col);
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
    }
}
