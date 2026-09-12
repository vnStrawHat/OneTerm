//! The OSC registration table — the extension point.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md`
//! section "OSC registration".
//!
//! The engine handles a fixed set of OSC numbers natively; everything else is
//! delivered to the embedder as [`crate::VtEvent::Osc`] **only if the embedder
//! claimed it**. That is what replaces the fork's `report_osc` patch: OSC 9;7,
//! the agent channel, becomes a claim on OSC 9 plus a sub-code match in
//! `crates/terminal/src/osc_agent/`, with no engine change.

/// The bitmap covers `0..2048`, which is every OSC number in common use; larger
/// numbers fall back to a sorted list, because a bitmap over `u32` would be half
/// a gigabyte.
const BITMAP_BITS: u32 = 2048;
const BITMAP_WORDS: usize = (BITMAP_BITS / 64) as usize;

/// Which OSC numbers reach the embedder, and which may spill past
/// [`crate::parser::OSC_INLINE`].
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct OscClaims {
    low: [u64; BITMAP_WORDS],
    high: Vec<u32>,
    large_low: [u64; BITMAP_WORDS],
    large_high: Vec<u32>,
}

impl OscClaims {
    pub fn new() -> OscClaims {
        OscClaims::default()
    }

    /// Deliver this OSC number to the embedder.
    pub fn claim(&mut self, code: u32) -> &mut Self {
        set(&mut self.low, &mut self.high, code);
        self
    }

    /// Deliver it, and allow its payload to spill to
    /// [`crate::parser::OSC_LARGE`].
    ///
    /// A **memory ceiling only**: who may write or read the clipboard, and under
    /// what limits, stays in `crates/terminal/src/security_policy.rs`.
    pub fn claim_large(&mut self, code: u32) -> &mut Self {
        set(&mut self.low, &mut self.high, code);
        set(&mut self.large_low, &mut self.large_high, code);
        self
    }

    pub fn is_claimed(&self, code: u32) -> bool {
        contains(&self.low, &self.high, code)
    }

    pub fn allows_large(&self, code: u32) -> bool {
        contains(&self.large_low, &self.large_high, code)
    }
}

fn set(low: &mut [u64; BITMAP_WORDS], high: &mut Vec<u32>, code: u32) {
    if code < BITMAP_BITS {
        low[(code / 64) as usize] |= 1 << (code % 64);
    } else if let Err(index) = high.binary_search(&code) {
        high.insert(index, code);
    }
}

fn contains(low: &[u64; BITMAP_WORDS], high: &[u32], code: u32) -> bool {
    if code < BITMAP_BITS {
        low[(code / 64) as usize] & (1 << (code % 64)) != 0
    } else {
        high.binary_search(&code).is_ok()
    }
}
