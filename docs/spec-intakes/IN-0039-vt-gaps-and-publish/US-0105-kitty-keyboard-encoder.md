# Work: the key encoder honours the kitty keyboard flags it already answers

ID: US-0105
Intake: [`IN-0039`](IN-0039.md)
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: **existing-contract change** (the bytes `encode_key` returns for a given input)
- Risk lane: high_risk (inherited: an external public contract, and this is the packet that changes
  behaviour rather than adding to it)
- Spec Intake, when required: [`IN-0039`](IN-0039.md)

## Outcome

A program that negotiates the kitty keyboard protocol with this terminal receives the bytes that
protocol defines.

Today it does not. `CSI > 31 u` sets the flags, `CSI ? u` answers `CSI ? 31 u`, and `encode_key`
still returns `ESC O A` for Up and `a` for the `a` key. The outside evaluation put it plainly:

> A program that negotiates kitty keyboard and is told yes will send keys it cannot receive. ...
> chapter 11's own rule -- never claim a capability that does not exist -- is violated here. Either
> implement the encoding or refuse the push.

This packet implements the encoding. It also wires `modifyOtherKeys`, which is stored and unread
for the same reason and would otherwise be the identical bug one release later.

## Scope

- [x] In scope:
  - `input::KeyEvent` and `input::KeyEventKind`, and `input::encode_key_event`.
  - `Terminal::encode_key_event`.
  - Two new `ModeSnapshot` fields (`keyboard_flags`, `modify_other_keys`) and the
    `#[non_exhaustive]` mark on `ModeSnapshot`.
  - The kitty `CSI u` encoder honouring all five flags.
  - The `modifyOtherKeys` levels 1 and 2 encoder.
  - Guide chapter 6 rewritten: it currently documents the gap, and the gap is the thing being
    closed.
  - The semver promise's clause 6, amended to cover the `input` encoders.
- [x] Out of scope:
  - **The sixty-odd kitty functional key codes** (`57358`-`57415`, `57428`+, `57441`+: keypad,
    lock, media and the modifier keys themselves). `input::NamedKey` cannot name them and no
    embedder in this repository can produce them. `NamedKey` is `#[non_exhaustive]`, so adding them
    is a patch bump the day one can.
  - **Super, hyper, meta, caps lock and num lock modifiers.** `KeyMods` has three booleans; adding
    five is a breaking change to a type every embedder constructs, for modifiers no embedder here
    supplies. The encoder can emit modifier values 1-8 and no others, and the guide says so.
  - **The pre-existing legacy defect** `US-0099` measured: `encode_key` ignores `alt` for `Insert`,
    `Tab` and F1-F24. The legacy path keeps that behaviour byte for byte; the kitty path does not
    have it, because the modifier goes in a field rather than a table lookup. The two paths
    therefore disagree for those keys, and the guide says so rather than quietly fixing one.
  - `crates/terminal-view` adopting the richer entry point. `Terminal::encode_key` keeps working;
    the view is untouched by this packet.

## Acceptance

- [x] **The claim is true.** One integration test drives a real `Terminal`: feed `CSI > 1 u`,
      assert `CSI ? u` answers `CSI ? 1 u`, **and** assert `Terminal::encode_key_event` for
      `Escape` returns `CSI 27 u`. This is the test the evaluation's gap 2 would have failed. It
      fails on `main` and passes here; both outputs attached.
- [x] **Every row of the specification's own worked examples passes, byte for byte.** The table in
      [`low-level-design/kitty-keyboard.md`](low-level-design/kitty-keyboard.md) section
      "Verification" is the mandatory minimum, and each row's test cites the specification section
      it came from. A row that cannot be made to pass is a finding recorded in Evidence with the
      reason, never a quietly dropped row.
- [x] **Nothing changes when nothing is negotiated.** With `keyboard_flags` empty and
      `modify_other_keys == 0`, `encode_key_event` returns exactly what `encode_key` returns on
      `main`, across `US-0099`'s full cross-product: 75 `KeySpec` values x 8 `KeyMods` x 1 280 mode
      snapshots. **768 000 comparisons, zero mismatches** is the bar, and the count is reported.
      A sample is not acceptance.
- [x] **The whole flag space is safe.** All 32 flag values x 75 keys x 8 modifier sets x 3 event
      kinds: no panic, and every non-`None` result parses back as a well-formed `CSI ... u`,
      `CSI ... ~`, `CSI ... <letter>` or legacy byte string, by a round-trip parser living in the
      test module and not in the crate.
