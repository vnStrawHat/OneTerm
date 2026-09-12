//! The OSC payload accumulator: bounded, and truncating rather than erroring.
//!
//! The reference accumulates into an unbounded `Vec<u8>` under `std`, which is a
//! remote memory-exhaustion vector reachable from any SSH session (IN-0029 P1).
//! This one is bounded in two tiers — [`OSC_INLINE`] for everything, and
//! [`OSC_LARGE`] only for a number the embedder claimed large — and it
//! **truncates**, because rejecting a long OSC would break OSC 52 for legitimate
//! large clipboard writes.

/// Payload bytes held without allocating.
pub const OSC_INLINE: usize = 2048;

/// Ceiling for an OSC number the embedder claimed large. A memory ceiling only:
/// who may write or read the clipboard stays with the embedder's policy.
pub const OSC_LARGE: usize = 8 * 1024 * 1024;

/// Parameters kept. Bytes past the last one keep accumulating into it, which is
/// what OSC 8's `;`-joined URIs rely on.
pub const MAX_OSC_PARAMS: usize = 16;

/// How an OSC or DCS string ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StringTerm {
    /// `BEL` (`0x07`).
    Bel,
    /// `ESC \` (ST). C1 `ST` (`0x9C`) is payload, not a terminator (trap 24).
    St,
}

/// The `;`-separated parameters of one OSC, as slices of the accumulated
/// payload.
pub struct OscParams<'a> {
    payload: &'a [u8],
    bounds: &'a [(u32, u32)],
}

impl<'a> OscParams<'a> {
    /// Number of parameters.
    pub fn len(&self) -> usize {
        self.bounds.len()
    }

    /// Whether the OSC carried no parameter at all. Never true in practice: an
    /// empty `ESC ] BEL` still dispatches with one empty parameter.
    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    /// The parameter at `index`.
    pub fn get(&self, index: usize) -> Option<&'a [u8]> {
        let &(start, end) = self.bounds.get(index)?;
        self.payload.get(start as usize..end as usize)
    }

    /// Every parameter in order.
    pub fn iter(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        (0..self.len()).filter_map(|index| self.get(index))
    }
}

/// Accumulates one OSC string.
pub(crate) struct OscAccumulator {
    inline: Box<[u8; OSC_INLINE]>,
    spill: Option<Vec<u8>>,
    len: usize,
    bounds: [(u32, u32); MAX_OSC_PARAMS],
    nparams: usize,
    truncated: bool,
    /// Parsed as soon as the first `;` arrives, so the spill decision is made
    /// before the payload does.
    code: Option<u32>,
    code_resolved: bool,
    large: bool,
}

impl OscAccumulator {
    pub(crate) fn new() -> Self {
        Self {
            inline: Box::new([0; OSC_INLINE]),
            spill: None,
            len: 0,
            bounds: [(0, 0); MAX_OSC_PARAMS],
            nparams: 0,
            truncated: false,
            code: None,
            code_resolved: false,
            large: false,
        }
    }

    pub(crate) fn start(&mut self) {
        self.len = 0;
        self.nparams = 0;
        self.truncated = false;
        self.code = None;
        self.code_resolved = false;
        self.large = false;
        if let Some(spill) = &mut self.spill {
            spill.clear();
            // A single large payload must not hold 8 MiB for the session.
            spill.shrink_to(OSC_INLINE);
        }
    }

    pub(crate) fn push(&mut self, byte: u8) {
        let cap = if self.large { OSC_LARGE } else { OSC_INLINE };
        if self.len >= cap {
            self.truncated = true;
            return;
        }
        if self.len < OSC_INLINE {
            self.inline[self.len] = byte;
        } else {
            let spill = self.spill.get_or_insert_with(Vec::new);
            if spill.is_empty() {
                spill.extend_from_slice(self.inline.as_slice());
            }
            spill.push(byte);
        }
        self.len += 1;
    }

    /// Close the current parameter at a `;` or at the terminator.
    ///
    /// `resolve_large` answers "may this OSC number spill past [`OSC_INLINE`]",
    /// and is only ever asked once, for the first parameter.
    pub(crate) fn push_param(&mut self, resolve_large: impl FnOnce(u32) -> bool) {
        let end = self.len as u32;
        if self.nparams == 0 {
            self.bounds[0] = (0, end);
            self.nparams = 1;
            self.resolve_code(resolve_large);
            return;
        }
        if self.nparams == MAX_OSC_PARAMS {
            // Past the limit the bytes keep accumulating into the last kept
            // parameter, which is the reference's behaviour.
            self.bounds[MAX_OSC_PARAMS - 1].1 = end;
            return;
        }
        let start = self.bounds[self.nparams - 1].1;
        self.bounds[self.nparams] = (start, end);
        self.nparams += 1;
    }

    /// Whether the parameter list is full, so a further `;` is payload rather
    /// than a separator: past the limit the bytes keep accumulating into the
    /// last parameter, which is what OSC 8's `;`-joined URIs rely on.
    pub(crate) fn params_full(&self) -> bool {
        self.nparams == MAX_OSC_PARAMS
    }

    /// The number this OSC carries, once the first parameter is known.
    pub(crate) fn code(&self) -> Option<u32> {
        self.code
    }

    pub(crate) fn truncated(&self) -> bool {
        self.truncated
    }

    pub(crate) fn params(&self) -> OscParams<'_> {
        OscParams {
            payload: self.payload(),
            bounds: &self.bounds[..self.nparams],
        }
    }

    fn payload(&self) -> &[u8] {
        match &self.spill {
            Some(spill) if !spill.is_empty() => &spill[..self.len],
            _ => &self.inline[..self.len.min(OSC_INLINE)],
        }
    }

    fn resolve_code(&mut self, resolve_large: impl FnOnce(u32) -> bool) {
        if self.code_resolved {
            return;
        }
        self.code_resolved = true;
        let code = {
            let (start, end) = self.bounds[0];
            let payload = self.payload();
            parse_code(&payload[start as usize..end as usize])
        };
        self.code = code;
        if let Some(code) = code {
            self.large = resolve_large(code);
        }
    }
}

/// `Some` only for a non-empty run of decimal digits that fits in a `u32`.
fn parse_code(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    let mut code: u32 = 0;
    for &byte in digits {
        let digit = (byte as char).to_digit(10)?;
        code = code.checked_mul(10)?.checked_add(digit)?;
    }
    Some(code)
}
