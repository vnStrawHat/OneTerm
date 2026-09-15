# Work: OSC handling is the engine's, with a route table an embedder can extend and override

ID: US-0098
Intake: IN-0038
Created: 2026-09-15

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

- Change type: new capability (plus an existing-contract change to `Config` and `VtEvent`)
- Risk lane: high_risk
- Spec Intake: `IN-0038`

## Outcome

`OscClaims` becomes `OscRoutes`, a per-number table of `Builtin` / `BuiltinAndForward` / `Forward` /
`Drop`. The engine gains built-in handlers for OSC 7, 9 and 133 and emits typed events for them and
for OSC 22 and 50. `crates/terminal/src/osc.rs` loses its whole parsing half and keeps only policy.
OneTerm's OSC 20308 agent channel keeps working through three lines of configuration and no engine
change -- which is the packet's real proof.

## Scope

- [x] In scope: `crates/vt/src/terminal/osc.rs` (the table), `dispatch.rs` (the route lookup and six
  new arms), `events/vt_event.rs` (seven new variants, `Progress`, `ShellMark`),
  `crates/terminal/src/osc.rs` (delete the parsers), `backend/osc_router.rs` (route typed events),
  `handle.rs` (`adapter_config`), `docs/osc-sequences-checklist.md` (rewrite).
- [x] **Pulled in:** OSC 1 (icon name). The packet had left it to `US-0102`; the owner ruled on
  2026-09-15 that it lands here, because it is one arm beside the OSC 0/2 arm it shares its
  parsing with and splitting it across two packets would cost more than it saves. OSC 17 / 19,
  OSC 777 and OSC 1337 stay out.
- [x] Out of scope: moving `TerminalSecurityPolicy`, `encode_osc52`, `osc_color.rs`, `url_policy.rs`
  or anything in `osc_agent/`. Decision (f).
- [x] Out of scope: per-sub-code routing. Considered and rejected in
  [`low-level-design/osc-extension.md`](low-level-design/osc-extension.md).

## Acceptance

Every criterion below is a test a hostile verifier can run and read.

**The table**

- [x] All six rows of the `forward` / `suppress` / `has_builtin` truth table
  (`osc-extension.md`, "The routing table") are asserted, for a built-in number, a non-built-in
  number below 2048, and a number above 2048.
- [x] `route(0, Forward)` then `OSC 0;hello ST`: exactly one `VtEvent::Osc { code: 0 }`, zero
  `VtEvent::Title`, and the terminal's title is unchanged.
- [x] `route(0, BuiltinAndForward)` then the same input: exactly two events, `Title` **then** `Osc`,
  in that order.
- [x] `route(8, Drop)` then a hyperlink sequence: zero events, no hyperlink on the cell,
  `unhandled_sequences` incremented by exactly 1.
- [x] `route(n, Forward).large(n, true)` then a 3 MiB payload: one `VtEvent::Osc` with
  `truncated == false` and the whole payload readable. Without `large`, the same input yields
  `truncated == true` and at most `OSC_INLINE` bytes.
- [x] A table routing **every** number to `Forward` turns the engine into a pure parser: a title
  sequence changes no engine state and arrives raw.
- [x] `route(n, Builtin)` where `has_builtin(n)` is false, and `large(n, true)` where the route is
  `Drop`, each trip a debug assertion and are each inert in release.

**The migration**

- [x] **Two deliberate departures from the adapter's behaviour**, both found by the independent
  verification and both recorded in the CHANGELOG. (a) `OSC 9;4` reads its state and percentage as
  `u32` and clamps the percentage, where the adapter read both as `u8`: `9;4;1;1000` was `Set(0)`
  because the parse failed before the clamp could run, and a state above 255 silently became
  `Remove`, clearing a progress bar nobody asked to clear. The contract already said "`st` in
  `0..=4`, `pr` clamped to 100", so the code was made to match the contract rather than the other
  way round. (b) A non-UTF-8 `OSC 7` URL is dropped and counted, which is what the adapter did and
  what the first implementation accidentally changed to a lossy report. Percent escapes that decode
  to non-UTF-8 bytes are still reported leniently, as they always were: by then the sequence has
  been accepted.
