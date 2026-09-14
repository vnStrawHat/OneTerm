//! The mode table, the cursor style, the title stack and the kitty flag stack.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md`
//! sections "Modes", "Title stack" and "Kitty keyboard flag stack".
//!
//! [`Mode`] is a typed enum rather than a bitflags dump the caller has to know
//! how to mask (deviation D1): `Terminal::mode(Mode::AltScreen)` instead of
//! `term.mode().contains(TermMode::ALT_SCREEN)`, and one
//! `Terminal::mouse_reporting()` accessor instead of the `MOUSE_MODE` bit union
//! the current code recombines in seven places.

use bitflags::bitflags;

use crate::render::{MouseEncoding, MouseProtocol, MouseReporting};

/// Title-stack depth (deviation D14). The reference caps at 4096, which no
/// program approaches and which is a cheap memory sink.
pub const TITLE_STACK_MAX: usize = 16;

/// Kitty keyboard flag-stack depth, Ghostty's shape: fixed size, no heap.
pub const KEYBOARD_STACK_MAX: usize = 8;

/// One terminal mode.
///
/// Every variant that carries state has a bit in [`Modes`]; `DecCoLm` is listed
/// because both `h` and `l` act (trap 40) even though nothing is stored.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mode {
    /// `? 1`, DECCKM.
    AppCursor,
    /// `ESC =` / `ESC >`, DECKPAM / DECKPNM.
    AppKeypad,
    /// `? 3`, DECCOLM. Acts, stores nothing, and DECRQM says `NotSupported`.
    DecCoLm,
    /// `? 6`, DECOM.
    Origin,
    /// `? 7`, DECAWM.
    LineWrap,
    /// `? 12`.
    CursorBlink,
    /// `? 25`, DECTCEM.
    ShowCursor,
    /// `? 45`.
    ReverseWrap,
    /// `? 1000`.
    MouseClick,
    /// `? 1002`.
    MouseDrag,
    /// `? 1003`.
    MouseMotion,
    /// `? 1004`.
    FocusInOut,
    /// `? 1005`.
    Utf8Mouse,
    /// `? 1006`.
    SgrMouse,
    /// `? 1007`.
    AlternateScroll,
    /// `? 1042`.
    UrgencyHints,
    /// `? 47` (correction C8).
    AltScreen47,
    /// `? 1047` (correction C8).
    AltScreen1047,
    /// `? 1048` (correction C8): save / restore the cursor, no screen swap.
    SaveCursor1048,
    /// `? 1049`.
    AltScreen,
    /// `? 2004`.
    BracketedPaste,
    /// `? 2026`. The bit is not stored: the real state lives in
    /// [`crate::render::SyncState`] (deviation D4).
    SyncUpdate,
    /// `? 2027`. Recognised and inert (R-56).
    GraphemeClusters,
    /// `? 9001`. Recognised and inert (R-36).
    Win32Input,
    /// `4`, IRM.
    Insert,
    /// `20`, LNM. Tracked, inert (deviation D9), and answered through
    /// [`Mode::inert_state`] for that reason.
    LineFeedNewLine,
}

impl Mode {
    /// The bit this mode occupies in [`Modes`], or `None` for a mode whose state
    /// is not a bit: `DecCoLm` stores nothing and `SyncUpdate` lives in
    /// [`crate::render::SyncState`].
    const fn bit(self) -> Option<u32> {
        let index = match self {
            Mode::AppCursor => 0,
            Mode::AppKeypad => 1,
            Mode::Origin => 2,
            Mode::LineWrap => 3,
            Mode::ShowCursor => 5,
            Mode::ReverseWrap => 6,
            Mode::MouseClick => 7,
            Mode::MouseDrag => 8,
            Mode::MouseMotion => 9,
            Mode::FocusInOut => 10,
            Mode::Utf8Mouse => 11,
            Mode::SgrMouse => 12,
            Mode::AlternateScroll => 13,
            Mode::UrgencyHints => 14,
            Mode::BracketedPaste => 16,
            Mode::GraphemeClusters => 17,
            Mode::Win32Input => 18,
            Mode::Insert => 19,
            Mode::LineFeedNewLine => 20,
            // These five own no bit. The alternate screen's state is the grid's
            // (`TerminalGrid::alt_active`), `? 1048` is a cursor operation with
            // no state of its own, `DECCOLM` acts and stores nothing (trap 40),
            // `? 12` lives in the cursor style, and `? 2026`'s real state lives
            // in `SyncState` (deviation D4).
            Mode::AltScreen
            | Mode::AltScreen47
            | Mode::AltScreen1047
            | Mode::SaveCursor1048
            | Mode::DecCoLm
            | Mode::CursorBlink
            | Mode::SyncUpdate => return None,
        };
        Some(1 << index)
    }

