# Work: Parser core — the Williams state machine of `oneterm-vt`

ID: US-0073
Intake: IN-0029
Created: 2026-09-12

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

- Change type: new capability
- Risk lane: high_risk
- Spec Intake, when required: IN-0029

## Outcome

`crates/vt/src/parser/` turns an untrusted byte stream into `Dispatch` calls and nothing else.
It is a module behind one narrow trait, so the state machine can be replaced without touching
`dispatch/` (US-0076). Concretely:

- the fourteen Williams states with the modern deviations
  [`low-level-design/parser.md`](low-level-design/parser.md) lists: 7-bit-only C1, colon
  sub-parameters preserved as structure, `ESC` inside OSC/DCS, `BEL` and `ST` terminators,
  `CAN`/`SUB` abort;
- typed parameters carrying the separator that precedes each value;
- streamed OSC / DCS / APC with the caps the LLD sets — 2 KiB inline, 8 MiB only for a number the
  embedder claimed large, 16 MiB per DCS/APC sequence — truncating or aborting, never erroring, and
  with no unbounded `Vec` anywhere;
- UTF-8 decoding with the reference's replacement rules and a four-byte carry across `advance`
  calls;
- printable runs batched into `print_str(&str)` instead of one call per character;
- synchronized output (mode 2026) parsed as an ordinary private mode, with no byte buffer.

## Scope

- [ ] In scope: `crates/vt/src/parser/{mod,state,params,osc,utf8,parser_tests}.rs`; the
  `crates/vt` manifest and its workspace registration (the crate skeleton is shared with US-0074);
  `crates/vt/src/lib.rs` limited to the `pub mod parser;` line; `crates/vt/tests/differential.rs`;
  `crates/vt/fuzz/` (the Linux-only `parser` target).
- [ ] Out of scope: `dispatch/`, the grid, `cell.rs` / `style.rs` / `grapheme.rs` (US-0074),
  `strip.rs`, `Config` / `OscClaims` / `FeedStats` (US-0076 and US-0079), and any embedder change.

## Acceptance

- [x] `cargo test -p oneterm-vt parser::` green, covering every test
  [`low-level-design/parser.md`](low-level-design/parser.md) § Verification names except the two
  it places outside this packet's files (`strip::tests::removes_sequences_keeps_text`).
- [x] `cargo test -p oneterm-vt --test differential` green: the action traces of the new parser and
  of the raw vendored `vte` state machine agree over all 45 recordings and over generated
  random/mutated streams, with every accepted difference filtered explicitly and counted.
- [x] `differential::oracle_precondition_patches_do_not_touch_the_state_machine` green — every
  `+++` line of `vendor/patches/vte/*.patch` is `src/ansi.rs`, so the oracle is pristine under
  `[patch]` (R-41).
- [x] An OSC that never terminates stays bounded and truncates; a DCS past the byte cap aborts
  once. Neither errors, neither panics, neither allocates without a bound.
- [x] 10^6 pseudo-random bytes drive the parser without a panic, and the same stream produces one
  action trace at 1-, 7- and 64 KiB chunking.
- [x] Tier 1 throughput recorded against the US-0072 old-engine baseline
  ([`evidence/US-0072-bench-baseline.md`](evidence/US-0072-bench-baseline.md)) as a ratio, never as
  a gate (R-29). **0.89x to 8.24x**, measured same-session after the rework
  ([`evidence/US-0073-verify.md`](evidence/US-0073-verify.md) § 7); the two columns still do not
  measure the same thing — see that file's caveats.
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/parser.md` — the specification this packet
  implements: states, parameters, OSC caps, DCS/APC streaming, UTF-8, C1, batching, the P1-P7
  deviation table, and the differential oracle.
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` — crate layout (parser is a module,
  not a crate), the memory-caps table, the dependency table, and problems P1, P21, P22, P33.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` — the consumer of
  this trait: `OscClaims`, `claim_large`, and `impl Dispatch for Handler`.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` — the `Config` that owns
  `osc_claims`, `FeedStats` counters the caps feed, and the rule that the whole parser module is
  `pub(crate)` once the engine has a public surface.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the five test
  layers, the differential runner's place among them, the Linux-only fuzz rule (R-47) and the
  bench tiers.
