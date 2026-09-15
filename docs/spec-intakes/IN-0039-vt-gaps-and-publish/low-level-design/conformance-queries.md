# Low-Level Design: conformance queries

Intake: [`IN-0039`](../IN-0039.md)
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: conformance-queries
Date: 2026-09-15

> One concern per file: the three queries that let an outside program -- and an outside test
> harness -- read this engine's state back, and the `esctest` run they unblock.

## Concern

Three sequences are parsed, counted unhandled and never answered:

| Sequence | Name | Why it matters |
| --- | --- | --- |
| `CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y` | `DECRQCRA`, request checksum of rectangular area | it is `esctest`'s only screen-readback channel, so without it there is no third-party conformance number for this engine at all |
| `DCS $ q <setting> ST` | `DECRQSS`, request status string | a program asks the terminal to report a setting back; silence makes it guess |
| `DCS + q <hex names> ST` | `XTGETTCAP`, request terminfo capability | tmux and neovim probe capabilities with it |

`BUG-0058` already routed the two DCS forms correctly -- they no longer open the Sixel decoder --
so the parsing half is done and the answering half is not.

## Design: the DCS payload buffer

`dcs_put` today feeds `self.state.graphics.parser` and discards the byte when there is none. Both
`DECRQSS` and `XTGETTCAP` need the complete payload before they can answer, so `dcs_hook` gains a
second thing it can open.

```rust
/// What `dcs_hook` opened, if anything. Exactly one at a time: the parser is a
/// single state machine and a DCS cannot nest.
enum DcsSink {
    Sixel(SixelParser),
    /// `DCS $ q` (DECRQSS) or `DCS + q` (XTGETTCAP). The payload is short --
    /// a DECRQSS request is two or three bytes, an XTGETTCAP request a handful
    /// of hex pairs -- and is bounded by the parser's existing DCS_MAX_BYTES.
    Query { kind: QueryKind, payload: Vec<u8> },
}
```

Rules, all of which fall out of mechanics that already exist:

1. **The ceiling is the existing one.** `parser::DCS_MAX_BYTES` already aborts an over-long DCS and
   `dcs_unhook(aborted: true)` already runs. The payload buffer inherits both; it adds no new
   limit and no new counter.
2. **An abort discards.** `CAN`, `SUB`, an over-long payload, or a `dcs_unhook(aborted)` for any
   other reason: the buffer is dropped and nothing is answered. `FeedStats::aborted_dcs` already
   counts this.
3. **The buffer is cleared at `RIS` and at `DECSTR`**, with the rest of the terminal state. A DCS
   can not be in flight across either, but the field is reset with its neighbours so a future
   reader does not have to prove that.
4. **One allocation, reused.** The `Vec<u8>` lives on the terminal and is `clear()`ed rather than
   dropped, so a program polling `XTGETTCAP` in a loop allocates once.
5. **`dcs_hook`'s routing key stays (intermediates, final byte)**, which is what `BUG-0058`
   established. `DCS q` with no intermediate is Sixel; `DCS $ q` and `DCS + q` open a query; every
   other DCS is still counted unhandled.

## Design: `DECRQCRA`

### The sequence

```text
CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y
```

| Parameter | Meaning | Default |
| --- | --- | --- |
| `Pid` | a label the requester chooses; echoed back verbatim | 0 |
| `Pp` | page number. This engine has one page | 0, and ignored |
| `Pt` | top row, 1-based | 1 |
| `Pl` | left column, 1-based | 1 |
| `Pb` | bottom row, 1-based | the last row |
| `Pr` | right column, 1-based | the last column |

The reply is xterm's, not DEC's `DECCKSR` spelling of it:

```text
DCS <Pid> ! ~ <four upper-case hex digits> ST
```

Verified against xterm's own source (`charproc.c`, `CASE_DECRQCRA`): `checksum &= 0xffff`, the
delimiter is `!~`, the parameter radix is 16, and `Pid` is echoed unchanged. Four digits, so a
checksum of 32 is `0020`.

### The checksum, and the quirk

xterm's `xtermCheckRect` (`screen.c`) is configurable, and the configuration bits are the quirk.
From `ptyx.h`:

```c
/* bit-assignments for extensions to DECRQCRA, to omit DEC features */
typedef enum {
    csDEC = 0
    ,csPOSITIVE = xBIT(0)  /* do not negate the result */
    ,csATTRIBS  = xBIT(1)  /* do not report the VT100 video attributes */
    ,csNOTRIM   = xBIT(2)  /* do not omit checksum for blanks */
    ,csDRAWN    = xBIT(3)  /* do not skip uninitialized cells */
    ,csBYTE     = xBIT(4)  /* do not mask cell value to 8 bits or ignore combining chars */
} CSBITS;
```

