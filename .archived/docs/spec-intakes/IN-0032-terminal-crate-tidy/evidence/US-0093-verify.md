# US-0093 — independent verification

**Verdict: PASS-WITH-NOTES.**

B3 (corpus move) and B5 (shared `ByteBudget`) both do what the packet claims, and every
claim the packet makes that I could re-measure re-measured to the same number. The three
defects below are all documentation/closure defects in `IN-0032.md`; none touches code,
data or the licence position.

- Verifier: independent agent, different from the implementer.
- Base: `git reset --hard 103b97e`; `git log --oneline -4` → `103b97e`, `7dbb1f8`,
  `ba0e5d9` on `394f075`.
- Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a4a04eeefb15159e6`
  (nothing committed, nothing pushed; only file changed by me is
  `crates/terminal/src/backend/byte_budget.rs`, my own tests, listed in § 9).

---

## 1. Defects

### D1 — `IN-0032` is declared complete with a `US-0093`-owned Open Decision left unticked (severity: medium, documentation)

`docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md:303-306`

```
- [ ] **Whether the `corpus/oneterm/` recording should stay adjacent to the engine for provenance**
  (§(d).4). `US-0072`'s blessing rules (`R-58`, "nothing blesses") may make the owner prefer it
  next to `crates/vt`. Default: move it with the rest — it has the same single reader — and record
  the provenance in the moved `NOTICE`. Owner: `US-0093`'s implementer. Not blocking.
```

The decision is explicitly owned by `US-0093`'s implementer. The implementer *did* take the
default (the `oneterm/` subtree moved with the rest; provenance is in the moved `NOTICE`) and
*did* say so in the commit body of `ba0e5d9`, but the checkbox is still `[ ]` and neither the
new `### Intake status: complete` block nor the Open Decisions section records the choice.
The sibling `terminal-diagnostics` decision was closed correctly, with a `[x]` "Settled by
`US-0090`" entry added above the original question (line 282) — that is the pattern this one
should have followed.

Net effect: the intake says "complete" while one of its three open decisions is, on the page,
still open and unanswered.

*Repro:* `Select-String -Path docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md -Pattern '\[ \] \*\*Whether the .corpus/oneterm'`

*Fix:* add a `[x]` settled line above line 303 naming `US-0093` and the outcome (moved with
the rest; provenance in `crates/tools/corpus/NOTICE`), as `US-0090` did at line 282.

### D2 — the closure block points at no evidence file (severity: low, documentation)

`docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md:215-232`

The task for this block was that it point at the four packets' evidence. It does not: it says
"recorded in their own Evidence" and "see the packet's Evidence and Gaps" without a single
path. The only `evidence/…` path anywhere in `IN-0032.md` is `evidence/US-0090-verify.md` at
line 178, which pre-dates this packet. There are in fact only three verify files on disk —
`US-0090-verify.md`, `US-0091-verify.md`, `US-0092-verify.md` — plus `US-0092`'s screenshots;
`US-0093` has none (this file is the first). Declaring the intake complete before the last
packet had an independent verification record is the substance of the gap; the missing links
are the symptom.

*Repro:* `Get-ChildItem docs/spec-intakes/IN-0032-terminal-crate-tidy/evidence` → no
`US-0093-*`; `Select-String IN-0032.md -Pattern 'evidence/'` → one hit, line 178.

### D3 — the closure block's line accounting double-counts (severity: low, prose)

`docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md:204-205`

> "B5's backends are −23 lines against the packet's ≥ −20 gate, with the shared type costing
> +29 production lines back"

−23 saved and +29 "back" reads as a net of +6. The real net is **+29**, which is what the
packet itself says, correctly and unambiguously, at
`US-0093-corpus-move-and-shared-byte-budget.md:446-451`: backends −23 (`event_loop.rs` +6/−19,
`transport.rs` +6/−16), shared file 77 lines (48 production + 29 test), `backend/mod.rs` +4,
so workspace production +29 and workspace total +58.

I re-derived it: 48 + 4 − 23 = **+29** production; 77 + 4 − 23 = **+58** total
(`crates/terminal/src/lib.rs` is +2/−2, net 0). The packet is right; only the intake's
one-sentence compression of it is wrong.

---

## 2. Notes (not defects)