- `docs/spec-intakes/IN-0029-vt-engine/research/engine-semantics.md` § 1 — the reference state
  table, parameter rules, OSC/DCS quirks and the seven UTF-8 cases.
- `docs/spec-intakes/IN-0029-vt-engine/research/prior-art.md` § 4 — Williams' scope, the deviations
  every modern terminal adopts, and the ranked performance techniques.
- `docs/terminal-backend.md` — the terminal backend contract the engine sits behind; unchanged by
  this packet because nothing is wired to the new parser yet.
- `docs/decisions/DEC-0014-oneterm-owns-its-vt-engine.md` — why a first-party engine exists at all.
- `docs/agents/{code-style,crate-dependency-rules,dependencies,error-policy}.md` — module layout,
  the new crate's place in the graph, the dependency policy, and the untrusted-input error rule.

### Documentation Action

No contract change. The LLD already specifies this packet completely; the implementation follows it
and records its deviations below rather than editing the design (the intake's design files are
owned by the design agent). `docs/terminal-backend.md` still describes the engine shipping today,
which is correct until US-0081 puts the new engine behind the seam; US-0082 owns its rewrite.

Reason: this packet adds a module nothing consumes yet, so no documented behaviour changes.

Update required, but owned elsewhere and recorded as a gap:

- `docs/agents/dependencies.md` § 3 gains the `oneterm-vt` rows (`memchr`, and `vte` as a
  dev-only oracle). The HLD assigns that table to US-0082 (risk 12) and
  `testing-and-bench.md` § 7 assigns it to US-0072; neither ran it. Not taken here because this
  packet's write scope excludes `docs/agents/`.
- `docs/agents/structure.md` § 1 and § 3 gain the `crates/vt` rows. US-0074 creates the rest of
  the crate; the row belongs with the crate, not with one of its modules.

### Reconciliation

Docs changed, after the merge with `feat/vt-engine` and the independent verification:

- `docs/agents/dependencies.md` § 3 and `docs/agents/structure.md` § 3 — both rows exist on the
  merged tree (US-0074 and US-0075 wrote them) and enumerate `oneterm-vt`'s *complete* dependency
  set, so omitting `memchr` and the `vte` dev-oracle made them **wrong** rather than merely
  incomplete. That is finding M-A, and it is why the write-scope exclusion recorded above no longer
  applies. The structure row also gained the parser module and the grid.
- `scripts/dependency-graph-policy.json` and the workspace manifest moved with the crate, as they
  must, or `verify-dependency-graph.py` fails.

No LLD edited. `parser.md`'s P2, P8 and P9 rows are now stale against the implementation; each is
recorded as a packet deviation (V9, V2, V7) and the edit is the design owner's (finding M-C).

## Context

- The crate skeleton is shared with US-0074, which lands `cell` / `style` / `grapheme` in its own
  worktree. This packet creates `crates/vt/Cargo.toml` and a `lib.rs` holding only `pub mod
  parser;`, so the merge is a one-line union.
- `parser` is `pub` rather than `pub(crate)` only because the crate has no other module yet and
  the differential test reaches it through the crate's public API;
  [`events-and-api.md`](low-level-design/events-and-api.md) narrows it at US-0076.
- The oracle is the vendored `vte`, which `[patch]` forces on every dependency kind. Both vendored
  patches touch only `src/ansi.rs`, so `vte::Parser` + `vte::Perform` are upstream bytes; the
  precondition is asserted by a test rather than trusted.
- `memchr` 2.8.2 and `vte` 0.15.0 are already in `Cargo.lock`, so the graph does not grow.

## Plan

- [x] Work packet and harness story row before any code.
- [x] Crate manifest, workspace member, dependency-graph policy entry.
- [x] `params.rs`, `osc.rs`, `utf8.rs`, `state.rs`, `mod.rs`.
- [x] Unit tests in `parser_tests.rs`, one per LLD verification row.
- [x] `tests/differential.rs` with the corpus, generated streams, chunk invariance and the oracle
  precondition.
