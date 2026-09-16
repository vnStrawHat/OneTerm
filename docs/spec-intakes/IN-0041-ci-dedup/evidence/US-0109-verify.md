# Independent verification: US-0109 — CI runs each check once per platform it needs

Packet: `docs/spec-intakes/IN-0041-ci-dedup/US-0109-ci-runs-each-check-once-per-platform-it-needs.md`
Intake: `docs/spec-intakes/IN-0041-ci-dedup/IN-0041.md`
Under test: `aa2bf37f` (on `e01943a3`, parent `main` @ `f315bf5e`)
Verifier: adversarial, independent, in a worktree. Nothing outside the worktree was written;
`harness.db` was not opened.
Date: 2026-09-16

## Verdict

**PASS-WITH-NOTES.**

The workflow restructuring is correct. Both YAML files parse, every step that existed before
exists after in exactly the jobs the packet claims, the two pinned action SHAs are byte-identical
to `f315bf5e`, the composite action obeys the GitHub Actions composite specification, and the
three coverage arguments (`terminal-diagnostics`, `vt-paranoid`, `regex`) hold against an
independent scan of the source. **No HIGH defect: nothing here fails to parse and nothing in the
composite would fail on a runner.**

What is wrong is the *recorded rationale for the macOS change*, which is factually false in five
places, and one harness snippet value that would abort the insert.

| # | Defect | Grade |
| --- | --- | --- |
| D1 | The macOS justification ("`oneterm-vt` / `pty/unix.rs` was never compiled on macOS") is false in the intake, the HLD, the packet, the commit message and the `ci.yml` comment. `oneterm-local-shell` depends on `oneterm-vt` with `pty`, and the macOS job already built it. | MEDIUM |
| D2 | The `story` snippet writes `last_verified_result="passed"`; the column's CHECK set is `pass`/`fail` and every other packet in the repo writes `"pass"`. The insert would raise `IntegrityError`. | MEDIUM |
| D3 | `action.yml`'s header says "This is the one copy", but `.github/workflows/release.yml:169-185` keeps a seventh verbatim copy with the same two SHAs. | LOW |
| D4 | Packet Gaps says coverage "narrows in three places" and then lists two (the story snippet's `notes` says two, correctly). | LOW |
| D5 | `docs/spec-intakes/IN-0029-vt-engine/evidence/US-0087-verify.md:235` still tabulates `vt-paranoid` as running on "Linux + Windows". Archived evidence, so leaving it may be deliberate — but it is the only remaining sentence in `docs/` that says where a moved step runs. | LOW |

Environment note that affects how these findings were reached: `grep` invoked through this
session's Bash tool **silently returns no matches** for patterns containing `\|` alternation or
`\[`. Two of my first greps returned empty where matches demonstrably exist
(`crates/ssh/src/sftp_task/transfer.rs:52` is `#[cfg(unix)]`). Every grep-based claim below was
therefore re-derived with a Python `re` scanner. The packet's own Context bullet quotes a
`grep -rn '#\[cfg(feature = "terminal-diagnostics")' crates/` command that would hit the same
mangling here; its **conclusion** is nevertheless correct, confirmed independently below.

---

## Attack 1 — composite action semantics

`.github/actions/setup-rust/action.yml`, read against the composite-action spec.

| Requirement | Finding |
| --- | --- |
| `runs.using: composite` | present |
| every `run:` step declares `shell:` | one `run:` step (`Read pinned toolchain`), `shell: bash`. The other two steps are `uses:`, which take no `shell:`. |
| `name` / `description` present | both present |
| inputs referenced as `${{ inputs.x }}` | `components: ${{ inputs.components }}` — correct |
| step outputs scoped to the same composite | `toolchain: ${{ steps.toolchain.outputs.channel }}` refers to `id: toolchain` in the same action — correct scoping |
| `uses:` of an external action inside a composite | allowed; `dtolnay/rust-toolchain` is itself a composite (nesting is supported, documented depth 10) and `Swatinem/rust-cache` is a JS action whose `post:` save step runs from inside a composite (supported since runner 2.283) |
| default working directory | composite `run:` steps inherit the **caller's** working directory, so `sed -n ... rust-toolchain.toml` resolves against `$GITHUB_WORKSPACE` — correct |
| `shell: bash` on `windows-latest` | Git Bash is on the image; `sed` and `$GITHUB_OUTPUT` both work |

**Pinned SHAs, diffed against `f315bf5e`** — the six inline copies and the one composite copy:

```text
before (all six jobs):  dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4
                        Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32
after  (action.yml)  :  dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4
                        Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32
```

Identical, comments (`# 2026-07-23`, `# v2 tag, 2026-07-23`) included. The `Read pinned toolchain`
`run:` body is byte-identical to the six copies it replaces (`git diff` shows it only as a move).

### `components: ""` — settled, not guessed

Four of the six jobs (`vt-package`, `vt-bench`, `vt-esctest`, `macos-tests`) previously **omitted**
`components:` and now pass the composite's default empty string through to
`dtolnay/rust-toolchain`. I read that action's `action.yml` at the pinned SHA
`4cda84d5c5c54efe2404f9d843567869ab1699d4`:

- the `components` input is `required: false` with **no default**, so an omitted input and an
  explicit `""` both arrive as the empty string — the action cannot tell them apart;
- the `flags` step builds the rustup flags with
  `for c in ${components//,/ }; do echo -n ' --component' $c; done`, which iterates zero times on
  an empty string.

**Empty string is exactly equivalent to omitting the key. No guard is needed**, and the
`${{ inputs.components != '' && inputs.components || null }}` style workaround would be dead
complexity here. The packet's Gaps bullet flags this as "the specific thing to watch" — the
concern was reasonable, and it is now closed rather than left open.

### `Swatinem/rust-cache` keying inside a composite

`rust-cache` derives its cache key from `process.env.GITHUB_JOB` (plus `shared-key`/`key` inputs,
neither of which is set here, before or after). `GITHUB_JOB` is a **job-scoped** environment
variable; a composite action does not create a job, so inside `setup-rust` it still holds the
**caller's** job id. Caches therefore stay one-per-job exactly as before, with no collision
between `workspace-quality`, `vt-package`, `windows-quality`, `vt-bench`, `vt-esctest` and
`macos-tests`. No `with:` was added or removed on that step.

**Attack 1: pass.**

---

## Attack 2 — the workflow

Both files parsed with `yaml.safe_load` and the two revisions mapped step-name → job
independently of the packet's table (`git show f315bf5e:.github/workflows/ci.yml` against the
working tree).

