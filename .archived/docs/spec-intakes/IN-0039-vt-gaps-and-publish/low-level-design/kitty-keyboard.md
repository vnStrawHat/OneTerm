# Low-Level Design: the kitty keyboard encoder

Intake: [`IN-0039`](../IN-0039.md)
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: kitty-keyboard
Date: 2026-09-15

> One concern per file: what `input::encode_key` must return once a program has pushed kitty
> keyboard flags, and nothing else. The flag *state* already exists and is correct; this file is
> about the bytes.

## Concern

`crates/vt/src/input/key.rs` reads exactly one field of the mode snapshot -- `app_cursor` -- and
returns the xterm legacy encoding for everything. `crates/vt/src/terminal/dispatch.rs` answers
`CSI ? u` with the pushed flag bits. A program that negotiates the protocol is therefore told
"yes" and then handed bytes from a protocol it has switched away from.

The source is the kitty keyboard protocol specification,
<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>, fetched 2026-09-15. Every table below is from
it; where this design deviates, the row says so and says why.

## What already exists, and is not touched

| Thing | Where | State |
| --- | --- | --- |
| `KeyboardFlags`, the five bits | `crates/vt/src/terminal/mode.rs` | complete and correct |
| `FlagStack` -- `apply`, `push`, `pop`, `top`, `live` | same | complete: fixed size, no heap, push evicts the oldest, `pop(n >= len)` resets, which is exactly what the specification asks for ("if a pop request is received that empties the stack, all flags are reset ... if a push request is received and the stack is full, the oldest entry must be evicted ... terminals should limit the size of the stack") |
| One stack per screen, swapped with the alternate screen | same | complete; the specification requires separate stacks for the main and alternate screens |
| `CSI = Ps ; Pm u` (apply, modes 1 / 2 / 3), `CSI > Ps u` (push), `CSI < Ps u` (pop), `CSI ? u` (query) | `crates/vt/src/terminal/dispatch.rs` | complete. Deviation D15 is recorded there: the pop default is 1 |
| `modifyOtherKeys` level, `CSI > 4 ; Ps m` | `Terminal::modify_other_keys()` | stored, never read by the encoder |
| The legacy encoder and its 75-key table | `crates/vt/src/input/key.rs` | stays, and stays the default path |

`US-0105` adds **no terminal state at all.** It reads state that has been live and tested since
`IN-0029` and was measured unreachable by `US-0099`'s equivalence run.

## Interfaces

Three additions and one field pair. Every existing signature keeps working.

```rust
// crates/vt/src/input/mod.rs

/// Which kind of key event this is. Only reported when the program asked for
/// event types (`REPORT_EVENT_TYPES`); otherwise a repeat is encoded as a press
/// and a release produces no bytes at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum KeyEventKind {
    #[default]
    Press,
    Repeat,
    Release,
}

/// Everything the kitty protocol can report about one key event.
///
/// `#[non_exhaustive]`: build it from `KeyEvent::new(key, mods)` and assign the
/// rest. The three optional fields come from the embedder's platform layer; an
/// embedder that cannot supply one leaves it `None` and the encoder omits the
/// corresponding sub-field, which the protocol allows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyEvent {
    pub key: KeySpec,
    pub mods: KeyMods,
    pub kind: KeyEventKind,
    /// The code point this key produces with shift applied, for
    /// `REPORT_ALTERNATE_KEYS`. `None` when the platform does not know it.
    pub shifted: Option<char>,
    /// The code point this key carries in the keyboard's base (ASCII) layout,
    /// for `REPORT_ALTERNATE_KEYS`.
    pub base_layout: Option<char>,
    /// The text this key event would insert, for `REPORT_ASSOCIATED_TEXT`.
    pub text: Option<String>,
}

