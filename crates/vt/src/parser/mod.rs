//! The byte-level state machine.
//!
//! It turns a byte stream into [`Dispatch`] calls and nothing else: it knows no
//! grid, no cursor and no mode. [`Dispatch`] is its only outward coupling, so
//! the state machine can be replaced without touching the dispatch layer.
//!
//! The design is
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/parser.md`; the
//! reference behaviour it deviates from is Paul Williams' table as implemented
//! by `vte 0.15`, and `tests/differential.rs` holds both engines to the same
//! action trace apart from the deviations that file lists.
//!
//! Every limit here truncates or aborts rather than erroring, because all of
//! this input is untrusted: there is no useful recovery for "the remote sent an
//! 8 MiB title", and dropping the sequence would break OSC 52 for legitimate
//! large clipboard writes.

mod osc;
mod params;
mod state;
mod utf8;

#[cfg(test)]
mod parser_tests;

pub use osc::{MAX_OSC_PARAMS, OSC_INLINE, OSC_LARGE, OscParams, StringTerm};
pub use params::{
    Intermediates, MAX_INTERMEDIATES, MAX_PARAMS, ParamGroups, ParamSep, Params,
};

use osc::OscAccumulator;
use params::ParamSep::Semicolon;
use state::State;

/// Payload bytes accepted per DCS or APC sequence.
///
/// Enforced by the parser so the bound exists in one place even for a sink that
/// forgot one. It is also the binding constraint on Sixel: 16 MiB of payload
/// cannot produce the 64 MiB pixel clamp.
pub const DCS_MAX_BYTES: usize = 16 * 1024 * 1024;

/// What the parser produces. The dispatch layer implements it; so do the
/// escape stripper, the benchmark's null sink and the differential recorder.
pub trait Dispatch {
    /// A run of printable characters. The primary entry point: a whole run lets
    /// the print path do run-length writes, one width pass and one damage stamp
    /// per run instead of per character.
    fn print_str(&mut self, text: &str);

    /// One printable character. Defaults to [`Dispatch::print_str`], so an
    /// alternative state machine can implement either.
    fn print(&mut self, c: char) {
        let mut buffer = [0u8; 4];
        self.print_str(c.encode_utf8(&mut buffer));
    }

    /// A C0 control, `DEL`, or an 8-bit C1 byte. C1 is executed, never treated
    /// as an introducer.
    fn execute(&mut self, byte: u8);

    /// The final byte of an escape sequence. `ignore` is set when a third
    /// intermediate arrived, so a malformed `ESC SP ! # 8` is distinguishable
    /// from a well-formed `ESC SP ! 8` and can be dropped whole.
    fn esc(&mut self, intermediates: &[u8], ignore: bool, byte: u8);

    /// The final byte of a control sequence. `ignore` is set when a parameter or
    /// intermediate limit was hit; the sequence is still reported so the
    /// dispatch layer can drop it deliberately.
    fn csi(&mut self, params: &Params, intermediates: &[u8], ignore: bool, byte: u8);

    /// A terminated operating-system command. `code` is the leading number when
    /// the first parameter is one, `truncated` says the payload hit its cap.
    fn osc(&mut self, code: Option<u32>, params: &OscParams<'_>, term: StringTerm, truncated: bool);

    /// Whether this OSC number's payload may grow past [`OSC_INLINE`] to
    /// [`OSC_LARGE`].
    ///
    /// A memory ceiling only, and deliberately not a table inside the parser:
    /// the engine has no opinion about which OSC numbers deserve memory, and
    /// the clipboard policy stays with the embedder.
    fn osc_allows_large(&self, _code: u32) -> bool {
        false
    }

    /// The final byte of a device control string. Payload bytes follow through
    /// [`Dispatch::dcs_put`].
    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], byte: u8);

    /// One device-control payload byte. The parser holds no payload buffer at
    /// all; the sink owns its own.
    fn dcs_put(&mut self, byte: u8);

    /// The device control string ended. `aborted` marks a `CAN`/`SUB` abort or a
    /// sequence that ran past [`DCS_MAX_BYTES`].
    fn dcs_unhook(&mut self, aborted: bool);

    /// An APC, SOS or PM string started; `introducer` says which.
    fn apc_start(&mut self, introducer: u8);

    /// One APC payload byte.
    fn apc_put(&mut self, byte: u8);

    /// The APC string ended.
    fn apc_end(&mut self, aborted: bool);
}