xterm's **default** is `csDEC` -- negate the total, add the video attributes into each cell's value
(`PROTECTED +0x4`, `INVISIBLE +0x8`, `UNDERLINE +0x10`, `INVERSE +0x20`, `BLINK +0x40`,
`BOLD +0x80`), trim trailing blanks, and skip cells that were never drawn. The definition moved
twice: patch **#334** made blank and empty cells count alike, and patch **#336** restored the DEC
VT520's calculation as the default, with `checksumExtension` factoring out the differences.

**This engine implements one variant and never negotiates one.** The chosen variant is the one
xterm reaches with `checksumExtension: 7` -- `csPOSITIVE | csATTRIBS | csNOTRIM`:

```text
checksum = (sum over every cell in the rectangle of the cell's first Unicode scalar value) & 0xffff
```

with an unwritten or erased cell counting as `U+0020`, no attribute contribution, no negation and
no trimming.

Four reasons, in order of weight:

1. **It is the only variant `esctest` can score without a correction.** `escutil.py`'s
   `AssertScreenCharsInRectEqual` compares a one-cell checksum against `ord(char)` directly, and
   applies the `0x10000 - actual` inverse only under `--xterm-checksum < 279`. `esc.py`'s `empty()`
   returns a space rather than `NUL` under `--xterm-checksum >= 334`. Both conditions are satisfied
   by this variant and by no other, so the harness runs with
   `--expected-terminal xterm --xterm-checksum 334` and needs no per-terminal special case.
2. **No sign convention to get wrong.** A negated 16-bit total is the single most common place an
   implementation and a harness silently disagree.
3. **The attribute contributions are xterm's private numbering.** They are not in any DEC document
   and they encode xterm's own attribute bit layout; reproducing them would mean reproducing a
   foreign internal representation for no reader's benefit.
4. **WezTerm reaches the same answer** for the ASCII case (positive sum, low byte, blank fudged to
   32), which is the other implementation an evaluator is likely to compare against.

Recorded as a cost, plainly: **a program written against xterm's default will disagree with this
engine.** No such program is known -- `DECRQCRA` is a test-harness primitive -- and the guide says
so in the sentence next to the number. No runtime selector (`CSI Ps * x`, xterm's
`XTERM_CHECKSUM`) is implemented: xterm has one because it had history to reconcile, and this
engine has none.

### The gate

```rust
// crates/vt/src/terminal/config.rs
pub struct Config {
    // ...
    /// Whether `DECRQCRA` (`CSI * y`) may answer. **Default `false`.**
    ///
    /// The sequence reports a checksum of a rectangle of the screen, which is
    /// how a conformance harness reads the screen back -- and how a program
    /// running inside the terminal could read back text it did not write.
    /// xterm gates it behind `allowWindowOps` and WezTerm behind
    /// `enable_checksum_rectangular_area`; this is the same gate.
    ///
    /// With this `false` the sequence is counted in
    /// `FeedStats::unhandled_sequences` and answers nothing, which is exactly
    /// what the engine does today.
    pub allow_screen_readback: bool,
}
```

Default `false` is **status quo preserving**: with it false, `US-0106` changes no byte OneTerm's
own adapter can observe, and the packet's whole user-visible risk is a `Config` field nobody sets.
`crates/terminal` does **not** set it. The `esctest` harness does, which is where the capability is
wanted.

This is IN-0039's Open Decision 4 and the owner has the final say; the design proceeds on `false`
because the reversible mistake is shipping it off.

### Bounds and hostile input

- The rectangle is clamped to the grid before it is walked: `Pt`, `Pl`, `Pb`, `Pr` are saturating,
  a reversed rectangle (`Pb < Pt`) is empty rather than an underflow, and a rectangle larger than
  the screen is the screen.
- The walk is over the **visible screen only**, never the scrollback. `DECRQCRA` addresses page
  memory, and one page here is one screen. A program cannot use it to read scrolled-off history.
- The walk is `O(rows x cols)` of the clamped rectangle and allocates nothing.
- `DECOM` (origin mode) **is** honoured: the rows are relative to the scrolling region while it is
  set, which is the bug Contour had to fix and is one row in the test table.

## Design: `DECRQSS`

```text
DCS $ q <setting> ST   ->   DCS 1 $ r <setting bytes> ST   (valid)
                       ->   DCS 0 $ r ST                   (invalid or unsupported)
```

