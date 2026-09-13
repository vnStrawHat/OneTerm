# Work: Decommission the vendored fork

ID: US-0087
Intake: IN-0029
Created: 2026-09-13

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] `vendor/` does not exist and no tracked file references it.
- [ ] `grep -rn "alacritty_terminal" crates/ scripts/ Cargo.toml` is empty except the
      `US-0088`-owned files named in Scope.
- [ ] `cargo tree -p oneterm-app` and `cargo metadata` resolve with no `alacritty_terminal`
      and no `vte` package.
- [ ] `pwsh scripts/ci-local.ps1` green, raw totals recorded.
- [ ] `python scripts/third-party-notices.py --check` green with the two fork rows gone
      and the corpus test-data row (and its `NOTICE`) intact.
- [ ] `cargo run -p oneterm-tools --bin vt-corpus -- check --engine new` is 45/45 on
      `alacritty-ref/` and 1/1 on `oneterm/sixel_basic`.
- [ ] `cargo run -p oneterm-tools --release --bin vt-bench -- all --mib 2` reports all
      five tiers against `oneterm-vt`.
- [ ] Reverse wrap: `BS` at column 0 with `? 45` set does not leave the scroll region,
      proved by a test that sets `DECSTBM` and shows the cursor stays put.
- [ ] `DECRQM` for LNM (`CSI 20 $p`) answers `Reset` even after `CSI 20 h`, proved by the
      same mode-table walk that covers the private inert modes.

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

Filled in at completion: every doc changed, plus the stale lines left in
design-owned files (`IN-0029.md`, HLD, LLDs) for the design owner.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Filled in at completion.

## Handoff

Filled in at completion.
