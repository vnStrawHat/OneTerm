# Work: DCS routing ignores intermediates, so DECRQSS and XTGETTCAP open the Sixel decoder

ID: BUG-0058
Intake: IN-0038
Created: 2026-09-15

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: normal (an engine defect with no security or data consequence; the intake's high-risk
  lane comes from the public contract, which this packet does not touch)
- Spec Intake: `IN-0038`

## Outcome

`DCS $ q ... ST` (DECRQSS) and `DCS + q ... ST` (XTGETTCAP) no longer start the Sixel decoder. Only
`DCS q` with no intermediate does. A program that asks the terminal about a setting or a terminfo
capability gets its query ignored and counted, instead of having its payload fed byte by byte into
an image decoder.

## Scope

- [x] In scope: `Handler::dcs_hook` in `crates/vt/src/terminal/dispatch.rs:1311-1319`, and its
  regression tests.
- [x] Out of scope: **answering** DECRQSS or XTGETTCAP. Both stay unhandled and counted; replying to
  them is conformance work and belongs to `US-0102` or later. This packet stops the wrong thing from
  happening, it does not start a new right thing.
- [x] Out of scope: every other DCS final byte, `dcs_put`, `dcs_unhook`, and the Sixel decoder
  itself.

## Acceptance

Each criterion is a command a verifier can run, not a claim to be believed.

- [x] Feeding `\x1bP$qm\x1b\\` leaves `Terminal` with **no** in-flight graphics parser and produces
  **no** `VtEvent` other than the end-of-batch `Repaint`; `FeedStats::unhandled_sequences` is
  exactly 1 and `aborted_dcs` is 0.
- [x] Feeding `\x1bP+q544e\x1b\\` behaves identically.
- [x] Feeding a valid one-pixel Sixel (`\x1bPq#0;2;0;0;0#0~\x1b\\`) still places exactly one
  graphic. The existing `crates/vt/src/graphics/graphics_tests.rs` suite passes untouched.
- [x] Feeding `\x1bPq` (an unterminated Sixel) followed by `\x1bP$qm\x1b\\` leaves **no** in-flight
  decoder and adds no graphic of its own. Corrected during verification: this criterion originally
  said the partial image is aborted and discarded, which is not what the engine does. The `ESC` that
  introduces the second DCS ends the first one *normally*, so a partial Sixel **with a payload** is
  finished and placed; the empty one named here yields nothing only because `SixelParser::finish`
  returns `None`. Both shapes are now pinned --
  `verify_intermediate_dcs_aborts_an_empty_unterminated_sixel` and
  `verify_intermediate_dcs_after_a_nonempty_unterminated_sixel`.
- [x] The tests that encode the defect fail against the pre-fix `dcs_hook` and pass on this branch;
  the two non-regression guards for the Sixel half of the routing key pass on both, as a test that
  pins unchanged behaviour must. Measured on the final suite: **7 of 9 fail**, 2 pass. The base is
  `main` @ `6dc3331`. (This line originally read "the three new tests fail on `main` @ `36977ca`".
  `36977ca` is not an ancestor of `main`, and a guard test cannot fail before the change; both
  errors are corrected here rather than left ticked.)
- [x] `cargo test -p oneterm-vt` and `cargo test -p oneterm-vt --features vt-paranoid` green.
- [x] No behaviour change for any byte sequence that does not contain a DCS with an intermediate: the
  frozen parity corpus replays identically, gated by
  `cargo test -p oneterm-tools --test corpus_check` (which `cargo test --workspace` runs). The
  original draft's `cargo run -p oneterm-tools --bin <parity harness>` was a placeholder naming no
  real binary; `crates/tools/tests/corpus_check.rs:23` is the real gate, and it pins the vendored
  set at 45 recordings plus OneTerm's own directory, not 46.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` -- the DCS section and
  the "only Sixel is decoded" rule. It specifies the final byte and is silent on intermediates,
  which is how the defect passed review.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md` -- the Sixel decoder's entry
  conditions.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/parser.md` -- confirms the parser already
  collects DCS intermediates and passes them to `dcs_hook`; the argument is present and discarded,
  so no parser change is needed.
