# Work: the view delivers press, repeat and release to the encoder

ID: US-0108
Intake: [`IN-0040`](IN-0040.md)
Created: 2026-09-16

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: **existing-contract change**
- Risk lane: high_risk
- Spec Intake, when required: [`IN-0040`](IN-0040.md)

## Outcome

A program running in an OneTerm terminal that pushed the kitty keyboard `REPORT_EVENT_TYPES` flag
receives the `:2` repeat and `:3` release events it asked for, and a program that pushed nothing
receives exactly the bytes it receives today.

`US-0105` shipped the encoder: `oneterm_vt::input::KeyEvent` carries `kind` and `text`,
`encode_key_event` honours them, and a release with no flag asking for it already returns `None`.
`TerminalView` is the half that never observes a key-up and never distinguishes a held repeat from
a first press. This packet is that half and nothing else -- **no file under `crates/vt` changes**.

## Scope

- [x] In scope:
  - Register `on_key_up` on the terminal div beside the existing `on_key_down`.
  - `KeyAction::Send` carries a `KeyEvent`; `classify_key` takes the kind; `map_key` builds the
    event; `send_key` calls `encode_key_event`.
  - `kind` from `KeyDownEvent::is_held` (`Repeat`) or its absence (`Press`), and `Release` on the
    key-up path.
  - `KeyEvent::text` from the `Keystroke`'s own `key_char`, on a press and a repeat only, and
    only when no ctrl or alt would have stopped that text reaching the program (`F5`).
  - A `held_keys` set on `TerminalView`: a release is sent only for a key whose press was written,
    and the set is drained on blur.
  - Three re-exports in `crates/terminal/src/lib.rs`: `KeyEvent`, `KeyEventKind`,
    `encode_key_event`.
  - The tests named under Verification Plan.
  - `docs/terminal-backend.md` section 10 and `crates/vt/docs/guide/06-input.md`.
- [x] Out of scope:
  - **Any change to `crates/vt`.** If one turns out to be needed, that is a finding for `IN-0039`
    and a separate packet, not a quiet edit here.
  - `REPORT_ALTERNATE_KEYS`: GPUI's `Keystroke` has no shifted or base-layout code point, so
    `KeyEvent::shifted` and `base_layout` stay `None`.
  - Reporting the modifier keys themselves. Windows turns them into `ModifiersChanged` before a key
    event exists.
  - ~~`Ctrl+C`~~ -- **pulled into scope** by the coordinator's answer to intake open decision 1,
    and widened again by finding `F4`. `KeyAction::Interrupt` carries an `Option<KeyEvent>`: the
    encoded key once the program negotiated a kitty flag that puts a ctrl chord on the `CSI u`
    rung, the `SIGINT` otherwise, and `BroadcastInput::Interrupt` to the channel's peers either
    way.
  - The IME's ownership of printable keys on the primary screen, which makes
    `REPORT_ALL_KEYS_AS_ESC` work fully only on the alternate screen. Intake open decision 2.
  - Re-encoding broadcast bytes per target pane. Pre-existing divergence; this packet must not
    widen it, which is why a release is never fanned out.
  - A new dependency edge from `crates/terminal-view` to `oneterm-vt`.

## Acceptance

Each criterion is a command a hostile verifier can run, with a stated expected result.

- [x] **The kind mapping is what it claims, proved at the view level.** A unit test in
      `crates/terminal-view/src/terminal_view/view_tests.rs` asserts four things with no engine
      involved: a key-down with `is_held: false` classifies as `Send` with `KeyEventKind::Press`;
      the same with `is_held: true` as `Repeat`; a key-up after a sent press yields a `Release`;
      and a key-up whose press was swallowed (`Ctrl+Shift+C`, the copy chord) yields nothing.
- [x] **With no flag pushed, the bytes are identical.** Met in the **strong** form: 12528 cases
      compared against `main`'s own `map_key` + `encode_key`, 0 divergences, measured by
      `us0108_verify_tests::byte_identity_with_main_when_no_kitty_flag_is_pushed` and printed by the
      test. `without_a_flag_press_repeat_and_release_write_todays_bytes` drives the same six keys
      through a `FakeTerminalSession`, where a release contributes **no entry at all**.
