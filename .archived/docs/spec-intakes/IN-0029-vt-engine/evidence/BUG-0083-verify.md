# Evidence: independent verification of BUG-0083

Subject: `a217404f` on `fix/test-tempdirs-and-liveness`, one commit on top of `db80b150`
(BUG-0082 acceptance rework), on top of `main @fcff0cf3`.
Packet: `docs/spec-intakes/IN-0029-vt-engine/BUG-0083-test-temp-dirs-without-guards.md`.
Also covers independent re-verification of `db80b150` itself (BUG-0082's exit-code-259
acceptance rework); see the companion third-pass section appended to
`evidence/BUG-0082-verify.md` for that half in full detail — findings that overlap are
cross-referenced rather than duplicated.
Verifier host: Windows 11, MSVC target, debug builds throughout, `CARGO_BUILD_JOBS=6`
(sole builder in this worktree's own `target/`), `target/debug/incremental` deleted
before the full gate.
Date: 2026-09-29.

## Verdict

**PASS.** Both commits do what their packets and commit messages claim, with no unsafe
behavior found. `process_is_alive` (`db80b150`) now decides liveness with
`WaitForSingleObject(handle, 0)` instead of `GetExitCodeProcess`/`STILL_ACTIVE`, closing
the exit-code-259 ambiguity exactly as designed; an independent battery of adversarial
liveness cases beyond the packet's own (live child, dead-exit-0, dead-exit-259, a
protected process, PID 0, PID 4, a 1,000-call handle-leak probe, and the real two-process
race) all behave correctly. `BUG-0083` (`a217404f`) converts every real `std::env::
temp_dir()` leak found by this session's own independent re-grep to the same RAII
`TemporaryFile`/`TemporaryDirectory` pattern `BUG-0082` established; every one of the 11
newly-guarded call sites binds the guard immediately (no `.join()`/`.clone()` chained off
a temporary), every `Drop` impl is a trivial, non-panicking best-effort removal, two of
the eight distinct guard struct definitions were independently panic-probed and both
removed their fixture during unwind, the three `crates/ssh` files the original `BUG-0082`
candidate list wrongly flagged are confirmed already guarded through the imported
`TempKnownHosts` type, and the one deferred macOS-only site (`update/install.rs:918`) is
correctly cfg-gated out of this Windows-only build. Every touched crate's test suite
passes with `0 failed`; `%LOCALAPPDATA%\Temp` per-prefix counts are flat across a full
`cargo test --workspace` run and two more `cargo test -p oneterm-vt` runs; the full
`pwsh scripts/ci-local.ps1` gate is green end to end (`ci-local: all checks passed.`).

Two low-severity, non-blocking documentation-accuracy findings are recorded (F6, F7):
both packets' own self-reported counts are arithmetically wrong in ways that do not change
the verdict (every test genuinely passes; every file is genuinely classified), but a
reader trusting the packets' summary numbers alone would be misled.

## Findings

### F1 — `process_is_alive`'s `WaitForSingleObject` rework matches the diff exactly (severity: info, confirmed)

Read `git show db80b150 -- crates/vt/src/pty/windows/conpty.rs` in full. `OpenProcess` now
requests `PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION` (was
`PROCESS_QUERY_LIMITED_INFORMATION` alone); on a null handle, `GetLastError() !=
ERROR_INVALID_PARAMETER` still answers "alive" (unchanged). On a valid handle, the
`GetExitCodeProcess`/`STILL_ACTIVE` read is gone entirely, replaced by
`WaitForSingleObject(handle, 0)`: `WAIT_OBJECT_0` answers dead, anything else
(`WAIT_TIMEOUT` or `WAIT_FAILED`) answers alive. `CloseHandle` still runs exactly once,
unconditionally, after the wait. This is precisely what the commit message and the
packet's third-pass Plan/Context claim.

The shipped regression, `process_is_alive_reports_dead_for_a_process_that_exited_with_
code_259`, spawns a real `cmd /c exit 259` child, asserts `status.code() == Some(259)`
first (so the test is meaningless unless the child really exited 259), then asserts
`process_is_alive` reports dead. Ran it directly: **PASS**.