- `docs/osc-sequences-checklist.md` -- reviewed, no change: it covers OSC, not DCS.

### Documentation Action

**Update required.** `dispatch-and-modes.md`'s DCS paragraph states the rule as "any other final
byte clears an in-flight decoder". That sentence is what the code implements and it is wrong, so it
must become "any DCS that is not `DCS q` with no intermediate". One paragraph.

Reason: the design document is the source the implementation was written from, and leaving it saying
the defective thing guarantees the defect returns.

### Reconciliation

Correction to the Documentation Action above: the sentence that states the defective rule is in
`graphics.md` (SS Sixel decoder), not in `dispatch-and-modes.md`. Grep for "any other final byte"
across `docs/` returns **two** hits on `main`, not one as an earlier draft of this line claimed:

- `graphics.md:141` -- the live design rule. **Edited.**
- `docs/spec-intakes/IN-0029-vt-engine/research/api-surface.md:361` -- **no change**. It is a
  research note cataloguing the API of the *engine being replaced* (`Term`'s `dcs_hook` in the
  vendored alacritty patch), where the sentence is an accurate description of that code. Rewriting
  it would falsify a historical record; it describes what OneTerm moved away from.

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md` -- **edited**. The wiring
  sentence now reads "final byte `q` **and no intermediate** ... any other DCS clears an in-flight
  parser", and a following paragraph names DECRQSS and XTGETTCAP as the two sequences the old rule
  mis-routed and points the unanswered-query gap at `US-0102`.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` -- reviewed, **no
  change**. Its only DCS mentions are XTVERSION's reply shape (unaffected) and `RIS` dropping an
  in-flight DCS (unaffected). It states no DCS routing rule.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/parser.md` -- reviewed, **no change**. It
  already documents `dcs_hook(params, intermediates, final_byte)` and the `DcsIntermediate` state;
  the parser was always correct.
- `docs/osc-sequences-checklist.md` -- reviewed, **no change**. Its three DCS mentions are all
  "Sixel is carried over DCS" context in an OSC document; it lists no DCS routing.
- `docs/terminal-backend.md` -- reviewed, **no change**. Grep for `DCS` returns no hit.
- `crates/vt/CHANGELOG.md` -- does not exist yet, so the conditional plan item is a no-op here. The
  `Fixed` line stays `US-0097`'s to write when it creates the file.
- `US-0102` already carries the unanswered-DECRQSS/XTGETTCAP gap in its out-of-scope list
  (`US-0102-conformance-gaps.md:41`) and in its known-gaps paragraph (line 190). Nothing to add.

## Context

```rust
// crates/vt/src/terminal/dispatch.rs:1311
/// Only Sixel (`DCS q`) is decoded (`US-0080`). Any other final byte clears
/// an in-flight decoder, so a non-Sixel DCS arriving mid-Sixel aborts the
/// prior unterminated one -- parity with the engine being replaced.
fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], byte: u8) {
    self.state.dispatched = true;
    if byte == b'q' {
        self.state.graphics.parser = Some(SixelParser::new());
    } else {
        self.state.graphics.parser = None;
        self.unhandled();
    }
}
```

`DECRQSS` is `DCS $ q <sequence> ST` and `XTGETTCAP` is `DCS + q <hex names> ST`. Both have the
final byte `q`, so both take the first branch and their payloads are fed to `SixelParser::put` until
`ST`. `dcs_unhook` then calls `SixelParser::finish`, which returns `None` on garbage, so no image is
placed and no panic occurs -- the visible symptom is only that a query is silently mis-parsed and
that a hostile payload is buffered in an image decoder that was never meant to see it.

Two ceilings bound that exposure, and both are **read from the source, not measured** (`SixelParser`
exposes no accessor a test could size): `DCS_MAX_BYTES` (`crates/vt/src/parser/mod.rs:40`, 16 MiB)
caps the payload the parser will stream into the decoder, and `MAX_PIXEL_BYTES`
(`crates/vt/src/graphics/mod.rs:50`, 4096 * 4096 * 4 = 64 MiB) caps the pixel buffer the decoder
will grow from it. The pre-fix worst case is the pair, not the 16 MiB figure alone.

The parser already collects and passes intermediates; only the handler ignores them.

## Plan

- [x] Match on `(byte, intermediates)` instead of `byte` alone, mirroring the shape `esc()` and
  `csi()` already use in the same file:
  ```rust
  fn dcs_hook(&mut self, _params: &Params, intermediates: &[u8], byte: u8) {
      self.state.dispatched = true;
      if byte == b'q' && intermediates.is_empty() {
          self.state.graphics.parser = Some(SixelParser::new());
      } else {
          self.state.graphics.parser = None;
          self.unhandled();
      }
  }
  ```
- [x] Rewrite the doc comment to state the rule the code now implements, and name DECRQSS and
  XTGETTCAP as the two sequences the old rule mis-routed.
- [x] Add the three regression tests to `crates/vt/src/terminal/terminal_tests.rs`, next to the
  existing DCS cases.
- [x] Update the DCS paragraph in `dispatch-and-modes.md`.
- [ ] Add an `Unreleased` / `Fixed` line to `crates/vt/CHANGELOG.md` -- **if** `US-0097` has already
  created it. This packet runs first, so the CHANGELOG line is `US-0097`'s to add; note it there.

## Decisions

None. This is a defect against a rule that was already written down correctly in prose; no choice is
being made that future work must inherit.

## Verification Plan

- Focused: the three new tests in `terminal_tests.rs`, plus the whole `graphics_tests.rs` suite
  unchanged.
- Regression: `cargo test -p oneterm-vt`, `cargo test -p oneterm-vt --features vt-paranoid`,
  `cargo test --workspace`.
- Corpus: the 46 frozen parity recordings replay identically.
- Manual: none needed. No OneTerm surface changes; no shell emits DECRQSS during normal use, which
  is why this was not noticed.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Change, after the independent verification pass: two lines of production code and their doc comment
in `crates/vt/src/terminal/dispatch.rs`, one test in `crates/vt/src/terminal/terminal_tests.rs`,
eight in the adopted `crates/vt/src/terminal/verify_bug0058_tests.rs` (three lines of `#[cfg(test)]`
wiring in `terminal/mod.rs`), and one paragraph in `graphics.md`.

