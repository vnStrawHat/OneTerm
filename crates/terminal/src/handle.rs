//! The shared terminal: the engine, the lock around it, and the render-demand
//! flag that keeps the renderer from starving behind a busy pump.
//!
//! `oneterm-vt` holds no lock and no interior mutability: it takes `&mut self`
//! and the embedder picks the synchronisation
//! (`docs/spec-intakes/IN-0029-vt-engine/high-level-design.md`, "Threading and
//! locking"). That embedder is this crate, and the choice is
//! `parking_lot::FairMutex` — already in `Cargo.lock`, and the one primitive
//! the fork's own `FairMutex` offered on top of it (`lease()`) never had a
//! call site in OneTerm (`research/api-surface.md` § 3.9).
//!
//! Fairness alone is not enough under sustained output: a pump thread that
//! unlocks and immediately relocks still beats a sleeping waiter. So the render
//! path raises a demand and the read loop tests it at a chunk boundary
//! — **after** that batch's reply bytes have left, never before
//! (`damage-and-render-state.md` § "Fairness and reply latency", R-37). The
//! adapter owns the flag as well as the lock: `Demand` is below, next to the
//! policy that uses it, and the policy is that **the waiter clears its own
//! demand, on acquisition** — see [`TerminalHandle::lock_for_render`].
//!
//! `Demand` itself is `pub(crate)`: raise, release and ask are one protocol, and
//! publishing a piece of it invites the unpaired raise that pins a pump into
//! yielding forever. [`TerminalHandle`] is the whole public door.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use oneterm_vt::grid::{DEFAULT_SCROLLBACK, SCROLLBACK_MAX};
use oneterm_vt::{Config, OscRoute, OscRoutes, Size, Terminal};
use parking_lot::{FairMutex, FairMutexGuard};

use crate::backend::GridSize;
use crate::osc_agent::{AGENT_OSC, LEGACY_AGENT_OSC};

/// The terminal both backends and the UI share.
pub type SharedTerminal = Arc<TerminalHandle>;

/// The renderer's "let me in" flag: how many renderers are waiting for the
/// engine, as a hand-off between the render thread and the pump thread.
///
/// Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
/// section "Fairness and reply latency" (R-37).
///
/// **It is a count of waiters, not a one-shot flag, and the asking does not
/// clear it** (`US-0082` rework, `US-0083` gap 6). A flag cleared by the pump's
/// ask can be consumed in the window between "the renderer raised" and "the
/// renderer is queued on the mutex": the pump then sees nothing waiting, keeps
/// the engine until the transport runs dry, and the frame starves for the whole
/// flood (measured 3/3 at > 5 s). Only the waiter itself knows when it no
/// longer needs the yield, so only the waiter clears — by [`Demand::release`],
/// once it holds the lock.
#[derive(Clone, Debug, Default)]
pub(crate) struct Demand(Arc<AtomicUsize>);

impl Demand {
    pub(crate) fn new() -> Demand {
        Demand::default()
    }

    /// The renderer wants the lock. Call it **before** blocking on the lock,
    /// and pair it with [`Demand::release`] once the lock is held.
    pub(crate) fn raise(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    /// The renderer is in (or has given up): it no longer needs the pump to
    /// yield for it.
    pub(crate) fn release(&self) {
        // Saturating: an unpaired release must not wrap the count into "the
        // whole world is waiting".
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |waiting| {
                waiting.checked_sub(1)
            });
    }

    /// Is anyone waiting? The pump asks at a chunk boundary and yields if so.
    ///
    /// Reading does **not** clear: the demand stands until the waiter that
    /// raised it holds the lock.
    pub(crate) fn is_raised(&self) -> bool {
        self.0.load(Ordering::Acquire) > 0
    }
}

/// The engine behind its lock, plus the render-demand handshake.
#[derive(Debug)]
pub struct TerminalHandle {
    engine: FairMutex<Terminal>,
    demand: Demand,
}

impl TerminalHandle {
    /// A terminal at `size` with `scrollback` history rows.
    pub fn new(size: GridSize, scrollback: usize) -> TerminalHandle {
        TerminalHandle {
            engine: FairMutex::new(Terminal::new(size.into(), adapter_config(scrollback))),
            demand: Demand::new(),
        }
    }

