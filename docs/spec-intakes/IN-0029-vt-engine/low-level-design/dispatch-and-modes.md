# Low-Level Design: Dispatch and modes

Intake: IN-0029
HLD: ../high-level-design.md
Topic: dispatch-and-modes
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/dispatch/`.

## Concern

The semantic layer: which sequence calls which grid operation, what every default parameter is,
what the engine answers, which modes exist and what `DECRQM` reports for each, and the
extension-point APIs (OSC registration, colour keys, title stack, keyboard flag stack).

Replaces `vendor/vte/src/ansi.rs` — the file where every OneTerm patch has lived — and the
control half of `vendor/alacritty_terminal/src/term/mod.rs`.

The governing rule for this file: **correctness first** (owner ruling, 2026-09-12 — *"think as a
new build, not as a copy of the old one"*). Where the engine being replaced is wrong, this engine
is right from the start; where it is right, it is reproduced exactly. Every correction is a `C`-row
in the table at the end, naming the recordings it affects, and the parity harness carries
per-recording, cell-level expected differences keyed by that id
([`testing-and-bench.md`](testing-and-bench.md) § 2), so a correction never masks a regression.

## Design

### Parameter defaults

One helper, used everywhere: `param_or(i, default)` returns `default` when parameter `i` is
absent **or zero** (trap 22). So `CSI 0 A` equals `CSI A` equals "up one", and
`CSI 0 SP q` equals `CSI SP q` equals "reset the cursor style". A sequence with `ignore` set
(parameter or intermediate overflow) or with more than two intermediates is dropped whole,
before any handler runs.

### C0

| Byte | Action |
| --- | --- |
| `BEL 0x07` | `VtEvent::Bell` |
| `BS 0x08` | `backspace()` — no reverse wrap (trap 1) |
| `HT 0x09` | `put_tab(1)` |
| `LF 0x0A`, `VT 0x0B`, `FF 0x0C` | `linefeed()` |
| `CR 0x0D` | `carriage_return()` |
| `SO 0x0E` / `SI 0x0F` | `set_active_charset(G1 / G0)` |
| `SUB 0x1A` | `substitute()` — write a replacement glyph at the cursor |
| anything else | counted, ignored |

`LNM` (`CSI 20 h`) is tracked but, like the reference, does not change `linefeed`'s behaviour:
`execute` always routes `LF` to `linefeed()`. Deviation D9 records that this is deliberate.

### ESC

| Sequence | Action |
| --- | --- |
| `ESC ( B` / `) B` / `* B` / `+ B` | designate G0..G3 as ASCII |
| `ESC ( 0` / `) 0` / `* 0` / `+ 0` | designate G0..G3 as DEC special graphics (line drawing) |
| `ESC D` (IND) | `linefeed()` |
| `ESC E` (NEL) | `linefeed()` then `carriage_return()` |
| `ESC H` (HTS) | set a tab stop at the cursor column |
| `ESC M` (RI) | `reverse_index()` |
| `ESC Z` | DA1 (same answer as `CSI c`) |
| `ESC c` (RIS) | full reset — see below |
| `ESC 7` / `ESC 8` (DECSC / DECRC) | save / restore cursor, style template and charset designations |
| `ESC # 8` (DECALN) | fill the screen with `E`, home the cursor, full damage |
| `ESC =` / `ESC >` | set / unset `DECKPAM` (application keypad) |
| `ESC \` (ST) | no-op |
| anything else | dropped; `FeedStats::unhandled_sequences` and a `debug` log (R-35) |

`ESC # 3/4/5/6` (DECDWL/DECDHL/DECSWL) stay unimplemented, as they are today, and now
dropped and counted rather than silently vanishing.

### CSI

Default column is the value used when the parameter is absent or zero.