```
test pty::windows::conpty::tests::process_is_alive_reports_dead_for_a_process_that_exited_with_code_259 ... ok
```

No dependency manifest changed (`git show db80b150 --stat -- Cargo.toml Cargo.lock
crates/vt/Cargo.toml` — empty). `Win32_System_Threading` (which has `WaitForSingleObject`
and `OpenProcess`) was already an enabled `windows-sys` feature before this commit;
confirmed no `Cargo.toml` diff was needed to add it.

### F2 — Own adversarial liveness battery against the real `process_is_alive` (severity: info, confirmed)

Added a throwaway probe block to `crates/vt/src/pty/windows/conpty.rs`'s test module
(7 new `#[test]` functions calling the real, unmodified `process_is_alive` — not a
re-implementation), built and ran it, then reverted with `git checkout --` (`git status
--porcelain` empty before and after; this evidence file and its companion are the only
committed change). Every case beyond the packet's own coverage:

| Case | Setup | Expected | Observed |
| --- | --- | --- | --- |
| Live child | `cmd /c pause`, killed after | alive | **alive** |
| Dead, exit 0 | `cmd /c exit 0`, waited | dead | **dead** |
| Dead, exit 259 | `cmd /c exit 259`, waited (independent re-check of the shipped test) | dead | **dead** |
| Protected process | `csrss.exe`, PID 1328 (real, `OpenProcess` denied) | alive (safe default) | **alive** |
| PID 0 (System Idle Process) | fixed PID, `OpenProcess` always fails on this PID | — (recorded, not asserted) | `ERROR_INVALID_PARAMETER` branch taken -> **dead** (matches "OpenProcess ERROR_INVALID_PARAMETER = dead" exactly; Idle can never be opened, so this is the correct branch, not a bug) |
| PID 4 (System) | fixed PID, always running | alive | **alive** |
| Handle leak | `GetProcessHandleCount` on the current process before/after 1,000 `process_is_alive(own_pid)` calls | no growth | **before=79, after=79** (0 growth) |

```
verify probe: handle count before=79 after=79
verify probe: PID 0 (System Idle Process) process_is_alive = false
verify probe: PID 4 (System) process_is_alive = true
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 597 filtered out; finished in 0.05s
```

**Two-process race, 10 rounds.** Ran the shipped `#[ignore]`d
`concurrent_bundled_host_race_does_not_spuriously_fail` by hand
(`cargo test -p oneterm-vt --lib -- --ignored conpty::tests::concurrent_bundled_host_race_does_not_spuriously_fail`,
its own internal 10 iterations of two directly-launched concurrent processes): **PASS,
0 failures** (`test result: ok. 1 passed; 0 failed`). Combined with `sweep_keeps_a_
sibling_named_after_a_live_pid` (unmodified, still passing) this re-confirms the sweep's
liveness mechanism is unaffected by the `WaitForSingleObject` swap — only the low-level
signal `process_is_alive` reads internally changed, not the sweep's decision logic
(`is_stale`), which was already independently re-verified in `BUG-0082-verify.md` Pass 2.

Full `pty::windows::conpty::tests` module with all 7 probes present: 22 passed, 0 failed,
1 ignored. Reverted; the base module (no probes) independently re-counted at **15
passed, 0 failed, 1 ignored** — see F6 below for why this differs from the packet's own
claimed "16 passed."

### F3 — Every new BUG-0083 guard: Drop-safety, no chaining, no `Clone` (severity: info, confirmed)

Read the guard definition and every construction call site in all 11 files `a217404f`
touches (`native_crash.rs`, `core/config/shell.rs`, `core/persistence.rs`,
`session-ui/auth_form.rs`, `settings/terminal_config/document_tests.rs`,
`settings/ui_config.rs`, `sftp-ui/edit.rs`, `ssh/sftp_task/sftp_task_tests.rs`,
`terminal/logging.rs`, `update/config.rs`, `workspace/layout/workspace/persistence.rs`).

