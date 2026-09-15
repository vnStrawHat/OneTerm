# 13. The pseudo-console

A terminal core that cannot open a terminal is a surprise, so the transport
ships in the crate, behind the `pty` feature, **on by default**. It is a child
process behind a ConPTY on Windows or an `openpty` on Unix, exposed as a passive
pollable object.

Nothing in it runs a read loop, owns a grid, or knows anything about VT parsing.
You own the poller, the read buffer and the thread they live on.

## Three ways to have a transport

**Take the one in the box.** Do nothing: the feature is on, `oneterm_vt::pty` is
there, and it costs `polling` plus `windows-sys` or `libc`.

**Bring your own, through the traits.** Implement `EventedReadWrite`,
`EventedPty` and `OnResize` over whatever you already have -- an SSH channel, a
container exec stream, a test harness, a replay of a recording. Nothing in the
engine names the concrete type; `feed` takes bytes, and bytes from a socket and
bytes from a console are indistinguishable to it.

**Switch it off.** `--no-default-features` removes the module, the three traits
and all three dependencies, leaving the engine and its six leaf crates. That
build has no platform code in it and spawns no thread at all. It is a CI target,
so the number cannot drift.

The real seam for an embedder who already owns a process model is not the traits
-- it is `feed`. The traits are there so that code written against the bundled
transport does not have to be rewritten when you replace it.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal};

// Whatever your transport is, this is all the engine ever sees of it.
fn pump(term: &mut Terminal, batch: &mut EventBatch, chunk: &[u8]) {
    term.feed(chunk, batch, Instant::now());
}

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();
pump(&mut term, &mut batch, b"from a socket");
pump(&mut term, &mut batch, b", a file, or a fixture");

let top = term.viewport().top;
assert_eq!(term.row_text(top).trim_end(), "from a socket, a file, or a fixture");
```

## You bring `polling`

The transport is registered with a `polling::Poller` that **you** construct, and
this crate does not re-export `polling`. Add it to your own manifest at the same
major version:

```toml
[dependencies]
polling = "3"
```

It has to be the same major, because `Poller`, `Event` and `PollMode` appear in
the `EventedReadWrite` signatures: two different majors are two different types
and they will not unify. That is what chapter 12 means when it calls `polling`
this crate's only public dependency.

A `pub use polling;` here would save you the line, and it is deliberately not
offered: it would be a new public item in this crate's surface, permanently,
carrying a semver promise about somebody else's crate. Naming the version in
your own manifest is one line and leaves you in control of it.

## The shape of a session

```rust,no_run
// `no_run`: compiled on every build that has the `pty` feature, so it cannot
// drift from the API, but never executed -- it spawns a child process and then
// loops forever.
use std::io::Read;
use std::sync::Arc;

use oneterm_vt::pty::{
    ChildEvent, EventedPty, EventedReadWrite, Options, PseudoConsole,
    PTY_CHILD_EVENT_TOKEN, PTY_READ_WRITE_TOKEN, WindowSize,
};
use polling::{Event, Events, PollMode, Poller};

