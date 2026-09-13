//! The shared terminal: the engine, the lock around it, and the render-demand
//! flag that keeps the renderer from starving behind a busy pump.
//!
//! `oneterm-vt` holds no lock and no interior mutability: it takes `&mut self`
//! and the embedder picks the synchronisation
//! (`docs/spec-intakes/IN-0029-vt-engine/high-level-design.md`, "Threading and
//! locking"). That embedder is this crate, and the choice is
//! `parking_lot::FairMutex` — already in `Cargo.lock`, and the one primitive
//! `alacritty_terminal::sync::FairMutex` offered on top of it (`lease()`) never
//! had a call site in OneTerm (`research/api-surface.md` § 3.9).
//!
//! Fairness alone is not enough under sustained output: a pump thread that
//! unlocks and immediately relocks still beats a sleeping waiter. So the render
//! path raises a demand and the read loop tests it at a chunk boundary
//! — **after** that batch's reply bytes have left, never before
//! (`damage-and-render-state.md` § "Fairness and reply latency", R-37). The
//! engine owns the flag type and nothing else about it: the policy below is the
//! adapter's, and the policy is that **the waiter clears its own demand, on
//! acquisition** — see [`TerminalHandle::lock_for_render`].

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use oneterm_vt::grid::{DEFAULT_SCROLLBACK, SCROLLBACK_MAX, Size};
use oneterm_vt::{Config, Demand, OscClaims, Terminal};
use parking_lot::{FairMutex, FairMutexGuard};

use crate::backend::GridSize;

/// The terminal both backends and the UI share.
pub type SharedTerminal = Arc<TerminalHandle>;

/// The engine, and the one thing the adapter still adds to it.
///
/// `US-0081` needed this wrapper to carry the render state as well; that moved
/// into `TerminalContent`, where the watermark belongs, and what is left is a
/// single no-op the local read loop calls. It is `US-0083`'s to delete, and this
/// type goes with it.
#[derive(Debug)]
pub struct Engine(Terminal);

impl Engine {
    /// A terminal at `size` with `scrollback` history rows.
    pub fn new(size: GridSize, scrollback: usize) -> Engine {
        Engine(Terminal::new(size.into(), adapter_config(scrollback)))
    }

    /// The child is gone.
    ///
    /// The reference's `Term::exit()` cleared an `is_alive` flag and emitted
    /// `Event::Exit`, which the router has always ignored; liveness is
    /// `SharedSessionState::alive` and the pump publishes the exit itself.
    // ponytail: a no-op that exists only so `crates/local-shell`'s read loop —
    // `US-0083`'s — is not edited by this packet. Upgrade path: delete the call
    // site and this type with it.
    pub fn exit(&mut self) {}
}

impl Deref for Engine {
    type Target = Terminal;

    fn deref(&self) -> &Terminal {
        &self.0
    }
}

impl DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut Terminal {
        &mut self.0
    }
}

/// The engine behind its lock, plus the render-demand handshake.
#[derive(Debug)]
pub struct TerminalHandle {
    engine: FairMutex<Engine>,
    demand: Demand,
}

impl TerminalHandle {
    /// A terminal at `size` with `scrollback` history rows.
    pub fn new(size: GridSize, scrollback: usize) -> TerminalHandle {
        TerminalHandle {
            engine: FairMutex::new(Engine::new(size, scrollback)),
            demand: Demand::new(),
        }
    }

    /// Acquire for a short read or for a parse batch.
    pub fn lock(&self) -> FairMutexGuard<'_, Engine> {
        self.engine.lock()
    }

    /// `None` when someone else holds it.
    pub fn try_lock(&self) -> Option<FairMutexGuard<'_, Engine>> {
        self.engine.try_lock()
    }

    /// Acquire for a frame: raise the demand first, so a pump that is mid-burst
    /// gives the lock up at its next chunk boundary instead of at its next
    /// natural pause, and drop it again once the frame is actually in.
    ///
    /// **The demand is released here, by the waiter, never by the pump's ask**
    /// (`US-0082` rework, `US-0083` gap 6): raising a one-shot flag and *then*
    /// blocking leaves a window in which a pump's `take_render_demand()`
    /// consumes the only signal this frame had, after which the pump sees an
    /// idle engine and keeps it until the transport runs dry. Holding the
    /// demand across the acquire closes the window — the frame is either
    /// waiting and visible, or in.
    ///
    /// Only the snapshot path calls this. `query_state` and `terminal_info` run
    /// on the same thread and are O(1) under the lock, so making them raise the
    /// flag would ask the pump to yield several times per frame for reads that
    /// never wait.
    pub fn lock_for_render(&self) -> FairMutexGuard<'_, Engine> {
        self.demand.raise();
        let engine = self.engine.lock();
        self.demand.release();
        engine
    }

    /// The pump's half of the handshake: "is a frame waiting for me?". A read
    /// loop calls it at a chunk boundary, after the batch's replies have been
    /// written, and drops its guard when it answers `true`.
    ///
    /// The name is the pump's verb, kept so the two backend loops read the way
    /// `US-0083` / `US-0084` wrote them; the asking takes nothing away. A
    /// standing `true` means a frame is still outside the lock, so a pump that
    /// yields again is right to.
    pub fn take_render_demand(&self) -> bool {
        self.demand.is_raised()
    }

    /// The same read, spelled for diagnostics and tests.
    pub fn render_demand_raised(&self) -> bool {
        self.demand.is_raised()
    }

    /// The flag itself, for a loop that wants to hold it across iterations.
    pub fn demand(&self) -> &Demand {
        &self.demand
    }

    // ── Kept for the backends' current read loops ────────────────────────
    //
    // `parking_lot`'s fairness lives in `unlock`, so there is no unfair acquire
    // to call and both of these are the plain ones. The call sites are
    // `crates/local-shell/src/event_loop.rs` and `crates/ssh/src/task.rs`, which
    // `US-0083` / `US-0084` rewrite onto `take_render_demand`; the upgrade path
    // is to delete these two methods with those call sites.

    /// See [`TerminalHandle::lock`].
    pub fn lock_unfair(&self) -> FairMutexGuard<'_, Engine> {
        self.engine.lock()
    }

    /// See [`TerminalHandle::try_lock`].
    pub fn try_lock_unfair(&self) -> Option<FairMutexGuard<'_, Engine>> {
        self.engine.try_lock()
    }
}