| Sequence | Action | Default |
| --- | --- | --- |
| `CSI Ps @` | ICH — insert blanks | 1 |
| `CSI Ps A` / `B` / `e` | CUU / CUD | 1 |
| `CSI Ps C` / `a` | CUF | 1 |
| `CSI Ps D` | CUB | 1 |
| `CSI Ps b` | REP — replay the preceding printed character through the **full print path**, so it wraps, honours insert mode and re-triggers wide handling (trap 43) | 1 |
| `CSI Ps d` | VPA — `goto_row(Ps - 1)` | 1 |
| `CSI Ps E` / `F` | CNL / CPL (down/up and carriage return) | 1 |
| `CSI Ps G` / `` CSI Ps ` `` | CHA / HPA | 1 |
| `CSI Ps ; Ps H` / `f` | CUP | 1;1 |
| `CSI Ps I` / `Z` | CHT / CBT | 1 |
| `CSI Ps J` | ED — 0 below, 1 above, 2 all, 3 saved; other values dropped | 0 |
| `CSI Ps K` | EL — 0 right, 1 left, 2 all; other values dropped | 0 |
| `CSI Ps L` / `M` | IL / DL | 1 |
| `CSI Ps P` / `X` | DCH / ECH | 1 |
| `CSI Ps S` / `T` | SU / SD | 1 |
| `CSI Ps g` | TBC — 0 the stop under the cursor, 3 all | 0 |
| `CSI ? 5 W` | reset tab stops to every eighth column — **implemented** (deviation D5) | — |
| `CSI Ps ; Ps r` | DECSTBM; `bottom == 0` means "to the last row"; `top >= bottom` is a no-op (trap 15); always homes the cursor | 1, rows |
| `CSI s` / `CSI u` | DECSC-style save / restore cursor | — |
| `CSI ! p` | **DECSTR (soft reset)** — deviation D6 | — |
| `CSI Pm h` / `l` | set / unset each ANSI mode listed below | — |
| `CSI ? Pm h` / `l` | set / unset each private mode listed below | — |
| `CSI Ps $ p` | DECRQM (ANSI) -> `CSI {ps};{state} $ y` | 0 |
| `CSI ? Ps $ p` | DECRQM (private) -> `CSI ? {ps};{state} $ y` | 0 |
| `CSI Ps n` | DSR — 5 -> `CSI 0 n`; 6 -> `CSI {row};{col} R` | 0 |
| `CSI ? Ps n` | DECXCPR — 6 -> `CSI ? {row};{col};1 R` — deviation D7 | 0 |
| `CSI Ps c` (only `Ps == 0`) | DA1 (no intermediate) / DA2 (`>`) | 0 |
| `CSI > 0 q` | **XTVERSION** -> `DCS > \| OneTerm({version}) ST` — deviation D8 | 0 |
| `CSI Ps SP q` | DECSCUSR — 0 or 1 or 2 block, 3 or 4 underline, 5 or 6 beam; blinking when `Ps` is odd; 0 resets to the configured default | 0 |
| `CSI Ps m` | SGR; no parameters means `Reset` | — |
| `CSI > Ps ; Ps m` | `modifyOtherKeys` level — **stored and queryable** (deviation D10) | — |
| `CSI ? 4 m` | report the `modifyOtherKeys` level | — |
| `CSI ? u` | report the kitty keyboard flags from the **top of the stack** (trap 42) | — |
| `CSI = Ps ; Pb u` | set kitty flags; `Pb` 3 difference, 2 union, else replace | 0, replace |
| `CSI > Ps u` | push kitty flags | 0 |
| `CSI < Ps u` | pop `Ps` entries | **1** |
| `CSI Ps t` | XTWINOPS — 14 text area in pixels, 18 text area in cells (`CSI 8;{rows};{cols} t`), 22 push title, 23 pop title | 1 |
| anything else | dropped and counted (R-35) |

`CSI Ps SP k` (SCP) and OSC 22 (mouse cursor icon) are parsed and ignored, as they are today,
but now counted in `FeedStats::unhandled_sequences`.

### SGR

| Parameters | Effect |
| --- | --- |
| `0` or none | reset the style template |
| `1` `2` `3` | bold, dim, italic |
| `4`, `4:1` | underline; `4:0` cancel; `4:2` double; `4:3` curly; `4:4` dotted; `4:5` dashed. Any underline attribute clears the other underline bits first (trap 21) |
| `5` `6` | blink slow / fast — **stored** (correction C11, `US-0076`) |
| `7` `8` `9` | inverse, hidden, strikeout |
| `21` | cancel bold — **not** double underline (trap 21) |
| `22` `23` `24` `25` `27` `28` `29` | cancel bold+dim, italic, underline, blink, inverse, hidden, strikeout |
| `30-37` / `40-47` / `90-97` / `100-107` | named foreground / background, normal and bright |
| `39` / `49` | default foreground / background |
| `53` / `55` | overline / cancel overline — **stored** (correction C11, `US-0076`) |
| `38` / `48` / `58` | extended colour, below |
| `59` | reset the underline colour |
| anything else | skipped; the rest of the SGR list still processes |

Extended colour, using the separator information the parser preserves
([`parser.md`](parser.md)) so trap 20 is a direct read rather than an inference:

| Form | Reading |
| --- | --- |
| `38;5;n` | the **next parameter** is the selector, the one after it the index; a value above 255 aborts the attribute after the parameters are consumed |
| `38;2;r;g;b` | three following parameters; each must fit in `u8` |
| `38:5:n` | sub-parameters of this parameter |
| `38:2:r:g:b` | five sub-parameters; no colour-space id |
| `38:2:cs:r:g:b` | six sub-parameters; the colour-space id is skipped |

### Modes

`Mode` is a typed enum, not a bitflags dump: `Terminal::mode(Mode::AltScreen)` rather than a
`TermMode` the caller must know how to mask. The composite the mouse encoder needs is one
accessor, `Terminal::mouse_reporting() -> Option<MouseProtocol>`, replacing the
`MOUSE_MODE` bit union OneTerm reads in seven places today.

| Mode | Sequence | Default | DECRQM | Notes |
| --- | --- | --- | --- | --- |
| `AppCursor` (DECCKM) | `? 1` | reset | real | reporting only; encoding is the app's |
| `AppKeypad` (DECKPAM/DECKPNM) | `ESC =` / `ESC >` | reset | real | R-64: the keypad state `crates/terminal/src/key_encode.rs` needs; set by `ESC =`, cleared by `ESC >`, reported in `ModeSnapshot` |
| `DecCoLm` | `? 3` | — | `NotSupported` | both `h` and `l` run DECCOLM: reset the region, wipe the grid, full damage; **the width does not change** (trap 40) |
| `Origin` (DECOM) | `? 6` | reset | real | `goto` becomes region-relative and clamps; setting it homes the cursor |
| `LineWrap` (DECAWM) | `? 7` | **set** | real | gates `wrapline()` and the wide-char-at-last-column path |
| `CursorBlink` | `? 12` | reset | real | |
| `ShowCursor` (DECTCEM) | `? 25` | **set** | real | clearing it makes the reported cursor shape `Hidden` |
| `ReverseWrap` | `? 45` | reset | **`Reset`** | additive feature D12, `US-0086`. **DECRQM must never answer `Set` for a mode that does nothing**: until the mode has a reader, `CSI ? 45 $ p` answers `Reset` even after `CSI ? 45 h`, the same rule `? 9001` already follows. Answering `Set` tells a program a capability exists when it does not: while reset (the default) `BS` at column 0 is a no-op, which is trap 1; while set it crosses into a `WRAPPED` row ([`grid-and-scrollback.md`](grid-and-scrollback.md), R-08) |
| `MouseClick` | `? 1000` | reset | real | setting any mouse mode clears the other mouse modes first; unsetting clears only that one (the reference's asymmetry, reproduced) |
| `MouseDrag` | `? 1002` | reset | real | |
| `MouseMotion` | `? 1003` | reset | real | |
| `FocusInOut` | `? 1004` | reset | real | conhost forces this on and re-injects it after any `l` ([`../research/prior-art.md`](../research/prior-art.md) § 5.2) |
| `Utf8Mouse` | `? 1005` | reset | real | mutually exclusive with SGR mouse on set |
| `SgrMouse` | `? 1006` | reset | real | |
| `AlternateScroll` | `? 1007` | **set** | real | |
| `UrgencyHints` | `? 1042` | **set** | real | |
| `AltScreen47` | `? 47` | reset | real | correction C8, `US-0076` (trap 13) |
| `AltScreen1047` | `? 1047` | reset | real | correction C8, `US-0076` |
| `SaveCursor1048` | `? 1048` | reset | real | correction C8, `US-0076` |
| `AltScreen` | `? 1049` | reset | real | save cursor, switch, clear |
| `BracketedPaste` | `? 2004` | reset | real | |
| `SyncUpdate` | `? 2026` | reset | **real** | reference hardcodes `Reset`; here it reports `Set` while an update is open ([`damage-and-render-state.md`](damage-and-render-state.md)) |
| `GraphemeClusters` | `? 2027` | reset | **`NotSupported`** | **deferred (R-56)**: recognised and inert. `cluster_width()` and the grapheme arena ship now; the print path and the ConPTY glyph-width axis do not ([`cell-and-style.md`](cell-and-style.md)) |
| `Insert` (IRM) | `4` | reset | real | does **not** force full damage (trap 34, deviation D2) |
| `LineFeedNewLine` (LNM) | `20` | reset | real | tracked, inert (deviation D9) |
| `Win32Input` | `? 9001` | reset | **`Reset`** | **recognised and inert (R-36)**: conhost sends `ESC [ ? 9001 h` unprompted at session start and re-injects it after any DECRST, so on every local Windows session this arrives repeatedly. It is accepted silently and never counted as unhandled; `Reset` is the honest DECRQM answer because the encoding is not implemented. Half-adopting win32-input-mode corrupts F3 ([`../research/prior-art.md`](../research/prior-art.md) § 5.5), so "recognised and off" is the correct v1 state |
| unknown private mode | | | `NotSupported` | dropped and counted |

Modes explicitly **not** implemented in this intake, each recognised as unknown, dropped and
counted: `? 69` (DECLRMM) and DECSLRM, `? 80` (DECSDM), `? 1016` (SGR-pixel mouse), `? 2031`
(colour-scheme notification), `? 2048` (in-band resize). Each is a later intake; the mode table
has room and the DECRQM answer (`NotSupported`, which means "do not use it") is honest today.

DECRQM state values: `0` not recognised, `1` set, `2` reset, `3` permanently set, `4`
permanently reset.

### Answers

| Query | Answer | Note |
| --- | --- | --- |
| DA1 `CSI c`, `ESC Z` | `CSI ? 62 ; 4 ; 22 c` | VT220, Sixel, ANSI colour. Today the fork answers `CSI ? 62 ; 4 c`; upstream answers `CSI ? 6 c`. `4` is what `tmux`, `lsix`, `chafa` and `timg` look for. Adding `22` is deviation D13 |
| DA2 `CSI > c` | `CSI > 0 ; {version} ; 1 c` | `version` from `CARGO_PKG_VERSION` as `major*10000 + minor*100 + patch` |
| DSR 5 | `CSI 0 n` | |
| DSR 6 (CPR) | `CSI {row};{col} R` | **Spec-correct (C5)**: region-relative while `DECOM` is set, absolute otherwise (trap 38). Conhost's handshake is unaffected because conhost never sets origin mode |
| DECXCPR `CSI ? 6 n` | `CSI ? {row};{col};1 R` | deviation D7 |
| XTVERSION `CSI > 0 q` | `DCS > \| OneTerm({version}) ST` | deviation D8 |
| `CSI 18 t` | `CSI 8 ; {rows} ; {cols} t` | |
| `CSI 14 t` | `CSI 4 ; {height} ; {width} t` | needs cell metrics the engine does not have; the embedder supplies them through `Terminal::set_cell_pixels` on a font change, defaulting to 0 (N-08) |
| `CSI ? u` | `CSI ? {flags} u` | from the top of the kitty stack (trap 42) |
| `CSI ? 4 m` | `CSI > 4 ; {level} m` | deviation D10 |

All answers leave as `VtEvent::Reply(bytes)` in the batch; the embedder writes them to the
transport, exactly as it handles `Event::PtyWrite` today — and **before any yield to the render
demand** ([`damage-and-render-state.md`](damage-and-render-state.md) § "Fairness and reply
latency", R-37), because a delayed DA1 costs a one-second stall at session start.

**Answering promptly matters on Windows.** Conhost's `VtIo::StartIfNeeded` sends `ESC [ 6 n`
(with `INHERIT_CURSOR`), `ESC [ c`, `ESC [ ? 1004 h` and `ESC [ ? 9001 h` the moment the first
client connects, and **blocks for up to one second** waiting for the DA1 reply
([`../research/prior-art.md`](../research/prior-art.md) § 5.4). The engine answers DA1 and CPR
inside the same `feed()` call that parsed them, so the reply is in the batch the pump drains
microseconds later.

### Colour model

Typed keys, replacing the magic indices 256 / 257 / 258 that appear in three files today
(`crates/terminal/src/osc_color.rs:23-27`,
`crates/terminal/src/palette.rs:136`, `crates/terminal-view/src/render/frame.rs:118`, `:121`).

```rust
pub enum Color { Named(NamedColor), Palette(u8), Rgb(Rgb) }

pub enum NamedColor {
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, …, BrightWhite,
    Foreground, Background, Cursor, BrightForeground, DimForeground,
    DimBlack, …, DimWhite,
}
impl NamedColor {
    pub fn bright(self) -> Option<NamedColor>;   // explicit mapping, no discriminant arithmetic
    pub fn dim(self) -> Option<NamedColor>;
}

pub enum ColorKey { Palette(u8), Foreground, Background, Cursor,
                    BrightForeground, DimForeground, Dim(u8) }
```

- `Terminal::color(key) -> Option<Rgb>` is the OSC-override layer only; `None` means "use the
  theme", exactly as the 269-slot table works today.
- `Terminal::set_theme_colors(&ThemeColors)` hands the engine the defaults it needs to answer
  a query; the renderer still resolves final colours itself, including bold-to-bright and dim
  mixing, which stays in `crates/terminal/src/palette.rs`.
- OSC 104 with no parameter resets indices 0..255 and **not** foreground / background / cursor
  (trap 26). OSC 4 requires an **odd** parameter count and rejects an index above 255
  (trap 26). Both reproduced.
- A colour change marks everything damaged unless the key is `Cursor` and the value did not
  actually change.

### OSC

Handled natively:

| OSC | Behaviour |
| --- | --- |
| `0`, `2` | set the title -> `VtEvent::Title`; requires at least two parameters |
| `4` | set or query palette entries. **Spec-correct (C7)**: every complete `index;spec` pair is applied and a trailing odd parameter is ignored, where the reference rejects an even parameter count wholesale (trap 26); `?` queries -> `VtEvent::ColorQuery { key: Palette(i) }` |
| `8` | hyperlink; `id=` parsed from `params[1]`, the URI rejoined from `params[2..]`; an empty URI clears it. **This packet owns the `HyperlinkTable` bound** — see below |
| `10` / `11` / `12` | foreground / background / cursor, set or `?` query; a multi-parameter form advances the key and stops past `Cursor` |
| `104` / `110` / `111` / `112` | reset |
| `52` | clipboard; `?` -> `VtEvent::ClipboardLoad`, otherwise base64-decode -> `VtEvent::ClipboardStore`. The selection byte must be `c`, `p` or `s`; anything else drops the request; undecodable base64 or invalid UTF-8 is dropped silently (trap 25). **The engine never applies a policy** — the decision stays in `crates/terminal/src/security_policy.rs` |
| `22` | mouse cursor icon; parsed and ignored |
| `50` | cursor shape (`CursorShape=0/1/2`) |

Everything else goes through the registration table.

### OSC registration — the extension point

```rust
pub struct OscClaims { low: [u64; 32], high: Vec<u32> }   // bitmap for 0..2048, list above

impl OscClaims {
    pub fn claim(&mut self, code: u32) -> &mut Self;
    pub fn claim_large(&mut self, code: u32) -> &mut Self;  // also allow-lists the 8 MiB spill
}
```

Claimed numbers are delivered as `VtEvent::Osc { code, params, terminator }` with the parameter
bytes living in the batch arena — no `Vec<Vec<u8>>` per OSC, which is what the fork allocates on
the hot path today (`crates/terminal/src/backend/osc_router.rs:249-257`). Unclaimed and unhandled
numbers are dropped and counted, exactly as the reference drops them (R-35).

`claim_large(code)` additionally allows that number's payload to spill past `OSC_INLINE` to
`OSC_LARGE` ([`parser.md`](parser.md), R-55). It is a **memory ceiling only**; who may write or
read the clipboard, and under what limits, stays in `crates/terminal/src/security_policy.rs`.

`crates/terminal` claims 7, 9, 133 and 633, plus 52 and 8 as large. **OSC 9;7 (the agent
channel, `docs/osc-agent-status.md`) becomes a claim on OSC 9 and a sub-code match in
`crates/terminal/src/osc_agent/`** — no engine change, which is the whole point of the
extension point. Its collision with ConEmu's "run some process with arguments" sub-code
([`../research/prior-art.md`](../research/prior-art.md) § 6.4) is a defect in
`docs/osc-agent-status.md` and is **not** solved here; it needs its own packet, and whichever
sub-code wins is a one-line change to the claim.

A claimed number whose handler is missing is a debug assertion, never a panic.

### The hyperlink table needs a ladder, and `US-0076` owns it

`HyperlinkTable` ([`cell-and-style.md`](cell-and-style.md) § "Extras") is the one interned table
with no bound: a link **without** an explicit `id=` gets a fresh implicit id on every occurrence,
so a stream of un-`id=`-ed OSC 8 links grows `entries` and its index map without limit. That is
attacker-reachable from any SSH session, so it cannot be left to a later packet:

| Step | Condition | Action |
| --- | --- | --- |
| 1 | the link is already interned (same `id` and URI) | reuse |
| 2 | `entries.len() < HYPERLINK_TABLE_LIMIT` (65 535, matching the other tables) | insert |
| 3 | full | drop the hyperlink attribute for that cell — the text still renders, the link is simply not clickable — count it in `FeedStats::hyperlink_table_exhausted` and `log::warn!` once per session |

Additionally, implicit ids are **recycled by `RIS` and by a full reset**, which `US-0076` also
owns: both clear the table, so a long session that never repeats a URI cannot accumulate across a
`clear`-and-restart cycle. An explicit `id=` still interns by value, so a program that groups its
links pays one entry per group, which is the case the protocol was designed for.

The US-0074 verification recorded this as the most substantive open risk that packet left behind;
it is written here so it cannot be dropped.

### Title stack

`push_title` (`CSI 22 t`) and `pop_title` (`CSI 23 t`) over a `Vec<Option<String>>` capped at
`TITLE_STACK_MAX = 16`; overflow drops the **oldest** entry. The reference caps at 4096, which
no program approaches and which is a cheap memory sink. Deviation D14.

### Kitty keyboard flag stack

```rust
pub struct FlagStack { flags: [KeyboardFlags; 8], len: u8 }
```

Ghostty's shape: fixed size, no heap, push wraps and evicts, and `pop(n >= len)` resets the whole
stack, which explicitly removes the denial-of-service vector of a client sending a huge pop count
([`../research/prior-art.md`](../research/prior-art.md) § 9.7). The stack is swapped with the
inactive one on an alternate-screen swap.

The engine owns the **state**; the app owns the **encoding**. `CSI ? u` reports the top of the
stack, which can legitimately differ from the live flags after `CSI = Ps u` (trap 42). The
reference's overflow bug — popping from the *title* stack on keyboard-stack overflow — is
**fixed**, deviation D15; no recording covers it. Unlike the reference, the protocol is not
gated behind a config flag that OneTerm never sets; the flags are simply readable, so a later
keyboard intake needs no engine change.

### Reset

**RIS (`ESC c`)** — reset both grids (clearing history, cursors and the viewport offset), the
scroll region, tab stops, title and title stack, selection, both keyboard stacks, the active
charset, the cursor style and the mode set; drop pending graphics and any in-flight DCS; emit
`VtEvent::ScreenCleared`; full damage. **Row ids are not reset** (`DEC-0015`). **Spec-correct
(C6): the colour override table is reset too**, where the reference leaves OSC 4 / 10 / 11 / 12
overrides in place across `RIS` (trap 39).

**DECSTR (`CSI ! p`)** — soft reset, deviation D6: cursor home, origin mode off, insert mode
off, `DECAWM` on, scroll region to the full screen, style template reset, saved cursor reset,
cursor visible. It does **not** clear the screen, the scrollback, the title or the palette. The
reference does not implement it at all, so programs that send it currently get nothing.

### Deliberate deviations

Every row here is behaviour that differs from the engine being replaced, with the reason and
the recording risk.

**Scheduling.** After the owner's correctness-first ruling (2026-09-12), every behaviour
*correction* lands in its natural packet with a declared expected difference; only **additive
features** — capabilities the reference never had and no recording exercises — wait for `US-0086`.
`US-0072` has measured the 45 recordings for every sequence a correction or deferred feature
touches; the "Affected recordings" columns below carry that measurement
(`evidence/US-0072-recording-risk.md`).

| # | Deviation | Packet | Reason | Recording risk |
| --- | --- | --- | --- | --- |
| D1 | Typed `Mode` enum and `ColorKey` instead of bitflags and magic indices | `US-0076` | `DEC-0015`; removes discriminant arithmetic from three files | none — representation only |
| D2 | Insert mode does not force full damage (trap 34) | `US-0076` | the reference silently disables partial redraw for the session | none — damage is not compared |
| D3 | *superseded* — now correction C8 (`? 47` / `? 1047` / `? 1048`) | `US-0076` | correctness first (owner ruling, 2026-09-12) | see the corrections table |
| D4 | Mode 2026 reports its real state and needs no external polling (trap 41) | `US-0076` | the reference can never report "set" and freezes if the host forgets to poll | none |
| D5 | *superseded* — now correction C10 (`CSI ? 5 W`) | `US-0076` | correctness first (owner ruling, 2026-09-12) | see the corrections table |
| D6 | *superseded* — now correction C9 (DECSTR) | `US-0076` | correctness first (owner ruling, 2026-09-12) | see the corrections table |
| D7 | DECXCPR (`CSI ? 6 n`) answered | `US-0076` (implemented) | cheap, and asked for by real programs | none — answers are discarded by the harness |
| D8 | XTVERSION answered | `US-0076` (implemented) | feature detection by modern programs | none — answers are discarded |
| D9 | LNM tracked but inert | `US-0076` | matches the reference; recorded so it is not read as an oversight | none |
| D10 | `modifyOtherKeys` level stored and reportable | `US-0076` (implemented) | the reference parses both and implements neither | none |
| D11 | *superseded* — now correction C11 (blink and overline stored) | `US-0076` | correctness first (owner ruling, 2026-09-12); the `US-0072` measurement shows no recording sends SGR 5 / 6 / 53 / 55, so it needs no declared diff at all | see the corrections table |
| D12 | Reverse wrap (`? 45`) implemented | **`US-0086`** | cheap; gated on the mode, default off, so trap 1 is unaffected until then (R-08) | **measured: none** — six recordings only ever reset `? 45`, none sets it |
| D13 | DA1 answers `CSI ? 62 ; 4 ; 22 c` | `US-0076` | adds the ANSI-colour claim to today's answer | none — DA answers are discarded by the harness |
| D14 | Title stack capped at 16, oldest dropped | `US-0076` | 4096 is a memory sink no program needs | none |
| D15 | Kitty stack overflow pops the right stack (trap 42) | `US-0076` | fixes a reference bug | none |

**Corrections in this file — spec-correct from the start.**

**Gate result (`US-0076`, `evidence/US-0076-parity-gate.md`): 45 of 45 green with no
`expected-diffs` file.** Every correction below is measured **free**, C9 included — `grid_reset`
does send `CSI ! p`, but `RIS` and `OSC 104` precede it and a run of mode sets and SGR follows, so
the soft reset's effects are overwritten before the grid is compared.

| C | Correction | Trap | Packet | Affected recordings (**measured**) |
| --- | --- | --- | --- | --- |
| C5 | `CPR` is region-relative under `DECOM` | 38 | `US-0076` | **none**. Eight recordings touch one half or the other and **no recording sends both**: five set `DECOM` without asking for a position, three send `CSI 6 n` without `DECOM`. DSR answers are discarded by the harness anyway |
| C6 | `RIS` resets the colour overrides | 39 | `US-0076` | **1 recording**: `grid_reset` (one `RIS`, one `OSC 104`). `OSC 104` already empties indices 0-255, so no `state.expect` palette key is expected to move |
| C7 | `OSC 4` applies complete pairs and ignores a trailing parameter | 26 | `US-0076` | **none of the 45**. `indexed_256_colors` sends 240 well-formed triples, which the old engine already accepts. Free |
| C9 | `DECSTR` (`CSI ! p`) implemented | — | `US-0076` | **1 recording, and it is the one certain diff in this table**: `grid_reset` sends `CSI ! p`, which the old engine drops, so implementing it **will** change that recording's grid. `US-0076` writes `grid_reset`'s `expected-diffs.json` naming C9 |
| C15 | An endpoint scrolled out of a scroll-region top kills the selection, where the reference clamps it | — | `US-0078` | see [`selection.md`](selection.md); **to measure in `US-0076`**, expected free (no recording selects) |
| C13 / C14 | Reflow corrections — a grow keeps the tail below the cursor; a bold trailing blank is kept | — | `US-0077` | see [`grid-and-scrollback.md`](grid-and-scrollback.md) § "Corrections"; both **to measure in `US-0076`** |
| C11 | Blink and overline attributes stored (SGR 5 / 6 / 53 / 55) | — | `US-0076` | **none of the 45** — no recording sends those parameters (`sgr` exercises 9, 4 and the colour forms; `underline` exercises `4:0`-`4:3`, 21, 24). **Free, with no declared diff**: N-03's reason for deferring it does not survive the measurement |

**Two behaviours that look like bugs and are parity, not corrections.** Both follow the vendored
reference and neither has a correction id, because the gate compares against that reference and a
difference would have nowhere to be declared:

- **`DECALN` does not home the cursor.** It fills the screen with `E` and leaves the cursor where
  it was. The reference does the same, and `decaln_reset` compares the cursor position.
- **`SUB` (`0x1A`) is a no-op**, not a replacement-glyph write. No recording sends it; the
  reference ignores it.

Kept deliberately, because they are correct: traps 15, 16, 18, 21, 22, 25, 40 and 43. Mode 2027 is
deferred whole (R-56), so it is not a deviation — it is unimplemented, and DECRQM says so.

## Interfaces

```rust
// crates/vt/src/dispatch.rs
// Built by Terminal::feed from a disjoint field split, so `&mut Parser` and `&mut Handler`
// can coexist; the full field list and the split are in events-and-api.md (R-32).
pub(crate) struct Handler<'a> {
    screens: &'a mut Screens,       // primary + alternate + which is active
    intern:  &'a mut Interner,
    anchors: &'a mut Anchors,
    modes:   &'a mut Modes,
    colors:  &'a mut ColorOverrides,
    title:   &'a mut TitleState,
    keyboard:&'a mut KeyboardStacks,
    graphics:&'a mut GraphicsState,
    sync:    &'a mut SyncState,
    stats:   &'a mut FeedStats,
    out:     &'a mut EventBatch,
    now:     Instant,
}
impl Dispatch for Handler<'_> { /* parser::Dispatch, see parser.md */ }

// public surface
impl Terminal {
    pub fn mode(&self, m: Mode) -> bool;
    pub fn mouse_reporting(&self) -> Option<MouseProtocol>;
    pub fn keyboard_flags(&self) -> KeyboardFlags;
    pub fn modify_other_keys(&self) -> u8;
    pub fn cursor_style(&self) -> CursorStyle;   // shape + blink; Hidden carries visibility
    pub fn title(&self) -> Option<&str>;
    pub fn color(&self, key: ColorKey) -> Option<Rgb>;
    pub fn set_theme_colors(&mut self, theme: &ThemeColors);
}
pub struct Config { pub osc_claims: OscClaims, pub default_cursor_style: CursorStyle,
                    pub scrollback_limit: u32, pub semantic_escape_chars: String,
                    pub accept_c1: bool }   // see the note below
impl Terminal { pub fn set_cell_pixels(&mut self, w: u16, h: u16); }  // one owner (R-40, N-08)

// `accept_c1` is currently a dead knob: the parser hard-codes the 7-bit behaviour, so setting it
// changes nothing. It is either threaded into the parser or deleted, and this line records which
// once the `US-0076` follow-up lands:
//     STATUS: <wired | removed>  —  fill in when the fix merges.
```

## By-design differential divergences

`vt-diff` (old engine against new) is **identical on all 45 recordings and all 10 bench fixtures**
today. The three families below would diverge if a stream exercised them, and they are listed so a
future verifier does not re-derive them from a red diff:

| Family | What differs | Why it is by design |
| --- | --- | --- |
| **C8 — legacy alt screen** | `? 47`, `? 1047`, `? 1048` change the screen here and do nothing in the reference | The reference recognises only `? 1049` and silently drops the rest (trap 13). No recording sends them |
| **C9 — `DECSTR`** | `CSI ! p` performs a soft reset here and is ignored by the reference | The reference has no handler at all. `grid_reset` sends it, but its effects are overwritten before the comparison |
| **Mode 2026 — synchronized output** | The reference **buffers up to 2 MiB of unparsed bytes**; this engine applies everything and suppresses frames in the renderer | A grid snapshot taken mid-block therefore shows different content in the two engines: the reference has not applied the block yet, this engine has. Both are correct at the closing sequence. `vt-diff` compares final grids, so it only appears if a fixture ends inside an open block |

Every other difference is a defect until a correction id says otherwise.

## Edge Cases and Failure Modes

- [ ] **Trap 12 — `ScreenCleared` only for `ED 2`, `ED 3` and RIS**, never for `ED 0`/`ED 1` or
  `EL`, and fired before the "is there history" check for `ED 3`.
- [ ] **Trap 20 — `38;5` versus `38:5`**, including the six-sub-parameter colour-space form and
  the "value above 255 aborts after consuming" rule.
- [ ] **Trap 21 — `SGR 21` is cancel-bold**, and any underline attribute clears the other
  underline bits first.
- [ ] **Trap 22 — a parameter of zero means default.**
- [ ] **Trap 25 — OSC 52** selection-byte validation and silent drops. The engine reports;
  the policy stays in the embedder.
- [ ] **Trap 26 — OSC 4 needs an odd parameter count**, rejects an index above 255, and
  OSC 104 with no argument does not reset 256/257/258.
- [ ] **Trap 38 — CPR ignores origin mode.**
- [ ] **Trap 39 — RIS keeps the colour palette.**
- [ ] **Trap 40 — DECCOLM does not change the column count** and DECRQM reports
  `NotSupported` for mode 3 even though both `h` and `l` act.
- [ ] **Trap 42 — `CSI ? u` reads the stack, not the live flags.**
- [ ] **Trap 43 — REP replays through the print path**, and `preceding_char` survives
  intervening escape sequences.
- [ ] **A claimed OSC with a malformed payload** reaches the embedder with `truncated` or a
  short parameter list; the engine never validates payload semantics.
- [ ] **An answer generated while the transport is closed** is still put in the batch; dropping
  it is the embedder's decision (`docs/agents/error-policy.md`, transport-closure row).
- [ ] **A very long title** is truncated by the OSC cap and still applied.

## Verification

`cargo test -p oneterm-vt dispatch::`

- [ ] `dispatch::tests::csi_table_is_exhaustive` — a table test feeding every listed sequence
  and asserting the operation and the default parameter.
- [ ] `dispatch::tests::param_zero_means_default` — trap 22.
- [ ] `dispatch::tests::sgr_semicolon_and_colon_colour_forms` — trap 20, all five forms plus
  the above-255 abort.
- [ ] `dispatch::tests::sgr_21_is_cancel_bold_and_underline_styles_are_exclusive` — trap 21.
- [ ] `dispatch::tests::screen_cleared_fires_only_for_ed2_ed3_and_ris` — trap 12.
- [ ] `dispatch::tests::ed3_with_no_history_still_fires_screen_cleared` — trap 12.
- [ ] `dispatch::tests::osc_4_applies_complete_pairs` — correction C7, trap 26.
- [ ] `dispatch::tests::osc_104_does_not_reset_the_special_colours` — trap 26.
- [ ] `dispatch::tests::osc_52_selection_byte_validation` — trap 25.
- [ ] `dispatch::tests::hyperlink_table_exhaustion_drops_the_attribute_and_logs_once` — the ladder
  above; drives 65 535 implicit-id links and asserts the text still renders.
- [ ] `dispatch::tests::ris_clears_the_hyperlink_table`
- [ ] `dispatch::tests::cpr_honours_origin_mode` — correction C5, trap 38.
- [ ] `dispatch::tests::ris_resets_the_palette_and_keeps_the_row_ids` — correction C6, trap 39.
- [ ] `dispatch::tests::deccolm_does_not_change_the_width` — trap 40.
- [ ] `dispatch::tests::rep_replays_through_the_print_path` — trap 43, asserting that a `REP`
  at the last column wraps.
- [ ] `dispatch::tests::kitty_query_reads_the_stack_top` — trap 42.
- [ ] `dispatch::tests::kitty_pop_beyond_len_resets_the_stack` — deviation D15.
- [ ] `dispatch::tests::decrqm_answers_match_the_mode_table` — a table test over every mode,
  including that no accepted-but-inert mode (`? 45`, `? 9001`) ever answers `Set`.
- [ ] `dispatch::tests::da1_da2_dsr_xtversion_answers`
- [ ] `dispatch::tests::decstr_soft_reset_scope` — correction C9.
- [ ] `dispatch::tests::mouse_modes_are_exclusive_on_set_not_on_unset`
- [ ] `dispatch::tests::title_stack_caps_at_sixteen_dropping_the_oldest` — deviation D14.
- [ ] `dispatch::tests::blink_and_overline_are_stored` — correction C11.
- [ ] `dispatch::tests::unhandled_sequences_are_counted_not_echoed` — R-35, over an unknown CSI,
  an unknown ESC, an unclaimed OSC and an unknown DCS.
- [ ] `dispatch::tests::win32_input_mode_is_accepted_silently` — R-36; `ESC [ ? 9001 h` sent
  repeatedly, as conhost sends it, produces no unhandled count and no state change, and DECRQM
  answers `Reset`.
- [ ] `dispatch::tests::app_keypad_mode_is_reported` — R-64; `ESC =` then `ESC >`, read back
  through `ModeSnapshot`.
- [ ] `dispatch::tests::mode_2027_is_recognised_and_inert` — R-56; setting it changes no
  behaviour and DECRQM answers `NotSupported`.
- [ ] `dispatch::tests::claimed_osc_reaches_the_batch_without_allocating_per_osc` — a counting
  allocator.
- [ ] `dispatch::tests::osc_9_7_reaches_the_embedder_through_a_claim` — the extension point,
  proving no engine change is needed for the agent channel.
- [ ] One byte-feed test per sequence marked supported in `docs/osc-sequences-checklist.md`, so
  the checklist and the engine cannot drift.

Integration: the 45-recording parity gate
([`testing-and-bench.md`](testing-and-bench.md)) is this file's real exit criterion; the unit
tests above exist so a failure names the sequence rather than the recording.