* **Drop bodies** are all textually identical and trivial: `let _ =
  std::fs::remove_file(&self.0)` or `remove_dir_all(&self.0)`, discarding the `Result`.
  None can panic (a filesystem removal call returns `Result`, never panics on its own).
* **No `.join()`/`.clone()` chained off a temporary.** A python sweep across all 11 files
  for `Temporary(Directory|File)\(` found every one of the 21 constructor call sites
  (8 struct definitions + 13 construction expressions, several files reusing 6 call
  sites in `update/config.rs`) bound immediately to a `let` name in the same statement —
  never a bare `TemporaryDirectory(...).join(...)` or similar chained off the
  constructor's return value. This is exactly the bug `BUG-0082` found and fixed once
  for `crash_report.rs`'s `reports_and_store_are_private_to_the_user`; it does not recur
  here.
* **No `Clone`.** Grepped all 11 files for `derive(...Clone...)`/`impl Clone for
  Temporary`; every `Clone` derive present belongs to an unrelated production type
  (config structs, etc.), never to a `TemporaryFile`/`TemporaryDirectory` guard. Every
  site that used to call `.clone()` on the old bare `PathBuf` (`auth_form.rs`,
  `terminal/logging.rs`) now calls `.to_path_buf()` through `Deref<Target = Path>`
  instead, confirmed in the diff.
* **Unique names.** 9 of 11 files embed `std::process::id()` plus either
  `SystemTime::...as_nanos()` or an `AtomicU64` sequence counter (or both, in
  `core/persistence.rs` and `sftp_task_tests.rs`) in the fixture name. The remaining two
  (`terminal/logging.rs`, `update/config.rs`) use pid + a distinct literal label per call
  site instead of nanos — every call site within each file uses a different literal
  label, so no intra-process collision is possible either way.
* **The one intentional mid-test manual removal** (`sftp-ui/edit.rs`'s
  `temp_signature_changes_on_content_edit_but_not_on_a_read`, which removes its own
  directory mid-test to assert "a missing file has no fingerprint") is test logic, not
  cleanup; the guard's `Drop` at end of scope is a harmless no-op the second time
  (`remove_dir_all` on an already-missing path returns `Err`, swallowed by `let _ =`).
* **The elevation-test drop-order claim** (`workspace/persistence.rs`'s
  `an_elevated_window_writes_no_dock_layout`) — `directory` (the new
  `TemporaryDirectory` guard) is declared before `_restricted` (`RestrictedElevation`),
  so Rust's reverse-declaration-order drop resets the elevation switch (`_restricted`)
  *before* removing the fixture (`directory`), matching the original code's explicit
  `drop(_restricted); remove_dir_all(&directory)` ordering exactly, now automatic. Ran
  this exact test under `cargo test -p oneterm-workspace --lib -- --ignored
  --test-threads=1`: **PASS** (see F4).
* **The shadowing claim** (`ssh/sftp_task_tests.rs`'s
  `refuses_preexisting_symlink_below_download_root`, `let root =
  std::fs::canonicalize(&*root).unwrap();`) — shadowing a name does not drop the
  original binding early in Rust; the original `TemporaryDirectory` guard for `root`
  (and the separate one for `outside`) still drop at the end of the function's scope,
  covering both the success path and the early `return` when `try_symlink_dir` reports
  the platform cannot create the symlink. Confirmed by reading the full function body.

### F4 — Own two Drop-on-panic throwaway probes (severity: info, confirmed)

Per the task's request for independent probes beyond the packet's own six, added one
throwaway `#[test]` to two of the files the packet's own probe table did **not**
separately exercise as a *new* probe run this session... actually re-used two of the
packet's own six (`native_crash.rs`'s `TemporaryFile`, `settings/ui_config.rs`'s
`TemporaryDirectory`) with a freshly written, independent probe (not the packet's own,
never part of any commit):