fn run() -> std::io::Result<()> {
    let mut options = Options::default();
    // TERM and COLORTERM belong here. This crate never touches the calling
    // process's own environment.
    options.env.insert("TERM".into(), "xterm-256color".into());

    let size = WindowSize { rows: 24, cols: 80, cell_width: 8, cell_height: 17 };
    let mut pty = PseudoConsole::spawn(&options, size)?;

    let poller = Arc::new(Poller::new()?);
    // SAFETY: the registered sources must outlive their registration, which
    // this thread guarantees by deregistering before it drops either.
    unsafe {
        pty.register(
            &poller,
            Event::readable(PTY_READ_WRITE_TOKEN),
            PollMode::Level,
        )?;
    }

    let mut events = Events::new();
    let mut buf = [0u8; 8192];
    loop {
        events.clear();
        poller.wait(&mut events, None)?;
        for event in events.iter() {
            match event.key {
                PTY_READ_WRITE_TOKEN => {
                    let read = pty.reader().read(&mut buf)?;
                    // ... term.feed(&buf[..read], &mut batch, Instant::now())
                    let _ = read;
                }
                PTY_CHILD_EVENT_TOKEN => {
                    if let Some(ChildEvent::Exited(status)) = pty.next_child_event() {
                        let _ = status;
                        pty.deregister(&poller)?;
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}
# let _ = run;
```

Two tokens, because child exit must be observable without reading: on Unix that
is race-free `SIGCHLD` handling, on Windows a wait callback. A child that exits
while you are blocked on a read would otherwise never be noticed.

Writing to the child is the same object: `EventedReadWrite::writer` is where
every `VtEvent::Reply` goes, and where you send the bytes `input::encode_key`
returns and any answer you choose to give a `ClipboardLoad` or a `ColorQuery`.

Resizing is the other half, and it is one call: `OnResize::on_resize` with the
same numbers you gave `Terminal::resize`. It returns an error rather than
panicking when it fails, because the session is still usable at the old size.
`cell_width` and `cell_height` are pixels; they reach the child as the pixel
fields of the Unix window size and are unused by ConPTY.

## Threading

The transport is evented, not async. There is no runtime, no executor and no
`Future` anywhere in this crate: you call `poller.wait`, and you read when it
says there is something to read.

Internally it runs two threads on Windows (a pipe reader and a pipe writer) and
one on Unix (a reaper that turns child exit into a pollable event). None of them
calls into your code, and they are the only threads this crate spawns, which is
why turning the feature off makes the engine's "no threads, no locks, no
interior mutability" claim literally true rather than nearly true.

**None of them is joined, and drop does not wait for them.** Be exact about
this, because shutdown ordering gets built on it:

- the Windows pipe threads are parked in a blocking read or write and return
  only when the pipe breaks, so there is no join to perform -- the handle is
  dropped at spawn;
- the Unix reaper owns the child handle and deliberately outlives the drop,
  which is how a child exit stays observable while the owner is tearing down.

Each thread holds only what it was given and none of it is yours, so a thread
still running after `drop` returns cannot touch your memory. But do not write
code that assumes the process has no more threads of this crate in it the
instant `drop` returns, because it does.

**Dropping a pseudo-console is an action, not a release.** It closes the
console, waits a bounded grace period for the *child* to exit -- not for those
threads -- and terminates the child if it never does. The drop therefore blocks,
and it belongs on an owner thread rather than on a UI thread. Deregister the
sources from the poller before dropping the poller, not after.

## Windows: the console host you have to ship

The crate ships **no console host**. On Windows the transport prefers a
`conpty.dll` found next to the *running executable* and falls back to the inbox
`conhost.exe`. That order is load-bearing, because the inbox host swallows Sixel
DCS payloads: with it, images in a local shell simply never arrive at the
engine.

If you want them, place a matched pair beside your own executable:

```text
  your-app.exe
  conpty.dll
  x64\OpenConsole.exe
```

They must be a matched pair from the same console package -- a `conpty.dll` with
a mismatched `OpenConsole.exe` fails to start a session at all. The loader
resolves the path at run time, which is why only your own build can put them
there and why this crate cannot do it for you. About twenty lines of `build.rs`
that copy them next to the binary is the whole job, and OneTerm's own app crate
is a worked example of it.

Neither file is needed on Unix, and neither is needed on Windows if you do not
care about images.

## Platform differences

`PseudoConsole` is a different type on each platform. The two share the trait
set rather than an inherent API, and `cargo doc` renders only the half that
matches the host you built on, so portable code goes through `EventedPty` and
`OnResize` and anything else is platform code.

- **Windows only**: `PipeReader`, `PipeWriter`, and `Options::escape_args`,
  which applies the C-runtime quoting rules before the arguments are joined into
  one command line.
- **Unix only**: `SignalMask`, and `Options::child_signal_mask`. Capture the
  mask on a thread where terminal signals are unblocked: a child spawned from a
  worker thread otherwise inherits that thread's blocked mask and never sees
  Ctrl-C.

`Shell::new(program, args)` takes both verbatim -- nothing is quoted or split
here -- and `Options::shell` of `None` selects the platform default.
