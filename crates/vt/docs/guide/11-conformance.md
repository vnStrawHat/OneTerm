# 11. Conformance: what is supported, and what is not

This chapter is a statement of fact about the engine as it stands, not a
roadmap. The rule it follows is the one that matters most to a program running
inside a terminal: **never claim a capability that does not exist.** A `DECRQM`
query about a mode the engine recognises but does not act on answers "not
supported" rather than "set", because answering "set" tells a program a feature
is there and then breaks it.

Everything the engine parses but does not implement is counted in
`FeedStats::unhandled_sequences` instead of being silently ignored, so a rising
count on a real workload is how you find the gap that matters to you.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();

// `DA1`, `DA2` and `DA3` are all answered, so a capability probe gets a reply.
for probe in [b"\x1b[c".as_slice(), b"\x1b[>c", b"\x1b[=c"] {
    term.feed(probe, &mut batch, Instant::now());
    assert!(batch.iter().any(|event| matches!(event, VtEvent::Reply(_))));
}

// `DECRQCRA` reads the screen back, so it answers only when you opened the
// gate. Shut — the default — it is counted and answers nothing.
let stats = term.feed(b"\x1b[1;1;1;1;1;1*y", &mut batch, Instant::now());
assert_eq!(stats.unhandled_sequences, 1);
assert!(!batch.iter().any(|event| matches!(event, VtEvent::Reply(_))));
```

## The screen-readback gate

`DECRQCRA` (`CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y`) reports a checksum of a
rectangle of the screen. That is how a conformance harness reads the screen
back, and it is also how a program running inside the terminal could read back
text it did not write. It is off unless you turn it on:

```rust
use oneterm_vt::Config;

let config = Config { allow_screen_readback: true, ..Config::default() };
assert!(config.allow_screen_readback);
assert!(!Config::default().allow_screen_readback);
```

xterm gates the same sequence behind `allowWindowOps` and WezTerm behind
`enable_checksum_rectangular_area`. Two things narrow it further here, whichever
way the flag is set: the checksum covers the **visible screen only**, never the
scrollback, so scrolled-off history is not reachable through it; and a rectangle
outside the grid is clamped rather than refused, so a probe cannot use an
out-of-range request to learn anything either.

### The checksum variant, pinned

**One variant is implemented and none is negotiated** (there is no `CSI Ps * x`
here). It is the one xterm reaches with `checksumExtension: 7` — the positive
sum of **every** Unicode scalar value in each cell, masked to 16 bits, with
**no** attribute contribution, **no** negation, and **no** trimming of trailing
blanks. An unwritten or erased cell counts as `U+0020`.

"Every scalar" is the part worth spelling out: a grapheme cluster contributes
its base character *and* each combining mark, which is what xterm's `combData`
walk adds while `csBYTE` is clear. `e` + `U+0301` in one cell is
`0x65 + 0x301`, not `0x65`.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

let mut term = Terminal::new(
    Size { rows: 4, cols: 8 },
    Config { allow_screen_readback: true, ..Config::default() },
);
let mut batch = EventBatch::new();

fn ask(term: &mut Terminal, batch: &mut EventBatch, request: &[u8]) -> String {
    batch.clear();
    term.feed(request, batch, Instant::now());
    let reply = batch
        .iter()
        .find_map(|event| match event {
            VtEvent::Reply(span) => Some(batch.bytes(*span)),
            _ => None,
        })
        .expect("the gate is open, so DECRQCRA answers");
    String::from_utf8_lossy(reply).into_owned()
}

term.feed(b"A", &mut batch, Instant::now());

// One cell over `A`, U+0041. Four upper-case hex digits, the label echoed back.
assert_eq!(ask(&mut term, &mut batch, b"\x1b[1;0;1;1;1;1*y"), "\x1bP1!~0041\x1b\\");

// A blank cell is U+0020, not zero and not skipped.
assert_eq!(ask(&mut term, &mut batch, b"\x1b[1;0;1;2;1;2*y"), "\x1bP1!~0020\x1b\\");

// The attributes contribute nothing: the same text answers the same number.
term.feed(b"\x1b[1;4;7;31mA\x1b[0m", &mut batch, Instant::now());
assert_eq!(ask(&mut term, &mut batch, b"\x1b[1;0;1;1;1;1*y"), "\x1bP1!~0041\x1b\\");
```

**A program written against xterm's default will disagree with this engine.**
xterm's own default negates the total and folds the video attributes into each
cell's value; this one does neither. The variant was chosen because it is the
only one `esctest` scores without a per-cell correction, and because a negated
16-bit total is the single most common place an implementation and a harness
silently disagree. No program other than a test harness is known to send
`DECRQCRA` at all, so the cost is recorded rather than hedged against.

