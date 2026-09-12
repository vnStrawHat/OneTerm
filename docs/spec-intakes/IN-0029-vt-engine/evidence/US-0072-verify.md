# US-0072 — independent verification

Verifier: independent agent (not the implementer)
Date: 2026-09-12
Branch: `worktree-agent-ad8ab3207833ccc87` @ `b79e6e7`, worktree
`.claude/worktrees/agent-ad8ab3207833ccc87`
Baseline: `feat/vt-engine` (`852206d`)
Reference spec: `low-level-design/testing-and-bench.md` **as of the main checkout** (revised after
the implementer branched: per-recording cell-level `expected-diffs.toml`, deviation ids `C1`-`C11`,
bless only by the old engine).

## Verdict

**Merge after fixes.** Everything the packet claims to have measured reproduces, including two
results that contradict the design. One dependency-policy violation (S/M) and five documentation /
test-coverage nits (S) should land first. Nothing found is a correctness defect in the harness.

## Pass / fail table

| # | Check | Result |
| --- | --- | --- |
| 1 | Diff scope; PTY code untouched | **pass** |
| 2 | `pwsh scripts/ci-local.ps1` green; raw test totals | **pass** (1146 / 0 / 5, matches the packet) |
| 3a | `vt-corpus check` on all 45 | **pass** (45 passed, 0 failed) |
| 3b | `cross-check --grid-json` against upstream | **pass** (45 of 45 match) |
| 3c | Three bless guards | **pass** (all three refuse, exit 1) |
| 3d | Tamper one cell -> readable row/col diff | **pass** (tool and `cargo test` both fail) |
| 3e | `expected-diffs.toml`: declared / undeclared / stale / unknown id | **pass** (all four behave as the LLD specifies) |
| 4 | `vt-bench all` end to end, JSON parses, five tiers, numbers plausible | **pass** |
| 5 | Corpus vendoring, attribution, `.gitattributes`, size, no `grid.json` | **pass** |
| 6 | Code review against `code-style.md` | **pass with findings** (F1 dependency policy, F2 missing tests) |
| 7 | Recording-risk report reproduces | **pass** (byte-identical, plus independent confirmation) |
| 8 | Packet completeness; `harness.db` row | **pass with findings** (F3) |

## 1. Diff scope

```
$ git diff feat/vt-engine...HEAD --stat | tail -1
253 files changed, 14249 insertions(+), 2 deletions(-)

$ git diff feat/vt-engine...HEAD --name-only | awk -F/ '{print $1"/"$2}' | sort -u
.gitattributes            .github/workflows        Cargo.lock       Cargo.toml
THIRD-PARTY-NOTICES.md    crates/tools             crates/vt
docs/agents               docs/license-analysis.md docs/spec-intakes
scripts/third-party-notices.py

$ git diff feat/vt-engine...HEAD --name-only | grep -i "pty\|doom\|sftp-dev"
(no output)

$ git diff feat/vt-engine...HEAD --name-only -- crates/tools/src/bin/
crates/tools/src/bin/vt-bench.rs
crates/tools/src/bin/vt-corpus.rs
```

`pty-throughput.rs`, `doom-fire.rs` and `sftp-dev-server.rs` are untouched; `crates/vt/` gains data
only (226 files, no `.rs`, no manifest). Nothing outside the packet's declared scope.

## 2. Quality gate and raw test totals

`pwsh scripts/ci-local.ps1` in the worktree: **exit 0**, every step green —
`cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace`,
`verify-dependency-graph.py` (19 packages), `check-doc-paths.py` (116 paths / 10 documents),
`test_check_english.py`, `check-english.py` (649 files), `completion-catalog.py validate`,
`third-party-notices.py --check` ("THIRD-PARTY-NOTICES.md is up to date"), then
`ci-local: all checks passed.`

Raw totals, summed over every `test result:` line of a separate unfiltered
`cargo test --workspace`:

```
50 result lines
passed=1146 failed=0 ignored=5
```

Exactly the packet's claim (1146 / 0 / 5 over 50 lines).

## 3. `vt-corpus`

