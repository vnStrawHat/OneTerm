# Independent verification: US-0105, the kitty keyboard encoder

Packet: [`../US-0105-kitty-keyboard-encoder.md`](../US-0105-kitty-keyboard-encoder.md)
Owning design: [`../low-level-design/kitty-keyboard.md`](../low-level-design/kitty-keyboard.md)
Branch: `feat/vt-kitty-keyboard` @ `341b6b97`, base `main` @ `c5ddad59`
Specification: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>, fetched 2026-09-16
Date: 2026-09-16
Verifier: independent agent, own worktree. Nothing committed to the implementer's branch.

## Verdict

**FAIL.**

Three defects produce wrong bytes or a wrong reply against explicit normative text of the
specification this packet cites, all inside the packet's own scope, none of them disclosed
anywhere in the packet, the design or the guide (findings 1, 2 and 3). Two of the three corrupt
ordinary typed input for a program that negotiated a flag combination this packet claims to
honour. A fourth (finding 4) emits a sequence the specification explicitly removed because it
collides with the Cursor Position Report.

The packet is otherwise unusually honest: it discloses the `F13`-`F24` deviation, the lower-cased
key code, the modifier ceiling, the unrepresentable functional keys, the budget overrun, the
missing platform walk and the missing reference-implementation comparison, and it says in its own
words that it "should not be accepted without the walk". The frozen-copy equivalence run, the
public-surface diff, the per-screen flag stack and the DECCKM rule all check out exactly as
claimed. The failure is not sloppiness; it is four specific rungs of the ladder that were never
tested against the sentences that govern them.

## How this was verified

Expected bytes were transcribed from the specification page's own tables and prose before the
implementation was read in detail, and encoded as a table-driven test file that collects every
mismatch instead of stopping at the first:

- `crates/vt/tests/verify_us0105_independent.rs` (this worktree, **not committed**)
- an external probe crate at `<scratchpad>/fru/` for the `#[non_exhaustive]` check

Nothing in this verification reuses the implementer's test tables or expectations.

## Spec-example scoreboard

211 table rows plus 4 standalone assertions. **43 row mismatches and 2 failed assertions.**

| Test | Spec section | Cases | Mismatches |
| --- | --- | ---: | ---: |
| `spec_functional_key_table_under_report_all_keys` | Functional key codes | 38 | **13** |
| `spec_modifier_arithmetic` | Modifiers | 8 | 0 |
| `spec_event_types_on_a_non_text_key` | Event types | 4 | 0 |
| `spec_text_keys_have_no_event_types_without_report_all_keys` | Event types, the Note | 6 | **6** |
| `spec_associated_text_and_alternate_keys` | Text as code points / Key codes | 10 | **2** |
| `spec_key_code_is_always_the_unshifted_key` | Key codes | 6 | **5** |
| `spec_disambiguate_scope` | Disambiguate escape codes | 10 | 0 |
| `spec_legacy_text_key_example_table` | Legacy text keys / Example encodings | 21 | **3** |
| `spec_legacy_c0_control_table` | Legacy functional keys / C0 controls | 35 | **13** |
| `spec_legacy_ctrl_mapping_table` | Legacy ctrl mapping of ASCII keys | 46 | **1** |
| `spec_legacy_functional_table` | Legacy functional encoding (incl. cursor key mode) | 27 | 0 |
| `kitty_form_ignores_cursor_key_mode` | Disambiguate escape codes | 1 | 0 |
| `the_flag_stack_is_per_screen` | Progressive enhancement, the Note | 1 | 0 |
| `the_live_flags_drive_the_encoder_and_the_query_agrees` | Progressive enhancement / Detection of support | 1 | **1** |
| `the_whole_flag_space_is_well_formed` | 32 x 3 x 25 x 8 x 3 = 57 600 cases, CPR-collision guard | 1 | **1** |

Of the 43 row mismatches, 24 are in the pre-existing legacy rung that `US-0099` froze (findings 8,
9, 10) and 19 are on the new kitty/`modifyOtherKeys` rungs.

## Findings

### 1. HIGH -- a repeat or release of a text key becomes `CSI u` without `REPORT_ALL_KEYS_AS_ESC`

`crates/vt/src/input/kitty.rs:182`

```rust
if event.kind != KeyEventKind::Press && flags.contains(KeyboardFlags::REPORT_EVENT_TYPES) {
    return true;
}
```

The rung is taken before `generates_text` is ever consulted. The specification, under "Event
types":

> **Note** -- Key events that result in text are reported as plain UTF-8 text, so events are not
> supported for them, **unless the application requests key report mode**, see below.