    /// Acquire for a short read or for a parse batch.
    pub fn lock(&self) -> FairMutexGuard<'_, Terminal> {
        self.engine.lock()
    }

    /// `None` when someone else holds it.
    pub fn try_lock(&self) -> Option<FairMutexGuard<'_, Terminal>> {
        self.engine.try_lock()
    }

    /// Acquire for a frame: raise the demand first, so a pump that is mid-burst
    /// gives the lock up at its next chunk boundary instead of at its next
    /// natural pause, and drop it again once the frame is actually in.
    ///
    /// **The demand is released here, by the waiter, never by the pump's ask**
    /// (`US-0082` rework, `US-0083` gap 6): raising a one-shot flag and *then*
    /// blocking leaves a window in which a pump's ask consumes the only signal
    /// this frame had, after which the pump sees an idle engine and keeps it
    /// until the transport runs dry. Holding the demand across the acquire
    /// closes the window — the frame is either waiting and visible, or in.
    ///
    /// Only the snapshot path calls this. `query_state` and `terminal_info` run
    /// on the same thread and are O(1) under the lock, so making them raise the
    /// flag would ask the pump to yield several times per frame for reads that
    /// never wait.
    ///
    /// That premise held everywhere except `terminal_info`'s `last_content_row`,
    /// which scanned the whole viewport on an idle screen; `US-0092` made it
    /// cost the content instead. So the premise is true again, and nothing here
    /// needs to move onto `lock_for_render`.
    pub fn lock_for_render(&self) -> FairMutexGuard<'_, Terminal> {
        self.demand.raise();
        let engine = self.engine.lock();
        self.demand.release();
        engine
    }

    /// The pump's half of the handshake: "is a frame waiting for me?". A read
    /// loop calls it at a chunk boundary, after the batch's replies have been
    /// written, and drops its guard when it answers `true`.
    ///
    /// The asking takes nothing away, which is why the name says `raised` and
    /// not `take_` (`US-0090`): a standing `true` means a frame is still
    /// outside the lock, so a pump that yields again is right to.
    pub fn render_demand_raised(&self) -> bool {
        self.demand.is_raised()
    }

    /// Raise the demand without taking the lock, for a caller that wants one
    /// **standing** across iterations. Nothing releases it but an acquisition
    /// through [`TerminalHandle::lock_for_render`], so the pump yields at every
    /// chunk boundary until then: use `lock_for_render` unless you mean that.
    pub fn raise_render_demand(&self) {
        self.demand.raise();
    }
}

/// The shared terminal both backends construct.
pub fn new_shared_terminal(size: GridSize, scrollback: usize) -> SharedTerminal {
    Arc::new(TerminalHandle::new(size, scrollback))
}

