# 5. OSC routing: extending the terminal

An Operating System Command is how a program asks the terminal for something
that is not a glyph: a title, a colour, a working directory, a clipboard write,
a hyperlink, a progress bar. Every terminal implements a fixed set of them, and
in the two reference cores that set is a compile-time `match` -- so supporting
one more number means forking the crate.

Here the set is data. The engine decides only **what to do with a number**,
never who handles it, and the decision is two bits per number in `OscRoutes`.
That table is the crate's extension point, and it costs nothing at run time: the
routing decision is three bit tests, no embedder code runs inside `feed`, and
nothing is allocated per sequence.

## The four routes

| Route | The engine's own handler | `VtEvent::Osc` |
| --- | --- | --- |
| `Builtin` | runs, and emits its typed event | not delivered |
| `BuiltinAndForward` | runs, and emits its typed event | delivered as well, after it |
| `Forward` | skipped | delivered |
| `Drop` | nothing happens | not delivered; counted as unhandled |

The default for a number the engine implements is `Builtin`; the default for
every other number is `Drop`. The twenty the engine implements are published as
`OscRoutes::BUILTIN` -- `0, 1, 2, 4, 7, 8, 9, 10, 11, 12, 17, 19, 22, 50, 52,
104, 110, 111, 112, 133` -- and `OscRoutes::has_builtin` answers for one number,
so a route can never be dead on arrival:

```rust
use oneterm_vt::{OscRoute, OscRoutes};

assert!(OscRoutes::has_builtin(52));
assert!(!OscRoutes::has_builtin(7501));

let mut routes = OscRoutes::new();
assert_eq!(routes.get(52), OscRoute::Builtin);   // implemented: runs
assert_eq!(routes.get(7501), OscRoute::Drop);    // unknown: dropped

routes.route(7501, OscRoute::Forward);
assert_eq!(routes.get(7501), OscRoute::Forward);
```

Asking for `Builtin` on a number the engine does not implement is a debug
assertion, because the route would silently mean `Drop` and you would never find
out. It is an assertion rather than a panic so that a table built from
configuration data cannot kill a release build.

## Size ceilings are separate from routes

A payload is held inline up to 2 KiB. A number you have explicitly claimed
`large` may spill to 8 MiB; everything past the ceiling is truncated, the event
carries `truncated: true`, and `FeedStats::truncated_osc` counts it. Buy the
ceiling per number, so a hostile stream cannot spend 8 MiB under a number nobody
reads.

**Route first, then raise the ceiling.** The `large` call reads the table as it
stands and debug-asserts that the number is not `Drop`, because accumulating
megabytes only to discard them is a memory hazard with no benefit:

```rust
use oneterm_vt::{OscRoute, OscRoutes};

let mut routes = OscRoutes::new();
routes.route(7501, OscRoute::Forward).large(7501, true);
// A ceiling is orthogonal to the route: this buys a large clipboard write
// while OSC 52 stays a built-in, because who may write the clipboard is
// your policy and not the engine's.
routes.large(52, true);

assert!(routes.allows_large(7501));
assert_eq!(routes.get(52), OscRoute::Builtin);
```

## Worked example 1: a number the engine has never heard of

OneTerm reads `OSC 7501`, the Program Status Protocol, a cross-vendor proposal
rather than a terminal standard. The engine does not parse it: adding it is one
route and one `match` arm, and the protocol's rules stay in the embedder:

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, OscRoute, OscRoutes, Size, Terminal, VtEvent};

const PROGRAM_STATUS: u32 = 7501;

let mut routes = OscRoutes::new();
// Above the engine's 2048-entry bitmap, so it lands in the sorted spill list;
// `large` because the protocol allows a 4096-byte sequence and the inline cap
// is 2 KiB.
routes.route(PROGRAM_STATUS, OscRoute::Forward).large(PROGRAM_STATUS, true);

