# US-0087 — independent verification

Verifier: a different agent from the implementer. Worktree
`D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-aa4e796412c8e15bd`,
reset to `cbc6d7b` before anything was read. Diff under review: `65139c5..cbc6d7b`
(111 files, 1 508 insertions, 27 453 deletions).

No GUI was launched, no `oneterm.exe` was enumerated, started or stopped. Nothing was
pushed. Only this worktree was touched.

## Verdict

**PASS-WITH-NOTES.** Every promise in the packet that can be mechanically checked holds:
the fork is gone from the tree, the manifests, the lock file, the resolved graph, CI and
the notices; the drift gate is real (proved by two tamper tests); the remaining harness
runs on the new engine; the two cleanup rows behave as described and survive six tests I
wrote myself. Eight notes follow, all minor — none blocks the merge.

---

## 1. Completeness of the removal

### Mechanical facts

| Check | Result |
| --- | --- |
| `git ls-files vendor` | **0 files** |
| `vendor/` on disk | absent |
| `grep -rn "vendor/"` over `crates/ scripts/ .github/ Cargo.toml Cargo.lock deny.toml AGENTS.md README.md NOTICE THIRD-PARTY-NOTICES.md` | exactly **one** hit: `crates/terminal/src/backend/osc_router.rs:305` (declared US-0088 leftover) |
| `grep -n "vte\|alacritty" Cargo.lock` | **0 hits** |
| `grep -n "git+" Cargo.lock` | **0 hits** |
| `cargo metadata --format-version 1 --locked` | 4.84 MB, **0** `git+` sources; no `alacritty_terminal`, no `vte` package |
| root `Cargo.toml` | no `[patch]`, no `exclude`, no fork workspace deps, no fork `[profile.*.package]` override |
| `deny.toml` | fork `allow-git` entry gone; `portable-pty` ban reason re-pointed at `oneterm-pty` |
| `.github/workflows/ci.yml` | `refresh.sh --check` step gone; both `vendor/**` path triggers gone |
| `scripts/ci-local.{ps1,sh}` | `--full` reduced to `cargo deny` only |
| `us0081` / `us0081_parity` | **0 hits** anywhere under `crates/ scripts/ .github/ Cargo.toml` |
| `vt-diff` / `vt_diff` | **0 hits** anywhere under `crates/ scripts/ .github/ Cargo.toml docs/agents/ docs/README.md` |

The one `vte`-shaped string in `cargo metadata` is `vte_generate_state_changes`, and it is
**not in the build graph**: it appears only as a declared *dev-dependency of the
third-party crate `anstyle-parse` 1.0.0*, which is never resolved (Cargo.lock has no entry
for it). So the honest answer to "does `vte` appear transitively" is **no**.

### Grep classification

Every surviving hit for `alacritty` / `vte` / `vendor` / `bless` / `differential`,
classified:

| Class | Where | Verdict |
| --- | --- | --- |
| Frozen corpus data + its `NOTICE` | `crates/vt/tests/corpus/**` (45 `recording` blobs, 46 `grid.expect`, `NOTICE`) | legitimate — data and attribution, not editable |
| Apache-2.0 provenance headers | `crates/pty/src/windows*.rs`, `crates/vt/src/{reflow,selection,grid/row,terminal/dispatch,graphics/sixel}.rs` | legitimate — every one now names the *upstream* path `alacritty_terminal/src/x.rs`, which exists; removing them would weaken an attribution |
| Historical prose ("the fork did X", "retired at US-0087") | `crates/terminal/src/{lib,handle,color_classification,osc_color}.rs`, `crates/vt/src/{events,parser,terminal,render}/*.rs`, `crates/tools/src/*`, `crates/vt/tests/parser_limits.rs` | legitimate — past tense, no dangling path, no live claim |
| Corpus tool naming its own directory (`alacritty-ref/`) | `crates/tools/src/{corpus.rs,bin/vt-corpus.rs}`, `crates/tools/tests/corpus_check.rs`, `crates/vt/fuzz/Cargo.toml` | legitimate — the directory is called that |
| **Accepted US-0088 leftovers** | `crates/terminal/src/osc.rs:3,:4,:6`; `crates/terminal/src/backend/osc_router.rs:305`; `docs/osc-sequences-checklist.md:129,144,163,188,283`; `docs/osc-agent-status.md:658` | accepted — `docs/osc-*.md` and those two files are named out of scope in the packet's Scope block |
| **Dangling / stale (defects)** | `crates/local-shell/src/transport.rs:16`; `docs/sftp-browser-design.md:98` | see defects 1 and 2 |

