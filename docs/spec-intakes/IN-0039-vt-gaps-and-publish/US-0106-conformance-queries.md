# Work: DECRQCRA, DECRQSS and XTGETTCAP are answered, and esctest runs

ID: US-0106
Intake: [`IN-0039`](IN-0039.md)
Created: 2026-09-15

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

- Change type: **new capability** (three answers the engine has never given, and a readback gate)
- Risk lane: high_risk -- both triggers: a public contract (new reply bytes are a contract under
  the crate's own clause 6) and a new capability at a trust boundary (screen readback)
- Spec Intake, when required: [`IN-0039`](IN-0039.md)

## Outcome

Three queries the engine parses, counts and never answers are answered, and an outside conformance
harness can finally score this engine.

The evaluation's gap 5:

> `DECRQCRA` is missing, so there is no third-party conformance score. ... `esctest` reads the
> screen back with rectangle checksums, so without `CSI * y` the harness cannot run, and no
> external number exists for this engine. `DECRQSS` and `XTGETTCAP` are absent for the same family
> of reasons and break tmux and neovim capability probing. Implementing `DECRQCRA` alone converts
> "we test ourselves thoroughly" into a number an outsider can compare against alacritty.

After this packet an `esctest` matrix exists, published as a CI artifact and pasted into Evidence,
and guide chapter 11's "Known gaps" table is rewritten from what the run actually found rather than
from what was never attempted.

## Scope

- [x] In scope:
  - A bounded DCS payload buffer, reusing the existing `DCS_MAX_BYTES` ceiling and abort path.
  - `DECRQCRA` (`CSI * y`) with the checksum variant pinned in the design, behind
    `Config::allow_screen_readback` (default **false**).
  - `DECRQSS` (`DCS $ q`) for `m` (SGR), `r` (`DECSTBM`), `SP q` (`DECSCUSR`), `" q` (`DECSCA`) and
    `" p` (`DECSCL`); everything else gets the invalid reply.
  - `XTGETTCAP` (`DCS + q`) over a fixed, compiled-in capability table, bounded and counted.
  - `#[non_exhaustive]` on `Config`, which the accepted IN-0038 design says should already be there.
  - `crates/tools/src/bin/vt-esctest.rs`, `#[cfg(unix)]`.
  - A Linux CI job that runs `esctest`, **report-only, never gating**.
  - Guide chapter 11 rewritten: its "Known gaps" table loses three rows and gains whatever the run
    finds.
- [x] Out of scope:
  - **`CSI Ps * x` (`XTERM_CHECKSUM`)**, xterm's runtime checksum-variant selector. One variant is
    implemented and pinned; xterm has a selector because it had its own history to reconcile.
  - **`DECRQSS` for `DECSLRM`, `DECSASD`, `DECSACE`, `DECSCPP`, `DECSNLS`.** The engine does not
    have those features, and answering would be the "claim a capability that does not exist"
    failure this whole intake is about. They take the invalid reply, which is the honest one.
  - **Left-right margins**, and therefore the `esctest` groups that need them. They fail correctly.
  - **Reading a terminfo database.** `XTGETTCAP` answers from a compiled-in table and touches no
    file, environment variable or network.
  - **Making `esctest` a gate.** `IN-0029`'s existing rule: conformance is a report.
  - **Running `esctest` on Windows.** The harness is `#[cfg(unix)]`.

## Acceptance

- [x] **Each query answers the exact bytes, proven by feeding the exact request.** One test per
      arm, named after the sequence, asserting the reply byte for byte. The `DECRQCRA` set includes
      at minimum: a one-cell rectangle over a known character; a blank cell (answers `0020`); a
      rectangle clamped from outside the grid; a reversed rectangle (`Pb < Pt`, empty, no
      underflow); origin mode set (rows relative to the scrolling region); and the gate closed.
- [x] **The gate is closed by default and is the status quo.** With `Config::default()`,
      `CSI 1;0;1;1;1;1*y` produces **no** `VtEvent::Reply` and increments
      `FeedStats::unhandled_sequences` -- byte-identical to the engine's behaviour on `main`.
      Asserted, not assumed.
- [x] **The checksum variant is pinned where a reader sees it.** A doctest in guide chapter 11
      feeds a known rectangle and asserts the four hex digits, so the number cannot change without
      a documentation diff. The chapter states the variant in words beside it: positive sum of
      Unicode scalar values, attributes excluded, unwritten cells counted as `U+0020`, no trimming,
      masked to 16 bits -- and states that a program written against xterm's default (negated, with
      attributes) will disagree.
