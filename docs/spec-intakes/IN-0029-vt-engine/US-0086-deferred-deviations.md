# Work: Deferred deviations and extension-point hardening

ID: US-0086
Intake: IN-0029
Created: 2026-09-13

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (the last deferred deviation, plus hardening of two shipped extension points)
- Risk lane: high_risk — the engine is live behind every session since `US-0081`/`US-0085`, and this
  packet changes a **cursor-movement primitive** (`BS`) and a **public engine API** (`OscClaims`).
- Spec Intake, when required: `IN-0029`

## Outcome

Every deviation row still assigned to `US-0086` is implemented spec-correct, its recording risk is
measured rather than asserted, and the two extension points the LLD calls API — the OSC
registration table and the DA / DECRQM / XTVERSION answers — can no longer shadow a registration
or claim a capability the engine does not have. `US-0087` can close the D / C / G tables from the
status list in this packet without re-deriving anything.

## Scope

- [x] In scope:
  - **D12 — reverse wrap (`CSI ? 45 h/l`).** `Mode::ReverseWrap` gets a reader: `BS` at column 0
    crosses into the previous row's last column **only when that row is `WRAPPED`** and only while
    the mode is set (`grid-and-scrollback.md` R-08 / trap 1). Default off.
  - **DECRQM for `? 45`** becomes real, because the mode now has a reader. The "never answer `Set`
    for a mode that does nothing" rule moves from three hand-written match arms into one table
    (`Mode::inert_state`) that a test walks.
  - **`OscClaims` hardening**: the set of natively handled OSC numbers becomes queryable data
    (`OscClaims::NATIVE` / `is_native`), a `claim` on one of them is a debug assertion rather than
    a silent no-op, `claim_large` on one of them registers the memory ceiling **without** a dead
    delivery claim, and a duplicate `claim` is documented and tested as idempotent.
  - **Unknown OSC / DCS / APC counted in `FeedStats`**: APC joins the existing
    `unhandled_sequences_are_counted_not_echoed` case list (OSC, DCS, CSI and ESC were already
    there; APC was the hole).
  - Measuring the recording risk (`vt-corpus grep-deviations`), re-running the parity gate, `vt-diff`
    over corpus + fixtures, and the `US-0076` verifier's differential families.
  - The final D / C / G status list for `US-0087`.
- [x] Out of scope:
  - `crates/terminal` in any form — another agent is reworking `handle.rs`, and `US-0088` owns the
    agent OSC registration. The `OscClaims` API stays **additive**: no existing signature moves.
  - `VtEvent::Passthrough` (`parser.md` § "No passthrough echo in v1"). It returns in `US-0086`
    *only if* a named consumer exists; the only candidate is SSH-to-ConPTY bridging, which
    `IN-0029.md` puts outside this intake. **Not implemented; recorded as not triggered.**
  - LNM's DECRQM answer (see Gaps) and every other row the design owner must rule on.
  - `US-0087`'s fork decommission.

## Acceptance

- [x] `CSI ? 45 h` then `BS` at column 0 moves to the previous row's last column when that row is
      `WRAPPED`, and does nothing when it is not.
- [x] `CSI ? 45 l` (the default) keeps `BS` at column 0 a complete no-op, pending wrap included —
      trap 1 is unchanged for every program that does not ask for reverse wrap.
- [x] Reverse wrap never crosses above `screen_top()`.
- [x] `CSI ? 45 $ p` answers `Set` after `h` and `Reset` after `l`, because the mode now has a reader.
- [x] A table test walks **every** private mode and asserts that every mode the engine recognises
      but nothing reads answers `Reset` or `NotSupported` — never `Set` — even after an explicit `h`.
- [x] A `claim` on a natively handled OSC number is caught (debug assertion), and `claim_large` on
      one registers only the memory ceiling; a duplicate `claim` is idempotent. All three tested.
- [x] An unknown APC moves `FeedStats::unhandled_sequences` and produces no reply and no glyph.
- [x] `vt-corpus check --engine new` is 45/45 plus the OneTerm recordings, with **no**
      `expected-diffs.json` added — the measurement predicts none (six recordings only ever reset
      `? 45`).