/// Encode one key event, honouring every protocol the snapshot reports.
pub fn encode_key_event(event: &KeyEvent, modes: &ModeSnapshot) -> Option<Vec<u8>>;
```

```rust
// crates/vt/src/snapshot/modes.rs -- two new fields, and the mark
#[non_exhaustive]                       // added in the same packet: free now, a
pub struct ModeSnapshot {               // minor bump for every later field if not
    // ... the nine fields that exist today ...
    /// The live kitty keyboard flags (`CSI > Ps u` and friends).
    pub keyboard_flags: KeyboardFlags,
    /// The `modifyOtherKeys` level, `CSI > 4 ; Ps m`: 0, 1 or 2.
    pub modify_other_keys: u8,
}
```

```rust
// crates/vt/src/terminal/mod.rs -- the sibling of the existing two-argument form
impl Terminal {
    pub fn encode_key_event(&self, event: &KeyEvent) -> Option<Vec<u8>>;
}
```

`encode_key(key, mods, modes)` and `Terminal::encode_key(key, mods)` keep their signatures and
become thin wrappers over `KeyEvent::new(key, mods)`. **Their returned bytes change** whenever the
snapshot carries non-empty flags or a non-zero `modifyOtherKeys` level -- that is the whole point of
the packet, and it is the one semver-relevant behaviour change in this intake. With both empty, the
bytes are identical to today's, which is what the equivalence test asserts.

### Why `ModeSnapshot` and not two more arguments

`Terminal::keyboard_flags()` and `modify_other_keys()` already exist, so `Terminal::encode_key_event`
could read them directly and the free function could take three arguments. It does not, for one
reason: `ModeSnapshot` is already the single "everything the encoder needs to know about the
terminal's state" value, the adapter already fetches exactly one of them per frame
(`crates/terminal/src/content.rs`), and an embedder holding a snapshot outside the lock can encode
without re-acquiring it. Two context types where one exists is the shape that rots.

Nothing outside `crates/vt` builds a `ModeSnapshot` with a struct literal -- checked: the four
`modes()` implementations in `crates/terminal`, `crates/terminal-view` and `test_support.rs` all
forward `Terminal::mode_snapshot()`. So `#[non_exhaustive]` costs nothing today and saves a minor
bump on the next field.

## Design: the decision ladder

`encode_key_event` picks exactly one encoding, in this order. The first rung that applies wins, and
no rung ever falls through to a later one after producing bytes.

```text
1. Release event, and REPORT_EVENT_TYPES is off       -> None (no bytes)
2. Any kitty flag applies to this key (table below)   -> kitty CSI u
3. modify_other_keys == 2, or == 1 and the key is
   "other" in xterm's sense, and the chord has a
   modifier                                           -> CSI 27 ; mod ; code ~
4. otherwise                                          -> the legacy table (today's code, unchanged)
```

Rung 2 before rung 3 is the protocols' own rule: the kitty specification's section "Why xterm's
modifyOtherKeys should not be used" makes the kitty flags the superseding negotiation, and every
terminal that implements both resolves it this way. A program that has pushed kitty flags and also
set `modifyOtherKeys` gets kitty.

Rung 1 is why `encode_key` cannot simply be extended: today it has no way to express "this event
produces nothing".

### Which flag makes rung 2 apply

| Flag | Bit | Makes rung 2 apply for | Does not apply to |
| --- | --- | --- | --- |
| `DISAMBIGUATE_ESC_CODES` | `0b1` | `Escape`, and any chord with `alt`, `ctrl`, `ctrl+alt` or `shift+alt`; every key event that does not generate text | **`Enter`, `Tab`, `Backspace`** -- the specification's explicit exception, "to allow the user to type and execute commands in the shell such as `reset` after a program that sets this mode crashes without clearing it" |
| `REPORT_EVENT_TYPES` | `0b10` | nothing on its own; it adds the `:event-type` sub-field and turns release events from "no bytes" into bytes | `Enter`, `Tab`, `Backspace` still send no release unless `REPORT_ALL_KEYS_AS_ESC` is also set |
| `REPORT_ALTERNATE_KEYS` | `0b100` | nothing on its own; it adds the `:shifted:base` sub-fields | -- |
| `REPORT_ALL_KEYS_AS_ESC` | `0b1000` | **every** key, including text-producing ones and `Enter` / `Tab` / `Backspace`. Text is no longer sent as text | -- |
| `REPORT_ASSOCIATED_TEXT` | `0b10000` | nothing on its own; it adds the trailing `;text-codepoints` field. The specification says it "is undefined if used without" `REPORT_ALL_KEYS_AS_ESC`; this engine treats it as inert in that case rather than guessing | -- |