/// The engine configuration the adapter needs.
///
/// Every standard OSC number is the engine's, parsed once and delivered as a
/// typed event. What is left here is the OSC that is OneTerm's rather than a
/// terminal's, and it is three calls:
///
/// * the agent channel (`docs/osc-agent-status.md`) is OneTerm's own proposal,
///   not a terminal standard, so the engine must not know it exists: the number
///   is routed straight out and parsed in `osc_agent`. It sits above the route
///   table's 2048-bit bitmap and lands in its sorted spill list, which is why
///   moving the protocol to a five-digit number needed no engine change;
/// * the channel's deprecated alias shares its number with a built-in, and
///   routing is per number because sub-codes are payload. `BuiltinAndForward`
///   keeps the engine's notification and `9;4` progress handling *and* hands
///   over the raw sequence, so the legacy sub-code stays this crate's
///   knowledge and no engine arm has ever heard of it;
/// * the ceilings. A ceiling is a memory question and not a policy one: who may
///   write the clipboard stays in `security_policy.rs`. Without them a
///   legitimate large OSC 52 write, and every agent payload past ~2040 base64
///   bytes, is truncated at the 2 KiB inline cap — and §3.4 of the spec
///   publishes an 8 KiB allowance to third-party agents. The spill is transient
///   (the parser doubles into it and shrinks back after each OSC) and the
///   8 KiB cap itself is enforced in `osc_agent`.
fn adapter_config(scrollback: usize) -> Config {
    let mut routes = OscRoutes::new();
    routes
        .route(AGENT_OSC, OscRoute::Forward)
        .large(AGENT_OSC, true);
    routes
        .route(LEGACY_AGENT_OSC, OscRoute::BuiltinAndForward)
        .large(LEGACY_AGENT_OSC, true);
    routes.large(52, true);
    Config {
        scrollback_limit: scrollback.min(SCROLLBACK_MAX as usize) as u32,
        osc_routes: routes,
        // The identity `XTVERSION` and `DA2` report is the product's, not the
        // engine's: tmux and vim key capability detection off it. Left unset,
        // the engine would answer `oneterm-vt(...)`.
        product_name: Some(concat!("OneTerm(", env!("CARGO_PKG_VERSION"), ")").into()),
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
            !handle.render_demand_raised(),
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
                handle.render_demand_raised(),
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
            !handle.render_demand_raised(),
            "the demand outlived the frame that raised it"
        );
    }

    /// The `Demand` contract on its own, against an **unfair** mutex — moved
    /// here with the type at `US-0090` (it was
    /// `oneterm_vt::render::tests::pump_yields_to_the_render_demand_…`).
    ///
    /// `TerminalHandle`'s own tests above prove the handshake over
    /// `parking_lot::FairMutex`, where the unlock alone hands the lock to a
    /// waiter. This one removes that help: `std::sync::Mutex` is unfair, so the
    /// only reason the renderer gets in within a bounded number of chunks is
    /// that the pump asks the flag and parks. The payload is deliberately not a
    /// terminal — the subject is the flag, not the engine.
    ///
    /// It is in three steps, and the order is the whole point (`BUG-0070`):
    ///
    /// 1. the frame raises and **waits for the pump to publish the chunk it
    ///    asked at**, so everything after this is provably behind the pump's
    ///    first yield rather than racing it. Every wait here is bounded by one
    ///    deadline, and every assertion runs after the pump is stopped and
    ///    joined — a panic that skips the join leaves a thread spinning at full
    ///    speed for the rest of the binary;
    /// 2. with the demand standing and nobody taking the lock, the pump's chunk
    ///    rate over a fixed window must collapse by three orders of magnitude —
    ///    *keeps* yielding, not yielded once. The bound there is an order of
    ///    magnitude, not a park count: see `STANDING_BOUND`;
    /// 3. only then does the frame take the lock, and the chunks it costs are
    ///    counted from the mark taken immediately before, against a pump that is
    ///    already throttled to one `PUMP_PARK` per chunk.
    ///
    /// The version this replaced counted from the **raise**, which measured the
    /// scheduler: until the raise is visible to it the pump is unthrottled — an
    /// uncontended `lock`/`unlock` plus two atomics, tens of nanoseconds — while
    /// the renderer's own path to being queued on the mutex is microseconds, so
    /// on an idle machine it got through 65 and 149 chunks before it could know
    /// anything, and the test passed only when `cargo test --workspace`
    /// descheduled it. Every bound below is now measured across the pump's own
    /// parks, which makes each of them milliseconds of slack rather than a race.
    #[test]
    fn a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks() {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, AtomicU64};

        /// `asked_at` before the pump has asked once with the demand raised.
        const NOT_ASKED: u64 = u64::MAX;
        /// What the pump does at a chunk boundary where a frame is waiting.
        const PUMP_PARK: std::time::Duration = std::time::Duration::from_micros(250);
        /// Chunks the pump may still take once the frame starts contending. One
        /// park each, so this is 2 ms for the renderer to reach the mutex.
        const BOUND: u64 = 8;
        /// How long the frame holds the demand up without taking the lock, to
        /// watch what the pump does with a demand that stands.
        const STANDING_WINDOW: std::time::Duration = std::time::Duration::from_millis(20);
        /// Chunks the pump may take during that window and still count as
        /// yielding.
        ///
        /// It is deliberately **not** `STANDING_WINDOW / PUMP_PARK`. That ratio
        /// bounds nothing: the window is itself a `sleep`, so the wall time
        /// between the two chunk reads is those 20 ms *plus* whatever the
        /// scheduler takes to run this thread again, while the pump's park is
        /// only a floor — `thread::sleep` never sleeps less, but nothing
        /// promises it sleeps no more. The ratio was 81 against a theoretical
        /// 80, i.e. no margin at all, and a Linux CI job fitted 100 chunks into
        /// the real window: its timers reach the 250 us floor that Windows
        /// rounds up to ~500 us, so a few ms of overshoot on the window was
        /// enough (`BUG-0070`, acceptance rework 2026-09-22).
        ///
        /// What this has to discriminate is *yielding from spinning*, and those
        /// are four orders of magnitude apart: measured, a yielding pump fits
        /// 34-38 chunks here and ~100 on Linux, while both mutations of a pump
        /// that stops yielding fit 519 549 - 607 271. 2 000 leaves an honest
        /// pump a >50x margin — it is half a second of window at one park per
        /// chunk — and still fails a broken one by >250x. The bound proves
        /// "the pump yields", not "the pump sleeps exactly `PUMP_PARK`".
        const STANDING_BOUND: u64 = 2_000;

        let engine = Arc::new(Mutex::new(0u64));
        let demand = Demand::new();
        let stop = Arc::new(AtomicBool::new(false));
        let chunks = Arc::new(AtomicU64::new(0));
        let asked_at = Arc::new(AtomicU64::new(NOT_ASKED));

        let pump = {
            let engine = Arc::clone(&engine);
            let demand = demand.clone();
            let stop = Arc::clone(&stop);
            let chunks = Arc::clone(&chunks);
            let asked_at = Arc::clone(&asked_at);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    {
                        let mut engine = engine.lock().expect("the engine lock was poisoned");
                        *engine += 1;
                    }
                    let chunk = chunks.fetch_add(1, Ordering::Relaxed) + 1;
                    // The contract: replies first, then the demand check, then
                    // the next lock. The ask takes nothing away — the demand
                    // stands until the renderer holds the lock and releases it
                    // itself.
                    if demand.is_raised() {
                        let _ = asked_at.compare_exchange(
                            NOT_ASKED,
                            chunk,
                            Ordering::AcqRel,
                            Ordering::Relaxed,
                        );
                        std::thread::sleep(PUMP_PARK);
                    }
                }
            })
        };

        // Every wait is bounded, and **nothing below panics before the pump is
        // stopped and joined**: a panic that skips the join detaches a thread
        // that spins `while !stop` at full speed until the binary exits, so a
        // single failing test slows every test after it. Hence the measurement
        // runs into an `Option` and every assertion waits until after the join.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let wait_for = |ready: &dyn Fn() -> bool| {
            while !ready() {
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                std::thread::yield_now();
            }
            true
        };

        // Let the pump reach its steady state before asking for the lock.
        let steady = wait_for(&|| chunks.load(Ordering::Relaxed) >= 4);
        let mut measured = None;
        if steady {
            demand.raise();
            // Do not contend until the pump has provably seen the demand.
            if wait_for(&|| asked_at.load(Ordering::Acquire) != NOT_ASKED) {
                let asked = asked_at.load(Ordering::Acquire);

                // The demand stands and nobody is taking the lock: a pump that
                // yields for as long as a frame is outside pays a park per
                // chunk, so it cannot get through more than the window holds. A
                // pump that consumed the demand with its ask, or stopped
                // asking, is back to full speed here and takes orders of
                // magnitude more.
                let before_standing = chunks.load(Ordering::Relaxed);
                let standing = std::time::Instant::now();
                std::thread::sleep(STANDING_WINDOW);
                let while_standing = chunks.load(Ordering::Relaxed) - before_standing;
                // The window the count is actually over, not the one asked for.
                let stood_for = standing.elapsed();

                // Now contend. The mark is taken here, not at the raise: from
                // the ask above the pump is throttled, so this delta is the
                // frame's own latency measured in parks.
                let at_contend = chunks.load(Ordering::Relaxed);
                let waiting = std::time::Instant::now();
                let chunks_in = {
                    let _engine = engine.lock().expect("the engine lock was poisoned");
                    // Read under the lock: outside it the pump adds chunks
                    // between the acquisition and the load, which is the same
                    // measurement error one step later.
                    let chunks_in = chunks.load(Ordering::Relaxed);
                    // In, so the pump need not yield for this frame any more.
                    demand.release();
                    chunks_in
                };
                measured = Some((
                    asked,
                    while_standing,
                    stood_for,
                    at_contend,
                    chunks_in,
                    waiting.elapsed(),
                ));
            }
        }

        stop.store(true, Ordering::Relaxed);
        pump.join().expect("the pump thread panicked");

        assert!(steady, "the pump never reached its steady state");
        let (asked_at, while_standing, stood_for, at_contend, chunks_in, waited) =
            measured.expect("the pump never asked while a frame was waiting");
        let chunks_waited = chunks_in.saturating_sub(at_contend);

        assert!(
            while_standing <= STANDING_BOUND,
            "the pump took {while_standing} chunks in {stood_for:?} with a frame waiting (it \
             asked at chunk {asked_at}); a pump that parks at every chunk boundary fits a few \
             hundred, one that spins fits hundreds of thousands, so the bound is {STANDING_BOUND}"
        );
        assert!(
            waited < std::time::Duration::from_secs(2),
            "the renderer waited {waited:?} for a pump under sustained output"
        );
        assert!(
            chunks_waited <= BOUND,
            "the frame started contending at chunk {at_contend} and only reached the engine at \
             chunk {chunks_in}"
        );
        assert!(
            !demand.is_raised(),
            "the demand outlived the frame that raised it"
        );
    }

    /// A pump lock must not ask itself to yield.
    #[test]
    fn a_pump_lock_does_not_raise_the_demand() {
        let handle = handle();
        drop(handle.lock());
        drop(handle.lock());
        assert!(!handle.render_demand_raised());
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
        while !handle.render_demand_raised() {
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
        assert!(handle.try_lock().is_none());
        drop(guard);
        assert!(handle.try_lock().is_some());
    }

    /// The adapter routes out only what is OneTerm's. Every standard number
    /// stays the engine's, and the two that are not go silent without this.
    #[test]
    fn the_adapter_routes_only_the_osc_numbers_that_are_its_own() {
        let config = adapter_config(DEFAULT_SCROLLBACK_LINES);
        let routes = &config.osc_routes;

        // OneTerm's own proposal: the engine never sees it.
        assert_eq!(routes.get(AGENT_OSC), OscRoute::Forward);
        // The deprecated alias shares a built-in's number, so the built-in is
        // kept and the raw sequence comes too.
        assert_eq!(
            routes.get(LEGACY_AGENT_OSC),
            OscRoute::BuiltinAndForward,
            "the OSC 9 built-in must survive, or notifications and 9;4 progress die with the alias"
        );
        // Everything a terminal is expected to do is the engine's, untouched.
        for code in [0, 2, 4, 7, 8, 52, 133] {
            assert_eq!(routes.get(code), OscRoute::Builtin, "OSC {code}");
        }
        assert_eq!(
            routes.overrides().collect::<Vec<_>>(),
            vec![
                (LEGACY_AGENT_OSC, OscRoute::BuiltinAndForward),
                (AGENT_OSC, OscRoute::Forward),
            ],
            "nothing else was taken away from the engine"
        );

        assert!(
            routes.allows_large(52),
            "OSC 52 needs the large payload ceiling"
        );
        // The agent channel publishes an 8 KiB cap (`docs/osc-agent-status.md`
        // § 3.4), which the inline tier cannot deliver: `OSC_INLINE` bounds the
        // whole payload at 2 KiB, prefix included. Both spellings need the
        // ceiling, because the alias is parsed identically for one release and
        // that includes how much of it there may be.
        for code in [AGENT_OSC, LEGACY_AGENT_OSC] {
            assert!(
                routes.allows_large(code),
                "OSC {code} carries agent status and needs the large ceiling, \
                 or the documented 8 KiB cap is unreachable"
            );
        }
    }

    #[test]
    fn the_scrollback_limit_is_clamped_to_the_engine_maximum() {
        let config = adapter_config(usize::MAX);
        assert_eq!(config.scrollback_limit, SCROLLBACK_MAX);
    }
}