```rust
// crates/vt/src/terminal/dispatch.rs -- the whole production change
if byte == b'q' && intermediates.is_empty() {
    self.state.graphics.parser = Some(SixelParser::new());
} else {
    self.unhandled();
}
```

### The dead `else` branch, deleted

The original fix kept the pre-existing `self.state.graphics.parser = None;` in the `else` arm. It is
unreachable, and it was the source of the false "a non-Sixel DCS aborts the prior unterminated one"
story that both the doc comment and `graphics.md` carried. Proof that no path reaches it with a live
decoder:

- `graphics.parser` is assigned `Some` in exactly one place, the `if` arm of `dcs_hook`
  (`grep -rn "graphics.parser" crates/vt/src` returns four sites: the two arms, `dcs_put`'s
  `as_mut`, and `dcs_unhook`'s `take`).
- `dcs_hook` moves the parser straight to `DcsPassthrough` (`crates/vt/src/parser/mod.rs:244`).
- Every exit from `DcsPassthrough` calls `Dispatch::dcs_unhook`, which `take()`s the decoder:
  `CAN`/`SUB` (`state.rs:243`), `ESC` (`state.rs:247`), 8-bit `ST` (`state.rs:256`), and the
  `DCS_MAX_BYTES` cap (`state.rs:328`). Its `_ => ()` arm drops every other 8-bit byte, and unlike
  the DCS entry states it never calls `advance_anywhere`, so there is no side door.
- `Parser::reset()` would be such a door, but `Terminal` never calls it -- its only caller in the
  workspace is `parser_tests.rs:847`, and `Terminal::parser` is a private field, so an embedder
  cannot reach it either.

So `dcs_hook` always runs with `parser == None`, the store was a no-op, and `self.unhandled()` is
the whole `else` arm now. `verify_intermediate_dcs_after_a_nonempty_unterminated_sixel` pins the
consequence that the deleted line pretended to prevent: the first Sixel is finished and placed by
the `ESC`, and the DECRQSS behind it adds no second graphic.

### Tests

`crates/vt/src/terminal/terminal_tests.rs` (SS Unhandled input), one test:

- `an_intermediate_dcs_q_is_not_sixel` -- feeds `\x1bP$qm\x1b\\` and `\x1bP+q544e\x1b\\`;
  asserts `graphics.parser` is `None`, `unhandled_sequences == 1`, `aborted_dcs == 0`, no image
  taken, and that the event batch holds exactly `[VtEvent::Repaint]` -- the acceptance criterion no
  other test covers.

Two further tests written for the first draft were **dropped** rather than kept: the verifier's
`verify_bare_dcs_q_still_places_one_graphic` and
`verify_intermediate_dcs_aborts_an_empty_unterminated_sixel` assert the same bytes with strictly
stronger assertions. One of the dropped pair was also misnamed (`..._aborts_an_unterminated_sixel`
proved no abort, only an empty payload), so deleting it settles that finding too.

`crates/vt/src/terminal/verify_bug0058_tests.rs`, eight tests written independently by the verifier
and adopted unchanged apart from the header note:

| Test | Covers |
| --- | --- |
| `verify_intermediate_dcs_q_never_reaches_the_decoder` | DECRQSS and XTGETTCAP: no decoder, counted once, nothing placed, nothing echoed |
| `verify_bare_dcs_q_still_places_one_graphic` | the Sixel half of the key still decodes |
| `verify_parameterised_dcs_q_still_decodes` | `DCS 0;1 q` etc. -- parameters are not intermediates |
| `verify_intermediate_dcs_aborts_an_empty_unterminated_sixel` | empty unterminated Sixel, then DECRQSS |
| `verify_intermediate_dcs_after_a_nonempty_unterminated_sixel` | the same with a real payload: finished and placed, no second graphic |
| `verify_one_mib_intermediate_payload_buffers_nothing` | 1 MiB DECRQSS payload, decoder checked after every 64 KiB |
| `verify_eight_bit_st_ends_an_intermediate_dcs` | `0x9C` terminates it and the terminal returns to ground |
| `verify_overflowed_intermediates_do_not_fall_back_to_sixel` | intermediate overflow does not re-open the Sixel branch |

### Fail-before / pass-after

Run by restoring `main`'s `dcs_hook` body in the working tree (both the missing
`&& intermediates.is_empty()` and the dead store) and running the nine tests:

