# US-0098 independent verification

Packet: [`../US-0098-osc-routes-and-builtins.md`](../US-0098-osc-routes-and-builtins.md)
Contract: [`../low-level-design/osc-extension.md`](../low-level-design/osc-extension.md),
[`DEC-0017`](../../../decisions/DEC-0017-osc-routing-table-not-handler-registry.md)
Intake: IN-0038 (risk lane: high_risk)
Verified at: `82680ce` on `feat/vt-osc-routes`, base `main` `0558fa2`
Date: 2026-09-15
Verifier: independent agent, own worktree, own tests, nothing committed

## Verdict

**PASS-WITH-NOTES.**

No functional defect was found. Every claim about the mechanism's *behaviour* was reproduced with
tests written against the crate's public API without reading the implementer's test module first,
and all of them held on the first run. The notes below are record drift (a gating decision still
marked `Proposed`, two documented facts about `Config` that are false), three small coverage gaps,
and two behaviour consequences the packet does not record. None of them blocks the merge; findings
1 and 2 should be fixed before the intake closes.

## Evidence

Verifier tests, in this worktree, **uncommitted**:

| File | Tests | What |
| --- | --- | --- |
| `crates/vt/tests/us0098_verify.rs` | 27 (26 in release) | the mechanism, the built-ins, the hostile input, OneTerm's shipped table as a black box, the invariants |
| `crates/terminal/src/backend/backend_tests.rs` (appended, `v0098_*`) | 3 | the adapter half: the `9;7` notification drop, the rest of OSC 9, every policy call |

Commands run (all from the worktree, `CARGO_BUILD_JOBS=3`):

```
cargo test -p oneterm-vt --test us0098_verify              27 passed
cargo test -p oneterm-vt --release --test us0098_verify    24 passed (+ the release-only assertion case)
cargo test -p oneterm-terminal                            279 passed (276 + 3 verifier)
cargo test -p oneterm-terminal-view                       304 passed, 3 ignored
cargo test -p oneterm-vt                                  384 + 6 + 8 + 7 + 25 + 4 passed
cargo test -p oneterm-vt --features vt-paranoid           same, all green
cargo test -p oneterm-vt --no-default-features            same, all green
cargo test -p oneterm-tools --test corpus_check             2 passed (the 46 frozen recordings)
cargo test --workspace                                    all green
RUSTDOCFLAGS=-D warnings cargo doc -p oneterm-vt --no-deps --all-features   clean
python scripts/vt-public-api.py --check --no-doc          "public API surface unchanged"
python scripts/check-doc-paths.py                         196 paths, 11 documents
python scripts/check-english.py                           832 files
pwsh scripts/ci-local.ps1 -Full                           "ci-local: all checks passed."
```

`ci-local.ps1 -Full` was run on the **pristine** branch tree (the verifier files were parked in the
scratchpad for that run and restored afterwards), so the summary line is the branch's, not mine.

## Claims attacked and upheld

