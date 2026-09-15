# oneterm-vt

An embeddable terminal core: a VT parser, a grid with scrollback, reflow, selection,
damage-tracked snapshots and Sixel. You bring the pixels and the process.

## Install

The crate is **not on crates.io**. Depend on it by git:

```toml
[dependencies]
oneterm-vt = { git = "https://github.com/vnStrawHat/OneTerm", tag = "v0.5.2" }
```

Pin a `tag` or a `rev`. `branch = "main"` builds whatever landed this morning, which is not what
you want in a build you expect to reproduce. The semver promise and
[`CHANGELOG.md`](CHANGELOG.md) apply to tags exactly as they would to published releases: a tag
that changes what you compile against is a minor bump with an entry naming the item.

## What it is not

- **Not a renderer.** It produces a snapshot of cells, styles and runs. Nothing here draws.
- **Not a PTY.** It never spawns a process and never reads a file descriptor. You feed it bytes.
- **Not a window.** No event loop, no input handling, no clipboard, no window title bar.
- **Not a policy.** Whether a program may write your clipboard, open a URL or set your window
  title is your decision; the engine hands you the request and stays out of it.

It also holds no lock, spawns no thread, allocates no globals and never panics on input: a
malformed or hostile stream is dropped, truncated or degraded to a documented fallback, and
counted in `FeedStats`.

## Dependencies

The whole tree, with default features:

```text
oneterm-vt
|-- bitflags
|-- log
|-- memchr
|-- rustc-hash
|-- unicode-segmentation
`-- unicode-width
```

Six leaf crates, no build script, no proc macro, and no `unsafe` block in the library itself.

## Quick start

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, RenderState, Size, Terminal, VtEvent};

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
// rows whose content changed since this `RenderState` last asked.
let mut state = RenderState::new();
let _update = term.render_update(&mut state, Instant::now());
assert_eq!(state.rows().len(), 24);
```

A longer version, with OSC forwarding and the screen printed as text, is
[`examples/headless.rs`](examples/headless.rs):

```console
$ cargo run --example headless
```

## Extending it: OSC

The engine answers a fixed set of OSC numbers itself (title, colours, clipboard, and the rest of
`OscClaims::NATIVE`). Every other number is delivered to you as `VtEvent::Osc`, but only if you
claim it, so a hostile stream cannot buy an 8 MiB payload under a number nobody reads:

```rust
use oneterm_vt::{Config, OscClaims, Size, Terminal};

let mut claims = OscClaims::new();
claims.claim(7);              // OSC 7: the shell's working directory.
claims.claim_large(20308);    // An application protocol of your own, allowed to spill.

let term = Terminal::new(
    Size { rows: 24, cols: 80 },
    Config { osc_claims: claims, ..Config::default() },
);
assert!(term.config().osc_claims.is_claimed(7));
```

Supporting a new OSC number is that one call plus a `match` arm on the event. This crate needs no
change, and neither does anything else in your program.

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
| `vt-paranoid` | off | A whole-history integrity walk after every `feed` and `resize`. Milliseconds per call at a large scrollback: for tests and fuzzing, never for a release build. |

No feature adds a dependency today, and no feature changes behaviour -- only availability.

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