The reply repeats the request's own final bytes after the value, which is what makes it
round-trippable: a program sends `DCS $ q m ST` and gets back the `SGR` parameters plus `m`.

| Request | Answers with | Source of the value |
| --- | --- | --- |
| `m` (SGR) | `0` plus every non-default attribute of the current pen, in xterm's canonical order, then `m` | the cell template |
| `r` (`DECSTBM`) | `<top> ; <bottom> r`, 1-based | the scrolling region |
| `SP q` (`DECSCUSR`) | `<id> SP q` where the id is the 1-6 encoding of shape plus blink | `Terminal::cursor_style()` |
| `" q` (`DECSCA`) | `<0 or 1> " q` | the protected-attribute state of the cell template |
| `" p` (`DECSCL`) | `<level> ; <8-bit flag> " p`, reporting the level `DA1` already claims | fixed, from the same constant `DA1` uses |
| anything else | `DCS 0 $ r ST` | -- |

Exactly six arms and a default. The list is bounded on purpose: `DECSLRM`, `DECSASD`, `DECSACE`,
`DECSCPP` and `DECSNLS` describe features this engine does not have, and answering them would be
the "claim a capability that does not exist" failure guide chapter 11 forbids. They take the
invalid reply, which is the honest one.

**The SGR round trip is the packet's hardest test and its best one:** feed an SGR sequence, ask for
it back, feed the answer into a second terminal, and assert the two cell templates are equal.
That is a property, not a golden string, and it catches an ordering or a default-omission bug that
a hand-written expectation would not.

## Design: `XTGETTCAP`

```text
DCS + q <hex name> [; <hex name>]... ST
  ->  DCS 1 + r <hex name> = <hex value> [; ...] ST    (known)
  ->  DCS 0 + r <hex name> ST                          (unknown)
```

Names and values are hex, two upper-case digits per byte, semicolon-separated. A request may carry
several names; **each name is answered in its own reply**, which is what xterm does and what tmux
parses.

The table, compiled into the crate as a `&[(&str, &str)]` sorted by name:

| Capability | Value | Why it is in the minimal set |
| --- | --- | --- |
| `TN` (terminal name) | `Config::product_name`'s name half, or `xterm-256color` | the first thing every prober asks |
| `Co` / `colors` | `256` | tmux decides its colour handling on it |
| `RGB` | present, empty value | the de-facto "I do truecolour" flag |
| `Tc` | present, empty value | tmux's own spelling of the same |
| `setrgbf` / `setrgbb` | the `38:2::` and `48:2::` forms | what a prober uses when `RGB` is absent |
| `Su` | present, empty value | styled underlines, which the engine has |
| `Ms` | the `OSC 52` form | clipboard write, which the engine routes |
| `smcup` / `rmcup` | `CSI ? 1049 h` / `l` | the alternate screen |
| `bel`, `cr`, `cud1`, `cuu1`, `cub1`, `cuf1`, `home`, `clear`, `el`, `ed` | the sequences the engine implements | the smallest set that lets a prober conclude the terminal is real |

**Bounded and counted, both hard rules:**

- **At most 16 names per request.** Past that the remainder is dropped and the reply carries the
  first 16. A request is a hex string, and a hostile one can be 8 KiB of semicolons.
- **At most 128 bytes per requested name.** A longer name cannot match any table entry, so it is
  answered `DCS 0 + r ...` with the name echoed truncated, or dropped if truncation would make the
  echo a lie -- the design chooses **dropped**, and the reply omits it.
- **Odd-length or non-hex input** is not a match and is answered as unknown. No panic, no partial
  decode.
- **Every dropped name and every over-long request increments `FeedStats::unhandled_sequences`**,
  so a prober hitting a ceiling is visible rather than silent.
- The table is `const` and the lookup is a linear scan over about twenty entries. No map, no
  allocation, no `lazy_static`.

The engine **never reads a terminfo database, an environment variable or a file.** The table is the
answer.

## The `esctest` run plan

Report-only, on the Linux CI job, and never a gate. This is
[`IN-0029/low-level-design/testing-and-bench.md`](../../IN-0029-vt-engine/low-level-design/testing-and-bench.md)
section 6's existing rule, not a new one.

### The harness