## Design: the byte tables

### The `CSI u` form

```text
CSI unicode-key-code [: shifted-key [: base-layout-key]] [; modifiers [: event-type]] [; text-as-codepoints] u
```

Every field except `unicode-key-code` is optional and is omitted when it would carry its default.
The rules the encoder follows, in the order it applies them:

1. **Trailing defaults are dropped.** `modifiers` defaults to `1`, `event-type` to `1`. A press with
   no modifiers and no alternates and no text is `CSI <code> u`, nothing more.
2. **An empty sub-field is a hole, not an omission.** If `base_layout` is known but `shifted` is
   not, the shifted slot is written empty: `CSI 97::97 ; 5 u`. The specification: "if the terminal
   wants to send only a base layout key but no shifted key, it must use an empty sub-field for the
   shifted key."
3. **The shifted key is only sent when shift is in the modifier set.** Otherwise the field is
   omitted even if the embedder supplied it.
4. **Functional keys that have a legacy `~` or letter form keep it**, with the kitty modifier and
   event-type fields attached: `CSI 1 ; <mod> [: <event>] A` for Up, `CSI 5 ; <mod> ~` for PageUp.
   The specification gives both forms for these keys and requires the legacy final byte.

### Modifier encoding

The transmitted value is `1 + sum of bits`:

| Modifier | Bit | Reachable from `KeyMods` |
| --- | --- | --- |
| shift | 1 | yes |
| alt | 2 | yes |
| ctrl | 4 | yes |
| super | 8 | **no** |
| hyper | 16 | **no** |
| meta | 32 | **no** |
| caps lock | 64 | **no** |
| num lock | 128 | **no** |

`KeyMods` has three booleans, so the encoder can emit `1` through `8` and no other value. **This is
a documented ceiling, not a bug to hide**: adding five booleans to `KeyMods` is a breaking change to
a type every embedder constructs, and no embedder in this repository can supply super, hyper, meta
or the lock states today. Guide chapter 6 states the ceiling; `KeyMods` stays exhaustive so the
compiler tells an embedder the day it grows. Recorded as a follow-up, not smuggled in here.

### Event types

| Event | Sub-field | When emitted |
| --- | --- | --- |
| press | `1` | always; omitted, being the default |
| repeat | `2` | only under `REPORT_EVENT_TYPES`; otherwise encoded as a press |
| release | `3` | only under `REPORT_EVENT_TYPES`; otherwise the event produces `None` |

### Functional key codes

The `CSI u` code for each `NamedKey` the crate has. Two columns because a key with a legacy form
keeps it (rule 4 above) and only the code-point form is used for keys that have none.

| `NamedKey` | Under kitty flags | Legacy form, which it keeps |
| --- | --- | --- |
| `Escape` | `CSI 27 u` | `0x1b` |
| `Enter` | `CSI 13 u` | `0x0d` |
| `Tab` | `CSI 9 u` | `0x09`, and `CSI Z` with shift |
| `Backspace` | `CSI 127 u` | `0x7f`, `0x08` with ctrl |
| `ArrowUp` / `Down` / `Right` / `Left` | `CSI 1 ; <mod> A` / `B` / `C` / `D` | `CSI A`, or `SS3 A` in cursor key mode |
| `Home` / `End` | `CSI 1 ; <mod> H` / `F` | `CSI H` / `CSI F`, or the `SS3` forms in cursor key mode. `CSI 7 ~` / `CSI 8 ~` are the alternate spellings and are **not** emitted |
| `Insert` / `Delete` | `CSI 2 ; <mod> ~` / `CSI 3 ; <mod> ~` | `CSI 2 ~` / `CSI 3 ~` |
| `PageUp` / `PageDown` | `CSI 5 ; <mod> ~` / `CSI 6 ; <mod> ~` | `CSI 5 ~` / `CSI 6 ~` |
| `F1`-`F4` | `CSI 1 ; <mod> P` / `Q` / `R` / `S` | `SS3 P` / `Q` / `R` / `S` |
| `F5`-`F12` | `CSI 15 ; <mod> ~`, `17`, `18`, `19`, `20`, `21`, `23`, `24` | the same without the modifier |
| `F13`-`F24` | the specification's private-use codes `57376`-`57387` | xterm's shifted `F1`-`F12` forms, as today |
| `KeySpec::Character(s)` | `CSI <code point of the first scalar> u` | the character itself, or the ctrl table |