- [x] `fuzz/` target plus the in-tree byte-budget substitute that CI on Windows can run.
- [x] Tier 1 timing note against the US-0072 baseline.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

- [`DEC-0014`](../../decisions/DEC-0014-oneterm-owns-its-vt-engine.md) — OneTerm owns its VT engine.
- No new decision record: every choice this packet makes is already ruled on in the LLD, and the
  deviations below are implementation readings, not inheritable rules.

## Verification Plan

1. `cargo test -p oneterm-vt` — the parser unit suite, the pseudo-random property test and the
   cap tests.
2. `cargo test -p oneterm-vt --test differential` — the oracle over 45 recordings, 4 generated
   stream families, chunk invariance and the patch-scope precondition.
3. `cargo test -p oneterm-vt --release parser::bench_note -- --nocapture` with
   `ONETERM_VT_BENCH_FIXTURES` pointing at `vt-bench fixtures --out` output — tier 1 throughput.
4. `pwsh scripts/ci-local.ps1` — fmt, clippy `-D warnings`, the whole workspace suite, the
   dependency-graph policy, doc paths, the English check, the completion catalogs and the
   third-party notices.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

See [`evidence/US-0073-verify.md`](evidence/US-0073-verify.md) for the raw command output, the
differential filter counts and the tier 1 table.

### Deviations from the LLD

