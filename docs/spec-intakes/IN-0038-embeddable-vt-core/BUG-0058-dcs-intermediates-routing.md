# Work: DCS routing ignores intermediates, so DECRQSS and XTGETTCAP open the Sixel decoder

ID: BUG-0058
Intake: IN-0038
Created: 2026-09-15

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

- [ ] In scope: `Handler::dcs_hook` in `crates/vt/src/terminal/dispatch.rs:1311-1319`, and its
  regression tests.
- [ ] Out of scope: **answering** DECRQSS or XTGETTCAP. Both stay unhandled and counted; replying to
  them is conformance work and belongs to `US-0102` or later. This packet stops the wrong thing from
  happening, it does not start a new right thing.
- [ ] Out of scope: every other DCS final byte, `dcs_put`, `dcs_unhook`, and the Sixel decoder
  itself.

## Acceptance

Each criterion is a command a verifier can run, not a claim to be believed.

- [ ] Feeding `\x1bP$qm\x1b\\` leaves `Terminal` with **no** in-flight graphics parser and produces
  **no** `VtEvent` other than the end-of-batch `Repaint`; `FeedStats::unhandled_sequences` is
  exactly 1 and `aborted_dcs` is 0.
- [ ] Feeding `\x1bP+q544e\x1b\\` behaves identically.
- [ ] Feeding a valid one-pixel Sixel (`\x1bPq#0;2;0;0;0#0~\x1b\\`) still places exactly one
  graphic. The existing `crates/vt/src/graphics/graphics_tests.rs` suite passes untouched.
- [ ] Feeding `\x1bPq` (an unterminated Sixel) followed by `\x1bP$qm\x1b\\` clears the in-flight
  decoder and discards the partial image -- the documented "a non-Sixel DCS aborts the prior
  unterminated one" behaviour is preserved for intermediates as well as for other final bytes.
- [ ] The three new tests fail on `main` @ `36977ca` and pass on this branch. A verifier proves this
  by checking out `main`, applying only the test file, and observing the failures.
- [ ] `cargo test -p oneterm-vt` and `cargo test -p oneterm-vt --features vt-paranoid` green.
- [ ] No behaviour change for any byte sequence that does not contain a DCS with an intermediate: the
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

Before completion: list the `dispatch-and-modes.md` edit, and confirm `graphics.md` and `parser.md`
needed no change.

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

- [ ] Match on `(byte, intermediates)` instead of `byte` alone, mirroring the shape `esc()` and
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
- [ ] Rewrite the doc comment to state the rule the code now implements, and name DECRQSS and
  XTGETTCAP as the two sequences the old rule mis-routed.
- [ ] Add the three regression tests to `crates/vt/src/terminal/terminal_tests.rs`, next to the
  existing DCS cases.
- [ ] Update the DCS paragraph in `dispatch-and-modes.md`.
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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record: the three test names and their output; the `main`-versus-branch run
showing they fail before and pass after; the corpus replay result.

Known gap to carry forward, not to fix here: neither DECRQSS nor XTGETTCAP is **answered**. A program
that asks now gets silence rather than a wrong answer, which is correct but incomplete. Recorded in
`US-0102`'s out-of-scope list so it is not lost.

## Handoff

Not expected to cross a session. If it does: the whole change is one `if` condition, one doc comment,
three tests and one design-document paragraph.