One difference from xterm survives even at that extension, and it is in the
rectangle rather than the sum: xterm's `validRect` **rejects** a rectangle that
falls outside the page, where this engine clamps it to the page. Only a
partially outside rectangle can tell the two apart — a wholly outside one
answers `0000` either way. Under `DECOM` the page is the scrolling region, so
the rectangle is clamped to the region's rows and not merely offset into them,
which is what xterm's `minRectRow` / `maxRectRow` do.

## Supported

**C0 and C1.** `BEL`, `BS` (with reverse wrap), `HT`, `LF`, `VT`, `FF`, `CR`,
`SO`, `SI`, `SUB` and `DEL`. An 8-bit C1 byte is executed, never treated as a
sequence introducer.

**ESC.** `ESC 7` and `ESC 8` (`DECSC` / `DECRC`), `ESC =` and `ESC >` (keypad
mode), `ESC c` (`RIS`), `ESC D` (`IND`), `ESC E` (`NEL`), `ESC H` (`HTS`),
`ESC M` (`RI`), `ESC Z` (`DECID`), `ESC # 8` (`DECALN`), `ESC \` (`ST`), the
charset designators `ESC ( ) * +` with `B` (ASCII) and `0` (line drawing), and
the shifts that make the last two of those printable: `LS2` (`ESC n`) and `LS3`
(`ESC o`) invoke `G2` or `G3` until something else does, `SS2` (`ESC N`) and
`SS3` (`ESC O`) for exactly one printed character. A pending single shift is
consumed by the next printed character and by nothing else, so an intervening
escape sequence does not eat it.

**CSI.** Cursor motion `A B C D E F G H I Z a b d e f`; erase `J K X`; insert and
delete `@ L M P`; scroll `S T`; `DECSTBM` (`r`); save and restore (`s u` without
intermediates); tab control `g` and `W`; `SGR` (`m`) including 256-colour and
truecolour, colon sub-parameters, and the underline styles; `DSR` (`n`), also in
its private form; `DECRQM` (`$ p`) and its private form; `DECSTR` (`! p`);
`DA1`, `DA2`, `DA3` (`c`, `> c`, `= c`); `XTVERSION` (`> q`); cursor style (`SP q`); window
operations (`t`); `modifyOtherKeys` (`> 4 m`); the kitty keyboard stack (`? u`,
`= u`, `> u`, `< u`); `DECRQCRA` (`* y`), behind the gate described above; and
`REP` (`b`), whose source character survives intervening escape sequences.

**Modes.** Private: `1` application cursor keys, `5` reverse video, `6` origin,
`7` autowrap, `9` X10 mouse, `12` cursor blink, `25` cursor visibility, `45`
reverse wrap, `47` / `1047` / `1049` alternate screen, `1000` / `1002` / `1003`
mouse reporting, `1004` focus reporting, `1005` / `1006` / `1015` mouse
encoding, `1007` alternate scroll, `1042` urgency, `1048` save cursor, `2004`
bracketed paste, `2026` synchronised output, `2027` grapheme clustering. ANSI:
`4` insert mode. Three more are parsed and stored but read by nothing; see
"Recognised but inert" below.

`? 5` (`DECSCNM`) reaches you as `ModeSnapshot::reverse_video`, a screen-level
flag and never a cell attribute: **you** swap the two defaults when you resolve
the palette, no cell's own style changes, and `? 5 l` therefore restores exactly
what was there.

**OSC.** The twenty in `OscRoutes::BUILTIN`: `0`, `1`, `2` (title and icon
name), `4` and `104` (indexed colours), `7` (working directory), `8`
(hyperlinks), `9` (notification and ConEmu progress), `10`, `11`, `12`, `110`,
`111`, `112` (dynamic colours and their resets), `17` and `19` (the selection
background and foreground, set and queried the way `10` and `11` are, one
parameter each rather than xterm's advancing multi-parameter form; there is no
`OSC 117` / `119` reset and `RIS` clears them), `22` (pointer shape), `50`
(cursor shape), `52` (clipboard), `133` (shell integration). Every other number
is `Drop` by default and available to you by route; chapter 5.

**DCS.** Sixel, as `DCS q`. The intermediate bytes are part of the routing key,
so `DCS $ q` (`DECRQSS`) and `DCS + q` (`XTGETTCAP`) -- which share the final
byte -- open a query rather than the image decoder. Both are answered:

- **`DECRQSS`** reports `m` (`SGR`), `r` (`DECSTBM`), `SP q` (`DECSCUSR`),
  `" q` (`DECSCA`) and `" p` (`DECSCL`) as `DCS 1 $ r <value><setting> ST`.
  Everything else gets `DCS 0 $ r ST`, the invalid reply. The list is short on
  purpose: `DECSLRM`, `DECSASD`, `DECSACE`, `DECSCPP` and `DECSNLS` describe
  features this engine does not have, and answering them would break the rule
  at the top of this chapter.
- **`XTGETTCAP`** answers from a table compiled into the crate -- the engine
  reads no terminfo database, no environment variable and no file. Each
  requested name gets its own reply, `DCS 1 + r <hex name> = <hex value> ST`
  when it is known and `DCS 0 + r <hex name> ST` when it is not. `TN` reports
  `Config::product_name`'s name half when you set one, and `xterm-256color`
  otherwise. The table covers the terminal name, `colors`, the truecolour flags
  and setters, styled underlines, `OSC 52` clipboard write, the cursor-style
  pair, the alternate screen and ten basic motion and erase capabilities.

Both are bounded against a hostile stream: at most 16 names per `XTGETTCAP`
request and 128 bytes per name, with the remainder dropped and counted rather
than truncated; odd-length or non-hex input answered unknown rather than
partially decoded; and a query payload past 8 KiB -- larger than any answerable
request -- answered with nothing and counted. A request that is not hex is not
echoed back at all, because the echo is spliced into a DCS reply.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();

fn ask(term: &mut Terminal, batch: &mut EventBatch, request: &[u8]) -> String {
    batch.clear();
    term.feed(request, batch, Instant::now());
    let mut out = String::new();
    for event in batch.iter() {
        if let VtEvent::Reply(span) = event {
            out.push_str(&String::from_utf8_lossy(batch.bytes(*span)));
        }
    }
    out
}

// A fresh terminal's SGR is a plain `0` -- not an empty answer.
assert_eq!(ask(&mut term, &mut batch, b"\x1bP$qm\x1b\\"), "\x1bP1$r0m\x1b\\");
// The scrolling region, 1-based and inclusive.
assert_eq!(ask(&mut term, &mut batch, b"\x1bP$qr\x1b\\"), "\x1bP1$r1;24r\x1b\\");
// A setting the engine does not have: the honest refusal.
assert_eq!(ask(&mut term, &mut batch, b"\x1bP$qs\x1b\\"), "\x1bP0$r\x1b\\");

// `XTGETTCAP` for `544e` (`TN`), hex in and hex out.
assert_eq!(
    ask(&mut term, &mut batch, b"\x1bP+q544e\x1b\\"),
    "\x1bP1+r544e=787465726D2D323536636F6C6F72\x1b\\",
);
```