| # | Claim | Result | How |
| --- | --- | --- | --- |
| 1 | Two bitmaps + sorted spill; four routes | PASS | `crates/vt/src/terminal/osc.rs:140-146` implements all six truth-table rows exactly as `osc-extension.md` tabulates them. Asserted for a built-in (0, 7), a non-built-in below 2048 (633) and five above it (2048, 2049, 20308, 31337, `u32::MAX`) |
| 2 | Dispatch order under `BuiltinAndForward` is typed-first, raw-second | PASS | `dispatch.rs:1285-1297`. `v_builtin_and_forward_is_typed_first_raw_second` asserts the exact event sequence `["Title(hi)", "Osc(0:0|hi)"]`, length 2, and that the engine state was applied |
| 3 | `Drop` counts `unhandled_sequences` | PASS | exactly `+1` for a built-in (`0`, `8`) and for an unknown number (`633`); no event, no engine state change, no hyperlink on the cell |
| 4 | `route(code, Builtin)` on a non-built-in and `large(code, true)` on a `Drop` number are debug assertions, not release panics | PASS | both panic in debug with the documented messages; in release both are inert and `get()` still reports `Drop` and the engine still drops the sequence (`v_the_two_assertions_are_inert_in_release`, release build only) |
| 5 | `overrides()` lists exactly the changed numbers | PASS | ascending, deduped, spill included; a ceiling is not a route change; routing a number back to its default removes it from the list. See finding 6 for the one wart |
| 6 | Numbers above 2048 land in the spill and behave identically | PASS | `v_numbers_above_the_bitmap_behave_identically`, `v_the_spill_stays_sorted_and_deduped` |
| 7 | A table cloned into two `Terminal`s keeps them independent | PASS | mutating the source table after the clone does not reach the already-built terminal |
| 8 | Route changes between feeds, no state leakage | PARTIAL — see finding 5b | there is no public way to change a live `Terminal`'s routes (`Terminal::config` is `&Config`, `crates/vt/src/terminal/mod.rs:465`). The parser-state half is verified: a half-fed OSC completes under the same route and leaves nothing behind for the next one |
| 9 | Six new built-in arms (1, 7, 9, 22, 50, 133) and seven new `VtEvent` variants | PASS | `OscRoutes::BUILTIN` (`osc.rs:71`) and the `osc_builtin` match (`dispatch.rs:1361-1443`) agree exactly, 18 numbers each; `public-api.txt` carries all seven variants, `Progress` and `ShellMark` |
| 10 | `file:///C:/` drive-slash rule moved with OSC 7 | PASS | `file:///C:/x` -> `C:/x`, `file:///C:` -> `C:`, `/Cx/y` untouched, `%20`/`%25`/`%C3%A9` decoded, `%zz` verbatim, bare path verbatim, `file://host/path` split into host + `/path` |
| 11 | `VtEvent`, `Progress`, `ShellMark`, `OscRoute` are `#[non_exhaustive]`; `VtEvent` still not `Clone` | PASS | `vt_event.rs:36/54/75`, `osc.rs:37`; `VtEvent` derives only `PartialEq, Eq, Debug` |
| 12 | Payload ceilings unchanged | PASS | a `Forward` route without `large` still caps at `OSC_INLINE` and sets `truncated`; `large` is per number and orthogonal to the route; `OSC 7` (no ceiling) refuses an 8 MiB payload and counts it once in `truncated_osc` and once in `unhandled_sequences` |
| 13 | Hostile input is counted and never panics | PASS | 924 (number x body x route) combinations plus 64 random tables x 8 KiB of biased random bytes in 64 chunk sizes; `stats.bytes` always equals the chunk, every counter stays bounded by it, no panic. A UTF-8 boundary cut mid-codepoint is lossy, not fatal |
| 14 | Adapter parsers deleted, `OscRouter::handle` rewritten, every policy call preserved | PASS | `parse_osc`, `OscPayload`, `parse_cwd_url`, `percent_decode`, `strip_windows_drive_slash` and `Osc133Kind` are all gone from `crates/terminal/src/osc.rs`. `security_policy.rs` and `osc_color.rs` do not appear in `git diff --stat 0558fa2..82680ce` at all. All five policy calls survive: `sanitize_title` (`osc_router.rs:350`), `validate_clipboard_write` (`:358`), `allow_clipboard_read` (`:180`), `sanitize_cwd` (`:196`), `sanitize_notification` + `NotificationRateLimiter::allow` (`:214-222`). `v0098_every_policy_call_still_happens` drives all four through a real engine with the real `adapter_config` |
| 15 | OSC 133 sub-code is now a whole parameter; the corpus still passes | PASS | `dispatch.rs:1550-1560` matches `Some(b"A")` etc. `133;Abc` is now `unhandled` where the old engine set the template from the first byte; the adapter always used the whole string, so this narrows the engine to what the adapter already did. `corpus_check` (46 recordings) green |
| 16 | Production LOC: `crates/vt` +400, `crates/terminal` -151 | PASS | reproduced exactly with an independent script (`*.rs` under `src`, `*_tests.rs` excluded, each file cut at its `#[cfg(test)] mod tests` tail): 13435 -> 13835 and 6683 -> 6532 |
| 17 | `grep -rn '20308\|AGENT_OSC' crates/vt/` is empty | PASS | zero lines, exit 1 |
| 18 | No callback at feed time | PASS | no `dyn` in `crates/vt/src` outside `reflow/columns.rs` (a `RowSink`, unrelated); the only `impl Fn*` are monomorphised argument positions (`batch.rs:137/151` take a `VtEvent` variant constructor, `parser/osc.rs:135/192` take the ceiling query). Nothing is stored in `Config` or `Terminal` |
| 19 | OneTerm's shipped table works as a black box | PASS | `v_oneterm_shipped_table_emits_what_the_adapter_expects` rebuilds `adapter_config`'s three calls and asserts the exact event sequence for `20308;1;<b64>`, `20308;0`, `9;7;<b64>` (`Notification(\|7;...)` then `Osc(9:9\|7\|...)`), `9;hello`, `9;4;1;50`, `020308;0`, `007;file:///tmp`, `0133;A` and an 8 KiB agent payload (not truncated). `v0098_legacy_alias_drops_the_notification_and_dispatches_the_payload` then proves the adapter emits **exactly one** `SessionEvent::AgentStatus` and no `Notification`, under both spellings |
| 20 | E2E not proven | CONFIRMED | correctly disclosed in the packet; not attempted here either (the owner runs their session inside OneTerm) |

