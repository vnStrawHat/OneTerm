# Work: CI runs each check once per platform it needs

ID: US-0109
Intake: IN-0041
Created: 2026-09-16

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: IN-0041

## Outcome

`.github/workflows/ci.yml` runs every check it ran before, each in exactly one job per platform
that check needs:

- the `terminal-diagnostics` clippy pass runs once, on Windows;
- `cargo test -p oneterm-vt --features vt-paranoid` and `--features regex` run once each, in
  `vt-package`;
- the macOS job builds and tests `oneterm-vt`, so the macOS flavour of `cfg(unix)` is compiled
  somewhere (`BUG-0063`);
- the three Rust setup steps exist once, in `.github/actions/setup-rust`, used by all six Rust
  jobs with the pinned action SHAs unchanged.

`scripts/ci-local.ps1` / `.sh` still run the union of every job's checks and are unchanged.

## Scope

- [x] In scope: `.github/workflows/ci.yml`; the new `.github/actions/setup-rust/action.yml`;
  the `AGENTS.md` § 4 sentence that describes how the local set maps onto CI; the
  `IN-0029` testing LLD sentence that names which jobs carry the `vt-paranoid` step.
- [x] Out of scope: the step **set** of `scripts/ci-local.ps1` / `scripts/ci-local.sh` — local
  runs everything on one machine and must stay a superset of each job (their comments cite
  packets, not CI jobs, so nothing there was reordered either); `needs:` edges between jobs
  (owner declined, IN-0041 Open Decision 1); the `paths:`/`concurrency:`/`permissions:` blocks;
  `.github/workflows/release.yml`; the action SHAs themselves; every `continue-on-error` job's
  behaviour; any crate, test or script.

## Acceptance

- [x] `.github/workflows/ci.yml` and `.github/actions/setup-rust/action.yml` both parse as YAML.
- [x] Every step name present before the change is present after it, in exactly one job for a
  platform-free check and in each platform job for a platform-dependent one — proved by the
  step-to-job table below, built from a diff of the two revisions.
- [x] `windows-quality` keeps clippy, clippy `+terminal-diagnostics`, and `cargo test --workspace`,
  and no longer runs `vt-paranoid`.
- [x] `workspace-quality` keeps fmt, clippy and `cargo test --workspace`, and no longer runs
  `+terminal-diagnostics`, `vt-paranoid` or `regex`.