- [x] **`Enter`, `Tab` and `Backspace` still work in a shell after a crashed program leaves the
      flags set.** With `DISAMBIGUATE_ESC_CODES` alone they return `0x0d`, `0x09` and `0x7f`
      unchanged -- the specification's explicit exception, and the one that decides whether a user
      can type `reset`. Tested directly, not inferred.
- [x] **A release event with flags empty returns `None`**, and the embedder-visible contract for
      `None` is documented.
- [x] **Hostile input does not panic.** `US-0099`'s hostile set is re-run through the new entry
      point: a 4-byte scalar, `U+10FFFF`, `U+FEFF`, a combining pair, `NUL`, the empty string, a
      100 000-character payload, and a text field containing a C0 byte (which must drop the text
      field rather than emit it).
- [x] **The corpus does not move.** The 46-recording parity corpus replays byte-identically. The
      encoder is not on the print path, so any difference is a bug in this packet.
- [x] **Guide chapter 6 no longer documents the gap**, and `grep -n "does not yet honour"
      crates/vt/docs/guide/06-input.md` returns nothing.
- [x] **Guide chapter 11's "Known gaps" table loses nothing about kitty keyboard**, because it
      never listed this -- chapter 6 did. Checked, and the check recorded, so the two chapters do
      not drift apart.
- [x] **The surface diff is exactly the planned additions**: `KeyEvent`, `KeyEventKind`,
      `encode_key_event`, `Terminal::encode_key_event`, two `ModeSnapshot` fields. Nothing removed.
- [ ] **The budget holds**: `crates/vt` +430 production, tests +400, measured with
      `git diff --stat` and attached.
- [ ] **A manual Windows walk**, because the claim is about a program inside a terminal and not
      about a function. A local shell and an SSH session each running a short script that pushes
      `CSI > 1 u` and echoes what it receives; transcript attached. `kitty +kitten show_key` is not
      available on Windows, which is why the instrument is a script.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/kitty-keyboard.md`](low-level-design/kitty-keyboard.md) -- this packet's owning
  design: the flag table, the decision ladder, the byte tables, the ceilings and the test plan.
- [`IN-0038/low-level-design/encoding-and-search.md`](../IN-0038-embeddable-vt-core/low-level-design/encoding-and-search.md)
  -- contains the sentence this packet reverses: "`KeyboardFlags` ... is *not* consumed by
  `encode_key` today. It stays where it is; wiring the two together is a future packet." **That
  sentence becomes stale on merge and must be updated to point here**, not left as a promise about
  a future that arrived.
- [`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
  -- the semver promise whose clause 6 this packet amends, and the `#[non_exhaustive]` doctrine.
- [`IN-0038/evidence/US-0099-verify.md`](../IN-0038-embeddable-vt-core/evidence/US-0099-verify.md)
  -- the measurement that proves the gap ("Gap 3", section 3) and the generator this packet reuses
  for its equivalence run.
- `crates/vt/docs/guide/06-input.md` -- documents the gap in prose and must be rewritten.
- `crates/vt/docs/guide/12-versioning.md` -- the promise, the `#[non_exhaustive]` count, and the
  clause-6 list.
- `crates/vt/src/terminal/mode.rs` -- `KeyboardFlags` and `FlagStack`, read and not changed.

### Documentation Action

**Update required.**

| Doc | Change |
| --- | --- |
| `crates/vt/docs/guide/06-input.md` | the "Kitty keyboard flags and modifyOtherKeys" section is rewritten: what the encoder now does, the three-rung ladder, the modifier ceiling (values 1-8 only), the unrepresentable functional keys, and the legacy-versus-kitty disagreement on `Alt+Insert`/`Tab`/F-keys |
| `crates/vt/docs/guide/12-versioning.md` | clause 6 gains the `input` encoders; the `#[non_exhaustive]` count and lists gain `ModeSnapshot`, `KeyEvent` and `KeyEventKind` |
| `crates/vt/CHANGELOG.md` | a `### Changed` entry (minor: a new field on an exhaustive struct, and changed output bytes) and an `### Added` entry for the three new items |
| `crates/vt/README.md` | it advertises `input` as "will turn a key press or a mouse click into the bytes the program expects" -- true today only for the legacy protocol. One sentence, now accurate |
| [`IN-0038/low-level-design/encoding-and-search.md`](../IN-0038-embeddable-vt-core/low-level-design/encoding-and-search.md) | the "future packet" sentence gains "-- `IN-0039`/`US-0105`" rather than being deleted; the accepted design's history stays readable |