```text
== jobs before: cargo-deny dependency-graph macos-tests vt-bench vt-esctest vt-package
                windows-quality workspace-quality
== jobs after : (identical, 8 jobs)

== steps removed entirely:
   'Cache cargo'           from workspace-quality vt-package windows-quality vt-bench vt-esctest macos-tests
   'Read pinned toolchain' from the same six jobs
== steps added entirely:
   (none)
== steps whose job set changed:
   'Lint workspace with terminal diagnostics': [windows-quality, workspace-quality] -> [windows-quality]
   'Test the VT engine with regex search':     [workspace-quality]                  -> [vt-package]
   'Test the VT engine with the full integrity walk':
                                               [windows-quality, workspace-quality] -> [vt-package]
```

Exactly the packet's claim: two names absorbed by the composite, **nothing new**, three re-homed
rows. The only other change is the `run:` body of `Test portable backend contracts`, which gains
`-p oneterm-vt` (invisible to a name map; read out of the diff).

Per-job step counts, recomputed from my own parse, excluding `checkout` — every number in the
packet's second table reproduces:

| Job | before | after |
| --- | --- | --- |
| dependency-graph | 7 | 7 |
| cargo-deny | 1 | 1 |
| workspace-quality | 10 | 5 |
| vt-package | 13 | 13 |
| windows-quality | 7 | 4 |
| vt-bench | 5 | 3 |
| vt-esctest | 8 | 6 |
| macos-tests | 4 | 2 |

Untouched, confirmed by comparing the parsed structures:

- top-level `on:` (both `paths:` filter lists), `permissions:`, `concurrency:` — **identical**;
- every job's non-`steps` attributes — **identical** (`runs-on`, and `continue-on-error: true` on
  `vt-bench` and `vt-esctest`);
- `if: always()` survives on both `vt-esctest` steps that had it (`Summarise the run`,
  `Publish the matrix`);
- `components: rustfmt, clippy` on `workspace-quality`, `components: clippy` on
  `windows-quality`, nothing on the other four — matching the before state exactly.

**`./.github/actions/setup-rust` after `checkout` in every job**: yes. In all six Rust jobs the
composite is step index 1, immediately after `actions/checkout` at index 0. A local `uses:` path
resolves against the checked-out workspace, so ordering is the whole requirement and it is met.

