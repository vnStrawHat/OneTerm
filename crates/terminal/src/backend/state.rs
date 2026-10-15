//! Session state cache shared between the pump (writer) and the
//! `TerminalSession` accessors (reader).
//!
//! The engine owns the grid, not the session: title, cwd, clipboard and the
//! OSC 133 counters are the embedder's, so the router caches them here. Hot-path
//! counters (alive, rx/tx bytes, absolute line count, clear epoch) are atomics
//! so a parse batch never takes the mutex (PERF-20); the rarely written fields
//! live behind one `Mutex`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use oneterm_vt::Rgb;

use crate::session::NetStats;

/// Theme defaults used to answer OSC 10/11/12/4 queries for colours the
/// program never set. Written by the UI through `set_default_colors`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefaultColors {
    /// Default foreground (OSC 10).
    pub foreground: Option<Rgb>,
    /// Default background (OSC 11).
    pub background: Option<Rgb>,
    /// Default cursor colour (OSC 12).
    pub cursor: Option<Rgb>,
    /// Default 16-colour ANSI palette (OSC 4 indices 0-15).
    pub ansi: Option<[Rgb; 16]>,
}

impl DefaultColors {
    /// The four colours `TerminalRender::set_default_colors` carries.
    pub fn new(foreground: Rgb, background: Rgb, cursor: Rgb, ansi: [Rgb; 16]) -> DefaultColors {
        DefaultColors {
            foreground: Some(foreground),
            background: Some(background),
            cursor: Some(cursor),
            ansi: Some(ansi),
        }
    }
}

/// Mutex-guarded part of the session state (rarely written).
#[derive(Debug, Default)]
pub struct SessionState {
    /// Title (OSC 0/2). `None` = reset/default.
    pub title: Option<String>,
    /// Working directory (OSC 7).
    pub cwd: Option<PathBuf>,
    /// Last clipboard value stored via OSC 52.
    pub clipboard: Option<String>,
    /// Exit code once the process exited.
    pub exit_code: Option<i32>,
    /// Number of prompt markers seen (OSC 133;A).
    pub prompt_count: usize,
    /// Exit code of the last command (OSC 133;D;exit_code).
    pub last_exit_code: Option<i32>,
    /// Theme defaults for colour queries.
    pub default_colors: DefaultColors,
}

/// Arc-shared session state: atomics for the pump hot path, a mutex for the rest.
#[derive(Debug, Default)]
pub struct SharedSessionState {
    inner: Mutex<SessionState>,
    alive: AtomicBool,
    rx_bytes: AtomicU64,
    tx_bytes: AtomicU64,
    absolute_line_count: AtomicUsize,
    clear_epoch: AtomicUsize,
    /// A repaint hint (`SessionEvent::Output`) is out and the UI has not caught
    /// up with it yet (`US-0147`). See [`SharedSessionState::claim_repaint_hint`].
    repaint_hint_out: AtomicBool,
}

/// Handle to a [`SharedSessionState`].
pub type SharedState = Arc<SharedSessionState>;

impl SharedSessionState {
    /// Create a state for a session that is starting (`alive == true`).
    pub fn new_alive() -> SharedState {
        let state = Arc::new(Self::default());
        state.set_alive(true);
        state
    }

    /// Lock the mutex-guarded fields. Keep the guard short; never hold it while
    /// taking the `Term` lock (the pump takes them in the opposite order).
    pub(crate) fn lock(&self) -> MutexGuard<'_, SessionState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Whether the child/remote is still running.
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Set the alive flag.
    pub fn set_alive(&self, alive: bool) {
        self.alive.store(alive, Ordering::Release);
    }

    /// Record process exit: exit code first, then `alive = false`.
    pub(crate) fn record_exit(&self, code: Option<i32>) {
        self.lock().exit_code = code;
        self.set_alive(false);
    }

    /// Exit code, when the process has exited with one.
    pub fn exit_code(&self) -> Option<i32> {
        self.lock().exit_code
    }

    /// Current title (OSC 0/2).
    pub fn title(&self) -> Option<String> {
        self.lock().title.clone()
    }

    /// Current working directory (OSC 7).
    pub fn cwd(&self) -> Option<PathBuf> {
        self.lock().cwd.clone()
    }

    /// Last clipboard value stored via OSC 52.
    pub fn clipboard(&self) -> Option<String> {
        self.lock().clipboard.clone()
    }

    /// Prompt markers seen so far (OSC 133;A).
    pub fn prompt_count(&self) -> usize {
        self.lock().prompt_count
    }

    /// Exit code of the most recently completed command block (OSC 133;D).
    pub fn last_exit_code(&self) -> Option<i32> {
        self.lock().last_exit_code
    }

    /// Replace the theme defaults used for colour-query replies.
    pub fn set_default_colors(&self, colors: DefaultColors) {
        self.lock().default_colors = colors;
    }

    /// Theme defaults used for colour-query replies.
    pub(crate) fn default_colors(&self) -> DefaultColors {
        self.lock().default_colors
    }

    /// Count bytes received from the child/remote.
    pub fn add_rx_bytes(&self, bytes: u64) {
        self.rx_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Count bytes sent to the child/remote.
    pub fn add_tx_bytes(&self, bytes: u64) {
        self.tx_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Network counters (SSH exposes them through `TerminalCapabilities`).
    pub fn net_stats(&self) -> NetStats {
        NetStats {
            rx_bytes: self.rx_bytes.load(Ordering::Relaxed),
            tx_bytes: self.tx_bytes.load(Ordering::Relaxed),
        }
    }

    /// Absolute lines output since spawn (`Terminal::lines_produced`).
    pub fn absolute_line_count(&self) -> usize {
        self.absolute_line_count.load(Ordering::Relaxed)
    }

    /// Publish the absolute line count after a parse batch.
    pub(crate) fn set_absolute_line_count(&self, count: usize) {
        self.absolute_line_count.store(count, Ordering::Relaxed);
    }

    /// Times the screen was cleared (`CSI 2J/3J`, RIS).
    pub fn clear_epoch(&self) -> usize {
        self.clear_epoch.load(Ordering::Relaxed)
    }

    /// Record one screen clear.
    pub(crate) fn bump_clear_epoch(&self) {
        self.clear_epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// May the pump post a repaint hint? `true` once per [`release_repaint_hint`],
    /// so at most one hint is ever out.
    ///
    /// A hint only asks the UI for a frame, and a frame reads everything fed up to
    /// its snapshot; a second hint before that snapshot asks for the same frame. Under
    /// line-at-a-time output (ConPTY hands over one line per chunk) posting one per
    /// chunk woke the UI thread once per line and cost the PTY owner two thirds of
    /// its per-chunk time (`US-0147`).
    ///
    /// Call it after the batch was fed and the engine lock released.
    ///
    /// [`release_repaint_hint`]: SharedSessionState::release_repaint_hint
    pub(crate) fn claim_repaint_hint(&self) -> bool {
        !self.repaint_hint_out.swap(true, Ordering::AcqRel)
    }

    /// The UI has caught up with the output so far, so the next batch posts a hint
    /// again. Called **before** a render snapshot takes the engine lock (so a batch
    /// fed after it posts, and one fed before it is in the snapshot), by the view
    /// when a hint's frame did not come (a hidden tab), and by the pump when the
    /// hint could not be queued.
    pub fn release_repaint_hint(&self) {
        self.repaint_hint_out.store(false, Ordering::Release);
    }
}