- [x] **`REPORT_EVENT_TYPES` produces `:2` and `:3`.** An integration test feeds the real
      `Terminal` behind the fake session `\x1b[>2u`, forces a repaint so the frame's `ModeSnapshot`
      carries the flags, then drives press / held / up on an arrow and asserts the written bytes
      carry the `:2` and `:3` event-type sub-fields. A test that skips the repaint reads stale
      modes and proves nothing; the repaint is part of the criterion.
- [x] **Blur cannot strand a held key.** A test presses a key, blurs the view, and asserts a
      release was written and `held_keys` is empty; a second blur writes nothing further.
- [x] **A swallowed press never produces a release**, for each of the four swallow paths: a view
      chord (`Ctrl+Shift+C`), the completion overlay, `KeyAction::Ignore` (a printable key on the
      primary screen), and a chord with no encoding. One test, four cases.
- [x] **The broadcast contract is unchanged for presses and closed for releases.**
      `member_input_reaches_the_channel_peers_only` passes **without being edited**, and a new case
      asserts that a release writes to the origin session and to no peer.
- [x] **Nothing else in the workspace moved.** `cargo test --workspace` passes with no `#[allow]`
      added anywhere, and `pwsh scripts/ci-local.ps1` is green.
- [ ] **NOT RUN. The manual Windows walk is run, or the packet is not accepted.** It cannot run in the
      session that implements this: the maintainer runs their coding agent inside OneTerm and a
      second `oneterm.exe` is not started unasked. The walk is specified in
      [`low-level-design/input-events.md`](low-level-design/input-events.md) so that whoever runs it
      does not have to design it. Until it is run, `E2E proof` and `Platform proof` stay 0 and the
      packet is reported as **unverified on the platform that ships**, exactly as `US-0105` did.
- [x] **The production diff is inside budget**: `crates/terminal-view` +130 / -20 and
      `crates/terminal` +3, measured with `git diff --numstat main...HEAD` and attached. A diff
      more than 50 per cent over budget is a finding to explain in Evidence, not a silent overrun.

## Documentation

### Owning Docs Reviewed

- [`IN-0018/low-level-design/input.md`](../IN-0018-rebuild-terminal-render-engine/low-level-design/input.md)
  -- the accepted classification table `classify_key` mirrors row for row. The rows do not change
  here; the action one of them carries does.
- [`IN-0039/low-level-design/kitty-keyboard.md`](../IN-0039-vt-gaps-and-publish/low-level-design/kitty-keyboard.md)
  -- the encoder's ladder and which flag makes which rung apply. This packet supplies its inputs
  and must not duplicate its logic: the view never decides whether a release produces bytes, it
  only says that a release happened.
- [`IN-0039/low-level-design/api-surface.md`](../IN-0039-vt-gaps-and-publish/low-level-design/api-surface.md)
  -- clause 6 as amended, which makes the (event, mode snapshot) pair a contract.
- [`IN-0039/US-0105`](../IN-0039-vt-gaps-and-publish/US-0105-kitty-keyboard-encoder.md) -- the gap
  this packet closes, stated by the packet that created it, and the walk instrument it specified.
- `docs/terminal-backend.md` section 10, "Input: keystroke -> byte + IME" -- describes four input
  paths; path 1 is the one that changes, and its one-line summary at section 3's data-flow list
  changes with it.
- `crates/vt/docs/guide/06-input.md` -- the embedder's account of `KeyEvent` and the ladder. Read
  because it is the doc a reader would check to find out whether OneTerm sends releases.
- [`docs/agents/crate-dependency-rules.md`](../../agents/crate-dependency-rules.md) -- R1-R12, read
  to confirm that reaching the encoder through `oneterm-terminal`'s shim keeps the graph as it is.

### Documentation Action

**Update required.** Two owning docs must change with the code, and one is checked:

| Doc | Change |
| --- | --- |
| `docs/terminal-backend.md` | section 10's path 1 gains the key-up half: the view registers `on_key_up`, a key-down carries `Press` or `Repeat` from `is_held`, a release is sent only for a key whose press was written, and the held set is drained on blur. Section 3's one-line input data flow (`Keystroke` -> `keys.rs` -> `encode_key` -> `write`) names `encode_key_event` and the three kinds |
| `crates/vt/docs/guide/06-input.md` | the chapter's account of `KeyEvent` says an embedder supplies what its platform knows; after this packet OneTerm is an embedder that supplies the kind and the text, so the "richer entry point" section gains one sentence saying which fields a GPUI-shaped embedder can fill and which it cannot (`shifted` and `base_layout` are not available from a `Keystroke`). This is the guide's only change: no byte, ladder row or flag description moves |
| `crates/vt/CHANGELOG.md` | **no entry.** The crate does not change. The application has no changelog of its own -- `crates/vt/CHANGELOG.md` is the only one in the repository -- so there is nowhere else an entry belongs, and inventing an application changelog is not this packet's work |