- **N1 — `docs/license-analysis.md` does not carry the statement that `crates/tools`'
  GPL-3.0-only term does not reach the corpus.** It *does* list the corpus by path and both
  paths were repathed correctly (lines 158 and 162). But the file never mentions `doom-fire`,
  `oneterm-tools`, or the `Apache-2.0 AND GPL-3.0-only` package expression at all — its §2 GPL
  analysis is entirely about the three Zed crates. So the co-location fact now lives only in a
  `crates/tools/Cargo.toml` comment. That is defensible (the statement is about a package
  Cargo.toml, and the licence position is unchanged: Apache-2.0 third-party data inside a
  package whose SPDX expression already contains `Apache-2.0`, never published —
  `publish = false` is inherited from `[workspace.package]`), and `IN-0032.md`'s "not done"
  table names the doom-fire licence split as the thing that would settle it properly. I would
  not spend a packet on it; a one-line cross-reference in `license-analysis.md` § 3 would close
  it for free.
- **N2 — `crates/vt/fuzz/Cargo.toml:17` keeps a pre-existing cwd inconsistency.** The comment
  is now `cp ../../tools/corpus/alacritty-ref/*/recording fuzz/corpus/parser/`. The source is
  correct if read from `crates/vt/fuzz`; the destination is correct if read from `crates/vt`.
  Both halves had exactly this mismatch before the move (`../tests/corpus/…` +
  `fuzz/corpus/parser/`), and the implementer transformed the source consistently with the old
  reading. Not introduced here, but it is the one repathed reference that no tool checks.
- **N3 — the root `NOTICE` pointer is at line 20, not 19.** The packet's evidence
  (`US-0093-…md:410`) says "line 19", which was true before the paragraph grew a line.
  `crates/tools/corpus/NOTICE` is reachable from the root `NOTICE` and the `alacritty-ref/` +
  `oneterm/` sections are intact (the file moved `R100`).
- **N4 — `ByteBudget::release()` has no underflow guard.** `fetch_sub` past zero panics in
  debug and wraps in release. Both deleted copies behaved identically, so this is not a
  regression — but it is now a `pub` item on a lower crate with no doc line saying "release
  exactly what you reserved". One sentence on the method would be cheap.
- **N5 — `oneterm-core` is technically lower than `oneterm-terminal`.** R10 says "whichever is
  lowest **and fits**". The byte budget is a `PtyTransport`/command-queue concept documented in
  `docs/terminal-backend.md` §6.5 and sits with `PtyTransport` and `OscRouter`; `core` is the
  domain crate and does not fit. `crates/terminal` is the right home. No manifest gained a
  dependency: `crates/local-shell/Cargo.toml:19` and `crates/ssh/Cargo.toml:19` already had
  `oneterm-terminal.workspace = true`.

---

## 3. B3 — rename purity

```
git diff 394f075..103b97e --name-status -M --find-renames=100% -- crates/vt/tests/corpus crates/tools/corpus
TOTAL:     231
R100:      231
NON-R100:  0          <- required 0, count required 231
```

Independent content checksum (blob hashes from `git ls-tree -r`, path prefix stripped, sorted,
SHA-256 of the resulting manifest):

```
old manifest (394f075, crates/vt/tests/corpus, 231 files)
  ea656197ac91e849a7b3259440e2311374cf3472d62b6158f2815a7d8828beb0
new manifest (103b97e, crates/tools/corpus,     231 files)
  ea656197ac91e849a7b3259440e2311374cf3472d62b6158f2815a7d8828beb0
Compare-Object diff count: 0
```

Every blob hash and every root-relative path is identical. No file was added, dropped or
renamed within the tree.

```
git ls-tree -r 103b97e -- crates/vt/tests/corpus   -> 0 entries (nothing left behind)
Test-Path crates/vt/tests/corpus                    -> False
git status --porcelain -uall -- crates/vt           -> clean (no untracked residue)
Get-ChildItem -Recurse -File crates/tools/corpus    -> 231 files, 2 981 328 bytes
```

### `git check-attr -a` at the new path

`.gitattributes:11` now reads `crates/tools/corpus/** -text -whitespace` (the only line
changed in that file).

```
crates/tools/corpus/alacritty-ref/alt_reset/recording:    text: unset   whitespace: unset
crates/tools/corpus/alacritty-ref/alt_reset/grid.expect:  text: unset   whitespace: unset
crates/tools/corpus/alacritty-ref/alt_reset/config.json:  text: unset   whitespace: unset
crates/tools/corpus/alacritty-ref/alt_reset/state.expect: text: unset   whitespace: unset
crates/tools/corpus/oneterm/sixel_basic/recording:        text: unset   whitespace: unset
crates/tools/corpus/NOTICE:                               text: unset   whitespace: unset
```