`crates/tools/src/bin/vt-esctest.rs`, `#[cfg(unix)]`, about 140 lines and no new dependency
(`oneterm_vt::pty`'s Unix half already has `openpty`):

1. `openpty`, spawn `esctest.py` on the slave with its stdin and stdout there.
2. Read the master, `Terminal::feed` it, and write every `VtEvent::Reply` back to the master.
3. Build the terminal with `Config { allow_screen_readback: true, ..Default::default() }` -- the
   one place in the repository that sets it.
4. Exit with `esctest`'s own status, and write its log to a path the job uploads.

### The invocation, pinned

```text
python esctest.py --expected-terminal xterm --xterm-checksum 334 \
                  --logfile <artifact>/esctest.log --max-vt-level 4
```

`--xterm-checksum 334` is not cosmetic: it is what makes `esc.py`'s `empty()` return a space rather
than `NUL` and what stops `escutil.py` applying the pre-#279 negation. It is the half of the
checksum decision that lives outside this repository, and it is pinned here so a later reader does
not rediscover it by watching every rectangle assertion fail.

### The report

A pass/fail count per test group, published as a CI artifact and pasted into the packet's Evidence
section, **with a before count**. "Before" is honest and is zero-ish by construction: on `main`
every rectangle assertion times out. The number that matters is the after count and the named list
of failures, which becomes the new content of guide chapter 11's "Known gaps" table.

**No threshold is enforced, in this packet or ever.** A failing `esctest` group is a finding to
record, not a red build: the harness tests a VT420-level terminal and this engine claims VT220 in
`DA1`, so some groups are expected to fail for a reason that is a deliberate design choice.

### What the run will not cover

- Anything requiring `DECRQSS` for a setting the engine answers `DCS 0 $ r` to.
- The `DECSLRM` / left-right-margin groups: the engine has no left-right margins, so those groups
  fail correctly.
- Windows. The job is Linux-only and the binary is `#[cfg(unix)]`, which means it is invisible to
  every check that runs on the maintainer's machine. That is a stated gap, not an oversight: the
  binary compiles in CI and nowhere else, so a change to it is only proven there.

## Edge Cases and Failure Modes

- [ ] **`DECRQCRA` with `allow_screen_readback` false** -> no reply, counted unhandled. A program
      that waits for one hangs; that is the same failure it gets from every terminal with the gate
      shut, and it is why the gate defaults to the status quo rather than to silence-where-there-
      used-to-be-an-answer.
- [ ] **`DECRQCRA` with a rectangle outside the grid** -> clamped, never a panic, never an
      allocation.
- [ ] **A `DECRQSS` request for `m` on a fresh terminal** -> `DCS 1 $ r 0 m ST`. The plain `0` is
      the correct answer and is easy to get wrong by emitting nothing.
- [ ] **An `XTGETTCAP` request with 4 000 names** -> the first 16 are answered, the rest dropped and
      counted.
- [ ] **An `XTGETTCAP` name that decodes to invalid UTF-8** -> not a table match, answered unknown.
      The decoder works on bytes and never builds a `String` from untrusted hex.
- [ ] **A DCS that is truncated by the stream ending** -> `dcs_unhook` never runs, the buffer stays
      until the next `dcs_hook` clears it, and nothing is answered. Bounded by `DCS_MAX_BYTES`.
- [ ] **Two queries in one `feed`** -> two `VtEvent::Reply` values in one batch, in order. The
      batch already supports this; `DA1` twice already works.
- [ ] **`RIS` between hook and unhook** -> impossible through the parser, and the buffer is cleared
      by `RIS` anyway.

## Verification

- [ ] **Byte-feed tests, one per arm.** Feed the exact request, assert the exact reply bytes, with
      the specification's spelling quoted in the test name. `DECRQCRA` gets at least: a one-cell
      rectangle over a known character; a blank cell (answer `0020`); a rectangle clamped from
      outside; a reversed rectangle; the gate closed (no reply, counter up); and origin mode set.
- [ ] **The checksum variant is pinned by a doctest** in guide chapter 11, so the number cannot
      change without a documentation diff.
- [ ] **The SGR round trip**, as described above: feed, request, replay, compare cell templates.
- [ ] **`XTGETTCAP` ceilings** are tested at the boundary: 16 names answered, 17 truncated, a
      128-byte name dropped, odd-length hex answered unknown.
- [ ] **Fuzz.** The existing `cargo-fuzz` parser target already covers DCS; the payload buffer adds
      a new state to it for free. No new target.
- [ ] **`cargo test -p oneterm-vt --features vt-paranoid`** -- the whole-history integrity walk,
      because `DECRQCRA` reads the grid and a reader that runs off the end is exactly what the walk
      catches.
- [ ] **The corpus does not move.** 46 recordings replay byte-identically; none of them contains
      any of these three sequences, so any difference is a bug.
- [ ] **The `esctest` matrix**, attached to the packet's Evidence with its command line, its
      artifact path and the named failures. Report-only.