- [x] Every input literal in `crates/terminal/src/osc.rs`'s current `mod tests` appears verbatim in
  the new `crates/vt` test module, with the same expectation. A verifier diffs
  `git show main:crates/terminal/src/osc.rs` against the new tests and checks the list: OSC 7
  `file:///home/marc`, `file://host/path`, a bare path, `%20`, `file:///C:/Users`; OSC 9;4 states 0
  to 4 and the percent clamp; OSC 9 with an embedded `;`; OSC 133 `A`, `B`, `C`, `D`, `D;0`,
  `D;not-a-number`.
- [x] `crates/terminal/src/osc.rs` no longer contains `parse_osc`, `OscPayload`, `parse_cwd_url`,
  `percent_decode`, `strip_windows_drive_slash` or `Osc133Kind`.
- [~] `crates/terminal` production line count drops by at least **350**. **Not met as written**,
  and the number is below. `git diff --numstat` over the five production files touched:
  600 deletions against 280 insertions, a net **-320** counting the test modules and **-151**
  counting production lines only (`crates/terminal/src`, `*_tests.rs` files and every
  `#[cfg(test)] mod tests` tail excluded: 6683 -> 6532). `crates/terminal/src/osc.rs` alone is
  458 deletions against 113 insertions. The estimate assumed the adapter would shed the parsers
  and gain nothing; it gained the agent-channel dispatch `parse_osc` used to host
  (`parse_agent_osc`, `AgentOsc`, `is_legacy_agent_notification`) and four typed arms in
  `osc_router.rs` in place of one raw one. Every symbol the criterion below names is gone, which
  is what the line count was standing in for.
- [~] `adapter_config` in `handle.rs` contains **no** reference to OSC 7, 9 or 133. **True for
  7 and 133, and not for 9.** There is no numeric `7`, `9` or `133` literal, but `adapter_config`
  routes `LEGACY_AGENT_OSC`, which *is* `9`, and its doc comment explains OSC 9 at length. The
  owner's ruling on the `9;7` alias made that unavoidable: the alias shares a built-in's number,
  routing is per number, so wrapping OSC 9 is the only way to keep the alias outside the engine.
  The criterion holds in the form that was meant — no OSC 7 or OSC 133 knowledge, and the agent
  channel named by a constant rather than a number.

**The worked example**

- [x] The whole OSC 20308 agent channel -- sub-code match, base64, schema validation, `seq` dedup,
  truncation drop, unknown-sub-code counter, support reply -- is unchanged in `crates/terminal/src/
  osc_agent/` and `backend/osc_router.rs`, and `cargo test -p oneterm-terminal` passes its existing
  suite untouched.
- [x] `grep -rn '20308\|AGENT_OSC' crates/vt/` returns **0** lines.

**No regression**

- [x] `cargo test --workspace`, `cargo test -p oneterm-vt --features vt-paranoid`, and the 46 frozen
  parity corpus recordings all pass.
- [x] The extended proptest (arbitrary bytes plus an arbitrary `OscRoutes`) never panics and keeps
  `FeedStats` counters monotone within a batch.