The rule travelled. This is the failure mode that would have been silent on a CRLF platform.

### Scoped grep for the old path

`git grep -n "crates/vt/tests/corpus" -- crates/ scripts/ docs/ .github/ Cargo.toml deny.toml NOTICE THIRD-PARTY-NOTICES.md .gitattributes README.md AGENTS.md`

| Area | Hits | Verdict |
|---|---|---|
| `crates/` | 0 | clean |
| `scripts/` | 0 | clean |
| `.github/` | 0 | clean |
| `Cargo.toml`, `deny.toml`, `NOTICE`, `THIRD-PARTY-NOTICES.md`, `.gitattributes`, `README.md`, `AGENTS.md` | 0 | clean |
| `docs/spec-intakes/IN-0029-vt-engine/US-0072, 0074, 0076, 0080, 0086, 0087, 0088` | 19 | historical packet prose (completed packets) — legitimate |
| `docs/spec-intakes/IN-0029-vt-engine/evidence/US-0072, 0080, 0086, 0087, 0088-verify.md` | 15 | historical evidence — legitimate, must not be rewritten |
| `docs/spec-intakes/IN-0029-vt-engine/research/crate-consolidation-audit.md` | 2 | the audit that *asked* for the move — legitimate |
| `docs/spec-intakes/IN-0032.../IN-0032.md:208`, `US-0093-….md` (9) | 10 | this packet describing the move — legitimate |

Every live (non-historical) reference moved. The five live docs that had to change did:
`.gitattributes`, `NOTICE`, `THIRD-PARTY-NOTICES.md` + its generator `scripts/third-party-notices.py`,
`docs/license-analysis.md`, `docs/agents/structure.md`,
`docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` (5 references),
`crates/tools/src/bin/vt-corpus.rs` (`--dir` help text) and `crates/vt/fuzz/Cargo.toml` (see N2).

`docs/agents/structure.md`'s replacement claim `crates/vt/tests/` = `parser_limits.rs,
us0087_cleanup_rows.rs` is accurate (verified on disk).

### Gate scripts

```
python scripts/third-party-notices.py --check  -> THIRD-PARTY-NOTICES.md is up to date.        exit=0
python scripts/check-doc-paths.py              -> passed for 190 current paths in 11 documents. exit=0
python scripts/check-english.py                -> passed for 787 files.                        exit=0
python scripts/verify-dependency-graph.py      -> passed for 21 workspace packages.            exit=0
```

---

## 4. The gate is still real at the new path

```
cargo run -q -p oneterm-tools --release --bin vt-corpus -- check
  ...
  ok    zsh_tab_completion
  45 recordings, 45 passed, 0 failed

cargo run -q -p oneterm-tools --release --bin vt-corpus -- check --dir crates/tools/corpus/oneterm
  ok    sixel_basic
  1 recordings, 1 passed, 0 failed

cargo test -q -p oneterm-tools --test corpus_check
  running 2 tests
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
```

`crates/tools/src/corpus.rs:86-87` now resolves from its own manifest directory
(`Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")`); the `.parent().join("vt/tests/corpus")`
sibling assumption is gone. `crates/tools/tests/corpus_check.rs:22-27` still asserts
`recordings.len() == 45`, so a recording lost in a future move fails the build rather than
quietly shrinking the gate.

### Tamper (a different recording from the implementer's `alt_reset`)

Target: `crates/tools/corpus/alacritty-ref/zsh_tab_completion/grid.expect`, one byte at
offset 3000 (`0x6f` → `0x6e`, inside a data run, well past the header).

```
BEFORE sha256: 4727D89F1BE608860DCA7B0688B7B8B1683811AFF9EFD0E46BCD547545854C70
AFTER  sha256: DB1C9961EAAD1B6FB7C62ED37A317051573306C55018D33133B6F1168BDB62BA

cargo test -q -p oneterm-tools --test corpus_check
  the_engine_matches_the_frozen_alacritty_expectations --- FAILED
  panicked at crates\tools\tests\corpus_check.rs:68:5:

  zsh_tab_completion:
    grid: row 27 col 34 fg: expected "nForegrnund", got "nForeground"
    grid: row 27 col 35 fg: expected "nForegrnund", got "nForeground"

  test result: FAILED. 1 passed; 1 failed; 0 ignored
```

The gate names the recording, the row, the column and the field. Restored:

```
git checkout -- crates/tools/corpus/alacritty-ref/zsh_tab_completion/grid.expect
RESTORED sha256: 4727D89F1BE608860DCA7B0688B7B8B1683811AFF9EFD0E46BCD547545854C70   (== BEFORE)
git status --porcelain -> clean
```

---

## 5. Licence

```
cargo deny check licenses bans
  bans ok, licenses ok
  exit=0
```

- `crates/tools/Cargo.toml`'s new sentence — "`corpus/` (US-0093) is third-party Apache-2.0
  test data carried under its own `corpus/NOTICE`: it is not source, is compiled into nothing,
  and the GPL term above does not reach it" — is **accurate**. The package expression is
  `Apache-2.0 AND GPL-3.0-only`, which already contains `Apache-2.0`; the GPL term exists only
  for `src/bin/doom-fire.rs`; nothing in `corpus/` is compiled (it is opened at runtime by
  `corpus.rs` and by `tests/corpus_check.rs`); and `publish.workspace = true` resolves to
  `publish = false` (`Cargo.toml:32`), so the data is never redistributed as a package. The
  `deny.toml:60-64` crate-scoped `GPL-3.0-only` exception for `oneterm-tools` is unchanged and
  still describes the truth.
- `docs/license-analysis.md` **does** list the corpus by path and both references were
  repathed (lines 158, 162). It does **not** carry the same GPL-does-not-reach-it statement —
  see N1. Its standing claim "Since `US-0087` this corpus is the only Alacritty-derived
  material in the repository" remains true.
- `crates/tools/corpus/NOTICE` **is** discoverable from the root `NOTICE` (line 20 after the
  edit, not 19 — see N3): "...unmodified and not linked into any binary; see
  `crates/tools/corpus/NOTICE`." The moved NOTICE is byte-identical (`R100`) and still carries
  the Apache-2.0 text, the upstream/via URLs, revision
  `fcf32feacb367b75ec84dd40f041e4fd411d3cc1`, the Changes-made-by-OneTerm list and the
  `oneterm/` provenance section.

### The advisories failure — confirmed, and pre-existing on `main`

```
cargo deny check advisories
error[vulnerability]: TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries
    ┌─ …/Cargo.lock:563:1
563 │ rustls 0.23.40 registry+https://github.com/rust-lang/crates.io-index
    ├ ID: RUSTSEC-2026-0285
    ├ Advisory: https://rustsec.org/advisories/RUSTSEC-2026-0285
    ├ Announcement: https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc
advisories FAILED
exit=1
```

**Advisory id: `RUSTSEC-2026-0285`**, against `rustls 0.23.40`, reached through `russh`.

`git diff 394f075..103b97e -- Cargo.lock` is **empty** — the lock is byte-identical to `main`.
`deny.toml` is also unchanged, and the only manifest this branch touched
(`crates/tools/Cargo.toml`) changed a comment, not a dependency or a licence field.
`cargo deny check advisories` reads the lock plus the advisory database and nothing else, so
**`main` fails identically today**.

Would CI be red on `main`? **Yes.** `.github/workflows/ci.yml:76-84` has a `cargo-deny` job
running `check licenses bans advisories` as one command, so the job fails on the advisory.
`scripts/ci-local.ps1:46-48` only runs `cargo deny` under `-Full`, so `ci-local` without
`-Full` is green (as run in § 7) and `ci-local -Full` would be red — on this branch and on
`main` alike. This is **not** a US-0093 regression; it is a `rustls` bump the repository owes
independently.

---

## 6. B5 — semantics against both deleted copies

`git diff 394f075..103b97e -- crates/local-shell/src/event_loop.rs crates/ssh/src/transport.rs`
→ `2 files changed, 12 insertions(+), 35 deletions(-)` (net −23).