## Findings

### 1. DEC-0017 is still `Proposed` and its consequences were never reconciled — MEDIUM (record)

`docs/decisions/DEC-0017-osc-routing-table-not-handler-registry.md:7` reads `Proposed`. Every other
implemented decision in `docs/decisions/` reads `Accepted` / `accepted`. The packet's own Decisions
section says "Proposed; the owner accepts. **This packet must not start before it is accepted.**"
The status was never flipped, so the gating record for a high-risk packet reads as if the work
should not have begun.

The decision's `## Consequences` boxes are all still `[ ]`, including the two this packet exists to
settle:

* line 84, "OneTerm's OSC 20308 agent channel keeps working through three lines ... and **zero**
  lines in `crates/vt`" — **confirmed** by this verification; should be ticked.
* line 91, "`crates/terminal` sheds at least 350 production lines" — **not met** (-151, see claim
  16). The packet discloses this against its own acceptance box but does not record it against the
  decision, which is where a future reader will look.

**Recommendation:** flip the status, tick line 84, and amend line 91 with the measured figure and
why the estimate was wrong (the adapter *gained* the agent-channel dispatch `parse_osc` used to
host, plus four typed arms in place of one raw one).

### 2. Both contract documents state that `Config` is `PartialEq`. It is not, and never was — MEDIUM (decision drift)