- [~] A one-hour local fuzz run of `crates/vt/fuzz/fuzz_targets/parser.rs`, with the route table
  derived from the input, finds no crash. **Not run**: `cargo-fuzz` needs nightly and libFuzzer,
  which are unusable on `x86_64-pc-windows-msvc`, and the crate's own `fuzz/Cargo.toml` says so.
  The target *was* extended (its `Sink` now derives an `OscRoutes` from the input's first bytes)
  so a Linux run gets the coverage. Standing in for it: the in-tree
  `arbitrary_bytes_and_an_arbitrary_table_never_panic`, 64 rounds of biased random bytes against
  64 randomly built tables, and the existing million-byte parser proptest.
- [~] Manual Windows walk: the SFTP panel follows the shell's cwd (OSC 7); an agent event reaches
  the Agent Panel (OSC 20308); `printf '\033]9;4;1;40\a'` moves the taskbar progress; a prompt mark
  is recorded (OSC 133); a title change still works (OSC 0). **Not run** -- the owner runs their
  own session inside OneTerm and this worktree must not start or stop the app. Each of the five
  steps has a test that drives the same bytes: `osc7_cwd_forwards_and_caches` and
  `osc133_prompt_forwards_and_counts` now feed a real engine through the real `adapter_config`,
  `agent_status_forwards_under_both_encodings` covers both agent spellings, and
  `osc_9_4_reports_every_progress_state` and `osc_0_and_2_set_the_title` cover the other two.
  E2E therefore stays **unproven**.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` -- the "OSC
  registration" section this packet replaces. **Stale after this packet.**
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` -- the events-are-values
  rule and the `EventBatch` arena. Confirmed still true; the new variants follow it.
- `docs/osc-sequences-checklist.md` -- its methodology section names the four adapter files as the
  source of truth for OSC. **Stale after this packet**; three of the four no longer parse anything.
- `docs/osc-agent-status.md` -- sections 3.1 (the `9;7` deprecation), 3.2 (the support query), 3.4
  (payload size) and 3.5 (malformed payloads). Its wire contract must not change.
- `docs/terminal-backend.md` -- the `OscRouter` description.
- `crates/terminal/src/security_policy.rs` -- read to confirm every policy call survives the move in
  the same order.

### Documentation Action

**Update required**: `dispatch-and-modes.md` (the OSC registration section becomes a pointer to
`osc-extension.md`), `docs/osc-sequences-checklist.md` (rewritten: which OSC the **engine** handles,
which the adapter routes, and the new `OscRoute` column), `docs/terminal-backend.md` (the
`OscRouter` paragraph). `docs/osc-agent-status.md` does not change: Open Decision 1 keeps the
`9;7` alias, handled in the adapter through the engine's `BuiltinAndForward` route.

Reason: three current documents name adapter files as the place OSC is parsed, and after this packet
that is false in all three.

### Reconciliation

Changed:

1. `docs/osc-sequences-checklist.md` -- the methodology section now names the engine's dispatch and
   the route table as the source of truth instead of the four adapter files; a **Route** column
   legend; the OSC 1, 22 and 50 rows moved from ❌ to ◐; and the OSC 7, 9 / 20308 / 9;7 and 133
   notes rewritten around the routes.
2. `docs/terminal-backend.md` -- the `OscRouter` row: the typed `Cwd` / `Notification` / `Progress`
   / `ShellMark` arms and what is left for `VtEvent::Osc`; the three events nothing above the seam
   speaks yet; and the OSC 7 sentence in the prompt-integration section.
3. `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` -- the "OSC
   registration" section is a pointer to `osc-extension.md` and `DEC-0017`, keeping the four rules
   that survived the rename verbatim; the OSC 22 table row.
4. `crates/vt/CHANGELOG.md` (Added and Changed, including the breaking rename and the two
   behaviour corrections), `crates/vt/README.md` (the "Extending it: OSC" section, with the
   four-route table), `crates/vt/public-api.txt` (regenerated).
5. After the independent verification: `DEC-0017` (the consequence boxes reconciled, the `Config`
   `PartialEq` claim and the lookup cost corrected),
   `low-level-design/osc-extension.md` (the same two corrections, the allocation promise restated
   with its two documented exceptions, what the table's value and lifetime mean, the `9;4` clamp,
   and what a wrapped number costs on a ceiling-sized payload),
   `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` (two code blocks that
   still showed `osc_claims` and `OscClaims` — listed as "reviewed, no change needed" in the first
   pass, which was wrong), and `IN-0038.md` (this packet's line-count estimate, against the
   measurement).

Reviewed, no change needed:

- `docs/osc-agent-status.md` -- §3.1 (the `9;7` alias is still "parsed identically, counted, and
  logged once per session"), §3.2 (the support query is still answered by the adapter, on the
  transport, under the engine guard), §3.4 (the 8 KiB cap is still `osc_agent`'s and is still
  bought with a payload ceiling) and §3.5 (a malformed payload is still dropped silently). The wire
  contract did not move, only which side of the seam parses the rest of OSC 9.
- `crates/terminal/src/security_policy.rs` -- read to confirm every policy call survives in the same
  order: `sanitize_cwd`, `sanitize_notification` + `NotificationRateLimiter::allow`,
  `sanitize_title`, `validate_clipboard_write`, `allow_clipboard_read`. All five are still in
  `osc_router.rs`, called from the same place in the same batch order. Not edited.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` -- the events-are-values
  rule and the `EventBatch` arena. Still true; the seven new variants are spans into the same arena
  and no callback was added. **But two of its code blocks named `osc_claims` and `OscClaims`**, which
  this packet renamed; the independent verification caught that and they are now updated. Listing
  this file as "no change needed" was a review miss, not a judgement.

## Context

The key design is [`low-level-design/osc-extension.md`](low-level-design/osc-extension.md) and must
be read before implementation: it carries the `OscRoute` semantics, the bit layout, the dispatch
path, the exact signatures, the per-parser migration table and the `OSC 9;7` collision analysis.

Sites this packet touches, with line numbers on `main` @ `36977ca`:

| Site | What |
| --- | --- |
| `crates/vt/src/terminal/osc.rs` | the whole file: `OscClaims` -> `OscRoutes` |
| `crates/vt/src/terminal/dispatch.rs:1207-1305` | `Handler::osc`, the built-in match, `forward_osc` |
| `crates/vt/src/events/vt_event.rs:38` | `VtEvent`, seven new variants |
| `crates/terminal/src/osc.rs:84-179` | `parse_osc`, deleted |
| `crates/terminal/src/backend/osc_router.rs:180-231` | the `VtEvent::Osc` arm, rewritten |
| `crates/terminal/src/handle.rs:184-206` | `adapter_config` |

## Plan

- [x] `OscRoutes` and `OscRoute` in `crates/vt/src/terminal/osc.rs`, with the truth-table tests,
  before anything else uses them.
- [x] Rewire `Handler::osc` to the route lookup; existing arms keep their behaviour. `cargo test
  --workspace` must be green here, with `crates/terminal` still doing its own parsing, before any
  parser moves. This is the checkpoint that separates "the table works" from "the migration works".
- [x] New `VtEvent` variants, `Progress`, `ShellMark`.
- [x] Move the OSC 7 parser (URL, percent decoding, Windows drive slash), with its tests.
- [x] Move the OSC 9 parsers (progress, notification), with their tests.
- [x] Move the OSC 133 parser, keeping the existing template-marking behaviour, with its tests.
- [x] Emit `VtEvent::Pointer` from the OSC 22 arm and `VtEvent::CursorStyleChanged` from OSC 50.
- [x] Rewrite `OscRouter::handle`'s arms; delete `OscPayload` and `parse_osc`.
- [x] Rewrite `adapter_config`: `route(20308, Forward).large(20308, true)` and
  `route(9, BuiltinAndForward).large(9, true)`; the adapter discards the `Notification` whose
  forwarded first parameter is `7` and routes that payload to `osc_agent` (Open Decision 1 ruled:
  the alias stays, handled outside the engine).
- [x] Extend the proptest and the fuzz target with a route table.
- [x] Rewrite `docs/osc-sequences-checklist.md`; update the two design documents.
- [x] CHANGELOG lines under `Unreleased`.

## Decisions

- [`DEC-0017`](../../decisions/DEC-0017-osc-routing-table-not-handler-registry.md) -- OSC extension
  is a routing table, not a handler registry. **Accepted by the owner on 2026-09-15**, so this
  packet's "must not start before it is accepted" precondition is satisfied. Its two "benefit to
  confirm" boxes were reconciled after the verification: the `20308` one is confirmed, the
  350-line one is not (see the line-count criterion above). Two statements in the record were also
  wrong and are corrected there and in `osc-extension.md`: `Config` is `Clone + Debug` and has never
  been `PartialEq` (the handler-registry rejection rests on the no-callback invariant, which is
  unaffected), and the lookup is three bit tests rather than two.

## Verification Plan

- Focused: the table truth-table suite; one test per moved parser, using the adapter's own input
  literals; the override, wrap, suppress and extend tests.
- Unit: `cargo test -p oneterm-vt`, `cargo test -p oneterm-terminal`, `cargo test --workspace`.
- Integration: `cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh`.
- Property and fuzz: the extended proptest in CI; a one-hour fuzz run recorded, not gated.
- Platform: `pwsh scripts/ci-local.ps1`; the parity corpus replay.
- E2E: the manual Windows walk in Acceptance.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Branch `feat/vt-osc-routes`, three commits on `0558fa2`:

| Commit | What |
| --- | --- |
| `204e931` | `feat(vt)`: the routing table, the six built-in arms, the seven events |
| `7d034c6` | `refactor(terminal)`: the adapter parsers deleted, the policy kept |
| `48e01fd` | `test(vt)`: `31337` replaces `20308` as the private-number example |

**The table.** `the_routing_table_answers_every_row` asserts all six truth-table rows for OSC 7
(built-in), 633 (below the bitmap, no built-in) and 31337 (above the bitmap), plus `overrides()`
and `has_builtin`. `every_route_behaves_the_way_the_table_says` feeds one sequence per route and
asserts the observable result, including the `BuiltinAndForward` order contract (`Title` then
`Osc`, exactly two events). `a_table_that_forwards_everything_makes_the_engine_a_parser` is the
escape hatch. `a_large_ceiling_is_opt_in_per_number` feeds 3 MiB with and without `large`.
`routing_a_number_with_no_builtin_to_builtin_asserts` and
`buying_a_ceiling_for_a_dropped_number_asserts` are the two debug assertions.

**The migration.** Every input literal from `git show 0558fa2:crates/terminal/src/osc.rs`'s `mod
tests` is in `crates/vt/src/terminal/terminal_tests.rs` with the same expectation:
`file:///home/marc`, `file://host/var/log`, `/tmp/x`, `file://host/home/me/My%20Docs`,
`file:///C:/Users/me/src`, `file:///C:`, `file:///tmp/100%25/x%zz`, `file:///home/%C3%A9t%C3%A9`,
`/Cx/y` (`osc_7_*`); `9;4` states 0-4, the 250 clamp and the unknown state 9
(`osc_9_4_reports_every_progress_state`); `Build finished`, `done: 3 tests; 0 failed` and
`71 bottles` (`osc_9_reports_a_notification_with_its_semicolons_rejoined`); `133;A/B/C/D`, `D;0`,
`D;127`, `D;not-a-number`, `133;X`, `133;Z;foo` (`osc_133_reports_every_marker`). The three that
tested the adapter's *policy* rather than its parsing stayed in `crates/terminal/src/osc.rs` as
`the_security_policy_caps_what_the_engine_reports`.

**The worked example.** `an_embedder_private_osc_number_and_a_wrapped_builtin_both_work` builds the
same table `adapter_config` builds and drives both halves. `grep -rn '20308\|AGENT_OSC' crates/vt/`
returns **0 lines**; `adapter_config` is three calls and names no OSC 7, 9 or 133 literal. Every
`osc_agent/` file and every agent test in `backend_tests.rs` is untouched, and
`cargo test -p oneterm-terminal` passes 276 tests.

**Line counts.** `crates/vt/src` +527 production lines, `crates/terminal/src` -151 (the criterion
above records why that is short of the estimate, and what was measured instead).

**Independent verification.** [`evidence/US-0098-verify.md`](evidence/US-0098-verify.md),
**PASS-WITH-NOTES** at `82680ce`, no functional defect: 20 behaviour claims attacked with tests
written against the public API by somebody who had not read the in-crate module, and all 20 upheld
on the first run. Its 27 tests are adopted as `crates/vt/tests/us0098_verify.rs` and its 3 adapter
tests as the `legacy_alias_drops_the_notification_and_dispatches_the_payload`,
`the_wrap_route_does_not_break_the_rest_of_osc_9` and `every_policy_call_still_happens` cases in
`backend_tests.rs`. The 16 notes are closed below.

**Gaps.**

- The manual Windows walk and the one-hour fuzz run were not performed; both are recorded against
  their Acceptance boxes with what stands in for them. **E2E is unproven.**
- OSC 1 came *in* (owner ruling). OSC 17 / 19, 777 and 1337 remain unhandled, deliberately.
- The `9;7` alias **survived**, per the owner's ruling on Open Decision 1 (2026-09-15): it is
  OneTerm's custom spelling and is handled outside the engine. `adapter_config` routes OSC 9 as
  `BuiltinAndForward`, and `is_legacy_agent_notification` in `crates/terminal/src/osc.rs` discards
  the notification whose forwarded first parameter is `7`. The engine's OSC 9 arm knows nothing
  about `7`.
- **A hostile ceiling-sized `OSC 9` costs more than it did.** `adapter_config` buys OSC 9 the 8 MiB
  tier so the legacy alias's payloads are not cut, and a `BuiltinAndForward` number pays for both
  outcomes: the notification arm's body *and* the forwarded parameters land in the arena. One
  hostile 8 MiB `OSC 9` therefore costs the parser spill plus two arena copies where it used to cost
  the spill plus one. Bounded and verified
  (`v_a_hostile_8_mib_osc_9_under_the_shipped_table_is_bounded`), and it disappears with the alias;
  the general lesson, recorded in the LLD, is to keep `BuiltinAndForward` rare.
- **`crates/terminal` lost compile-time exhaustiveness on `VtEvent`.** `#[non_exhaustive]` forces a
  wildcard across the crate boundary, so `OscRouter::handle` ends in `_ => {}` and `progress_style`
  and `set_progress` do the same for `Progress`. A future engine event that OneTerm *should* handle
  will be ignored instead of failing the build. This is the documented price of the
  `#[non_exhaustive]` follow-up in `DEC-0017`; the three sites each chose a sensible default, and
  the mitigation is the CHANGELOG, not the compiler.
- Rebased onto `072560a` (`US-0100` merged, `DEC-0017` accepted). `US-0099` and `US-0104` may land
  before this; this packet's `crates/vt/src/lib.rs` edit is two lines, both in existing `pub use`
  lists.

## Verification Notes Closed

Every note in [`evidence/US-0098-verify.md`](evidence/US-0098-verify.md), and what was done with it.

| # | Note | Outcome |
| --- | --- | --- |
| 1 | `DEC-0017` still `Proposed`; its consequence boxes never reconciled | **Closed.** The owner accepted it on 2026-09-15. The `20308` benefit is ticked as confirmed; the 350-line benefit is marked partly met with the measured -151 and why the estimate was wrong. |
| 2 | Both contracts say `Config` is `PartialEq`. It is not, and never was | **Closed.** Corrected in `DEC-0017` and `osc-extension.md`. The handler-registry rejection rests on the no-callback invariant, which is untouched; the conclusion does not change. `OscRoutes` is what is `Clone + PartialEq`. |
| 3 | "Two bit tests" understates the lookup: `has_builtin` was a linear scan | **Closed, by fixing the code.** `has_builtin` reads a `BUILTIN_BITS` bitmap built from `OscRoutes::BUILTIN` at compile time. The prose now says three bit tests, plus a fourth for the payload ceiling. |
| 4 | The new arms allocate per sequence, contradicting `osc-extension.md` | **Closed, by fixing the code.** `EventBatch` gains `mark`/`extend`/`finish_trimmed`/`finish_lossy`; the arms assemble their payload in the arena. `OSC 7` went from up to five allocations to none. Two exceptions are documented in the LLD: the title's owned copy (the title stack outlives the batch) and a UTF-8 repair for a percent escape that decodes to invalid bytes. |
| 5a | `large()` asserts on call ordering, not intent | **Closed as documented, not changed.** The rustdoc and the LLD now say to route before buying the ceiling, and why. Relaxing the assertion would mean re-checking every ceiling whenever any route changes, for a builder whose chain already makes the order natural. |
| 5b | Routes cannot be changed on a live `Terminal`, and nothing says so | **Closed as documented.** `OscRoutes`'s rustdoc and the LLD say the table is read at construction and is not live, and why: a route that changed under a half-parsed sequence would be a race the batch-of-values contract has no way to report. |
| 6 | `OscRoutes: PartialEq` was structural, not semantic | **Closed, by fixing the code.** `route()` stores the bits of the route the number actually gets, so a redundant `Drop` is a no-op and a spilled number routed back to its default leaves the spill. The verifier's test is adopted, inverted to assert equality. |
| 7 | The fuzz target's `Sink` never reaches the mechanism | **Closed, by fixing the target.** It builds a real `Terminal` with the input-derived table and drains the batch, so the route lookup, the six arms and `forward_osc` are all in the fuzzed path. Still Linux-only and still not a gate, but it does type-check on stable now (`cargo check` inside `crates/vt/fuzz`), so it cannot rot unnoticed between runs. |
| 8 | The deleted allocation guard was not replaced | **Closed.** `the_builtin_arms_do_not_grow_the_batch_once_warm` feeds eight OSC kinds 1024 times and asserts all three batch capacities are flat. |
| 9 | The zero-padded `007` literal lost its test | **Closed.** `a_zero_padded_osc_number_reaches_the_same_builtin` covers `007;file:///tmp` and `0133;A`. |
| 10 | Non-UTF-8 `OSC 7` changed behaviour and the CHANGELOG denied it | **Closed, by restoring the old behaviour.** The URL must be valid UTF-8 or the sequence is dropped and counted, as the adapter did. |
| 11 | The wrap route roughly doubles what a hostile 8 MiB `OSC 9` costs | **Closed as recorded.** Noted in the LLD's hostile-input section and in Gaps below. Bounded and verified; it is the price of the owner's ruling on the alias. |
| 12 | The "no reference to OSC 7, 9 or 133" criterion is over-ticked | **Closed.** The criterion is re-marked partial and says that `adapter_config` routes `LEGACY_AGENT_OSC`, which is 9. |
| 13 | `events-and-api.md` and `IN-0038.md` were stale | **Closed.** Both updated; the Reconciliation section records that listing `events-and-api.md` as "no change needed" was a miss. |
| 14 | The arm table's "clamped to 100" was not what the code did | **Closed, by fixing the code.** Both `9;4` fields are read as `u32` and the percentage clamps. The LLD's table records what the adapter did and why it changed. |
| 15 | `crates/terminal` lost compile-time exhaustiveness on `VtEvent` | **Closed as recorded.** Noted in Gaps below; it is the documented price of `#[non_exhaustive]` and the three wildcard arms each chose a defensible default. |
| 16 | An unrelated flake in `render_bench` | **Acknowledged, not acted on.** Pre-existing and timing-sensitive. Seen once here too, in `handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`, under a parallel build; green in isolation and in `ci-local -Full`. |

## Harness Row

`harness.db` was **not** written by this task: the task forbids editing the database. The schema is
`story(id, title, created_at, risk_lane, contract_doc, packet_doc, status, unit_proof,
integration_proof, e2e_proof, platform_proof, evidence, verify_command, last_verified_at,
last_verified_result, notes, intake_id)`, with the four `*_proof` columns as `0`/`1`.

```python
#!/usr/bin/env python3
"""Insert the US-0098 story row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    id="US-0098",
    title=(
        "OSC handling is the engine's, with a route table an embedder can extend "
        "and override"
    ),
    created_at="2026-09-15T00:00:00",
    risk_lane="high_risk",
    contract_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/"
        "osc-extension.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/"
        "US-0098-osc-routes-and-builtins.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=1,
    evidence=(
        "OscClaims -> OscRoutes (four routes, two bitmaps + spill); six new "
        "built-in arms (OSC 1, 7, 9, 22, 50, 133) and seven new VtEvent "
        "variants; crates/terminal loses parse_osc, OscPayload, Osc133Kind, "
        "TerminalProgress, parse_cwd_url, percent_decode and "
        "strip_windows_drive_slash. grep -rn '20308|AGENT_OSC' crates/vt/ = 0 "
        "lines. 384 vt tests, 276 terminal tests, workspace green, "
        "ci-local.ps1 -Full pass. Every OSC 7/9/133 input literal from the "
        "adapter's old test module moved with its expectation."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "e2e_proof=0: the manual Windows walk was not run (the owner runs their "
        "own session inside OneTerm), and the one-hour cargo-fuzz run needs "
        "nightly libFuzzer, unusable on windows-msvc; the fuzz target was still "
        "extended with an input-derived route table. OSC 1 was pulled in from "
        "US-0102 by owner ruling. The OSC 9;7 alias survived by owner ruling and "
        "is handled outside the engine through a BuiltinAndForward route. "
        "Branched from 0558fa2; expects a rebase onto main."
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

## Handoff

The checkpoint in Plan step 2 -- table landed, nothing moved yet, workspace green -- is the natural
session boundary and the safest place to stop.