- [x] `vt-package` runs `vt-paranoid` and `regex`, and its `vt-paranoid` comment still cites
  `IN-0029` R-28 (workflow comments may cite packets; only the crate's own rustdoc may not).
- [x] `macos-tests` runs `cargo test ... -p oneterm-vt`, and its comment says why.
- [x] All six Rust jobs use `./.github/actions/setup-rust`; the `dtolnay/rust-toolchain` and
  `Swatinem/rust-cache` SHAs are byte-identical to the ones on `main` @ `f315bf5e`.
- [x] Each job's top-of-file explanatory comment survives.
- [x] `pwsh scripts/ci-local.ps1` passes end to end.

## Documentation

### Owning Docs Reviewed

- `.github/workflows/ci.yml` — the gate itself. Its per-job and per-step comments are the
  rationale record and are edited with the steps they explain.
- `scripts/ci-local.ps1`, `scripts/ci-local.sh` — the local twin. Reviewed line by line: their
  comments cite packets (`US-0090`, `IN-0029` R-28, `US-0101`, `US-0104`, `BUG-0059`) and never
  a CI job or a runner OS, so there is nothing in them that this change makes false, and no
  step-to-job mapping to reorder. **No change.**
- `AGENTS.md` § 4 — says the agent must run "the same set of checks CI runs". True as a set both
  before and after; **changed** by one sentence so the reader knows CI no longer runs every one
  of them in every job, and that the local script deliberately does.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` § "integrity
  budget" — stated that "both quality jobs in `.github/workflows/ci.yml`" carry the
  `vt-paranoid` step. That becomes false. **Changed.**
- `docs/agents/structure.md` (`vt` row) — says CI's `vt-package` job greps the crate's rustdoc
  for repository-only citations. Still true; that step did not move. **No change.**
- `crates/vt/docs/guide/11-conformance.md` § integrity walk and `12-versioning.md` feature
  table — describe what `vt-paranoid` *is*, never where it runs, and are published rustdoc that
  must not name this repository's CI anyway. **No change.**
- `README.md` — its `terminal-diagnostics` section documents the feature as a developer opt-in
  and makes no claim about CI jobs; its platform-support section says Linux/macOS compile but
  are untested by QA, which this change does not alter (adding a package to a macOS `cargo test`
  is not a QA pass). **No change.**
- `docs/README.md` — indexes owning *designs*; maintenance intakes (IN-0034..IN-0037) are not
  listed there, so IN-0041 is not either. **No change.**
- `docs/license-analysis.md` — names the `cargo-deny` job, which this change does not touch.
  **No change.**

### Documentation Action

Update required: `AGENTS.md` § 4 (one sentence about how the local set maps onto CI jobs) and
`docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` (the sentence naming
which jobs carry `vt-paranoid`). Everything else in the reviewed set describes features,
commands or jobs this change leaves alone, with the per-doc reasons recorded above.

Reason: the only contract statements that this change falsifies are statements about *which CI
job runs which check*. Two documents make such a statement; the rest describe the checks
themselves.

### Reconciliation

Changed: `.github/workflows/ci.yml`, `.github/actions/setup-rust/action.yml` (new),
`AGENTS.md` § 4, `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md`.
The no-change reasons recorded above were re-read after implementation and still hold.

## Context

Why each move is safe, in one line each:

- `terminal-diagnostics`: `grep -rn '#\[cfg(feature = "terminal-diagnostics")' crates/` hits only
  `crates/local-shell/src/event_loop.rs`, `crates/ssh/src/transport.rs` and
  `crates/terminal-view/src/render/`; none of those sites sits inside a `cfg(windows)` or
  `cfg(unix)` block, so one platform compiling them is all the gate ever bought.
- `vt-paranoid`: the feature only widens `integrity_lo()` to the whole history
  (IN-0029 R-28). `oneterm-vt`'s sole platform-conditional module is `src/pty/`, which
  `cargo test --workspace` builds and runs on each platform job regardless of this feature.
- `regex`: a pure `oneterm-vt` cargo feature; `vt-package` is the job that owns the crate's
  feature configurations.
- macOS `-p oneterm-vt`: `BUG-0063` is the precedent — `libc::sigset_t` is a `u32` type alias on
  macOS and a struct on glibc, so `crates/vt/src/pty/unix.rs` has two compilations, and before
  this change only the glibc one was ever performed.

## Plan

- [x] Write the intake, the HLD and this packet first.
- [x] Add `.github/actions/setup-rust/action.yml` with an optional `components` input, carrying
  the three steps and both pinned SHAs verbatim.
- [x] Replace the inline setup triple in all six Rust jobs with the composite action.
- [x] Remove the duplicated steps, move the two feature tests into `vt-package`, extend the
  macOS test command, and update the affected comments.
- [x] Reconcile `AGENTS.md` § 4 and the IN-0029 testing LLD.
- [x] Run the verification plan and record it below.

## Decisions

None. IN-0041's three closed Open Decisions (no `needs:` edges, `ci-local` stays a superset,
`vt-package` receives the two feature tests) are owner rulings recorded in the intake; none of
them is a rule later work must inherit beyond this workflow file, so no `DEC-NNNN` is created.

## Verification Plan

1. `python -c "import yaml; yaml.safe_load(open(...))"` on both YAML files.
2. `git diff` of `ci.yml`, reduced to a step-to-job table, before against after.
3. `python scripts/check-doc-paths.py` — the doc edits touch `AGENTS.md`, which it checks.
4. `python scripts/check-english.py` — it scans `.github/`, so the new action is in its set.
5. `pwsh scripts/ci-local.ps1` to completion — the local gate is unchanged, so a green run
   proves the doc and script edits broke nothing.

Not available: an actual GitHub Actions run. There is no runner in this environment, so the
workflow's *execution* is unverified; the first real proof is the next push.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Step-to-job table (before -> after)

`main` @ `f315bf5e` against this change. "setup triple" = *Read pinned toolchain* +
*Set up Rust* + *Cache cargo*.

| Step | Job(s) before | Job(s) after | Verdict |
| --- | --- | --- | --- |
| checkout | all 8 jobs | all 8 jobs | unchanged |
| setup triple (3 steps, inline) | workspace-quality, vt-package, windows-quality, vt-bench, vt-esctest, macos-tests | the same six jobs, via `./.github/actions/setup-rust` | deduplicated, same steps, same SHAs |
| Set up Python | dependency-graph, vt-package | dependency-graph, vt-package | unchanged |
| the 6 policy/English/notices steps | dependency-graph | dependency-graph | unchanged |
| cargo deny check | cargo-deny | cargo-deny | unchanged |
| Install Linux system dependencies (GPUI) | workspace-quality | workspace-quality | unchanged |
| Verify formatting | workspace-quality | workspace-quality | unchanged |
| Lint workspace | workspace-quality, windows-quality | workspace-quality, windows-quality | unchanged — per-platform on purpose |
| Lint workspace with terminal diagnostics | workspace-quality, **windows-quality** | **windows-quality** | deduplicated (platform-free feature) |
| Test workspace | workspace-quality, windows-quality | workspace-quality, windows-quality | unchanged — per-platform on purpose |
| Test the VT engine with the full integrity walk | workspace-quality, windows-quality | **vt-package** | deduplicated and re-homed |
| Test the VT engine with regex search | workspace-quality | **vt-package** | re-homed, still runs on ubuntu |
| Build the feature matrix | vt-package | vt-package | unchanged |
| The transport-free build is six leaf dependencies | vt-package | vt-package | unchanged |
| Run the headless example | vt-package | vt-package | unchanged |
| Document with warnings denied | vt-package | vt-package | unchanged |
| Check the public API surface | vt-package | vt-package | unchanged |
| Package the crate | vt-package | vt-package | unchanged |
| Check what the package carries | vt-package | vt-package | unchanged |
| Published rustdoc must stand alone | vt-package | vt-package | unchanged |
| The embedder guide must stand alone | vt-package | vt-package | unchanged |
| Run the five tiers / Publish the table | vt-bench | vt-bench | unchanged |
| Fetch esctest / Build the bridge / Run esctest / Summarise / Publish the matrix | vt-esctest | vt-esctest | unchanged |
| Test portable backend contracts | macos-tests | macos-tests | **command extended** with `-p oneterm-vt` |

No step name that existed before is absent afterwards. The three rows in bold are the whole
behavioural change, plus the macOS command extension. Step counts, excluding `checkout`:

| Job | before | after | difference |
| --- | --- | --- | --- |
| dependency-graph | 7 | 7 | — |
| cargo-deny | 1 | 1 | — |
| workspace-quality | 10 | 5 | -2 setup triple, -3 deduplicated / re-homed |
| vt-package | 13 | 13 | -2 setup triple, +2 re-homed feature tests |
| windows-quality | 7 | 4 | -2 setup triple, -1 deduplicated (`vt-paranoid`) |
| vt-bench | 5 | 3 | -2 setup triple |
| vt-esctest | 8 | 6 | -2 setup triple |
| macos-tests | 4 | 2 | -2 setup triple |

Every reduction is either the setup triple collapsing into one step (-2 per Rust job) or one of
the three deduplicated/re-homed rows above.

## Evidence and Gaps

### 1. Both YAML files parse

```text
$ python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); yaml.safe_load(open('.github/actions/setup-rust/action.yml')); print('yaml ok')"
yaml ok
```

PyYAML 6.0.2 / CPython 3.13.1 was already installed; nothing was installed for this check.
`actionlint` is not installed here and was not installed, so the YAML parse plus a careful
re-read of the diff is the syntax evidence.

### 2. The step-to-job diff

Both revisions were parsed and their step names mapped to jobs, `f315bf5e` against the working
tree:

```text
LOST: ['Cache cargo', 'Read pinned toolchain']
NEW : []
MOVED: Lint workspace with terminal diagnostics ['workspace-quality', 'windows-quality'] -> ['windows-quality']
MOVED: Test the VT engine with regex search ['workspace-quality'] -> ['vt-package']
MOVED: Test the VT engine with the full integrity walk ['workspace-quality', 'windows-quality'] -> ['vt-package']
```

The two "LOST" names are the setup triple's first and third steps; they now live in
`.github/actions/setup-rust/action.yml` with the same names, the same `run:` body and the same
pinned SHAs, and every one of the six Rust jobs still invokes them through `Set up Rust`, whose
name is unchanged and therefore not in the list. Nothing is "NEW", so no check was invented
either. The three "MOVED" rows are the whole intended behavioural change; the macOS command
extension is a `run:` change under an unchanged step name and so does not appear here — it is
the `-p oneterm-vt` addition visible in the diff.

The per-job step counts in the second table above came from the same two parses.

### 3. Policy checks

```text
$ python scripts/check-doc-paths.py
Doc path check passed for 199 current paths in 11 documents.