**`vt-package` needs no apt/GPUI dependencies for the two moved tests.** It installs none, and
still does not.

```text
$ cargo tree -p oneterm-vt --features vt-paranoid,regex -e normal
oneterm-vt v0.5.2
├── bitflags ├── log ├── memchr
├── polling (→ cfg-if, concurrent-queue → crossbeam-utils, pin-project-lite, windows-sys)
├── regex (→ aho-corasick, memchr, regex-automata, regex-syntax)
├── rustc-hash ├── unicode-segmentation ├── unicode-width
└── windows-sys → windows-targets
```

No OneTerm crate, no `gpui`, no system library. On Linux `windows-sys` is replaced by `libc`
(`crates/vt/Cargo.toml` target tables). Dev-dependency `proptest` is pure Rust. `vt-paranoid` and
`regex` are both `[]`-style features that add no dependency of their own beyond optional `regex`.
The decisive precedent is already in that job: `vt-package` has been running
`cargo test -p oneterm-vt --no-default-features` and `cargo build -p oneterm-vt --all-features`
with no apt step since before this change, and `--all-features` already compiles both of these
feature flags there.

**Attack 2: pass.**

---

## Attack 3 — the coverage argument

Both greps re-run with a Python scanner (see the environment note above).

### `oneterm-vt`'s platform-conditional code

```text
$ scan 'cfg!?\((not\()?\s*(unix|windows|target_os|target_family|target_env)' crates/vt/src crates/vt/tests crates/vt/examples
crates/vt/src/pty/loopback_tests.rs:162  #[cfg(unix)]
crates/vt/src/pty/loopback_tests.rs:167  #[cfg(windows)]
crates/vt/src/pty/mod.rs:58,60,63,65,135,139
--- 8 matches
```

plus `crates/vt/src/pty/unix.rs:329,343` (`#[cfg(any(target_os = "linux", target_os = "macos"))]`,
missed by the pattern above, found by a separate `cfg(` sweep). **Every one is under
`crates/vt/src/pty/`**; `crates/vt/tests/` and `crates/vt/examples/` have none.

`vt-paranoid` itself is one runtime branch — `crates/vt/src/grid/screen.rs:1818`
`if cfg!(feature = "vt-paranoid")` — plus a bench helper at
`crates/vt/src/snapshot/snapshot_bench.rs:137`. Grid logic, no platform in it.

So dropping `vt-paranoid` from the Windows job loses nothing platform-specific: the Windows half
of `src/pty/` is still compiled and run by `cargo test --workspace` in `windows-quality`, and the
feature changes nothing about it. `crates/vt/tests/pty_contract.rs` (`#![cfg(all(windows, feature
= "pty"))]`, a real ConPTY spawning `cmd.exe`) used to run **twice** on Windows and now runs once
— which is precisely the duplication this packet set out to remove. Claim holds.

### `terminal-diagnostics` sites vs. platform cfgs

```text
$ scan 'feature = "terminal-diagnostics"' crates       → 42 sites in 5 files:
    crates/local-shell/src/event_loop.rs
    crates/ssh/src/transport.rs
    crates/terminal-view/src/render/{diagnostics,element,state}.rs

$ scan 'cfg!?\((not\()?\s*(unix|windows|target_os|...)' crates/local-shell/src crates/ssh/src crates/terminal-view/src
    → 29 matches, in: local-shell/src/{event_loop_tests,session,session_terminal,session_tests}.rs
                        ssh/src/agent.rs, ssh/src/sftp_task/{sftp_task_tests,transfer}.rs
                        terminal-view/src/input/{keys,keys_tests}.rs
                        terminal-view/src/panel/terminal_panel.rs
                        terminal-view/src/terminal_view/view_tests.rs
```

**The two file sets are disjoint.** Not one of the 42 diagnostics sites sits in a file that has a
platform `cfg` at all, let alone inside one. In particular `terminal_panel.rs:644/657`
(`#[cfg(windows)]` / `#[cfg(not(windows))]`) carries no diagnostics-gated item, so the
combination "non-Windows branch × `terminal-diagnostics`" does not exist in the source and Linux
loses nothing by no longer compiling it.

Note that the three crates are *not* free of platform cfgs, as the packet's and the intake's
prose can be read to say ("none of it platform-conditional"); the correct and verified statement
is that none of the **diagnostics sites** is. The conclusion is unaffected.