**Amended after independent verification (`evidence/US-0105-verify.md`, findings 4 and 6).** This
design first kept `F13`-`F24` on xterm's shifted `F1`-`F12` forms and gave `F3` a
`CSI 1 ; <mod> R` form, on the grounds that the legacy rung already sends those. Both were wrong on
the kitty rung and are corrected above:

- **`F3` must not take the `R` final byte.** The specification: "The original version of this
  specification allowed `F3` to be encoded as both `CSI R` and `CSI ~`. However, **`CSI R` conflicts
  with the Cursor Position Report, so it was removed**." `CSI 1 ; 6 R` for `ctrl+shift+F3` is
  byte-identical to a CPR for row 1, column 6. The permitted second form is
  `CSI 1 ; modifier [~ABCDEFHPQS]`, which has no `R` in it. `F3` is `Form::Tilde(13)`.
- **`F13`-`F24` must not assert a shift the user did not press.** Spelling them as shifted
  `F1`-`F12` sets the shift bit in the modifier field, so a program matching `shift+F5` fires on a
  bare `F17`. Under the flags they take the private-use codes; the legacy rung keeps xterm's forms,
  because `US-0099` froze it and a legacy program has no code point to read anyway.

The legacy rung's `F15` is still `CSI 1 ; 2 R` and still collides with a CPR. That is pre-existing,
frozen by the equivalence bar, and named in guide chapter 6 rather than fixed here.

**Not representable, and therefore never emitted:** the keypad keys (`57399`-`57415`), the lock and
system keys (`57358`-`57363`), the media keys (`57428`+) and the modifier keys themselves
(`57441`+). `input::NamedKey` cannot name them, so under `REPORT_ALL_KEYS_AS_ESC` an embedder that
delivers, say, a `Super` press has nothing to deliver it *as*. `NamedKey` is `#[non_exhaustive]`, so
adding them is a patch bump whenever an embedder needs them. Adding sixty variants for a case no
embedder in this repository can produce is the speculative work this design refuses.

### The legacy ctrl table stays exactly as it is

