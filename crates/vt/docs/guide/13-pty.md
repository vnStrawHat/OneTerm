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

## The shape of a session

```rust,ignore
// `ignore`: this block names `oneterm_vt::pty`, which does not exist in a
// `--no-default-features` build, and spawning a real child process is not
// something a doctest should do.
use std::io::Read;
use std::sync::Arc;
use oneterm_vt::pty::{
    ChildEvent, EventedPty, EventedReadWrite, Options, PseudoConsole, WindowSize,
    PTY_CHILD_EVENT_TOKEN, PTY_READ_WRITE_TOKEN,
};

let mut options = Options::default();
// TERM and COLORTERM belong here. This crate never touches the calling
// process's own environment.
options.env.insert("TERM".into(), "xterm-256color".into());

let size = WindowSize { rows: 24, cols: 80, cell_width: 8, cell_height: 17 };
let mut pty = PseudoConsole::spawn(&options, size)?;

let poller = Arc::new(polling::Poller::new()?);
// Safety: the sources must outlive their registration, which the owner thread
// guarantees by dropping the console before the poller.
unsafe {
    pty.register(&poller, polling::Event::readable(PTY_READ_WRITE_TOKEN), polling::PollMode::Level)?;
}

let mut events = Vec::new();
let mut buf = [0u8; 8192];
loop {
    events.clear();
    poller.wait(&mut events, None)?;
    for event in &events {
        match event.key {
            PTY_READ_WRITE_TOKEN => {
                let read = pty.reader().read(&mut buf)?;
                // ... term.feed(&buf[..read], &mut batch, Instant::now())
                let _ = read;
            }
            PTY_CHILD_EVENT_TOKEN => {
                if let Some(ChildEvent::Exited(status)) = pty.next_child_event() {
                    let _ = status;
                    return Ok(());
                }
            }
            _ => {}
        }
    }
}
```

Two tokens, because child exit must be observable without reading: on Unix that
is race-free `SIGCHLD` handling, on Windows a wait callback. A child that exits
while you are blocked on a read would otherwise never be noticed.

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
one on Unix (a reaper that turns child exit into a pollable event). All are
joined on drop and none of them calls into your code. They are the only threads
this crate spawns, which is why turning the feature off makes the engine's "no
threads, no locks, no interior mutability" claim literally true rather than
nearly true.

**Dropping a pseudo-console is an action, not a release.** It closes the
console, waits a bounded grace period for the child to exit, and terminates that
child if it never does. The drop therefore blocks, and it belongs on an owner
thread rather than on a UI thread. Deregister the sources from the poller before
dropping the poller, not after.

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
