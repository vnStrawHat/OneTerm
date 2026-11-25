# Independent verification: BUG-0058 (DCS intermediates routing)

Packet: `docs/spec-intakes/IN-0038-embeddable-vt-core/BUG-0058-dcs-intermediates-routing.md`
Branch verified: `fix/vt-dcs-intermediates` @ `a76f9d4`, base `main` @ `6dc3331`
Verifier worktree: `.claude/worktrees/agent-aa0f509d38b8cb709` (nothing committed, nothing pushed)
Date: 2026-09-15

## Verdict

**PASS-WITH-NOTES.**

The fix is correct, minimal, and does what the packet says it does. It is in the right place --
the single routing function every DCS passes through -- so no sibling caller is left broken. Eight
independent tests written by the verifier all pass on the branch; six of the eight fail on `main`.
The full `ci-local -Full` gate is green with the verifier's tests added on top.

The notes are about record and comment accuracy, not about the fix. The most substantive one
(finding 2) is that the doc comment and the `graphics.md` paragraph the packet **edited** both
restate a claim -- "a non-Sixel DCS aborts the prior unterminated one" -- that the engine does not
actually implement. That claim is pre-existing on `main`; this packet copied it forward into its
documentation update rather than correcting it.

## 1. Diff scope -- confirmed clean

`git diff main..HEAD --stat`:

```
 crates/vt/src/terminal/dispatch.rs                 |  14 +-
 crates/vt/src/terminal/terminal_tests.rs           |  55 +++++++
 .../IN-0029-vt-engine/low-level-design/graphics.md |  12 +-
 .../BUG-0058-dcs-intermediates-routing.md          | 178 ++++++++++++++++++---
 4 files changed, 228 insertions(+), 31 deletions(-)
```

The only production-code change is in `crates/vt/src/terminal/dispatch.rs`:

- `dispatch.rs:1308-1314` -- the doc comment, rewritten.
- `dispatch.rs:1315` -- `_intermediates` renamed to `intermediates`.
- `dispatch.rs:1317` -- `if byte == b'q'` becomes `if byte == b'q' && intermediates.is_empty()`.

Nothing else. `dcs_put` (`dispatch.rs:1325`), `dcs_unhook` (`dispatch.rs:1331`), the parser and the
Sixel decoder are untouched. `cargo fmt --all` produced no change to either `dispatch.rs` or
`terminal_tests.rs`, so the implementer's files were already formatted.

Root-cause check: `dcs_hook` is the only DCS routing point in the crate, and the parser already
collected and passed intermediates (`crates/vt/src/parser/mod.rs:242`). The fix shape matches the
`(byte, intermediates)` key `csi()` (`dispatch.rs:917-924`) and `esc()` (`dispatch.rs:861-866`)
already use in the same file.

Bypass check: the parser keeps the first `MAX_INTERMEDIATES` bytes and sets `ignore` on overflow, it
does **not** clear the slice, so an over-long intermediate run still leaves `intermediates`
non-empty and still routes away from the decoder. Verified by test
`verify_overflowed_intermediates_do_not_fall_back_to_sixel`.

## 2. Verifier's own tests -- 8/8 pass on the branch

File (worktree only, **not committed**):
`crates/vt/src/terminal/verify_bug0058_tests.rs`, wired by a two-line `#[cfg(test)]` module in
`crates/vt/src/terminal/mod.rs`. Both are uncommitted working-tree changes.