```
running 9 tests
test terminal::verify_bug0058_tests::verify_bare_dcs_q_still_places_one_graphic ... ok
test terminal::verify_bug0058_tests::verify_parameterised_dcs_q_still_decodes ... ok
test terminal::tests::an_intermediate_dcs_q_is_not_sixel ... FAILED
test terminal::verify_bug0058_tests::verify_eight_bit_st_ends_an_intermediate_dcs ... FAILED
test terminal::verify_bug0058_tests::verify_intermediate_dcs_aborts_an_empty_unterminated_sixel ... FAILED
test terminal::verify_bug0058_tests::verify_intermediate_dcs_after_a_nonempty_unterminated_sixel ... FAILED
test terminal::verify_bug0058_tests::verify_intermediate_dcs_q_never_reaches_the_decoder ... FAILED
test terminal::verify_bug0058_tests::verify_one_mib_intermediate_payload_buffers_nothing ... FAILED
test terminal::verify_bug0058_tests::verify_overflowed_intermediates_do_not_fall_back_to_sixel ... FAILED

test result: FAILED. 2 passed; 7 failed
```

Selected panics: `verify_one_mib_intermediate_payload_buffers_nothing` fails at its **first**
assertion ("the hook already opened a decoder"), which is the defect's mechanism stated directly --
a 1 MiB DECRQSS payload reaching a live `SixelParser`. `an_intermediate_dcs_q_is_not_sixel` fails on
`unhandled_sequences` (`left: 0, right: 1`): the query was not merely mis-parsed, it was not even
counted.