| Property | `local-shell` before | `ssh` before | `ByteBudget<LIMIT>` now | Identical? |
|---|---|---|---|---|
| reserve | `fetch_update(AcqRel, Acquire, \|c\| c.checked_add(len).filter(\|&n\| n <= LOCAL_COMMAND_BYTE_BUDGET)).is_ok()` | same, `SSH_COMMAND_BYTE_BUDGET` | `fetch_update(AcqRel, Acquire, \|c\| c.checked_add(bytes).filter(\|&n\| n <= LIMIT)).is_ok()` | **yes** |
| `checked_add` guard | **present** | **present** | present | **yes — it existed in *both*, neither was silently changed** |
| release | `fetch_sub(n, AcqRel)` (two sites) | `fetch_sub(bytes, AcqRel)` (one site) | `fetch_sub(bytes, AcqRel)` | **yes** |
| read | n/a in prod; tests used `load(Acquire)` | `load(Relaxed)` in `diagnostics()` | `load(ordering)` — caller passes | **yes**, both orderings preserved verbatim |
| construction | `AtomicUsize` inside `#[derive(Default)] struct ShellControl` | `Arc::new(AtomicUsize::new(0))` | `#[derive(Debug, Default)]`, `Arc::new(ByteBudget::default())` | **yes**, `ShellControl` keeps its derived `Default` |
| limit constant | `4 * 1024 * 1024` | `4 * 1024 * 1024` | type parameter, each backend keeps its own | **yes**, both still 4 MiB and still `pub(crate)` in their own crate |

Release-on-error / release-on-drop paths — every `release(` call site and its error return:

| Site | Path | Before | After |
|---|---|---|---|
| `event_loop.rs:130` | `sender.try_send` failed → `Err(WouldBlock/BrokenPipe)` | `fetch_sub(length)` then return the mapped error | `release(length)`, same mapped error | 
| `event_loop.rs:352` | PTY write returned `Ok(n)` | `fetch_sub(n)` | `release(n)` |
| `transport.rs:167` | `cmd_tx.try_send` failed after a successful reserve | `release_write_bytes(bytes.len())` | unchanged wrapper, now `release()` |
| `transport.rs:131-132` | `release_write_bytes`, called from `task.rs:123` after delivering/dropping a write | `fetch_sub(bytes, AcqRel)` | `release(bytes)` |

Every path is preserved one-for-one. The refusal branches are unchanged: `local-shell` still
returns `io::ErrorKind::WouldBlock` with "local-shell command byte budget is full"; `ssh` still
calls `record_budget_full()` then returns `TerminalError::QueueFull`, and still checks
`is_closing()` **before** reserving.

### Backend tests byte-identical to `main`

```
git diff 394f075..103b97e --stat -- 'crates/local-shell/**' 'crates/ssh/**'
 crates/local-shell/src/event_loop.rs | 25 ++++-------------
 crates/ssh/src/transport.rs          | 22 ++++-------------
 2 files changed, 12 insertions(+), 35 deletions(-)
```

No `*_tests.rs` appears in the diff at all. `crates/local-shell/src/event_loop_tests.rs`
(including the loopback worst-wait timing tests) and `crates/ssh/src/transport_tests.rs` are
**unchanged**, and their four budget assertions
(`event_loop_tests.rs:43,53,62-63`; `transport_tests.rs:98-111,138`) still name their own
backend's constant and still pass. `transport_tests.rs:103` still reads through
`diagnostics().queued_write_bytes`, which now goes through `ByteBudget::load(Relaxed)` — the
same ordering as before.

### Concurrency test (mine)

8 threads × 20 000 rounds × 7-byte chunks against `LIMIT = 20`. `LIMIT < THREADS * CHUNK`
deliberately, so the ceiling is genuinely contended — with the obvious `LIMIT = 64` the
assertion can never fire (8 × 7 = 56 < 64) and the test is decorative.

```
cargo test -q -p oneterm-terminal --lib verify_us0093
  running 3 tests
  test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 287 filtered out
```

**Negative control** — temporarily relaxing `reserve` to `next <= LIMIT + 8` in the production
line and re-running:

```
concurrent_reserve_and_release_never_exceed_the_limit_and_return_to_zero --- FAILED
  panicked at crates\terminal\src\backend\byte_budget.rs:112:29:
  observed 21 bytes over LIMIT 20
  observed 28 bytes over LIMIT 20
a_refused_reservation_leaves_the_total_untouched --- FAILED
  "two over the remaining one is refused"
```

The tests have teeth. The production line was restored and re-verified
(`checked_add(bytes).filter(|&next| next <= LIMIT)`) before the CI run in § 7.

`287 filtered out` independently confirms the packet's `oneterm-terminal` 285 → **287** claim.

---

## 7. `pwsh scripts/ci-local.ps1`

Run on the **pristine** `103b97e` tree (my tests removed for the run, re-applied after), so the
totals are directly comparable to the implementer's.

```
exit=0
ci-local: all checks passed.

sections (test result lines): 60
passed = 1973   failed = 0   ignored = 14
```

**60 / 1973 / 0 / 14 — exact match with the implementer's report** and with the packet's
"`main` baseline 60 / 1 971 / 0 / 14, +2 for the `ByteBudget` tests".