`ctrl_bytes` already implements the specification's "Legacy ctrl mapping" table
(`a`-`z` -> 1-26, space and `@` -> 0, `[` -> 27, `\` -> 28, `]` -> 29, `^` -> 30, `_` and `/` -> 31,
`?` -> 127). It is reached at rung 4 and is untouched.

### Interplay with DECCKM

The specification: "Some keys have an alternate representation when the terminal is in *cursor key
mode* ... This form is used only in cursor key mode and only when no modifiers are present."

That is the rule the current encoder already implements for the legacy path, and it is unchanged.
Under kitty flags the arrows take the `CSI 1 ; <mod> A` form, which is a modified form, so DECCKM
does not apply to it. The one case that needs stating: **an unmodified arrow press with
`DISAMBIGUATE_ESC_CODES` set and nothing else**. Disambiguate only covers "key events that do not
generate text" and the arrows qualify, so the kitty form applies and it is `CSI A` with the
modifier field omitted -- *not* `SS3 A`, even in cursor key mode. The engine's rule, pinned by a
test: **once rung 2 applies, `app_cursor` is not read.**

### Interplay with `modifyOtherKeys`

Reached only at rung 3, only when no kitty flag applied. xterm's form:

```text
CSI 27 ; <modifier> ; <code point> ~
```

| Level | What the encoder does |
| --- | --- |
| `0` | nothing; rung 4 |
| `1` | the `CSI 27 ; ... ~` form for every ctrl or alt chord **except** the ones whose legacy encoding is already a control byte |
| `2` | the `CSI 27 ; ... ~` form for every ctrl or alt chord that is not already an unambiguous functional key |

**Amended after independent verification (finding 5).** This design first described level `1` as
"the chords that have no unambiguous legacy encoding" and listed `Ctrl+Enter`, `Ctrl+Tab` and
`Shift+Enter` among them, which is the complement of xterm's own list. xterm(1):

> **1** -- Enables this feature for keys **except** for those with well-known behavior, e.g., Tab,
> Backarrow and some special control character cases which are built into the X11 library, e.g.,
> Control-Space to make a NUL, or Control-3 to make an Escape character.
>
> **2** -- Enables this feature for keys including the exceptions listed.

Every exception in that sentence is a chord whose legacy encoding *is* a control byte, so the rule
is one predicate rather than a list: `Ctrl+A` stays `0x01`, `Ctrl+Space` and `Ctrl+2` stay `0x00`,
`Ctrl+3` stays `ESC`, `Tab` stays `0x09`, and `Ctrl+;` -- which has no control byte and is exactly
what `modifyOtherKeys` exists for -- escapes. Level `2` drops the exceptions, which is the setting
that makes `Ctrl+I` distinguishable from `Tab`.

**Neither level fires on shift alone.** xterm reaches `modifyOtherKeys` only for a ctrl or alt
chord; the layout has already consumed shift to produce the character, so `Shift+A` is the letter
`A` at every level. The first draft of this design did not say so, and the implementation turned
every capital letter into `CSI 27 ; 2 ; 97 ~` at level `2` until the verification's neighbouring
finding made it visible. `Shift+Enter` therefore leaves the level-1 list with the others.

`US-0099`'s `modify_other_keys_never_reaches_the_bytes` test is **inverted** by this packet: it
becomes the assertion that the level *does* reach the bytes, with the four chords it already names
(`Ctrl+a`, `Ctrl+2`, `Ctrl+Enter`, `Ctrl+Tab`) as the table.

## Edge Cases and Failure Modes

- [ ] **A release event with flags empty** -> `None`. The embedder must tolerate `None` from a key
      it delivered; it already must, because `Ctrl` plus non-ASCII returns `None` today.
- [ ] **`REPORT_ASSOCIATED_TEXT` without `REPORT_ALL_KEYS_AS_ESC`** -> the specification calls it
      undefined. The engine treats the text field as absent, and guide chapter 6 says so. It does
      not guess and it does not refuse the flag: refusing would make `CSI ? u` disagree with what
      was pushed, which is the bug this packet exists to fix.
- [ ] **A `Character` payload with more than one scalar** (a compose result, `\r\n`, two CJK
      scalars) -> the key code is the **first** scalar; the whole string still goes in the text
      field when that field is being emitted. `Ctrl` plus such a payload keeps returning `None`, as
      today.
- [ ] **An empty `Character("")`** -> `None` under every flag combination. Today it returns
      `Some(vec![])` without ctrl; that asymmetry is pre-existing and stays on the legacy rung
      unchanged, so nothing regresses.
- [ ] **A text field containing a C0 or C1 control code** -> the specification forbids it ("no
      C0/C1 control codes allowed"). The encoder drops the text field entirely rather than emitting
      a code point the receiver must reject.
- [ ] **A 100 000-character `Character` payload** -> the text field is bounded. The ceiling is the
      same one the rest of the crate uses for an unbounded client-supplied span; past it the text
      field is omitted and the key code is still emitted. No allocation is unbounded and nothing
      panics. This is the hostile-input case `US-0099` already exercises and it keeps its test.
- [ ] **All 32 flag combinations, including reserved-looking ones** -> `KeyboardFlags` is
      `from_bits_truncate`, so an unknown bit is already dropped at the dispatch site. The encoder
      never sees one.
- [ ] **An alt-screen swap mid-chord** -> the flags swap with the screen, as they already do. The
      encoder reads a snapshot taken at one instant, so it cannot observe a half-swapped stack.
- [ ] **`Alt` on `Insert`, `Tab` and F1-F24 is ignored by the legacy encoder** -- a pre-existing
      defect `US-0099` measured and scoped out. Under kitty flags it is *not* ignored, because the
      modifier goes in a field rather than into a table lookup. The legacy rung keeps the old
      behaviour byte for byte; the two paths therefore disagree for these keys, and guide
      chapter 6 says so rather than quietly fixing one and not the other. Fixing the legacy path is
      a separate packet against a separate claim.

## Verification

- [ ] **The comparison table is the proof.** `crates/vt/src/input/kitty_tests.rs` is table-driven,
      and every row whose expected bytes come from the specification cites the section it came
      from. The mandatory rows are the specification's own worked examples, byte for byte:

      | Input | Expected | Source |
      | --- | --- | --- |
      | `shift+a`, `REPORT_ALL_KEYS_AS_ESC` and `REPORT_ASSOCIATED_TEXT` | `CSI 97 ; 2 ; 65 u` | "Legacy text keys" |
      | `alt+a`, same flags, text `a-ring` | `CSI 0 ; ; 229 u` | same. **Not reachable, recorded as a finding in `US-0105`:** the specification's prose for this row says the OS consumed the modifier and the terminal got a pure text event with *no key information*, which is why the code is `0`. `KeySpec` cannot say "text with no key"; it is `#[non_exhaustive]`, so a `Text` variant is a patch release the day an embedder produces one |
      | `ctrl+space`, legacy | `0x00` | "Legacy ctrl mapping" |
      | `Escape`, `DISAMBIGUATE_ESC_CODES` | `CSI 27 u` | "Functional key definitions" |
      | `Enter` / `Tab` / `Backspace`, `DISAMBIGUATE_ESC_CODES` | `0x0d` / `0x09` / `0x7f` -- unchanged | the explicit exception in "Disambiguate escape codes" |
      | `Enter`, `REPORT_ALL_KEYS_AS_ESC` | `CSI 13 u` | "Report all keys as escape codes" |
      | release of `a`, `REPORT_EVENT_TYPES` and `REPORT_ALL_KEYS_AS_ESC` | `CSI 97 ; 1 : 3 u` | "Report event types" |
      | `ctrl+shift+Up`, `DISAMBIGUATE_ESC_CODES` | `CSI 1 ; 6 A` | "Legacy functional keys" |
      | base layout known, shifted unknown | `CSI 97 : : 97 ; 5 u` | the empty-sub-field rule |