**Attack 3: pass**, with the wording note above.

---

## Attack 4 — the macOS job — **D1 (MEDIUM)**

What the records say:

- intake IN-0041 § Source 2: "The macOS job does not build `oneterm-vt` at all, so its flavour of
  `cfg(unix)` is unverified."
- HLD: "macOS is the flavour nothing currently builds."
- packet § Context: "before this change only the glibc one was ever performed."
- commit message: "only the glibc one was ever compiled."
- `ci.yml` macos-tests comment: "this is the only job that compiles its macOS half."

All five are false.

```text
$ cargo tree -p oneterm-local-shell -e normal --depth 1
oneterm-local-shell v0.5.2
├── async-channel ├── log ├── oneterm-core ├── oneterm-terminal
├── oneterm-vt v0.5.2          ← crates/local-shell/Cargo.toml:
└── polling                       oneterm-vt = { workspace = true, features = ["pty"] }
```

`crates/vt/src/lib.rs:91` gates `pub mod pty;` on `feature = "pty"`, and `crates/vt/src/pty/mod.rs:58`
gates `mod unix;` on `cfg(unix)`. The macOS job ran `cargo test ... -p oneterm-local-shell` before
this change, so **`crates/vt/src/pty/unix.rs` was already compiled on macOS**, derives and all.

The `BUG-0063` framing is inverted too. That commit's own message says the defect "failed **the
ubuntu job**" — the glibc compilation is what *caught* it. A macOS build would have accepted it
(`sigset_t = u32` derives `PartialEq` fine), so the missing macOS build is not how BUG-0063
reached `main`; the missing *Linux* compilation of a file only ever built on Windows is.

**The change is still worth making**, for a reason nobody wrote down: `-p oneterm-vt` makes macOS
**run** `oneterm-vt`'s own test targets for the first time — `crates/vt/src/pty/unix.rs`'s
`#[cfg(test)]` module (line 403), `loopback_tests.rs`'s `#[cfg(unix)]` arm, and the whole engine
suite. Compiling a crate as a dependency never builds its `#[cfg(test)]` modules. Recommended
fix: replace "compiles" with "runs `oneterm-vt`'s own tests on" in all five places.

### Everything else in the macOS job

No GPUI or apt-style dependency is reachable: the five packages are `oneterm-core`,
`oneterm-terminal`, `oneterm-local-shell`, `oneterm-ssh`, `oneterm-vt`; the GPUI crates are
`terminal-view` and `app`, neither of which is selected. `oneterm-vt`'s `pty` feature pulls
`polling` (kqueue on macOS) and `libc` via `[target.'cfg(unix)'.dependencies]` — no `rustix`, no
`windows-sys` on that target, nothing needing a system package. `crates/vt/tests/pty_contract.rs`
is Windows-gated and simply does not run there.

**Attack 4: the job is fine; the reason recorded for it is not.**

---

## Attack 5 — the docs

