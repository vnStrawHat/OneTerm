//! The modes the renderer reads, snapshotted so paint never takes the lock.
//!
//! A painter reads `DECCKM` at paint time, and alt-screen and mouse state
//! about ten times per frame. Copying them into the snapshot state on **every**
//! update, including one that returns `Unchanged`, is what lets an embedder
//! paint without taking its engine lock a second time.

// Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
// section "Mode snapshot" (R-17). The mode table itself lives in
// `terminal::mode`; this is only what leaves it.

/// `? 9` / `? 1000` / `? 1002` / `? 1003`: how much motion the host asked for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseReporting {
    /// `? 9`: button press only, and never a modifier.
    X10,
    /// `? 1000`: press and release.
    Normal,
    /// `? 1002`: press, release and motion while a button is down.
    ButtonEvent,
    /// `? 1003`: every motion.
    AnyEvent,
}

/// `? 1005` / `? 1006` / `? 1015`: how a report is encoded.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum MouseEncoding {
    /// X10 coordinates, the power-on encoding.
    #[default]
    Default,
    /// `? 1005`.
    Utf8,
    /// `? 1006`.
    Sgr,
    /// `? 1015`: the legacy values as decimal parameters, so a coordinate past
    /// 223 survives.
    Urxvt,
}

/// One composite instead of five booleans the embedder has to recombine.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MouseProtocol {
    /// Which events the program asked to be told about.
    pub reporting: MouseReporting,
    /// How a report is encoded on the wire.
    pub encoding: MouseEncoding,
}

/// Everything the view reads that is not a cell, a cursor or a selection.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ModeSnapshot {
    /// Whether the alternate screen is the current one (`? 1049`, `? 47`).
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
    /// `? 5`, DECSCNM: the whole screen is drawn with the default foreground
    /// and background swapped. A screen-level flag, never a cell attribute, so
    /// the embedder swaps the two defaults when it resolves the palette and no
    /// cell's own style changes.
    pub reverse_video: bool,
    /// `None` when the host asked for no mouse reporting at all.
    pub mouse: Option<MouseProtocol>,
}

impl Default for ModeSnapshot {
    /// The power-on state: the cursor is visible and the alternate scroll
    /// translation is on, which is what `xterm` starts with.
    fn default() -> ModeSnapshot {
        ModeSnapshot {
            alt_screen: false,
            app_cursor: false,
            app_keypad: false,
            bracketed_paste: false,
            show_cursor: true,
            insert: false,
            alternate_scroll: true,
            reverse_video: false,
            mouse: None,
        }
    }
}