Reason: the packet's entire purpose is to make a documented statement false and replace it. Leaving
chapter 6 saying "`encode_key` does not yet honour either" after it does is the exact failure this
intake exists to fix, one level up.

### Reconciliation

Before completion: list the docs changed, and confirm by grep that no file under `crates/vt/docs/`
or `crates/vt/src/` still says the encoder ignores the flags.

## Context

- `KeyboardFlags`, `FlagStack` and the four CSI forms are complete, correct and tested. This packet
  **adds no terminal state**; it reads state that has been live since `IN-0029`.
- `ModeSnapshot` is not constructed by a struct literal outside `crates/vt` -- checked: the four
  `modes()` implementations in `crates/terminal`, `crates/terminal-view` and `test_support.rs` all
  forward `Terminal::mode_snapshot()`. So two new fields and the `#[non_exhaustive]` mark break
  nothing.
- The `Character("")` asymmetry (`Some(vec![])` without ctrl, `None` with it) is pre-existing and
  stays on the legacy rung unchanged. The kitty rung returns `None` for it under every flag.
- `US-0099`'s `modify_other_keys_never_reaches_the_bytes` test is **inverted** by this packet into
  the assertion that the level does reach the bytes, keeping its four chords as the table.
- Specification source: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>, fetched 2026-09-15.
  The design quotes it; the tests cite sections of it.

## Plan

- [x] Add `keyboard_flags` and `modify_other_keys` to `ModeSnapshot`, plus `#[non_exhaustive]`;
      populate them in `Terminal::mode_snapshot`.
- [x] Add `KeyEventKind`, `KeyEvent` and `KeyEvent::new`.
- [x] Write `crates/vt/src/input/kitty.rs`: the `CSI u` writer, the functional-key table, the
      modifier and event-type fields, the alternate-key and text sub-fields.
- [x] Write the `modifyOtherKeys` arm.
- [x] Write `encode_key_event` as the three-rung ladder; make `encode_key` delegate.
- [x] Add `Terminal::encode_key_event`.
- [x] Port `US-0099`'s generator for the equivalence run; run it and report the comparison count.
- [x] Write the specification-example table tests, each citing its section.
- [x] Rewrite guide chapter 6; amend chapter 12; CHANGELOG; README sentence; the IN-0038 pointer.
- [x] Regenerate both surface files.

## Decisions

No new decision record. The three-rung ladder, the modifier ceiling and the excluded functional
keys are implementation-level and belong in
[`low-level-design/kitty-keyboard.md`](low-level-design/kitty-keyboard.md). The one thing a future
packet must inherit -- that clause 6 of the semver promise now covers the `input` encoders -- is
recorded in [`low-level-design/api-surface.md`](low-level-design/api-surface.md) and written into
the CHANGELOG and guide chapter 12, which are the documents a consumer actually reads.

## Verification Plan

- `cargo test -p oneterm-vt` -- the specification-example table, the ladder tests, the
  `Enter`/`Tab`/`Backspace` exception, the release-event rule, the hostile set.
- The equivalence run: 768 000 comparisons against `main`'s `encode_key`, count reported.
- The flag cross-product: 32 x 75 x 8 x 3, no panic, all output well formed.
- The 46-recording corpus replay, byte-identical.
- `cargo test --workspace` -- `crates/terminal-view` must still compile against the unchanged
  `Terminal::encode_key`.