    /// The private-mode number, for the modes that have one.
    pub const fn private_code(self) -> Option<u16> {
        Some(match self {
            Mode::AppCursor => 1,
            Mode::DecCoLm => 3,
            Mode::Origin => 6,
            Mode::LineWrap => 7,
            Mode::CursorBlink => 12,
            Mode::ShowCursor => 25,
            Mode::ReverseWrap => 45,
            Mode::AltScreen47 => 47,
            Mode::MouseClick => 1000,
            Mode::MouseDrag => 1002,
            Mode::MouseMotion => 1003,
            Mode::FocusInOut => 1004,
            Mode::Utf8Mouse => 1005,
            Mode::SgrMouse => 1006,
            Mode::AlternateScroll => 1007,
            Mode::UrgencyHints => 1042,
            Mode::AltScreen1047 => 1047,
            Mode::SaveCursor1048 => 1048,
            Mode::AltScreen => 1049,
            Mode::BracketedPaste => 2004,
            Mode::SyncUpdate => 2026,
            Mode::GraphemeClusters => 2027,
            Mode::Win32Input => 9001,
            Mode::AppKeypad | Mode::Insert | Mode::LineFeedNewLine => return None,
        })
    }

    /// A private-mode number to its mode.
    pub const fn from_private(code: u16) -> Option<Mode> {
        Some(match code {
            1 => Mode::AppCursor,
            3 => Mode::DecCoLm,
            6 => Mode::Origin,
            7 => Mode::LineWrap,
            12 => Mode::CursorBlink,
            25 => Mode::ShowCursor,
            45 => Mode::ReverseWrap,
            47 => Mode::AltScreen47,
            1000 => Mode::MouseClick,
            1002 => Mode::MouseDrag,
            1003 => Mode::MouseMotion,
            1004 => Mode::FocusInOut,
            1005 => Mode::Utf8Mouse,
            1006 => Mode::SgrMouse,
            1007 => Mode::AlternateScroll,
            1042 => Mode::UrgencyHints,
            1047 => Mode::AltScreen1047,
            1048 => Mode::SaveCursor1048,
            1049 => Mode::AltScreen,
            2004 => Mode::BracketedPaste,
            2026 => Mode::SyncUpdate,
            2027 => Mode::GraphemeClusters,
            9001 => Mode::Win32Input,
            _ => return None,
        })
    }

    /// An ANSI-mode number to its mode.
    pub const fn from_ansi(code: u16) -> Option<Mode> {
        Some(match code {
            4 => Mode::Insert,
            20 => Mode::LineFeedNewLine,
            _ => return None,
        })
    }

    /// The fixed `DECRQM` answer for a mode the engine **recognises but nothing
    /// reads**, or `None` for a mode whose state is real.
    ///
    /// This is the table behind the rule in `dispatch-and-modes.md`: *DECRQM
    /// must never answer `Set` for a mode that does nothing.* Answering `Set`
    /// tells a program a capability exists when it does not, so an inert mode
    /// answers `NotSupported` when it is not even stored and `Reset` when it is
    /// stored and simply unread. `? 45` was in this table until `US-0086` gave
    /// it a reader (`Screen::backspace`), which is exactly when a row leaves.
    pub const fn inert_state(self) -> Option<ModeState> {
        Some(match self {
            // Trap 40: both `h` and `l` act, and the honest answer is still
            // "not supported", because the width never changes.
            Mode::DecCoLm => ModeState::NotSupported,
            // R-56: recognised and inert, and `NotSupported` says so.
            Mode::GraphemeClusters => ModeState::NotSupported,
            // R-36: conhost sends `? 9001 h` unprompted, so it is accepted
            // silently — but the encoding is not implemented.
            Mode::Win32Input => ModeState::Reset,
            // Deviation D9: LNM is tracked and read by nothing — `LF` never
            // implies `CR` here. It is an ANSI mode, not a private one, but the
            // rule is about readers, not about which space the number lives in,
            // so it answers through this table like every other inert mode
            // (`US-0087`, the cleanup row `migration.md` queued).
            Mode::LineFeedNewLine => ModeState::Reset,
            _ => return None,
        })
    }