Steps run, all green: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
`cargo clippy … --features oneterm-app/terminal-diagnostics -D warnings` (the US-0090 step),
`cargo test --workspace`, `cargo test -p oneterm-vt --features vt-paranoid`,
`verify-dependency-graph.py`, `check-doc-paths.py`, `test_check_english.py`, `check-english.py`,
`completion-catalog.py validate`, `third-party-notices.py --check`.

---

## 8. Intake closure (`### Intake status: complete`)

| Check | Result |
|---|---|
| Every packet ticked | **Yes** — `US-0090` (174), `US-0091` (183), `US-0092` (195), `US-0093` (201) all `[x]` |
| Points at the four evidence files | **No** — see **D2** |
| Points at the `terminal-diagnostics` decision | **Partly** — names it as "settled and recorded in Open Decisions" and describes the outcome correctly (the CI step found two dead items in `crates/ssh/src/transport.rs`), matching the `[x]` entry at line 282. No `DEC-` id or line reference, but the intake carries no `DEC-` for it. |
| The `oneterm/`-provenance decision | **Not closed** — see **D1** |
| "Deliberately not done" table consistent with the audit's option list | **Yes** — all seven present and each reason checks out |

The seven rows, checked one by one:

| Row | Consistent? |
|---|---|
| Option C (folding `pty`/`local-shell`/`ssh` into `terminal`, `agent-ui` into `terminal-view`) | Yes — owner rejected; matches `IN-0032.md:308-310` and the High-Level Design |
| B2 (`TerminalContent`'s 11 forwarding accessors) | Yes — audit recommends against; matches line 309 |
| `frame.rs` engine-vocabulary mirror | Yes — same sentence at line 310 |
| Splitting `doom-fire.rs` out for cleaner licence metadata (audit §1.4) | Yes — and it correctly names what `US-0093` did instead (the `crates/tools/Cargo.toml` sentence). See N1 |
| Per-OSC `Vec` allocation | Yes — audit did not recommend it |
| Unifying the two 4 MiB constants | Yes — and the code matches the stated reason: both are still 4 MiB, both still live with their backend, both still named by their own tests |
| Rest of `crates/terminal-view` (`shapes.rs`, `row_plan.rs`) | Yes — matches the still-open decision at line 300-302, which is correctly left `[ ]` |

Other claims in the block, re-measured: 231 files as `R100` with a zero diff ✓ (§ 3);
identical root-relative content checksum ✓ (§ 3); 45/45 and 1/1 ✓ (§ 4); one-byte tamper still
fails ✓ (§ 4); `.gitattributes` / root `NOTICE` / `third-party-notices.py` /
`THIRD-PARTY-NOTICES.md` repathed and `--check` green ✓ (§ 3); backends −23 ✓; shared type +29
production ✓ (but see **D3** for the wording); `oneterm-terminal` 285 → 287 ✓; no other crate's
count changed ✓ (workspace total 1971 → 1973).

---

## 9. Trailers

All three commits carry both lines, verbatim:

```
103b97e docs(terminal): US-0093 implemented; IN-0032 is complete
7dbb1f8 refactor(terminal): one ByteBudget for both backends' write reservation
ba0e5d9 refactor(tools): move the VT parity corpus next to its only reader

  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

Each also carries a `Refs: US-0093 (IN-0032…)` line and a Conventional-Commits subject.

---

## 10. My tests (kept in the worktree, uncommitted)

`crates/terminal/src/backend/byte_budget.rs` — one added module `mod verify_us0093` (three
`#[test]`s), appended after the packet's own `mod tests`. Production code untouched.

| Test | What it pins |
|---|---|
| `concurrent_reserve_and_release_never_exceed_the_limit_and_return_to_zero` | 8 threads, 20 000 rounds, 7-byte chunks, `LIMIT = 20` (contended on purpose): the total is never *observed* above `LIMIT`, lands back on exactly 0, and the run actually granted reservations |
| `a_refused_reservation_leaves_the_total_untouched` | at the real 4 MiB ceiling: a 2-byte reserve over a 1-byte remainder is refused and reserves nothing; the last byte still fits |
| `a_zero_byte_reservation_always_succeeds` | `reserve(0)` succeeds even at the ceiling — both deleted copies did this; pinned so a later change is deliberate |

Also kept: `…/scratchpad/us0093-verify-tests.patch` (the same diff) and
`…/scratchpad/ci-local.txt` (the full § 7 log).