/// The VT state machine.
///
/// State, parameters, the OSC accumulator and the UTF-8 carry all survive
/// between [`Parser::advance`] calls, so an arbitrary chunking of one stream
/// produces one action sequence.
pub struct Parser {
    state: State,
    params: Params,
    intermediates: Intermediates,
    /// The value being accumulated, not yet in `params`.
    param: u16,
    /// The separator that preceded `param`.
    param_sep: ParamSep,
    osc: OscAccumulator,
    /// Payload bytes streamed in the current DCS or APC sequence.
    string_bytes: usize,
    /// Set once a DCS or APC sequence passed [`DCS_MAX_BYTES`], so the abort is
    /// reported exactly once and the remainder is dropped.
    string_aborted: bool,
    partial_utf8: [u8; 4],
    partial_utf8_len: usize,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// A parser in the ground state.
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: Params::default(),
            intermediates: Intermediates::default(),
            param: 0,
            param_sep: Semicolon,
            osc: OscAccumulator::new(),
            string_bytes: 0,
            string_aborted: false,
            partial_utf8: [0; 4],
            partial_utf8_len: 0,
        }
    }

    /// Feed bytes, calling `dispatch` for every action they produce.
    ///
    /// Never blocks, never allocates without a bound and never panics on input.
    pub fn advance<D: Dispatch>(&mut self, dispatch: &mut D, bytes: &[u8]) {
        let mut index = 0;

        // A codepoint carried over from the previous call comes first; it can
        // consume nothing, in which case the bytes are re-read from ground.
        if self.partial_utf8_len != 0 {
            index += self.advance_partial_utf8(dispatch, bytes);
        }

        while index < bytes.len() {
            if self.state == State::Ground {
                index += self.advance_ground(dispatch, &bytes[index..]);
            } else {
                self.advance_one(dispatch, bytes[index]);
                index += 1;
            }
        }
    }

    /// Return to the ground state, dropping every partial sequence.
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.reset_params();
        self.osc.start();
        self.string_bytes = 0;
        self.string_aborted = false;
        self.partial_utf8_len = 0;
    }

    /// Clear parameters and intermediates.
    ///
    /// Runs on `ESC`, on `ESC [`, on `ESC P`, and on leaving `DcsPassthrough`
    /// through `ESC`, so the intermediates of the following escape are clean.
    fn reset_params(&mut self) {
        self.params.clear();
        self.intermediates.clear();
        self.param = 0;
        self.param_sep = Semicolon;
    }

    fn collect(&mut self, byte: u8) {
        if !self.intermediates.push(byte) {
            self.params.mark_overflow();
        }
    }

    fn param_digit(&mut self, byte: u8) {
        if self.params.is_full() {
            self.params.mark_overflow();
            return;
        }
        // Clamps at u16::MAX: `CSI 9223372036854775808 m` is `[65535]`.
        self.param = self
            .param
            .saturating_mul(10)
            .saturating_add(u16::from(byte - b'0'));
    }

    /// Close the pending value and record which separator follows it.
    fn param_separator(&mut self, next: ParamSep) {
        self.push_param();
        self.param_sep = next;
    }

    /// Push the pending value, whatever preceded it.
    ///
    /// A separator with nothing before it pushes `0`, and a dispatch with no
    /// parameters still pushes the pending `0`: `CSI m` is `[0]`.
    fn push_param(&mut self) {
        self.params.push(self.param, self.param_sep);
        self.param = 0;
    }

    fn csi_dispatch<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        self.push_param();
        dispatch.csi(
            &self.params,
            self.intermediates.as_slice(),
            self.params.ignored(),
            byte,
        );
        self.state = State::Ground;
    }

    fn dcs_hook<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        self.push_param();
        dispatch.dcs_hook(&self.params, self.intermediates.as_slice(), byte);
        self.string_bytes = 0;
        self.string_aborted = false;
        self.state = State::DcsPassthrough;
    }

    fn osc_put_param<D: Dispatch>(&mut self, dispatch: &mut D) {
        self.osc.push_param(|code| dispatch.osc_allows_large(code));
    }

    fn osc_end<D: Dispatch>(&mut self, dispatch: &mut D, term: StringTerm) {
        self.osc.push_param(|code| dispatch.osc_allows_large(code));
        let code = self.osc.code();
        let truncated = self.osc.truncated();
        dispatch.osc(code, &self.osc.params(), term, truncated);
        self.osc.start();
    }
}