- `python scripts/vt-public-api.py --check` and `--check-nameable` (added by `BUG-0059`) and
  `--diff-platforms`.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` -- guide chapter 6
  is `include_str!`d, so its Rust blocks are doctests and a stale example fails the build.
- CI's "the embedder guide must stand alone" grep over the rewritten chapter 6.
- `pwsh scripts/ci-local.ps1`.
- The manual Windows walk, local shell and SSH.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Verification notes closed

An independent verification of `341b6b97` returned **FAIL** with twelve findings, four of them
blocking, and a 211-row table test derived from the specification before the implementation was
read: **43 row mismatches and 2 failed assertions**. The full report is
[`evidence/US-0105-verify.md`](evidence/US-0105-verify.md) and its test file is adopted at
`crates/vt/tests/verify_us0105_independent.rs`. Nothing was deleted from it.

Every finding, and what closed it:

| # | Finding | Closed by |
| --- | --- | --- |
| 1 | **HIGH.** A repeat or release of a *text* key became `CSI u` under `REPORT_EVENT_TYPES` alone, so holding a letter stopped typing it. The event-type rung fired before `generates_text` was consulted | `kitty.rs`: rung 1 computes `reportable`, which is false for a text-producing key (and for `Enter`/`Tab`/`Backspace`) without `REPORT_ALL_KEYS_AS_ESC`; rung 2 reads the same value. A repeat falls to the legacy rung -- "key repeat events are treated as key press events" -- and a release is silent. 6/6 rows |
| 2 | **HIGH.** The associated-text field was fabricated from the key payload without looking at the modifiers, so under `ALL_ESC \| TEXT` a `Ctrl+A` told the program an `a` was inserted | `text_field` uses the payload fallback only when neither ctrl nor alt is held. An embedder-supplied `KeyEvent::text` still wins. 2/2 rows |
| 3 | **HIGH.** `CSI ? u` answered the stack **top** while the encoder read the **live** flags, so the specification's own detection recipe (`CSI = Ps u`, then query) reported that the terminal implements nothing | `dispatch.rs` answers `live()`. The in-crate test that asserted the old behaviour is inverted and renamed `kitty_query_reads_the_live_flags`. CHANGELOG entry under clause 6 |
| 4 | **MEDIUM-HIGH.** `F3` emitted `CSI R` / `CSI 1 ; <mod> R`, which the specification removed because it collides with the Cursor Position Report; `CSI 1 ; 6 R` *is* a CPR for row 1 column 6 | `F3` is `Form::Tilde(13)`. `F15`, which inherited the final byte, moved to its private-use code with the rest of `F13`-`F24`. A new test walks every flag set and both keys and asserts the final byte is never `R` |
| 5 | **MEDIUM.** `modifyOtherKeys` level 1 was the complement of xterm's exception list | The rule is now xterm's own, as one predicate: level 1 leaves any chord whose legacy encoding is already a control byte. And neither level fires on shift alone -- the first draft turned every capital letter into `CSI 27 ; 2 ; 97 ~` at level 2, which the verification did not test and which is the same class of defect as finding 2 |
| 6 | **MEDIUM.** `F13`-`F24` asserted a shift the user never pressed | The kitty rung uses the private-use codes `57376`-`57387`; the legacy rung keeps xterm's shifted forms, which `US-0099` freezes. 12/12 rows |
| 7 | **MEDIUM.** The un-shifted key code was a lower-case, so `ctrl+shift+1` reported `33`; and the documented remedy ("supply `base_layout`") could not work, because that field is the third sub-field and never the primary code | `unshifted` also walks the PC-101 shift table, so `!`→`49`, `$`→`52`, `+`→`61`. 5/5 rows. Guide chapter 6 no longer offers the remedy that does not exist and states the ceiling instead |
| 8 | **LOW-MEDIUM.** The legacy rung drops `alt` on more keys than the guide listed, and sends xterm's `Enter` forms rather than kitty's | Documented exactly: guide chapter 6 has a five-row table of every place the two rungs disagree. The 13 C0-control rows are kept in the adopted test as **frozen deviations** -- the engine's answer is asserted, the specification's is printed on every run -- because `US-0099`'s equivalence bar freezes rung 4 |
| 9 | **LOW.** `ctrl+~` was missing from the legacy ctrl table | `key.rs` maps `~` to `0x1e` beside `^`. This is the **only** byte on the legacy rung that moved, and the equivalence harness names the row explicitly rather than widening a tolerance |
| 10 | **LOW.** `ctrl+shift+<text key>` in legacy mode is `CSI u` in the specification and is not here | Frozen; the three rows are kept as frozen deviations and the disagreement table in guide chapter 6 names it, with "push `DISAMBIGUATE_ESC_CODES`" as the answer |
| 11 | **LOW.** `#[non_exhaustive]` on `ModeSnapshot` is breaking (`E0639` on functional update syntax) and was filed under `### Added` as a benefit | Moved to `### Changed` with the **Breaking** marker, both consequences spelled out, and guide chapter 12 now says functional update syntax is refused too |
| 12 | **INFO.** Ceilings confirmed as disclosed | Unchanged, and chapter 6 now also names the caps-lock/num-lock bits and `DECKPAM` as unreachable |

One expectation in the adopted test was corrected rather than accepted, with the reason in the test
file beside it: `modifyOtherKeys 1: ctrl+2` expected `CSI 27;5;50~`, but `Control-Space to make a
NUL` is named in xterm(1)'s exception list and `ctrl+2` is its alias, so the row now expects `0x00`
and the level-2 row that the verification did not have was added beside it. The report's own
finding 5 table agrees with the correction; its test row did not. The report also says it could not
reach xterm's source, which is why this is stated as a citation rather than a measurement.

