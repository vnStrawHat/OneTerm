# Work: Decommission the vendored fork

ID: US-0087
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

- Change type: maintenance (the last slice of a migration; no product behaviour changes except the two cleanup rows below)
- Risk lane: high_risk — it removes a public licence claim (`THIRD-PARTY-NOTICES.md`), a CI job and a `[patch]` source, and the two cleanup rows touch the VT engine's cursor path
- Spec Intake, when required: `IN-0029`

## Outcome

The vendored `alacritty_terminal` / `vte` fork is gone from the repository and from
every manifest, script, CI job, notice and current-state document. `oneterm-vt` is the
only VT engine the workspace knows about. The test and bench harness keeps the parts
that measure the shipped engine and loses the parts that only existed to compare it
with the fork. The two cleanup rows `migration.md` § "Cleanup before decommission
(`US-0087`)" queued are implemented, each with a test.

## Scope

- [ ] In scope:
  - `git rm -r vendor/` (trees, `patches/`, `refresh.sh`, `README.md`).
  - Root `Cargo.toml`: the `exclude`, the `alacritty_terminal` and `vte` workspace
    dependencies, the two `[profile.*.package]` overrides, both `[patch]` blocks.
    `Cargo.lock` follows.
  - `deny.toml`: the `allow-git` entry for the fork and the two prose mentions.
  - `.github/workflows/ci.yml`: the `refresh.sh --check` step and the `vendor/**` path
    triggers. `scripts/ci-local.{sh,ps1}`: the `--full` step. `scripts/README.md` row.
  - `crates/terminal`: the `alacritty_terminal` dev-dependency and
    `tests/us0081_parity.rs`.
  - `crates/vt`: the `vte` dev-dependency, `tests/differential.rs`, and the oracle half
    of `tests/ext_differential.rs`.
  - `crates/tools`: `vt-diff`, `corpus_replay.rs` (old engine), `corpus_upstream.rs`
    (the `US-0072` cross-check), `vt-corpus bless` and `cross-check`, the `Engine` enum;
    `bench.rs` and `corpus_grep.rs` ported onto `oneterm-vt`.
  - The two cleanup rows: reverse wrap confined to the scroll / `DECOM` region, and LNM
    answering `DECRQM` through `Mode::inert_state`.
  - `THIRD-PARTY-NOTICES.md` regenerated from `scripts/third-party-notices.py`; `NOTICE`.
  - Current-state docs: `docs/agents/{dependencies,structure,crate-dependency-rules}.md`,
    `docs/terminal-backend.md`, `docs/PROJECT.md`, `README.md`, `AGENTS.md`,
    `docs/license-analysis.md`, `docs/README.md`, `scripts/README.md`.
  - Provenance doc-comments under `crates/` that name `vendor/...` paths: rewritten to
    name upstream, so attribution survives the deletion and no path dangles.
- [ ] Out of scope:
  - `IN-0029.md`, the HLD and every LLD — the design owner's. Stale lines are recorded
    in Reconciliation, not edited.
  - `docs/archive/**` and `docs/decisions/DEC-*.md` — historical records of what was
    true when they were written.
  - `crates/terminal/src/osc.rs`, `crates/terminal/src/backend/osc_router.rs`,
    `crates/vt/src/terminal/osc.rs`, `docs/osc-*.md`, `README.md`'s OSC section and
    `crates/completion/assets/**` — `US-0088` owns them and is in progress in another
    worktree.
  - The bundled ConPTY pair (`THIRD-PARTY-NOTICES.md` § 1) — `IN-0030`.

## Acceptance

- [x] `vendor/` does not exist and no tracked file references it. — `Test-Path vendor` is
      `False`; the only `vendor/` string left under `crates/` is one comment line in
      `crates/terminal/src/backend/osc_router.rs`, which `US-0088` owns (Handoff).
- [x] `grep -rn "alacritty_terminal" crates/ scripts/ Cargo.toml` names no dependency and
      no dangling path. — What it still prints is provenance comments citing the upstream
      file a port came from, the frozen corpus data, its `NOTICE`, and one notices line
      saying what was removed. Enumerated in Gaps 1; the `US-0088`-owned files are in
      Scope.
- [x] `cargo tree -p oneterm-app` and `cargo metadata` resolve with no `alacritty_terminal`
      and no `vte` package. — Both report zero matches; `Cargo.lock` has no `git+` source
      at all.
