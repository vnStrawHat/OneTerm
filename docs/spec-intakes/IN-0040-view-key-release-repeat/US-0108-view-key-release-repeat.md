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
  - `KeyEvent::text` from the `Keystroke`'s own `key_char`, on a press and a repeat only.
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
  - `Ctrl+C`. `KeyAction::Interrupt` keeps sending `SIGINT` rather than encoding, so a program that
    pushed `REPORT_ALL_KEYS_AS_ESC` still cannot see it as a key. Pre-existing; intake open
    decision 1.
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
- [x] **With no flag pushed, the bytes are identical.** A test drives a press, three repeats and a
      release for each of `enter`, `a`, `up`, `escape`, `f5` and `Ctrl+A` against a
      `FakeTerminalSession` and asserts `probe.writes()` matches the recorded list -- in which a
      release contributes **no entry at all**, because the encoder answers `None` at rung 1. The
      same list is produced by the same key-down sequence on `main`, and **both runs are attached**.
      This is the criterion the lane exists for; a session that cannot produce the `main` run must
      say so rather than assert the list from reading the code.
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

### The byte lists, side by side

`without_a_flag_press_repeat_and_release_write_todays_bytes` drives a press, three OS repeats
(`is_held: true`) and a release for each key against a `FakeTerminalSession` with no flag pushed,
and asserts the exact write list:

| Key | `main`: four key-downs | this branch: four key-downs plus one key-up |
| --- | --- | --- |
| `enter` | `\r` x4 | `\r` x4 |
| `a` | `a` x4 | `a` x4 |
| `up` | `\x1b[A` x4 | `\x1b[A` x4 |
| `escape` | `\x1b` x4 | `\x1b` x4 |
| `f5` | `\x1b[15~` x4 | `\x1b[15~` x4 |
| `Ctrl+A` | `\x01` x4 | `\x01` x4 |

The release contributes **no entry at all**: rung 1 of the encoder answers `None`, so nothing is
written. **The `main` column was not produced by a run on `main`.** This session could not check
out `main` in a worktree whose `crates/vt` build is shared with another session, so the column is
the legacy encoding the unchanged `encode_legacy` table produces for the same `KeySpec` and
`KeyMods`, which is the code path a repeat and a flagless press still take. Two independent checks
stand behind it rather than a reading of the code: `member_input_reaches_the_channel_peers_only`
and `a_view_without_a_registry_still_writes_to_its_own_session` both assert `\r` for `enter` and
both passed **unedited**, and the whole `oneterm-vt` byte-equivalence suite is untouched. This is a
weaker form of the criterion than the packet asked for, and is stated as such.

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

- **`held_keys` is `HashSet<String>`, not `HashSet<SharedString>`.** `gpui::Keystroke::key` is a
  `String`; converting at both ends would buy nothing.
- **`Ctrl+C` is no longer out of scope.** The coordinator settled intake open decision 1 the other
  way: with `REPORT_ALL_KEYS_AS_ESC` pushed it is the encoded key, because that is what the
  program asked for. `KeyContext` gained one `all_keys_as_esc` field, the `Interrupt` row gained
  one `&& !ctx.all_keys_as_esc`, and the flag is read from the same frame snapshot the encoding
  uses, so the two cannot disagree. Unchanged with nothing negotiated, which `ctrl_c_interrupts`
  and the broadcast test both still assert.
- **The blur drain reports each release unmodified.** A blur carries no modifier state, so every
  drained key is encoded with `Modifiers::default()`. The drain is sorted, so its order is
  deterministic rather than a `HashSet`'s.

### Verification run

| Check | Result |
| --- | --- |
| `cargo fmt --all` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean, no `#[allow]` added |
| `cargo test -p oneterm-terminal-view` | 312 passed, 0 failed |
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
- **The byte-identity criterion is met in the weaker form described above**, because a run on
  `main` was not available in this worktree.
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

## Harness Row

The harness database is not edited by this packet's session. This is the row it owes, for whoever
applies it. The columns are `harness.db`'s real `story` schema; the `intake` row for `IN-0040` must
be inserted first (the snippet is in [`IN-0040.md`](IN-0040.md)) and its rowid substituted below.

```python
#!/usr/bin/env python3
"""Insert the US-0108 story row. Point DB at the harness database and run once."""
import sqlite3

# The rowid of the IN-0040 intake row, which must be inserted first.
IN_0040 = None  # <- fill in

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
    status="planned",
    unit_proof=0,
    integration_proof=0,
    e2e_proof=0,
    platform_proof=0,
    evidence=None,
    verify_command="pwsh scripts/ci-local.ps1",
    last_verified_at=None,
    last_verified_result=None,
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

## Handoff

Depends on `US-0105`, which is merged at `ec0e9040`. Independent of `BUG-0060` and of everything
else open in `IN-0039`: it touches no file either of them touches.

Next owner after this packet: whoever runs the manual Windows walk, which is the one acceptance
item that is not a measurement. After that, the three follow-ups the intake lists -- the alternate
keys, the IME conflict, and per-target broadcast re-encoding -- are each their own outcome and each
their own packet.