**Result after rework: 214 rows, 0 mismatches, 16 recorded frozen deviations**, plus the four
standalone assertions and the 57 600-case flag-space walk with its CPR-collision guard.

### Re-verification: PASS-WITH-NOTES, and the five notes

A second independent pass at `63c99535` confirmed all four blockers closed and raised five more,
from a table re-derived from the specification without reusing the first one. Its report replaces
the first in [`evidence/US-0105-verify.md`](evidence/US-0105-verify.md) and its file is adopted at
`crates/vt/tests/reverify_us0105.rs`.

| # | Note | Closed by |
| --- | --- | --- |
| R1 | **LOW-MED.** `text_field` guarded the modifiers but not the event kind, so a *release* of a text key carried the text it did not insert: `CSI 97 ; 1 : 3 ; 97 u` where kitty sends `CSI 97 ; 1 : 3 u`. The same defect as finding 2 on the other axis, and it survived because the fix added one guard and not two | `text_field` returns `None` for a release. A repeat still carries text, because a repeat does insert |
| R2 | **MED.** The legacy `F15` CPR collision was reachable **with flags negotiated** -- 240 of 95 976 swept cases -- at every `REPORT_EVENT_TYPES` / `REPORT_ALTERNATE_KEYS` / `REPORT_ASSOCIATED_TEXT` combination, and no packet owned it. Both existing `R` guards excluded exactly the flag sets where it lived | **Fixed, not deferred.** Legacy `F15` is `CSI 28 ~`, the DEC VT220 code, with the same named-divergence treatment as `ctrl+~`: both bytes asserted, cases counted, count pinned. The sweep now walks **all 32 flag sets including the empty one**, and asserts no CSI sequence ending in `R` on either rung. (`SS3 R`, which plain `F3` still sends on the legacy rung, is a different introducer and cannot be read as a CPR.) |
| R3 | **LOW.** The CHANGELOG's clause-6 entry still listed two ceilings this rework removed | Rewritten to the three that remain, plus a pointer to the disagreement table |
| R4 | **LOW.** Reworked after a FAIL without `Reopened (acceptance rework)` being ticked | Ticked, and the harness snippet's `status` is `reopened` |
| R5 | **INFO.** `alt` alone at `modifyOtherKeys` level 1, where xterm(1) is quoted both ways and the source was unreachable | Recorded in Gaps below as measured behaviour with the ambiguity named. No claim made |

One expectation in the adopted re-verification was corrected, with the citation beside it in the
file: `the_detection_recipe_and_the_stack` expected a pop of the *only* stack entry to restore the
live value `CSI = 4 ; 3 u` had set before the push. The specification's next sentence, which the
row itself quotes, is "If a pop request is received that empties the stack, **all flags are
reset**", so it resets to `0`; the stack never held `3`. The row keeps its original assertion as
well, moved to a two-entry stack where a pop really does uncover the older value.

## Evidence and Gaps

Branch `feat/vt-kitty-keyboard`, rebased onto `main` at `4437b98e` (`BUG-0059`).

**The claim is true.** `crates/vt/tests/verify_us0105.rs::a_negotiated_protocol_is_actually_spoken`
drives a real `Terminal`: it feeds `CSI > 1 u`, asserts `CSI ? u` answers `CSI ? 1 u`, and asserts
`Terminal::encode_key_event(Escape)` returns `CSI 27 u`. The third assertion is the one that fails
on `main`, where the same terminal answers `CSI ? 1 u` and then sends `0x1b`. Popping the flag
takes the claim back with it, which the same test checks.

**The specification table.** `crates/vt/src/input/kitty_tests.rs::specification_worked_examples`
is seventeen rows, each labelled with the specification section it came from, all passing. Beyond
the mandatory list it adds the un-shifted-key-code rule (`ctrl+shift+a` is `97`, never `65`), the
repeat event type, and `PageUp` keeping its `CSI 5 ~` form. Beside it now sit the regression tests
for the rework: the text-key event-type rule, the text-field fallback's modifier check, the `F3`
CPR guard across every flag set, and the private-use `F13`-`F24` codes against their frozen legacy
forms. **The independently derived 214-row table in
`crates/vt/tests/verify_us0105_independent.rs` is the stronger artefact** -- it was written against
the specification before this implementation was read.