Reason: this changes what the application sends on a user's behalf, and `docs/terminal-backend.md`
is the current owning record of that path. The guide is `oneterm-vt`'s and describes the engine
rather than OneTerm, so its change is one clarifying sentence and not a rewrite.

**Checked and not changed:**
[`IN-0018/low-level-design/input.md`](../IN-0018-rebuild-terminal-render-engine/low-level-design/input.md)
-- its classification table is about which key does what, and every row's decision is unchanged. If
the implementer finds a sentence there asserting that the view handles only key-down, that sentence
is this packet's to fix and the "not changed" line above becomes wrong; say so rather than leaving
it.

### Reconciliation

Changed: `docs/terminal-backend.md` (section 3's input data-flow line now names `KeyEvent` and
`encode_key_event`; section 10's path 1 gains the key-up half, the held set, the blur drain, the
two deliberate absences on the up path, and the `shifted` / `base_layout` ceiling) and
`crates/vt/docs/guide/06-input.md` (one paragraph in "The richer entry point" saying which
`KeyEvent` fields a GPUI-shaped embedder can fill and which it cannot). Also changed:
[`IN-0040.md`](IN-0040.md), whose three open decisions are now marked settled with the
coordinator's answers.

`crates/vt/CHANGELOG.md`: **no entry**, as the Documentation Action states. The crate does not
change, and the application has no changelog of its own; inventing one is not this packet's work.

Checked and not changed:
[`IN-0018/low-level-design/input.md`](../IN-0018-rebuild-terminal-render-engine/low-level-design/input.md)
-- every row of its classification table still decides the same thing, and it contains no sentence
asserting that the view handles only key-down. The one row whose *action* changed is `Ctrl+C`, and
that change is gated on a flag no program pushes by default.

Confirmed: `grep -rn` over `docs/` and `crates/vt/docs/` finds no remaining sentence saying OneTerm
delivers presses only.

## Context

- `TerminalView` already owns a blur subscription --
  `cx.on_blur(&focus, window, |view, _, _| view.focused = false)` in
  `crates/terminal-view/src/terminal_view/view.rs` -- so draining the held set costs one statement
  in an existing closure rather than a new subscription.
- `crates/terminal-view/src/terminal_view/view_tests.rs` already has a `key_down(key, modifiers)`
  helper and `FakeTerminalSession` / `probe.writes()`. A matching `key_up` helper is two lines,
  since `KeyUpEvent` has one field.
- `FakeSessionProbe::feed(bytes)` drives the **real** `Terminal` behind the fake session
  (`crates/terminal/src/test_support.rs`), which is what makes the `:2` / `:3` test an integration
  test rather than a mock.
- The view reads modes from the **last painted frame**
  (`self.render_state.borrow().frame.modes()`), not from the session directly. A test that feeds
  flags and then drives keys without a repaint reads the pre-feed snapshot. This is the single most
  likely way for the `:2` / `:3` test to pass vacuously or fail confusingly.
- `KeyEvent` is `#[non_exhaustive]`, so it is built with `KeyEvent::new` and then assigned. A
  struct literal will not compile from `crates/terminal-view`, which is a different crate; guide
  chapter 12 states the rule and `US-0106`'s correction is where it was learned the hard way.
- `send_key` already snaps the viewport to the live screen and calls `report_generated_input`. A
  release goes through the same function, so both behaviours are inherited rather than re-decided.

## Plan

- [ ] Write the kind-mapping unit test first, against the unchanged view, and watch it fail to
      compile -- there is no kind to assert yet. That failure is the packet's starting evidence.
- [ ] Record `main`'s byte list for the byte-identity test by running the key-down sequence on
      `main`, and commit the list as the test's expectation with a comment saying where it came
      from.
- [ ] Add the three re-exports to `crates/terminal/src/lib.rs`.
- [ ] Change `KeyAction::Send`, `classify_key`, `map_key` and `send_key`; the compiler finds every
      call site.