| Test | Covers |
|---|---|
| `verify_intermediate_dcs_q_never_reaches_the_decoder` | `DCS $ q m ST` and `DCS + q 544e ST`: no decoder, `unhandled_sequences == 1`, `aborted_dcs == 0`, no pending graphic, no placement, nothing echoed |
| `verify_bare_dcs_q_still_places_one_graphic` | bare `DCS q` Sixel still places exactly one graphic |
| `verify_parameterised_dcs_q_still_decodes` | `DCS 0;1 q`, `DCS 0;1;0 q`, `DCS 7;1;0 q` -- parameters but no intermediate -- still decode, still `unhandled_sequences == 0` |
| `verify_intermediate_dcs_aborts_an_empty_unterminated_sixel` | unterminated `DCS q` then `DCS $ q ... ST` leaves no in-flight parser (implementer's case) |
| `verify_intermediate_dcs_after_a_nonempty_unterminated_sixel` | same shape with a real Sixel payload -- see finding 2 |
| `verify_one_mib_intermediate_payload_buffers_nothing` | 1 MiB payload behind `DCS $ q`: decoder checked mid-stream after every 64 KiB chunk, `pending` stays empty, `unhandled_sequences == 1` (once, not per byte), `aborted_dcs == 0`, no panic |
| `verify_eight_bit_st_ends_an_intermediate_dcs` | `0x9C` terminates the intermediate DCS and the terminal returns to ground (next printable byte reaches the grid) |
| `verify_overflowed_intermediates_do_not_fall_back_to_sixel` | `DCS $ + ! q` -- intermediate overflow does not re-open the Sixel branch |

Commands and results:

```
cargo test -p oneterm-vt verify_bug0058
  -> 8 passed; 0 failed

cargo test -p oneterm-vt
  -> 375 passed; 0 failed; 2 ignored   (367 implementer + 8 verifier)

cargo test -p oneterm-vt --features vt-paranoid
  -> 375 passed; 0 failed; 2 ignored
```

The packet's claim of "367 passed / 0 failed / 2 ignored" for `cargo test -p oneterm-vt` reproduces
exactly once the verifier's 8 tests are subtracted.

Memory-relevant state for the 1 MiB case: `GraphicsState` (`crates/vt/src/graphics/mod.rs:88-99`)
holds `pending`, `placements`, `released` and `parser`; the only byte buffer a DCS payload can reach
is `SixelParser::pixels`, and with `parser == None` `dcs_put` (`dispatch.rs:1325-1329`) is a no-op.
The test asserts `parser.is_none()` after every chunk, so a buffer that filled and was later dropped
would still be caught.

## 3. Fail-on-main reproduced independently

`git show main:crates/vt/src/terminal/dispatch.rs` was written over the working-tree file, the tests
were run, and the file was restored from a backup (`git status` then showed `dispatch.rs` clean).

`cargo test -p oneterm-vt --lib -- dcs_q dcs_aborts verify_bug0058` against `main`'s `dcs_hook`:

```
test result: FAILED. 4 passed; 8 failed

failures:
    terminal::tests::an_intermediate_dcs_aborts_an_unterminated_sixel
    terminal::tests::an_intermediate_dcs_q_is_not_sixel
    terminal::verify_bug0058_tests::verify_eight_bit_st_ends_an_intermediate_dcs
    terminal::verify_bug0058_tests::verify_intermediate_dcs_aborts_an_empty_unterminated_sixel
    terminal::verify_bug0058_tests::verify_intermediate_dcs_after_a_nonempty_unterminated_sixel
    terminal::verify_bug0058_tests::verify_intermediate_dcs_q_never_reaches_the_decoder
    terminal::verify_bug0058_tests::verify_one_mib_intermediate_payload_buffers_nothing
    terminal::verify_bug0058_tests::verify_overflowed_intermediates_do_not_fall_back_to_sixel
```

Selected panics:

```
an_intermediate_dcs_q_is_not_sixel        terminal_tests.rs:1659   left: 0  right: 1
verify_one_mib_intermediate_payload_...   verify_bug0058_tests.rs:161  "the hook already opened a decoder"
verify_intermediate_dcs_q_never_reach...  verify_bug0058_tests.rs:49   "\u{1b}P$qm\u{1b}\\" unhandled count  left: 0  right: 1
```

This confirms the packet's disclosed deviation exactly: of the implementer's three tests, **two**
fail on `main` and `a_bare_dcs_q_still_decodes_a_sixel` passes, because it pins behaviour that was
already correct. Of the verifier's eight, six fail on `main`; the two that pass are the two
non-regression guards (`verify_bare_dcs_q_still_places_one_graphic`,
`verify_parameterised_dcs_q_still_decodes`).

The `main` run also confirms the defect's mechanism directly: `verify_one_mib_intermediate_payload_
buffers_nothing` fails at the very first assertion, i.e. on `main` a 1 MiB DECRQSS payload is fed to
a live `SixelParser`.

## 4. Corpus and wider suites

`cargo fuzz` is not installed in this environment (not attempted).

```
cargo test -p oneterm-tools --test corpus_check
  running 2 tests
  test the_engine_matches_the_frozen_oneterm_expectations ... ok
  test the_engine_matches_the_frozen_alacritty_expectations ... ok
  test result: ok. 2 passed; 0 failed

cargo test -p oneterm-tools
  -> 14 passed; 0 failed  (plus the integration targets above)
```

`crates/tools/tests/corpus_check.rs:23` pins the vendored set at **45** recordings, plus OneTerm's
own directory. The packet's Evidence section already records this correction against its own
Acceptance wording of "46".

## 5. Records review

`python scripts/check-english.py` -> `English contributor-text check passed for 823 files.` (exit 0)
`python scripts/check-doc-paths.py` -> `Doc path check passed for 191 current paths in 11 documents.` (exit 0)

Status block: `Planned`/`In progress`/`Implemented` ticked, nothing else. Correct for a landed,
unaccepted packet.

Proof block: `Unit` and `Integration` and `Verify command passed` ticked, `E2E` and `Platform`
clear. Unit and integration proofs exist and were reproduced. See finding 9 on `Platform`.

`harness.db` INSERT snippet: the schema quoted in the packet was checked against a read-only copy of
`D:\TrungKFC-Research\Rust\myTerm2\harness.db` (the live file was **not** opened for writing and was
**not** modified). The `story` table's column list matches the packet's quote exactly, in order. The
four `*_proof` columns carry `CHECK(... IN (0,1))`, so `1/1/0/0` is valid. `status='implemented'`,
`risk_lane='normal'` and `last_verified_result='pass'` all satisfy their CHECK constraints.
`intake_id=43` is correct: `intake` row 43 is "oneterm-vt becomes a public embeddable terminal core
... (IN-0038)". No `BUG-0058` row exists in `story`, consistent with the packet's statement that the
database was not written.

`graphics.md` edit (`docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md:139-148`): the
new wiring sentence ("final byte `q` **and no intermediate**") and the new paragraph naming DECRQSS
and XTGETTCAP and pointing the unanswered-query gap at `US-0102` are accurate and match the code.
`US-0102-conformance-gaps.md` does carry the gap in both places the packet cites. One clause in the
edited sentence is inaccurate -- see finding 2.

## 6. Quality gate

```
$env:CARGO_BUILD_JOBS=4; pwsh scripts/ci-local.ps1 -Full
...
advisories ok, bans ok, licenses ok

ci-local: all checks passed.
```

Run with the verifier's 8 extra tests present, so the gate is green for the branch **plus** the
independent tests.

## Findings

1. **(Informational, confirmed)** The production diff is exactly the `intermediates.is_empty()`
   condition and its doc comment, at `crates/vt/src/terminal/dispatch.rs:1308-1317`. No other
   production file changed. The fix is at the single shared routing point, so no sibling caller is
   left unfixed.

2. **(Medium -- doc accuracy, propagated by this packet)** Both
   `crates/vt/src/terminal/dispatch.rs:1312-1314` and the edited paragraph at
   `docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md:141-143` state that "any other
   DCS clears an in-flight parser, so a non-Sixel DCS aborts a prior unterminated Sixel". The engine
   does not do this. `crates/vt/src/parser/state.rs:247` handles the `ESC` that introduces the next
   DCS by calling `dcs_unhook(dispatch, false)` -- an ordinary, **non-aborted** end -- which takes
   the parser and calls `SixelParser::finish`. So by the time `dcs_hook` runs,
   `state.graphics.parser` is always already `None`, the `else` branch's
   `self.state.graphics.parser = None` (`dispatch.rs:1320`) is a no-op on every reachable path, and
   a partial Sixel with a real payload is **finished and placed**, not aborted. Proven by
   `verify_intermediate_dcs_after_a_nonempty_unterminated_sixel`: after
   `ESC P q #0;2;0;0;0 #0 ~` then `ESC P $ q m` then `ST`, `placements().len() == 1` and
   `take_graphics().len() == 1`. Pre-existing on `main`, so not a regression -- but the packet's
   documentation update copied the wrong sentence forward instead of correcting it, which is the one
   thing the packet's own Documentation Action says such an update exists to prevent.

3. **(Low -- test naming)** `crates/vt/src/terminal/terminal_tests.rs:1683`
   `an_intermediate_dcs_aborts_an_unterminated_sixel` passes only because the Sixel it starts has an
   **empty** payload, so `finish()` returns `None` and no image appears. It does not demonstrate an
   abort. With any real payload the image is placed (finding 2). The test is still a useful
   regression guard for "no decoder left in flight"; the name over-claims.

4. **(Low -- record)** Acceptance line "The three new tests fail on `main` @ `36977ca`" names a
   commit that is **not an ancestor of `main`** (`36977ca` is "Merge US-0096 ... (IN-0037)"). The
   correct base is `6dc3331`, which the Evidence section uses. The two SHAs disagree inside one
   packet.

5. **(Low -- record)** That same Acceptance line is ticked `[x]` although only **two** of the three
   tests fail on the old behaviour. The deviation is disclosed honestly in Evidence and the
   reasoning is right, but the checkbox asserts a criterion that was not met as worded. Reword the
   criterion rather than leaving a tick that contradicts the evidence three sections below it.

6. **(Low -- record)** The Reconciliation claim "Grep for `any other final byte` across `docs/`
   returns exactly one hit, `graphics.md:141`" is false. On `main` there are **two**:
   `graphics.md:141` and `docs/spec-intakes/IN-0029-vt-engine/research/api-surface.md:361`. The
   second needs no change -- it is a research note describing the *vendored alacritty patch*, i.e.
   the engine being replaced, where the statement is accurate -- but the packet's claim as written
   is wrong and would mislead the next person who greps.

7. **(Low -- record)** The Acceptance section still contains the literal placeholder
   `cargo run -p oneterm-tools --bin <parity harness>`. The real gate is
   `crates/tools/tests/corpus_check.rs`, which Evidence names correctly. Placeholder left in a
   ticked acceptance criterion.

8. **(Low -- claim accuracy)** The Context section says the pre-fix exposure is "up to
   `DCS_MAX_BYTES` (16 MiB) of a hostile payload ... buffered in an image decoder". 16 MiB is the
   *payload* cap enforced at `crates/vt/src/parser/state.rs:325`; the decoder's own allocation is
   bounded separately by `MAX_PIXEL_BYTES` (`crates/vt/src/graphics/mod.rs:50`) at 4096 * 4096 * 4 =
   64 MiB. The pre-fix memory exposure is therefore understated, not overstated. Not quantified
   experimentally by this verification -- `SixelParser`'s fields are private to the `graphics`
   module and no accessor exists -- so this is read from the source, not measured.

9. **(Low -- record)** `platform_proof = 0` in the harness snippet, where every recent sibling row
   in `story` (`US-0096`, `US-0095`, `BUG-0057`, `BUG-0056`) carries `1`. Defensible for a change
   with no platform-specific behaviour, but it is a departure from the surrounding rows and is not
   explained. Also `created_at="2026-09-15"` is a bare date where sibling rows use a full timestamp
   (`2026-09-15T12:36:30`); the column has no CHECK, so it will insert, but the format is
   inconsistent.

10. **(Low -- unverified claim in production code)** The new doc comment at `dispatch.rs:1311-1312`
    asserts as fact that XTGETTCAP is what "tmux, neovim and kitty send at startup". The packet's
    own "Not verified" paragraph admits this was taken from the intake and not measured. It is a
    plausible claim, but it is now stated without hedge in a source comment. Either soften it or
    cite the intake.

## What could not be verified

- **Fuzzing.** `cargo fuzz` is not installed here and installing it was out of scope. Substituted by
  the corpus drift gate (section 4), the whole `oneterm-vt` suite with and without `vt-paranoid`,
  and the 1 MiB hostile-payload test.
- **Real clients.** No tmux, neovim or kitty session was driven against the built engine. The claim
  that those programs emit XTGETTCAP at startup is not measured here (finding 10). The tests pin the
  byte-level behaviour, not the client that produces the bytes.
- **Quantified pre-fix allocation.** Finding 8 is reasoned from the source, not measured; the
  `SixelParser` pixel buffer has no test-visible accessor.
- **`harness.db` row.** Nothing was written to the database, per the task's constraint. Only a
  read-only copy was inspected, to check the INSERT snippet's schema.
- **Acceptance of the packet.** This is a verification of the implementation against its own record,
  not an acceptance decision.