/// The shared terminal both backends construct.
pub fn new_shared_terminal(size: GridSize, scrollback: usize) -> SharedTerminal {
    Arc::new(TerminalHandle::new(size, scrollback))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> TerminalHandle {
        TerminalHandle::new(GridSize { cols: 20, lines: 4 }, DEFAULT_SCROLLBACK_LINES)
    }

    /// The half of the handshake this packet owns: a frame that is not waiting
    /// leaves nothing standing, so an idle pump is never asked to yield.
    #[test]
    fn an_uncontended_frame_leaves_no_demand_behind() {
        let handle = handle();
        assert!(!handle.render_demand_raised());

        drop(handle.lock_for_render());
        assert!(
            !handle.take_render_demand(),
            "the frame was never blocked, so nothing is waiting for the pump"
        );
    }

    /// The rework's property, and the race that made it necessary
    /// (`US-0083` gap 6): `lock_for_render` raises the demand and *then* blocks,
    /// so a pump asking inside that window used to consume the frame's only
    /// signal — every later ask said "nobody is waiting" and the pump kept the
    /// engine until the transport ran dry (measured > 5 s, 3/3).
    ///
    /// Deterministic because the pump holds the engine throughout: once the
    /// raise is visible, the frame provably has not acquired anything, so every
    /// ask in the loop below falls inside the window. On the one-shot flag this
    /// fails on the second iteration.
    #[test]
    fn a_pumps_ask_does_not_consume_a_frame_that_is_still_waiting() {
        let handle = Arc::new(handle());
        let pump = handle.lock();

        let (in_tx, in_rx) = std::sync::mpsc::channel();
        let frame = {
            let handle = Arc::clone(&handle);
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let guard = handle.lock_for_render();
                let waited = started.elapsed();
                drop(guard);
                in_tx.send(waited)
            })
        };

        // The raise has landed and the engine is ours: the frame is inside the
        // window, by construction.
        while !handle.render_demand_raised() {
            std::thread::yield_now();
        }
        for ask in 0..100 {
            assert!(
                handle.take_render_demand(),
                "ask {ask} lost a frame that is still waiting for the engine"
            );
        }

        // The pump's yield: one batch boundary, one unlock.
        drop(pump);
        let waited = in_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the frame never reached the engine after the pump yielded");
        frame.join().unwrap().expect("the frame thread reports");

        assert!(
            waited < std::time::Duration::from_secs(5),
            "the frame waited {waited:?} for one batch boundary"
        );
        assert!(
            !handle.take_render_demand(),
            "the demand outlived the frame that raised it"
        );
    }

    /// A pump lock must not ask itself to yield.
    #[test]
    fn a_pump_lock_does_not_raise_the_demand() {
        let handle = handle();
        drop(handle.lock());
        drop(handle.lock_unfair());
        assert!(!handle.take_render_demand());
    }

    /// The demand is a hand-off between threads, so it has to survive one: the
    /// pump asking on this thread sees a frame that raised on another.
    #[test]
    fn the_demand_crosses_threads() {
        let handle = Arc::new(handle());
        let pump = handle.lock();
        let render = {
            let handle = Arc::clone(&handle);
            std::thread::spawn(move || drop(handle.lock_for_render()))
        };
        while !handle.take_render_demand() {
            std::thread::yield_now();
        }
        drop(pump);
        render.join().unwrap();
        assert!(!handle.render_demand_raised(), "the frame cleared its own");
    }

    #[test]
    fn try_lock_reports_a_held_lock() {
        let handle = handle();
        let guard = handle.lock();
        assert!(handle.try_lock_unfair().is_none());
        drop(guard);
        assert!(handle.try_lock_unfair().is_some());
    }

    /// The adapter claims the three OSC numbers it interprets itself; without
    /// them the agent channel, the cwd tracker and shell integration go silent.
    #[test]
    fn the_adapter_claims_the_osc_numbers_it_routes() {
        let config = adapter_config(DEFAULT_SCROLLBACK_LINES);
        for code in [7, 9, 133, 52] {
            assert!(
                config.osc_claims.is_claimed(code),
                "OSC {code} is not claimed"
            );
        }
        assert!(!config.osc_claims.is_claimed(8), "OSC 8 is the engine's");
        assert!(
            config.osc_claims.allows_large(52),
            "OSC 52 needs the large payload ceiling"
        );
    }

    #[test]
    fn the_scrollback_limit_is_clamped_to_the_engine_maximum() {
        let config = adapter_config(usize::MAX);
        assert_eq!(config.scrollback_limit, SCROLLBACK_MAX);
    }
}
