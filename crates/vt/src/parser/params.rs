//! CSI and DCS parameters, with the separator that produced each value.
//!
//! Storing sub-parameters as run lengths is enough for SGR but loses the
//! structural difference the dispatch layer needs: `38;5;n` consumes a
//! following *parameter* where `38:5:n` reads a *sub-parameter*, and
//! `38:2:<cs>:R:G:B` skips a colour-space id that `38:2:R:G:B` does not.
//! Keeping the separator per value makes that a lookup instead of a
//! re-derivation.

/// Maximum number of values, parameters and sub-parameters together.
///
/// Williams' table asks for at least 16; this is a wider budget.
pub const MAX_PARAMS: usize = 32;

/// Maximum number of collected intermediate bytes (`0x20..=0x2F`, plus a CSI
/// private marker).
pub const MAX_INTERMEDIATES: usize = 2;

/// The separator that preceded a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ParamSep {
    /// `;` — this value starts a new parameter.
    #[default]
    Semicolon,
    /// `:` — this value is a sub-parameter of the preceding one.
    Colon,
}

/// A flat list of parameter values plus the separator preceding each.
///
/// `seps[0]` is always [`ParamSep::Semicolon`]: the first value cannot be a
/// sub-parameter of anything.
#[derive(Clone, Debug)]
pub struct Params {
    values: [u16; MAX_PARAMS],
    seps: [ParamSep; MAX_PARAMS],
    len: u8,
    /// Set when the 33rd value or the third intermediate arrived. The dispatch
    /// still fires so the sequence can be dropped deliberately, which is a
    /// different path from the `CsiIgnore` state.
    overflowed: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            values: [0; MAX_PARAMS],
            seps: [ParamSep::Semicolon; MAX_PARAMS],
            len: 0,
            overflowed: false,
        }
    }
}

impl Params {
    /// Number of values, sub-parameters included.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether no value was collected at all.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The value at `index`, sub-parameters counted in the same flat space.
    pub fn get(&self, index: usize) -> Option<u16> {
        self.values().get(index).copied()
    }

    /// The separator that preceded the value at `index`.
    pub fn sep(&self, index: usize) -> Option<ParamSep> {
        self.seps[..self.len()].get(index).copied()
    }

    /// Every value in order, sub-parameters flattened in.
    pub fn values(&self) -> &[u16] {
        &self.values[..self.len()]
    }

    /// Whether a limit was hit while collecting. The sequence still dispatches.
    pub fn ignored(&self) -> bool {
        self.overflowed
    }

    /// Parameters as groups: one slice per `;`-separated parameter, its tail
    /// being that parameter's `:`-separated sub-parameters.
    pub fn groups(&self) -> ParamGroups<'_> {
        ParamGroups {
            params: self,
            index: 0,
        }
    }

    pub(crate) fn is_full(&self) -> bool {
        self.len() == MAX_PARAMS
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
        self.overflowed = false;
    }

    pub(crate) fn mark_overflow(&mut self) {
        self.overflowed = true;
    }

    /// Append `value` behind `sep`, or set the overflow flag when full.
    pub(crate) fn push(&mut self, value: u16, sep: ParamSep) {
        if self.is_full() {
            self.overflowed = true;
            return;
        }
        let index = self.len();
        self.values[index] = value;
        self.seps[index] = if index == 0 { ParamSep::Semicolon } else { sep };
        self.len += 1;
    }
}

/// Iterator over `;`-separated parameters, yielding each with its
/// sub-parameters.
pub struct ParamGroups<'a> {
    params: &'a Params,
    index: usize,
}

impl<'a> Iterator for ParamGroups<'a> {
    type Item = &'a [u16];

    fn next(&mut self) -> Option<Self::Item> {
        let len = self.params.len();
        if self.index >= len {
            return None;
        }
        let start = self.index;
        let mut end = start + 1;
        while end < len && self.params.seps[end] == ParamSep::Colon {
            end += 1;
        }
        self.index = end;
        Some(&self.params.values[start..end])
    }
}

/// Collected intermediate bytes, including a CSI private marker.
#[derive(Clone, Copy, Debug, Default)]
pub struct Intermediates {
    bytes: [u8; MAX_INTERMEDIATES],
    len: u8,
}

impl Intermediates {
    /// The collected bytes, in the order they arrived.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    /// Collect `byte`, reporting `false` when the buffer was already full so the
    /// caller can set the overflow flag.
    pub(crate) fn push(&mut self, byte: u8) -> bool {
        if self.len as usize == MAX_INTERMEDIATES {
            return false;
        }
        self.bytes[self.len as usize] = byte;
        self.len += 1;
        true
    }
}
