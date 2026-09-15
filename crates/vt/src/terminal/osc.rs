//! The OSC routing table: the extension point.
//!
//! The engine decides only **what to do with an OSC number**, never who handles
//! it. The decision is data — two bits per number — so an embedder can extend
//! the engine with a number it has never heard of, override a built-in, or keep
//! a built-in and watch the raw bytes go past, all without this crate changing
//! and without any embedder code running inside `feed`.
//!
//! ```
//! use oneterm_vt::{Config, OscRoute, OscRoutes};
//!
//! let mut routes = OscRoutes::new();
//! // A number the engine has never heard of, delivered raw.
//! routes.route(31337, OscRoute::Forward).large(31337, true);
//! // A built-in the embedder wants to watch as well as keep.
//! routes.route(9, OscRoute::BuiltinAndForward);
//! let config = Config {
//!     osc_routes: routes,
//!     ..Config::default()
//! };
//! # let _ = config;
//! ```
//!
//! Design: <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/osc-extension.md>.

/// The bitmap covers `0..2048`, which is every OSC number in common use; larger
/// numbers fall back to a sorted list, because a bitmap over `u32` would be half
/// a gigabyte.
const BITMAP_BITS: u32 = 2048;
const BITMAP_WORDS: usize = (BITMAP_BITS / 64) as usize;

/// What the engine does with one OSC number.
///
/// The default for a number the engine implements is [`OscRoute::Builtin`]; the
/// default for every other number is [`OscRoute::Drop`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum OscRoute {
    /// The engine's own handler runs and emits its typed event. The raw
    /// sequence is not delivered.
    Builtin,
    /// The engine's own handler runs *and* the raw parameters are delivered as
    /// [`crate::VtEvent::Osc`], after the typed event. Use this to observe a
    /// number without changing what the terminal does with it.
    BuiltinAndForward,
    /// The engine's own handler is skipped; only [`crate::VtEvent::Osc`] is
    /// delivered. Use this to replace a built-in, or to handle a number the
    /// engine does not implement.
    Forward,
    /// Nothing happens. The sequence is parsed, counted in
    /// [`FeedStats::unhandled_sequences`](crate::FeedStats::unhandled_sequences),
    /// and discarded.
    Drop,
}

/// Which OSC numbers the engine handles, forwards, or ignores.
///
/// Cheap to clone and compare; holds no allocation for any number below 2048.
/// Two tables that route every number the same way **are** equal: the bits are
/// canonical, so saying `Drop` about a number the engine does not implement
/// leaves the table exactly as it was.
///
/// The table is read when a `Terminal` is built and is not live: a `Terminal`
/// keeps the `Config` it was given ([`Terminal::config`](crate::Terminal::config)
/// hands out a shared reference and there is no setter), so an embedder that
/// wants a different route mid-session builds a new terminal. This is
/// deliberate — a route that could change under a half-parsed sequence would be
/// a race the engine has no way to describe.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct OscRoutes {
    forward: CodeSet,
    suppress: CodeSet,
    large: CodeSet,
}

impl OscRoutes {
    /// The OSC numbers the engine implements itself.
    ///
    /// Publishing the set is what stops a route being dead on arrival: an
    /// embedder can ask before it routes, and [`OscRoutes::route`] asserts.
    pub const BUILTIN: [u32; 18] = [
        0, 1, 2, 4, 7, 8, 9, 10, 11, 12, 22, 50, 52, 104, 110, 111, 112, 133,
    ];

    /// A table in which every built-in runs and nothing is forwarded.
    pub fn new() -> OscRoutes {
        OscRoutes::default()
    }

    /// Whether the engine implements this number itself.
    ///
    /// A bitmap read, not a scan of [`OscRoutes::BUILTIN`], so the whole
    /// routing decision stays three bit tests.
    pub fn has_builtin(code: u32) -> bool {
        code < BITMAP_BITS && BUILTIN_BITS[(code / 64) as usize] & (1 << (code % 64)) != 0
    }

    /// Set the route for one number. Idempotent; later calls win.
    ///
    /// Asking for [`OscRoute::Builtin`] on a number the engine does not
    /// implement is a **debug assertion**: there is no handler to run, so the
    /// route would silently mean [`OscRoute::Drop`] and the caller would never
    /// find out. It is an assertion rather than a panic because a table can be
    /// built from data, and a release build of an embedder should not die over
    /// one.
    pub fn route(&mut self, code: u32, route: OscRoute) -> &mut Self {
        debug_assert!(
            route != OscRoute::Builtin || OscRoutes::has_builtin(code),
            "OSC {code} has no built-in handler, so routing it to `Builtin` can never run \
             anything; ask `OscRoutes::has_builtin` first"
        );
        // Store the bits of the route the number will *actually* get, not of
        // the one that was asked for. Two tables that behave the same way then
        // compare equal, and a number put back to its default leaves the spill
        // list instead of sitting in it for ever.
        let builtin = OscRoutes::has_builtin(code);
        let (forward, suppress) = match route {
            OscRoute::Builtin => (false, false),
            OscRoute::BuiltinAndForward if builtin => (true, false),
            OscRoute::BuiltinAndForward | OscRoute::Forward => (true, true),
            OscRoute::Drop => (false, builtin),
        };
        self.forward.set(code, forward);
        self.suppress.set(code, suppress);
        self
    }

