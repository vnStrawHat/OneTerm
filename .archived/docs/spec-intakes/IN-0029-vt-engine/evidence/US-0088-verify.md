# US-0088 — independent verification

Verifier: a second agent; did not write the code under review.
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a5a7ea0c82fee4626`
Under review: `git diff d35460f..3e822ee` (804e43a, 842eac9, ebab9d2, 3e822ee on `feat/vt-engine`).
Date: 2026-09-13. No GUI was launched; no `oneterm.exe` was enumerated, driven or stopped.

## Verdict

**PASS-WITH-NOTES**, with two **major** defects that should be fixed before merge.

Everything the packet claims about the *parse and route* path is true in code: both
spellings reach one `parse_agent_status`, one `seq` watermark, one silent-drop policy;
the support reply's terminator mirrors the query's; reserved sub-codes are ignored and
counted; nothing is echoed into the grid; the engine needed only a claim. `ci-local` is
green at exactly the totals the packet reports.

What does not hold is the part of the **public contract** that lives outside
`parse_osc`: the §3.2 DA1 detection idiom is defeated by the router's reply ordering,
and the §3.4 8 KiB cap is unreachable because the engine truncates the claim at 2 KiB.
Both are one-line fixes; both are shipped as a wire contract this document asks third
parties to implement, which is why they are major rather than minor.

## Disk (pre-build check)

```
Name FreeGB
---- ------
C     16.60
D     34.50   (36.0 after the gate run)
```

D: above the 20 GB threshold, so builds were run.

---

## Defects

### 1. BLOCKER-adjacent / MAJOR — the DA1 reply overtakes the support reply, so the spec's own detection idiom reports "unsupported"

`docs/osc-agent-status.md` § 3.2 is prescriptive:

> ```
> ESC ] 20308 ; 0 ST   ESC [ c
> ```
> Every terminal answers `CSI c`. **If the DA1 reply arrives with no `20308` reply
> before it, the terminal does not implement the protocol** — no timeout to choose, no
> guess.

`OscRouter::drain` (`crates/terminal/src/backend/osc_router.rs:138-148`) writes **every
`VtEvent::Reply` in a first pass** and routes everything else in a second, and the
support reply is produced in that second pass
(`crates/terminal/src/backend/osc_router.rs:347`). So when the two sequences arrive in
one `feed` — which is what a single `write()` of the documented byte string produces —
the order on the transport is inverted relative to the wire order.

Repro (new test `verify_the_support_reply_precedes_the_da1_reply_in_one_batch`):

```
feed: \x1b]20308;0\x07\x1b[c
transport: "\x1b[?62;4;22c\x1b]20308;0;1;OneTerm;0.5.2\x07"
           ^^^^^^^^^^^^^ DA1 first
panicked: spec 3.2: the 20308 reply must precede DA1
```

An agent that follows the documented idiom therefore concludes OneTerm does **not**
support the protocol and falls back — on the very terminal that implements it. The
`agent_support_reply` doc comment (`crates/terminal/src/osc.rs:203-207`) explicitly
cites this idiom as the reason the terminator is mirrored, so the code states the
property it does not deliver.

Note the R-37 claim in the packet is *separately* true — the reply does leave under the
engine guard before the pump yields, verified by
`verify_the_support_reply_leaves_from_the_alt_screen_inside_a_sync_block` (alt screen +
open mode-2026 block, the reply is on the transport before the guard drops). R-37 is
about *latency*; § 3.2 is about *order*, and only the first was checked.

Fix shape: either emit the answer as a `VtEvent::Reply`-equivalent so it lands in the
first pass, or drain in one pass. The cheapest correct change is to handle
`OscPayload::AgentSupportQuery` in the first pass alongside `VtEvent::Reply`.

### 2. MAJOR — the documented 8 KiB cap is unreachable; the real ceiling is 2040 base64 bytes, and everything above it is dropped silently

`crates/terminal/src/handle.rs:185` claims the agent channel with `claim`, not
`claim_large`:

```rust
claims.claim(7).claim(9).claim(133).claim(AGENT_OSC);
claims.claim_large(52);
```

`OscClaims::claim` leaves the payload bounded by `OSC_INLINE = 2048`
(`crates/vt/src/parser/osc.rs:11`, `:106-108`), which covers the **whole** OSC payload
including the `20308;1;` prefix. Anything larger is truncated by the parser (the
`truncated` flag is then discarded by the router's `VtEvent::Osc { .. }` pattern at
`crates/terminal/src/backend/osc_router.rs:183-188`), so the base64 is cut mid-stream
and `parse_agent_status` drops it silently.

Measured ceiling (new test `verify_the_documented_cap_is_reachable_through_the_engine`):

```
base64 len 2040 -> AgentStatus delivered
base64 len 2044 -> nothing
base64 len 4096 -> nothing   <-- FAILED
panicked: a 4 KiB base64 payload is inside the documented 8 KiB cap (and inside the
'< 4 KiB worst case' spec 3.4 calls legitimate), but AGENT_OSC is claimed with `claim`,
not `claim_large`, so the engine truncates it at OSC_INLINE = 2048 and
`parse_agent_status` then drops it silently
```

2040 base64 bytes is ~1530 bytes of raw JSON — barely above the "< 1 KiB typical" the
spec quotes and far below the "< 4 KiB worst case" it calls legitimate. A large `model`
or `approval` event vanishes with no user-visible signal and no counter.

The packet's cap test (`agent_status_forwards_under_both_encodings` and friends) only
exercises `parse_agent_status` through a hand-built `EventBatch`, so the engine's
truncation is invisible to the whole existing suite. My
`verify_the_8kib_cap_boundary_under_both_encodings` confirms the *parser* half is
correct (8192 accepted, 8196 rejected, identical under both spellings, and an oversized
payload is **not** miscounted as an unknown sub-code) — the defect is purely the claim.

Pre-existing on `9;7` (also `claim`), so US-0088 inherited rather than introduced it.
But this is the packet that (a) re-published the 8 KiB cap as § 3.4 of a public
contract and (b) wrote the new claim line; `claim_large(AGENT_OSC)` is the one-word fix
and belongs here. Acceptance row "same 8 KiB cap as 9;7" is technically satisfied and
substantively not.

### 3. MAJOR (coverage) — no test covers the production claim; removing it breaks nothing

Tamper test, per instruction 3. `crates/terminal/src/handle.rs:185` changed to

```rust
claims.claim(7).claim(9).claim(133);
let _ = AGENT_OSC;
```

Result on the **pristine** implementer tree:

```
cargo test -p oneterm-terminal -p oneterm-vt
test result: ok. 255 passed; 0 failed; ...
test result: ok.   5 passed; 0 failed; ...
test result: ok. 364 passed; 0 failed; ...
test result: ok.   5 passed; 0 failed; ...
test result: ok.   5 passed; 0 failed; ...
```

**Zero failures.** No existing test caught the removal of the one line the packet calls
"the whole engine-side story". Every existing agent test either builds an `EventBatch`
by hand (`backend_tests.rs`, bypassing the engine entirely) or builds its own
`claiming(20, 4, &[20308])` config in `crates/vt` — neither touches `adapter_config`.
`crates/vt/src/terminal/terminal_tests.rs:1333` proves the *table* works, not that
OneTerm *uses* it.

Two of my new end-to-end tests do catch it (same tamper, my tests restored):

```
verify_support_reply_terminator_fidelity_end_to_end ... FAILED
verify_the_support_reply_leaves_from_the_alt_screen_inside_a_sync_block ... FAILED
verify_the_documented_cap_is_reachable_through_the_engine ... FAILED
verify_the_support_reply_precedes_the_da1_reply_in_one_batch ... FAILED
```

`handle.rs` was restored to `claim(AGENT_OSC)` immediately afterwards and the gate was
run on the restored tree.

### 4. MINOR — `docs/agent-panel-display.md` still documents OSC 9;7, and now contradicts the UI string this packet changed

`crates/agent-ui/src/view.rs:509` was changed to `"Agents that emit OSC 20308 appear
here."`, but `docs/agent-panel-display.md:173` still quotes the **old** copy
`Agents that emit OSC 9;7 appear here.`, plus five more 9;7 references at lines 4, 38,
51, 66, 318. `docs/README.md` marks that file **current** and it is the owning doc for
the panel. It appears neither in the packet's Owning Docs Reviewed, nor in the
Documentation Action, nor in the "no change, with reason" list.

### 5. MINOR — `docs/agents/structure.md:120` still says "Folded OSC 9;7 agent card model"

`AGENTS.md` § 1 makes `structure.md` required reading for every agent session. Not in
the packet's touch list.

### 6. MINOR — the intake's own HLD names a test that no longer exists

`docs/spec-intakes/IN-0029-vt-engine/high-level-design.md:60` (row P20, Evidence column)
cites `dispatch::tests::osc_9_7_reaches_the_embedder_through_a_claim`. This packet
renamed it to `osc_9_7_still_reaches_the_embedder_during_the_alias_release`
(`crates/vt/src/terminal/terminal_tests.rs:1370`) and split out
`osc_20308_reaches_the_embedder_through_a_claim` (`:1333`). The HLD row now points at
nothing. (By contrast `low-level-design/dispatch-and-modes.md:571-574` already names
both new tests correctly — the design owner wrote those ahead.)

### 7. MINOR — `DEC-0014` still carries the open follow-up this packet closes

`docs/decisions/DEC-0014-oneterm-owns-its-vt-engine.md:149`:
`- [ ] Follow-up: OSC 9;7 collides with ConEmu's "run some process with arguments" sub-code`.
US-0088 is that follow-up; the box is still unticked and there is no pointer to US-0088.

### 8. MINOR (test quality) — "logged once per session" is asserted by calling the counter, not by exercising the router

`crates/terminal/src/backend/backend_tests.rs:401-404` (pristine) proves the once-per-
session property with

```rust
assert_eq!(f.state.count_legacy_agent_osc(), 1, "the first announces");
assert_eq!(f.state.count_legacy_agent_osc(), 2, "the rest only count");
```

i.e. by driving the counter directly, not by routing alias events and observing that
the router logs once. The counting half *is* covered by the routed events later in the
same test; the "logged once" half is not covered at all. Low value to fix (it is a
`debug` log), recorded for completeness.

### 9. NIT — the claim is numeric, the dispatch is a string match

`crates/terminal/src/osc.rs:102` matches `params[0]` against the string
`AGENT_OSC_PARAM`, while the claim and `VtEvent::Osc { code }` are `u32`. A zero-padded
number, which xterm-derived parsers accept, is claimed and forwarded by the engine and
then dropped by the string arm:

```
feed: \x1b]020308;0\x07  -> nothing written
verify_a_zero_padded_osc_number_reaches_the_same_handler ... FAILED
```

Pre-existing and identical for OSC 7 / 9 / 133, so not introduced here; the packet's
`the_agent_osc_number_and_its_wire_spelling_agree` test does pin the two constants
together, which bounds the risk. Reported only because US-0088 adds a second source of
truth for the same number.

---

## What was verified and holds

| Claim | How | Result |
|---|---|---|
| `20308;1` and `9;7` reach one `parse_agent_status` | `crates/terminal/src/osc.rs:102-127` → `parse_agent_status_param` (`:180`) | true in code, single call site each |
| Engine change is a claim only | `git diff` of `crates/vt/` is doc comment + tests; `handle.rs:185` | true |
| 20308 exercises the overflow list | `OscClaims::set`, `BITMAP_BITS = 2048` (`crates/vt/src/terminal/osc.rs:17`) | true |
| Reply terminator mirrors the query (BEL and ST), end to end | new `verify_support_reply_terminator_fidelity_end_to_end` | PASS |
| Reply written exactly once per query | same test, two queries → two replies, byte-exact | PASS |
| Reply leaves under the engine guard, alt screen + mode 2026 | new `verify_..._alt_screen_inside_a_sync_block` | PASS |
| Reply is not echoed into the grid; no UI event but the repaint | new `verify_the_support_reply_is_not_echoed_into_the_grid` | PASS |
| 8 KiB cap boundary, both encodings (8192 in, 8196 out) | new `verify_the_8kib_cap_boundary_under_both_encodings` | PASS at the parser; see defect 2 for the engine |
| Same `seq` on 9;7 then 20308;1 **and reversed** is deduplicated | new `verify_the_seq_watermark_is_shared_in_both_directions` | PASS |
| `20308;2`, `20308;999`, `20308;`, `20308`, `20308;10`, `20308;01` → no event, counted | new `verify_the_dead_shapes_...` | PASS, counter +1 each |
| `20308;1` with no / empty / malformed / non-JSON / wrong-`v` payload → no event, **not** counted as unknown sub-code | same test | PASS |
| Malformed base64 on the alias behaves identically and is still counted as legacy | same test | PASS |
| `9;4` progress and `9;<msg>` notifications still route the old way | `the_alias_does_not_swallow_the_rest_of_osc9` (implementer's) | PASS |
| `docs/osc-agent-status.md` § numbering is internally consistent after the renumber | script over headings vs `§x.y` references | **0 dangling**: refs `{2.1,2.2,3.1-3.5,4,4.1,4.2,4.2.2,4.3,5,5.1,5.3,7}` all resolve |
| Checklist Group G matches the code | `docs/osc-sequences-checklist.md:153-175` vs `osc.rs`/`osc_router.rs` | matches, including "ignored and counted" |
| Commit trailers | see below | all four correct |

## Commit trailers

All four commits end with exactly the two required lines — **no deviation**:

```
804e43a docs(vt): the US-0088 work packet, before any code (IN-0029)
842eac9 feat(terminal): the agent channel answers on OSC 20308 (US-0088)
ebab9d2 docs(vt): the agent channel is OSC 20308 everywhere it is described (US-0088)
3e822ee docs(vt): the US-0088 evidence, including the GUI check (IN-0029)

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9
```

## `pwsh scripts/ci-local.ps1` — pristine implementer tree

Green, exit 0, all 10 steps:

```
==> cargo fmt --all -- --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo test --workspace
==> cargo test -p oneterm-vt --features vt-paranoid
==> python scripts/verify-dependency-graph.py
    Dependency graph policy passed for 21 workspace packages and 21 explicit members.
==> python scripts/check-doc-paths.py
    Doc path check passed for 124 current paths in 10 documents.
==> python -m unittest scripts/test_check_english.py
    Ran 2 tests ... OK
==> python scripts/check-english.py
    English contributor-text check passed for 765 files.
==> python scripts/completion-catalog.py validate
    [completion-catalog] all catalogs valid
==> python scripts/third-party-notices.py --check
    THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
```

Aggregated over the two test steps: **62 sections, 1939 passed / 0 failed / 15 ignored**
— exactly the figures the packet's Evidence section reports. `--features vt-paranoid`
exists and is already part of the gate (4 sections, 364+5+5 passed, 3 ignored).

`check-doc-paths.py` and `check-english.py` both pass; note that neither validates
prose accuracy, which is why defects 4–7 survive a green gate.

## New tests added by the verifier

All in `crates/terminal/src/backend/backend_tests.rs`, under the header
`── US-0088 independent verification ──`. They drive a **real** `Terminal::feed`
through `TerminalPump::advance` instead of a hand-built `EventBatch`, which is the gap
that let defects 1–3 through. `cargo fmt` clean, `cargo clippy -p oneterm-terminal
--all-targets -- -D warnings` clean.

```
verify_support_reply_terminator_fidelity_end_to_end ................ ok
verify_the_support_reply_is_not_echoed_into_the_grid ............... ok
verify_the_support_reply_leaves_from_the_alt_screen_inside_a_sync_block ... ok
verify_the_8kib_cap_boundary_under_both_encodings .................. ok
verify_the_seq_watermark_is_shared_in_both_directions .............. ok
verify_the_dead_shapes_produce_no_event_and_count_as_documented .... ok
verify_the_support_reply_precedes_the_da1_reply_in_one_batch ....... FAILED  (defect 1)
verify_the_documented_cap_is_reachable_through_the_engine .......... FAILED  (defect 2)
verify_a_zero_padded_osc_number_reaches_the_same_handler ........... FAILED  (defect 9)

test result: FAILED. 261 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out
```

Each failure is one defect; they are left red on purpose so the fix has a gate. Keeping
all nine is recommended — the six green ones close the coverage hole defect 3 names.

## Out of scope / not verified

- The GUI acceptance screenshot (`US-0088-agent-panel-both-encodings.png`). Not
  re-run: the instructions forbid launching any GUI. Taken at face value; note that
  the screenshot cannot have exercised defects 1 or 2, since the demo scripts emit
  neither a DA1-paired query nor a payload over 2 KiB.
- Platform proof (Windows only), same gap the packet records.
- The 45-recording parity gate and `vt-diff` were not re-run separately; the packet's
  pre-edit byte scan showing neither sequence occurs in any recording is consistent
  with `grep` finding no `20308` and no `\x1b]9;7` in `crates/vt/tests/corpus`.

---

# Re-verification of 25599a2

Same verifier, same worktree, reset to `25599a2` (one commit on top of `3e822ee`).
Date: 2026-09-14. No GUI; no `oneterm.exe` enumerated, driven or stopped.

## Verdict: **PASS**

All three major defects and all six minors/nits from the first pass are fixed, and the
fixes are root-cause fixes rather than patches at the reported symptom. The rework also
writes the two receiver obligations it got wrong back into the public spec, which is
the right place for them. Nothing blocking remains. Two stale test doc comments and one
un-updated claim-set assertion are the only things left, all cosmetic.

## Defect 1 — reply ordering: FIXED

`OscRouter::drain` (`crates/terminal/src/backend/osc_router.rs:145-147`) is now one
pass in byte order; the two-pass hoist is gone. The doc comment on `drain` records why
two passes was a misreading of R-37 ("latency, not order") and names the §3.2 contract
it broke.

R-37 still holds — `verify_the_support_reply_leaves_from_the_alt_screen_inside_a_sync_block`
is green, so the reply is on the transport before the guard drops even from the alt
screen inside an open mode-2026 block. The argument in the commit message is sound: post
`US-0082` the only thing a reply can now queue behind is a `Vec::push` onto `out`, which
never blocks and never leaves the function.

The fix is positional, not "the agent reply wins", and the implementer pinned that
themselves — `verify_the_da1_reply_precedes_the_support_reply_when_it_comes_first` and
`verify_replies_leave_in_byte_order_regardless_of_producer` (DA1 → support → DSR) are
both green. Those are the right counterparts: a router that simply hoisted the support
reply to the front would have passed my original §3.2 test and still been wrong.

## Defect 2 — the 8 KiB cap: FIXED, and my test's inverted assertion is correct

`crates/terminal/src/handle.rs:205` — `claims.claim_large(AGENT_OSC).claim_large(LEGACY_AGENT_OSC)`,
both spellings, since the alias is "parsed identically for one release" and that has to
include its ceiling.

**On the inversion the coordinator asked about.** I had asserted `!survives(2044)`,
which pinned the *defect* (the ~2040-byte inline ceiling), not a requirement. With
`claim_large` lifting that ceiling, `survives(2044)` is the correct assertion and the
inversion is the point of the test. The implementer left an in-body comment saying
exactly that, and — importantly — did not stop at flipping one line: the test now also
asserts `survives(4096)`, `survives(8192)` and `!survives(8196)`, so the published cap
itself is pinned at both ends **end to end through `TerminalPump::advance`**, not just
at `parse_agent_status`. Verified green. `the_documented_cap_is_reachable_under_the_alias_too`
pins 8192 under `9;7` as well, and `an_oversized_agent_payload_is_dropped_and_the_loss_is_counted`
pins 8196 refused under both.

The `truncated` flag is now honoured (`osc_router.rs:194-212`): a truncated agent OSC is
dropped **before** anything parses it and counted in `SharedSessionState::truncated_agent_osc`
(`state.rs:128-139`). The reasoning is right and is the part I would have got wrong —
cut base64 can decode to a *shorter well-formed* event the agent never sent, so
"malformed JSON" is the wrong disposition. Scoped deliberately to the agent channel via
`is_agent_osc` (`osc_router.rs:385-389`), with the free-form-text-degrades-gracefully
rationale spelled out. `a_truncated_agent_payload_is_dropped_and_counted` covers both
spellings plus the "a truncated reserved sub-code is still just an unknown sub-code"
case. Green.

Side effect checked: `claim_large(9)` lifts the inline cap on OSC 9 **notifications**
too. The comment claims the security policy already truncates them; confirmed —
`crates/terminal/src/security_policy.rs:36` `MAX_NOTIFICATION_BYTES = 8 * 1024` and
`sanitize_notification` truncates. So the payload cannot reach the UI oversized. The
residual cost is that a notification is now *parsed* up to `OSC_LARGE` (8 MiB) before
being cut to 8 KiB, where previously the parser stopped at 2 KiB. The spill is transient
and shrinks back after each OSC, and it is the same exposure `claim_large(52)` already
accepts. Accepted trade-off, documented at the call site — not a defect, but it is a new
(small) allocation surface on a channel that did not have one, so recording it.

## Defect 3 — the untested claim: FIXED

Tamper test re-run, twice, to pin down what each half covers.

Tamper A — remove only `claim_large(AGENT_OSC)` (the exact equivalent of my first-pass
tamper), keeping the alias claim:

```
test result: FAILED. 262 passed; 8 failed
  verify_support_reply_terminator_fidelity_end_to_end
  verify_the_support_reply_precedes_the_da1_reply_in_one_batch
  verify_the_da1_reply_precedes_the_support_reply_when_it_comes_first
  verify_replies_leave_in_byte_order_regardless_of_producer
  verify_the_support_reply_leaves_from_the_alt_screen_inside_a_sync_block
  verify_the_documented_cap_is_reachable_through_the_engine
  the_documented_cap_is_reachable_under_the_alias_too
  verify_a_zero_padded_osc_number_reaches_the_same_handler
```

Tamper B — remove both `claim_large` calls:

```
test result: FAILED. 261 passed; 9 failed
  ... the eight above, plus
  handle::tests::the_adapter_claims_the_osc_numbers_it_routes
```

So the implementer's "nine" is accurate for removing *both* claims; removing only the
agent number gives **eight**. Either way the first-pass finding is closed: it was zero.
`crates/terminal/src/handle.rs` was restored after each tamper and
`git status --porcelain` is clean apart from this evidence file; the restored tree was
re-run before anything below (`270 passed / 0 failed`).

## Defect 9 (the nit) — root-caused rather than papered over: FIXED

`parse_osc` now parses `params[0]` to a `u32` and matches the **number**
(`crates/terminal/src/osc.rs:86-93`), so `ESC ] 020308` reaches the same arm — and so do
zero-padded 7, 9 and 133, which were broken the same way and were never reported. The
second source of truth, `AGENT_OSC_PARAM`, is deleted; grep confirms no remaining
reference outside prose. `osc::tests::a_zero_padded_number_reaches_the_same_arm` pins
`020308;0` and `007;file:///tmp`, and pins that `20308x` and `""` are still rejected.
`verify_a_zero_padded_osc_number_reaches_the_same_handler` passes end to end.

Checked for collateral: the string arms `"7"`, `"9"`, `"133"` all became numeric, no
other code in `crates/` keyed on the spelling, and `agent_support_reply` now formats
from `AGENT_OSC` directly.

## Defects 4–7 (docs): all FIXED

| | Was | Now |
|---|---|---|
| 4 | `docs/agent-panel-display.md` × 6, incl. the UI copy at :173 contradicting `view.rs:509` | all six updated; :173 now quotes `Agents that emit OSC 20308 appear here.` |
| 5 | `docs/agents/structure.md:120` | "Folded OSC 20308 agent card model" |
| 6 | `high-level-design.md:60` naming a deleted test | now `dispatch::tests::osc_20308_reaches_the_embedder_through_a_claim` |
| 7 | `DEC-0014:149` open follow-up | ticked, with a "Closed by `US-0088`" note and the prediction's outcome |

Re-grepped `9;7` across `docs/agent-panel-display.md`, `docs/agents/structure.md`,
`docs/PROJECT.md`, `README.md`, `AGENTS.md`, `docs/README.md` — **no hits**.

## Spec additions (not asked for, and the right call)

`docs/osc-agent-status.md` § 3.2 and § 3.4 each gained a receiver obligation this
implementation got wrong: replies must leave in the order the causing sequences arrived
(naming the engine-replies-first seam), and the published cap must be *reachable*, with
"never decode a payload the parser had to cut" stated explicitly. A document that asks
third parties to implement the protocol is the right home for both. § numbering still
resolves after the edits.

## Confirmed for routing to the design agent (not defects in this packet)

Both are in `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md`, flagged but not
edited by the implementer. Text as it stands today:

- **line 193** (the pipeline diagram):
  `│    drain batch  ─▶ OscRouter (OSC 7/9/9;4/9;7/133, clipboard policy, PtyWrite)         │`
  — the router list omits 20308.
- **lines 409-411** (step 4 of the pump walkthrough):
  `4. **Unlock and drain.** The pump drops the guard, then walks `batch`: OSC 7 / 9 / 9;4 / 9;7 /`
  `   133 through the existing `OscRouter`, clipboard through the existing security policy,`
  `   `Reply` bytes into the transport, `Repaint` into one coalescible `SessionEvent::Output`.`
  — same omission, **and** "The pump drops the guard, then walks `batch`" is independently
  stale: since `US-0082` the drain runs *with* the guard held and the UI events are
  flushed after it is dropped. Worth routing as one edit, not two.

## Remaining (cosmetic, non-blocking)

1. **MINOR — two test doc comments now describe the defect they no longer have.**
   - `crates/terminal/src/backend/backend_tests.rs`, above
     `verify_the_documented_cap_is_reachable_through_the_engine`: still says "`AGENT_OSC`
     is claimed with `claim`, not `claim_large`, so a larger payload is truncated". The
     in-body comment explains the inversion correctly; the doc comment above it is stale.
   - Above `verify_a_zero_padded_osc_number_reaches_the_same_handler`: still says "the
     dispatch is a string match on `params[0]` … dropped by the string arm". The dispatch
     is numeric now.
   Both are my original wording, carried over unedited. Harmless, but they are the first
   thing the next reader sees.

2. **MINOR — the one test that directly pins the claim set was not updated.**
   `crates/terminal/src/handle.rs:343-356`,
   `the_adapter_claims_the_osc_numbers_it_routes`, still iterates `[7, 9, 133, 52]` and
   asserts `allows_large(52)` only. It does not assert that `AGENT_OSC` is claimed or
   that either agent spelling allows large — which is why tamper A did not move it. The
   eight end-to-end tests cover the behaviour, so this is no longer a coverage hole, but
   this is the obvious place for a one-line `allows_large(AGENT_OSC)` assertion.

3. **NIT — a truncated `9;7` is not counted as a legacy alias event.** The truncated
   drop (`osc_router.rs:203`) returns before `note_agent_osc`, so such an event
   increments `truncated_agent_osc` but not `legacy_agent_osc_events`, while § 3.1 says
   the alias is "counted". Reachable only above 8 MiB now. Mentioning for completeness.

## Commit trailer — new commit

`25599a2 fix(terminal): replies leave in byte order, and the 8 KiB cap is real (US-0088)`
ends with exactly:

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9
```

No deviation. (The four earlier commits were checked in the first pass and were correct.)

## `pwsh scripts/ci-local.ps1` — 25599a2, from this worktree

Green, exit 0, all 10 steps, `ci-local: all checks passed.`

**62 sections, 1954 passed / 0 failed / 15 ignored** — exactly the implementer's
figures, and +15 over the 1939 of `3e822ee`. `check-english.py` now covers 766 files
(was 765). `check-doc-paths.py`: 124 current paths in 10 documents, passed.

Every one of the 16 agent-channel tests in `crates/terminal` is green, including the
three that were red in the first pass:

```
verify_support_reply_terminator_fidelity_end_to_end ....................... ok
verify_the_support_reply_is_not_echoed_into_the_grid ...................... ok
verify_the_support_reply_precedes_the_da1_reply_in_one_batch .............. ok   (was FAILED)
verify_the_da1_reply_precedes_the_support_reply_when_it_comes_first ....... ok
verify_replies_leave_in_byte_order_regardless_of_producer ................. ok
verify_the_support_reply_leaves_from_the_alt_screen_inside_a_sync_block .... ok
verify_the_8kib_cap_boundary_under_both_encodings ......................... ok
verify_the_documented_cap_is_reachable_through_the_engine ................. ok   (was FAILED)
the_documented_cap_is_reachable_under_the_alias_too ....................... ok
an_oversized_agent_payload_is_dropped_and_the_loss_is_counted ............. ok
a_truncated_agent_payload_is_dropped_and_counted .......................... ok
verify_the_seq_watermark_is_shared_in_both_directions ..................... ok
verify_the_dead_shapes_produce_no_event_and_count_as_documented ........... ok
verify_a_zero_padded_osc_number_reaches_the_same_handler .................. ok   (was FAILED)
osc::tests::a_zero_padded_number_reaches_the_same_arm ..................... ok
osc::tests::the_agent_osc_prefixes_spell_the_claimed_numbers .............. ok

test result: ok. 270 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Disk

Contended throughout by other agents: D: 18.6 GB when the rework run started, dipping to
17.3 GB, recovering to 25.1 GB before the gate and 23.1 GB after. The gate was run only
once free space was back above the 20 GB floor; everything before that was incremental
on an already-warm target directory, with no cold rebuild.

## Still not verified

Unchanged from the first pass: no GUI re-run (forbidden), Windows only, and the
45-recording parity gate was not re-run separately — it is inside `cargo test
--workspace`, which is green.