`docs/agents/structure.md` and `docs/agents/dependencies.md` were checked specifically, as
asked: **neither describes `vendor/` or the patch workflow any more.** `structure.md` has
zero `vendor`/`patch` hits; `dependencies.md`'s only mentions are past tense ("the
vendored fork ... was deleted at `US-0087`", "There is no `[patch]` section at all any
more"). `crate-dependency-rules.md` likewise. `grep -rn "vendor/" docs/*.md docs/agents/
scripts/*.py scripts/*.md` is **empty**.

---

## 2. The drift gate is real — two tamper tests

### Tamper A — the engine's behaviour

`crates/vt/src/grid/screen.rs:1192`, an off-by-one in the `EL 1` fill (the cursor cell
stops being erased):

```rust
-            LineClear::Left => 0..col.saturating_add(1).min(cols),
+            LineClear::Left => 0..col.min(cols),
```

`cargo test -p oneterm-tools --test corpus_check` → **exit 101**:

```
test the_engine_matches_the_frozen_oneterm_expectations ... ok
test the_engine_matches_the_frozen_alacritty_expectations ... FAILED

erase_in_line:
  grid: row 28 col 138 content: expected "0020", got "002b"
vttest_cursor_movement_1:
  grid: row 13 col 9 content: expected "0020", got "0045"
  ... and 2 more
test result: FAILED. 1 passed; 1 failed
```

Caught by **`the_engine_matches_the_frozen_alacritty_expectations`**, naming the
recordings `erase_in_line` and `vttest_cursor_movement_1` and the exact cells. Reverted.

### Tamper B — one frozen expectation byte

`crates/vt/tests/corpus/alacritty-ref/alt_reset/grid.expect`, row 0 changed from
`106*0020;...` to `1*0041;... 105*0020;...`:

```
test the_engine_matches_the_frozen_alacritty_expectations ... FAILED

alt_reset:
  grid: row 0 col 0 content: expected "0041", got "0020"
  grid: row 0 col 0 hyperlink: expected "- 105*0020", got "-"
test result: FAILED. 1 passed; 1 failed
```

Caught by the same test, naming **`alt_reset`**. Reverted with `git checkout --`; re-run
green (`2 passed; 0 failed`). `git status --porcelain` afterwards shows only my own
untracked scratch test file.

Both halves of the gate — engine drift and expectation drift — fail loudly and name the
recording. The gate is not decorative.

---

## 3. `vt-corpus`, `grep-deviations` and `vt-bench` (release)

All run as `cargo run -q -p oneterm-tools --release --bin <tool> -- ...`.

```
### vt-corpus check (default = alacritty-ref)
45 recordings, 45 passed, 0 failed          EXIT=0

### vt-corpus check --dir crates/vt/tests/corpus/oneterm
ok    sixel_basic
1 recordings, 1 passed, 0 failed            EXIT=0

### vt-corpus check --engine old
vt-corpus: the old engine was deleted at US-0087; the frozen expectations are what
it left behind                              EXIT=1
```

`vt-corpus grep-deviations` runs on `oneterm_vt::parser` and emits the full table —
`C1`–`C11`, `G3`, `D12` — with per-recording counts (e.g. `D12 | Reverse wrap implemented
| CSI ? 45 h/l | vttest_cursor_movement_1 (?45: 0 set, 1 reset); ...`). The port is real,
not a stub.

`vt-bench all --mib 2` — **all five tiers print numbers** on `oneterm-vt`:

- Tiers 1–2, ten fixtures: parser 418.2–1460.5 MiB/s, parse+grid 41.4–201.9 MiB/s.
- Tier 3, 600 frames at 7 200 cells: 1.2–22.6 µs/frame.
- Tier 4, resize 80x24→100x40: grow 8 / 2 236 / 26 211 µs at 0 / 10 000 / 100 000
  scrollback rows.
- Tier 5, live heap after 10 000 rows: 1 364–1 365 bytes/row across plain / unicode /
  styled / mixed.

These match the packet's reported shape; tier 4 differs by a few percent (2.2 ms vs the
packet's 2.6 ms, 26.2 ms vs 27.8 ms) — machine variance on an unpinned benchmark that
gates nothing (R-29). No discrepancy.

---

## 4. The two cleanup rows — read, then re-tested independently

**Reverse wrap.** `crates/vt/src/grid/screen.rs:667-670`: the floor at the crossing site
became `self.row_of_index(self.region.top)` instead of `self.screen_top()`. Verified that
`row_of_index(0) == screen_top()` (`screen.rs:358,380`), so a full-screen region leaves
the old history guard byte-for-byte identical — the claim checks out. `row_of_index`
clamps out-of-range indices, so a stale `region.top` cannot panic.

**LNM.** `Mode::LineFeedNewLine` joined `Mode::inert_state` (`mode.rs:219`) and the ANSI
`DECRQM` arm now consults it before the live bit (`dispatch.rs:1111-1116`).

I wrote six tests of my own, from the **wire only** (public API, real bytes, none of the
implementer's fixture) in `crates/vt/tests/verify_us0087.rs` (left untracked in the
worktree for re-running; not committed):

```
running 6 tests
test verify_mode_ansi_walk_covers_every_recognised_ansi_code ... ok
test verify_lnm_does_not_make_lf_imply_cr ... ok
test verify_reverse_wrap_blocked_at_a_non_zero_region_top ... ok
test verify_reverse_wrap_above_the_region_is_also_blocked ... ok
test verify_reverse_wrap_crosses_once_the_region_is_dropped ... ok
test verify_decrqm_lnm_is_consistent_and_never_claims_set ... ok

test result: ok. 6 passed; 0 failed; 0 ignored
```

What each one pins:

- **(a)** `verify_reverse_wrap_blocked_at_a_non_zero_region_top` — 8x6, `CSI ? 45 h`,
  nine glyphs so row 0 is WRAPPED, `CSI 2;5 r` (region = indices 1..5, top ≠ row 0),
  `CSI 2;1 H`, `BS` → cursor stays at `(1, 0)`. **Passes.**
- **(b)** `verify_reverse_wrap_crosses_once_the_region_is_dropped` — same setup, blocked
  while the region stands, then `CSI r`, `CSI 2;1 H`, `BS` → cursor `(0, 7)`.
  **Passes.**
- **(c)** `verify_decrqm_lnm_is_consistent_and_never_claims_set` — `CSI 20 $p` answers
  `ESC [ 20;2 $y` before any `h`/`l`, after `CSI 20 h`, and after `CSI 20 l`; the mode
  that *does* have a reader (IRM, `CSI 4 h`) still answers `ESC [ 4;1 $y`. **Passes.**
- `verify_lnm_does_not_make_lf_imply_cr` — see the note below.
- `verify_mode_ansi_walk_covers_every_recognised_ansi_code` — exhaustive over codes
  `0..=2000`: every code `Mode::from_ansi` recognises is in `Mode::ANSI`, and every entry
  of `Mode::ANSI` round-trips through `from_ansi`. The set is exactly
  `{4 → Insert, 20 → LineFeedNewLine}`. **Nothing was lost when LNM joined the table.**
  (`from_ansi` at `mode.rs:187-193` maps only 4 and 20; `Mode::ANSI` at `mode.rs:227`
  lists exactly those two.)

**Note on the task's expectation (c), second half — "LF behaves as CR+LF when LNM is
set".** It does **not**, and that is deliberate, not a defect: deviation D9 says LNM is
tracked and read by nothing, so `LF` never implies `CR`. My
`verify_lnm_does_not_make_lf_imply_cr` pins the actual behaviour (`ab`, `CSI 20 h`, `LF`
→ cursor `(1, 2)`, column kept). The change under review is precisely what makes this
honest: DECRQM now answers `Reset`, so a program asking whether LNM is on is told the
truth instead of being sold a capability that does nothing. Had DECRQM answered `Set`
while `LF` stayed inert, *that* would be the bug.

**Behavioural question, not a defect (note 8).** The new guard also blocks a cursor
parked *above* the region: with region = indices 2..6 and the cursor at index 1 col 0 on a
row whose predecessor is WRAPPED, `BS` now does nothing
(`verify_reverse_wrap_above_the_region_is_also_blocked`). The doc comment at
`screen.rs:664-666` states this on purpose ("A cursor parked above the region cannot
reverse-wrap at all"). It is a *widening* of the old behaviour's refusal set beyond what
`migration.md`'s cleanup row literally asks for ("confine reverse wrap to the scroll
region"). Worth a design-owner glance, since xterm with `DECOM` reset lets the cursor
address rows outside the margins; no recording exercises it (`grep-deviations` shows every
`? 45` occurrence in the 45 recordings is a *reset*, never a set), so nothing measurable
changed.

---

## 5. CI parity

`.github/workflows/ci.yml` vs `scripts/ci-local.{ps1,sh}`:

| | ci.yml | ci-local |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `workspace-quality` (Linux only) | step 1 |
| `cargo clippy --workspace --all-targets -- -D warnings` | Linux + Windows | step 2 |
| `cargo test --workspace` | Linux + Windows | step 3 |
| `cargo test -p oneterm-vt --features vt-paranoid` | Linux + Windows | step 4 |
| the six python checks | `dependency-graph` job | steps 5–10 |
| `cargo deny check licenses bans advisories` | its own job (always) | `--full` only |
| `vt-bench all --mib 25` | its own job, `continue-on-error` | **no counterpart** |
| `cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh` | `macos-tests` | subsumed by `cargo test --workspace` |

**Divergences, all benign:** (i) the six python checks run in a different *order*
(ci.yml: graph, catalog, doc-paths, notices, unittest, english; ci-local: graph,
doc-paths, unittest, english, catalog, notices) — same set, so no coverage gap; (ii)
ci-local has no `vt-bench` counterpart, which is correct because that job records and
never gates; (iii) `cargo deny` is unconditional in CI but `--full`-gated locally, which
both scripts document. Nothing in ci.yml gates on something ci-local cannot run.

**`--full` handling.** Both scripts parse clean (`Parser::ParseFile` no errors;
`bash -n` exit 0) and both accept `--full`: PowerShell binds `--full` to
`[switch]$Full` (verified with a minimal `[CmdletBinding()] param([switch]$Full)`
script — `--full → Full=True`, `-Full → Full=True`), and the bash twin's
`[[ "${1:-}" == "--full" ]]` sets `FULL=1`. No dangling `--full` branch in either.

`cargo deny check licenses bans advisories` (cargo-deny is installed here):

```
advisories ok, bans ok, licenses ok
exit 0
```

---

## 6. Notices

`python scripts/third-party-notices.py --check` — **green** (ci-local step 10, exit 0).

The generator and the generated file were edited in lockstep: commit `cbc6d7b` adds the
same § 2.1 block to `scripts/third-party-notices.py` (HEADER) and to
`THIRD-PARTY-NOTICES.md`, which is why `--check` passes.

**avt licence — correct and consistent.** avt is **Apache-2.0** (single, not dual). The
repository's own research records it: `docs/spec-intakes/IN-0029-vt-engine/research/
prior-art.md:1477` — `| avt | **Apache-2.0** (single, not dual) | **Yes** | Apache §4(b)
"state changes" applies |`, and `:1635` again. All four places agree:

| Where | Says |
| --- | --- |
| `NOTICE:22-26` | "follows the algorithm of the `Reflow` iterator in avt (…), Apache-2.0 … No avt source is copied" |
| `THIRD-PARTY-NOTICES.md` § 2.1 | table row: `crates/vt/src/reflow/columns.rs` \| avt's `Reflow` iterator \| Apache-2.0 \| "the approach only" |
| `docs/license-analysis.md:169-173` | "An algorithm is not source." — Apache-2.0, credit deliberate |
| `crates/vt/src/reflow/columns.rs:8-13` (source header) | "Derived from the algorithm of `avt`'s `Reflow` iterator (Apache-2.0 …). No `avt` source is copied" |

§ 2.1 also carries the `crates/pty/src/windows*.rs` row (Alacritty, Apache-2.0), which
matches the existing headers in those four files. The Apache § 4(b) statement ("notices
live in the source headers of the files listed above") is accurate — I checked
`crates/pty/src/windows.rs:3-4`, `windows/child.rs:10-13`, `windows/conpty.rs:4-5`,
`windows/pipe.rs:21-22`.

---

## 7. The full gate

`Get-PSDrive C,D` before any cargo run: **C: 16.6 GB free, D: 28.4 GB free** — above the
20 GB threshold on D:, so the build proceeded. (After the debug + release builds: D: 22 GB
free.)

`pwsh scripts/ci-local.ps1` from this worktree, cold `target/`:

```
ci-local: all checks passed.
EXITCODE=0
```

Ten steps, in this order, all green:

```
==> cargo fmt --all -- --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo test --workspace
==> cargo test -p oneterm-vt --features vt-paranoid
==> python scripts/verify-dependency-graph.py
==> python scripts/check-doc-paths.py
==> python -m unittest scripts/test_check_english.py
==> python scripts/check-english.py
==> python scripts/completion-catalog.py validate
==> python scripts/third-party-notices.py --check
```

Raw totals, summed from the 58 `test result:` lines:

```
sections   = 58   (cargo test --workspace 55, vt-paranoid 3)
passed     = 1893
failed     = 0
ignored    = 13
```

**Identical to the implementer's reported 58 / 1893 / 0 / 13.** Independently reproduced.

`cargo test -p oneterm-vt --features vt-paranoid` is step 4 of the same script (the
feature exists); it contributed 3 sections / 364 passed / 3 ignored.

---

## 8. Commit trailers

All five commits end with exactly the two required lines (last two non-blank lines of
each message):

```
f68b60e  [Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>] [Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9]
735bea2  [Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>] [Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9]
b4fae00  [Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>] [Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9]
30fde9a  [Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>] [Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9]
cbc6d7b  [Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>] [Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9]
```

**PASS.**

---

## 9. The thirteen stale lines left for the design owner

Every one was opened and read at the stated path and line. **All thirteen rows are
accurate — none is fabricated and none has drifted.** Verbatim, for routing:

| # | File:line | The line, verbatim |
| --- | --- | --- |
| 1 | `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md:161` | ``- [ ] `US-0087` — **Decommission the fork.** Delete `vendor/`, `vendor/refresh.sh`, the CI job, the`` |
| 2 | `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md:71` | ``| P31 | **Every capability costs a fork patch**: 823 patch lines across five patches, a `refresh.sh --check` CI job, and a rebase on every upstream move | `vendor/patches/`, `.github/workflows/ci.yml:65-68` | A first-party crate under normal review; extension points are API — `DEC-0014`, [`migration.md`](low-level-design/migration.md) § "Deletion list" | `US-0087` | `python scripts/third-party-notices.py --check`; `test -d vendor` fails |`` |
| 3 | `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md:123` | ``| `vte` | 0.15, dev-only | `oneterm-vt` | the differential oracle, retired at `US-0087` |`` |
| 4 | `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md:511` | ``| 16 | `US-0087` | Decommission | `vendor/` absent; no `refresh.sh` CI job; no `[patch]`; the `vte` dev-oracle and `vt-diff` deleted; `python scripts/third-party-notices.py --check`, `check-doc-paths.py` and `pwsh scripts/ci-local.ps1` green; every owning doc reconciled |`` |
| 5 | `.../low-level-design/migration.md:394` (deletion list, 394-417) | ``| `vt-diff` and every `--engine old` path | `crates/tools` | `US-0087` |`` |
| 5b | `.../low-level-design/migration.md:417` | ``| The `vte` dev-dependency and the differential oracle | `crates/vt/Cargo.toml`, `crates/vt/tests/differential.rs` |`` |
| 6 | `.../low-level-design/migration.md:437` (cleanup rows, 437-449) | ``### Cleanup before decommission (`US-0087`)`` |
| 6b | `.../low-level-design/migration.md:449` | `set. The reference applies the mode to `BS` only, and so does this engine.` |
| 7 | `.../low-level-design/migration.md:545` | ``- [ ] `US-0087`: the two cleanup rows above are done, each with its test; `test -d vendor` fails; `grep -rn "alacritty_terminal\|vendor/" Cargo.toml .github/workflows/ci.yml scripts/``` |
| 8 | `.../low-level-design/testing-and-bench.md:152` (152-163) | `**Who blesses (R-58).** `US-0072` generates both files **with the old engine**, they are reviewed` |
| 8b | `.../low-level-design/testing-and-bench.md:163` | ``After `US-0072` the cross-check is never run again.`` |
| 9 | `.../low-level-design/testing-and-bench.md:195` | ``**Old engine versus new** (`crates/tools/src/bin/vt-diff.rs`, alive from `US-0072` to `US-0087`).`` |
| 9b | `.../low-level-design/testing-and-bench.md:402` | ``- [ ] `US-0087`: the fork and the old-engine paths are deleted, and with them `vt-diff`, the`` |
| 10 | `.../low-level-design/parser.md:336` | ``US-0073`-`US-0087` in which the oracle is the primary proof.`` |
| 10b | `.../low-level-design/parser.md:351` | `silently weakening it. The oracle and the dev-dependency retire with the fork at `US-0087`.` |
| 11 | `.../low-level-design/reflow-and-resize.md:104` | ``against `Cargo.lock`, a hand-written section needs the generator's owner: **`US-0087` owns the`` |
| 12 | `docs/spec-intakes/IN-0029-vt-engine/US-0077-reflow-and-resize.md:465` | ``` `Cargo.lock`, so a hand-written section needs the generator's owner. **Assigned to `US-0087`** ``` |
| 13 | `docs/spec-intakes/IN-0029-vt-engine/US-0085-terminal-view-native.md:312` | ``**To `US-0087`.** You inherit `crates/terminal/tests/us0081_parity.rs` — which now`` |

One of these is a real decision rather than a tick, and the packet flags it correctly:
`migration.md`'s deletion list (rows 394-417) does **not** name `vt-corpus bless` or the
US-0072 `cross-check`, yet both were deleted. The reasoning (R-58: with the fork gone no
engine may bless, so keeping the subcommand keeps a writer with nothing behind it) is
sound and I agree with it — but it is a scope extension beyond the design's written
deletion list and the design owner should ratify it. See also defect 5 below: the writer
itself is still in the tree.

---

## Defects

### 1 — minor — stale type name in a file the doc sweep did not visit

`crates/local-shell/src/transport.rs:16`

```rust
/// Alacritty `EventListener` for the local shell (shared router + PTY transport).
pub(crate) type LocalListener = OscRouter<LocalTransport>;
```

`EventListener` was `alacritty_terminal`'s trait. It no longer exists anywhere:
`grep -rn "EventListener" crates/ --include=*.rs` returns exactly this line plus one
historical mention at `crates/terminal/src/backend/osc_router.rs:11`. The comment now
names a type a reader cannot find, on a crate the packet's provenance sweep never covered
(`crates/{app,core,pty,terminal,vt}` only — `crates/local-shell` is missing from the
Reconciliation list). It is not in any out-of-scope clause.

*Repro:* `grep -rn "EventListener" crates/ --include=*.rs`
*Fix:* one line — "The OSC/PTY listener for the local shell (shared router + PTY
transport)."

### 2 — minor — a current-state design doc still names the fork as the engine

`docs/sftp-browser-design.md:98`

```
│   ├── local/                     # Local shell (alacritty_terminal + ConPTY)
```

Not covered by the Scope's out-of-scope list (which names `docs/osc-*.md`, `docs/archive`,
`docs/decisions` and the LLDs, not this file), not in the Reconciliation table, and
contradicts the packet's own stated reason for the documentation action: "A doc that still
says the terminal engine is a patched fork is wrong about what OneTerm ships."

*Repro:* `grep -rn "alacritty_terminal" docs/*.md`
*Fix:* `# Local shell (oneterm-vt + ConPTY)`.

### 3 — minor — Acceptance checkboxes left unticked while Status is Implemented

`docs/spec-intakes/IN-0029-vt-engine/US-0087-decommission.md:71-89` — all **nine**
Acceptance boxes are `- [ ]`. Sibling packets in the same intake tick theirs:
`US-0085` is 8 ticked / 1 open, `US-0086` is 10 ticked / 0 open. A reviewer cannot tell
from the packet which criteria the implementer considers met; the Evidence section carries
the proof but the checklist does not reflect it, and the packet's own pre-code gate note
says "keep authored checklists current".

*Repro:*
`awk '/^## Acceptance/{s=1;next} /^## /{s=0} s' <packet> | grep -c "^- \[x\]"` → `0`.

### 4 — minor — 46 frozen expectation files instruct the reader to run a deleted command

Every `grid.expect` under `crates/vt/tests/corpus/` carries:

```
# Do not hand-edit: regenerate with `vt-corpus bless --engine old --deviation <id>`.
```

`vt-corpus bless` was deleted by this packet, and `--engine old` now refuses. Meanwhile
`crates/tools/src/corpus.rs:207` — the *encoder* for those same files — was updated to the
new wording ("the engine that blessed these files was deleted at US-0087"). So the writer
and the 46 written files now disagree about their own header. The packet's position (the
frozen files are data and not editable) is defensible, but it leaves 46 files pointing a
future maintainer at a subcommand that no longer exists, and the encoder change proves the
sentence was considered editable in principle.

*Repro:* `grep -rl "vt-corpus bless" crates/vt/tests/corpus/ | wc -l` → `46`;
`grep -rh "Do not hand-edit" crates/vt/tests/corpus/ | sort -u` → one distinct line, the
old one. Harmless to the gate (comment lines are skipped by `decode`; `corpus_check` is
green).

### 5 — minor — the bless *writer* survived the bless *subcommand*

`crates/tools/src/corpus.rs:201` (`GridExpect::encode`) and `:358`
(`StateExpect::encode`) are the functions that produced the frozen files. After the
deletion:

- `GridExpect::encode` has exactly two callers, both in a round-trip unit test
  (`crates/tools/src/corpus_tests.rs:68,86`).
- `StateExpect::encode` has **no caller at all** — it survives only because it is `pub`
  in a library crate, so `dead_code` never fires.

The packet's stated reason for deleting `bless` was that keeping it "would have meant
keeping a writer with nothing behind it". The writer is still there.

*Repro:* `grep -rn "\.encode()" crates/tools/` → two hits, both in `corpus_tests.rs`.
*Not a correctness problem* — just the opposite of what the commit message claims.

### 6 — minor — `deny.toml`'s `allow-git` list is now entirely dead

`deny.toml:124-127` still allows `https://github.com/zed-industries/zed` while
`cargo metadata` resolves **zero** `git+` sources and `Cargo.lock` contains no `git+` line.
The packet removed the fork's entry from this exact block and left a second entry that
allows nothing. Pre-existing rather than introduced here, but this is the packet that
touched the block.

*Repro:* `grep -n "git+" Cargo.lock` → empty; `deny.toml` `allow-git` → one entry.
*Fix:* drop the list (or keep it with a comment saying it is a deliberate guard for a
future GPUI git pin).

### 7 — minor — `AGENTS.md` § 4 claims the local script list, and omits one of its steps

`AGENTS.md:104-115` lists nine commands as "The script runs, in order". Both `ci-local`
twins run **ten** — `cargo test -p oneterm-vt --features vt-paranoid` sits between
`cargo test --workspace` and the python checks and is not listed. AGENTS.md § 4 is exactly
the place that asserts CI/local parity, so the omission matters more there than elsewhere.
Pre-existing (`git show 65139c5:AGENTS.md` has no `vt-paranoid` either; the step arrived in
`8f60fc9`), but this packet rewrote the paragraph immediately below the list.

*Repro:* `grep -c "vt-paranoid" AGENTS.md` → `0`; `grep -n "vt-paranoid" scripts/ci-local.ps1`
→ present.

### 8 — minor / design question — reverse wrap is now refused *above* the region too

`crates/vt/src/grid/screen.rs:667` (`let floor = self.row_of_index(self.region.top);`)
together with the unchanged `<=` makes `BS` a no-op for any cursor at or above the region
top, not just at it. The doc comment claims this deliberately, and no corpus recording
sets `? 45`, so nothing measurable changed — but `migration.md`'s cleanup row asks only to
"confine reverse wrap to the scroll region", and refusing to wrap *outside* the region is
a second behaviour. Flagging for the design owner rather than as a defect.

*Repro:* `crates/vt/tests/verify_us0087.rs::verify_reverse_wrap_above_the_region_is_also_blocked`
(region `CSI 3;6 r`, cursor at index 1, `BS` → cursor unmoved).

### Informational — packet Handoff says "Four commits"

`US-0087-decommission.md` Handoff: "Four commits: the packet (pre-code), the deletions,
the two cleanup rows, the documentation." There are **five** (`cbc6d7b`, which added the
avt attribution and this Evidence, is the fifth and wrote the sentence). It also names
branch `worktree-agent-aec9c63d7aab09c7e`. Cosmetic.

---

## What I could not verify

- The "before and after" build-time and tree-size numbers (198.4 s → 168.8 s, 30.8 →
  29.9 MiB) — a cold rebuild on a different machine proves nothing and I did not spend the
  wall clock on it. The direction is obviously right (27 k lines and two dependencies
  removed).
- `ext_memory_stays_bounded` in `crates/vt/tests/parser_limits.rs` — still `#[ignore]`d,
  as the packet's Gap 3 states. It needs `--test-threads=1` on a quiet machine and this
  machine was building; the packet declares this openly rather than claiming it passed.
- No GUI walk, by instruction and by the owner's standing rule.
