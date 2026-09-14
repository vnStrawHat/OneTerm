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