"Key report mode" is `REPORT_ALL_KEYS_AS_ESC` (`0b1000`). So with `REPORT_EVENT_TYPES` set and
`REPORT_ALL_KEYS_AS_ESC` clear, a repeat of `a` must be the byte `a` again ("key repeat events are
treated as key press events") and a release of `a` must send nothing at all.

Measured, 6/6 rows wrong:

| Flags | Event | Spec | Got |
| --- | --- | --- | --- |
| `REPORT_EVENT_TYPES` | repeat of `a` | `a` | `ESC[97;1:2u` |
| `REPORT_EVENT_TYPES` | release of `a` | (nothing) | `ESC[97;1:3u` |
| `DISAMBIGUATE \| REPORT_EVENT_TYPES` | repeat of `a` | `a` | `ESC[97;1:2u` |
| `DISAMBIGUATE \| REPORT_EVENT_TYPES` | release of `a` | (nothing) | `ESC[97;1:3u` |
| `REPORT_EVENT_TYPES \| REPORT_ALTERNATE_KEYS` | repeat of `a` | `a` | `ESC[97;1:2u` |
| `REPORT_EVENT_TYPES \| REPORT_ALTERNATE_KEYS` | release of `a` | (nothing) | `ESC[97;1:3u` |

Flags `0b11` (disambiguate + event types) is a common application setting. Under it, **holding a
letter key down stops inserting that letter and starts emitting escape sequences instead**, and
every key-up emits a spurious sequence. This is input corruption, not a spelling difference.

The implementer's own table test avoids the case by pairing `REPORT_EVENT_TYPES` with
`REPORT_ALL_KEYS_AS_ESC` in every repeat/release row
(`crates/vt/src/input/kitty_tests.rs:182,193`), so the bug is invisible to the packet's evidence.

Fix: add `&& !generates_text(event)` to the rung at `kitty.rs:182`, and route the release of a
text key to `Encoded::Silent`.

### 2. HIGH -- the associated-text field is fabricated for chords that produce no text

`crates/vt/src/input/kitty.rs:279-283`

```rust
let text = match (event.text.as_deref(), &event.key) {
    (Some(text), _) => text,
    (None, KeySpec::Character(payload)) => payload.as_str(),
    (None, _) => return None,
};
```

The fallback looks at the key payload and never at the modifiers. `encode_key` and
`KeyEvent::new` both leave `text` at `None`, so this is the *default* path for every embedder that
does not supply text -- including OneTerm's own view, which supplies none (the packet confirms
this).

| Flags | Event | Spec | Got |
| --- | --- | --- | --- |
| `ALL_ESC \| TEXT` | `ctrl+a` | `ESC[97;5u` | `ESC[97;5;97u` |
| `ALL_ESC \| TEXT` | `alt+a` | `ESC[97;3u` | `ESC[97;3;97u` |