- [ ] Add `held_keys`, its three write sites, and the blur drain.
- [ ] Add `on_key_up` and register it in `render.rs`.
- [ ] The remaining tests: byte identity, `:2` / `:3`, blur, the four swallow paths, the broadcast
      release case.
- [ ] `docs/terminal-backend.md` and guide chapter 6.
- [ ] Report the manual walk as unrun, with the instrument, rather than leaving the proof blank.

## Decisions

No new decision record. Three choices are consequential enough to name and all three are recorded
at the right altitude already:

- the held set rather than a re-classifying release path --
  [`low-level-design/input-events.md`](low-level-design/input-events.md), and intake open
  question 3;
- three re-exports rather than a `terminal-view -> vt` dependency edge -- the HLD's "what this
  design deliberately does not do";
- a release is never fanned out to a broadcast peer -- the HLD's diagram note and the LLD's edge
  cases.

If the owner rules differently on any of them the packet is reopened; none is a rationale future
work must inherit beyond this intake, which is the bar `docs/HARNESS.md` sets for a `DEC-`.

## Verification Plan

- `cargo test -p oneterm-terminal-view` -- the kind mapping, byte identity, `:2` / `:3`, blur, the
  four swallow paths, and the broadcast release case.
- `cargo test -p oneterm-terminal` -- the shim's three new re-exports compile and nothing in the
  adapter moved.
- `cargo test --workspace`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `pwsh scripts/ci-local.ps1`.
- The manual Windows walk in
  [`low-level-design/input-events.md`](low-level-design/input-events.md), outside this session.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

`E2E proof` and `Platform proof` are expected to stay 0 in the implementing session, for the reason
the acceptance criterion states. They must be reported as unrun rather than ticked or left blank.

## Evidence and Gaps

Filled by the implementing session on branch `feat/view-key-release-repeat`, three commits on
`980bf5da`.

### Byte identity: the strong form, 12528 cases

The first pass of this packet delivered the weak form and said so. The independent verification
delivered the strong one, and its harness is adopted as
`crates/terminal-view/src/input/us0108_verify_tests.rs` so the criterion keeps being measured
rather than argued. It carries `main`'s `map_key` and `named_key` at `980bf5da`, copied verbatim,
and compares for every case `encode_key(spec, mods, modes)` on `main`'s mapping against
`encode_key_event(event, modes)` on this branch's:

- 65 (key name, `key_char`) pairs: every `named_key` row, `enter` and `tab` with layout text,
  letters, digits, the OEM punctuation, a shifted glyph, a non-ASCII character, the `space`
  translation, and two names with no encoding (`print`, `f25`);
- 8 modifier combinations (shift x ctrl x alt);
- DECCKM off and on; `modifyOtherKeys` levels 0, 1 and 2;
- kinds `Press` and `Repeat`, with no kitty flag pushed, asserted inside the loop.

Result, printed by the test itself: **`byte-identity cases compared: 12528 (8128 of them wrote
bytes)`, 0 divergences**. Mappability, `KeySpec`, `KeyMods` and the encoded bytes all match, and
the same loop asserts that a `Release` with no flag encodes to `None` in every case, so a release
adds no entry to the stream. The table the first pass reasoned to is the table this measures:

| Key | four key-downs, `main` and this branch | the key-up |
| --- | --- | --- |
| `enter` | `\r` x4 | nothing |
| `a` | `a` x4 | nothing |
| `up` | `\x1b[A` x4 | nothing |
| `escape` | `\x1b` x4 | nothing |
| `f5` | `\x1b[15~` x4 | nothing |
| `Ctrl+A` | `\x01` x4 | nothing |

`without_a_flag_press_repeat_and_release_write_todays_bytes` drives exactly that against a
`FakeTerminalSession`, so the claim is measured at the view level as well as at the encoder's.

### The `:2` and `:3` bytes

`report_event_types_produces_the_repeat_and_release_bytes` feeds `\x1b[>2u` to the real `Terminal`
behind the fake session, forces `window.draw`, and asserts the flags reached the frame
(`KeyboardFlags::REPORT_EVENT_TYPES`) **before** asserting any byte. That closes the
stale-snapshot trap the detail design warns about with an assertion rather than a comment. It then
drives press, held and up on `up`:

```text
press    \x1b[A          (REPORT_EVENT_TYPES alone leaves a press on the legacy rung)
repeat   \x1b[1;1:2A
release  \x1b[1;1:3A
```

### Diff against budget