$ python scripts/check-english.py
English contributor-text check passed for 921 files.
```

### 4. The local gate, unchanged, end to end

`CARGO_BUILD_JOBS=3` (another worktree was building on the same machine). Tail of the run:

```text
==> python scripts/vt-public-api.py --diff-platforms
...
the delta is 6 lines, all inside `oneterm_vt::pty`

==> cargo package -p oneterm-vt --list | verify-dependency-graph.py --package-list -
Dependency graph policy passed for 20 workspace packages and 20 explicit members, ...
Package set passed: the oneterm-vt package carries CHANGELOG.md, LICENSE, NOTICE,
README.md, examples/headless.rs, and reaches nothing outside crates/vt.

==> python scripts/verify-dependency-graph.py
Dependency graph policy passed for 20 workspace packages and 20 explicit members, ...

==> python scripts/check-doc-paths.py
Doc path check passed for 199 current paths in 11 documents.

==> python -m unittest scripts/test_check_english.py
Ran 2 tests in 0.010s
OK

==> python scripts/check-english.py
English contributor-text check passed for 921 files.

==> python scripts/completion-catalog.py validate
[completion-catalog] all catalogs valid

==> python scripts/third-party-notices.py --check
THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.

[exited with code 0]
```

The script's step set is unchanged by this work, so the value of this run is negative evidence:
the `AGENTS.md` and IN-0029 LLD edits, and the new `.github/` file, broke none of the checks that
read them (`check-doc-paths.py` covers `AGENTS.md`; `check-english.py` covers `.github/`).

### 5. Harness DB mirror

This packet's row for `harness.db` (table `story`) and the matching `intake` row. Recorded here
for the DB owner to apply; **not applied by this change** — the database lives in the main
checkout and this work was done in a worktree. Replace `<intake_rowid>` with the `rowid` the
`intake` insert returns.

```python
import sqlite3
con = sqlite3.connect("harness.db")
con.execute(
    """INSERT INTO intake
       (created_at, input_type, summary, risk_lane, risk_flags, affected_docs,
        story_id, doc_path, notes, document_number, design_doc)
       VALUES (?,?,?,?,?,?,?,?,?,?,?)""",
    (
        "2026-09-16",
        "maintenance",
        "CI duplicated three platform-free checks across the Linux and Windows quality "
        "jobs, never compiled the macOS half of the pty module's cfg(unix) code, and "
        "repeated the same three Rust setup steps in six jobs.",
        "normal",
        "",  # risk_flags: none; CI configuration only, no crate, test or script changes
        ".github/workflows/ci.yml;AGENTS.md;scripts/ci-local.ps1;scripts/ci-local.sh;"
        "docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md;"
        "docs/agents/structure.md;crates/vt/docs/guide/11-conformance.md;"
        "crates/vt/docs/guide/12-versioning.md;README.md;docs/README.md",
        "US-0109",
        "docs/spec-intakes/IN-0041-ci-dedup/IN-0041.md",
        "Owner decided all four moves on 2026-09-16 and declined needs: edges between "
        "jobs. Follows BUG-0063, which is the evidence that cfg(unix) is two platforms.",
        41,
        "docs/spec-intakes/IN-0041-ci-dedup/high-level-design.md",
    ),
)
con.execute(
    """INSERT INTO story
       (id, title, created_at, risk_lane, contract_doc, packet_doc, status,
        unit_proof, integration_proof, e2e_proof, platform_proof, evidence,
        verify_command, last_verified_at, last_verified_result, notes, intake_id)
       VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
    (
        "US-0109",
        "CI runs each check once per platform it needs",
        "2026-09-16",
        "normal",
        ".github/workflows/ci.yml",
        "docs/spec-intakes/IN-0041-ci-dedup/"
        "US-0109-ci-runs-each-check-once-per-platform-it-needs.md",
        "implemented",
        0,  # unit_proof: n/a, no code changed
        0,  # integration_proof: n/a, no code changed
        0,  # e2e_proof: no GitHub runner here; the workflow was never executed
        1,  # platform_proof: pwsh scripts/ci-local.ps1 green on Windows
        "yaml.safe_load parses ci.yml and the new composite action; a step-name diff "
        "between f315bf5e and this change reports LOST=[Read pinned toolchain, Cache "
        "cargo] (both moved into .github/actions/setup-rust), NEW=[] and exactly three "
        "MOVED rows (terminal-diagnostics clippy -> windows-quality only; vt-paranoid "
        "and regex -> vt-package); check-doc-paths 199 paths / 11 documents ok; "
        "check-english 921 files ok; ci-local.ps1 all checks passed.",
        "pwsh scripts/ci-local.ps1",
        "2026-09-16",
        "passed",
        "CI-configuration-only change; the workflow itself is unverified until the next "
        "push, since there is no GitHub runner in this environment. Coverage narrows by "
        "decision in two places (terminal-diagnostics no longer linted on Linux, "
        "vt-paranoid no longer tested on Windows) and widens in one (macOS now builds "
        "and tests oneterm-vt).",
        "<intake_rowid>",
    ),
)
con.commit()
```

### Gaps

- **No GitHub Actions run.** The workflow is not executed anywhere in this environment, so
  "these jobs succeed on the runners" is unverified. The first real proof is the next push;
  the risk is a YAML/GHA semantic error that `yaml.safe_load` cannot see — the composite
  action's `inputs.components` reaching `dtolnay/rust-toolchain` as an empty string in the four
  jobs that pass no components is the specific thing to watch on that first run (the action
  treats an empty component list as "install none", which is what those four jobs did before by
  omitting the key).
- Coverage genuinely narrows in three places, by decision, not by accident: the
  `terminal-diagnostics` lint no longer runs on Linux, and `vt-paranoid` no longer runs on
  Windows. If either ever grows a platform-conditional site, its job assignment has to be
  revisited — the rule to apply is the table at the top of the HLD.

## Handoff

None — the change, its records and its local proof are complete in one branch. The next actor
only needs to watch the first CI run on the pushed branch.
