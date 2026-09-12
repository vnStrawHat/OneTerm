//! `oneterm-pty` — the pseudo-console transport.
//!
//! A child process behind a pseudo-console, exposed as a **passive pollable
//! object**: this crate never runs a read loop, owns no grid and knows nothing
//! about VT parsing. The caller drives its own `polling::Poller`
//! (`crates/local-shell/src/event_loop.rs`) and reads the PTY when the poller
//! says it is readable.
//!
//! Platforms:
//!
//! - **Windows** — ConPTY. The bundled `conpty.dll` sitting next to the
//!   executable is preferred and `kernel32!CreatePseudoConsole` is the fallback;
//!   that order is [`DEC-0013`](../../../docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md)
//!   and it is load-bearing, because the inbox `conhost.exe` swallows Sixel DCS
//!   payloads. See [`windows::conpty`].
//! - **Unix** — `openpty` plus a reaper thread that turns child exit into a
//!   pollable event.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md`.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;

use polling::{Event, PollMode, Poller};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use crate::unix::{PseudoConsole, SignalMask};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use crate::windows::{PipeReader, PipeWriter, PseudoConsole};

/// Poll key for child-process events (exit).
///
/// Public on **both** platforms on purpose: the value used to be `pub(crate)` on
/// Unix, which forced every consumer to hard-code it.
pub const PTY_CHILD_EVENT_TOKEN: usize = 1;

/// Poll key for pseudo-console reads and writes.
pub const PTY_READ_WRITE_TOKEN: usize = 2;

/// How the console host is asked to measure character widths.
///
/// The host and the terminal engine must agree, otherwise columns drift on CJK
/// and emoji. The flag is fixed at spawn: a program that turns mode 2027 on
/// mid-session keeps the width mode it was spawned with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphWidth {
    /// `wcswidth` semantics — what OneTerm's engine does today.
    #[default]
    WcsWidth,
    /// Measure whole grapheme clusters (mode 2027).
    Graphemes,
}

/// The program a pseudo-console starts.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Shell {
    program: String,
    args: Vec<String>,
}

impl Shell {
    pub fn new(program: String, args: Vec<String>) -> Self {
        Self { program, args }
    }
}

/// Everything [`PseudoConsole::spawn`] needs.
///
/// `drain_on_exit` is deliberately absent: it only ever configured the read loop
/// this crate does not have.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Options {
    /// The program to run. `None` selects the platform default.
    pub shell: Option<Shell>,
    /// Startup directory of the child.
    pub working_directory: Option<PathBuf>,
    /// Environment entries applied on top of the parent environment.
    ///
    /// This crate never touches the *calling* process's environment: `TERM` and
    /// `COLORTERM` belong here, not in a process-global `set_var`.
    pub env: HashMap<String, String>,
    /// Width mode requested from the console host.
    pub glyph_width: GlyphWidth,
    /// Signal mask applied in the child before `exec`.
    ///
    /// Capture it on a thread where terminal signals are unblocked
    /// ([`SignalMask::current`]); a child spawned from a worker thread otherwise
    /// inherits that thread's blocked mask and never sees Ctrl-C.
    #[cfg(unix)]
    pub child_signal_mask: Option<SignalMask>,
    /// Escape the shell arguments with the C-runtime rules before they are
    /// joined into one `CreateProcessW` command line.
    #[cfg(windows)]
    pub escape_args: bool,
}

/// Grid size plus the cell metrics the embedder owns.
///
/// `cell_width` / `cell_height` are pixels; they reach the child as the
/// `TIOCSWINSZ` pixel fields on Unix and are unused by ConPTY.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowSize {
    pub rows: u16,
    pub cols: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

/// Something that happened to the child process.
#[derive(Debug, PartialEq, Eq)]
pub enum ChildEvent {
    /// The child is gone. `None` means the platform watcher could not read an
    /// exit code — the session is over either way.
    Exited(Option<ExitStatus>),
}

/// A pollable read/write pair.
///
/// Associated types rather than `impl Trait`: the trait stays object-safe and
/// the reader type is nameable by the caller.
pub trait EventedReadWrite {
    type Reader: io::Read;
    type Writer: io::Write;

    /// # Safety
    ///
    /// The registered sources must outlive their registration in the `Poller`.
    unsafe fn register(
        &mut self,
        poller: &Arc<Poller>,
        interest: Event,
        mode: PollMode,
    ) -> io::Result<()>;

    fn reregister(
        &mut self,
        poller: &Arc<Poller>,
        interest: Event,
        mode: PollMode,
    ) -> io::Result<()>;

    fn deregister(&mut self, poller: &Arc<Poller>) -> io::Result<()>;

    fn reader(&mut self) -> &mut Self::Reader;
    fn writer(&mut self) -> &mut Self::Writer;
}

/// An [`EventedReadWrite`] that also reports what its child process did.
///
/// Separate from the read/write half because child exit must be observable
/// without reading: on Unix that is race-free `SIGCHLD` handling, on Windows a
/// wait callback.
pub trait EventedPty: EventedReadWrite {
    /// The next pending child event, or `None`.
    fn next_child_event(&mut self) -> Option<ChildEvent>;
}

/// Tell the child its window changed.
pub trait OnResize {
    /// A failing resize returns an error instead of panicking: the session is
    /// still usable at the old size (`docs/agents/error-policy.md`).
    fn on_resize(&mut self, size: WindowSize) -> io::Result<()>;
}

#[cfg(test)]
mod loopback_tests;