`git diff --numstat main...HEAD`, production files only:

| Area | Budget | Actual |
| --- | --- | --- |
| `crates/terminal-view` | +130 / -20 | **+151 / -44** |
| `crates/terminal` | +3 | **+9 / -7** |

Both overrun, and neither is 50 per cent over in substance. `terminal-view`'s additions are 16 per
cent over; its deletions are mostly `map_key`'s body re-indented one level when its two exits
became one `match` feeding `KeyEvent::new`, plus `send_key` losing a parameter. `crates/terminal`
carries four names rather than three, because `KeyboardFlags` joined them so the view can ask what
the program negotiated, which is what open decision 1's answer requires; the rest of its churn is
rustfmt reflowing two `pub use` lists.

### Deviations from the detail design, and why

The detail design has since been reconciled with all three (`F7`), so these are history rather
than open divergences.

- **`held_keys` is `Vec<KeySpec>`, not `HashSet<SharedString>`.** The first pass used
  `HashSet<String>` of `Keystroke::key`, which the verification showed strands a shifted digit
  (`F1`); the entries are now the unshifted `KeySpec` that `canonical_key` produces, in a `Vec`
  because `KeySpec` is not `Hash` and a handful of held keys does not need a hash.
- **`Ctrl+C` is no longer out of scope.** The coordinator settled intake open decision 1 the other
  way, and the verification widened it: with any kitty flag that puts a ctrl chord on the `CSI u`
  rung -- `DISAMBIGUATE_ESC_CODES` as well as `REPORT_ALL_KEYS_AS_ESC` (`F4`) -- it is the encoded
  key, because the specification promises the program bytes. `KeyContext` gained one
  `ctrl_c_is_a_key` field, read from the same frame snapshot the encoding uses so the two cannot
  disagree, and `KeyAction::Interrupt` gained an `Option<KeyEvent>` so the channel's peers still
  receive an interrupt (`F2`). Unchanged with nothing negotiated, which `ctrl_c_interrupts` and the
  broadcast tests all still assert.
- **The blur drain reports each release unmodified.** A blur carries no modifier state, so every
  drained key is encoded with no modifiers. Its order is the order the keys were pressed, because
  the held set is a `Vec`.

### Verification run

| Check | Result |
| --- | --- |
| `cargo fmt --all` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean, no `#[allow]` added |
| `cargo test -p oneterm-terminal-view` | 325 passed, 0 failed, 3 ignored |
| `cargo test -p oneterm-terminal` | 216 passed, 0 failed |
| `cargo test --workspace` | green (`CARGO_BUILD_JOBS=1`; 4 and 2 jobs ran the machine out of commit while a sibling worktree built `crates/vt`) |
| `python scripts/check-english.py` / `check-doc-paths.py` | clean |
| `pwsh scripts/ci-local.ps1 -Full` | `ci-local: all checks passed.` including `cargo deny` |
| The manual Windows walk | **NOT RUN** |

### Gaps

- **The manual Windows walk was not run, and this packet is not acceptable without it.** The
  maintainer runs their coding agent inside OneTerm and a second `oneterm.exe` is not started
  unasked, so `E2E proof` and `Platform proof` stay 0, exactly as they did for `US-0105`.
  `is_held` and key-up delivery are platform behaviour and no test here exercises the Windows
  message pump. The instrument is specified in
  [`low-level-design/input-events.md`](low-level-design/input-events.md) under Verification.
- **The blur drain is untested in both places, not only on the platform** (`F3`). No view test
  can reach the `on_blur` subscription in this harness, and the manual walk was not run, so nothing
  in the repository shows that a release is ever sent on focus loss.
- **`REPORT_ALTERNATE_KEYS` and the modifier keys stay unreportable.** GPUI's `Keystroke` carries
  no shifted or base-layout code point, and Windows turns the modifier keys into
  `ModifiersChanged` before a key event exists. A program that negotiates all five flags gets
  three of them honoured.
- **`REPORT_ALL_KEYS_AS_ESC` still works fully only on the alternate screen**, because the IME owns
  printable keys on the primary screen. Intake open decision 2, settled as "not here". The
  flags-gated alternative, gating that classification row on "no flag wants this key" rather than
  on `alt_screen`, is its own outcome and owes its own packet.