**One mandatory row could not be made to pass and is recorded rather than dropped:**
`alt+a -> CSI 0 ; ; 229 u`. Read in full, the specification's own prose for that row says the
**OS consumed the alt modifier to produce the text and the terminal received a pure text event
with no key information at all** -- which is why the key code is `0` and the modifier field empty.
`input::KeySpec` has no way to say "text with no key": `Character("")` is spoken for (the legacy
rung returns an empty write, which this packet did not change), and adding a variant is outside
this packet's scope. `KeySpec` is `#[non_exhaustive]`, so a `Text` variant is a patch release
whenever an embedder can produce one. Until then an IME-composed event reaches the encoder as the
text it produced, and is encoded with that text's own key code.

**Nothing changes when nothing is negotiated.**
`crates/vt/tests/verify_us0099_equiv.rs::encode_key_is_byte_identical_to_main`, the frozen-copy
harness, now runs both entry points against the frozen `0558fa2` oracle:

```text
encode_key cases compared: 3072000 (specs 75 x mods 8 x snapshots 2560),
  61440 deliberately moved (ctrl+~, F15), 0 mismatches
test encode_key_is_byte_identical_to_main ... ok
```

**3 072 000 comparisons, zero mismatches** -- 1 536 000 through `encode_key` and 1 536 000 through
`encode_key_event`, which is the doubled count. Every snapshot asserts `keyboard_flags.is_empty()`
and `modify_other_keys == 0` before it is used, so the run is the equivalence claim and not a
sample.

The 61 440 are the two rows this packet deliberately moved on the legacy rung: `ctrl+~` (finding 9,
four ctrl-bearing modifier sets) and `F15` (note R2, all eight, because the functional-key table
ignores modifiers for it), each across 2 560 snapshots and both entry points. Neither is excused by
a tolerance -- the harness asserts the corrected byte on this side, the old byte from the frozen
oracle, and that the moved set is exactly `(4 + 8) x snapshots x 2` in size, so a third row moving
fails the test.

**The whole flag space is safe.** `verify_us0105.rs::the_whole_flag_space_is_safe`: 32 flag values
x 2 `app_cursor` values x 75 keys x 8 modifier sets x 3 event kinds = **115 200 cases, 92 688 of
them producing bytes**, no panic, and every non-`None` result parsed back by a round-trip parser
living in the test file. (The count of emitted results fell by 1 120 against the first
implementation: those were the spurious escape sequences finding 1 produced for repeats and
releases of text keys.) The adopted independent test walks 57 600 more with a CPR-collision guard. The parser accepts a legacy byte string, `SS3`, and a CSI whose final byte
is `u`, `~` or a letter and whose parameters are digits, `;` and `:` with no trailing empty
sub-field.

**The flags matrix covered**, by name:

| Flag | Where it is proved |
| --- | --- |
| `DISAMBIGUATE_ESC_CODES` | `specification_worked_examples` rows 1, 5-9; `disambiguate_covers_only_the_keys_that_produce_no_text`; `enter_tab_and_backspace_survive_a_crashed_program` |
| `REPORT_EVENT_TYPES` | the repeat and release rows; `the_three_legacy_keys_have_no_release_without_report_all`; `a_release_is_silent_until_it_is_asked_for` |
| `REPORT_ALTERNATE_KEYS` | the three alternate-key rows, including the empty sub-field and the shift-only rule; `alternate_keys_and_text_are_inert_on_their_own` |
| `REPORT_ALL_KEYS_AS_ESC` | rows 2-4 and 10-13; `an_empty_character_has_no_kitty_encoding` |
| `REPORT_ASSOCIATED_TEXT` | the `CSI 97 ; 2 ; 65 u` row; `the_text_field_is_dropped_rather_than_sent_unsafely`; the inert-without-`ALL_ESC` case |
| all 32 combinations | `the_whole_flag_space_is_safe`, and `kitty_flags_swap_with_alt_screen_and_reach_the_bytes` over a real `Terminal` |
| `modifyOtherKeys` 0 / 1 / 2 | `modify_other_keys_levels`; `modify_other_keys_reaches_the_bytes` (the inverted `US-0099` test, same four chords); `kitty_wins_over_modify_other_keys` |

**`Enter`, `Tab` and `Backspace` after a crash.** `enter_tab_and_backspace_survive_a_crashed_program`
asserts `0x0d`, `0x09` and `0x7f` under three flag sets that include `DISAMBIGUATE_ESC_CODES`;
`the_three_legacy_keys_have_no_release_without_report_all` asserts the release half.

