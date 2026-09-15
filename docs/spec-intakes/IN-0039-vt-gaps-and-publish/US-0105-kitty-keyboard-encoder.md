# Work: the key encoder honours the kitty keyboard flags it already answers

ID: US-0105
Intake: [`IN-0039`](IN-0039.md)
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
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

- [ ] **The claim is true.** One integration test drives a real `Terminal`: feed `CSI > 1 u`,
      assert `CSI ? u` answers `CSI ? 1 u`, **and** assert `Terminal::encode_key_event` for
      `Escape` returns `CSI 27 u`. This is the test the evaluation's gap 2 would have failed. It
      fails on `main` and passes here; both outputs attached.
- [ ] **Every row of the specification's own worked examples passes, byte for byte.** The table in
      [`low-level-design/kitty-keyboard.md`](low-level-design/kitty-keyboard.md) section
      "Verification" is the mandatory minimum, and each row's test cites the specification section
      it came from. A row that cannot be made to pass is a finding recorded in Evidence with the
      reason, never a quietly dropped row.
- [ ] **Nothing changes when nothing is negotiated.** With `keyboard_flags` empty and
      `modify_other_keys == 0`, `encode_key_event` returns exactly what `encode_key` returns on
      `main`, across `US-0099`'s full cross-product: 75 `KeySpec` values x 8 `KeyMods` x 1 280 mode
      snapshots. **768 000 comparisons, zero mismatches** is the bar, and the count is reported.
      A sample is not acceptance.
- [ ] **The whole flag space is safe.** All 32 flag values x 75 keys x 8 modifier sets x 3 event
      kinds: no panic, and every non-`None` result parses back as a well-formed `CSI ... u`,
      `CSI ... ~`, `CSI ... <letter>` or legacy byte string, by a round-trip parser living in the
      test module and not in the crate.
- [ ] **`Enter`, `Tab` and `Backspace` still work in a shell after a crashed program leaves the
      flags set.** With `DISAMBIGUATE_ESC_CODES` alone they return `0x0d`, `0x09` and `0x7f`
      unchanged -- the specification's explicit exception, and the one that decides whether a user
      can type `reset`. Tested directly, not inferred.
- [ ] **A release event with flags empty returns `None`**, and the embedder-visible contract for
      `None` is documented.
- [ ] **Hostile input does not panic.** `US-0099`'s hostile set is re-run through the new entry
      point: a 4-byte scalar, `U+10FFFF`, `U+FEFF`, a combining pair, `NUL`, the empty string, a
      100 000-character payload, and a text field containing a C0 byte (which must drop the text
      field rather than emit it).
- [ ] **The corpus does not move.** The 46-recording parity corpus replays byte-identically. The
      encoder is not on the print path, so any difference is a bug in this packet.
- [ ] **Guide chapter 6 no longer documents the gap**, and `grep -n "does not yet honour"
      crates/vt/docs/guide/06-input.md` returns nothing.
- [ ] **Guide chapter 11's "Known gaps" table loses nothing about kitty keyboard**, because it
      never listed this -- chapter 6 did. Checked, and the check recorded, so the two chapters do
      not drift apart.
- [ ] **The surface diff is exactly the planned additions**: `KeyEvent`, `KeyEventKind`,
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

- [ ] Add `keyboard_flags` and `modify_other_keys` to `ModeSnapshot`, plus `#[non_exhaustive]`;
      populate them in `Terminal::mode_snapshot`.
- [ ] Add `KeyEventKind`, `KeyEvent` and `KeyEvent::new`.
- [ ] Write `crates/vt/src/input/kitty.rs`: the `CSI u` writer, the functional-key table, the
      modifier and event-type fields, the alternate-key and text sub-fields.
- [ ] Write the `modifyOtherKeys` arm.
- [ ] Write `encode_key_event` as the three-rung ladder; make `encode_key` delegate.
- [ ] Add `Terminal::encode_key_event`.
- [ ] Port `US-0099`'s generator for the equivalence run; run it and report the comparison count.
- [ ] Write the specification-example table tests, each citing its section.
- [ ] Rewrite guide chapter 6; amend chapter 12; CHANGELOG; README sentence; the IN-0038 pointer.
- [ ] Regenerate both surface files.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record: the equivalence comparison count and mismatch count; the
specification-example table results row by row; the failing-on-`main` and passing-here integration
test; `git diff --stat` against budget; the surface diff; the corpus replay result; the Windows
walk transcript.

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

## Handoff

Independent of `US-0106` and `US-0107`; all three sit behind `BUG-0059` only. Next owner: whoever
takes the remaining two. If `US-0106` lands first, both regenerate the surface files and the second
one rebases its snapshot -- a mechanical conflict, not a semantic one.