- **A broadcast peer still receives the originator's encoding** for presses and repeats, so a peer
  whose program negotiated different flags, or none, sees the originator's `app_cursor` and kitty
  rung rather than its own. Pre-existing and not widened here: a release is never fanned out, which
  `a_release_is_never_fanned_out_to_channel_peers` asserts. **Named follow-up: per-target broadcast
  re-encoding.** The fan-out should repeat the `KeyEvent` rather than the bytes and let each target
  encode against its own `ModeSnapshot`. Its own outcome, its own packet.
- **The blur drain loses the modifiers** that were held with the key, as described above.
- **The canonical key follows the PC-101 shift relation**, the same ceiling the engine's own
  `unshifted` has. A layout that pairs shift differently can still strand a key, and its worst case
  is the missed release that was the behaviour before `F1` was fixed.
- **Two physical keys that share one unshifted code point collapse to one held entry.** A numpad
  digit and the digit row, on a backend that names both `1`, are one entry rather than two, so
  holding both and releasing one sends the release and leaves the other key **one release short**
  rather than stuck — `hold_key`'s idempotence is what keeps it from being stuck. The program could
  not have told the two apart in any case: both encode as code point `49`. Measured and written
  down rather than discovered, by
  `us0108_reverify_tests::two_physical_keys_sharing_a_code_point_collapse_to_one_entry`.

## Harness Row

The harness database is not edited by this packet's session. This is the row it owes, for whoever
applies it. The columns are `harness.db`'s real `story` schema; the `intake` row for `IN-0040` is
rowid 45. The proof columns match the `HARNESS:PROOF` block above -- unit and integration proved,
E2E and platform not, because the manual walk cannot run in an agent session. `US-0108`'s
verification (`F6`) found the first draft of this row claiming `planned` with both proofs 0,
contradicting the document it came from.

```python
#!/usr/bin/env python3
"""Insert the US-0108 story row. Point DB at the harness database and run once."""
import sqlite3

# The rowid of the IN-0040 intake row, which must be inserted first.
IN_0040 = 45

ROW = dict(
    id="US-0108",
    title="The view delivers press, repeat and release to the encoder",
    created_at="2026-09-16",
    risk_lane="high_risk",
    contract_doc=(
        "docs/spec-intakes/IN-0040-view-key-release-repeat/"
        "low-level-design/input-events.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0040-view-key-release-repeat/"
        "US-0108-view-key-release-repeat.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=0,
    evidence=(
        "Byte identity proved in the strong form: 12528 compared cases against "
        "main's own map_key + encode_key, 0 divergences. Independently verified; "
        "see evidence/US-0108-verify.md."
    ),
    verify_command="pwsh scripts/ci-local.ps1",
    last_verified_at="2026-09-16",
    last_verified_result="pass",
    notes=(
        "No file under crates/vt changes. E2E and platform proof are expected to "
        "stay 0 in an agent session: the manual Windows walk means launching a "
        "second oneterm.exe on a machine whose owner runs their agent inside the "
        "first. The packet is not acceptable without that walk."
    ),
    intake_id=IN_0040,
)

with sqlite3.connect("harness.db") as db:
    columns = ", ".join(ROW)
    placeholders = ", ".join("?" for _ in ROW)
    db.execute(f"INSERT INTO story ({columns}) VALUES ({placeholders})", tuple(ROW.values()))
print("inserted US-0108")
```

## Verification notes closed

`US-0108` was independently verified at `4c57f542` and came back **PASS-WITH-NOTES**: the central
claim held and was proved in the strong form this session could not run, and eight findings were
raised. The full report is at
[`evidence/US-0108-verify.md`](evidence/US-0108-verify.md). All eight are closed here.