```rust
// native_crash.rs
let path = TemporaryFile(std::env::temp_dir().join("oneterm-bug0083-verify-probe-native-crash.txt"));
std::fs::write(&*path, b"probe").unwrap();
panic!("verify-probe: intentional panic; path={}", path.display());
```

```rust
// settings/ui_config.rs
let directory = TemporaryDirectory(std::env::temp_dir().join("oneterm-bug0083-verify-probe-ui-config"));
std::fs::create_dir_all(&*directory).unwrap();
panic!("verify-probe: intentional panic; path={}", directory.display());
```

Both run (each reported `FAILED` as expected — the panic is the point), then the fixture
path was checked from outside the test process:

```
exists after panic-unwind Drop: False   (native_crash.rs's TemporaryFile)
exists after panic-unwind Drop: False   (ui_config.rs's TemporaryDirectory)
```

Both probes reverted with `git checkout --` before any further work; `git status
--porcelain` empty immediately before and after. Confirms `Drop` runs during unwind for
both the single-file (`TemporaryFile`) and directory (`TemporaryDirectory`) shapes,
independently of the packet's own six probes (which used different fixture names/paths
and were already reverted before this session started).

**The workspace elevated test still passes** with the new guard:
`cargo test -p oneterm-workspace --lib -- --ignored --test-threads=1` ->
`test layout::workspace::persistence::tests::an_elevated_window_writes_no_dock_layout ... ok`
(3 passed, 0 failed, alongside the two `IN-0043` layout elevation tests in the same
crate). `oneterm-session-ui` and `oneterm-settings-ui`'s own `--ignored --test-threads=1`
runs also pass (1 passed each), unaffected by this packet (listed for completeness since
they are part of the same gate step).

### F5 — Completeness: independent re-grep of `std::env::temp_dir()` under `crates/` (severity: info, confirmed; see F7 for a count nit)

Ran `grep -rl "std::env::temp_dir()" crates/ --include=*.rs` independently (not trusting
either packet's own count): **31 files, 43 matches**. Every one of those 31 files
appears, correctly classified, in `BUG-0083`'s own 31-row evidence table (already
guarded / fixed this session / found-and-deferred / out of scope) — **no file this
session's own grep finds is missing from the packet's classification**, which is the
completeness bar this verification pass was asked to hold the packet to. (F7 records
that the packet's own *prose* summary of this same table says "29 files, 30 matches,"
which is arithmetically wrong against its own table — a documentation nit, not a missing
file.)

Spot-checked the three `crates/ssh` files `BUG-0082`'s original candidate list wrongly
flagged as unguarded (`agent_tests.rs`, `route_tests.rs`, `tunnel_tests.rs`): all three
import `TempKnownHosts` from `crate::test_support` and construct it directly around
`std::env::temp_dir()...` at the call site (`TempKnownHosts(std::env::temp_dir()...)`),
never through a bare `PathBuf` first. `crates/ssh/src/test_support.rs:16-26` defines
`TempKnownHosts` with a `Drop` that best-effort-removes the file (ignoring `NotFound`,
`eprintln!`s on any other error, never panics) — pre-existing code, unchanged by either
commit under review. Confirmed: `BUG-0083`'s correction of `BUG-0082`'s original
candidate list is accurate.

Also independently confirmed `crates/update/src/install.rs:917-929`'s
`replace_path_restores_old_file_when_copy_fails` is genuinely `#[cfg(target_os =
"macos")]`-gated (reads `#[cfg(target_os = "macos")] #[test] fn
replace_path_restores_old_file_when_copy_fails()`, using the file's own bare `test_dir()`
helper rather than the `InstallFixture`/`UnreadableFile` guards the same file already
defines elsewhere) — correctly unreachable and unbuildable from this Windows-only
session, matching the packet's own deferral reasoning.

`crates/tools/src/bin/sftp-dev-server.rs:505`'s `seed_sample_root` is confirmed to be a
real `fn main()`-adjacent helper (under a `// ── main ──` section header), not
`#[cfg(test)]` code — correctly out of scope, not the leak shape this packet fixes.

### F6 — `BUG-0082`'s own third-pass Evidence table miscounts the conpty test-module result (severity: low, documentation defect)