**AGENTS.md § 4** — "CI spreads that set across eight jobs and three runner OSes". Counted
independently from the parse: 8 jobs (`dependency-graph`, `cargo-deny`, `workspace-quality`,
`vt-package`, `windows-quality`, `vt-bench`, `vt-esctest`, `macos-tests`) and 3 OSes
(`ubuntu-latest`, `windows-latest`, `macos-latest`). **Accurate.** The added clause ("the local
script is one machine, so it runs the whole set in one place") is also accurate: `ci-local.ps1`
still runs `+terminal-diagnostics` (line 33), `vt-paranoid` (line 38) and `regex` (line 41), and
neither script was touched.

**`IN-0029/low-level-design/testing-and-bench.md`** — now says "`scripts/ci-local.ps1`,
`scripts/ci-local.sh` and the `vt-package` job". **Accurate**: `vt-paranoid` is in `vt-package`
only, and in both local scripts.

**Sweep for stale placement claims** over `docs/`, `README.md`, `crates/vt/docs/guide/`,
`crates/vt/README.md`, `crates/vt/CHANGELOG.md`, for `windows-latest`, `macos`,
`Windows workspace quality gate`, `Full workspace quality gate`, `vt-paranoid`,
`terminal-diagnostics` and the job names:

- **No current (non-archival) document states where a moved step runs.** The guide chapters
  describe what `vt-paranoid` *is*, never where it runs — as the packet claims.
- `docs/spec-intakes/IN-0029-vt-engine/evidence/US-0087-verify.md:235` tabulates
  "`cargo test -p oneterm-vt --features vt-paranoid` | Linux + Windows | step 4" — **D5**, an
  archived evidence record; leaving history alone is defensible, but the packet's Owning-Docs
  review did not mention it.
- `docs/review-refresh-2026-08/07-build-deps-ci.md:131,135` names a `cross-platform-tests` job
  that no longer exists — pre-existing drift in an archived review, untouched by and unrelated to
  this change.
- `docs/PROJECT.md:91` "(mirrors `.github/workflows/ci.yml`)" — still true as a set; the AGENTS.md
  sentence is where the nuance now lives. No change needed.

**D3**: `.github/actions/setup-rust/action.yml` says "Six jobs carried six identical copies … This
is the one copy." `.github/workflows/release.yml:169,174,179,185` still carries a seventh, with
the same two SHAs. The packet lists `release.yml` as out of scope, so the *decision* is recorded —
the *comment* is the thing that overstates. A SHA bump remains two edits.

---

## Attack 6 — packet honesty and the harness snippets

| Check | Result |
| --- | --- |
| `intake` column list | `created_at, input_type, summary, risk_lane, risk_flags, affected_docs, story_id, doc_path, notes, document_number, design_doc` — 11 columns, 11 values, in the expected order. **OK** |
| `story` column list | 17 columns, 17 values, in the expected order. **OK** |
| `status` | `"implemented"` ∈ {planned, in_progress, implemented, changed, reopened, retired}, and matches the packet's HARNESS:STATUS block (Planned/In progress/Implemented ticked). **OK** |
| `*_proof` flags | `0,0,0,1` — matches the HARNESS:PROOF block (Platform proof + Verify command). Honest: no unit/integration/e2e claim for a config-only change. **OK** |
| `last_verified_result` | **`"passed"` — D2.** The CHECK set is `pass`/`fail`; 13 other packets in this repo write `"pass"`, and two prior independent verifications record the CHECK constraint explicitly (`IN-0038/evidence/BUG-0058-verify.md:167`, `US-0100-verify.md:215`). As written the insert aborts with `IntegrityError`. I did **not** open `harness.db` (main-checkout file, off limits), so this is graded from the repo's documented schema and its precedents. |
| `intake_id` | left as the literal `"<intake_rowid>"` with an instruction to substitute. Deliberate and stated. **OK** |
| Gaps honesty | The "no GitHub Actions run" gap is stated plainly and is correct. The `components`-empty-string worry is stated as the thing to watch and is now **resolved in this document** (attack 1), not left hanging. |
| Gaps arithmetic | **D4**: "narrows in three places", then two are listed. The story `notes` field says "two places" and is right. |
| Acceptance boxes | All nine re-checked against the files. Every one is true as written. |

The packet's Evidence § 2 step-name diff reproduces exactly (my attack 2). Evidence § 3's numbers
reproduce exactly (below). No claim in the packet was found to be inflated except D1 and D4.

---

## Attack 7 — the gates, run here

```text
$ python scripts/check-doc-paths.py
Doc path check passed for 199 current paths in 11 documents.        exit 0

$ python scripts/check-english.py
English contributor-text check passed for 921 files.                exit 0
```

Both reproduce the packet's figures to the number. `scripts/check-english.py:17` includes
`ROOT / ".github"`, so the new `action.yml` is genuinely inside its 921-file set, and
`git ls-files .github` confirms the action is the only file added there.

```text
$ pwsh scripts/ci-local.ps1          (CARGO_BUILD_JOBS=3)
...
==> python scripts/check-doc-paths.py
Doc path check passed for 199 current paths in 11 documents.

==> python -m unittest scripts/test_check_english.py
Ran 2 tests in 0.006s
OK

==> python scripts/check-english.py
English contributor-text check passed for 922 files.

==> python scripts/completion-catalog.py validate
[completion-catalog] all catalogs valid

==> python scripts/third-party-notices.py --check
THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.                                        exit 0
```

**25 steps, all green. 131 test-result sections, 4533 passed, 0 failed, 24 ignored.**

These are the same totals `BUG-0063` / `BUG-0064` recorded on `f315bf5e` (25 steps, 131 sections,
4533 passed, 0 failed) — which is the point of running it: US-0109 changes no code, and the local
gate proves that the two doc edits and the new `.github/` file changed nothing the gate reads.
The 922-file count for `check-english.py` is 921 plus this evidence document.

Other steps of note from the same run: `vt-public-api.py --diff-platforms` → "the delta is 6
lines, all inside `oneterm_vt::pty`"; `verify-dependency-graph.py` → "20 workspace packages and 20
explicit members"; `cargo test -p oneterm-vt --features vt-paranoid` and `--features regex` both
green, which is the pair that CI now runs in `vt-package`.

---

## Observations (not defects)

- Wall-clock shape: `vt-package` gains two full `oneterm-vt` test compiles and runs; the two
  quality jobs each lose one or more. Since `workspace-quality` (GPUI + apt + full workspace test)
  and `windows-quality` are the long poles, moving work onto the short `vt-package` job should
  shorten the critical path, not lengthen it. Unmeasured — no runner here.
- `scripts/ci-local.{ps1,sh}` remain a strict superset of the union of all CI Rust/Python checks
  (`cargo-deny` behind `-Full`, and the two never-gating recorder jobs excluded). Re-read line by
  line; the packet's "no change needed" reasoning is correct — their comments cite packets, never
  CI jobs.
- The composite's `Read pinned toolchain` step keeps the `[ -n "$channel" ] || exit 1` guard, so a
  malformed `rust-toolchain.toml` still fails loudly, now in one place instead of six.

## What remains unverified

The workflow is **never executed** in this environment — there is no GitHub runner, and none was
installed. Everything above is static analysis of GitHub Actions semantics plus source scans. The
first real proof is the next push, exactly as the packet says.

---

# Re-verification of the acceptance rework

Under test: `22027a6c` ("fix(ci): correct the macOS rationale and four record defects"), on top of
this document at `3efaeca7`. Targeted pass over the five findings only; the workflow
restructuring was not re-litigated, and `ci-local` was not re-run because nothing failed.
Date: 2026-09-16.

## Verdict: **PASS.** All five defects are fixed, and no fix introduced a new false statement.

### D1 — the macOS rationale (was MEDIUM) — **fixed in all five places**

| Place | Now says |
| --- | --- |
| `IN-0041.md` § Source 2 | "The macOS job does *compile* that file — `oneterm-local-shell` … depends on `oneterm-vt` with `features = ["pty"]` — but it has never **run** `oneterm-vt`'s own test targets" |
| `IN-0041.md` § Project Impact | "so the engine's own test targets run there for the first time" |
| `high-level-design.md` | "so the engine's own test targets run there for the first time … The file was already *compiled* on macOS" |
| packet § Outcome + § Context | "the point is **running**, not compiling" and "`pty/unix.rs` was compiled on macOS before this change" |
| `ci.yml` macos-tests comment | "It was already *compiled* here, as a dependency of `oneterm-local-shell` … but compiling a crate as a dependency never builds its `#[cfg(test)]` modules" |
| rework commit body (`22027a6c`) | corrects `aa2bf37f`'s claim explicitly, without rewriting history |

The BUG-0063 attribution is fixed too. No text now says or implies that macOS would have caught
it: `ci.yml` cites it only as evidence that "`cfg(unix)` is two platforms, not one", and the
packet says outright "BUG-0063 itself was *caught* by the Linux job, not missed by macOS". The
same correction is carried into the `intake.summary` and `story.notes` snippet strings.

Automated sweep of the five files for any surviving form of the old claim
(`never compil|not compil|nothing currently builds|does not build|only the glibc one|is
unverified|only job that compiles`):

```text
--- 9 hits, every one of them either a correct statement
    ("compiling a crate as a dependency does not build its #[cfg(test)] modules",
     "the point is **running**, not compiling",
     "the workflow's *execution* is unverified")
    or the D1-D5 rework table quoting the old wrong wording in order to record it.
--- no surviving assertion that pty/unix.rs was uncompiled on macOS.
```

Choosing to correct `aa2bf37f`'s message in the rework body rather than by rewriting an already
pushed-to-nobody-but-shared branch is the right call; the false sentence stays findable next to
its correction.

### D2 — `last_verified_result` (was MEDIUM) — **fixed, and proved rather than eyeballed**

The value is now `"pass"`, with a sentence above the snippet recording why. I extracted the
snippet's two `INSERT`s with `ast`, checked the shapes, and **executed the snippet** against a
throwaway SQLite file carrying the documented CHECK constraints (the main checkout's
`harness.db` was never opened):

```text
intake  columns=11 values=11  placeholders=11
story   columns=17 values=17  placeholders=17
   risk_lane = 'normal'   status = 'implemented'
   unit_proof=0 integration_proof=0 e2e_proof=0 platform_proof=1
   last_verified_result = 'pass'
inserted story rows: 1     inserted intake rows: 1
```

Both rows insert cleanly. The earlier `"passed"` would have raised `IntegrityError` here.

### D3 — `action.yml` vs `release.yml` (was LOW) — **fixed, and the stated reason is true**

The header now says "This is the one copy for `ci.yml`" and names `release.yml`. I checked the
reason against the file rather than taking it:

```text
.github/workflows/release.yml:179  uses: dtolnay/rust-toolchain@4cda84d5…
                              182    targets: ${{ matrix.target }}
.github/workflows/release.yml:184  uses: Swatinem/rust-cache@e18b4977…
                              186    key: ${{ matrix.target }}
```

`setup-rust` takes neither `targets` nor `key`, so `release.yml` is genuinely **not** a drop-in
caller — the decision to reword rather than convert is correct, and declining to add two inputs
for one caller is the right shape. The arithmetic checks out too: a SHA bump was 7 edits (6 in
`ci.yml` + 1 in `release.yml`) and is now 2, "five fewer than before".

### D4 — the Gaps count (was LOW) — **fixed**

Packet line 491 now reads "Coverage genuinely narrows in **two** places", matching the two it
lists and the `story.notes` string that always said two. The `components` empty-string bullet was
also restructured from an open worry into a closed finding citing this document — accurate, since
attack 1 settled it against `dtolnay/rust-toolchain`'s pinned `action.yml`.

### D5 — the archived table (was LOW) — **fixed, and fixed the right way**

`docs/spec-intakes/IN-0029-vt-engine/evidence/US-0087-verify.md` gains a three-line dated note
**under** the table instead of having the table rewritten — the correct treatment for archived
evidence of a past run. The note's content is accurate: `vt-paranoid` is now `vt-package`-only,
`macos-tests` gained `-p oneterm-vt`, and the `ci-local` column is indeed unchanged. The finding
is also recorded in the packet's Owning Docs Reviewed as found by verification rather than by the
original sweep, which is the honest bookkeeping.

### The Python scan the packet now quotes — **reproduces exactly**

The packet replaced the `grep` bullet (whose pattern this shell mangles) with a Python `re` scan.
Re-run independently here:

```text
diagnostics sites: 42 in 5 files
  D crates/local-shell/src/event_loop.rs
  D crates/ssh/src/transport.rs
  D crates/terminal-view/src/render/{diagnostics,element,state}.rs
files with a platform cfg: 11
  P local-shell/src/{event_loop_tests,session,session_terminal,session_tests}.rs
  P ssh/src/agent.rs, ssh/src/sftp_task/{sftp_task_tests,transfer}.rs
  P terminal-view/src/input/{keys,keys_tests}.rs, panel/terminal_panel.rs,
    terminal_view/view_tests.rs
intersection: []
```

42 / 5 / 11 / empty — the packet's numbers to the file. The added parenthetical ("The three
*crates* do contain platform cfgs … the claim is about the diagnostics sites, not about the
crates") is exactly the wording note this document asked for.

### Gates re-run after the rework

```text
$ python -c "yaml.safe_load(...)"   yaml ok: .github/workflows/ci.yml
                                    yaml ok: .github/actions/setup-rust/action.yml
$ python scripts/check-doc-paths.py Doc path check passed for 199 current paths in 11 documents.   exit 0
$ python scripts/check-english.py   English contributor-text check passed for 922 files.           exit 0
```

`ci-local.ps1` was not re-run: the rework touches only comments, Markdown and YAML text, no Rust
and no script, and nothing failed. Its result from the first pass (25 steps, 131 sections, 4533
passed, 0 failed, exit 0) still stands.

## One observation, no action required

The packet now ticks `[x] Reopened (acceptance rework)` while the snippet keeps
`status="implemented"`. Both conventions exist in this repository — `IN-0036/US-0095` ticks
Reopened and writes `implemented`; `IN-0039/US-0105` ticks Reopened and writes `reopened` — so
this is a pre-existing repo-wide inconsistency, not something this packet introduced.
`implemented` is defensible as the end state after a same-day rework, and the `story.notes`
string records the reopening explicitly.

**The workflow is still never executed here.** Both passes are static; the first real proof
remains the next push.