    /// Set the route for several numbers at once.
    pub fn route_all(&mut self, codes: &[u32], route: OscRoute) -> &mut Self {
        for &code in codes {
            self.route(code, route);
        }
        self
    }

    /// Allow this number's payload to grow from [`crate::parser::OSC_INLINE`]
    /// to [`crate::parser::OSC_LARGE`].
    ///
    /// A **memory ceiling only**, orthogonal to the route: who may write the
    /// clipboard, and under what limits, is the embedder's policy and not the
    /// engine's. `large(52, true)` buys a large clipboard write while OSC 52
    /// stays a built-in.
    ///
    /// Buying a ceiling for a number whose route is [`OscRoute::Drop`] is a
    /// **debug assertion**: the payload is accumulated and then thrown away,
    /// which is a memory hazard with no benefit. The assertion reads the table
    /// as it stands, so **route the number first**:
    /// `route(n, OscRoute::Forward).large(n, true)`, not the other way round.
    /// A table assembled from configuration data in an arbitrary order should
    /// apply every route before any ceiling.
    pub fn large(&mut self, code: u32, allow: bool) -> &mut Self {
        debug_assert!(
            !allow || self.get(code) != OscRoute::Drop,
            "OSC {code} is dropped, so a large payload would be accumulated and discarded; \
             route it before raising its ceiling"
        );
        self.large.set(code, allow);
        self
    }

    /// What the engine does with this number.
    pub fn get(&self, code: u32) -> OscRoute {
        match (self.forward.contains(code), self.suppress.contains(code)) {
            (false, false) if OscRoutes::has_builtin(code) => OscRoute::Builtin,
            (true, false) if OscRoutes::has_builtin(code) => OscRoute::BuiltinAndForward,
            (true, _) => OscRoute::Forward,
            (false, _) => OscRoute::Drop,
        }
    }

    /// Whether this number's payload may spill past
    /// [`crate::parser::OSC_INLINE`].
    pub fn allows_large(&self, code: u32) -> bool {
        self.large.contains(code)
    }

    /// Every number whose route differs from the default, for diagnostics.
    pub fn overrides(&self) -> impl Iterator<Item = (u32, OscRoute)> + '_ {
        let mut spill: Vec<u32> = self
            .forward
            .high
            .iter()
            .chain(self.suppress.high.iter())
            .copied()
            .collect();
        spill.sort_unstable();
        spill.dedup();
        (0..BITMAP_BITS)
            .chain(spill)
            .map(|code| (code, self.get(code)))
            .filter(|&(code, route)| route != default_route(code))
    }
}

/// [`OscRoutes::BUILTIN`] as a bitmap, so asking whether a number has a
/// built-in is a bit test rather than a scan. Every built-in number is below
/// [`BITMAP_BITS`], which this build check relies on.
const BUILTIN_BITS: [u64; BITMAP_WORDS] = {
    let mut bits = [0u64; BITMAP_WORDS];
    let mut index = 0;
    while index < OscRoutes::BUILTIN.len() {
        let code = OscRoutes::BUILTIN[index];
        assert!(
            code < BITMAP_BITS,
            "a built-in OSC number must fit the bitmap"
        );
        bits[(code / 64) as usize] |= 1 << (code % 64);
        index += 1;
    }
    bits
};

/// The route a number has when nothing has been said about it.
fn default_route(code: u32) -> OscRoute {
    if OscRoutes::has_builtin(code) {
        OscRoute::Builtin
    } else {
        OscRoute::Drop
    }
}

/// A set of OSC numbers: a bitmap over the common range plus a sorted spill.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct CodeSet {
    low: [u64; BITMAP_WORDS],
    high: Vec<u32>,
}

impl CodeSet {
    fn set(&mut self, code: u32, member: bool) {
        if code < BITMAP_BITS {
            let (word, bit) = ((code / 64) as usize, 1 << (code % 64));
            if member {
                self.low[word] |= bit;
            } else {
                self.low[word] &= !bit;
            }
            return;
        }
        match (self.high.binary_search(&code), member) {
            (Err(index), true) => self.high.insert(index, code),
            (Ok(index), false) => {
                self.high.remove(index);
            }
            _ => {}
        }
    }

    fn contains(&self, code: u32) -> bool {
        if code < BITMAP_BITS {
            self.low[(code / 64) as usize] & (1 << (code % 64)) != 0
        } else {
            self.high.binary_search(&code).is_ok()
        }
    }
}
