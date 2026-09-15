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
- [x] Feeding `\x1bPq` (an unterminated Sixel) followed by `\x1bP$qm\x1b\\` clears the in-flight
  decoder and discards the partial image -- the documented "a non-Sixel DCS aborts the prior
  unterminated one" behaviour is preserved for intermediates as well as for other final bytes.
- [x] The three new tests fail on `main` @ `36977ca` and pass on this branch. A verifier proves this
  by checking out `main`, applying only the test file, and observing the failures.
- [x] `cargo test -p oneterm-vt` and `cargo test -p oneterm-vt --features vt-paranoid` green.
- [x] No behaviour change for any byte sequence that does not contain a DCS with an intermediate: the
  46 frozen parity corpus recordings replay identically
  (`cargo run -p oneterm-tools --bin <parity harness>`), byte for byte.

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
across `docs/` returns exactly one hit, `graphics.md:141`, so that is the paragraph that was edited.

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
that up to `DCS_MAX_BYTES` (16 MiB) of a hostile payload is buffered in an image decoder that was
never meant to see it.

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
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Change: one `if` condition and its doc comment in `crates/vt/src/terminal/dispatch.rs`, three tests
in `crates/vt/src/terminal/terminal_tests.rs`, one paragraph in `graphics.md`.

```rust
// crates/vt/src/terminal/dispatch.rs
if byte == b'q' && intermediates.is_empty() {
```

The three tests, in `crates/vt/src/terminal/terminal_tests.rs` SS Unhandled input:

- `an_intermediate_dcs_q_is_not_sixel` -- feeds `P$qm\` and `P+q544e\`; asserts
  `graphics.parser` is `None`, `unhandled_sequences == 1`, `aborted_dcs == 0`, no image taken, and
  that the batch holds exactly `[VtEvent::Repaint]`.
- `a_bare_dcs_q_still_decodes_a_sixel` -- feeds `Pq#0;2;0;0;0#0~\`, asserts exactly one
  graphic.
- `an_intermediate_dcs_aborts_an_unterminated_sixel` -- feeds `Pq`, asserts a decoder is
  in flight, then feeds `P$qm` and asserts the decoder is gone and the DECRQSS was counted, then
  feeds the `ST` and asserts no image and no placement.

Fail-before / pass-after, run by reverting only the `&& intermediates.is_empty()` clause in the
working tree (which is what `main` @ `6dc3331` has) and running the same three tests:

```
running 3 tests
test terminal::tests::a_bare_dcs_q_still_decodes_a_sixel ... ok
test terminal::tests::an_intermediate_dcs_aborts_an_unterminated_sixel ... FAILED
test terminal::tests::an_intermediate_dcs_q_is_not_sixel ... FAILED

---- an_intermediate_dcs_aborts_an_unterminated_sixel ----
assertion failed: session.term.state.graphics.parser.is_none()
---- an_intermediate_dcs_q_is_not_sixel ----
assertion `left == right` failed   left: 0   right: 1     (unhandled_sequences)

test result: FAILED. 1 passed; 2 failed
```

Deviation from the acceptance wording, recorded rather than hidden: **two** of the three tests fail
on the old behaviour, not three. `a_bare_dcs_q_still_decodes_a_sixel` asserts behaviour that was
already correct -- it is the non-regression guard for the other half of the routing key, and a test
that pins existing behaviour cannot fail before the change. The two that encode the defect both
fail.

The fail-before run also shows the compiler catching the revert (`warning: unused variable:
intermediates`), which is a second, independent signal that the argument is the whole fix.

Corpus parity: `crates/tools/tests/corpus_check.rs` replays the frozen recordings inside
`cargo test --workspace`. Both gates pass --
`the_engine_matches_the_frozen_alacritty_expectations` (the 45 vendored alacritty recordings) and
`the_engine_matches_the_frozen_oneterm_expectations` (OneTerm's own set). Note for the record: the
acceptance list says 46 recordings; the corpus is 45 vendored plus OneTerm's own directory, which
`corpus_check.rs:24` pins.

Verify command: `pwsh scripts/ci-local.ps1 -Full` with `CARGO_BUILD_JOBS=4`. All twelve steps pass,
`cargo deny check licenses bans advisories` included -- "ci-local: all checks passed."
`cargo test -p oneterm-vt` is 367 passed / 0 failed / 2 ignored, and the same with
`--features vt-paranoid`.

Not verified: nothing was run against a real tmux, neovim or kitty session. The claim that those
programs send XTGETTCAP at startup is taken from the intake, not measured here; the tests pin the
byte-level behaviour, not the client that produces the bytes.

Known gap to carry forward, not to fix here: neither DECRQSS nor XTGETTCAP is **answered**. A program
that asks now gets silence rather than a wrong answer, which is correct but incomplete. Recorded in
`US-0102`'s out-of-scope list so it is not lost.

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
    created_at="2026-09-15",
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
    platform_proof=0,
    evidence=(
        "3 tests in crates/vt/src/terminal/terminal_tests.rs; 2 of them fail with the "
        "intermediates check reverted, all 3 pass with it; corpus parity unchanged "
        "(frozen alacritty + oneterm recordings)."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "Fix is one condition: dcs_hook routes on (intermediates, final). DECRQSS and "
        "XTGETTCAP stay unhandled and counted; answering them is US-0102."
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