    /// Every mode with an ANSI number, and that number, for the `DECRQM` table
    /// test. The counterpart of [`Mode::PRIVATE`]; both walks exist so a mode
    /// added without a reader fails a test instead of lying on the wire.
    pub const ANSI: [(Mode, u16); 2] = [(Mode::Insert, 4), (Mode::LineFeedNewLine, 20)];

    /// Every mode with a private number, for the DECRQM table test.
    pub const PRIVATE: [Mode; 23] = [
        Mode::AppCursor,
        Mode::DecCoLm,
        Mode::Origin,
        Mode::LineWrap,
        Mode::CursorBlink,
        Mode::ShowCursor,
        Mode::ReverseWrap,
        Mode::AltScreen47,
        Mode::MouseClick,
        Mode::MouseDrag,
        Mode::MouseMotion,
        Mode::FocusInOut,
        Mode::Utf8Mouse,
        Mode::SgrMouse,
        Mode::AlternateScroll,
        Mode::UrgencyHints,
        Mode::AltScreen1047,
        Mode::SaveCursor1048,
        Mode::AltScreen,
        Mode::BracketedPaste,
        Mode::SyncUpdate,
        Mode::GraphemeClusters,
        Mode::Win32Input,
    ];
}

/// What `DECRQM` reports. The numbers are the wire values.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum ModeState {
    /// Not recognised; "do not use it".
    NotSupported = 0,
    Set = 1,
    Reset = 2,
    PermanentlySet = 3,
    PermanentlyReset = 4,
}

impl From<bool> for ModeState {
    fn from(value: bool) -> ModeState {
        if value {
            ModeState::Set
        } else {
            ModeState::Reset
        }
    }
}

/// The stored mode bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Modes {
    bits: u32,
}

impl Default for Modes {
    /// The power-on state: `DECAWM`, `DECTCEM`, alternate scroll and urgency
    /// hints, which is what the engine being replaced starts with.
    fn default() -> Modes {
        let mut modes = Modes { bits: 0 };
        modes.set(Mode::LineWrap, true);
        modes.set(Mode::ShowCursor, true);
        modes.set(Mode::AlternateScroll, true);
        modes.set(Mode::UrgencyHints, true);
        modes
    }
}

impl Modes {
    pub fn contains(&self, mode: Mode) -> bool {
        mode.bit().is_some_and(|bit| self.bits & bit != 0)
    }

    pub fn set(&mut self, mode: Mode, on: bool) {
        let Some(bit) = mode.bit() else { return };
        if on {
            self.bits |= bit;
        } else {
            self.bits &= !bit;
        }
    }

    /// Setting any mouse reporting mode clears the other two first; unsetting
    /// clears only that one. The reference's asymmetry, reproduced.
    pub fn set_mouse_reporting(&mut self, mode: Mode) {
        self.set(Mode::MouseClick, false);
        self.set(Mode::MouseDrag, false);
        self.set(Mode::MouseMotion, false);
        self.set(mode, true);
    }

    /// The composite the mouse encoder needs, as one accessor.
    pub fn mouse_reporting(&self) -> Option<MouseProtocol> {
        let reporting = if self.contains(Mode::MouseMotion) {
            MouseReporting::AnyEvent
        } else if self.contains(Mode::MouseDrag) {
            MouseReporting::ButtonEvent
        } else if self.contains(Mode::MouseClick) {
            MouseReporting::Normal
        } else {
            return None;
        };
        let encoding = if self.contains(Mode::SgrMouse) {
            MouseEncoding::Sgr
        } else if self.contains(Mode::Utf8Mouse) {
            MouseEncoding::Utf8
        } else {
            MouseEncoding::Default
        };
        Some(MouseProtocol {
            reporting,
            encoding,
        })
    }
}

