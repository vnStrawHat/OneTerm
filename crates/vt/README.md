# oneterm-vt

An embeddable terminal core: a VT parser, a grid with scrollback, reflow, selection,
damage-tracked snapshots, Sixel, and an optional pseudo-console transport. You bring the
pixels.

## Install

The crate is **not on crates.io**. Depend on it by git, pinned to a commit:

```toml
[dependencies]
oneterm-vt = { git = "https://github.com/vnStrawHat/OneTerm", rev = "<commit sha>" }
```

Use the sha of the merge commit you want; `git log --oneline -- crates/vt` on the repository lists
the ones that changed the engine.

To track the tip instead, `branch = "main"`. That builds whatever landed this morning, which is not
what you want in a build you expect to reproduce.

**Tags do not work yet.** Every existing tag predates this crate, and the crate inherits the
application's version, so the first tag that can carry it is the next release, `v0.5.3` or later.
From that release on, `tag = "v0.5.3"` is the form to prefer, and the semver promise and
[`CHANGELOG.md`](CHANGELOG.md) apply to tags exactly as they would to published releases: a tag
that changes what you compile against is a minor bump with an entry naming the item.

The whole repository is checked out by a git dependency, not just this directory, so expect the
first build to fetch a few tens of megabytes.

## What it is not

- **Not a renderer.** It produces a snapshot of cells, styles and runs. Nothing here draws.
- **A PTY you can turn off.** The engine itself never spawns a process and never reads a file
  descriptor -- you feed it bytes. A pseudo-console transport ships beside it in
  `oneterm_vt::pty`, behind the default-on `pty` feature; turn the feature off and the engine
  is all that is left.
- **Not a window.** No event loop, no platform event handling, no clipboard, no window title bar.
  `input` will turn a key press or a mouse click into the bytes the program expects, but you
  deliver the event and you own every side effect it has on your own view.
- **Not a policy.** Whether a program may write your clipboard, open a URL or set your window
  title is your decision; the engine hands you the request and stays out of it.

It also holds no lock, allocates no globals and never panics on input: a malformed or hostile
stream is dropped, truncated or degraded to a documented fallback, and counted in `FeedStats`.
It spawns no thread either -- the only threads in this crate are the transport's own, behind
the `pty` feature.

## Dependencies

The whole tree, with the transport turned off -- `cargo tree -p oneterm-vt -e normal
--no-default-features`:

```text
oneterm-vt
|-- bitflags
|-- log
|-- memchr
|-- rustc-hash
|-- unicode-segmentation
`-- unicode-width
```

Six leaf crates, no build script, no proc macro, and no `unsafe` block in the engine. That build is
a CI target, so the number above cannot drift.

The default build adds the `pty` feature's three: `polling` everywhere, plus `windows-sys` on
Windows or `libc` on Unix. That comes to 8 direct dependencies and 16 crates in the tree on
`x86_64-pc-windows-msvc`, and 8 direct and 11 crates on `x86_64-unknown-linux-gnu`.

## Quick start

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, SnapshotState, Size, Terminal, VtEvent};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();

// Feed bytes from wherever they come from: a PTY, a socket, a test fixture.
term.feed(b"\x1b]0;a title\x07hello", &mut batch, Instant::now());

for event in batch.iter() {
    match event {
        // Payloads are spans into the batch, so nothing is copied per event.
        VtEvent::Title(span) => assert_eq!(batch.str(*span), "a title"),
        // Bytes the terminal owes the program: write them back to its input.
        VtEvent::Reply(span) => { let _answer = batch.bytes(*span); }
        _ => {}
    }
}

// Drawing is a pull, not a push: ask when you want to, and get back only the
// rows whose content changed since this `SnapshotState` last asked.
let mut state = SnapshotState::new();
let _update = term.snapshot_update(&mut state, Instant::now());
assert_eq!(state.rows().len(), 24);
```

A longer version, with OSC forwarding and the screen printed as text, is
[`examples/headless.rs`](examples/headless.rs):

```console
$ cargo run --example headless
```

## Extending it: OSC

The engine answers a fixed set of OSC numbers itself (title, colours, the working directory, the
clipboard, and the rest of `OscRoutes::BUILTIN`), each as a typed event. `OscRoutes` says what
happens to every other number, and lets you override or observe a built-in:

| Route | What the engine does |
| --- | --- |
| `Builtin` | its own handler runs and emits its typed event; the default for a number it implements |
| `BuiltinAndForward` | the handler runs **and** the raw parameters follow as `VtEvent::Osc` |
| `Forward` | the handler is skipped; only `VtEvent::Osc` is delivered |
| `Drop` | parsed, counted, discarded; the default for everything else |

```rust
use oneterm_vt::{Config, OscRoute, OscRoutes, Size, Terminal};

let mut routes = OscRoutes::new();
// An application protocol of your own, delivered raw and allowed to spill
// past the 2 KiB inline cap to 8 MiB.
routes.route(31337, OscRoute::Forward).large(31337, true);
// Keep the engine's OSC 9 handling and see the bytes as well.
routes.route(9, OscRoute::BuiltinAndForward);

let term = Terminal::new(
    Size { rows: 24, cols: 80 },
    Config { osc_routes: routes, ..Config::default() },
);
assert_eq!(term.config().osc_routes.get(31337), OscRoute::Forward);
```

Supporting a new OSC number is that one call plus a `match` arm on the event. This crate needs no
change, and neither does anything else in your program. The payload ceiling is bought per number
and is independent of the route, so a hostile stream cannot spend 8 MiB under a number nobody
reads.

## Identity

`XTVERSION` and `DA2` tell the program inside the terminal what it is talking to. Set
`Config::product_name` so they name your product rather than this engine:

```rust
use oneterm_vt::Config;

let config = Config {
    product_name: Some("MyTerm(1.4.0)".into()),
    ..Config::default()
};
assert!(config.product_name.is_some());
```

## Features

| Feature | Default | What it adds |
| --- | --- | --- |
| `pty` | **on** | `oneterm_vt::pty`: a child process behind a ConPTY (Windows) or an `openpty` (Unix), as a passive pollable object. Adds `polling`, plus `windows-sys` or `libc`. `--no-default-features` removes the module, the three transport traits and all three dependencies. |
| `vt-paranoid` | off | A whole-history integrity walk after every `feed` and `resize`. Milliseconds per call at a large scrollback: for tests and fuzzing, never for a release build. |
| `regex` | off | `search::SearchPattern::Regex`, so scrollback search takes a compiled regular expression as well as a literal. |

No feature changes behaviour -- only availability. Two add dependencies: `pty` (on by default) adds
`polling`, plus `windows-sys` on Windows or `libc` on Unix; `regex` pulls in the `regex` crate, and
with it `aho-corasick`, `regex-automata` and `regex-syntax`. Turn both off -- `regex` already is --
and you are back to the six the Dependencies section lists.

**On Windows the `pty` feature ships no console host.** The transport prefers a `conpty.dll` found
next to the *running executable* and falls back to the inbox `conhost.exe`, which swallows Sixel DCS
payloads. If you want images in a local shell, place a matched `conpty.dll` and
`x64\OpenConsole.exe` pair beside your own executable -- the loader resolves the path at run time,
so only your build can put it there. OneTerm does this in about twenty lines of `build.rs`;
`crates/app/build.rs` in this repository is the worked example.

`polling` is this crate's **only public dependency**, and only under `pty`: `polling::Poller`,
`Event` and `PollMode` appear in the `EventedReadWrite` signatures, so a `polling` major bump is a
breaking change here and gets a `CHANGELOG.md` entry naming both versions.

## Documentation

- The API reference: `cargo doc -p oneterm-vt --no-deps --open`. Every public item is documented,
  and the crate builds with `#![warn(missing_docs)]` so it stays that way. There is no docs.rs
  page, because the crate is not published.
- An embedder's guide is planned, and will render beside the API reference rather than living
  somewhere else. It does not exist yet; this line will say where it is when it does.
- The full design, including why the grid, damage and reflow work the way they do:
  <https://github.com/vnStrawHat/OneTerm/tree/main/docs/spec-intakes/IN-0029-vt-engine>.

## Compatibility

The minimum supported Rust version is **1.96.0**. Raising it is a minor version bump with a
changelog entry, never a patch. CI builds on the pinned toolchain rather than on the MSRV, so
treat the number as a statement of intent that is checked by hand, not by a job.

The crate is `0.x`, and the semver promise it keeps is written at the top of
[`CHANGELOG.md`](CHANGELOG.md).

## Licence

Apache-2.0. The text is in [`LICENSE`](LICENSE) beside this file, with [`NOTICE`](NOTICE).