**Unicode.** East-asian width, wide-glyph spacer cells, and two width rules the
stream chooses between. With `? 2027` reset -- the power-on state -- width is
decided per scalar by `width::scalar_width`, which is what most terminals do: a
ZWJ family emoji lands as a base plus a zero-width tail. Set `? 2027` and the
print path segments its run into grapheme clusters and measures each with
`width::cluster_width`, so that family lands in one cell. Both functions are
published, because an embedder measuring its own text has to make the same
choice the grid made.

Three things about `? 2027` are worth knowing before you turn it on.

**A cluster split across a `feed` is still measured whole.** The engine carries
the last cluster of a printed run and re-places it when the next run extends it.
Any dispatch that is not a print breaks the carry, and so does a resize.

**The carry is bounded at 32 scalars.** Past that the cluster is not carried, a
continuation arriving in a later `feed` starts a cluster of its own, and
`FeedStats::dropped_cluster_carries` counts it -- **once per over-long cluster,
never once per scalar**, so the number counts offending clusters and is not a
rate. The bound is what stops a stream that feeds one unbounded cluster a scalar
at a time from making the re-placing quadratic.

**A presentation selector needs a base.** A cluster of combining scalars alone --
a stray `VS16`, a leading combining mark, the tail of a keycap split in front of
its selector -- is zero-width and joins the cell on its left, exactly as it is
with the mode reset. `VS16` widens an emoji base; it does not widen nothing.

On Windows there is a caveat the engine cannot enforce. A ConPTY session opened
with `pty::GlyphWidth::WcsWidth` has already told the console host to measure by
`wcswidth`, so a program that then sets `? 2027` gets cluster measurement here
and scalar measurement there, and the two disagree about where the cursor is.
There is no `Config` flag to refuse the mode on such a session; if you open one
in `WcsWidth`, know that a `? 2027` stream can desynchronise it.

## Recognised but inert

Three modes are accepted so that a stream setting one is not noise, and then do
nothing. The rule that governs all three is the one at the top of this chapter:
**`DECRQM` never answers `Set` for a mode nothing reads.** An inert mode answers
`NotSupported` when its state is not even stored, and `Reset` when it is stored
and simply unread -- so a program probing for the capability is told the truth
and falls back, instead of being told it has something it has not.