let mut term = Terminal::new(
    Size { rows: 24, cols: 80 },
    Config { osc_routes: routes, ..Config::default() },
);
let mut batch = EventBatch::new();
term.feed(b"\x1b]7501;state=working:app=build\x1b\\", &mut batch, Instant::now());

let mut seen = 0;
for event in batch.iter() {
    if let VtEvent::Osc { code: PROGRAM_STATUS, params, truncated, .. } = event {
        // Parameter 0 is the number itself, so the payload starts at 1.
        let fields: Vec<&[u8]> = batch.params(*params).skip(1).collect();
        assert_eq!(fields, [b"state=working:app=build".as_slice()]);
        assert!(!*truncated);
        seen += 1;
    }
}
assert_eq!(seen, 1);
```

No engine change, no fork, no patch section in your manifest. The grammar, the
record table and the probe reply all stay in your crate, where the knowledge of
the protocol already lives.

## Worked example 2: wrapping a built-in

Suppose your application also accepts a private sub-code of a built-in, say
`OSC 9 ; 7 ; <payload>`. `OSC 9` is a built-in -- a desktop notification -- so a
clean `Builtin` route would parse that sequence as a notification whose body is
`7;<payload>`. `BuiltinAndForward` is exactly the escape hatch for that: keep the
engine's handling, see the raw bytes too, and drop the notification whose
forwarded first parameter is `7`.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, OscRoute, OscRoutes, Size, Terminal, VtEvent};

let mut routes = OscRoutes::new();
routes.route(9, OscRoute::BuiltinAndForward).large(9, true);

let mut term = Terminal::new(
    Size { rows: 24, cols: 80 },
    Config { osc_routes: routes, ..Config::default() },
);
let mut batch = EventBatch::new();
term.feed(b"\x1b]9;7;eyJ2IjoxfQ==\x07", &mut batch, Instant::now());

// Both arrive, the typed event first.
let mut notification = None;
let mut private_subcode = false;
for event in batch.iter() {
    match event {
        VtEvent::Notification { body, .. } => notification = Some(batch.str(*body)),
        VtEvent::Osc { code: 9, params, .. } => {
            private_subcode = batch.params(*params).nth(1) == Some(b"7".as_slice());
        }
        _ => {}
    }
}
assert_eq!(notification, Some("7;eyJ2IjoxfQ=="));
assert!(private_subcode, "so the notification is suppressed by your own code");
```

It is one line in your adapter and it is ugly, which is honest: the private
knowledge stays in the crate that owns it, and the engine keeps a clean
`OSC 9`. Note the general property behind it -- routes are per number, and an
OSC sub-code is payload, not a route. Per-sub-code routing was considered and
rejected: it would double the table, and the two-event wrap already covers the
case.

Overriding a built-in outright is the third shape: `route(n, OscRoute::Forward)`
skips the engine's handler and delivers only the raw parameters, so you can
implement `OSC 7` or `OSC 52` yourself without touching this crate.

## `OSC 7`, and why the engine stops early

`Cwd` is worth its own paragraph because it is the event most likely to be
mishandled. The engine percent-decodes the path and strips the leading slash of
a `file:///C:/...` drive URL, and stops there. It builds no path, consults no
current directory, checks no host and touches no filesystem. The `host` field is
the URL's authority, verbatim and undecoded; the `path` field is the decoded
path, and a payload that was not a `file://` URL at all is reported there
verbatim.

A remote shell sending `file://evil/../../etc` is data. Deciding whether to
trust it -- and whether a directory reported by a machine on the other side of
an SSH connection means anything on yours -- is yours, and the engine gives you
the undigested bytes so that you can.

## What the table is not

`OscRoutes` is read when a `Terminal` is built and is not live: `Terminal::config`
hands out a shared reference and there is no setter. Changing a route mid-session
means building a new terminal. That is deliberate -- a route that could change
under a half-parsed sequence is a race the engine has no way to describe.