```
$ ./target/release/vt-corpus.exe check
...
45 recordings, 45 passed, 0 failed

$ vt-corpus bless --engine new
vt-corpus: the new engine never blesses: the expectations are what it is measured against, and a
self-blessed gate proves only self-consistency (R-58)                       exit=1

$ vt-corpus bless --engine old --filter sgr
vt-corpus: sgr is already blessed and frozen; re-blessing needs --deviation <id> so the expectation
diff is reviewable with a stated reason                                     exit=1

$ vt-corpus bless --engine old --deviation Z9 --filter sgr
vt-corpus: unknown deviation id "Z9": it must name a row in the corrections or deviation tables of
dispatch-and-modes.md or grid-and-scrollback.md                             exit=1
```

Cross-check. Upstream's `ref/<name>/grid.json` (45 files, 45 MB) was copied out of
`<CARGO_HOME>/git/checkouts/alacritty-20195d12a03fa0c5/fcf32fe/alacritty_terminal/tests/ref/` and
flattened to `<name>.json` in a scratch directory, which is the layout `load_grid_json` expects:

```
45 recordings, 45 match upstream `grid.json`, 0 differ
```

Tamper test — `selective_erasure/grid.expect`, row 2 col 1, `0042` -> `0058`:

```
FAIL  selective_erasure
        grid: row 2 col 1 content: expected "0058", got "0042"
1 recordings, 0 passed, 1 failed                                            exit=1

$ cargo test -p oneterm-tools --test corpus_check
test the_alacritty_reference_corpus_matches_its_frozen_expectations ... FAILED
panicked at crates\tools\tests\corpus_check.rs:49:5:
selective_erasure:
  grid: row 2 col 1 content: expected "0058", got "0042"
```

The diff names the file, the row, the column and the field. Restored with `git checkout --`.

`expected-diffs.toml` mechanism, written by hand for `selective_erasure` in the LLD's shape
(`deviation`/`rows`/`cols`/`fields`/`reason`), against the tampered cell:

| Case | Result |
| --- | --- |
| A. declared window + declared field, real difference | `ok selective_erasure (1 declared differences: {"C1": 1})` — **pass**, exit 0 |
| B. window moved to `cols = "5"` | `FAIL` — `stale declared diff: C1 ...` **and** `grid: row 2 col 1 content: expected "0058", got "0042"`, exit 1 |
| C. expectation restored, window kept | `FAIL` — `stale declared diff: C1 (grid, rows 2, cols 1, fields ["content"]) declared a difference that did not happen`, exit 1 |
| D. `deviation = "Z9"` | `vt-corpus: ...expected-diffs.toml: unknown deviation id "Z9" (not a row in dispatch-and-modes.md or grid-and-scrollback.md)`, exit 1 |

File removed afterwards; `git status --porcelain` clean; `check --filter selective_erasure` green
again. `KNOWN_DEVIATIONS` was cross-read against the two design tables: it is exactly the live rows
(C1-C11, D1/D2/D4/D7-D10/D12-D15, G1/G2/G3/G6/G7) and correctly omits the *superseded* rows
D3/D5/D6/D11 and the retired G4/G5.

## 4. `vt-bench`

`vt-bench all --mib 2` completes in **3.75 s** and prints all five tiers.
`vt-bench all --mib 2 --json --out <file>` parses as JSON with
`tiers = {tier1_parser(10), tier2_grid(10), tier3_render(10), tier4_resize(3), tier5_memory(4)}` —
the same shape and row counts as the committed `US-0072-bench-baseline.json` (which records
`mib_per_fixture = 100`). Note the flag is `--json`, not `--format json`.

Plausibility against the committed baseline (evidence run at 100 MiB, this run at 2 MiB):

| Tier | Evidence (100 MiB) | Fresh (2 MiB) |
| --- | --- | --- |
| 1 parser | 65.9 - 1345.7 MiB/s | 95.1 - 1602.0 MiB/s |
| 2 parse+grid | 38.5 - 180.8 MiB/s | 43.3 - 279.1 MiB/s |
| 3 snapshot | 21.1 - 42.3 us/frame | 18.2 - 20.2 us/frame |
| 4 resize @100k | 53182 / 39146 us | 29934 / 30555 us |
| 5 heap @10k rows | 41380491 B, 4138 B/row (all four contents) | **identical** |

Same order of magnitude and the same ordering of fixtures; the tier-2 and tier-4 spreads are wider
than the packet's stated 30 % but are explained by the much smaller working set and by the variance
the packet already records. Tier 5 is deterministic and reproduces to the byte — expected, since an
`alacritty_terminal` row allocates `columns` cells regardless of content.

## 5. Corpus vendoring