| Mode | `DECRQM` answers | Why it is inert |
| --- | --- | --- |
| `? 3`, `DECCOLM` | `NotSupported` (`0`) | both `h` and `l` act on the screen, but the column count never changes, so "not supported" is the honest answer about the capability |
| `? 9001`, win32 input | `Reset` (`2`) | the Windows console host sends `? 9001 h` unprompted, so accepting it silently beats counting it unhandled; the input encoding it selects is not implemented |
| `20`, `LNM` (an ANSI mode) | `Reset` (`2`) | the state is tracked and read by nothing: `LF` never implies `CR` here |

A mode leaves that table on the day something reads it, and the answer changes
with it. The doctest below pins all three, so this section cannot drift away
from the engine the way it did once already.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

fn decrqm(term: &mut Terminal, batch: &mut EventBatch, query: &[u8]) -> String {
    term.feed(query, batch, Instant::now());
    let reply = batch
        .iter()
        .find_map(|event| match event {
            VtEvent::Reply(span) => Some(batch.bytes(*span)),
            _ => None,
        })
        .expect("DECRQM is always answered");
    String::from_utf8_lossy(reply).into_owned()
}

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();

// `CSI ? Ps $ p` -> `CSI ? Ps ; state $ y`. State 0 is "not recognised,
// do not use it", 1 is set, 2 is reset.
assert_eq!(decrqm(&mut term, &mut batch, b"\x1b[?3$p"), "\x1b[?3;0$y");
assert_eq!(decrqm(&mut term, &mut batch, b"\x1b[?9001$p"), "\x1b[?9001;2$y");
// `LNM` is an ANSI mode, so the query and the answer carry no `?`.
assert_eq!(decrqm(&mut term, &mut batch, b"\x1b[20$p"), "\x1b[20;2$y");

// A mode something reads answers its real state, for contrast.
assert_eq!(decrqm(&mut term, &mut batch, b"\x1b[?25$p"), "\x1b[?25;1$y");
```

## Known gaps

Verified absent in the current tree. None of them is hard; each is simply not
done.

| Gap | What a program sees |
| --- | --- |
| **Left-right margins** (`DECSLRM`, `DECLRMM`, and the `DECSACE` rectangle modes that need them) | counted unhandled. This is the largest single gap, and it is what the `esctest` groups below fail on |
| `DECRQSS` for `DECSLRM`, `DECSASD`, `DECSACE`, `DECSCPP`, `DECSNLS` | the invalid reply, `DCS 0 $ r ST`. Deliberate: the engine does not have those features, and reporting a value would claim a capability that does not exist |
| `CSI Ps * x` (`XTERM_CHECKSUM`), the runtime checksum-variant selector | counted unhandled. One variant is implemented and pinned above; xterm has a selector because it had its own history to reconcile |
| `DA1` claims VT220 (`? 62`), not VT420 | a harness testing at VT420 level will find level-4 features absent, correctly. `DECSCL` reports the same level, so the two answers agree |
| No way to refuse `? 2027` on a `WcsWidth` session | described above: there is no `Config` flag, so an embedder who opens a ConPTY in `WcsWidth` cannot tell the engine to ignore the mode |

## How conformance is checked

Four layers. Three are this repository's own; the fourth is an outside
harness's, and none of them is enforced as a threshold:

- **A frozen parity corpus.** Recorded byte streams from real programs, replayed
  through the engine, with the resulting grid compared against a stored
  expectation. It is what stops a refactor changing behaviour nobody noticed
  was load-bearing.
- **Property tests.** Randomised sequences asserted against invariants rather
  than against expected output: row identity is stable, anchors stay inside the
  grid, no feed panics.
- **A whole-history integrity walk**, behind the `vt-paranoid` feature. It
  re-checks every row of both screens after every `feed` and `resize`, which
  costs milliseconds per call at a large scrollback and is therefore a test and
  fuzzing tool, never a release build.

- **`esctest`**, the outside harness. It drives the engine through a pty and
  reads the screen back with `DECRQCRA` rectangle checksums, which is why it
  could not run here at all until that sequence existed. It runs on a Linux CI
  job with `--expected-terminal xterm --xterm-checksum 334 --max-vt-level 4`,
  and its log is published as a build artifact.

**The `esctest` job is a report and never a gate**, which is a design decision
rather than a convenience. The harness tests a VT420-level terminal; this engine
claims VT220 in `DA1` and has no left-right margins, so several groups are
expected to fail for reasons that are deliberate and listed in the table above.
A pass threshold would turn a capability map into a quality score and would fail
the build for choices this project made on purpose.

So read the artifact as a map of what the engine does, not as a grade — and when
a group fails, check the table above before assuming it is a defect.