**Hostile input.** `verify_us0105.rs::hostile_input_does_not_panic` runs the `US-0099` payloads --
a 4-byte scalar, `U+10FFFF`, `U+FEFF`, a combining pair, `NUL`, the empty string and a
100 000-character payload -- through all 32 flag values and three `modifyOtherKeys` levels, each
with a text field carrying a C0 byte. The text field is dropped in every case, and any escape
sequence the encoder *builds* stays under 1 KiB. A `Character` payload the embedder hands over on
the legacy rung is still written through unchanged, which is pre-existing and unchanged.

**The corpus does not move.** `cargo test -p oneterm-tools --test corpus_check`: 2 passed.

**The gate.** `pwsh scripts/ci-local.ps1 -Full` ends `ci-local: all checks passed.`, `cargo deny`
included. It also passed on the delivery the verification failed, which is the reason that line is
not evidence on its own: `--check-nameable` (new, from `BUG-0059`) and `--diff-platforms` are both
green, and neither can see a wrong byte.

**The surface diff is exactly the planned additions**, nothing removed: two `ModeSnapshot` fields,
`Terminal::encode_key_event`, `input::KeyEvent` with its seven items, `input::KeyEventKind` with
three variants, and `input::encode_key_event`. Sixteen lines added to each snapshot; the Unix file
was derived from the Windows one by replaying the same insertions at the mapped line numbers, and
`--diff-platforms` reports **"the delta is 6 lines, all inside `oneterm_vt::pty`"**.

**The documentation greps.** `grep -rn "does not yet honour" crates/vt/` returns nothing. Guide
chapter 11's "Known gaps" table never listed kitty keyboard -- checked, four rows, all about
`DECRQCRA`, `DECRQSS`, `XTGETTCAP` and `? 2027` -- so it lost nothing; chapter 11's "Supported"
section gained one sentence saying the two keyboard protocols are encoded and not only tracked, so
the two chapters cannot drift apart silently.

**The budget did not hold, and the overrun is reported rather than absorbed.** `git diff --numstat`
against `main`:

| | Insertions | Deletions | Net | Budget |
| --- | --- | --- | --- | --- |
| `crates/vt` production | 597 | 13 | **+584** | +430 |
| `crates/vt` tests | 2 182 | 49 | **+2 133** | +400 |

Production is 154 lines over. `crates/vt/src/input/kitty.rs` is 410 lines of which about 130 are
comment lines and 25 blank, so the executable half is around 255; the thirty-eight-arm `named_form`
table is 50 of those and is irreducible, and most of the comments are the specification sentences
each rung answers -- which is what the verification found missing the first time.

Tests are 1 733 over, and 1 132 of that is one file: the independently derived 214-row table
adopted whole from the verification. Keeping it is the point. The rest is the three exhaustive runs
the acceptance criteria ask for, their round-trip parsers, and the regression rows for the four
blocking defects.

Both numbers are reported rather than met. Cutting documentation or deleting the independent table
to reach an estimate written before the work would be the wrong trade, and the estimate is the
thing that was wrong.

Gaps to state rather than discover:

- **The excluded functional keys mean `REPORT_ALL_KEYS_AS_ESC` is incomplete.** A program that sets
  it and expects a `Super` press reported gets nothing. This is a documented ceiling, not a bug,
  and the guide names it -- but a reviewer should know the flag is honoured for the keys the crate
  can name and not for the ones it cannot.
- **The modifier ceiling (values 1-8).** Same shape: correct for what `KeyMods` can express.
- **No comparison against a reference implementation's live output.** The tests compare against the
  specification's documented examples, not against bytes captured from kitty itself. Capturing
  those needs kitty running, which is not available on the maintainer's platform. This is the
  weakest link in the proof and it should be said so.
- **The legacy-versus-kitty disagreement on `Alt+Insert`, `Alt+Tab` and `Alt+F5`** is deliberate
  and recorded, and it will look like a bug to the next reader who finds it without this note.
  Guide chapter 6 names it with `Alt+F5` as the worked case.

- **`alt` alone at `modifyOtherKeys` level 1 is unresolved, and the measured behaviour is stated
  rather than claimed.** This engine escapes `alt+a` to `CSI 27 ; 3 ; 97 ~`, because `ESC a` is not
  a control byte and the level-1 rule is "leave the chords that already produce one". The
  pre-rework code sent `ESC a`. xterm(1) is quoted both ways by the secondary sources reachable
  from here -- "The Alt- and Meta- modifiers do not cause xterm to send escape sequences" for
  value 1, against a paraphrase saying they do -- and `invisible-island.net` was unreachable in
  both verification passes and in this one. `reverify_us0105.rs` prints the measured bytes so a
  maintainer with the source can settle it; **no claim is made in either direction**, and if xterm
  turns out to exclude alt, the fix is one condition in `modify_other_keys`.