- `crates/vt/tests/corpus/NOTICE`: Apache-2.0 text, upstream + via URLs, revision
  `fcf32feacb367b75ec84dd40f041e4fd411d3cc1`, copyright line, and an explicit "changes made"
  section. Present and complete.
- `docs/license-analysis.md` § 3 and `THIRD-PARTY-NOTICES.md` § 2.1 both carry the row;
  `scripts/third-party-notices.py` carries the same block, and
  `python scripts/third-party-notices.py --check` reports **up to date**.
- `.gitattributes`: `crates/vt/tests/corpus/** -text -whitespace`.
  `git check-attr text whitespace` on `recording` and `grid.expect` both report `unset`;
  `git ls-files --eol` shows index == worktree for the CRLF-carrying binary recording.
- Size: `git ls-tree -r -l HEAD crates/vt/tests/corpus` -> **2 973 629 bytes in 226 files**
  (= 2.84 MiB, the packet's figure). 45 directories, 5 files each.
- `git ls-files crates/vt | grep grid.json` -> no match. No upstream `grid.json` committed.

## 6. Code review

Good:

- No `unwrap()` and no `panic!` anywhere in the new non-test code. The single `.expect()` is
  `Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("crates/tools has a parent")` — a
  compile-time invariant, not an I/O path. Every `fs::read`/`fs::write`/parse is
  `anyhow::Context`-wrapped with the path in the message.
- The one `unsafe` block (`CountingAllocator: GlobalAlloc`) carries a `// SAFETY:` comment naming
  the invariant, as `code-style.md` requires; the `realloc` counter update is correct on the
  null-return path.
- Module layout follows the project idiom (`#[cfg(test)] #[path = "corpus_tests.rs"] mod ...`).
  Scoped, one concern per file. Both binaries return `ExitCode` rather than panicking.
  `clippy --all-targets -D warnings` is clean, so no dead code.
- 14 unit tests in `corpus_tests.rs` cover the deviation mechanism thoroughly: RLE round trip
  (including an empty row), undeclared difference fails, declared window passes, window covers only
  the cells / only the fields it names, stale window fails, geometry difference short-circuits the
  cell walk, state key-prefix matching, unknown id / unknown field / missing range refused, and the
  design's own TOML example parses. Plus the integration drift gate.
- No dependency is added to the linked application graph: `anyhow`, `serde`, `serde_json` were
  already declared, and the `Cargo.lock` diff is four lines inside the `oneterm-tools` node only.

### F1 (S/M) — `toml` is a dependency the policy forbids without a decision

`Cargo.toml` gains `toml = "1"` in `[workspace.dependencies]`. But
`docs/agents/dependencies.md` § 3 states verbatim:

> Do not re-add without a design decision: `tracing` / `tracing-subscriber`, `directories`,
> **`toml`**, `russh-cryptovec`, `ssh-key`, `smol`, or `rust-i18n`.

The packet's **Decisions** section says "None new", and `docs/agents/dependencies.md` is not in the
diff — even though `testing-and-bench.md` § 7 explicitly assigns "the `docs/agents/dependencies.md`
§ 3 rows for the new declarations" to `US-0072`. Mitigating: `crates/tools` is never shipped and
`toml` was already in `Cargo.lock`. Fix: record the decision (or cite the LLD ruling that the
declared-diff file is TOML) **and** add the § 3 row / amend the "do not re-add" line.

### F2 (S) — the cross-check normalisation has no unit test

`LinkIds::normalize` (hyperlink-id renumbering), the `raw.zero != 0` rejection (trap 45 rezero),
the `raw.len` truncation, `parse_flags` alias resolution and `percent_encode` have **no** test.
They are proved only by the once-only cross-check run. Acceptable for `corpus_upstream` (dead after
`US-0072`), but `LinkIds` and `percent_encode` also run in `corpus_replay` on every bless and every
gate run. Three small tests would close it.

### F3 (S) — stale wording in the packet

`Scope` and `Acceptance` still say the overlay is **`deviations.json`, keyed by deviation id**; the
code and the revised LLD use a per-recording `expected-diffs.toml`. The same acceptance box is
ticked "exists", but no such file exists anywhere in the corpus (the Gaps section says so honestly).
Reword both to match what shipped.

### F4 (S) — the dispatch LLD table now contradicts the measurement

`grid-and-scrollback.md` has absorbed the C1-C4 / C8 / C10 answers, but
`dispatch-and-modes.md` has not:

- `C9` still reads "none expected; the reference ignores it" — measured: `grid_reset` sends one
  `CSI ! p`, so this is the one correction certain to move `grid.expect`.
- `C11` still reads "`sgr`, `underline` — the old engine drops these bits, so the cells differ in
  `attrs`" — measured: **none of the 45** sends `SGR 5/6/53/55`, independently confirmed below.
- `C5`, `C6`, `C7` still read "if ... the grep confirms".

The packet correctly records this as a handoff gap, but leaving the two tables in opposite states is
the failure mode the gap was meant to prevent. Copy the five rows.

### F5 (S, nits)

- `bless --deviation <id>` without `--filter` re-blesses **all 45** recordings: the guard validates
  that the id is known, never that it relates to the recording being rewritten. Requiring `--filter`
  alongside `--deviation` would make the escape hatch as narrow as the design intends.
- `vt-bench --help` (as the first argument) is parsed as a *command*, so it prints the report header
  instead of the usage line; `vt-bench all --help` works. `vt-corpus` handles both.
- `docs/agents/structure.md` § 3 lists the `tools` dependencies as
  "`alacritty_terminal`, `russh`, `polling`, `toml`" — `serde`, `serde_json` and `anyhow` are
  missing.

## 7. Recording-risk report

`vt-corpus grep-deviations` re-run: the table is **byte-identical** to the committed
`US-0072-recording-risk-raw.md`. Independently re-derived from the raw recording bytes with a
separate regex scanner (not the implementer's parser):

```
C9  (CSI ! p)         grid_reset -> 1             # exactly one recording, one occurrence
C11 (SGR 5/6/53/55)   (no output across all 45)   # none, with 38/48/58 arguments skipped
C1  (CSI Ps P)        8 recordings: decaln_reset(1,max1) deccolm_reset(1,max15)
                      delete_chars_reset(12,max1) erase_chars_reset(1,max21)
                      insert_blank_reset(1,max1) region_scroll_down(1,max4)
                      scroll_up_reset(1,max11) underline(4,max1)
C3  (CSI r + S/T/L/M) 2 recordings: vim_24bitcolors_bce, vttest_insert
```

All four claims confirmed: **C9 is not free** (contradicting the design), **C11 is free**
(contradicting the design), **C1 touches eight recordings**, **C3's candidates are
`vim_24bitcolors_bce` and `vttest_insert`**. The parser-level approach is the right one and the SGR
walk correctly steps over extended-colour arguments in both separator forms.

## 8. Packet completeness and the harness record

Every heading of `docs/templates/work.md` is present and filled; the only differences are the
concrete title and seven extra `###` subsections under Evidence and Gaps. Acceptance boxes are
ticked only where evidence exists, except the `deviations.json` wording in F3. The Gaps section is
unusually honest (no `expected-diffs.toml` in anger, `vt-diff`/`vt-fixtures` not built, tier 5 is
live heap not RSS, no title stack, unverified CI job, bench variance).

`harness.db` in the main checkout (read-only sqlite3):

```
id=US-0072  status=implemented  risk_lane=high_risk
title=Benchmark and parity harness in crates/tools
contract_doc=.../low-level-design/testing-and-bench.md
packet_doc=.../US-0072-bench-and-parity-harness.md
unit_proof=1 integration_proof=1 e2e_proof=0 platform_proof=0
verify_command=pwsh scripts/ci-local.ps1  last_verified_result=pass  (2026-09-12T18:30)
```

The `evidence` and `notes` columns match the packet, name the branch as unmerged, and record the
same gaps — including the C9 / C11 contradictions with the design.

## Fix list

| Fix | Size |
| --- | --- |
| F1 — `toml` is on the "do not re-add without a design decision" list: record the decision and add the `dependencies.md` § 3 row (the LLD assigns that row to this packet) | S/M |
| F2 — unit-test the cross-check normalisation: hyperlink-id renumbering, `raw.zero != 0` rejection, flag-alias parsing | S |
| F3 — packet Scope / Acceptance still say `deviations.json`; reword to `expected-diffs.toml` and untick "exists" | S |
| F4 — copy the measured C5-C7 / C9 / C11 answers into `dispatch-and-modes.md` (the grid table is already updated; the dispatch table now contradicts the evidence) | S |
| F5 — nits: require `--filter` with `--deviation`; `vt-bench --help` as a command; `structure.md` tools dependency list | S |
