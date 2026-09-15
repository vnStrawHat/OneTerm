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

// `DECRQCRA` is a gap: parsed, counted, and never answered with a guess.
let stats = term.feed(b"\x1b[1;1;1;1;1;1*y", &mut batch, Instant::now());
assert_eq!(stats.unhandled_sequences, 1);
assert!(!batch.iter().any(|event| matches!(event, VtEvent::Reply(_))));
```

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
`= u`, `> u`, `< u`); and `REP` (`b`), whose source character survives
intervening escape sequences.

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
byte -- are counted unhandled rather than fed to the image decoder.

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
| `DECRQCRA` (`CSI * y`), the checksum report | counted unhandled. This is also why there is no `esctest` score below: that harness reads the screen back by asking for rectangle checksums, so without this sequence it cannot run at all |
| `DECRQSS` (`DCS $ q`) | parsed and counted unhandled, never answered. A program asking the terminal to report a setting back gets silence rather than a wrong answer |
| `XTGETTCAP` (`DCS + q`) | the same. Clients such as tmux and neovim use it to probe capabilities and fall back when it goes unanswered |
| No way to refuse `? 2027` on a `WcsWidth` session | described above: there is no `Config` flag, so an embedder who opens a ConPTY in `WcsWidth` cannot tell the engine to ignore the mode |

## How conformance is checked

Three layers, none of which is a claim about a published score:

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

There is no `esctest` pass count, and the reason is mechanical rather than a
matter of taste: `esctest` reads the screen back by asking the terminal for
rectangle checksums with `DECRQCRA`, which this engine does not implement, so
the harness cannot run at all. That gap is the first row of the table above. Add
`DECRQCRA` and the score becomes measurable; until then a number here would be
invented, and this chapter would rather say what is implemented and what is not.
