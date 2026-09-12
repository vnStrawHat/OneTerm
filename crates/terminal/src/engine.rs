//! The shared terminal: the engine plus the render state that produces today's
//! snapshot from it.
//!
//! `oneterm-vt` is `Send` and **not** `Sync`, holds no lock and takes
//! `&mut self`, so the synchronisation is the embedder's
//! (`docs/spec-intakes/IN-0029-vt-engine/high-level-design.md`, "Threading and
//! locking"). Both backends therefore share one
//! [`SharedTerminal`] = `Arc<FairMutex<Engine>>`, exactly the shape they shared
//! `Arc<FairMutex<Term<EP>>>` in before.
//!
//! [`LegacySnapshot`] lives **inside** the lock rather than beside it for one
//! reason: it owns the render watermark, and `TerminalModel` is rebuilt on every
//! call (`impl_pty_terminal_session!`), so it has nowhere else to live. It is
//! also where the render state belongs — `render_update` runs under the lock by
//! design.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use oneterm_vt::grid::{DEFAULT_SCROLLBACK, SCROLLBACK_MAX, Size};
use oneterm_vt::{Config, OscClaims, Terminal};

use crate::backend::GridSize;
use crate::content::TerminalContent;
use crate::engine_shim::LegacySnapshot;
use crate::sync::FairMutex;

/// The terminal both backends and the UI share.
pub type SharedTerminal = Arc<FairMutex<Engine>>;

/// The engine plus its legacy render state.
#[derive(Debug)]
pub struct Engine {
    terminal: Terminal,
    snapshot: LegacySnapshot,
}

impl Engine {
    /// A terminal at `size` with `scrollback` history rows.
    pub fn new(size: GridSize, scrollback: usize) -> Engine {
        Engine {
            terminal: Terminal::new(size.into(), adapter_config(scrollback)),
            snapshot: LegacySnapshot::new(),
        }
    }

    /// Rebuild `out` from the engine, consuming this consumer's damage.
    pub fn refill(&mut self, out: &mut TerminalContent) {
        self.snapshot.refill(&mut self.terminal, out);
    }

    /// Force the next [`Engine::refill`] to report full damage.
    pub fn invalidate_render(&mut self) {
        self.snapshot.invalidate();
    }

    /// The child is gone.
    ///
    /// The reference's `Term::exit()` cleared an `is_alive` flag and emitted
    /// `Event::Exit`, which `OscRouter` has always ignored; liveness is
    /// `SharedSessionState::alive` and the pump publishes the exit itself. Kept
    /// as a no-op so `crates/local-shell`'s read loop — which `US-0083` owns —
    /// is not touched by this packet.
    pub fn exit(&mut self) {}
}

impl Deref for Engine {
    type Target = Terminal;

    fn deref(&self) -> &Terminal {
        &self.terminal
    }
}

impl DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }
}

/// The shared terminal both backends construct.
pub fn new_shared_terminal(size: GridSize, scrollback: usize) -> SharedTerminal {
    Arc::new(FairMutex::new(Engine::new(size, scrollback)))
}

/// The engine configuration the adapter needs.
///
/// `OscClaims` is the extension point that replaces the fork's `report_osc`
/// patch: the engine forwards only what is claimed here, and `crates/terminal`
/// claims exactly the three numbers it interprets itself — OSC 7 (cwd), OSC 9
/// (notification, `9;4` progress, `9;7` agent status) and OSC 133 (shell
/// integration). Everything else the engine either handles natively (title,
/// colours, hyperlinks, clipboard) or drops and counts, which is what the
/// engine being replaced did.
fn adapter_config(scrollback: usize) -> Config {
    let mut claims = OscClaims::new();
    claims.claim(7).claim(9).claim(133);
    // A memory ceiling, not a policy: who may write the clipboard stays in
    // `security_policy.rs`. Without it a legitimate large OSC 52 write is
    // truncated at the 2 KiB inline cap.
    claims.claim_large(52);
    Config {
        scrollback_limit: scrollback.min(SCROLLBACK_MAX as usize) as u32,
        osc_claims: claims,
        ..Config::default()
    }
}

impl From<GridSize> for Size {
    fn from(size: GridSize) -> Size {
        Size {
            rows: size.lines.min(usize::from(u16::MAX)) as u16,
            cols: size.cols.min(usize::from(u16::MAX)) as u16,
        }
    }
}

/// The shipped default, so a caller that has no configured value agrees with the
/// engine.
pub const DEFAULT_SCROLLBACK_LINES: usize = DEFAULT_SCROLLBACK as usize;