- [ ] **The equivalence test.** With `keyboard_flags` empty and `modify_other_keys == 0`,
      `encode_key_event` returns exactly what `encode_key` returns on `main`, across the full
      `US-0099` cross-product: 75 `KeySpec` values x 8 `KeyMods` x the mode snapshots. `US-0099`
      ran 768 000 comparisons this way; the same generator is reused with the two new fields at
      their defaults. **Zero mismatches is the acceptance bar**, not a sample.
- [ ] **The flag cross-product.** All 32 flag values x 75 keys x 8 modifier sets x 3 event kinds:
      no panic, and every non-`None` result parses back as a well-formed `CSI ... u`, `CSI ... ~`,
      `CSI ... [A-Z]` or legacy byte string. A round-trip parser in the test module, not in the
      crate.
- [ ] **The claim is now true.** One integration test drives a real `Terminal`: feed
      `CSI > 1 u`, assert `CSI ? u` answers `CSI ? 1 u` **and** that `Terminal::encode_key_event`
      for `Escape` returns `CSI 27 u`. That single test is the one the evaluation's gap 2 would
      have failed, and it is what makes guide chapter 11's rule true again.
- [ ] **The corpus does not move.** The 46-recording parity corpus replays byte-identically; the
      encoder is not on the print path, so any difference is a bug in the packet.
- [ ] **The surface file.** `python scripts/vt-public-api.py --check` after regenerating both
      platform files, with `KeyEvent`, `KeyEventKind`, `encode_key_event` and the two
      `ModeSnapshot` fields as the only additions, and `Terminal::encode_key_event` the only new
      method.
- [ ] **A manual Windows walk**, because the acceptance claim is about a program inside a terminal,
      not about a function: a local shell running a short script that pushes `CSI > 1 u` and echoes
      what it receives, then the same over SSH. Attach the transcript.
