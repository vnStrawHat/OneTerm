//! The modes the renderer reads, snapshotted so paint never takes the lock.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
//! section "Mode snapshot" (R-17).
//!
//! The view reads `DECCKM` at paint time and alt-screen and mouse state about
//! ten times per frame. Copying them into the render state on **every** update,
//! including one that returns `Unchanged`, is what keeps
//! `docs/terminal-backend.md` § 5.2's rule — the lock is never held while
//! painting — true without a second lock acquisition.
//!
//! The mode *table* is `US-0076`'s; this is only what leaves it.

/// `? 1000` / `? 1002` / `? 1003`: how much motion the host asked for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseReporting {
    /// `? 1000`: press and release.
    Normal,
    /// `? 1002`: press, release and motion while a button is down.
    ButtonEvent,
    /// `? 1003`: every motion.
    AnyEvent,
}

/// `? 1005` / `? 1006`: how a report is encoded.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum MouseEncoding {
    /// X10 coordinates, the power-on encoding.
    #[default]
    Default,
    /// `? 1005`.
    Utf8,
    /// `? 1006`.
    Sgr,
}

/// One composite instead of five booleans the embedder has to recombine.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MouseProtocol {
    pub reporting: MouseReporting,
    pub encoding: MouseEncoding,
}

/// Everything the view reads that is not a cell, a cursor or a selection.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ModeSnapshot {
    pub alt_screen: bool,
    /// `DECCKM`.
    pub app_cursor: bool,
    /// `DECKPAM`.
    pub app_keypad: bool,
    /// `? 2004`.
    pub bracketed_paste: bool,
    /// `DECTCEM`.
    pub show_cursor: bool,
    /// `IRM`.
    pub insert: bool,
    /// `? 1007`.
    pub alternate_scroll: bool,
    /// `None` when the host asked for no mouse reporting at all.
    pub mouse: Option<MouseProtocol>,
}

impl Default for ModeSnapshot {
    /// The power-on state: the cursor is visible and the alternate scroll
    /// translation is on, which is what `xterm` and the engine being replaced
    /// both start with.
    fn default() -> ModeSnapshot {
        ModeSnapshot {
            alt_screen: false,
            app_cursor: false,
            app_keypad: false,
            bracketed_paste: false,
            show_cursor: true,
            insert: false,
            alternate_scroll: true,
            mouse: None,
        }
    }
}