The two that pass are the non-regression guards. A test that pins behaviour which was already
correct cannot fail before the change; that is what it is for.

The revert also drew `warning: unused variable: intermediates` from the compiler -- an independent
signal that the argument is the whole fix.

### Gate

`pwsh scripts/ci-local.ps1 -Full` with `CARGO_BUILD_JOBS=4`: all twelve steps pass,
`cargo deny check licenses bans advisories` included -- "ci-local: all checks passed".
`cargo test -p oneterm-vt` is 373 passed / 0 failed / 2 ignored (the verifier report quotes 375,
measured before the two duplicated tests were dropped), and identical with
`--features vt-paranoid`. Corpus parity holds:
`the_engine_matches_the_frozen_alacritty_expectations` (45 vendored recordings) and
`the_engine_matches_the_frozen_oneterm_expectations` both green.

The independent verification report is
[`evidence/BUG-0058-verify.md`](evidence/BUG-0058-verify.md): verdict PASS-WITH-NOTES, every note
addressed above.

### Gaps

- Neither DECRQSS nor XTGETTCAP is **answered**. A program that asks now gets silence rather than a
  wrong answer, which is correct but incomplete. Carried in `US-0102`'s out-of-scope list
  (`US-0102-conformance-gaps.md:41`) and known-gaps paragraph (line 190); nothing to add.
- Not verified: no tmux, neovim or kitty session was driven against the built engine. The claim that
  those clients send XTGETTCAP comes from the intake, not from measurement, so both the source
  comment and `graphics.md` now hedge it ("clients such as ... are documented to send"). The tests
  pin the byte-level behaviour, not the client that produces the bytes.
- Not verified: the pre-fix allocation ceilings in Context are read from the source, not measured.
- Not run: `cargo fuzz` is not installed in this environment. The 1 MiB hostile-payload test and the
  corpus drift gate stand in for it.

## Handoff

Not expected to cross a session. If it does: the whole change is one `if` condition, one doc comment,
three tests and one design-document paragraph.

## Harness Row

`harness.db` was **not** written by this task: no harness binary is available in this worktree and
the task forbids editing the database. The schema is
`story(id, title, created_at, risk_lane, contract_doc, packet_doc, status, unit_proof,
integration_proof, e2e_proof, platform_proof, evidence, verify_command, last_verified_at,
last_verified_result, notes, intake_id)`, with the four `*_proof` columns as `0`/`1`.

```python
#!/usr/bin/env python3
"""Insert the BUG-0058 story row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    id="BUG-0058",
    title="DCS routing ignores intermediates, so DECRQSS and XTGETTCAP open the Sixel decoder",
    created_at="2026-09-15T18:40:00",
    risk_lane="normal",
    contract_doc="docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md",
    packet_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/"
        "BUG-0058-dcs-intermediates-routing.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=1,
    evidence=(
        "9 tests (1 in terminal_tests.rs, 8 adopted from the verifier in "
        "verify_bug0058_tests.rs); 7 fail against the pre-fix dcs_hook, 2 are "
        "non-regression guards; dead else-branch store deleted as unreachable; "
        "corpus parity unchanged. Verification: PASS-WITH-NOTES, "
        "docs/spec-intakes/IN-0038-embeddable-vt-core/evidence/BUG-0058-verify.md."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "dcs_hook routes on (intermediates, final). DECRQSS and XTGETTCAP stay "
        "unhandled and counted; answering them is US-0102. platform_proof=1 because "
        "ci-local -Full passed on Windows, the only platform exercised."
    ),
    intake_id=43,
)

with sqlite3.connect(DB) as db:
    db.execute(
        "INSERT INTO story ({}) VALUES ({})".format(
            ", ".join(ROW), ", ".join("?" * len(ROW))
        ),
        tuple(ROW.values()),
    )
```