`DEC-0017:65` ("keeps `Config` `Clone` and `PartialEq`") and `:72` ("a `Box<dyn ...>` in `Config`
costs `Clone` and `PartialEq`, both of which `Config` has today and both of which existing tests
use"), and `osc-extension.md:56-57` ("makes `Config` neither `Clone` nor `PartialEq`, both of which
it is today and both of which the adapter's tests use").

`crates/vt/src/terminal/mod.rs:68` is `#[derive(Clone, Debug)]`, and
`git show 0558fa2:crates/vt/src/terminal/mod.rs` is the same. `Config` has never been `PartialEq`.
Proven: asserting `config_a == config_b` fails to compile with `E0369: binary operation == cannot
be applied to type Config`, `Config does not implement PartialEq`.

This is one of the two arguments DEC-0017 uses to reject the trait-object handler registry. The
rejection still stands on the other one (the no-callback invariant, which is real and which this
implementation honours), so the *decision* is sound; the *reasoning as written* is not. What is
`Clone + PartialEq` is `OscRoutes` itself (`osc.rs:59`), which is what the design actually needed.

**Recommendation:** correct both sentences, or add `PartialEq` to `Config` so the documents become
true. Pinned in `v_config_is_clone_and_the_table_is_clone_plus_partial_eq`, which will stop
compiling if `Config` ever gains it.

### 3. The lookup is not "two bit tests" — LOW (decision drift)

`DEC-0017:46` and `:65` say the lookup costs "two bit tests" / "two bit tests per OSC";
`osc-extension.md:425` says "two bitmap lookups". `OscRoutes::get` (`osc.rs:140-146`) does two
bitmap reads **plus** `has_builtin`, which is `BUILTIN.contains(&code)` over an 18-element `[u32;
18]` (`osc.rs:79-81`) — a linear scan, evaluated on the built-in-route rows. The parser separately
calls `allows_large` (`dispatch.rs:1297`), a third bitmap read, before buffering.

Still O(1) and allocation-free; the documented cost is simply understated. No action required
beyond a wording fix if the number is meant to be load-bearing.

### 4. The new built-in arms allocate per sequence, where `main` forwarded zero-copy spans — LOW (design-claim drift, minor perf regression)

`osc-extension.md:14` states the mechanism works "without the engine ever running embedder code,
taking a lock, or **allocating per sequence**". The routing table honours that. The arms do not:

* `osc_cwd` (`dispatch.rs:1470-1504`): `String::from_utf8_lossy(url).into_owned()`,
  `format!("/{path}")`, `percent_decode`'s `Vec::with_capacity` + `into_owned()`, `host.to_owned()`
  — up to five allocations per `OSC 7`.
* `osc_progress_or_notification` notification path (`dispatch.rs:1536-1545`): `Vec<Cow>` +
  `join(";")`.
* `osc_text` (`dispatch.rs:1450-1465`): `Vec<&str>` + `join` + `to_owned` — pre-existing for OSC
  0/2, new for OSC 1.

On `main` these three numbers were forwarded raw and cost nothing but the arena copy. OneTerm's own
prompt integration emits `OSC 7` on every prompt, so this is a real (tiny) regression on a
per-prompt path. It is bounded: `v_the_builtin_arms_stop_growing_the_batch_once_warm` feeds eight
OSC kinds 1024 times and the batch's arena, param and event capacities do not move after warm-up.

**Recommendation:** correct the sentence in `osc-extension.md`, or lower the allocation count in
`osc_cwd` (the `format!("/{path}")` and the `host.to_owned()` are both avoidable).

### 5. Two mechanism gaps neither document records — LOW

**5a. `large()` asserts on ordering, not on intent.** `OscRoutes::large` (`osc.rs:129-135`) asserts
`self.get(code) != Drop`, so `routes.large(20308, true)` **before** `routes.route(20308, Forward)`
trips the assertion even though the finished table is correct. `adapter_config` happens to call them
in the right order (`handle.rs:190-196`), and the builder chain makes the right order natural, but
the edge is undocumented and a table built from configuration data in an arbitrary order will hit
it.

**5b. Routes cannot be changed on a live `Terminal`.** `Terminal::config` returns `&Config`
(`crates/vt/src/terminal/mod.rs:465`) and there is no setter. An embedder that wants to start
forwarding a number mid-session must rebuild the `Terminal`, losing the grid. Neither DEC-0017 nor
`osc-extension.md` mentions this; for an "extension mechanism" it is worth one sentence, because the
obvious mental model is that the table is live.

### 6. `OscRoutes: PartialEq` is structural, not semantic — LOW (API wart)

`OscRoutes::new()` and `OscRoutes::new().route(633, OscRoute::Drop)` answer `get()` identically for
every number and have identical `overrides()`, yet compare **unequal**: `route()` sets the suppress
bit even when the resulting route is the number's default, and `CodeSet::set` (`osc.rs:190-207`)
never prunes. The same holds for the spill — a number above 2048 routed and then routed back to its
default stays in `CodeSet::high` forever. Pinned by `v_table_equality_is_structural_not_semantic`.

`Config` is not `PartialEq` (finding 2), so nothing in-tree is affected; but `OscRoutes: PartialEq +
Eq` is public API and an embedder comparing two tables will get the wrong answer. `overrides()` is
the correct semantic view and behaves.

### 7. The extended fuzz target does not fuzz the new mechanism — LOW (coverage)

The packet says the fuzz target "*was* extended (its `Sink` now derives an `OscRoutes` from the
input's first bytes) so a Linux run gets the coverage." The table is real
(`crates/vt/fuzz/fuzz_targets/parser.rs:52-68`) but `Sink` implements `Dispatch` with empty bodies
and uses it **only** in `osc_allows_large` (`:36-38`). The route lookup, `osc_builtin`, the six new
arms and `forward_osc` all live in `terminal::Handler`, which the fuzz target never constructs. A
Linux fuzz run would therefore cover the parser under varying ceilings, not the mechanism this
packet adds.

The coverage does exist, just not there: the in-tree
`arbitrary_bytes_and_an_arbitrary_table_never_panic` drives a real `Terminal`, and so do my
`v_random_tables_and_random_bytes_never_panic` (64 tables x 8 KiB random streams x 64 chunk sizes)
and `v_hostile_osc_input_is_counted_and_never_panics` (924 combinations). The claim in the packet
should be narrowed.

### 8. A deleted regression guard was not replaced — LOW (coverage)

`git show 0558fa2:crates/vt/src/terminal/terminal_tests.rs` contained
`claimed_osc_reaches_the_batch_without_allocating_per_osc`, the only test pinning "no allocation per
OSC on the hot path" — one of DEC-0017's selling points and the subject of finding 4. It was deleted
with the rename and nothing took its place.

Two replacements are in `crates/vt/tests/us0098_verify.rs`
(`v_the_osc_path_stops_growing_the_batch_once_warm`,
`v_the_builtin_arms_stop_growing_the_batch_once_warm`). Recommend porting one into the crate's own
test module.

### 9. Two migrated literals lost their test — LOW (coverage)

`git show 0558fa2:crates/terminal/src/osc.rs`'s `a_zero_padded_number_reaches_the_same_arm` asserted
`parse_osc(&[b"007", b"file:///tmp"])`. That literal did not move to `crates/vt`, and
`verify_a_zero_padded_osc_number_reaches_the_same_handler` (backend_tests.rs) covers only `020308`.
So "an xterm-derived parser accepts `007` for `7`" is no longer pinned for the built-in numbers.

Verified still correct here: `007;file:///tmp` -> `Cwd(|/tmp)` and `0133;A` -> `ShellMark(PromptStart)`
(`v_oneterm_shipped_table_emits_what_the_adapter_expects`). Recommend adding the two lines to the
crate's tests.

### 10. Non-UTF-8 `OSC 7` changed behaviour, and the CHANGELOG says it did not — LOW

On `main`, `parse_osc`'s OSC 7 arm did `std::str::from_utf8(params[1]).ok()?` and dropped a
non-UTF-8 URL entirely. `osc_cwd` now uses `String::from_utf8_lossy` (`dispatch.rs:1473`) and
reports a path containing `U+FFFD`, leaving the decision to `sanitize_cwd`. The doc comment on
`percent_decode` (`dispatch.rs:1714-1717`) argues for this deliberately ("a directory name is not
required to be valid UTF-8"), and it is defensible — but `CHANGELOG.md` says "The wire behaviour is
unchanged, down to the percent decoding and the Windows drive-slash rule", which is not quite true.
Verified by `v_osc_7_utf8_boundary_cut_never_panics`.

### 11. The `BuiltinAndForward` ruling roughly doubles what a hostile `OSC 9` costs — LOW (memory)

`adapter_config` buys OSC 9 the 8 MiB tier for the legacy alias's sake
(`handle.rs:194-196`). On `main`, `claim(9) + claim_large(9)` meant one arena copy of the raw
parameters. Now the notification arm **also** joins the whole body into a `String` and interns it,
so one hostile 8 MiB `OSC 9` costs the parser spill, the transient join, *and* two arena copies.

Verified bounded and panic-free: `v_a_hostile_8_mib_osc_9_under_the_shipped_table_is_bounded` feeds
8 MiB, gets `truncated_osc == 1`, both events, and an arena at least the payload's size. Not a
defect — the 8 MiB tier is the ceiling and it holds — but `osc-extension.md`'s "Payload ceilings and
hostile input: unchanged in mechanism, extended in coverage" does not mention that the *cost per
ceiling-sized sequence* moved for the one number OneTerm wraps.

### 12. Acceptance criterion wording overstates what shipped — INFORMATIONAL

"`adapter_config` in `handle.rs` contains **no** reference to OSC 7, 9 or 133" is ticked. There is no
numeric `7`/`9`/`133` literal, but `adapter_config` routes `LEGACY_AGENT_OSC`, which **is** 9
(`handle.rs:194`), and its doc comment discusses OSC 9 at length. The owner's ruling on the alias
made OSC 9 knowledge in `adapter_config` unavoidable, so the criterion as written reads stronger
than reality. The spirit — no OSC 7 or 133 knowledge, and the agent number handled as a constant —
holds.

### 13. Documentation staleness the packet did not catch — INFORMATIONAL

* `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md:203` and `:279` still show
  `pub osc_claims: OscClaims` and `pub use osc::{OscClaims, ...}`. The packet's Reconciliation lists
  that file as "Reviewed, no change needed"; two of its code blocks are now false.
  `dispatch-and-modes.md` got a "Superseded by `US-0098`" banner, `events-and-api.md` did not.
* `docs/spec-intakes/IN-0038-embeddable-vt-core/IN-0038.md:233` still estimates `crates/terminal
  -420` for this packet; the measured figure is -151.

### 14. Migration fidelity carried three oddities forward — INFORMATIONAL

Faithful to `git show 0558fa2:crates/terminal/src/osc.rs`, so this is not a new defect, but the
contract's arm table (`osc-extension.md:213`, "`st` in `0..=4`, `pr` clamped to 100") is only true
for values that fit a `u8`:

* `OSC 9;4;1;1000` -> `Progress::Set(0)`, not `Set(100)` — `1000` fails `parse::<u8>()` and
  `unwrap_or(0)` wins before the `min(100)` clamp. `250` clamps correctly because it fits.
* `OSC 9;4;x;1` -> `Progress::Remove` — a non-numeric state parses as 0.
* `OSC 9;4` with no state at all -> `Progress::Remove`.

All three pinned in `v_osc_9_4_progress_states_and_out_of_range`.

### 15. `crates/terminal` lost compile-time exhaustiveness on `VtEvent` — INFORMATIONAL

`#[non_exhaustive]` forces a wildcard across the crate boundary, so `OscRouter::handle` now ends in
`_ => {}` (`osc_router.rs:310`), and `progress_style` (`terminal-view/.../render.rs:537`) and
`set_progress` (`terminal-view/.../view.rs:461`) do the same for `Progress`. A future engine event
that OneTerm *should* handle will be silently ignored instead of failing the build. This is the
documented price of the `#[non_exhaustive]` follow-up in DEC-0017 and both terminal-view sites chose
a sensible default, but it is a real loss of a safety net and nothing records it.

### 16. Unrelated flake observed — INFORMATIONAL

`render::bench::integrity_walk_cost_per_feed_and_render_update`
(`crates/vt/src/render/render_bench.rs:143`) failed once while the machine was compiling in
parallel, and passed in isolation, on re-run, and inside `ci-local -Full`. Timing-sensitive,
pre-existing, unrelated to this packet.

## What could not be verified

* **The manual Windows E2E walk.** Not attempted: the owner runs their own session inside OneTerm
  and this worktree must not start or stop the app. The packet discloses this and names a test per
  step; I confirmed those tests exist and drive the same bytes, and added `v0098_*` for the one path
  they missed (the `9;7` notification drop through the real engine). **E2E remains unproven.**
* **The one-hour fuzz run.** Not attempted (`cargo-fuzz` needs nightly libFuzzer, unusable on
  `x86_64-pc-windows-msvc`). See finding 7 for what the extended target would and would not have
  covered.
* **Real allocation counts.** Finding 4 is read from the source, not measured with an allocator
  hook; `crates/vt/tests/parser_limits.rs` installs a `#[global_allocator]` and is the place to
  measure it if anyone wants the number.
* **The harness row.** `harness.db` was not written (the packet forbids it and so does this task).
  Read-only check: `intake_id = 43` is IN-0038, and the snippet's proof flags `(1, 1, 0, 1)` match
  every sibling row on this intake. `US-0098` has no row yet.
* **Rebase.** The branch is on `0558fa2`; `main` has since moved to `a13002a`. Verified as branched,
  not as rebased.

---

## Final re-check at `e62da2d`

Rebased onto `main` `072560a` (`DEC-0017` accepted, `US-0100` search merged). Re-verified by the
same verifier, in the same worktree, nothing committed.

**Verdict: PASS.** Every note from the first pass is closed truthfully, four of them by fixing the
code rather than the prose. Two new informational notes below; neither blocks the merge.

### The adopted suite was not weakened

`crates/vt/tests/us0098_verify.rs` differs from the verifier's original by **109 diff lines**, all
of them the module doc plus the three cases the verification itself invalidated. Every other test is
byte-identical, and each change makes the assertion **stronger**:

| Was | Is |
| --- | --- |
| `v_table_equality_is_structural_not_semantic` -- `assert_ne!` | `v_table_equality_is_semantic` -- `assert_eq!`, plus the `BuiltinAndForward`-on-a-non-built-in normalisation |
| non-UTF-8 `OSC 7` -> a lossy `Cwd` | dropped and counted, plus a new case for the percent-escape path that stays lenient |
| `9;4;1;1000` -> `Set(0)`, "pinned as observed" | `Set(100)`, plus a new `9;4;300` case |

The three adapter tests in `backend_tests.rs` were adopted verbatim; only the `v0098_` prefix and
one doc sentence changed. Bodies identical.

### New attacks, all passing

`crates/vt/tests/us0098_recheck.rs`, 11 tests, debug and release, written against the reworked
pieces only:

| Note | Attack | Result |
| --- | --- | --- |
| 3 | `has_builtin` vs `BUILTIN` for **every** number `0..4096` plus five above it; `BUILTIN.len() == 18`; every built-in inside the bitmap | PASS. `BUILTIN_BITS` is a `const` block with a compile-time `assert!(code < BITMAP_BITS)`; `git grep BUILTIN.contains -- crates/vt/src` is empty. `get()` is at most three bit tests (two `CodeSet::contains` plus `has_builtin`, and the `(true, _)` / `(false, _)` arms skip the third); `allows_large` is the fourth, asked once by the parser |
| 3 | `get()` is order-independent and repeatable across 2100 numbers | PASS |
| 6 | `Drop` on a non-built-in writes nothing (bitmap **and** spill); `BuiltinAndForward` on a non-built-in equals `Forward`; a spilled number routed away and back equals a fresh table; `Drop` on a built-in stays visible | PASS |
| 6 | Brute force: for six numbers x every legal route, equal `get()` implies equal table and unequal `get()` implies unequal table | PASS |
| 4 | `OSC 7` with `%C3%A9%C3` (the documented single repair), 256 warm + 4096 feeds | PASS, capacities flat |
| 4 | `OSC 0/2` titles (the documented owned `String`), 256 warm + 2048 feeds | PASS, capacities flat |
| 4 | 14 alternating kinds x 64 warm + 512 feeds | PASS, capacities flat |
| 4 | 2 MiB `OSC 9` body under `BuiltinAndForward` + `large` | PASS: `truncated_osc == 0`, both events, body intact, arena settles at one payload |
| 10 | `OSC 7` URLs ending in a lone lead byte, a lone continuation byte, `FF FE`, and a surrogate | PASS: all four dropped and counted, matching `main`. Percent escapes (`%FF`, `%80`) stay lenient, `%C3%A9` still decodes, and the drive-slash rule survives the span-cut rewrite including the decoded form `file:///C%3A/x` -> `C:/x` |
| 14 | `9;4;1;1000` / `;250` / `;101` / `;4294967295` all clamp to 100; `9;4;5`, `;255`, `;256`, `;300`, `;4294967295` (state) are all counted, never `Remove`; `9;4;x;1` and `9;4` still `Remove` | PASS |
| -- | 64 random tables x 8 KiB biased random bytes x 64 chunk sizes against the reworked arms | PASS, no panic, counters bounded |

The in-tree guard `the_builtin_arms_do_not_grow_the_batch_once_warm`
(`crates/vt/src/terminal/terminal_tests.rs`) is genuine, not vacuous: eight real OSC kinds in one
buffer, 64 warm-up feeds, then 1000, with all three capacities asserted.

### Records

* `DEC-0017:7` reads `Accepted (owner ruling, 2026-09-15: ...)`. Line 48 now says "three bit tests
  ... plus a fourth for the payload ceiling"; line 67 says "keeps `Config` `Clone`, and makes the
  table itself `Clone + PartialEq`"; lines 76-79 record the `PartialEq` claim as false and why the
  conclusion survives. The `20308` consequence is ticked `[x]` as independently confirmed.
* The packet's "Verification Notes Closed" table covers all sixteen notes (5 as `5a` / `5b`), the
  precondition sentence at lines 251-252 records that the decision was accepted before the packet
  merged, `events-and-api.md:280` carries an "`OscClaims` until `US-0098`" marker, and
  `IN-0038.md:278` records "estimated ... **measured +527 and -151**".
* LOC re-measured against the rebase base with the verifier's own script:
  `crates/vt/src` 13846 -> 14373 (**+527**), `crates/terminal/src` 6442 -> 6291 (**-151**). Exact.
* No public surface change from the rework: `python scripts/vt-public-api.py --check` (fresh
  rustdoc) says "public API surface unchanged". `mark`, `extend`, `finish_trimmed`, `finish_lossy`
  and `StrSpan::skip` are `pub(crate)`, and `mod batch` is `pub(crate)`.

### Gates

```
cargo test -p oneterm-tools --test corpus_check    2 passed
cargo test -p oneterm-vt                          400 + 6 + 8 + 7 + 11 + 27 + 5 + 6 passed
cargo test -p oneterm-vt --features vt-paranoid   green
cargo test -p oneterm-vt --no-default-features    green
cargo test -p oneterm-vt --features regex         405 passed
cargo test -p oneterm-terminal                    267 passed
cargo test -p oneterm-terminal-view               304 passed, 3 ignored
cargo check   (inside crates/vt/fuzz, stable)     exit 0
pwsh scripts/ci-local.ps1 -Full                   "ci-local: all checks passed."
```

`ci-local -Full` was again run on the pristine tree, the verifier's re-check file parked in the
scratchpad for that run.

### New notes

**A. `EventBatch::finish_trimmed` does not rewind on invalid UTF-8, though its doc says it does --
LOW (latent).** `crates/vt/src/events/batch.rs`: the doc reads "`None`, **and the arena rewound**,
when what was assembled is not valid UTF-8", but the body returns through
`std::str::from_utf8(..).ok()?` **before** any `truncate`, so the partial bytes stay in the arena.
Unreachable today: its only caller, `osc_text`, appends `str::from_utf8(part).ok()` pieces joined
with `;`, which is always valid UTF-8, so the `None` arm cannot fire. It is a trap for the next
caller -- and that `None` also returns from `osc_text` without calling `unhandled()`, so such a
sequence would be silent as well as leaky. One `self.arena.truncate(mark);` before the `?` closes
both.

**B. The fuzz target does not resolve the payload spans it says it resolves -- LOW (coverage).**
`crates/vt/fuzz/fuzz_targets/parser.rs` drains with `std::hint::black_box(event)` under the comment
"so every payload span is resolved against the arena that produced it". `black_box` only stops the
loop being optimised away; it never calls `batch.str(span)`, which is what would catch a span whose
bounds landed off a character boundary -- precisely the new risk that `StrSpan::skip` and
`finish_lossy` introduce. Matching on the event and reading each `StrSpan` would make the claim
true. The target is otherwise correct and now drives a real `Terminal`, which was the point of
note 7.