- [x] `pwsh scripts/ci-local.ps1` green, raw totals recorded. — Evidence § "The gate".
- [x] `python scripts/third-party-notices.py --check` green with the two fork rows gone
      and the corpus test-data row (and its `NOTICE`) intact.
- [x] `cargo run -p oneterm-tools --bin vt-corpus -- check --engine new` is 45/45 on
      `alacritty-ref/` and 1/1 on `oneterm/sixel_basic`.
- [x] `cargo run -p oneterm-tools --release --bin vt-bench -- all --mib 2` reports all
      five tiers against `oneterm-vt`.
- [x] Reverse wrap: `BS` at column 0 with `? 45` set does not leave the scroll region,
      proved by a test that sets `DECSTBM` and shows the cursor stays put. —
      `grid::tests::reverse_wrap_stays_inside_the_scroll_region`, plus three wire-level
      cases in `crates/vt/tests/us0087_cleanup_rows.rs` including a region top that is not
      row 0.
- [x] `DECRQM` for LNM (`CSI 20 $p`) answers `Reset` even after `CSI 20 h`, proved by the
      same mode-table walk that covers the private inert modes. — `Mode::ANSI` joins
      `Mode::PRIVATE` in `decrqm_answers_match_the_mode_table`, and the wire tests add the
      `l` case, the IRM control and an exhaustive `from_ansi` / `Mode::ANSI` cross-check.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` § "Deletion list",
  § "Cleanup before decommission", § "Documentation reconciliation", § "Notices and
  licensing" — the authoritative list of what goes and who owns each doc edit.
- `.../low-level-design/testing-and-bench.md` § 3, § 7 and its `US-0087` verification
  row — what the gate looks like afterwards: `vt-corpus check --engine new` only, bench
  tiers on the new engine only, the parser differential oracle retires.
- `.../low-level-design/parser.md` § "The differential oracle" — why the oracle exists
  and that it retires with the fork.
- `.../research/prior-art.md` § 6 — the corpus stays, with its `NOTICE`.
- `docs/agents/dependencies.md` § 1 / § 3 / § 4, `docs/agents/structure.md` § 1 / § 3,
  `docs/agents/crate-dependency-rules.md` R6 / R7 / R8 — they still name the fork as a
  live dependency.
- `docs/terminal-backend.md` § 4, `docs/PROJECT.md`, `README.md`, `AGENTS.md` § 2 / § 4,
  `docs/license-analysis.md`, `vendor/README.md` — the removal rows `migration.md`
  assigns to this packet.
- `scripts/third-party-notices.py` HEADER § 2, `NOTICE` — the licence claim being
  withdrawn.

### Documentation Action

Update required. `migration.md` § "Documentation reconciliation" assigns this packet the
**removal** rows in `docs/terminal-backend.md` § 4, `docs/agents/dependencies.md` § 1,
`docs/PROJECT.md`, `README.md`, `vendor/README.md`, `THIRD-PARTY-NOTICES.md` and
`NOTICE`; `python scripts/check-doc-paths.py` additionally forces every back-ticked
`vendor/...` path out of `docs/architecture.md`, `docs/agents/*.md`, `docs/README.md`,
`README.md` and `AGENTS.md` before CI can pass. `docs/agents/structure.md`,
`docs/agents/crate-dependency-rules.md`, `docs/license-analysis.md` and
`scripts/README.md` describe the same removed things and are reconciled with them.

Reason: the fork is a licence claim and a build input, not an implementation detail. A
doc that still says the terminal engine is a patched fork is wrong about what OneTerm
ships, and `THIRD-PARTY-NOTICES.md` saying so is a false attribution.

### Reconciliation

**Changed by this packet:**

| Doc | What changed |
| --- | --- |
| `THIRD-PARTY-NOTICES.md` (via `scripts/third-party-notices.py`) | § 2's two fork rows and the "pristine upstream plus the listed patch set" prose removed; § 2.1 promoted to § 2 (the corpus row and its `NOTICE` stay); a new § 2.1 "Derived algorithms (no source copied)" added; the `vendored fork` source label replaced |
| `NOTICE` | the "lightly patched fork of alacritty_terminal and vte" bullet replaced by the corpus attribution; the `avt` reflow bullet added |
| `docs/license-analysis.md` | the reuse rule, the corpus bullet, a new `avt` bullet, and the fork row in the Zed-crate table |
| `docs/agents/dependencies.md` | § 1 table and rule 6, § 3's two engine rows, § 4's pointer to the deleted vendor readme |
| `docs/agents/structure.md` | the `vendor/` subtree, the `crates/terminal` and `crates/tools` manifest notes, the deleted `engine_shim.rs` line, the `corpus_replay` / `corpus_upstream` lines, the `vt` and `tools` responsibility rows, four "alacritty-free" phrasings |
| `docs/agents/crate-dependency-rules.md` | the L0 paragraph, R6, R7, and two `cargo tree` comments |
| `docs/terminal-backend.md` | the header, core decisions 2-5, principle 6, § 4 (rewritten), the § 5 compatibility-surface paragraph `US-0085` left stale, the § 6.2 sketch notes, the § 13 risk row and two § 14 rows |
| `docs/PROJECT.md` | the stack bullets and the vendored-crates invariant |
| `README.md`, `AGENTS.md` | the "Powered by" line, the VT-rendering bullet, § 4's optional-checks paragraph, the quick-reference row |
| `docs/README.md`, `scripts/README.md` | the vendor-readme and refresh rows; the `check-doc-paths.py` description |
| `scripts/check-doc-paths.py` | stops scanning a `vendor/` prefix that cannot exist |
| `AGENTS.md` § 4 | the tenth gate step (`vt-paranoid`) was missing from the command list |
| `docs/sftp-browser-design.md` | its crate tree named the fork as the local shell's engine |
| `deny.toml` | the dead `allow-git` list |
| `crates/vt/tests/corpus/**/grid.expect` | one comment line in each of 46 frozen files: "regenerate with `vt-corpus bless --engine old`" named a deleted subcommand. No data line moved — proof in Evidence |
| Source provenance comments | `crates/{app,core,pty,terminal,vt}` — 20 files; every dangling vendor path became the upstream path it names, so the Apache-2.0 notices in `crates/pty` and the ported-from references in `crates/vt` still resolve |

**Owed since `US-0077` and paid here.** That packet's Gaps recorded the `avt` attribution
as "assigned to `US-0087`, the packet that already opens both files". `NOTICE` and
`THIRD-PARTY-NOTICES.md` § 2.1 now carry it, and `docs/license-analysis.md` records why an
algorithm is not source.

**Deliberately not changed:**

- `docs/archive/**` and `docs/decisions/DEC-*.md` — records of what was true when written.
- `crates/terminal/src/osc.rs` (3 lines) and `crates/terminal/src/backend/osc_router.rs`
  (1 line) — `US-0088` owns them. See Handoff.
- Every **data** line of the frozen corpus, the captured `recording` bytes, the
  `state.expect` files and `crates/vt/tests/corpus/NOTICE` — the gate and its attribution.
  One `grid.expect` *comment* line was corrected; nothing else in that tree moved.

**For the design owner** (`IN-0029.md`, the HLD and the LLDs are theirs, not this packet's):

| File:line | Now stale |
| --- | --- |
| `IN-0029.md:161` | the `US-0087` checkbox is unticked |
| `high-level-design.md:71` | P31's verification (`test -d vendor` fails) is met |
| `high-level-design.md:123` | the `vte` row still retires the oracle in the future tense |
| `high-level-design.md:511` | phase 16's row is met, but lists only `vt-diff` of the old-engine paths |
| `migration.md:394-417` | the deletion list is executed; `bless` and the `US-0072` cross-check are **not** in it and went too — with the fork gone there is no engine that may bless (R-58), so keeping the subcommand would have meant keeping a writer with nothing behind it |
| `migration.md:437-449` | both cleanup rows are done |
| `migration.md:545` | the `US-0087` verification checkbox |
| `testing-and-bench.md:152-163` | "`vt-corpus bless` refuses to write unless given `--deviation`" — the subcommand is gone; the rule is now "nothing blesses" |
| `testing-and-bench.md:195`, `:402` | `vt-diff` "alive from `US-0072` to `US-0087`", and the `US-0087` checkbox |
| `parser.md:336`, `:351` | the oracle's retirement is done; `tests/differential.rs` no longer exists |
| `reflow-and-resize.md:104` | the notices-generator owner row is discharged |
| `US-0077-reflow-and-resize.md:465` | the attribution gap is closed |
| `US-0085-terminal-view-native.md:312` | its Handoff hands `us0081_parity.rs` to this packet; it is deleted |
| `migration.md` § "Cleanup before decommission" | the reverse-wrap row says "confined to the scroll region"; the guard as written also refuses a cursor parked **above** the region (the verifier's minor 8). That is a deliberate reading — the row above an out-of-region cursor is not the region's to write — but it is behaviour the row does not state. Pinned by `verify_reverse_wrap_above_the_region_is_also_blocked`, and left for the design owner to confirm or narrow |

## Context

- `US-0085` already removed the compatibility surface, so nothing in `crates/` *uses*
  the fork any more: what is left is three manifest lines, two test files, the old half
  of the `crates/tools` harness, and prose.
- The frozen expectation files (`grid.expect`, `state.expect`) stay exactly as they are.
  They were blessed by the old engine at `US-0072` and are the gate; with the old engine
  gone nothing can bless again, so `vt-corpus bless` loses its engine and goes with it
  (R-58 — the new engine never blesses, so a `--deviation` re-bless would be
  self-consistency and nothing else).
- `crates/tools/src/corpus_grep.rs` traces recordings through `vte::Parser`. It is
  independent of the grid, so it ports to `oneterm_vt::parser::{Parser, Dispatch}`
  one-for-one; `Params::groups()` is `vte::Params::iter()` and `OscParams` carries the
  code as parameter 0 exactly as `vte` does.
- `crates/vt/tests/ext_differential.rs` is mostly oracle, but
  `ext_memory_stays_bounded` is the measured proof of the intake's headline P1 (the
  unbounded `osc_raw`). It needs no `vte`, so it is kept rather than deleted with the
  oracle around it.

## Plan

- [ ] Record the packet and the harness row before touching code.
- [ ] Measure the baseline: cold `cargo test --workspace` wall time and the working-tree
      size.
- [ ] Commit 1 — deletions: `vendor/`, the manifests, `[patch]`, `deny.toml`, CI,
      `ci-local`, `us0081_parity.rs`, `differential.rs`, `vt-diff`, the old-engine
      modules; `bench.rs` and `corpus_grep.rs` ported; `Cargo.lock` regenerated.
- [ ] Commit 2 — the two cleanup rows with their tests.
- [ ] Commit 3 — notices, `NOTICE` and every current-state doc; provenance comments.
- [ ] Run the gate and record raw totals; re-measure the build time and tree size.

## Decisions

No new decision. `DEC-0014` (a first-party engine under normal review, replacing the
per-capability fork patch) is the choice this packet completes.

## Verification Plan

- Unit: the two cleanup rows — one grid-level test that `BS` with `? 45` set does not
  cross a `DECSTBM` top row, and the `DECRQM` mode-table walk extended to the ANSI modes
  so LNM is covered mechanically rather than by a hand-picked case.
- Integration: `cargo test --workspace` (the corpus gate runs inside it),
  `cargo test -p oneterm-vt --features vt-paranoid`.
- Tooling: `vt-corpus check --engine new` over both corpora; `vt-bench all --mib 2`.
- Policy: `python scripts/third-party-notices.py --check`,
  `python scripts/verify-dependency-graph.py`, `python scripts/check-doc-paths.py`,
  `cargo deny check licenses bans advisories`.
- Whole gate: `pwsh scripts/ci-local.ps1`.
- No E2E and no platform proof: the packet changes no UI surface, and the owner runs
  Claude Code inside their own `oneterm.exe`, which is never enumerated or stopped.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### The gate

`pwsh scripts/ci-local.ps1` — **exit 0, all ten steps**, 58.8 s warm. Raw totals over its
two test steps: **60 sections, 1905 passed, 0 failed, 13 ignored**. (Before the verifier's
minors: 58 / 1893 / 0 / 13. The delta is the six adopted wire tests, counted once in
`cargo test --workspace` and once in the `vt-paranoid` run, plus two round-trip tests
replaced by two literal-fixture decode tests.)

Individually, all green: `python scripts/verify-dependency-graph.py` (21 packages, 21
members), `python scripts/check-doc-paths.py` (120 paths in 10 documents),
`python scripts/third-party-notices.py --check`, `python scripts/check-english.py` (760
files), `python scripts/completion-catalog.py validate`,
`python -m unittest scripts/test_check_english.py`.
`cargo deny check licenses bans advisories` — installed here; **advisories ok, bans ok,
licenses ok** with the fork's `allow-git` entry removed.

### The engine

- `vt-corpus check --engine new`: **45 / 45**, 0 failed, and `oneterm/sixel_basic` **1 / 1**.
  No `expected-diffs.json` exists anywhere under the corpus and none was added.
- `vt-corpus check --engine old`: refuses with "the old engine was deleted at `US-0087`",
  which is the flag's remaining job.
- `vt-bench all --mib 2`: all five tiers on `oneterm-vt`. Tier 2 53.5-194.0 MiB/s across
  the ten fixtures (against the ConPTY transport ceiling of about 1.2 MiB/s for a `cmd.exe`
  producer), tier 3 1.7-22.8 us/frame at 7 200 cells, tier 4 9 us / 2.6 ms / 27.8 ms to grow
  at 0 / 10 000 / 100 000 scrollback rows, tier 5 about 1 364 heap bytes per 160-column row
  for all four content kinds — the packed-cell claim, measured.

### The two cleanup rows

- **Reverse wrap in the region.** `crates/vt/src/grid/screen.rs:667` — the floor at the
  crossing site is now `row_of_index(self.region.top)` instead of `screen_top()`. With a
  full-screen region those are the same row, so the history guard is byte-for-byte
  unchanged; with a `DECSTBM` region the cursor stops at its top row.
  `grid::tests::reverse_wrap_stays_inside_the_scroll_region` pins three cases: blocked at
  the region's top, still crossing one row inside it, and crossing again once the region is
  dropped back to the whole screen.
- **LNM through `inert_state`.** `Mode::LineFeedNewLine` joined the table (answering
  `Reset`: stored, unread), and the ANSI `DECRQM` arm in `dispatch.rs` consults it before
  the live bit. `Mode::ANSI` is the counterpart of `Mode::PRIVATE`, and
  `decrqm_answers_match_the_mode_table` now walks both: `CSI 20 h` followed by `CSI 20 $p`
  answers `;2`, and `CSI 4 h` (IRM, which has a reader) still answers `;1`. One test added
  in total — the walk is where the assertion belongs.
- Neither row moves a corpus cell: no recording sets `? 45` (measured at `US-0086`) and
  none queries LNM.
- **`crates/vt/tests/us0087_cleanup_rows.rs`** — six wire-level tests written by the
  independent verifier and adopted on their finding: reverse wrap blocked at a `DECSTBM`
  top that is **not** row 0, crossing again once the region is dropped, a cursor parked
  *above* the region also refused, `DECRQM` for LNM identical before / after `h` / after
  `l` with IRM as the control, `LNM` set not turning `LF` into `CR`+`LF` (which is *why*
  the answer must not be `Set`), and an exhaustive cross-check that every code
  `Mode::from_ansi` recognises appears in the `Mode::ANSI` walk. All six pass. They go
  through the public API with real bytes, so none of it reuses the unit suites' fixtures.

### The verifier's minors

`PASS-WITH-NOTES`, no blocker, eight minors; six were mine and are fixed in one commit on
top of `cbc6d7b`:

| # | Fix |
| --- | --- |
| 1 | `crates/local-shell/src/transport.rs:16` named an "Alacritty `EventListener`" that no longer exists — reworded to what `LocalListener` is |
| 2 | `docs/sftp-browser-design.md:98` — `(alacritty_terminal + ConPTY)` → `(oneterm-vt + ConPTY)` |
| 3 | This packet's nine Acceptance boxes ticked with their evidence; the Handoff commit count corrected |
| 4 | The 46 frozen `grid.expect` headers still told the reader to `vt-corpus bless --engine old`, a subcommand deleted here. **One comment line** per file: 46 files, 46 insertions, 46 deletions, one distinct line pair, zero non-comment changes, every file `1 1` in `--numstat`; `corpus_check` and both `vt-corpus check` runs re-run green afterwards |
| 5 | `StateExpect::encode` had no caller and `GridExpect::encode` only its own round trip, so both writers and `encode_runs` are deleted (about 45 lines). The two round-trip tests became **literal-fixture decode tests**: a round trip proves a writer and a reader agree with each other, which says nothing about whether the 46 frozen files still parse, and `decode` is live — it reads every one of them on every gate run |
| 6 | `deny.toml`'s `allow-git` list was dead (`Cargo.lock` has zero `git+` sources); deleted, leaving `unknown-git = "deny"` as the whole policy |
| 7 | `AGENTS.md` § 4 listed nine of the ten steps both `ci-local` twins run; `cargo test -p oneterm-vt --features vt-paranoid` added in order |

Minor 8 (reverse wrap is refused *above* the region as well as at its top) and the
observation that `migration.md`'s deletion list names neither `bless` nor the cross-check
are the design owner's, per the coordinator; the code is unchanged and the behaviour is
pinned by `verify_reverse_wrap_above_the_region_is_also_blocked` so a future reader does
not have to re-derive it.

### Before and after

| | Before (`65139c5`) | After |
| --- | ---: | ---: |
| `cargo test --workspace`, cold, empty `target/` | 198.4 s | **168.8 s** (-15 %) |
| test sections / passed / ignored | 58 / 1555 / 12 | 55 / 1529 / 10 |
| working tree (no `.git`, no `target/`) | 30.8 MiB, 1 384 files | **29.9 MiB, 1 332 files** |
| tracked files | 1 382 | 1 330 |

`git count-objects -vH` does not shrink — the deleted blobs stay reachable from history —
so the tree measurement above is the one that means anything. The 26 tests that went are
the old-engine differential (`us0081_parity.rs`, `differential.rs`,
`ext_differential.rs`'s oracle half) and the two corpus-gate tests that drove the old
engine; one test was added.

### Gaps

1. **`grep -rn alacritty_terminal crates/ scripts/ Cargo.toml` is not literally empty**, and
   cannot be. What is gone is every *dependency* and every dangling `vendor/...` path. What
   remains is (a) 13 provenance comments naming the upstream file a port or an
   Apache-2.0-licensed fragment came from — `Alacritty's alacritty_terminal/src/x.rs`, which
   is a path that exists upstream and whose removal would weaken an attribution; (b) the 46
   frozen `grid.expect` headers and several captured `recording` bytes, which are data;
   (c) `crates/vt/tests/corpus/NOTICE`; (d) one line of the notices header saying what was
   removed. Of those, `crates/terminal/src/backend/osc_router.rs:305` still carries a
   `vendor/...` path and is the one real leftover — `US-0088` owns that file.
2. **No E2E and no platform proof.** The packet deletes a build input and two doc-comment
   families; it changes no UI surface, and the two cleanup rows are proved from the wire.
   The owner runs Claude Code inside their own `oneterm.exe`, which was never enumerated,
   driven or stopped.
3. **`ext_memory_stays_bounded` is still `#[ignore]`d** in its new home
   (`crates/vt/tests/parser_limits.rs`) and was not run here: its counting allocator is
   process-wide, so it needs `--test-threads=1` and a quiet machine. It is unchanged apart
   from the file it lives in and one renamed sink.
4. **`ext_osc_caps` was deleted rather than moved.** It had no assertions — it printed five
   payload sizes — and `parser::tests::osc_truncates_at_inline_cap_and_still_dispatches`
   already asserts the same cap. The same reasoning retired the two `GridExpect` round-trip
   tests (verifier minor 5): they proved a writer and a reader agreed with each other, and
   the writer is now gone, so they were replaced by literal-fixture decode tests that pin
   the format the 46 frozen files are actually written in.
5. **The bench tiers are not comparable across the swap.** The old numbers were the fork's;
   these are the engine's, on this machine, at `--mib 2`. No number here is a gate (R-29).

## Handoff

Branch `worktree-agent-aec9c63d7aab09c7e` off `feat/vt-engine` @ `65139c5`, **not merged
and not pushed**. Six commits: the packet (pre-code), the deletions, the two cleanup rows,
the documentation, the `avt` attribution plus this evidence, and the verifier's minors.

**To `US-0088`.** Four comment lines in files you own still name the fork:
`crates/terminal/src/osc.rs:3`, `:4`, `:6` ("the OSCs vte does not dispatch…", "the OneTerm
alacritty fork routes them through…", "no second `vte::Parser`") and
`crates/terminal/src/backend/osc_router.rs:305`, which cites
`vendor/alacritty_terminal/src/term/mod.rs:1692-1705` — a path that no longer exists. They
were left untouched on purpose, since you are rewriting those files. Fold them into your
edit, or open a follow-up.

**To the design owner.** The Reconciliation table above lists thirteen stale lines across
`IN-0029.md`, the HLD and four LLDs. The one that is a real decision rather than a tick:
`migration.md`'s deletion list does not mention `vt-corpus bless` or the `US-0072`
cross-check, and both were deleted here, because with the fork gone no engine may bless
(R-58) and the cross-check's oracle was the fork. If that reading is wrong, the packet to
reopen is this one.