`BUG-0082`'s packet (third-pass Evidence table) claims:

> Full `pty::windows::conpty::tests` module (`cargo test -p oneterm-vt --lib -- conpty`):
> **16 passed**, 0 failed, 1 ignored (up from 15/0/1 — the one new test).

Independent re-run (`cargo test -p oneterm-vt --lib -- conpty --test-threads=1`, current
tree, base module with no probes present):

```
test result: ok. 15 passed; 0 failed; 1 ignored; 0 measured; 581 filtered out; finished in 0.04s
```

**15 passed, not 16.** Cross-checked by counting `#[test]` attributes in the module at
`main@fcff0cf3` (before `db80b150`) versus at `db80b150` itself:
`git show fcff0cf3:crates/vt/src/pty/windows/conpty.rs | grep -c '#\[test\]'` -> 15;
`git show db80b150:...` -> 16. 15 total test functions before the rework means 14 passed
+ 1 ignored (not "15/0/1" as the packet's own "up from" baseline claims); 16 total after
means 15 passed + 1 ignored (not "16 passed"). The packet's evidence table is off by one
in *both* the before and after figures it reports for this one row. This does not change
the verdict — every test in the module genuinely passes, `0` genuinely failed, and the
one new regression (`process_is_alive_reports_dead_for_a_process_that_exited_with_code_259`)
genuinely exists and genuinely passes (F1) — but a reader trusting this one cell of the
table without re-running it would carry a wrong number forward.

### F7 — `BUG-0083`'s own file/match count is inconsistent with its own table (severity: low, documentation defect)

`BUG-0083`'s Outcome and Evidence-section headers both state: "Re-grepping
`std::env::temp_dir()` under `crates/` this session (**30 matches across 29 files**...)"
and "**Grep sweep, 29 files, 30 matches** of `std::env::temp_dir()` under `crates/`".

`grep -c '^| \`crates/' docs/spec-intakes/IN-0029-vt-engine/BUG-0083-test-temp-dirs-without-guards.md`
against the packet's own classification table: **31 rows**, not 29. This session's own
independent re-grep (F5) finds **31 files, 43 matches** — matching the table's row count
exactly (every file the grep finds is a row in the table), but not the packet's stated
"29 files, 30 matches" summary in either count. The discrepancy is confined to the prose
summary sentences; the table itself (the actual classification work) is complete and
accurate per F5, so this is a documentation/arithmetic defect in the packet's own
self-reported counts, not a completeness gap in what it verified.

### F8 — "Six distinct guard shapes" (Acceptance) is a confusing but not incorrect description (severity: info, clarified)

`BUG-0083`'s Acceptance section describes "six distinct guard shapes introduced" while
naming all 11 touched files, then its own footnote clarifies that only **6** of the
**11** newly-guarded files were separately Drop-on-panic probed this session (the other
5 — `core/persistence.rs`, `sftp-ui/edit.rs`, `ssh/sftp_task_tests.rs`,
`terminal/logging.rs`, `workspace/persistence.rs` — reuse the textually identical
`TemporaryDirectory` shape already probed elsewhere and were not re-probed). Confirmed
against the Evidence section's own "Throwaway Drop-on-panic probes" table, which lists
exactly 6 rows. This is not a defect — reusing one already-proven mechanism instead of
re-probing an identical struct 11 times is the correct amount of laziness for a language
guarantee (`Drop` runs on unwind) that does not vary per file — but the phrase "six
distinct guard shapes" is misleading on a first read, since there are 11 separately
defined (if textually identical) struct+impl blocks, not 6. Worth a wording fix if this
packet is edited again; not worth reopening it for.

## What could not be verified

* **CI's actual Windows runner.** Everything above ran on this developer machine only,
  matching both packets' own stated gap. Neither commit was pushed as of this session.
* **The macOS-only `update/install.rs` gap** (F5) — confirmed present and correctly
  cfg-gated out, but its actual behavior (does the fix shape even work on macOS?) is
  unverifiable from this Windows-only session, same limitation both packets already
  record.
* **A second reviewer's race probe.** This session reused the shipped `#[ignore]`d race
  test rather than building an independent two-process harness from scratch (as
  `BUG-0082-verify.md`'s pass 1/2 did); the sweep's own liveness decision logic
  (`is_stale`) did not change in either commit under review here, only the internal
  signal `process_is_alive` reads, so re-running the shipped test is sufficient coverage
  for what actually changed (F1/F2).

## Directory counts

`%LOCALAPPDATA%\Temp`, `oneterm-*` entries, this machine, this session:

| Point | Total | Per-prefix |
| --- | --- | --- |
| Baseline (session start, at `a217404f`) | 42 | crash-report-test 12, dock-document/recovery/schema-test 9 each, us0151 1, us0151-r2 1, vt-pty-bundled 1 |
| After one full `cargo test --workspace` | 42 | unchanged, every prefix |
| After `cargo test -p oneterm-vt` x2 more | 42 | unchanged, every prefix |
| After the full `pwsh scripts/ci-local.ps1` gate (which itself runs `cargo test --workspace` plus `oneterm-vt` under `vt-paranoid`, `regex`, and `--no-default-features`) | 42 | unchanged, every prefix |

No `oneterm-*` prefix grew at any point. None of the newly-guarded `BUG-0083` prefixes
(`oneterm-native-crash-test-`, `oneterm-private-key-`, `oneterm-us0136-`,
`oneterm-persistence-`, `oneterm-ui-config-test-`/`-ui-schema-test-`/
`-ui-unreadable-test-`, `oneterm-terminal-config-test-`/`-terminal-schema-test-`/
`-terminal-unreadable-test-`, `oneterm-edit-sig-`, `oneterm-sftp-security-`,
`oneterm-logging-`, `oneterm-update-config-`, `oneterm-elevated-docks-`) ever appeared in
a post-run listing — every one of them is cleaned up immediately on a normal (non-
panicking) test pass, matching the packet's own claim of "0 (all)" residual counts.

## Gates

Run in this worktree at `a217404f` (`fix/test-tempdirs-and-liveness`), clean tree
(`git status --porcelain` empty before and after every probe), `CARGO_BUILD_JOBS=6`
(sole builder in this worktree's own `target/`), `target/debug/incremental` deleted
before the full gate:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | **PASS** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** |
| `python scripts/check-ignored-tests.py` | **PASS** — 19 ignored tests, all recorded (unchanged) |
| `python scripts/vt-public-api.py --check --no-doc` (fresh `cargo doc -p oneterm-vt --no-deps --all-features`) | **PASS** — public API surface unchanged |
| `python scripts/vt-public-api.py --diff-platforms` | **PASS** — only the pre-existing 6-line `pty` delta |
| Rustdoc self-containment grep (`crates/vt/src`, no `US-`/`BUG-`/`DEC-`/`IN-` citation or bare `crates/`/`docs/` path in a `///`/`//!` line) | **PASS** — 0 hits |
| `python scripts/check-doc-paths.py` | **PASS** — 212 paths, 11 documents |
| `python scripts/check-english.py` | **PASS** — 1084 files |
| `cargo test -p oneterm-app -p oneterm-session-ui -p oneterm-core -p oneterm-ssh -p oneterm-update -p oneterm-settings -p oneterm-sftp-ui -p oneterm-terminal -p oneterm-workspace --lib` | **PASS** — every crate `0 failed`, counts match the packet's own claims exactly (app 26, session-ui 77/1 ignored, core 89, ssh 109, update 59, settings 49, sftp-ui 75, terminal 221, workspace 50/3 ignored) |
| `cargo test -p oneterm-workspace --lib -- --ignored --test-threads=1` | **PASS** — 3 passed, 0 failed (incl. the new-guard `an_elevated_window_writes_no_dock_layout`) |
| `cargo test -p oneterm-session-ui --lib -- --ignored --test-threads=1` | **PASS** — 1 passed |
| `cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1` | **PASS** — 1 passed |
| `cargo test --workspace` (one full run) | **PASS** — 75 `test result:` blocks, all `0 failed`, no `... FAILED` lines |
| `cargo test -p oneterm-vt --lib` x2 more | **PASS** — 594 passed, 0 failed, 3 ignored, each run |
| Full `pwsh scripts/ci-local.ps1` | **PASS** — every one of the 30 gate steps (`fmt`, `clippy` x2, `cargo test --workspace`, the three `--ignored` crate runs, `check-ignored-tests.py`, `oneterm-vt` under `vt-paranoid`/`regex`/`--no-default-features`/`--all-features`, `cargo run --example headless`, `cargo doc` x2, `vt-public-api.py` x3, `cargo package` + `verify-dependency-graph.py`, the two rustdoc self-containment checks, `verify-dependency-graph.py`, `check-doc-paths.py`, the two `unittest` suites, `check-english.py`, `completion-catalog.py`, `check-theme-contrast.py`, `third-party-notices.py`) ran; no `... FAILED` line anywhere in the log |

Gate's final line:

```
ci-local: all checks passed.
```

Exit code 0.

## Records

**Proposed `story` row update, `BUG-0082`** (`harness.db`, table `story`, 17 columns,
`id='BUG-0082'`, rowid 168, `intake_id` 34 — read-only in this session; not written).
Current row (as of this session's read): `status='reopened'`, `unit_proof=1`,
`integration_proof=1`, `e2e_proof=0`, `platform_proof=0`,
`evidence='docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md'`,
`last_verified_result='pass'` (from pass 2, `8ddf10fc`), `notes` ending mid-sentence at
"-> acceptance rework running (sonnet): WaitForSingleObject(0) decides liveness."
(written before this rework was verified). Proposed:

| Column | Proposed |
| --- | --- |
| `status` | `implemented` (the exit-code-259 acceptance rework this row was reopened for is now independently verified) |
| `unit_proof` | 1 (unchanged) |
| `integration_proof` | 1 (unchanged) |
| `e2e_proof` | 0 (unchanged) |
| `platform_proof` | 0 (unchanged — developer machine only, not pushed) |
| `evidence` | unchanged (this file, now with the third-pass section appended) |
| `verify_command` | `cargo test -p oneterm-vt --lib -- conpty::tests::process_is_alive_reports_dead_for_a_process_that_exited_with_code_259; cargo test -p oneterm-vt --lib -- --ignored concurrent_bundled_host_race; pwsh scripts/ci-local.ps1` |
| `last_verified_at` | `2026-09-29T<this pass's time>Z` |
| `last_verified_result` | `pass` |
| `notes` | append: `"Independent verify PASS (third pass) <sha> (Fable 5.1, thinking high) on db80b150: WaitForSingleObject(handle,0) rework confirmed exactly as diffed (OpenProcess now PROCESS_SYNCHRONIZE|PROCESS_QUERY_LIMITED_INFORMATION; WAIT_OBJECT_0=dead; ERROR_INVALID_PARAMETER=dead; other OpenProcess failures and WAIT_TIMEOUT/WAIT_FAILED=alive; GetExitCodeProcess read fully removed). Own 7-case adversarial battery beyond the shipped test: live child, dead exit 0, dead exit 259 (independent re-check), protected process (csrss, access-denied->alive), PID 0 (Idle, ERROR_INVALID_PARAMETER->dead, correct), PID 4 (System->alive), 1000-call handle-leak probe (79->79, no growth). Ignored 2-process race (10 internal rounds) 0 failures. F6 (new, low severity): the packet's own third-pass Evidence table claims 16 passed for the conpty module; actual, reproducible count is 15 passed/0 failed/1 ignored (16 total tests) -- the packet's before/after figures are both off by one; does not change the verdict, every test genuinely passes. Full evidence: docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md, third-pass section; docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0083-verify.md."` |

**Proposed `story` row update, `BUG-0083`** (`harness.db`, table `story`, 17 columns,
`id='BUG-0083'`, rowid 169, `intake_id` 34 — read-only in this session; not written).
Current row: `status='in_progress'`, all four proof columns `0`, `evidence=NULL`,
`verify_command=NULL`, `last_verified_at=NULL`, `last_verified_result=NULL`. Proposed:

| Column | Proposed |
| --- | --- |
| `status` | `implemented` |
| `unit_proof` | 1 |
| `integration_proof` | 1 (cross-crate guard consistency, the elevation `--ignored` test, and the two independent Drop-on-panic probes are integration-level, not pure unit checks) |
| `e2e_proof` | 0 (no end-to-end app scenario touches this test-only plumbing) |
| `platform_proof` | 0 (developer machine only; the one `#[cfg(windows)]` guard shape (`native_crash.rs`) was exercised directly, the one `#[cfg(target_os = "macos")]` gap was not, matching the packet's own gap) |
| `evidence` | `docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0083-verify.md` |
| `verify_command` | `cargo test -p oneterm-app -p oneterm-session-ui -p oneterm-core -p oneterm-ssh -p oneterm-update -p oneterm-settings -p oneterm-sftp-ui -p oneterm-terminal -p oneterm-workspace --lib; cargo test -p oneterm-workspace --lib -- --ignored --test-threads=1; pwsh scripts/ci-local.ps1` |
| `last_verified_at` | `2026-09-29T<this pass's time>Z` |
| `last_verified_result` | `pass` |
| `notes` | append: `"Independent verify PASS <sha> (Fable 5.1, thinking high) on a217404f: all 11 fixed files read and confirmed against the same RAII TemporaryFile/TemporaryDirectory pattern BUG-0082 established -- Drop bodies trivial/non-panicking, every construction call site binds immediately (no .join()/.clone() chaining off a temporary), no guard implements Clone, unique names per file. Own re-grep of std::env::temp_dir() under crates/ independently finds 31 files/43 matches, matching the packet's own 31-row table exactly (F7: the packet's OWN prose summary says '29 files, 30 matches', inconsistent with its own table -- a documentation nit, not a missing file). The 3 ssh files (agent_tests.rs/route_tests.rs/tunnel_tests.rs) confirmed already guarded via test_support.rs's TempKnownHosts, correcting BUG-0082's original candidate list as claimed. update/install.rs's macOS-only gap confirmed correctly cfg-gated out of this Windows build. Ran 2 independent throwaway Drop-on-panic probes (native_crash.rs TemporaryFile, ui_config.rs TemporaryDirectory) beyond the packet's own six -- both removed their fixture during unwind. Every touched crate's test suite 0 failed, counts matching the packet's claims exactly. %LOCALAPPDATA%\\Temp counts flat (42 baseline, unchanged) across a full cargo test --workspace run, 2 more oneterm-vt runs, and the full ci-local gate. Full pwsh scripts/ci-local.ps1: ci-local: all checks passed. Full evidence: docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0083-verify.md."` |

The `notes` appends for both rows preserve every prior entry; only a further append is
proposed in each case. Neither row was written this session (`harness.db` read-only per
the task's own instruction).

## How the probes were applied and reverted

F2's 7 throwaway `verify_probe_*` tests were added to `crates/vt/src/pty/windows/
conpty.rs`'s test module, built once (`cargo build -p oneterm-vt --lib --tests`), run
filtered by name, then reverted with `git checkout -- crates/vt/src/pty/windows/
conpty.rs`; `git status --porcelain` was empty immediately before and after. F4's two
throwaway Drop-on-panic probes were added to `crates/app/src/native_crash.rs` and
`crates/settings/src/ui_config.rs`, each run individually (each reported `FAILED`, the
expected outcome of an intentional panic), the fixture's absence confirmed from outside
the test process via `Test-Path`, then both files reverted with `git checkout --
crates/app/src/native_crash.rs crates/settings/src/ui_config.rs`; `git status
--porcelain` was empty immediately before and after. Nothing outside
`%LOCALAPPDATA%\Temp` and this worktree's own `target/` was touched by any probe.
