//! The state transitions: Paul Williams' fourteen states, with the deviations
//! [`low-level-design/parser.md`](../../../../docs/spec-intakes/IN-0029-vt-engine/low-level-design/parser.md)
//! lists.
//!
//! `Ground` is not handled here — it is the scan in [`super::utf8`] — and
//! `Utf8` is not a state at all: a partial codepoint lives in a four-byte carry
//! buffer consulted at the top of `advance`.

use super::osc::StringTerm;
use super::params::ParamSep;
use super::{DCS_MAX_BYTES, Dispatch, Parser};

/// Where the state machine is between bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    OscString,
    /// APC, SOS and PM share one state; only APC reaches the sink.
    ApcString,
}

impl Parser {
    /// Advance one byte in every state but `Ground`.
    pub(super) fn advance_one<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match self.state {
            State::Ground => unreachable!("ground is handled by the run scanner"),
            State::Escape => self.advance_escape(dispatch, byte),
            State::EscapeIntermediate => self.advance_escape_intermediate(dispatch, byte),
            State::CsiEntry => self.advance_csi_entry(dispatch, byte),
            State::CsiParam => self.advance_csi_param(dispatch, byte),
            State::CsiIntermediate => self.advance_csi_intermediate(dispatch, byte),
            State::CsiIgnore => self.advance_csi_ignore(dispatch, byte),
            State::DcsEntry => self.advance_dcs_entry(dispatch, byte),
            State::DcsParam => self.advance_dcs_param(dispatch, byte),
            State::DcsIntermediate => self.advance_dcs_intermediate(dispatch, byte),
            State::DcsPassthrough => self.advance_dcs_passthrough(dispatch, byte),
            State::DcsIgnore => self.advance_anywhere(dispatch, byte),
            State::OscString => self.advance_osc_string(dispatch, byte),
            State::ApcString => self.advance_apc_string(dispatch, byte),
        }
    }

    fn advance_escape<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => dispatch.execute(byte),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::EscapeIntermediate;
            }
            0x50 => {
                self.reset_params();
                self.state = State::DcsEntry;
            }
            0x5B => {
                self.reset_params();
                self.state = State::CsiEntry;
            }
            0x5D => {
                self.osc.start();
                self.state = State::OscString;
            }
            0x58 | 0x5E | 0x5F => {
                self.string_bytes = 0;
                self.string_aborted = false;
                dispatch.apc_start(byte);
                self.state = State::ApcString;
            }
            0x30..=0x4F | 0x51..=0x57 | 0x59..=0x5A | 0x5C | 0x60..=0x7E => {
                dispatch.esc(self.intermediates.as_slice(), byte);
                self.state = State::Ground;
            }
            0x18 | 0x1A => {
                dispatch.execute(byte);
                self.state = State::Ground;
            }
            // `ESC` is idempotent: `ESC ESC ESC [ A` is one CUU.
            0x1B => (),
            _ => (),
        }
    }

    fn advance_escape_intermediate<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => dispatch.execute(byte),
            0x20..=0x2F => self.collect(byte),
            0x30..=0x7E => {
                dispatch.esc(self.intermediates.as_slice(), byte);
                self.state = State::Ground;
            }
            0x7F => (),
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_csi_entry<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => dispatch.execute(byte),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::CsiIntermediate;
            }
            0x30..=0x39 => {
                self.param_digit(byte);
                self.state = State::CsiParam;
            }
            0x3A => {
                self.param_separator(ParamSep::Colon);
                self.state = State::CsiParam;
            }
            0x3B => {
                self.param_separator(ParamSep::Semicolon);
                self.state = State::CsiParam;
            }
            // `<`, `=`, `>` and `?` are private markers, collected as
            // intermediates and only accepted here.
            0x3C..=0x3F => {
                self.collect(byte);
                self.state = State::CsiParam;
            }
            0x40..=0x7E => self.csi_dispatch(dispatch, byte),
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_csi_param<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => dispatch.execute(byte),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::CsiIntermediate;
            }
            0x30..=0x39 => self.param_digit(byte),
            0x3A => self.param_separator(ParamSep::Colon),
            0x3B => self.param_separator(ParamSep::Semicolon),
            // A private marker here poisons the whole sequence (trap 23).
            0x3C..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.csi_dispatch(dispatch, byte),
            0x7F => (),
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_csi_intermediate<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => dispatch.execute(byte),
            0x20..=0x2F => self.collect(byte),
            0x30..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.csi_dispatch(dispatch, byte),
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_csi_ignore<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => dispatch.execute(byte),
            0x20..=0x3F | 0x7F => (),
            // The final byte dispatches nothing at all, unlike an overflow.
            0x40..=0x7E => self.state = State::Ground,
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_dcs_entry<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            // C0 is dropped here, not executed, unlike in the CSI states.
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::DcsIntermediate;
            }
            0x30..=0x39 => {
                self.param_digit(byte);
                self.state = State::DcsParam;
            }
            0x3A => {
                self.param_separator(ParamSep::Colon);
                self.state = State::DcsParam;
            }
            0x3B => {
                self.param_separator(ParamSep::Semicolon);
                self.state = State::DcsParam;
            }
            0x3C..=0x3F => {
                self.collect(byte);
                self.state = State::DcsParam;
            }
            0x40..=0x7E => self.dcs_hook(dispatch, byte),
            0x7F => (),
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_dcs_param<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::DcsIntermediate;
            }
            0x30..=0x39 => self.param_digit(byte),
            0x3A => self.param_separator(ParamSep::Colon),
            0x3B => self.param_separator(ParamSep::Semicolon),
            0x3C..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.dcs_hook(dispatch, byte),
            0x7F => (),
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_dcs_intermediate<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => self.collect(byte),
            0x30..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.dcs_hook(dispatch, byte),
            0x7F => (),
            _ => self.advance_anywhere(dispatch, byte),
        }
    }

    fn advance_dcs_passthrough<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x7E => {
                self.string_put(byte, |d, b| d.dcs_put(b), dispatch)
            }
            0x18 | 0x1A => {
                self.dcs_unhook(dispatch, true);
                dispatch.execute(byte);
                self.state = State::Ground;
            }
            0x1B => {
                self.dcs_unhook(dispatch, false);
                // The intermediates of the *following* escape must be clean.
                self.reset_params();
                self.state = State::Escape;
            }
            0x7F => (),
            // The one place an 8-bit `ST` is a terminator rather than payload.
            0x9C => {
                self.dcs_unhook(dispatch, false);
                self.state = State::Ground;
            }
            _ => (),
        }
    }

    fn advance_apc_string<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x18 | 0x1A => {
                self.apc_end(dispatch, true);
                dispatch.execute(byte);
                self.state = State::Ground;
            }
            0x1B => {
                self.apc_end(dispatch, false);
                self.reset_params();
                self.state = State::Escape;
            }
            _ => self.string_put(byte, |d, b| d.apc_put(b), dispatch),
        }
    }

    fn advance_osc_string<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x07 => {
                self.osc_end(dispatch, StringTerm::Bel);
                self.state = State::Ground;
            }
            0x18 | 0x1A => {
                self.osc_end(dispatch, StringTerm::St);
                dispatch.execute(byte);
                self.state = State::Ground;
            }
            0x1B => {
                self.osc_end(dispatch, StringTerm::St);
                self.reset_params();
                self.state = State::Escape;
            }
            0x3B if self.osc.params_full() => self.osc.push(byte),
            0x3B => self.osc_put_param(dispatch),
            // Everything else, `0x9C` included, is payload (trap 24).
            _ => self.osc.push(byte),
        }
    }

    /// `CAN`, `SUB` and `ESC` leave any string or ignore state; the rest is
    /// discarded.
    fn advance_anywhere<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        match byte {
            0x18 | 0x1A => {
                dispatch.execute(byte);
                self.state = State::Ground;
            }
            0x1B => {
                self.reset_params();
                self.state = State::Escape;
            }
            _ => (),
        }
    }

    /// Stream one payload byte to a sink, enforcing [`DCS_MAX_BYTES`] in the
    /// parser so the bound exists even for a sink that forgot one.
    fn string_put<D: Dispatch>(&mut self, byte: u8, put: fn(&mut D, u8), dispatch: &mut D) {
        if self.string_aborted {
            return;
        }
        if self.string_bytes == DCS_MAX_BYTES {
            self.string_aborted = true;
            match self.state {
                State::DcsPassthrough => dispatch.dcs_unhook(true),
                _ => dispatch.apc_end(true),
            }
            return;
        }
        self.string_bytes += 1;
        put(dispatch, byte);
    }

    fn dcs_unhook<D: Dispatch>(&mut self, dispatch: &mut D, aborted: bool) {
        // A sequence past the cap already reported its abort.
        if !self.string_aborted {
            dispatch.dcs_unhook(aborted);
        }
        self.string_aborted = false;
        self.string_bytes = 0;
    }

    fn apc_end<D: Dispatch>(&mut self, dispatch: &mut D, aborted: bool) {
        if !self.string_aborted {
            dispatch.apc_end(aborted);
        }
        self.string_aborted = false;
        self.string_bytes = 0;
    }
}
