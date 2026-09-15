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

// `DA1` is answered, so a program's capability probe gets a reply.
term.feed(b"\x1b[c", &mut batch, Instant::now());
assert!(batch.iter().any(|event| matches!(event, VtEvent::Reply(_))));

// `DA3` is a gap: parsed, counted, and never answered with a guess.
let stats = term.feed(b"\x1b[=c", &mut batch, Instant::now());
assert_eq!(stats.unhandled_sequences, 1);
assert!(!batch.iter().any(|event| matches!(event, VtEvent::Reply(_))));
```

## Supported

**C0 and C1.** `BEL`, `BS` (with reverse wrap), `HT`, `LF`, `VT`, `FF`, `CR`,
`SO`, `SI`, `SUB` and `DEL`. An 8-bit C1 byte is executed, never treated as a
sequence introducer.

**ESC.** `ESC 7` and `ESC 8` (`DECSC` / `DECRC`), `ESC =` and `ESC >` (keypad
mode), `ESC c` (`RIS`), `ESC D` (`IND`), `ESC E` (`NEL`), `ESC H` (`HTS`),
`ESC M` (`RI`), `ESC Z` (`DECID`), `ESC # 8` (`DECALN`), `ESC \` (`ST`), and the
charset designators `ESC ( ) * +` with `B` (ASCII) and `0` (line drawing).

**CSI.** Cursor motion `A B C D E F G H I Z a b d e f`; erase `J K X`; insert and
delete `@ L M P`; scroll `S T`; `DECSTBM` (`r`); save and restore (`s u` without
intermediates); tab control `g` and `W`; `SGR` (`m`) including 256-colour and
truecolour, colon sub-parameters, and the underline styles; `DSR` (`n`), also in
its private form; `DECRQM` (`$ p`) and its private form; `DECSTR` (`! p`);
`DA1`, `DA2` (`c`, `> c`); `XTVERSION` (`> q`); cursor style (`SP q`); window
operations (`t`); `modifyOtherKeys` (`> 4 m`); the kitty keyboard stack (`? u`,
`= u`, `> u`, `< u`); and `REP` (`b`), whose source character survives
intervening escape sequences.

**Modes.** Private: `1` application cursor keys, `3` column mode, `6` origin,
`7` autowrap, `12` cursor blink, `25` cursor visibility, `45` reverse wrap, `47`
/ `1047` / `1049` alternate screen, `1000` / `1002` / `1003` mouse reporting,
`1004` focus reporting, `1005` / `1006` mouse encoding, `1007` alternate scroll,
`1042` urgency, `1048` save cursor, `2004` bracketed paste, `2026` synchronised
output. ANSI: `4` insert mode, `20` newline mode.

**OSC.** The eighteen in `OscRoutes::BUILTIN`: `0`, `1`, `2` (title and icon
name), `4` and `104` (indexed colours), `7` (working directory), `8`
(hyperlinks), `9` (notification and ConEmu progress), `10`, `11`, `12`, `110`,
`111`, `112` (dynamic colours and their resets), `22` (pointer shape), `50`
(cursor shape), `52` (clipboard), `133` (shell integration). Every other number
is `Drop` by default and available to you by route; chapter 5.

**DCS.** Sixel, as `DCS q`. The intermediate bytes are part of the routing key,
so `DCS $ q` (`DECRQSS`) and `DCS + q` (`XTGETTCAP`) -- which share the final
byte -- are counted unhandled rather than fed to the image decoder.

**Unicode.** Grapheme clusters, east-asian width, and wide-glyph spacer cells.
`width::cluster_width` and `width::scalar_width` are the engine's own answers and
are published so a renderer can agree with the grid about how wide something is.

## Recognised but inert

Two modes are accepted so that a stream setting them is not noise, and do
nothing:

- `? 2027`, grapheme cluster mode. The width logic it would select is
  implemented and tested; nothing reads it yet.
- `? 9001`, win32 input mode. The Windows console host sends it unprompted, so
  accepting it silently is better than counting it as unhandled; the encoding is
  not implemented.

`DECRQM` answers "not supported" for both.

## Known gaps

Verified absent in the current tree. None of them is hard; each is simply not
done.

| Gap | What a program sees |
| --- | --- |
| Mouse mode `? 9` (X10) | the mode is not recognised, so a program asking only for X10 reporting gets no mouse events |
| Mouse encoding `? 1015` (urxvt) | not recognised; programs fall back to SGR or the legacy encoding |
| `DECSCNM`, `? 5` (reverse video) | not recognised; a program that inverts the screen this way sees nothing happen |
| `LS2` / `LS3` / `SS2` / `SS3` | `G2` and `G3` can be designated but never selected, so a stream that loads and then shifts to them prints the wrong glyphs |
| `? 2027` wiring | see above: recognised and inert |
| `OSC 17` / `OSC 19` (selection colours) | counted unhandled |
| `DA3` (`CSI = c`) | counted unhandled; `DA1` and `DA2` are answered |

**Pending, and not yet merged into this tree.** The list above is written
against the engine as it stands. A separate piece of work closes every one of
those seven gaps; when it lands, this section is the one that changes, and the
gaps table above should shrink to whatever remains. Treat the table as current
and this paragraph as the notice that it is expected to.

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

There is no published `esctest` pass count. Saying "we pass N of M" without a
pinned harness and a recorded run is a number that decays the moment anybody
quotes it, and this chapter would rather say what is implemented and what is
not.
