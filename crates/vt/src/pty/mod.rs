//! The pseudo-console transport: a child process behind a ConPTY on Windows or
//! an `openpty` on Unix, exposed as a **passive pollable object**.
//!
//! Gated by the `pty` cargo feature, which is **on by default**. Turn it off and
//! this module, its three traits and every platform dependency disappear, while
//! the engine is unaffected: [`Terminal::feed`](crate::Terminal::feed) takes
//! bytes, and bytes from a socket, a file or a test vector are indistinguishable
//! to it. That, not this module, is the seam for an embedder who already owns a
//! process model.
//!
//! Nothing here runs a read loop, owns a grid or knows anything about VT
//! parsing. The embedder owns a `polling::Poller`, registers the console through
//! [`EventedReadWrite`], and reads it when the poller says it is readable; child
//! exit arrives separately through [`EventedPty::next_child_event`], because it
//! must be observable without reading.
//!
//! Passive while it lives — **dropping** a pseudo-console is an action with an
//! external side effect. It closes the console, waits a bounded grace period for
//! the child to exit, and terminates that child if it never does. The drop
//! therefore blocks, and belongs on an owner thread rather than on a UI thread.
//!
//! # Platforms
//!
//! Both halves are described here in prose on purpose: `PseudoConsole` is a
//! different type on each platform, the two share the trait set rather than an
//! inherent API, and `cargo doc` renders only the half that matches the host.
//! Portable code goes through [`EventedPty`] and [`OnResize`]; anything else is
//! platform code.
//!
//! - **Windows** — ConPTY. A `conpty.dll` next to the *running executable* is
//!   preferred and `kernel32!CreatePseudoConsole` is the fallback. That order is
//!   load-bearing and this crate ships no console host: an embedder who does not
//!   place a matched `conpty.dll` and `x64\OpenConsole.exe` pair beside their own
//!   executable gets the inbox `conhost.exe`, which swallows Sixel DCS payloads.
//!   The loader resolves the path at run time, so only the embedder's own build
//!   can put the pair there. Windows-only items: `PipeReader` and `PipeWriter`.
//! - **Unix** — `openpty` plus one reaper thread that turns child exit into a
//!   pollable event. Unix-only items: `SignalMask` and the
//!   `Options::child_signal_mask` field.
//!
//! Two threads exist inside the transport on Windows (a pipe reader and a pipe
//! writer) and one on Unix (the reaper). All are internal, all are joined on
//! drop, and none calls into embedder code. They are the only threads this crate
//! spawns, which is why `--no-default-features` leaves the engine's "no threads,
//! no locks, no interior mutability" guarantee literally true.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md>.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;

use polling::{Event, PollMode, Poller};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use crate::pty::unix::{PseudoConsole, SignalMask};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use crate::pty::windows::{PipeReader, PipeWriter, PseudoConsole};

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
    /// The program and the arguments it is started with, verbatim.
    ///
    /// Nothing is quoted or split here: on Windows the arguments are joined
    /// into one command line at spawn, under the Windows-only `Options::escape_args`.
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
    /// Visible rows.
    pub rows: u16,
    /// Visible columns.
    pub cols: u16,
    /// Width of one cell in pixels, or 0 when the embedder does not measure.
    pub cell_width: u16,
    /// Height of one cell in pixels, or 0 when the embedder does not measure.
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
    /// Where the child's output arrives. Reads are non-blocking only in the
    /// sense that the poller says when there is something to read.
    type Reader: io::Read;
    /// Where input for the child goes.
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

    /// Change the interest an already-registered source is polled with.
    fn reregister(
        &mut self,
        poller: &Arc<Poller>,
        interest: Event,
        mode: PollMode,
    ) -> io::Result<()>;

    /// Take the sources back out of the poller. Call it before dropping the
    /// poller, not after.
    fn deregister(&mut self, poller: &Arc<Poller>) -> io::Result<()>;

    /// The read half, for when the poller reports [`PTY_READ_WRITE_TOKEN`]
    /// readable.
    fn reader(&mut self) -> &mut Self::Reader;
    /// The write half.
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
    // The repository rule behind this is `docs/agents/error-policy.md`.
    /// A failing resize returns an error instead of panicking: the session is
    /// still usable at the old size.
    fn on_resize(&mut self, size: WindowSize) -> io::Result<()>;
}

#[cfg(test)]
mod loopback_tests;