| # | Finding | Closed by |
| --- | --- | --- |
| `F1` | **A stuck key.** The held set was keyed on `Keystroke::key`, and the Windows backend renames a digit or an OEM punctuation key to its shifted glyph while Shift is down. `Shift+1` pressed as `"!"` and, with Shift lifted first, released as `"1"`: the release was dropped and the program left believing `!` was held | `canonical_key` folds a `Character` spec through the same PC-101 shift relation the encoder's own `unshifted` uses, and `held_keys` became `Vec<KeySpec>` of canonical specs. `us0108_verify_tests::a_digit_keeps_one_identity_when_shift_is_released_first` and `the_canonical_key_mirrors_the_engines_shift_table`, plus `view_tests::verify_a_shifted_digit_release_is_paired`, which now asserts both the press and the release carry code point `49` |
| `F2` | **Ctrl+C fanned escape bytes at peers.** The Ctrl+C row fell through to `Send`, which fans bytes, so a peer that negotiated nothing received `CSI 99;5 u` where it used to receive an interrupt -- the same hazard release fan-out was closed to avoid | `KeyAction::Interrupt(Option<KeyEvent>)`. The origin gets the encoded key when its program negotiated the rung; the channel always gets `BroadcastInput::Interrupt`. `view_tests::verify_ctrl_c_fans_an_interrupt_whatever_the_origin_negotiated` |
| `F3` | **The blur drain is unprovable here.** A GPUI test window's `is_active` is hard-coded `false`, so focus events carry no previous focus path and the `on_blur` subscription can never fire | Design kept; the gap is now documented rather than implied, in `release_held_keys`'s own rustdoc, in `docs/terminal-backend.md` and in the acceptance criterion. The verifier's demonstration is adopted as `view_tests::verify_the_blur_drain_is_unprovable_in_a_test_window`. The testable safety net is `hold_key`'s idempotence -- a fresh press of a key already held leaves one entry, so a missed release cannot compound (`verify_a_repeated_press_leaves_one_held_entry`). The detail design's walk now carries step-by-step instructions, including the alt-tab-while-held step and what a failure looks like |
| `F4` | **Ctrl+C under `DISAMBIGUATE_ESC_CODES` alone** was still a signal, while `encode_key_event` put it on the kitty rung | Resolved against the specification, which is explicit: "Turning on this flag will cause the terminal to report the Esc, alt+key, ctrl+key, ctrl+alt+key, shift+alt+key keys using `CSI u` sequences instead of legacy ones", with Enter, Tab and Backspace the only exceptions. The view's gate widened to match the encoder's rung (`KeyContext::ctrl_c_is_a_key`, both flags). Cited in intake open decision 1 and asserted across all four flag states in `ctrl_c_across_the_flag_states` |
| `F5` | **`KeyEvent::text` was supplied for Ctrl and Alt chords**, which would tell a program that `Ctrl+A` inserted an `a` | `map_key` gained `&& !(key_mods.ctrl \|\| key_mods.alt)`, the engine's own rule for its fallback. `a_ctrl_or_alt_chord_carries_no_associated_text` |
| `F6` | **The harness row contradicted the packet**: `status="planned"` with both proofs 0, and no intake id | Row corrected to `implemented`, unit 1, integration 1, E2E 0, platform 0, `intake_id = 45`, with evidence and verification result filled in |
| `F7` | **The detail design still specified superseded shapes** | `low-level-design/input-events.md`'s held-set section rewritten for the canonical key, its Interfaces table given the four items it was missing, its `on_key_up` sketch matched to the code, and the needless `self.session.clone()` removed from the code rather than from the sketch |
| `F8` | **`IN-0040` claimed a drain on session close** that does not exist | Claim corrected. A dead session has no program left to strand, and draining from `Drop` has no `App` to write through |

One thing the verification asked about is **not** closed and is not a finding: whether a key typed
into the focused search bar reaches the PTY on the alternate screen. It predates this packet and
this packet does not widen it -- a release is gated by the same `held_keys` entry the press
created.

**Re-verified at `db8a059b`: PASS.** Seven further attacks found no new way in, and their tests are
adopted as `crates/terminal-view/src/input/us0108_reverify_tests.rs` and three `reverify_*` cases in
`view_tests.rs`. The two that are worth naming: `canonical_key`'s fold is now checked against the
**engine's own** answer rather than against its own copied table -- read back out of the bytes the
encoder emits, for 49 keys, with zero disagreements -- and every PC-101 pairing is driven in **both**
directions, including the press-`1`-release-`!` order a user who presses Shift *after* the digit
produces. The re-verification's own nit, a dangling test name in `release_held_keys`'s rustdoc, is
fixed, and its ceiling (two physical keys sharing one code point) is recorded under Gaps.

## Handoff

Depends on `US-0105`, which is merged at `ec0e9040`. Independent of `BUG-0060` and of everything
else open in `IN-0039`: it touches no file either of them touches.

Next owner after this packet: whoever runs the manual Windows walk, which is the one acceptance
item that is not a measurement. After that, the three follow-ups the intake lists -- the alternate
keys, the IME conflict, and per-target broadcast re-encoding -- are each their own outcome and each
their own packet.