/// `DECSCUSR` shapes, plus the two the engine derives.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Beam,
    /// Reported when the window loses focus; the engine never sets it.
    HollowBlock,
    /// `DECTCEM` reset.
    Hidden,
}

/// Shape plus blink, which is what `DECSCUSR` sets as a pair.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct CursorStyle {
    pub shape: CursorShape,
    pub blinking: bool,
}

bitflags! {
    /// The kitty keyboard protocol flags. The engine owns the state; the app
    /// owns the encoding, so nothing here is gated behind a config flag.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct KeyboardFlags: u8 {
        const DISAMBIGUATE_ESC_CODES = 1 << 0;
        const REPORT_EVENT_TYPES     = 1 << 1;
        const REPORT_ALTERNATE_KEYS  = 1 << 2;
        const REPORT_ALL_KEYS_AS_ESC = 1 << 3;
        const REPORT_ASSOCIATED_TEXT = 1 << 4;
    }
}

/// How `CSI = Ps ; Pb u` combines the new flags with the live ones.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FlagApply {
    Replace,
    Union,
    Difference,
}

/// Ghostty's shape: fixed size, no heap, push evicts the oldest, and
/// `pop(n >= len)` resets the whole stack, which removes the denial-of-service
/// vector of a client sending a huge pop count.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FlagStack {
    flags: [KeyboardFlags; KEYBOARD_STACK_MAX],
    len: u8,
    /// The live flags, which can legitimately differ from the stack top after
    /// `CSI = Ps u` (trap 42).
    live: KeyboardFlags,
}

impl FlagStack {
    pub fn live(&self) -> KeyboardFlags {
        self.live
    }

    /// What `CSI ? u` reports: the top of the stack, not the live flags.
    pub fn top(&self) -> KeyboardFlags {
        match self.len {
            0 => KeyboardFlags::empty(),
            len => self.flags[len as usize - 1],
        }
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn apply(&mut self, flags: KeyboardFlags, how: FlagApply) {
        self.live = match how {
            FlagApply::Replace => flags,
            FlagApply::Union => self.live | flags,
            FlagApply::Difference => self.live & !flags,
        };
    }

    /// `CSI > Ps u`. A full stack drops the oldest entry, which is the fix for
    /// the reference's overflow bug — it pops the *title* stack here
    /// (deviation D15).
    pub fn push(&mut self, flags: KeyboardFlags) {
        if self.len as usize == KEYBOARD_STACK_MAX {
            self.flags.rotate_left(1);
            self.len -= 1;
        }
        self.flags[self.len as usize] = flags;
        self.len += 1;
        self.live = flags;
    }

    /// `CSI < Ps u`. Popping at or beyond the depth resets the stack.
    pub fn pop(&mut self, count: u16) {
        let count = count as usize;
        self.len = if count >= self.len as usize {
            0
        } else {
            self.len - count as u8
        };
        self.live = self.top();
    }
}

/// One stack per screen, swapped on an alternate-screen swap.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct KeyboardStacks {
    pub active: FlagStack,
    inactive: FlagStack,
}

impl KeyboardStacks {
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.active, &mut self.inactive);
    }

    pub fn reset(&mut self) {
        *self = KeyboardStacks::default();
    }
}

/// The current title and the `CSI 22 t` / `CSI 23 t` stack.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TitleState {
    pub title: Option<String>,
    stack: Vec<Option<String>>,
}

impl TitleState {
    /// `CSI 22 t`. Overflow drops the oldest entry (deviation D14).
    pub fn push(&mut self) {
        if self.stack.len() >= TITLE_STACK_MAX {
            self.stack.remove(0);
        }
        self.stack.push(self.title.clone());
    }

    /// `CSI 23 t`. `None` means the stack was empty and nothing is applied.
    pub fn pop(&mut self) -> Option<Option<String>> {
        self.stack.pop()
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn reset(&mut self) {
        self.title = None;
        self.stack.clear();
    }
}