- [x] **`DECRQSS` for SGR round-trips as a property, not a golden string.** Feed an SGR sequence,
      request it back, feed the answer into a second fresh terminal, assert the two cell templates
      are equal. Run over a set of SGR states including 256-colour, truecolour, colon
      sub-parameters and the underline styles.
- [x] **`XTGETTCAP` ceilings hold at the boundary.** 16 names answered; a 17th dropped and counted;
      a 128-byte name dropped; odd-length hex answered unknown; non-hex answered unknown; none of
      them panics and none allocates unboundedly.
- [x] **The DCS payload buffer is bounded and reused.** Bounded, but **not by
      `DCS_MAX_BYTES`** -- see the deviation in Evidence. The buffer has its own 8 KiB ceiling; a
      `CAN` abort still increments `FeedStats::aborted_dcs` and answers nothing. The
      single-allocation claim is **measured**, not argued: `ten_thousand_queries_reuse_one_buffer`
      records `Vec::capacity` after the first request and asserts it is unchanged after 10 000
      more.
- [ ] **`esctest` runs, and its matrix is attached.** **NOT MET -- the run has not happened.** The
      harness is Linux-only and this is a Windows host, so the matrix can only come from the CI
      job, which has not run: the branch is unpushed by instruction. The command line is pinned in
      the job (`--expected-terminal xterm --xterm-checksum 334 --max-vt-level 4`) and the "before"
      count is zero by construction, as the packet predicted. **This is the packet's one unmet
      acceptance criterion and it blocks acceptance.**
- [x] **The CI job cannot fail the build.** `continue-on-error: true` on the `vt-esctest` job,
      asserted mechanically (`yaml.safe_load` reports `True`), with a comment saying why. The
      **demonstration** half -- a green job with failing groups -- is part of the unmet criterion
      above.
- [~] **Guide chapter 11's "Known gaps" table is rewritten.** Rewritten, but **from the design and
      the code, not from a run**: the three rows for these sequences are gone and five rows are
      named in their place (left-right margins; the `DECRQSS` settings that take the invalid reply;
      `CSI Ps * x`; the VT220-level `DA1`; the `? 2027` / `WcsWidth` row that was already there).
      Every one is a gap this packet can name from the implementation. Whether `esctest` finds
      others is the open question the unmet criterion above holds.
- [x] **The corpus does not move.** 46 recordings replay byte-identically (`corpus_check`, part of
      `cargo test --workspace`).
- [x] **`cargo test -p oneterm-vt --features vt-paranoid` passes.**
- [x] **The surface diff is one `Config` field.** One line in each snapshot,
      `structfield allow_screen_readback`. **Not** the `#[non_exhaustive]` mark -- see the
      deviation in Evidence.
- [x] **Nothing about publishing changed.** `crates/vt/Cargo.toml` is untouched by this branch, and
      so is `scripts/verify-dependency-graph.py`.
- [ ] **The budget holds.** **NOT MET.** Over on every line; `git diff --stat` and the reasons are
      in Evidence.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/conformance-queries.md`](low-level-design/conformance-queries.md) -- this
  packet's owning design: the payload buffer, all three sequences, the checksum variant and its
  four-reason justification, the capability table, the ceilings and the `esctest` run plan.
- [`IN-0038/evidence/US-0102-esctest.md`](../IN-0038-embeddable-vt-core/evidence/US-0102-esctest.md)
  -- why the previous attempt stopped, and the three-step list this packet executes. **It is the
  document this packet closes**; it should gain a pointer here on merge.
- [`IN-0029/low-level-design/dispatch-and-modes.md`](../IN-0029-vt-engine/low-level-design/dispatch-and-modes.md)
  -- the CSI and DCS dispatch rules and the `DECRQM` honesty rule this packet must not break.
- [`IN-0029/low-level-design/testing-and-bench.md`](../IN-0029-vt-engine/low-level-design/testing-and-bench.md)
  section 6 -- "Conformance as a report, not a gate", which the CI job implements verbatim.
- [`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
  -- the semver promise (clause 6, reply bytes) and the `#[non_exhaustive]` doctrine, including the
  `Config` mark that was designed and never applied.