`ctrl+a` produces `0x01`, which the specification forbids in the field ("The associated text must
not contain control codes"); `alt+a` on this platform produces no text either. Reporting `97` says
the event inserted the character `a`. A program under flags `0b11000` (report-all-keys +
associated text -- the combination the specification's own "Report associated text" section tells
applications to use) will therefore **insert a literal `a` for every `Ctrl+A`**.

This reaches OneTerm itself: `crates/terminal-view/src/input/keys.rs:306` calls the three-argument
`encode_key`, which builds a `KeyEvent` with `text: None`, so the fallback is the only path the
application has.

Fix: return `None` from `text_field` when `event.text` is `None` and `mods.ctrl || mods.alt`.

### 3. HIGH -- `CSI ? u` reports the stack top, not the flags the encoder uses

`crates/vt/src/terminal/dispatch.rs:1482-1486`

```rust
// Trap 42: the query reads the stack top, which can legitimately
// differ from the live flags after `CSI = Ps u`.
(b'u', [b'?']) => {
    let flags = self.state.keyboard.active.top().bits();
```

The specification:

> The program running in the terminal can query the terminal for the **current values of the
> flags** by sending `CSI ? u`.

and, under "Detection of support for this protocol":

> applications can detect such implementations by **first setting the desired progressive
> enhancements and then querying for the current progressive enhancement**.

`CSI = Ps ; Pm u` sets the live flags (`FlagStack::apply`) and leaves the stack top alone, so an
application that follows the specification's own detection recipe is told the terminal implements
nothing:

```
feed  CSI = 1 ; 1 u     -> encoder now emits CSI 27 u for Escape   (correct)
feed  CSI ? u           -> reply CSI ? 0 u                          (wrong; must be CSI ? 1 u)
```

Measured by `the_live_flags_drive_the_encoder_and_the_query_agrees`: `left: "\x1b[?0u"`,
`right: "\x1b[?1u"`.

This directly falsifies the packet's headline claim and the commit message
`test(vt): prove the negotiated protocol is the one actually spoken`. The packet's integration
test (`crates/vt/tests/verify_us0105.rs`) only exercises the `CSI > 1 u` push route, where `push`
happens to set `live` and `top` together, so it passes while the `CSI = Ps u` route -- the one the
specification recommends -- is broken.

The divergence predates this packet (the `live`/`top` split came with the flag stack), but
US-0105 is the packet that makes the query meaningful, and its acceptance criterion is exactly
"what `CSI ? u` reports is what the key bytes actually are" (guide chapter 11, added by this
branch). It must be fixed here or the new guide sentence must be withdrawn.

Fix: reply with `live()`. Kitty and Ghostty keep a single current value that *is* the stack top;
if the split is kept for another reason, the reason belongs in a decision record, not in a
comment calling itself a trap.

### 4. MEDIUM-HIGH -- `F3` emits `CSI R` / `CSI 1;<mod>R`, which the specification removed

`crates/vt/src/input/kitty.rs:93` (`F3`) and `:105` (`F15`)

The specification's functional key table gives `F3` the single form `13 ~`, and adds:

> **Note** -- The original version of this specification allowed F3 to be encoded as both `CSI R`
> and `CSI ~`. However, **`CSI R` conflicts with the Cursor Position Report, so it was removed**.

The "Disambiguate escape codes" section also fixes the permitted second form as
`CSI 1 ; modifier [~ABCDEFHPQS]` -- `R` is not in that set. Measured under `REPORT_ALL_KEYS_AS_ESC`:
`F3` gives `ESC[R` where the table says `ESC[13~`; with `ctrl+shift` it gives `ESC[1;6R`, which is
byte-identical to a Cursor Position Report for row 1, column 6. `F15` inherits the same final byte
(`ESC[1;2R`).

The packet mentions `F3` only in a subordinate clause of the `F13`-`F24` gap bullet ("which also
shows `F3` has no letter form there") and guide chapter 6 does not mention `F3` at all, so the one
deviation that creates an *ambiguous* sequence is the one that is not documented. The
justification offered for `F13`-`F24` -- "that is what the legacy path already sends" -- does not
transfer: the legacy path sends `SS3 R` for plain `F3`, which does *not* collide, while the kitty
rung sends `CSI R`, which does.

**Must be fixed before merge.** `Form::Tilde(13)` for `F3`, and `F15` needs a separate answer.

### 5. MEDIUM -- `modifyOtherKeys` level 1 is inverted against xterm's documented exception list

`crates/vt/src/input/kitty.rs:321-340`

xterm's own resource documentation for `modifyOtherKeys`:

> **1** -- Enables this feature for keys **except** for those with well-known behavior, e.g., Tab,
> Backarrow and some special control character cases which are built into the X11 library, e.g.,
> **Control-Space to make a NUL, or Control-3 to make an Escape character**.

`ambiguous_in_legacy` implements the complement of that list:

| Chord, level 1 | xterm | This encoder |
| --- | --- | --- |
| `ctrl+a` | `CSI 27;5;97~` | `0x01` |
| `ctrl+2` | `0x00` (X11 special case kept) | `CSI 27;5;50~` |
| `ctrl+3` | `0x1b` (X11 special case kept) | `CSI 27;5;51~` |
| `ctrl+Tab` | `0x09` (well-known key kept) | `CSI 27;5;9~` |

The code comment states the inverted rule as if it were xterm's ("`Ctrl+a` is `0x01` and
unambiguous; `Ctrl+3` is `ESC`, which is not"). Level 2 and the `CSI 27;m;code~` shape itself are
correct. Note that the kitty page itself says `modifyOtherKeys` "is completely unspecified", so
the xterm resource text is the best available authority; I could not reach xterm's ctlseqs source
from this network (see "Could not verify").

Related, same function: `Escape` is never an "other" key at any level (`kitty.rs:311`, the `_`
arm), where xterm level 2 sends `CSI 27;5;27~` for `ctrl+Escape`.

### 6. MEDIUM -- `F13`-`F24` assert a shift the user did not press (disclosed)

`crates/vt/src/input/kitty.rs:107-118`. Confirmed: 12/12 rows deviate from the specification's
`57376`-`57387`. The packet and guide chapter 6 both disclose this and the reasoning (agreement
with the legacy rung) is defensible **in legacy mode**, where the specification explicitly permits
a terminal to choose ("Terminals may choose what they want to do about functional keys that have
no legacy encoding").

It is not defensible under `REPORT_ALL_KEYS_AS_ESC`, because the encoder does not merely spell the
key differently -- it sets the shift bit in the modifier field (`csi_u`'s `implies_shift`), telling
the program a modifier is held that is not. A program doing shortcut matching against
`shift+F5` will fire on a bare `F17`. Acceptable to merge *given the disclosure*; must be fixed
before the crate is published as a kitty-protocol implementation.

### 7. MEDIUM -- the un-shifted key code, and a workaround the guide states that does not work

`crates/vt/src/input/kitty.rs:125-128`. Confirmed: 5/6 rows. `ctrl+shift+1` reports `33` (`!`)
where the specification requires `49` (`1`); `shift+4` reports `36`, `shift+=` reports `43` where
`61` is required. The specification is unambiguous ("the codepoint used is always the lower-case
(or more technically, **un-shifted**) version of the key"), and its legacy section gives the shift
relation for exactly these keys ("output the shifted key, for example, `A` for `a` and `$` for
`4`").

The deviation is disclosed. **The stated remedy is not correct**, and that part is a defect in the
documentation rather than a ceiling: guide chapter 6 says "Supply `base_layout` when your platform
knows better", and the packet says "An embedder that knows better supplies `base_layout`".
`base_layout` is written into the *third* colon sub-field (`kitty.rs:250-258`) and never replaces
the primary key code, so supplying it does not make `ctrl+shift+1` report `49`. There is no field
on `KeyEvent` that can. The honest statement is that the primary code is wrong for shifted
punctuation and the API has no way to correct it -- the same shape as the `KeySpec::Text` gap the
packet does record.

### 8. LOW-MEDIUM -- the legacy rung drops `alt` on more keys than the guide lists

`crates/vt/src/input/key.rs:293-303,313`. The guide and packet name `Insert`, `Tab` and
`F1`-`F24`. Measured against the specification's "C0 controls" table, `Escape`, `Enter` and
`Backspace`-with-`ctrl` drop it too:

| Chord | Spec | Got |
| --- | --- | --- |
| `alt+Escape` | `ESC ESC` | `ESC` |
| `alt+Enter` | `ESC 0x0d` | `0x0d` |
| `ctrl+alt+Backspace` | `ESC 0x08` | `0x08` |
| `alt+shift+Tab` | `ESC CSI Z` | `CSI Z` |

`alt+Escape` and `alt+Enter` are in daily use (emacs `ESC ESC ESC`, shells' `alt+Enter`). The
behaviour is pre-existing and frozen by the equivalence bar, so this is a documentation-completeness
finding against a list that claims to be exhaustive, not a demand to change the bytes in this packet.

Also in this family and not measured by the packet: the same table gives `ctrl+Enter` and
`shift+Enter` as `0x0d`, where this encoder sends `CSI 13;5u` / `CSI 13;2u` (13 of 35 C0-control
rows differ in total). That is xterm's table rather than kitty's, which is a legitimate choice for
a legacy path, but the guide says "the rules the encoder follows" without saying whose rules.

### 9. LOW -- `ctrl+~` is missing from the legacy ctrl table the design says is complete

`crates/vt/src/input/key.rs:213-220`. The specification's "Emitted bytes when ctrl is held down"
table has a row `~ -> 30`; `ctrl_bytes` maps `^` to `30` and falls through for `~`, emitting `~`.
45/46 rows pass. The owning design states the table is already implemented and enumerates it
without `~` (`low-level-design/kitty-keyboard.md`, "The legacy ctrl table stays exactly as it is"),
so the claim is false by one row. One character in a match arm.

### 10. LOW -- `ctrl+shift+<text key>` in legacy mode is not the `CSI u` the spec's own table requires

Measured 3/21 rows of the "Example encodings" table: `ctrl+shift+i` gives `0x09` where the table
gives `CSI 105;6u`; `ctrl+shift+3` gives `#`; `ctrl+shift+;` gives `:`. The specification's
"Legacy text keys" algorithm covers only `shift`, `alt`, `ctrl`, `shift+alt`, `ctrl+alt` and then
says "Any other combination of modifiers with these keys is output as the appropriate `CSI u`
escape code" -- i.e. `ctrl+shift` is `CSI u` **even with no progressive enhancement**. (The page
contradicts itself once, in an informal Note claiming `ctrl+r` and `ctrl+shift+r` are the same in
legacy mode; the table is the normative artefact.)

Pre-existing, and directly in tension with the packet's own "nothing changes when nothing is
negotiated" acceptance bar, which freezes it. Worth one line in the guide so the next reader does
not treat the legacy rung as spec-complete.

### 11. LOW -- `#[non_exhaustive]` on `ModeSnapshot` is a breaking change filed under "Added"

Confirmed from an external crate (scratchpad probe, `cargo build`):

```
error[E0639]: cannot create non-exhaustive struct using struct expression
 --> src\lib.rs:6:5   ModeSnapshot { app_cursor: true, ..ModeSnapshot::default() }
error[E0639]: cannot create non-exhaustive struct using struct expression
 --> src\lib.rs:14:5  KeyEvent { kind: ..., ..KeyEvent::new(...) }
```

Functional-update syntax is refused for both; `KeyMods` (unmarked) and the documented
`default() + assign` pattern both compile. This is the specific hazard the US-0106 implementer
raised, and it is real for `ModeSnapshot`: external code that previously wrote
`ModeSnapshot { .., ..Default::default() }` no longer compiles at all. The branch's own harness
had to be rewritten from FRU to assignment for exactly this reason
(`crates/vt/tests/verify_us0099_equiv.rs:678-698`), which is the proof that the pattern was in use.

Guide chapter 12 states the cost correctly ("you cannot build one with a struct literal") and the
migration path is one line, so the risk is small -- but the CHANGELOG files the mark under
**`### Added`**, phrased as a benefit ("so the next field is a patch rather than another minor"),
while the two breaking consequences (the added fields, and the ban on struct expressions) belong
in `### Changed` with the **Breaking** marker the file uses elsewhere.

`KeyEvent` and `KeyEventKind` are new types, so their marks cost nothing.

### 12. INFO -- ceilings confirmed as disclosed, not re-litigated

- `KeyMods` carries shift/ctrl/alt only, so modifier values `1`-`8` are the whole range. **The
  caps-lock (`64`) and num-lock (`128`) bits of the check list cannot be produced or tested** --
  the specification requires them under `REPORT_ALL_KEYS_AS_ESC` ("if the lock is enabled, the key
  event must have the bit for that modifier set"), and the crate cannot express them. Disclosed in
  the guide and the design.
- Keypad (`57399`-`57427`), lock/system (`57358`-`57363`), media (`57428`+) and modifier keys
  (`57441`+) have no `NamedKey` variant, so **the check-list item "keypad keys carry their PUA
  codes" could not be tested**; nothing can deliver such a key. DECKPAM/`app_keypad` is therefore
  not read by the encoder at all. Disclosed.
- `KeySpec::Text` (the `alt+a -> CSI 0;;229 u` row) and the `terminal-view` key-up/`is_held` gap
  are both recorded in the packet with a named next owner. Confirmed by reading
  `crates/terminal-view/src/input/keys.rs` and the packet's Handoff section: accurate.

## What checked out

- **The frozen-copy equivalence harness.** `verify_us0099_equiv.rs` asserts
  `keyboard_flags.is_empty() && modify_other_keys == 0` on every generated snapshot before use,
  compares both entry points against the inlined `0558fa2` oracle, and reports
  `encode_key cases compared: 3072000` with zero mismatches. 75 x 8 x 2560 x 2 = 3 072 000: the
  arithmetic and the claim hold.
- **Modifier arithmetic** (8/8), **event types on non-text keys** (4/4), **disambiguate scope**
  including the `Enter`/`Tab`/`Backspace` exception (10/10), **the legacy functional table**
  including cursor key mode (27/27), **the legacy ctrl mapping** (45/46).
- **DECCKM is not read once the kitty rung applies** -- `CSI A`, never `SS3 A`, under
  `DISAMBIGUATE_ESC_CODES` with `app_cursor` set. Matches kitty, which consults cursor key mode
  only on the legacy path.
- **The flag stack is per screen.** A push on the main screen does not follow `CSI ?1049h` into
  the alternate screen, and the main screen's flag survives the return.
- **The whole flag space is safe.** 57 600 independent cases (32 flags x 3 `modifyOtherKeys`
  levels x 25 keys x 8 modifier sets x 3 event kinds): no panic, every parameter byte is a digit,
  `;` or `:`, every final byte is a letter or `~`. The only structural complaint is the `R` final
  byte of finding 4.
- **Hostile input is bounded.** Empty text, `NUL`, 4-byte scalars, `U+10FFFF`, combining pairs,
  10 000-character and 10 000-emoji payloads, across all 32 flags x 3 event kinds x 4 modifier
  sets (2 688 cases): no panic, and nothing the encoder *builds* exceeds 1 KiB (`TEXT_SCALAR_MAX = 32` scalars
  drops the field). The legacy rung still writes an oversized payload through unchanged, which is
  pre-existing and stated.
- **The public surface diff** is exactly the planned additions, +16 lines on each platform file,
  nothing removed.
- **Records.** The status ticks match reality (Implemented ticked, budget and platform-walk items
  honestly left unticked); the budget overrun is disclosed with numbers (+514 production against
  +430, +790 tests against +400) rather than absorbed; the harness snippet's proof row
  (`unit_proof=1, integration_proof=1, e2e_proof=0, platform_proof=0, intake_id=44`) agrees with
  the `HARNESS:PROOF` block; CHANGELOG clause 6 is amended in both the CHANGELOG and guide
  chapter 12 and marked **Breaking**; guide chapters 6, 11 and 12 are all updated as the
  Documentation Action promised. The `KeySpec::Text` gap and the `terminal-view` key-up gap are
  both recorded with owners in Handoff.

## Commands

```
git reset --hard feat/vt-kitty-keyboard           # 341b6b97, base c5ddad59
$env:CARGO_BUILD_JOBS=3
cargo test -p oneterm-vt                          # branch as delivered: pass
cargo test -p oneterm-vt --test verify_us0105_independent -- --nocapture --test-threads=1
cargo build --manifest-path <scratchpad>/fru/Cargo.toml   # external #[non_exhaustive] probe
pwsh scripts/ci-local.ps1 -Full                   # see below
```

## Gates

`pwsh scripts/ci-local.ps1 -Full`, run on the branch **as delivered** (my test file was moved out
of the tree for this run and restored afterwards). Full log kept privately at
`<scratchpad>/ci-full.log`, 3 226+ lines. Every step passed, in this order:

| Step | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings` | pass |
| `cargo test --workspace` (covers `oneterm-terminal` and `oneterm-terminal-view`) | pass |
| `cargo test -p oneterm-vt --features vt-paranoid` | pass |
| `cargo test -p oneterm-vt --features regex` | pass |
| `cargo build -p oneterm-vt --no-default-features --examples` | pass |
| `cargo test -p oneterm-vt --no-default-features` | pass |
| `cargo build -p oneterm-vt --all-features --examples` | pass |
| `cargo tree -p oneterm-vt -e normal --no-default-features` | pass |
| `cargo run -p oneterm-vt --example headless` | pass |
| `cargo doc -p oneterm-vt --no-deps` and `--all-features` (`-D warnings`) | pass |
| `python scripts/vt-public-api.py --check --no-doc` | pass |
| `python scripts/vt-public-api.py --diff-platforms` | pass -- "the delta is 6 lines, all inside `oneterm_vt::pty`", matching the packet |
| `cargo package -p oneterm-vt --list \| verify-dependency-graph.py --package-list -` | pass |
| rustdoc self-containment (`crates/vt/src`, `crates/vt/docs/guide`) | pass |
| `python scripts/verify-dependency-graph.py` | pass |
| `python scripts/check-doc-paths.py` | pass |
| `python -m unittest scripts/test_check_english.py` | pass |
| `python scripts/check-english.py` | pass (888 files) |
| `python scripts/completion-catalog.py validate` | pass |
| `python scripts/third-party-notices.py --check` | pass |
| `cargo deny check licenses bans advisories` | pass -- "advisories ok, bans ok, licenses ok" (the advisory database was reachable) |
| | **`ci-local: all checks passed.`** |

`cargo test -p oneterm-vt` alone (default features) also passed before any of my files were added.

The gate being green is exactly the point of findings 1-4: **every one of them is invisible to this
gate**, because the branch's own tables never pair the flags in the combinations that expose them.
A passing `ci-local -Full` is not evidence that the encoder speaks the protocol.


## Could not verify

- **No comparison against a live reference implementation.** Same limitation the packet states.
  Every expectation here is derived from the specification page's prose and tables, not from bytes
  captured from kitty, ghostty or foot. Where the specification is silent or self-contradictory
  (finding 10) this is called out in the finding.
- **xterm's `modifyOtherKeys` source.** `invisible-island.net` and `xfree86.org` were both
  unreachable from this network for the pages that carry the resource text; finding 5 rests on the
  xterm(1) resource description as reproduced in search results, not on xterm's own source. The
  direction of the inversion is clear enough to act on; the exact keycode xterm sends for a
  shifted key (`65` vs `97`) is **not** established here and is deliberately not raised as a
  finding.
- **Caps lock and num lock (bits 64 / 128), keypad PUA codes, DECKPAM, media and modifier keys.**
  Unrepresentable in `KeyMods` / `NamedKey`; see finding 12. No test can exist until the types grow.
- **The manual Windows walk.** Not attempted: it means launching a second `oneterm.exe` on a
  machine whose owner runs their agent inside the first. The packet's own assessment -- that it
  should not be accepted without the walk -- stands, and this verification does not substitute for it.
- **`terminal-view` release/repeat delivery.** Read, not exercised; the packet's description of
  the gap (GPUI has `KeyUpEvent` and `is_held`, `TerminalView` registers only `on_key_down`)
  matches the source.

## What must happen before merge

1. Finding 1 -- `kitty.rs:182` must not take the kitty rung for a text-producing key on
   `REPORT_EVENT_TYPES` alone. Add the regression row that the implementer's table skipped.
2. Finding 2 -- `text_field` must not invent a text field for a `ctrl`/`alt` chord.
3. Finding 3 -- `CSI ? u` must answer the live flags, or guide chapter 11's new sentence and the
   packet's headline claim must be withdrawn.
4. Finding 4 -- `F3` must leave the `R` final byte.
5. Findings 5, 7, 9, 11 -- small and local; 6, 8, 10, 12 are documentation or disclosed ceilings.

Because the contract is "the bytes a program parses", each of 1-4 is the kind of change clause 6
of the semver promise now covers: they belong in this packet, before the promise is published.

---

# Re-verification at 63c99535

Branch `feat/vt-kitty-keyboard` @ `63c99535`, rebased onto `main` @ `4437b98e`.
Date: 2026-09-16. Same verifier, same rules, nothing committed.

**Verdict: PASS-WITH-NOTES.** All four blocking findings above are closed and verified closed.
One new byte defect (R1) and three record/prose precision notes remain.

## Method

The branch adopted this report's test file, so it was not trusted: a second file was written from
the specification page again, with its own expectation derivations and its own shape rules --
`crates/vt/tests/reverify_us0105.rs` (this worktree, **not committed**). Nine tests, plus a
95 976-case CPR sweep and a 23 040-case hostile sweep. Two of its rows were wrong and were
corrected against the specification, not against the engine (below).

| Re-derived test | Cases | Mismatches |
| --- | --- | ---: |
| `event_type_matrix_over_every_flag_set` (32 flags x 7 key classes x repeat/release) | 448 | **4** |
| `associated_text_over_every_modifier_set` | 9 | 0 |
| `modify_other_keys_matrix` (levels 0/1/2 x letters, digits, punctuation, space, Tab, Enter, Backspace, Escape, shift-only, functional) | 26 | 0 |
| `unshifted_key_code_for_every_printable_ascii_key` (PC-101) | 68 | 0 |
| `function_keys_in_both_rungs` (F1-F24, plus a no-phantom-shift assertion each) | 24 | 0 |
| `cursor_key_mode_and_keypad_mode` (DECCKM on both rungs, DECKPAM inert) | 122 | 0 |
| `alternate_key_sub_fields` | 5 | 0 |
| `the_detection_recipe_and_the_stack` (13 assertions) | 13 | 0 |
| `hostile_text_is_bounded` | 23 040 | 0 |
| `no_kitty_form_collides_with_the_cursor_position_report` | 95 976 | **240** |

Two expectations of mine were wrong and were fixed rather than reported: a repeat of a text key
**does** carry the associated-text field (a repeat inserts the character), and one alternate-key
row passed the wrong payload. Both were my error, not the engine's.

## Closed, and verified closed

| # | Closed | How it was re-checked |
| --- | --- | --- |
| 1 | yes | The 448-case event-type matrix: `reportable` matches the specification's rule exactly for text keys, `Enter`/`Tab`/`Backspace`, functional keys and `Escape`, across all 32 flag sets. A repeat under `REPORT_EVENT_TYPES` alone is the character again; a release is silent |
| 2 | yes | 9 modifier combinations under `ALL_ESC \| TEXT`: no text field for ctrl, alt, ctrl+alt, ctrl+shift or ctrl+shift+alt; the field present for plain and shift-only |
| 3 | yes | The specification's detection recipe end to end: `CSI = 5 ; 1 u` then `CSI ? u` answers `CSI ? 5 u`; union (mode 2) and difference (mode 3) both visible; push/pop; a pop past the depth resets; separate stacks across `CSI ? 1049 h` / `l` |
| 4 | yes, on the kitty rung | `F3` is `CSI 13 ~`; `F15` is `CSI 57378 u`. The 95 976-case sweep finds **no** `R` final byte on the kitty rung. See R2 for the legacy rung |
| 5 | yes | Level 1 keeps `ctrl+a`, `ctrl+space`, `ctrl+3`, `ctrl+Tab`, `ctrl+Backspace` legacy and escapes `ctrl+;`, `ctrl+0`, `ctrl+1`, `ctrl+9` -- xterm(1)'s exception list, not its complement. Level 2 escapes all of them and `ctrl+Escape` too. **No level fires on shift alone**, so a capital letter stays a capital letter, which the packet discloses its own first draft got wrong |
| 6 | yes | All 24 F-keys take the private-use codes, and for each one `shift+Fn` differs from `Fn`, so no modifier bit is asserted that was not pressed |
| 7 | yes | The PC-101 table is correct for all 21 shifted-punctuation pairs, all 26 letters and all 21 un-shifted keys, re-derived independently |
| 8, 10 | yes (documented) | Guide chapter 6's five-row disagreement table matches what the engine does, row for row |
| 9 | yes | `ctrl+~` is `0x1e`. The frozen oracle in `verify_us0099_equiv.rs` was **not** edited: the divergence is asserted in both directions, counted, and pinned to an exact size (`4 x snapshots x 2`), so a second row cannot move unnoticed. This is the right mechanism and it matters for R2 |
| 11 | yes | `E0639` is named in the CHANGELOG under `### Changed` / **Breaking** with both consequences, and in guide chapter 12 with "including functional update syntax" |
| 12 | yes | Chapter 6 now names the caps-lock/num-lock bits and `DECKPAM` as unreachable. Confirmed by test: `app_keypad` changes nothing the encoder returns, under any flag set |

**The 16 frozen deviations are genuinely pre-existing, and this is proved rather than asserted.**
All 16 are at `keyboard_flags == 0` and `modify_other_keys == 0`, and the frozen-copy harness
compares that exact configuration against the un-edited `0558fa2` oracle across 3 072 000 cases
with exactly one named divergence. Anything at flags 0 that differed from `main` would fail that
run, so the freeze claim is true by construction. The `modifyOtherKeys 1: ctrl+2` correction is
sound: xterm(1) names `Control-Space to make a NUL` in its level-1 exception list and `ctrl+2` is
that chord's alias, so `0x00` is right and my original row was wrong.

## Remaining findings

### R1. LOW-MEDIUM -- a **release** of a text key still carries the associated-text field

`crates/vt/src/input/kitty.rs`, `text_field`: it consults `event.mods` but never `event.kind`.

| Flags | Event | Spec | Got |
| --- | --- | --- | --- |
| `EVT \| ALL \| TXT` and its three supersets | release of `a` | `ESC[97;1:3u` | `ESC[97;1:3;97u` |

A key release inserts nothing, so it has no associated text; kitty sends the field on press and
repeat only. This is the same defect as finding 2, on the event-kind axis instead of the modifier
axis, and it survived because the fix added a modifier guard and not a kind guard. Four flag sets,
4 of 448 rows. One condition: drop the payload fallback when `event.kind == Release`.

### R2. MEDIUM -- the legacy rung's `F15` CPR collision is reachable **with flags negotiated**, and has no owner

240 of 95 976 swept cases end in the final byte `R`. Every one is `F15` on the legacy rung -- and
every one is at a **non-empty** flag set: `REPORT_EVENT_TYPES`, `REPORT_ALTERNATE_KEYS` or
`REPORT_ASSOCIATED_TEXT` pushed alone, and their combinations, at all three `modifyOtherKeys`
levels. A program that pushed `CSI > 2 u` and pressed `F15` receives `ESC[1;2R`, which is
byte-identical to a Cursor Position Report for row 1, column 2.

Three things follow:

1. Guide chapter 6's mitigation sentence -- "If you need the specification's legacy tables rather
   than xterm's, push `DISAMBIGUATE_ESC_CODES`" -- is narrower than it reads. It is true for
   `DISAMBIGUATE_ESC_CODES` and `REPORT_ALL_KEYS_AS_ESC` and false for the other three flags.
2. The packet's finding-4 closure says "A new test walks every flag set and both keys and asserts
   the final byte is never `R`". It walks 24 of the 31 non-empty flag sets: `kitty_tests.rs`
   skips `bits & 0b1001 == 0` and the adopted sweep's key list omits `F15` entirely. Both
   exclusions carry honest comments in the code; the packet prose does not carry the caveat.
3. "Frozen by the equivalence bar" is a **choice here, not a constraint**. `ctrl+~` (finding 9)
   proves the mechanism for a deliberate, named, counted legacy divergence already exists inside
   this packet. A collision with a reply sequence is a correctness hazard rather than a spelling,
   and kitty itself resolves it the same way ("kitty chooses to encode these using `CSI u`
   encoding even in legacy mode"). Either move the legacy `F13`-`F24` to their private-use codes
   with the same named-divergence treatment, or record a follow-up packet ID and owner. At present
   the note appears in three documents and **no packet owns it**.

### R3. LOW -- the CHANGELOG describes ceilings this release removed

`crates/vt/CHANGELOG.md`, the clause-6 `Changed` entry, still reads:

> Guide chapter 6 has the decision ladder, the flag table and the ceilings -- modifier values
> `1`-`8` only, no private-use functional keys, **`F13`-`F24` on xterm's shifted forms**, and
> **the un-shifted key code derived by lower-casing, which is wrong for shifted punctuation** and
> has no API to correct it.

Both bolded clauses were made false by this same rework: the kitty rung now uses `57376`-`57387`
(finding 6) and the un-shifted code goes through the PC-101 table, so shifted punctuation is right
(finding 7). Guide chapter 6 says so correctly in three places; the CHANGELOG, which is the file a
consumer reads first, contradicts it. Two clauses to edit.

### R4. LOW -- the packet was reworked after a FAIL without being reopened

`Status` still reads `- [x] Implemented` with `- [ ] Reopened (acceptance rework)` unticked, while
`## Verification notes closed` records a full rework answering twelve findings from an independent
FAIL. `AGENTS.md` calls that acceptance rework of the owning US and asks for the reopen. Everything
else in the records is consistent: the `HARNESS:PROOF` block (`unit`/`integration`/`verify` ticked,
`e2e`/`platform` not) matches the snippet's `unit_proof=1, integration_proof=1, e2e_proof=0,
platform_proof=0`, `intake_id=44` is correct, and the budget and gate lines were updated.

### R5. INFO -- unresolved, deliberately not raised as a defect

`alt` alone at `modifyOtherKeys` level 1. This engine escapes `alt+a` to `CSI 27;3;97~`; the
pre-rework code sent `ESC a`. xterm(1) is quoted both ways in the two secondary sources reachable
from this network ("The Alt- and Meta- modifiers do not cause xterm to send escape sequences" for
value 1, versus a paraphrase saying they do), and `invisible-island.net` remained unreachable. The
measured behaviour is printed by `print_alt_at_modify_other_keys_level_one` so the maintainer can
decide; no claim is made here.

## Gates at 63c99535

`pwsh scripts/ci-local.ps1 -Full` on the branch as delivered (my re-derivation file moved out of
the tree for the run, restored after). Private log `<scratchpad>/ci-full-2.log`, 21 653 lines.
**`ci-local: all checks passed.`** -- fmt, both clippy passes, `cargo test --workspace`,
`vt-paranoid`, `regex`, the `--no-default-features` build and test, the `--all-features` build,
`cargo tree`, the headless example, both `cargo doc` runs with `-D warnings`,
`vt-public-api.py --check --no-doc`, the new `--check-nameable`, `--diff-platforms`
("the delta is 6 lines, all inside `oneterm_vt::pty`"), the package list, both rustdoc
self-containment greps, the dependency graph, doc paths, the check-english unit tests,
check-english, the completion catalogs, third-party notices, and
`cargo deny check licenses bans advisories` ("advisories ok, bans ok, licenses ok" -- the advisory
database was reachable).

As before, the gate is green and R1 and R2 are invisible to it: no committed test pairs
`REPORT_ASSOCIATED_TEXT` with a release, and both `F15` guards exclude the flag sets where the
collision lives.

## Could not verify (unchanged)

No live reference-implementation comparison; xterm's own pages still unreachable; caps lock, num
lock, the keypad, media and modifier keys remain unrepresentable in `KeyMods` / `NamedKey`; the
manual Windows walk was not attempted and the packet's own "should not be accepted without the
walk" still stands.

---

# Final confirmation at 4574b6ac

Branch `feat/vt-kitty-keyboard` @ `4574b6ac`. Date: 2026-09-16. Nothing committed by the verifier.

**Verdict: PASS.** Both remaining byte defects are fixed, the three record and prose notes are
closed, and the one expectation of mine the branch corrected was right to correct.

| | Confirmed |
| --- | --- |
| R1 | `text_field` returns `None` on `KeyEventKind::Release`. The 448-case event-type matrix is **0 mismatches**: a release of a text key under `EVT \| ALL \| TXT` is `ESC[97;1:3u`, and a repeat still carries `;97` because a repeat does insert. Both halves asserted, on all 32 flag sets |
| R2 | Legacy `F15` is `CSI 28 ~` -- xterm's and rxvt's own `kf15`, and the DEC VT220 code -- which collides with nothing. The equivalence harness reports `3072000` compared, `61440 deliberately moved (ctrl+~, F15)`, **0 mismatches**, so the divergence is named and counted rather than tolerated, and the moved count is pinned. The in-crate guard now walks `0u8..32` -- 32 of 32 flag sets, the empty one included -- on both rungs, and my sweep covers the 31 non-empty ones: no CSI sequence anywhere ends in `R`. Excluding `SS3 R` for a plain `F3` is correct: a Cursor Position Report is `CSI <row> ; <col> R`, so only a CSI introducer can be mistaken for one, and the test says so |
| R3 | The clause-6 entry now reads "the three remaining ceilings -- modifier values `1`-`8` only, no private-use keypad, lock, media or modifier keys, and an un-shifted key code derived from the PC-101 shift relation". Both stale clauses are gone, and `F15` has its own **Breaking, clause 6** entry |
| R4 | `- [x] Reopened (acceptance rework)` is ticked and the harness snippet carries `status="reopened"` |
| R5 | Recorded twice: a row in the findings table and a paragraph in Gaps, both naming the ambiguity and claiming nothing. Measured behaviour, printed by the test: `alt+a` is `ESC a` at level 0 and `CSI 27;3;97~` at levels 1 and 2 |

## The corrected row: the branch is right, I was wrong

My adopted row asserted that `CSI < u` restores the live value in force before the push. It does
not, and the specification's own sentence is the one my row quoted two lines later:

> If a pop request is received that **empties the stack, all flags are reset**.

In that sequence `CSI = 4 ; 3 u` had set the live flags to `3` **without pushing**, so the stack was
still empty and the following `CSI > 1 u` was its first and only entry. Popping it therefore empties
the stack, and the flags reset to `0` -- the pre-push live value was never on the stack and there is
nothing in the specification that preserves it. `CSI ? 0 u` is correct and my `CSI ? 3 u` was not.
The branch also added the case my row was reaching for, with two entries on the stack, where the pop
really does uncover the older one (`CSI > 3 u`, `CSI > 1 u`, pop, `CSI ? 3 u`). That is the right
correction and the right addition.

## Test results at this commit

```
[event-type-matrix]  448 cases, 0 mismatches      [pc101-shift-table]    68 cases, 0 mismatches
[associated-text-modifiers] 9 cases, 0 mismatches [function-keys]        24 cases, 0 mismatches
[modify-other-keys] 26 cases, 0 mismatches        [deckm-deckpam]       122 cases, 0 mismatches
[alternate-keys]     5 cases, 0 mismatches        [legacy-f15] F15 on the legacy rung -> ESC[28~
reverify_us0105:              11 passed, 0 failed
verify_us0105_independent:    17 passed, 0 failed, 16 frozen deviations (13 + 3, all at flags 0)
verify_us0099_equiv:  encode_key cases compared: 3072000, 61440 deliberately moved
                      (ctrl+~, F15), 0 mismatches
```

## Gates

`pwsh scripts/ci-local.ps1 -Full` on the branch as delivered, with no file of mine moved out --
the re-derivation test is committed now, so the gate ran it too. Private log
`<scratchpad>/ci-full-3.log`, 21 720 lines. **`ci-local: all checks passed.`** All 24 steps,
including `vt-public-api.py --check --no-doc`, `--check-nameable --no-doc`, `--diff-platforms`
("the delta is 6 lines, all inside `oneterm_vt::pty`"), both `cargo doc` runs under `-D warnings`,
the package list, the two rustdoc self-containment greps, check-doc-paths, check-english, and
`cargo deny check licenses bans advisories` ("advisories ok, bans ok, licenses ok").

## Standing, unchanged

The manual Windows walk was still not run, and the packet's own "should not be accepted without the
walk" still stands -- that is the one acceptance item this verification cannot supply. There is
still no comparison against a live reference implementation, `alt` at `modifyOtherKeys` level 1 is
still unresolved upstream, and caps lock, num lock, the keypad, the media keys and the modifier keys
remain unrepresentable in `KeyMods` / `NamedKey` and therefore untestable.