- **The un-shifted key code is derived, not looked up.** Letters are lower-cased and ASCII
  punctuation goes through the PC-101 shift table, which covers the layout the base-layout
  sub-field is itself defined against. A layout that pairs `!` with something other than `1`
  reports the key it produced. There is **no field on `KeyEvent` that overrides the primary code**
  -- `base_layout` is the third colon sub-field -- so this is a ceiling and not a setting, and the
  guide no longer suggests otherwise.

- **The manual Windows walk was not run, so `Platform proof` and `E2E proof` stay unticked.**
  A GUI walk means launching `oneterm.exe`, and the maintainer runs their coding agent inside this
  application; a second instance is not something to start unasked. The closest evidence that does
  exist is `a_negotiated_protocol_is_actually_spoken`, which drives a real `Terminal` end to end
  but not a real pseudo-console and not the view. **This is the weakest link in the proof after the
  reference-implementation gap, and the packet should not be accepted without the walk.**

- **`crates/terminal-view` does not yet deliver release or repeat events**, which was out of scope
  and is confirmed rather than assumed: GPUI *can* supply both -- `gpui::KeyDownEvent` carries
  `is_held` (the repeat) and `gpui::KeyUpEvent` exists -- but `TerminalView` registers only
  `on_key_down`, and `crates/terminal-view/src/input/keys.rs` maps a `Keystroke` to
  `(KeySpec, KeyMods)` with no event kind. So today every event the application delivers is a
  press, `REPORT_EVENT_TYPES` never produces a repeat or release byte in OneTerm itself, and
  `REPORT_ALTERNATE_KEYS` and `REPORT_ASSOCIATED_TEXT` get no platform values either. The engine
  side is complete and tested; adopting it in the view is a separate packet with a separate claim.

## Handoff

Independent of `US-0106` and `US-0107`; all three sit behind `BUG-0059` only. Next owner: whoever
takes the remaining two. If `US-0106` lands first, both regenerate the surface files and the second
one rebases its snapshot -- a mechanical conflict, not a semantic one.

`BUG-0059` touches `crates/vt/src/lib.rs` and both surface snapshots as well, so the snapshot
conflict is the same shape there and is resolved by regenerating on Windows and re-running the
mirror step in the Evidence section above.

Two things a next owner should pick up, neither of which belongs in this packet:

1. the manual Windows walk, which is the one unticked acceptance item that is not a measurement;
2. `crates/terminal-view` delivering release and repeat events, so OneTerm itself can use
   `REPORT_EVENT_TYPES` -- the engine is ready and the platform has the data.

### Harness row

This worktree has no harness binary, so the `story` row is written here rather than inserted.
The schema is the real one in `harness.db` (`intake` for `IN-0039` is `id = 44`).

```python
#!/usr/bin/env python3
"""Insert the US-0105 story row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    id="US-0105",
    title="The key encoder honours the kitty keyboard flags it already answers",
    created_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    risk_lane="high_risk",
    contract_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/"
        "low-level-design/kitty-keyboard.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/"
        "US-0105-kitty-keyboard-encoder.md"
    ),
    # Reworked twice after independent verifications: FAIL, then
    # PASS-WITH-NOTES. `AGENTS.md` calls that acceptance rework of the owning US.
    status="reopened",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=0,
    evidence=(
        "3072000 frozen-copy comparisons, 0 mismatches; 115200-case flag "
        "cross-product, no panic and nothing malformed; 17 specification rows; "
        "corpus_check 2 passed; surface +16 lines per platform, delta between "
        "the two files still 6 pty lines."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "Platform and E2E proof are 0 on purpose: the manual Windows walk was "
        "not run, because it means launching a second oneterm.exe on a machine "
        "whose owner runs their agent inside the first. Budget overrun reported "
        "in the packet: production +514 against +430, tests +790 against +400."
    ),
    intake_id=44,
)

with sqlite3.connect(DB) as db:
    columns = ", ".join(ROW)
    placeholders = ", ".join("?" for _ in ROW)
    db.execute(f"INSERT INTO story ({columns}) VALUES ({placeholders})", tuple(ROW.values()))
print("inserted US-0105")
```