- `crates/vt/docs/guide/11-conformance.md` -- lists all three as known gaps and states that there
  is no `esctest` score. Both statements become false.
- `crates/vt/src/terminal/dispatch.rs` -- `dcs_hook`'s header comment names `DECRQSS` and
  `XTGETTCAP` as counted-unhandled; it describes the behaviour this packet replaces.
- `docs/osc-sequences-checklist.md` -- reviewed and **not changed**: it is about OSC, and nothing
  here is OSC. Recorded so the no-change is a decision.

### Documentation Action

**Update required.**

| Doc | Change |
| --- | --- |
| `crates/vt/docs/guide/11-conformance.md` | the "Supported" DCS paragraph, the "Known gaps" table (three rows out, the run's findings in), the "How conformance is checked" section (an `esctest` matrix now exists), and the new checksum doctest with its variant paragraph |
| `crates/vt/docs/guide/12-versioning.md` | clause 6's list gains `DECRQSS`, `DECRQCRA` and `XTGETTCAP`; the `#[non_exhaustive]` count gains `Config` |
| `crates/vt/CHANGELOG.md` | `### Added` for the three answers and the `Config` field; a note that the reply bytes are a contract from here on |
| `crates/vt/src/terminal/dispatch.rs` | `dcs_hook`'s header comment, which currently describes the old behaviour |
| [`IN-0038/evidence/US-0102-esctest.md`](../IN-0038-embeddable-vt-core/evidence/US-0102-esctest.md) | a closing pointer to this packet. The evidence record is not rewritten -- it was accurate when written |
| `.github/workflows/ci.yml` | the new report-only job, with a comment saying why it never gates |

Reason: guide chapter 11 currently tells a reader, correctly, that this engine has no `esctest`
score and cannot get one. After this packet that is false in both halves, and chapter 11 is the
document an evaluator reads to decide whether to trust the crate.

### Reconciliation

Before completion: list the docs changed, and confirm by grep that no file under `crates/vt/`
still says `DECRQCRA`, `DECRQSS` or `XTGETTCAP` is unimplemented.

**Done.** Docs changed: `crates/vt/docs/guide/11-conformance.md` (the readback gate, the pinned
checksum variant with its doctest, the `DECRQSS` / `XTGETTCAP` answers, the rewritten gaps table,
the fourth conformance layer); `crates/vt/docs/guide/12-versioning.md` (clause 6, and why `Config`
is not marked); `crates/vt/CHANGELOG.md` (clause 6 and the `Added` entries);
`crates/vt/src/terminal/dispatch.rs` (`dcs_hook`'s header comment, which described the behaviour
this packet replaced); both API snapshots;
[`IN-0038/evidence/US-0102-esctest.md`](../IN-0038-embeddable-vt-core/evidence/US-0102-esctest.md)
(the closing pointer); and inline corrections in this intake's two low-level designs
([api-surface](low-level-design/api-surface.md), [conformance-queries](low-level-design/conformance-queries.md)).

`docs/osc-sequences-checklist.md` reviewed and **not changed**, as planned: it is about OSC, and
nothing here is OSC.

The grep is clean -- no file under `crates/vt/` still describes any of the three as unimplemented,
unanswered or uncountable:

```text
grep -rniE "(DECRQCRA|DECRQSS|XTGETTCAP)[^.]{0,80}(unimplemented|not implemented|\
counted unhandled|does not implement|never answered|cannot run)" crates/vt/
-> no matches
```

## Context

- `BUG-0058` already made `dcs_hook` route on (intermediates, final byte), so `DCS $ q` and
  `DCS + q` no longer open the Sixel decoder. The parsing half is done; only the answering half is
  missing.
- `dcs_put` today feeds the Sixel parser or discards the byte. It needs a second sink, not a second
  state machine.
- The checksum variant is the one xterm reaches with `checksumExtension: 7`. It is chosen because
  it is the only variant `esctest` scores without a per-cell correction: `escutil.py` compares a
  one-cell checksum against `ord(char)` and applies the negation inverse only under
  `--xterm-checksum < 279`, and `esc.py`'s `empty()` returns a space only under
  `--xterm-checksum >= 334`. Both conditions are met by this variant and by no other. The design
  carries the xterm source excerpt the bit meanings come from.
- `Config` is **not** `#[non_exhaustive]` today, although
  [`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
  says it should be. This packet adds a `Config` field and therefore discovers it. Every
  construction site in this repository already uses `Config { ..Config::default() }`, so the mark
  is compatible.
- xterm gates `DECRQCRA` behind `allowWindowOps` and requires `vtXX_level >= 4`; WezTerm gates it
  behind `enable_checksum_rectangular_area`, default off. This engine's `DA1` claims VT220, not
  VT420 -- worth knowing, and not worth changing: `DA1`'s reply is a contract and no client is
  known to gate `DECRQCRA` on it.

## Plan

- [ ] Add the DCS payload sink to the terminal state; extend `dcs_hook`, `dcs_put`, `dcs_unhook`;
      clear it at `RIS` and `DECSTR`.
- [ ] `Config::allow_screen_readback` plus the `#[non_exhaustive]` mark.
- [ ] `DECRQCRA`: parameter parsing with defaults, rectangle clamping, origin-mode handling, the
      checksum walk, the `DCS Pid ! ~ xxxx ST` reply.
- [ ] `DECRQSS`: the six arms and the default; the SGR serialiser is the bulk.
- [ ] `XTGETTCAP`: hex decode, the const table, hex encode, the per-name reply, the two ceilings.
- [ ] `crates/tools/src/bin/vt-esctest.rs` -- it cannot be compiled on the maintainer's host, so it
      is written against the CI job and the job is part of the same commit.
- [ ] The CI job, report-only, uploading the log as an artifact.
- [ ] Run it; read the matrix; rewrite guide chapter 11 from what it says.
- [ ] Chapter 12, CHANGELOG, `dcs_hook`'s comment, the IN-0038 evidence pointer.
- [ ] Regenerate both surface files.

## Decisions

No new decision record, and that is a judgement worth stating. The checksum variant is the kind of
choice future work must inherit -- changing it later breaks every harness that stored a number --
but it is a single implementation detail of one sequence in one crate, fully recorded with its
justification in
[`low-level-design/conformance-queries.md`](low-level-design/conformance-queries.md), which is an
accepted design document of a high-risk intake. A `DEC-` record would duplicate it.

If the owner rules that `allow_screen_readback` should default **true** (this intake's Open
Decision 2), **that** is a decision record: it would be a deliberate choice to give every program
in every OneTerm terminal a screen-readback primitive, and the reasoning should outlive this
packet.

## Verification Plan

- `cargo test -p oneterm-vt` -- one test per arm with exact bytes; the `DECRQCRA` edge set; the SGR
  round-trip property; the `XTGETTCAP` boundary set; the DCS abort path.
- `cargo test -p oneterm-vt --features vt-paranoid` -- the whole-history walk, because `DECRQCRA`
  reads the grid.
- `cargo test -p oneterm-vt --no-default-features` -- none of this is behind `pty`, so the
  transport-free build must have it all.
- The 46-recording corpus replay, byte-identical.
- The existing `cargo-fuzz` parser target, which gains the new DCS state for free. No new target.
- `python scripts/vt-public-api.py --check`, `--check-nameable`, `--diff-platforms`.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` -- chapter 11's new
  doctest compiles and runs.
- CI's "the embedder guide must stand alone" grep over the rewritten chapter 11.
- The Linux CI `esctest` job: matrix attached, job green even when tests fail.
- `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

Platform proof is **not** met. Its two halves were the Linux `esctest` job -- which has not run --
and the `--no-default-features` build, which passes (504 tests). Half a criterion is not the
criterion, so the box stays empty.

E2E proof is **not applicable**: nothing a OneTerm user can see changes, because the adapter does
not set the gate and OneTerm sends none of these queries. Record it as not applicable rather than
leaving it blank. Platform proof is the Linux `esctest` job plus the `--no-default-features` build.

## Evidence and Gaps

Branch `feat/vt-conformance-queries`, three commits on `c5ddad59`. Unpushed.

### The reply bytes, as implemented

| Request | Reply |
| --- | --- |
| `CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y`, gate open | `DCS <Pid> ! ~ <4 upper-case hex> ST` |
| the same, gate shut (the default) | nothing; `unhandled_sequences += 1` |
| `DCS $ q m ST` | `DCS 1 $ r <SGR parameters> m ST`, always starting `0` |
| `DCS $ q r ST` | `DCS 1 $ r <top> ; <bottom> r ST`, 1-based inclusive |
| `DCS $ q SP q ST` | `DCS 1 $ r <1-6> SP q ST`, the shape selector, blink included |
| `DCS $ q " q ST` | `DCS 1 $ r 0 " q ST` (`DECSCA`; the protected bit is set by nothing) |
| `DCS $ q " p ST` | `DCS 1 $ r 62 ; 1 " p ST` (`DECSCL`; the level `DA1` claims, 7-bit controls) |
| any other `DCS $ q ... ST` | `DCS 0 $ r ST` |
| `DCS + q <hex> ST`, known | `DCS 1 + r <hex name> = <hex value> ST`, one reply per name |
| `DCS + q <hex> ST`, unknown | `DCS 0 + r <hex name> ST` |

Worked examples, all asserted in `crates/vt/src/terminal/query_tests.rs` (40 tests):

```text
\x1b[1;0;1;1;1;1*y   over `A`   -> \x1bP1!~0041\x1b\\
\x1b[7;0;1;1;1;1*y   over blank -> \x1bP7!~0020\x1b\\
\x1bP$qm\x1b\\       fresh      -> \x1bP1$r0m\x1b\\
\x1bP$qr\x1b\\       24 rows    -> \x1bP1$r1;24r\x1b\\
\x1bP$qs\x1b\\       DECSLRM    -> \x1bP0$r\x1b\\
\x1bP+q544e\x1b\\    `TN`       -> \x1bP1+r544e=787465726D2D323536636F6C6F72\x1b\\
```

`Config::allow_screen_readback` defaults to **`false`**. `crates/terminal` does not set it; the
only construction in the repository that sets it is `crates/tools/src/bin/vt-esctest.rs`.

### Two deviations from the accepted design

**1. `Config` is NOT `#[non_exhaustive]`, and cannot be.** The design
([`low-level-design/api-surface.md`](low-level-design/api-surface.md) § "Newly `#[non_exhaustive]`",
inheriting `IN-0038/low-level-design/api-surface.md`) states that the mark is "cheap and
compatible" because "every construction site in this repository already uses
`Config { ..Config::default() }`, which is the supported form under the mark". **That premise is
false.** Rust forbids a struct expression for a non-exhaustive struct outside the defining crate,
functional-update syntax included. The mark was applied, and the compiler rejected it:

```text
error[E0639]: cannot create non-exhaustive struct using struct expression
  --> crates\vt\tests\product_name_hostile.rs:14:9
   |
14 | /         Config {
15 | |             product_name: name.map(|name| name.to_owned().into()),
16 | |             ..Config::default()
17 | |         },
   | |_________^
```

It breaks this crate's own integration tests, the `headless` example, six guide doctests and
`crates/terminal`. Marking `Config` would mean replacing struct-literal construction with setters
across the whole surface -- a different packet, a worse API, and outside this budget. So the mark
is **not** applied, guide chapter 12's count stays at eight, and the chapter gains a paragraph
saying why the type a reader would expect to be marked is not. A new `Config` field stays a
**minor** bump under clause 1. **The design document and `IN-0038`'s should be corrected; this
packet did not rewrite an accepted design it does not own.**

**2. The payload buffer has its own 8 KiB ceiling**, where the design says it "inherits" the
parser's `DCS_MAX_BYTES` and "adds no new limit". Inheriting was unsafe in combination with the
design's own rule 4: the buffer is `clear()`ed rather than dropped so the common case allocates
once, so inheriting a 16 MiB ceiling would let one hostile `DCS + q` make the terminal retain
16 MiB for the rest of the session. 8 KiB is twice the largest answerable request
(`16 * 128 * 2 + 15` = 4 111, asserted by `every_answerable_request_fits_the_payload_ceiling`). A
payload that reaches the ceiling answers nothing and is counted, which is the same treatment the
`XTGETTCAP` ceilings give. `verify_one_mib_intermediate_payload_buffers_nothing` (the `BUG-0058`
verifier's test) still passes with its original numbers, for this new reason.

A third, smaller choice worth recording: **a non-hex `XTGETTCAP` request is not echoed back.** The
design has the name echoed into the reply, and the reply is a DCS string -- so a "name" carrying
`ESC \` would end the reply early and spill its tail onto the program's input as text. Only hex
digits are echoed; anything else echoes empty (`hex_echo`, with
`hex_echo_refuses_anything_that_could_end_the_reply`).

### Verification run here

| Check | Result |
| --- | --- |
| `cargo test -p oneterm-vt` | 528 pass, 0 fail (495 lib + integration) |
| `cargo test -p oneterm-vt --no-default-features` | 504 pass, 0 fail |
| `cargo test -p oneterm-vt --features vt-paranoid` | pass (via `ci-local`) |
| `cargo test -p oneterm-vt --doc` | 38 pass -- the guide's chapters, including chapter 11's new doctests |
| `cargo test --workspace` (includes `corpus_check`) | pass; the 46-recording corpus is byte-identical |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `python scripts/vt-public-api.py --check --no-doc` | unchanged after `--update` |
| `python scripts/vt-public-api.py --diff-platforms` | 6 lines, all inside `oneterm_vt::pty` |
| `pwsh scripts/ci-local.ps1 -Full` | see the run log referenced below |

### Budget: over, on every line

```text
 .github/workflows/ci.yml                    |  83 ++      (budget +40)
 crates/tools/Cargo.toml                     |   9 +
 crates/tools/src/bin/vt-esctest.rs          | 220 ++      (budget +140 for both)
 crates/vt/CHANGELOG.md                      |  26 +-
 crates/vt/docs/guide/11-conformance.md      | 169 ++
 crates/vt/docs/guide/12-versioning.md       |  19 +-
 crates/vt/public-api.unix.txt               |   1 +
 crates/vt/public-api.windows.txt            |   1 +
 crates/vt/src/terminal/dcs_routing_tests.rs |  20 +-
 crates/vt/src/terminal/dispatch.rs          | 260 ++      \
 crates/vt/src/terminal/mod.rs               |  45 ++       > ~660 (budget +470)
 crates/vt/src/terminal/query.rs             | 369 ++      /
 crates/vt/src/terminal/query_tests.rs       | 584 ++      \
 crates/vt/src/terminal/terminal_tests.rs    |   6 +-       > ~610 (budget +380)
 14 files changed, 1770 insertions(+), 42 deletions(-)
```

Stated rather than explained away. Two honest causes and no third: roughly 40% of `query.rs` and of
the new `dispatch.rs` lines are doc comments, which this crate's conventions require and which the
budget did not account for; and the acceptance criteria themselves ask for more tests than 380
lines hold -- the `DECRQCRA` edge set alone is nine named tests, and the `XTGETTCAP` boundary set is
five. The CI job is 83 lines because it is a full job (toolchain pin, cache, fetch, run, summary,
artifact), not the 40-line increment the budget assumed. Nothing here was padded and nothing was
cut to fit; if the budget is the binding constraint, the tests are the only place with slack.

### Gaps

- **The checksum variant disagrees with xterm's default.** Deliberate, justified four ways in the
  design, and documented in the guide -- but it means a hypothetical program written against
  xterm's negated-with-attributes form gets a different number here. No such program is known;
  `DECRQCRA` is a harness primitive.
- **`esctest` has not been run. No matrix exists.** This is the packet's largest gap and the
  reason it is not acceptable as it stands. The harness is Linux-only, the host here is Windows,
  and the branch is unpushed by instruction, so the only place the run can happen is the CI job --
  which has never executed. Everything the packet says about conformance is therefore a claim about
  what the code does, proven by this repository's own tests, and **not** an outside number. The
  "before" count is zero by construction, as predicted: every rectangle assertion times out on
  `main`.
- **`vt-esctest.rs`'s pty half was never compiled.** The file carries a Windows stub, so
  `cargo check -p oneterm-tools` and `cargo clippy -p oneterm-tools --all-targets -- -D warnings`
  pass here -- but they compile the stub. `rustup target list --installed` reports only
  `x86_64-pc-windows-msvc`, so not even a cross `cargo check` was possible. The Unix `mod unix`
  body has been **read, not compiled**. That was the exact argument for not writing it in
  `US-0102`; it is written now because the answering half landed, but the risk it named is real and
  unretired until the CI job runs.
- **`esctest` is GPL-2.0.** Executed, never linked and never vendored -- the same relationship CI
  has with every other tool it runs. The job clones it from
  <https://github.com/ThomasDickey/esctest2> at run time; nothing is committed here.
- **The groups expected to fail** are named in guide chapter 11's rewritten table with their
  reasons (no left-right margins; VT220-level `DA1`; the `DECRQSS` settings that take the invalid
  reply; no `CSI Ps * x`). The chapter says in words that the artifact is a capability map and not
  a grade. That framing is in place **before** the first run, which is the right order.
- **The `ESCTEST_REF` pin is `master`, a branch, not a commit.** The job's own comment asks for a
  commit so a rewritten upstream test cannot silently change what the engine is scored against. A
  sha could not be chosen here without fetching the repository, which this host does not do. The
  first CI run should replace it with the sha it resolved.
- **The DCS buffer's single-allocation claim is measured, not argued** --
  `ten_thousand_queries_reuse_one_buffer` asserts `Vec::capacity` is unchanged across 10 000
  requests. Recorded because the packet explicitly allowed the weaker form.
- **`DECSCA` reports `0` and always will, until something sets the bit.** No `CSI Ps " q` is
  dispatched, so the answer is honest rather than useful. It is listed as supported because the
  reply is correct, not because the feature exists.

## Harness Delta

`harness.db` was **not** written by this task: no harness binary is available in this worktree and
the task forbids editing the database. The `IN-0039` intake row already exists as `id = 44`
(`document_number = 39`), so the story row below references it directly.

```python
#!/usr/bin/env python3
"""Insert the US-0106 story row. Point DB at the harness database and run once."""
import sqlite3

DB = "<path to harness.db>"

ROW = dict(
    id="US-0106",
    title="DECRQCRA, DECRQSS and XTGETTCAP are answered, and esctest runs",
    risk_lane="high_risk",
    contract_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/"
        "low-level-design/conformance-queries.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/US-0106-conformance-queries.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=0,
    evidence=(
        "feat/vt-conformance-queries, three commits on c5ddad59, unpushed. "
        "40 exact-byte tests in crates/vt/src/terminal/query_tests.rs; 528 tests "
        "pass by default and 504 with --no-default-features; the 46-recording "
        "corpus is byte-identical; both public API snapshots gain one line, "
        "Config::allow_screen_readback. NOT met: esctest has never run, so no "
        "outside conformance matrix exists, and the vt-esctest pty half compiles "
        "on no host available here. Two design deviations: Config is NOT "
        "#[non_exhaustive] (E0639 -- the accepted design's premise that "
        "functional-update syntax survives the mark is false), and the DCS query "
        "buffer takes its own 8 KiB ceiling rather than inheriting DCS_MAX_BYTES."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_result="pass",
    notes=(
        "allow_screen_readback defaults to false, per the coordinator. The only "
        "construction that opens it is crates/tools/src/bin/vt-esctest.rs. "
        "Checksum variant: xterm checksumExtension 7 (positive, no attributes, "
        "blanks counted, masked to 16 bits), pinned by a guide chapter 11 "
        "doctest. Acceptance is blocked on the esctest run."
    ),
    intake_id=44,
)

with sqlite3.connect(DB) as db:
    columns = ", ".join(ROW)
    placeholders = ", ".join("?" for _ in ROW)
    db.execute(f"INSERT INTO story ({columns}) VALUES ({placeholders})", tuple(ROW.values()))
print("inserted US-0106")
```

## Handoff

Independent of `US-0105` and `US-0107`; all three sit behind `BUG-0059` only. If `US-0105` lands
first, both regenerate the surface files and the second rebases its snapshot -- mechanical, not
semantic. Blocked on the owner for Open Decision 2 (the readback default) **at acceptance**, not at
start: the implementation is identical either way and only the default constant moves.