- [x] `vt-diff` over the corpus, the bench fixtures and the `US-0076` verifier families reports no
      **new** divergence family: the "By-design differential divergences" list must not grow.
- [x] `pwsh scripts/ci-local.ps1` green with raw totals recorded.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md` — the `US-0086` packet line ("every deviation
  marked `US-0086` in the two deviation tables … each with its recording risk measured"), R-08,
  R-53, and the two conditional returns (ConPTY bundle, SSH-to-ConPTY bridging) that this packet
  does **not** trigger.
- `low-level-design/dispatch-and-modes.md` — the D table (D12 is the only row left on `US-0086`),
  the mode table's `? 45` row and its "DECRQM must never answer `Set` for a mode that does nothing"
  rule, the OSC registration section, the Answers table, and the "By-design differential
  divergences" list.
- `low-level-design/grid-and-scrollback.md` — R-08 / trap 1 (`BS` at column 0 crosses into a
  `WRAPPED` row only while `? 45` is set), the G table, the C table, and the named test
  `grid::tests::reverse_wrap_crosses_a_wrapped_row`.
- `low-level-design/parser.md` — § "No passthrough echo in v1 (R-35)": the one thing that may
  return in `US-0086` and the condition under which it may.
- `low-level-design/events-and-api.md` — `FeedStats` is the only channel for a dropped sequence.
- `low-level-design/testing-and-bench.md` — the `expected-diffs.json` mechanism and its rules, the
  gate result table, the trap-coverage table.
- `docs/spec-intakes/IN-0029-vt-engine/evidence/US-0072-recording-risk.md` — the D12 row: six
  recordings reset `? 45`, none sets it, so the recording risk is **none**.
- `docs/agents/error-policy.md` — untrusted input: a malformed or unknown sequence moves a counter
  and parsing continues; it is never an error. The debug assertion added here is on a
  **construction-time programmer input** (the embedder's claim list), not on terminal bytes.
- `docs/agents/code-style.md`, `docs/agents/dependencies.md` — no new dependency, no new module.

### Documentation Action

**No contract change** for the behaviour: the LLDs already specify D12, R-08 and the DECRQM rule
exactly as implemented, and this packet implements the contract rather than changing it. The
reviewed docs describe the correct behaviour; what they carry is now-stale **scheduling** text
("deferred to `US-0086`", "to measure in `US-0076`"), which is the design owner's to rewrite and
which this packet is forbidden to edit. Every such line is listed under Reconciliation for
`US-0087`.

Reason: the packet's job is to make the shipped code match the accepted contract and to measure
what the contract said to measure. Nothing here asks the contract to change.

### Reconciliation

No owning doc was edited (the packet may not touch `IN-0029.md`, the HLD or the LLDs). The design
owner must apply the following before `US-0087` closes the tables:

| Doc | Line | What is now stale |
| --- | --- | --- |
| `dispatch-and-modes.md` | mode table, `ReverseWrap` row | "additive feature D12, `US-0086`" and the "until the mode has a reader, `CSI ? 45 $ p` answers `Reset`" clause. The mode **has** a reader now; DECRQM is real. The rule itself stays — `? 9001` still follows it |
| `dispatch-and-modes.md` | D table, D12 | packet `US-0086` → **implemented**; recording risk **measured none**, confirmed by re-running the grep and the gate |
| `dispatch-and-modes.md` | C table, C13 / C14 / C15 | still say "to measure in `US-0076`"; they were measured free there (`crates/tools/src/corpus.rs:44-53` records it) |
| `dispatch-and-modes.md` | OSC registration section | add the two new facts: the native set is queryable (`OscClaims::NATIVE`), and a `claim` on a native number is a debug assertion, symmetric with the existing "a claimed number whose handler is missing is a debug assertion" |
| `grid-and-scrollback.md` | trap 1 bullet, line 580 | "`? 45` is an additive feature and lands in `US-0086`" — landed |
| `grid-and-scrollback.md` | Verification list, line 664 | `grid::tests::reverse_wrap_crosses_a_wrapped_row` is no longer "deferred"; it exists |
| `testing-and-bench.md` | trap table row 13 | cites "grid (G4, `US-0086`)". There is no G4 in the G table any more — it became correction C8 and shipped in `US-0076` |
| `IN-0029.md` | packet list, `US-0086` | tick it |

## Context

- The only D row left on this packet is **D12**. D3, D5, D6 and D11 were superseded into
  corrections C8, C10, C9 and C11 by the owner's correctness-first ruling of 2026-09-12 and shipped
  in `US-0076`; D7, D8 and D10 were implemented in `US-0076` despite the table's column (recorded
  in `evidence/US-0076-verify.md:220`). The G table has no `US-0086` row: G1, G2, G6 and G7 are
  `US-0075` representation changes and G3 was withdrawn.
- `Mode::ReverseWrap` already exists, is already stored by `CSI ? 45 h`, and is already in
  `Mode::PRIVATE`. What is missing is a **reader**: `Screen::backspace` is unconditional.
- The engine is single-threaded and takes `&mut self`; `BS` is on the hot path only in the sense
  that every shell line-edit uses it, so the added work must be a branch on a bit, not a lookup.
- `OscClaims` is a bitmap, so a duplicate claim is already idempotent — the real silent shadow is a
  claim on a number the engine handles natively, where the native match arm wins and the embedder
  never learns its claim is dead. That is what the hardening closes.

## Plan

- [x] Write this packet and mirror the story row into `harness.db`.
- [x] Baseline the gate and `vt-diff` before any edit, so a new divergence is attributable.
- [x] D12: `Screen::backspace(reverse_wrap)`, the `? 45` read at the `BS` call site, DECRQM made
      real, tests for set / reset / not-`WRAPPED` / screen top.
- [x] Hardening: `Mode::inert_state` as the DECRQM table + its walking test; `OscClaims::NATIVE`,
      `is_native`, the `claim` debug assertion, the `claim_large` native path, their tests; APC
      added to the unhandled-count case list.
- [x] Re-measure: `vt-corpus grep-deviations`, `vt-corpus check --engine new`, `vt-diff` over the
      corpus, the fixtures and the verifier families.
- [x] `pwsh scripts/ci-local.ps1`, raw totals into Evidence.

## Decisions

No new decision record. `DEC-0015` (typed `Mode`, `ColorKey`) already governs the mode table, and
`DEC-0014` (extension points are API) already governs `OscClaims`; this packet implements both
rather than choosing anything new.

## Verification Plan

1. **Unit** — `cargo test -p oneterm-vt`: the four reverse-wrap cases, the DECRQM mode-table walk,
   the three `OscClaims` cases, the APC count.
2. **Integration / parity** — `vt-corpus check --engine new` (45 alacritty-ref + the OneTerm
   recordings including `sixel_basic`); `cargo test -p oneterm-tools` runs the same comparison as
   the drift gate.
3. **Differential** — `vt-diff` over the corpus and the bench fixtures, plus the nine `US-0076`
   verifier families left in the scratchpad (`fam`, `micro`, `repro`, `combo`, `clean`, `clean2`,
   `widecase`, `zcase`, `fuzz`). Compared against a baseline taken **before** the edits, so any new
   divergence is this packet's.
4. **Recording risk** — `vt-corpus grep-deviations`, reading the D12 row.
5. **Regression** — `pwsh scripts/ci-local.ps1` (fmt, clippy `-D warnings`, `cargo test
   --workspace`, `vt-paranoid`, the six Python policy checks).

E2E / platform proof is **not met**: this packet changes no UI surface and needs no app launch, and
the owner runs Claude Code inside their own `oneterm.exe`, which must never be driven or stopped.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Results

| Check | Command | Result |
| --- | --- | --- |
| Parity gate, alacritty-ref | `vt-corpus check --engine new` | **45 recordings, 45 passed, 0 failed** — unchanged from the pre-edit baseline |
| Parity gate, OneTerm | `vt-corpus check --engine new --dir crates/vt/tests/corpus/oneterm` | **1 recording, 1 passed, 0 failed** (`sixel_basic`) |
| `expected-diffs.json` | `Get-ChildItem -Recurse crates/vt/tests/corpus -Filter expected-diffs.json` | **none exist, none added** — as the D12 measurement predicted |
| Recording risk, D12 | `vt-corpus grep-deviations` | `vttest_cursor_movement_1`, `vttest_insert`, `vttest_origin_mode_1`, `vttest_origin_mode_2`, `vttest_scroll`, `vttest_tab_clear_set` — each **`?45: 0 set, 1 reset`**. Six recordings turn the mode off and none turns it on, so implementing it moves no cell. Risk: **none**, re-measured, not inherited |
| Differential, corpus | `vt-diff` | 45 recordings, **45 identical**, 0 differing |
| Differential, bench fixtures | `vt-diff --fixtures` | 10 recordings, **10 identical**, 0 differing |
| Differential, the `US-0076` verifier families | `vt-diff --dir <family>` over `fam`, `micro`, `repro`, `combo`, `clean`, `clean2`, `widecase`, `zcase`, `fuzz` (120 recordings) | **byte-identical to the pre-edit baseline in eight of nine**; `fam` moved 15/5 → 14/6 — see below |
| Unit | `cargo test -p oneterm-vt --lib` | 363 passed, 0 failed, 2 ignored |
| Regression | `pwsh scripts/ci-local.ps1` | **green, exit 0**, all ten steps |

**Raw totals**, over the gate's two test steps: **62 sections, 1927 passed / 0 failed / 15
ignored** — `cargo test --workspace` 58 sections 1554/0/12, `cargo test -p oneterm-vt --features
vt-paranoid` 4 sections 373/0/3. Four tests were added and none was rewritten or deleted; the two
sections that carry `oneterm-vt`'s lib suite each gain those four, which is the whole delta.

### The one new differential divergence, and why it is D12 and nothing else

`fam/f08_mode45` was **identical** before this packet and shows **7 252 differences** after. That
is the correction arriving: the fixture is 16 414 bytes of nothing but reverse wrap — 360
`CSI ? 45 h`, 380 `CSI ? 45 l` and 704 `BS` — and the vendored reference does not implement the
mode at all, so an engine that does must differ from it.

Established **by mutation, not by inspection**, the method `US-0076`'s verifier used: with the
single read at the `BS` call site forced to `false` and nothing else changed, `fam` returns to
exactly its baseline 15 identical / 5 differing. The divergence is attributable to D12 alone.

The by-design list therefore gains **no new family** — the existing list keys families by declared
id, and `D12` is already a declared id in the deviation table and in
`crates/tools/src/corpus.rs`'s `KNOWN_DEVIATIONS`. The design owner should add the row
`f08_mode45 → D12` when reconciling. Nothing became unexplained.

### Final D / C / G status, for `US-0087`

**D table (`dispatch-and-modes.md`)**

| Row | Status |
| --- | --- |
| D1 typed `Mode` / `ColorKey` | done, `US-0076`; representation only, free |
| D2 insert mode does not force full damage | done, `US-0076`; free (damage is not compared) |
| D3, D5, D6, D11 | **superseded** into corrections C8, C10, C9, C11 by the correctness-first ruling; all shipped in `US-0076` |
| D4 mode 2026 reports its real state | done, `US-0076`; free |
| D7 DECXCPR, D8 XTVERSION, D10 `modifyOtherKeys` | done in `US-0076` despite the table's `US-0086` column (`evidence/US-0076-verify.md:220`); free — answers are discarded by the harness. Covered by `terminal::tests::da1_da2_dsr_xtversion_answers` |
| D9 LNM tracked but inert | done, `US-0076`; free. **See Gaps** — its DECRQM answer is the one place the "never Set for an inert mode" rule is not applied |
| **D12 reverse wrap `? 45`** | **done here.** Measured **free** against the 45 recordings (six reset it, none sets it); one declared differential divergence on the dedicated `f08_mode45` fixture, proved by mutation |
| D13 DA1 answer, D14 title stack cap, D15 kitty stack overflow | done, `US-0076`; all free |

**C table** — every correction is implemented and **measured free**, with one declared exception
that also came out free in the end:

| Row | Status |
| --- | --- |
| C1, C2, C3, C4, C12 | done `US-0075`; measured free — the gate is 45/45 with no `expected-diffs.json` |
| C5, C6, C7, C8, C10, C11 | done `US-0076`; measured free |
| C9 DECSTR | done `US-0076`. Predicted the one certain diff; **measured free** because `RIS` and `OSC 104` precede it in `grid_reset` and the soft reset's effects are overwritten before the grid is compared |
| C13, C14 | done `US-0077`; **measured free** (`corpus.rs:44-53`). The LLD text still reads "to measure in `US-0076`" — stale, listed under Reconciliation |
| C15 | done `US-0078`; **measured free** — no recording selects. Same stale text |
| C16 grapheme cap | done `US-0074`; measured free — no recording or fixture reaches 17 codepoints on one cell |

**G table (`grid-and-scrollback.md`)** — **no row is or ever was assigned to `US-0086`.** G1, G2,
G6 and G7 are `US-0075` representation changes, all free; G3 was withdrawn; G4 and G5 no longer
exist in the table (G4 became correction C8 and shipped in `US-0076`).

**Still deferred, with the reason:**

| Item | Reason |
| --- | --- |
| `VtEvent::Passthrough` / the echo buffer (R-35) | `parser.md` allows it back in `US-0086` **only if a named consumer exists**. The only candidate is SSH-to-ConPTY bridging, which `IN-0029.md` puts outside this intake. Not triggered, so not built |
| The bundled ConPTY pair (`IN-0029.md:430`) | same shape: it "returns in `US-0086` only if SSH-to-ConPTY bridging is ever built". Not triggered |
| Mode 2027 grapheme clusters (R-56) | deferred whole by the design, not a deviation. Recognised, inert, and DECRQM says `NotSupported` — which the new table walk now enforces |
| `? 69`, `? 80`, `? 1016`, `? 2031`, `? 2048` | each a later intake; unknown, dropped, counted, and `NotSupported` on DECRQM, which is the honest answer |

### Gaps

1. **LNM (`CSI 20 h`) answers `Set` while nothing reads it.** D9 says the mode is tracked and
   inert, and grepping `crates/` finds no reader outside `mode.rs` itself — yet DECRQM reports the
   stored bit, because the mode table's LNM row says "real". That is the same shape as `? 45`
   before this packet and `? 9001` today, and the rule as written ("DECRQM must never answer `Set`
   for a mode that does nothing") would make it `Reset`. **Not changed here**: the rule is stated
   against private modes, the mode table explicitly says "real" for LNM, and the reference answers
   the stored bit too. It is the **design owner's** call — either add `Mode::LineFeedNewLine` to
   `Mode::inert_state` (one line, and the walk would then cover it) or write down why an ANSI mode
   is exempt.
2. **`claim_large` on a native number still registers an inert delivery claim.** `claim_large(52)`
   sets both bits, the delivery half can never fire, and `crates/terminal`'s
   `the_adapter_claims_the_osc_numbers_it_routes` pins `is_claimed(52) == true`. Tidying it was
   tried and reverted: this packet may not touch `crates/terminal`, and breaking a sibling crate's
   contract test to remove a dead bit is not worth it. The bit is documented as inert and
   `is_native` is how a caller finds out. Owner: whoever next touches `handle.rs`.
3. **No E2E and no platform proof.** This packet changes no UI surface and needs no app launch. The
   owner runs Claude Code inside their own `oneterm.exe`, which was never enumerated, driven or
   stopped.
4. **The mode-table walk asserts `inert_state` against itself for the wire value.** It is still a
   real test — it drives `CSI ? n h` then `CSI ? n $ p` through a real `Terminal` and reads the
   reply bytes, so a dispatch path that ignored the table fails. What it cannot catch is the table
   itself being wrong about which modes are inert; that stays a review question, and the
   `assert_eq!(inert, 3)` line is there so a row appearing or leaving is noticed.
5. **Owning docs are stale but unedited**, by instruction. The Reconciliation table above is the
   complete list for the design owner.

## Handoff

Next owner: `US-0087` (close the tables, decommission the fork) and the design owner for the
Reconciliation table above. `US-0088` owns the agent OSC registration; the `OscClaims` API it
builds on is unchanged except for two additions.