| # | LLD text | What was built | Why |
| --- | --- | --- | --- |
| V1 | `Config::osc_claims` decides which OSC number may spill to `OSC_LARGE` | `Dispatch::osc_allows_large(&self, code) -> bool`, default `false` | `Config` and `OscClaims` are US-0076 files. The parser's only outward coupling is the `Dispatch` trait, so the claim query goes through it; US-0076's `Handler` answers it from `Config::osc_claims` in one line. |
| V2 | `Ground` executes `0x7F` | implemented as specified, and filtered in the differential as **P8** | The reference *prints* `U+007F`. The LLD's Ground row says `execute`, which is the correctness-first reading (xterm, Ghostty and kitty all ignore DEL in ground), but it is not in the LLD's P1-P7 exclusion list, so the oracle needed an eighth filter. Flagged for the design owner: either the row or the table is wrong. |
| V3 | `parser::props::arbitrary_bytes_never_panic_and_chunking_is_invariant` implies `proptest` | a deterministic xorshift generator inside the same test | `proptest` is not yet a workspace dependency and this packet needs no shrinking; US-0077 owns the reflow properties and can add it then. The test drives 10^6 bytes plus 4 000 mutated corpus slices. |
| V4 | `differential::action_traces_agree_on_fuzz_seeds` over Ghostty's AFL++ seeds | `action_traces_agree_on_generated_streams` over four generated families (uniform random, escape-biased, corpus splices, corpus bit flips) | The Ghostty seed corpus is not vendored and vendoring an MIT corpus is a third-party-notices change this packet does not own. |
| V5 | tier 1 runs through `vt-bench` | a `#[test]` timing note reading fixtures from `ONETERM_VT_BENCH_FIXTURES` | `vt-bench` has no `--engine new` hook and adding one means an `oneterm-vt` dependency in `crates/tools`, a graph-policy edit and a collision with US-0074. `vt-bench fixtures --out <dir>` already writes the same fixtures the baseline used, so the note is comparable without touching the tool. |
| V6 | the UTF-8 carry is "ported from the reference verbatim" | the carry consumes only the **first** character's bytes where the reference consumes its whole valid prefix | The reference drops a character when the four-byte carry holds a completed codepoint, a second one, and then an error: `C5 93 \| 40 97` loses the `@`. Copying that would break the contract the same LLD states two sections earlier — an arbitrary chunking of one stream produces one action sequence — and the property test found it immediately. Regression: `utf8_carry_does_not_swallow_the_next_character`. Not visible to the oracle, which is fed whole buffers. |
| V7 | "Parameters past the 16th are dropped but their bytes keep accumulating into the 16th, **which is the reference behaviour**" | the bytes do accumulate, separators included — but this is **not** the reference behaviour | `vte`'s `action_osc_put_param` returns early once sixteen parameters exist, so the sixteenth stops growing at the seventeenth `;` and the tail is unreachable. The LLD's stated intent (OSC 8's `;`-joined URIs) only works if the tail is kept, so the intent wins over the citation, and the difference is filtered as **P9** in the differential with its reason. The independent verifier confirmed both halves and corrected the motive: the number that actually exceeds sixteen parameters is OSC 4/104, not OSC 8. |
| V8 | the UTF-8 carry is "ported from the reference verbatim" (second instance) | a C1 control completed across a chunk boundary is **executed**, where the reference prints it | `vte`'s `advance_partial_utf8` calls `print(c)` with no control filtering while its ground path executes `U+0080..=U+009F`, so `C2 | 9B` is CSI in one buffer and a printable glyph in two. The carry here goes through the same ground rules, so the meaning does not depend on where the read boundary fell. Found by the independent verifier; unreachable by the oracle, which is fed whole buffers. |
| V9 | P2 — `memchr3(ESC, LF, CR)` "so `\n` and `\r` leave the per-character loop" | `memchr(ESC)`, as the reference has | There is no per-character loop to leave: `print_run` already splits the validated run at every byte below `0x20`, so P2 bought nothing and cost a SIMD restart every eighty bytes of ordinary CRLF output. Measured by the independent verifier and reproduced: `plain_ascii` 0.88x to 1.08x, `scroll_region` 0.82x to 1.09x. **P2's row in the LLD is now stale**; the design owner owns the edit, as with P8 and P9. |

### Gaps

- `strip.rs` (`strip::tests::removes_sequences_keeps_text`) is named in the LLD's verification list
  but lives outside `parser/`; it is not written here.
- `cargo-fuzz` cannot run on this host (libFuzzer is unavailable on `x86_64-pc-windows-msvc` and the
  pinned toolchain is stable). `crates/vt/fuzz/` is committed and unbuilt; the in-tree byte-budget
  test is the substitute that Windows CI runs. Per R-47 this is not a packet gate.
- ~~`docs/agents/dependencies.md` § 3 and `docs/agents/structure.md` still have no `oneterm-vt`
  rows~~ — closed by the rework (finding M-A); both rows now list `memchr` and the `vte` oracle.
- Minors m-D, m-F, m-G, m-H and finding M-C are left open with their reasons in
  [`evidence/US-0073-verify.md`](evidence/US-0073-verify.md) § 7.
- Tier 1 is a single-machine, three-run median with the same variance the baseline records
  (up to 30 % between runs); read it as an order of magnitude.
- The parser has no `FeedStats` to increment yet — truncation and abort are reported through the
  `Dispatch` call arguments (`truncated`, `aborted`), which is where US-0079 reads them.

## Handoff

**The base moved while this packet was in flight.** It is committed on `f3cf1a5`, where
`crates/vt` held only the corpus; `feat/vt-engine` now carries US-0071, US-0074 and US-0075, so the
crate skeleton exists upstream. The merge is a four-line union and one deletion:

| File | Resolution |
| --- | --- |
| `crates/vt/Cargo.toml` | keep the upstream file; add `memchr.workspace = true` to `[dependencies]` and `vte.workspace = true` to `[dev-dependencies]` with this packet's comment about the oracle |
| `crates/vt/src/lib.rs` | keep the upstream file; add `pub mod parser;` |
| `Cargo.toml` | keep both sides: upstream already has the `crates/vt` member and the `oneterm-vt` path entry, this side adds `memchr = "2"` and `vte = "0.15"` to `[workspace.dependencies]` |
| `scripts/dependency-graph-policy.json` | upstream already has both entries; drop this side's |

Nothing under `crates/vt/src/parser/`, `crates/vt/tests/differential.rs` or `crates/vt/fuzz/`
overlaps with any other packet.

US-0076 implements `Dispatch` on `Handler`, answers `osc_allows_large` from `Config::osc_claims`
(deviation V1), narrows `pub mod parser` to `pub(crate)`, and rules on V2 — whether `DEL` in the
ground state is executed, as the LLD's state table says, or printed, as the reference does.
