# Evidence: independent verification of BUG-0082

Subject: `a7854507` on `fix/vt-pty-test-tempdir`, one commit on top of `main @8b4b7abd`.
Packet: `docs/spec-intakes/IN-0029-vt-engine/BUG-0082-pty-test-tempdir-leak.md`.
Verifier host: Windows 11, MSVC target, debug builds throughout, `CARGO_BUILD_JOBS=4`.
Date: 2026-09-29.

## Verdict

**FAIL — the sweep's safety claim is false, and it is reproducible on demand.**
`scratch_directory`'s stale-sibling sweep (`crates/vt/src/pty/windows/conpty.rs:515-538`) deletes
every directory matching the `oneterm-vt-pty-{name}-` prefix except its own current-PID name, with
**no liveness check of any kind** (no `OpenProcess`/`GetExitCodeProcess`, no `sysinfo`, confirmed by
grep — nothing in the file or its `Cargo.toml` reaches Win32 process APIs). The packet's own
justification — "a sibling from an earlier test process is safe to remove unconditionally: that
process has exited" — is asserted, not checked, and is empirically false in general: it is only ever
true for the `bundled` family, and only *after* `LoadLibraryW` has locked `conpty.dll` inside it.
Before that lock exists (a real window between `create_dir_all` and the DLL load, present on every
run) and for the whole life of the `system` family (which never locks anything), the guard is
nothing but "does this name look like a sibling," and two genuinely concurrent test processes can
and do delete each other's live scratch directory.

Reproduced two ways, both direct evidence against the task's stated pass bar ("Any deletion of a
live process's directory = FAIL"):

* **F1a** — a fake sibling directory named after a real, running, unrelated process's PID
  (`explorer.exe`, live for the whole test) was deleted by one real `conpty_api_prefers_the_bundled_host`
  run, unconditionally, with that PID still running afterward.
* **F1b** — 15 iterations of two genuinely concurrent OS processes each running
  `conpty_api_prefers_the_bundled_host` produced a spurious `FAILED` (a real `assert_eq!` panic,
  not a hang or a flake in the harness) in **13 of 15** (87 %): the loser's sweep deletes the
  winner's — or rather, the winner deletes the loser's not-yet-locked directory before it calls
  `LoadLibraryW`, so `resolve_in` falls back to `System` and the loser's own assertion
  (`assert_eq!(..., Bundled)`) fails.

Everything else in the packet verifies. Drop-on-panic is correct (reproduced with a throwaway
`#[ignore]` probe, reverted). No other call site repeats the `.join()`-off-a-temporary chaining bug
the implementer already found and fixed once. The four family counts are flat across three runs each
of the specified configurations on this machine, matching the packet's own numbers. The sweep is
best-effort and does not panic on a locked or otherwise undeletable sibling. No new dependency was
added, and — worth recording for whoever reworks this — `Win32_System_Threading` (which has
`OpenProcess`/`GetExitCodeProcess`) is *already* a workspace `windows-sys` feature (`Cargo.toml:139`,
enabled for `US-0137`'s `GetProcessMemoryInfo`), so a liveness check costs no new dependency either.
Public API, rustdoc, fmt, the ignored-test census, and `--no-default-features` all pass. The full
`pwsh scripts/ci-local.ps1` gate result is recorded below.

## Findings

### F1 — Sweep deletes same-prefix siblings with no liveness check (severity: high, FAIL)

Read `crates/vt/src/pty/windows/conpty.rs:515-538` (the diff in `a7854507`). The sweep loop:

```rust
if let Ok(entries) = std::fs::read_dir(&base) {
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_str().is_some_and(|name| name.starts_with(&prefix) && name != current_name.as_str())
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
```

The only test applied to a candidate sibling is "same prefix, not my own PID's name." Grepped the
whole file (and `crates/vt/Cargo.toml`) for `OpenProcess`, `GetExitCodeProcess`, `sysinfo`,
`is_alive`, `ProcessId`, `liveness`: zero matches. There is no code path that could skip a live
sibling; correctness depends entirely on the *accident* that Windows refuses to delete a file another
process still has mapped (`ERROR_SHARING_VIOLATION`), which only applies once `LoadLibraryW` has run.

**F1a — direct proof with a real live PID.** Picked a real, currently-running, unrelated process
(`explorer.exe`, PID 32020 on this host) and pre-created
`%LOCALAPPDATA%\Temp\oneterm-vt-pty-bundled-32020\sentinel.txt` to simulate a same-prefix sibling
whose "owner" is still alive (the PID-reuse / live-sibling case the task asks for):

```
Before: exists = True
test pty::windows::conpty::tests::conpty_api_prefers_the_bundled_host ... ok
After: exists = False
   Id ProcessName StartTime
   -- ----------- ---------
32020 explorer    9/24/2026 5:38:33 PM   <- still running after the sweep deleted its "directory"
```

The sweep deleted a directory named after a PID it never checked, while that PID was demonstrably
still alive. This is the exact failure mode the task names as automatic: *"Any deletion of a live
process's directory = FAIL."*

**F1b — real concurrent-process race, no simulation.** Built the `oneterm-vt` lib test binary once
(`cargo test -p oneterm-vt --lib --no-run`) and launched it directly (bypassing `cargo test`'s own
build-lock serialization) as two independent OS processes running
`conpty_api_prefers_the_bundled_host --exact --test-threads=1` at the same instant, 15 times:

| Iteration | Process A | Process B |
| --- | --- | --- |
| 0–9, 11, 13 (13 of 15) | **FAILED** — `assert_eq!` panic, `left: System, right: Bundled` | `ok` |
| 10, 12 (2 of 15) | `ok` | `ok` |

Every failure is the same assertion panic at `conpty.rs:547`, i.e. `load_bundled` did not find
`conpty.dll` where it had just been staged. The mechanism: process A creates its directory and starts
copying `conpty.dll` in; before A calls `LoadLibraryW` (the only thing that would lock the file),
process B's sweep runs, sees A's directory as an unrecognized sibling, and removes it. Whichever
process's sweep runs second in a given iteration is the one that survives — the data above does not
imply an ordering guarantee, only that the race is real and produces a wrong, spurious test failure
the vast majority of the time under genuine concurrency. No corruption, no hang, no panic in the
sweep itself — just a false-negative test result that would be reported (and likely re-investigated
as a "flaky ConPTY test") by anyone hitting it.

**Control: the `system` family does not fail under the identical race** (6 of 6 concurrent iterations
of `conpty_api_falls_back_to_the_system_host` passed). Not because it is protected — it has no locked
file at all, so it is *more* exposed to the same unconditional deletion — but because that test's own
assertion (`ConptyBackend::System`) is what happens anyway when `conpty.dll` is absent, whether the
directory was swept out from under it or not. The sweep is equally blind for both families; only the
`bundled` test's assertion is sensitive enough to expose it.

**Why this matters here specifically, not just in the abstract.** The scratch directory lives under
`%LOCALAPPDATA%\Temp`, which is a single OS-wide location, not scoped per `CARGO_TARGET_DIR` or per
worktree. This repository's own present working setup — many `worktree-agent-*` branches, i.e.
multiple concurrent Claude Code sessions building and testing the same crate on one physical Windows
machine — is exactly the concurrency shape that triggers F1b. Two independent `cargo test -p
oneterm-vt` invocations from two different worktrees, run close enough together, hit this.

CI itself is not exposed the same way: `.github/workflows/ci.yml`'s `concurrency:` group
(`ci-${{ github.workflow }}-${{ github.ref }}`, `cancel-in-progress: true`) allows only one run per
ref, and `windows-quality` is the only job that runs on `windows-latest` and reaches
`crates/vt/src/pty/windows.rs` — so within one CI run there is exactly one Windows process touching
this code. Two *different* refs (two PRs) running CI at once each get their own GitHub-hosted runner
VM with its own `%LOCALAPPDATA%\Temp`, so they cannot collide either. `cargo test --workspace` itself
does not run multiple test binaries concurrently by default (confirmed: the ci-local run below shows
each `Running tests\...` line finishing before the next starts). The exposure is real developer/agent
machines, not the CI matrix as configured today — which is exactly this verification environment.

**Recommended direction (not implemented here, out of scope for a verification pass):** before
deleting a same-prefix sibling, parse the trailing PID out of its name and skip it unless
`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, ...)` fails or `GetExitCodeProcess` reports something
other than `STILL_ACTIVE` — both already reachable with zero new dependency, since
`Win32_System_Threading` is already an enabled `windows-sys` feature (`Cargo.toml:139`, for
`GetProcessMemoryInfo`, `US-0137`). A name that fails to parse as `<prefix><u32>` should be left
alone (not a directory this code owns).

### F2 — `Drop` runs on panic (severity: info, confirmed)

Added a throwaway `#[ignore] fn verify_probe_drop_runs_on_panic` to `conpty.rs`'s test module that
creates a `scratch_directory("panic-probe")` and immediately panics; reverted with `git checkout --`
before this file was written (`git status --porcelain` was empty before and after — this evidence
file is the only change committed).

```
thread '...verify_probe_drop_runs_on_panic' (27348) panicked at conpty.rs:573:9:
verify-probe: ...; path=C:\Users\trunglt\AppData\Local\Temp\oneterm-vt-pty-panic-probe-12812
Directory exists after panic-unwind Drop: False
```

Confirms `Drop` fires during unwind for a scratch directory with no locked file (the shape shared by
`system`, `TemporaryDirectory` in `crash_report.rs`, and `TempDirGuard` in `dock_persistence.rs`).
For `bundled`, immediate removal on panic is impossible for the same reason it is impossible on a
normal pass (the DLL stays locked regardless of how the test function exits) — the packet is correct
that this is deferred to the next process's sweep, not skipped; see F1 for why that deferral
mechanism is unsafe under concurrency.

### F3 — No other instance of the `.join()`-off-a-temporary chaining bug (severity: info, confirmed)

Every call site of all three helpers:

* `scratch_directory`: 2 call sites (`conpty.rs:544,555`) — both `let directory = scratch_directory(...)`.
* `temporary_directory` (`crash_report.rs`): 10 call sites, all `let directory = temporary_directory(...)`
  except the one the implementer already fixed (`reports_and_store_are_private_to_the_user`, now
  `let root = temporary_directory("private"); let directory = root.join("crashes");` — bound
  separately, as claimed).
* `temp_directory` / `TempDirGuard` (`dock_persistence.rs`): 11 call sites, all plain `let directory =
  temp_directory(...)`.

No other site chains a method off the constructor's return value in the same statement. Confirmed by
grep (`temporary_directory(`, `scratch_directory(`, `temp_directory(`) across the three files.

### F4 — Counts reproduce on this machine, flat across all specified configurations (severity: info, confirmed)

| Family | Config | Runs | Before -> After (each run) |
| --- | --- | --- | --- |
| `oneterm-vt-pty-bundled-*` | `cargo test -p oneterm-vt --lib` (default) | x3 | 1->1, 1->1, 1->1 |
| `oneterm-vt-pty-bundled-*` | `--features vt-paranoid` | x3 | 1->1, 1->1, 1->1 |
| `oneterm-vt-pty-bundled-*` | `-- --test-threads=1` | x3 | 1->1, 1->1, 1->1 |
| `oneterm-vt-pty-system-*` | all of the above | x9 | 0->0 throughout |
| `oneterm-crash-report-test-*` | `cargo test -p oneterm-app --lib -- crash_report` | x3 | 12->12, 12->12, 12->12 |
| `oneterm-dock-*` | `cargo test -p oneterm-state --lib -- dock_persistence` | x3 | 24->24, 24->24, 24->24 |

All 588 `oneterm-vt --lib` tests passed in every one of the 9 single-process runs (2 ignored, the
elevation-style tests unrelated to this packet); `oneterm-app` crash_report: 16 passed / 10 filtered
out, x3; `oneterm-state` dock_persistence: 12 passed / 31 filtered out, x3. `cargo test -p oneterm-vt
--no-default-features` separately: 558 + assorted doctest/example suites, all passed. This matches
the packet's own before/after story (25/24/12/24 baseline draining to steady state, then flat) even
though the absolute starting numbers differ (this machine's own historical backlog).

The `oneterm-vt-pty-bundled-*` steady state of exactly 1 was independently confirmed to behave as
described: a single-process run leaves its own directory standing (DLL locked, `Drop` fails silently),
and the *next* single-process run's sweep removes the previous one, holding the family at exactly 1
indefinitely — provided no second process's sweep or creation races with the first (see F1).

### F5 — The packet's own "other `temp_dir()` users checked" list is incomplete (severity: moderate, documentation gap)

The packet's Context section names 6 files as "already have [a guard]" and 2 as BUG-0083 candidates
(`settings/src/ui_config.rs`, `settings/src/terminal_config/document_tests.rs`, both confirmed
accurate — each uses `let _ = std::fs::remove_dir_all(directory);` as the last statement of the test,
the same latent panic-leak shape `crash_report.rs` had). Grepping every `std::env::temp_dir()` call
site in `crates/` independently (`grep -rl "std::env::temp_dir()" crates/`) returns 27 files, not the
8 the packet discusses. Cross-referencing against every file containing `impl Drop for` (28 files)
finds these additional test-only, unguarded, same-pattern users the packet does not mention:

* `crates/app/src/native_crash.rs:242` — `fs::remove_file(path).expect(...)` as the last statement
  (leaks one file, not a directory tree, on an earlier panic).
* `crates/session-ui/src/auth_form.rs:469` — `std::fs::remove_file(path).unwrap()` as the last
  statement (same shape, one file).
* `crates/core/src/config/shell.rs` — test module using `temp_dir()`, no `impl Drop` in the file.
* `crates/ssh/src/agent_tests.rs`, `crates/ssh/src/route_tests.rs`, `crates/ssh/src/tunnel_tests.rs`
  — no `impl Drop` in any of the three (unlike sibling files in the same crate —
  `handler_tests.rs`, `keyfile_tests.rs`, `test_support.rs`, `sftp_task/handle_limit_tests.rs`,
  `sftp_task/transfer/in0037_verify_tests.rs`, `sftp_task/transfer/pipeline_budget_tests.rs`,
  `us0095_verify_tests.rs` — which do have one, so the crate is inconsistent, not uniformly exposed).
* `crates/update/src/config.rs` — no `impl Drop` in the file (its sibling test files in the same
  crate, `archive_tests.rs` and `manager_tests.rs`, do have one).

None of these were run in isolation to confirm they actually leak on a real panic (same caveat the
packet already records for its own two candidates) — flagged here only so a `BUG-0083` scoped from
this packet's Context does not under-count. Not a defect in `a7854507`'s own diff; a completeness gap
in what that diff's packet claims to have checked.

### F6 — Sweep is best-effort; does not panic on a locked or otherwise undeletable sibling (severity: info, confirmed)

Created a sibling `bundled` directory containing a file held open with an exclusive lock
(`FileShare.None`) from this PowerShell session, then ran the real `conpty_api_prefers_the_bundled_host`
test:

```
test pty::windows::conpty::tests::conpty_api_prefers_the_bundled_host ... ok
Locked dir still exists: True
Locked file still exists: True
```

The `let _ = std::fs::remove_dir_all(entry.path());` swallows the failure and moves on; the test does
not panic and the locked sibling is left alone (not because of a liveness check — see F1 — but because
the OS itself refused the delete). This is the one case where the sweep's total absence of error
handling happens to be safe.

### F7 — No new dependency; a zero-dependency fix for F1 already exists in the crate graph (severity: info, confirmed)

`git show a7854507 --stat -- Cargo.toml Cargo.lock crates/vt/Cargo.toml crates/app/Cargo.toml
crates/state/Cargo.toml` touches none of those files — confirmed no dependency changed. Separately:
`Win32_System_Threading` (which has `OpenProcess` and `GetExitCodeProcess`, the two calls a liveness
check needs) is already enabled in the workspace `windows-sys` feature list (`Cargo.toml:139`, added
for `US-0137`'s `GetProcessMemoryInfo`), and `windows-sys` is already an optional dependency of
`oneterm-vt` gated on the `pty` feature (`crates/vt/Cargo.toml:25,65`). A liveness-checked sweep would
cost zero new dependencies.

### F8 — Public API, rustdoc, formatting, ignored-test census, `--no-default-features` all pass (severity: info, confirmed)

```
$ python scripts/vt-public-api.py --check --no-doc
public API surface unchanged (public-api.windows.txt)

$ python scripts/vt-public-api.py --diff-platforms
(only the pre-existing 6-line pty delta; unchanged by this commit)

$ grep -rn '...US-0.../BUG-0.../DEC-0.../IN-0.../crates\//docs\/...' crates/vt/src --include='*.rs' | grep -v 'https://github.com/'
(no matches -- passes)

$ cargo fmt --all -- --check
(exit 0)

$ python scripts/check-ignored-tests.py
check-ignored-tests: 18 ignored tests, all recorded

$ cargo test -p oneterm-vt --no-default-features
(all suites: ok, 0 failed)
```

## What could not be verified

* **CI's actual Windows runner.** All of the above ran on this developer machine, matching the
  packet's own stated gap. The `windows-quality` job would run this code but was not exercised on a
  hosted runner.
* **F1 under `cargo test --workspace` itself** (as opposed to two directly-launched binaries). Cargo
  runs test binaries sequentially by default within one invocation (observed in the ci-local run
  below: each `Running tests\...` line completes before the next starts), so a single `cargo test
  --workspace` does not self-race. The exposure demonstrated is specifically *two independent
  invocations* (two developers, two worktrees, an IDE test runner alongside a terminal one, etc.).
* **BUG-0083 candidates' actual leak behavior** (F5) — named but not run to confirm they leak on a
  real panic, same as the packet's own two candidates.

## Gate

Run in this worktree at `a7854507` with a clean tree (`git status --porcelain` empty before and after
every probe), `CARGO_BUILD_JOBS=4`, `target/debug/incremental` deleted before the full gate:

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-vt --lib` (default) x3 | **PASS** — 588 passed, 0 failed, 2 ignored, each run |
| `cargo test -p oneterm-vt --lib --features vt-paranoid` x3 | **PASS** — 588 passed, 0 failed, 2 ignored, each run |
| `cargo test -p oneterm-vt --lib -- --test-threads=1` x3 | **PASS** — 588 passed, 0 failed, 2 ignored, each run |
| `cargo test -p oneterm-vt --no-default-features` | **PASS** — all suites ok |
| `cargo test -p oneterm-app --lib -- crash_report` x3 | **PASS** — 16 passed, 0 failed, each run |
| `cargo test -p oneterm-state --lib -- dock_persistence` x3 | **PASS** — 12 passed, 0 failed, each run |
| Two concurrent `conpty_api_prefers_the_bundled_host` processes x15 | **FAIL** — 13/15 spurious panics (F1b) |
| `pwsh scripts/ci-local.ps1` | **PASS** — every step green, 0 real test failures (grepped `FAILED` case-insensitively across the whole run: every hit is a test *name* containing the word, e.g. `failed_transfer_handle_reports_the_error_once ... ok`; every `test result:` line reads `0 failed`) |

Gate's final line:

```
ci-local: all checks passed.
```

## Records

**Proposed `story` row update** (`harness.db`, table `story`, 17 columns, `id='BUG-0082'`, rowid 168,
`intake_id` 34 — read-only in this session; not written):

| Column | Current | Proposed |
| --- | --- | --- |
| `status` | `in_progress` | `in_progress` (unchanged — F1 blocks acceptance, not a rework-and-reopen of *shipped* behavior since this packet was never accepted) |
| `unit_proof` | 0 | 0 (unchanged) |
| `integration_proof` | 0 | 0 (unchanged) |
| `e2e_proof` | 0 | 0 (unchanged) |
| `platform_proof` | 0 | 0 (unchanged — still developer-machine only) |
| `evidence` | `NULL` | `docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md` |
| `verify_command` | `NULL` | `cargo test -p oneterm-vt --lib -- conpty (x2 concurrent processes, x15); pwsh scripts/ci-local.ps1` |
| `last_verified_at` | `NULL` | `2026-09-29T<verification time>Z` |
| `last_verified_result` | `NULL` | `fail` |
| `notes` | (existing, see below) | append: `"Verify FAIL <sha> (Fable 5.1, thinking high): sweep has no PID-liveness check -- deletes any same-prefix sibling by name alone. F1a: deleted a fake sibling named after a real live PID (explorer.exe). F1b: two genuinely concurrent bundled-host test processes spuriously failed 13/15 runs (peer's sweep removed the not-yet-DLL-locked directory before LoadLibraryW). system family unaffected only because its assertion doesn't depend on directory survival, not because it's protected. Everything else verifies: Drop-on-panic correct, no other .join()-chaining bug, counts flat (bundled 1, system 0, crash-report 12, dock 24) across 9 runs x3 configs, best-effort sweep doesn't panic on a locked sibling, no new dependency (Win32_System_Threading already enabled -- OpenProcess/GetExitCodeProcess is a zero-dependency fix). BUG-0083 candidate list in the packet's Context is incomplete: 6 more unguarded temp_dir() test files found (native_crash.rs, auth_form.rs, core/config/shell.rs, ssh/agent_tests.rs, ssh/route_tests.rs, ssh/tunnel_tests.rs, update/config.rs). Full evidence: docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md."` |

The existing `notes` value (owner report, implementer summary) is preserved; only an append is
proposed.

## How the probes were applied and reverted

F1a and F6 only created/removed directories and files directly under `%LOCALAPPDATA%\Temp` (never
inside this worktree); nothing in the repository was touched. F1b built the `oneterm-vt` lib test
binary once (`cargo test -p oneterm-vt --lib --no-run`) and invoked the resulting `.exe` directly,
bypassing `cargo test`; no source was mutated for this probe. F2's throwaway
`verify_probe_drop_runs_on_panic` was added to `crates/vt/src/pty/windows/conpty.rs`, run once, then
reverted with `git checkout -- crates/vt/src/pty/windows/conpty.rs`; `git status --porcelain` was
empty immediately before and after. This evidence file is the only change carried into the
`verify/bug-0082` commit.

---

## Pass 2 (2026-09-29): re-verification of the rework at `8ddf10fc`

Subject: `8ddf10fc` on `fix/vt-pty-test-tempdir`, one commit on top of this file's own `326bc389`
(which is one commit on top of `a7854507`, on top of `main @8b4b7abd`).

### Verdict

**PASS.** The rework replaces the unconditional name-only sweep with a liveness check
(`is_stale` + `process_is_alive`, `OpenProcess`/`GetExitCodeProcess`) and closes every case F1/F1a/F1b
found. All four liveness cases behave correctly, the 15-round real concurrent race that failed 13/15
times in pass 1 now passes 15/15, and the full local CI gate is green. One residual gap is worth the
owner's attention but does not block acceptance: `GetExitCodeProcess`'s `STILL_ACTIVE` ambiguity is
unhandled (no `WaitForSingleObject` disambiguation) — see F9 — but its only possible failure mode is
an extra-conservative "treat as alive, keep the directory," the same safe default the code already
chooses deliberately elsewhere, and it requires a dead process to have exited with the exact code 259,
which no PID this sweep ever evaluates (a `cargo test` binary: exit 0/101; a `cmd /c pause` child: not
259 either) is likely to produce.

### F1 re-test — the four liveness cases, explicitly

All four run against the *real* production code path (the shipped `scratch_directory` /
`process_is_alive`, not a re-implementation), by pre-seeding fake sibling directories under the
`system` family's prefix and running the real `conpty_api_falls_back_to_the_system_host` test, which
invokes the real sweep over all `oneterm-vt-pty-system-*` entries:

| Case | PID used | Expected | Observed |
| --- | --- | --- | --- |
| Live PID | `explorer.exe`, PID 32020 (real, running for the whole test) | kept (survives) | **kept** — directory existed after the sweep |
| Dead PID | a `cmd.exe` child spawned then `Stop-Process -Force`d by this session, confirmed not running (`Get-Process -Id <pid>` empty) before the sweep ran | swept (removed) | **swept** — directory gone after the sweep |
| Unopenable / protected PID | `csrss.exe`, PID 1328 (a protected system process; `OpenProcess` fails with `ERROR_ACCESS_DENIED`, not `ERROR_INVALID_PARAMETER`, since the process genuinely exists) | kept (safe default — "any other `OpenProcess` failure answers alive") | **kept** — directory existed after the sweep |
| Unparsable name | `oneterm-vt-pty-system-not-a-pid` (suffix does not parse as `u32`) | kept (not a directory this code owns) | **kept** — directory existed after the sweep |

```
Before: unparsable=True dead=True protected=True live=True
test pty::windows::conpty::tests::conpty_api_falls_back_to_the_system_host ... ok
After:  unparsable=True dead=False protected=True live=True
```

All four match the claimed decision table exactly. The "protected" case is the one pass 1 did not
test at all: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, 1328)` against a genuinely-running
process this session cannot open returns `ERROR_ACCESS_DENIED` (not `ERROR_INVALID_PARAMETER`), which
`process_is_alive` correctly routes to "alive" (the `!= ERROR_INVALID_PARAMETER` branch), so the
directory is kept rather than deleted out from under a process this code merely lacks permission to
inspect.

### F1b re-test — 15 rounds of real concurrency, 0 spurious failures

Same harness as pass 1 (two independently-launched OS processes, each running
`conpty_api_prefers_the_bundled_host --exact --test-threads=1` directly against the built `.exe`,
bypassing `cargo test`), 15 rounds:

```
Total spurious failures across 15 concurrent iterations: 0
```

Where pass 1 hit **13 of 15** spurious `assert_eq!` panics, this pass hits **0 of 15**. The mechanism
is now liveness, not lock-accident: a peer's sweep no longer removes a directory just because the DLL
hasn't been `LoadLibraryW`'d into it yet — it is skipped because its owning PID is alive, at every
stage of that owner's own setup. Also ran the shipped `#[ignore]`d
`concurrent_bundled_host_race_does_not_spuriously_fail` test directly (its own internal 10 iterations):
`ok`, `1 passed; 0 failed`.

### F-PIDreuse — PID reuse is the accepted residual, not a new hole

The "live PID" case above **is** the PID-reuse case: `explorer.exe`'s PID was never truly the PID of a
`oneterm-vt-pty-system-*` scratch-directory owner. The code has no way to distinguish "the same
conceptual test process that created this directory" from "some unrelated live process that now holds
this PID number" — `is_stale`/`process_is_alive` only ever ask "is *a* process alive at this PID right
now," which is exactly what PID reuse requires. Consequence, as the task frames it: a stale directory
whose PID has been reused by an unrelated live process is kept (never deleted while that PID names
anything alive), and is swept later by whichever process's sweep runs after the reusing process (or
the original, whichever exited later) finally exits and the PID goes idle or gets reused by something
this crate's own next test process also doesn't collide with. This is the correct, safe residual:
over-retention, never over-deletion.

### F9 — `STILL_ACTIVE` (259) cast ambiguity: unhandled, severity low

Grepped `crates/vt/src/pty/windows/conpty.rs` for `WaitForSingleObject`: zero matches. `process_is_alive`
disambiguates a dead-vs-alive process only by `GetExitCodeProcess(handle, &mut exit_code) != 0 &&
exit_code == STILL_ACTIVE as u32`. This is the documented Win32 ambiguity: if a process has **already
exited** with the specific exit code `259` (`STILL_ACTIVE`, `0x103`) and something still holds the
kernel process object alive (so `OpenProcess` by PID still resolves to it rather than failing with
`ERROR_INVALID_PARAMETER`), `GetExitCodeProcess` returns `259` indistinguishably from "still running,"
and `process_is_alive` reports **alive** for an actually-dead process. The documented fix is an
additional `WaitForSingleObject(handle, 0)`: `WAIT_OBJECT_0` means the process object is signaled
(exited) regardless of what `GetExitCodeProcess` reports; `WAIT_TIMEOUT` means still running. Not
present here.

**Severity: low, not a safety defect.** Two independent reasons:

1. **The failure direction is the safe one.** A misclassified dead-as-alive PID causes the sweep to
   *keep* that stale directory rather than delete it — the exact same "when genuinely uncertain, don't
   delete" default the code already chooses on purpose for `ERROR_ACCESS_DENIED` and unparsable names.
   It can never cause a live directory's deletion, which is the property this whole rework exists to
   guarantee. At worst it is one extra directory not yet swept.
2. **The trigger condition does not apply to any PID this sweep ever evaluates.** The only PIDs that
   ever appear in `oneterm-vt-pty-{bundled,system}-<pid>` names are `std::process::id()` of the
   `oneterm_vt-*.exe` test binary process itself (`cargo test`'s Rust test harness exits `0` on success
   or `101` on failure — never `259`) and, in the new `sweep_keeps_a_sibling_named_after_a_live_pid`
   regression test, a `cmd /c pause` child that is always explicitly `kill()`ed by the test rather than
   left to exit on its own with any particular code. `259` is not a code either process family
   organically produces, so this is a latent gap in the general-purpose `process_is_alive` helper, not
   a live exposure of the specific sweep it backs today. Worth a one-line `WaitForSingleObject(handle,
   0)` if this helper is ever reused somewhere PIDs are less controlled (recorded here rather than
   fixed, since fixing it is outside a verification pass's scope and the packet did not claim to have
   handled it).

A secondary, even narrower gap in the same function: if `OpenProcess` succeeds but the immediately
following `GetExitCodeProcess` itself fails (returns `0`), `still_active` short-circuits to `false`
(treated as dead). `GetExitCodeProcess` only needs `PROCESS_QUERY_LIMITED_INFORMATION`, the exact right
`OpenProcess` just granted, on a handle used once immediately after opening it, so this is expected to
be unreachable in practice; noted for completeness, not filed separately.

### Census and ignored-test entry

`scripts/ignored-tests.txt` gained exactly one line, in alphabetical position:
`pty::windows::conpty::tests::concurrent_bundled_host_race_does_not_spuriously_fail`. Confirmed against
the live file (`git show 8ddf10fc -- scripts/ignored-tests.txt`). `python scripts/check-ignored-tests.py`
on this host:

```
check-ignored-tests: 19 ignored tests, all recorded
```

(18 before this rework, matching pass 1's own count; 19 after, matching the packet's claim exactly.)

### Counts, flat over 3 runs

| Family | Config | Runs | Before -> After |
| --- | --- | --- | --- |
| `oneterm-vt-pty-*` | `cargo test -p oneterm-vt --lib` (default) | x3 | 2->1 (draining a leftover pair from the F1b race probe above), 1->1, 1->1 |
| `oneterm-vt-pty-*` | `--features vt-paranoid` | x3 | 1->1, 1->1, 1->1 |
| `oneterm-vt-pty-*` | `--no-default-features` | x1 | all suites `ok`, `0 failed` |

Every `oneterm-vt --lib` run: 593 passed, 0 failed, 3 ignored (up from 588/2 in pass 1 — the four new
`is_stale` unit tests plus one net new test, minus the one now-`#[ignore]`d race test moving out of
the counted total). `is_stale`'s four unit tests and `sweep_keeps_a_sibling_named_after_a_live_pid`
all pass individually; the `#[ignore]`d race test passes both via its own internal 10 iterations and
via this pass's independent 15-round external harness (F1b above).

### Static gates

```
$ python scripts/vt-public-api.py --check --no-doc   (fresh cargo doc -p oneterm-vt --no-deps --all-features)
public API surface unchanged (public-api.windows.txt)

$ python scripts/vt-public-api.py --diff-platforms
(only the pre-existing 6-line pty delta; unchanged by this commit)

$ grep -rn '...US-0.../BUG-0.../DEC-0.../IN-0.../crates\//docs\/...' crates/vt/src --include='*.rs' | grep -v 'https://github.com/'
(no matches -- passes. The new code does cite `BUG-0082` four times (`conpty.rs:581,650,688,698`),
but the grep's pattern only matches `///`/`//!` doc-comment lines, and every one of those four is a
plain `//` line comment inside `#[cfg(test)] mod tests` -- never emitted into rustdoc and never a doc
comment to begin with, so the grep correctly does not flag them)

$ cargo fmt --all -- --check
(exit 0)

$ cargo clippy -p oneterm-vt --all-targets -- -D warnings
(exit 0)
```

### Gate

Run at `8ddf10fc`, clean tree before and after (only this evidence file's edit outstanding),
`CARGO_BUILD_JOBS=4`, `target/debug/incremental` deleted before the run:

```
ci-local: all checks passed.
```

Exit code 0. `grep "test result:" | grep -v "0 failed"` over the whole run's output: no matches (every
suite reports `0 failed`).

### Records

**Proposed final `story` row** (`harness.db`, table `story`, `id='BUG-0082'`, rowid 168,
`intake_id` 34 — read-only in this session; not written). The row already shows `evidence` and
`last_verified_result='fail'` from pass 1 (set outside this session, between passes); this proposes
promoting it to the pass-2 result:

| Column | Value before this pass | Proposed |
| --- | --- | --- |
| `status` | `in_progress` | `implemented` |
| `unit_proof` | 0 | 1 |
| `integration_proof` | 0 | 1 (the two-real-process concurrent regression and the live-child-PID regression are process-level integration checks, not pure unit tests) |
| `e2e_proof` | 0 | 0 (unchanged — no end-to-end app scenario touches this code) |
| `platform_proof` | 0 | 0 (unchanged — verified on this developer machine only; `#[cfg(windows)]` code, so CI's Windows job is the only place platform proof can come from, and this branch was not pushed) |
| `evidence` | `docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md` | unchanged (this file, now with the Pass 2 section) |
| `verify_command` | `NULL` | `cargo test -p oneterm-vt --lib -- conpty; cargo test -p oneterm-vt --lib -- --ignored concurrent_bundled_host_race; pwsh scripts/ci-local.ps1` |
| `last_verified_at` | `2026-09-29T00:00:00Z` | `2026-09-29T<pass-2 time>Z` |
| `last_verified_result` | `fail` | `pass` |
| `notes` | (pass-1 note, see file history) | append: `"Verify PASS (pass 2) <sha> (Fable 5.1, thinking high) on 8ddf10fc: is_stale/process_is_alive liveness check closes F1/F1a/F1b. All 4 liveness cases confirmed against the real sweep (live PID kept, dead PID swept, ERROR_ACCESS_DENIED-protected PID kept, unparsable name kept) -- the protected-PID case is new coverage pass 1 did not have. 15/15 concurrent real-process rounds now pass (was 13/15 FAIL in pass 1); shipped #[ignore]d race test's own 10 iterations also pass. PID reuse is the accepted residual (kept, swept later once the reusing PID also dies) -- not a new hole, same mechanism as the live-PID case. F9 (new): STILL_ACTIVE=259 cast ambiguity is unhandled (no WaitForSingleObject) but severity low -- only biases toward the same safe keep-it default already used elsewhere, and no PID this sweep ever evaluates (cargo test exit 0/101; a killed cmd/pause child) plausibly exits with 259. Counts flat over 3 runs, ignored-test census at 19 (was 18), full ci-local green (ci-local: all checks passed.). BUG-0083 candidate list (9 files) already folded into the packet's Handoff. Full evidence: docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md, Pass 2 section."` |

The `notes` append preserves every prior entry; only a further append is proposed.

---

## Pass 3 (2026-09-29): acceptance rework for F9 (exit code 259), self-verified with implementation

Subject: the rework applied on branch `fix/test-tempdirs-and-liveness` (this session), on top of
`main @fcff0cf3` (which already includes pass 2's `8ddf10fc`).

### What changed

`process_is_alive` (`crates/vt/src/pty/windows/conpty.rs`) no longer reads `GetExitCodeProcess` /
`STILL_ACTIVE` at all. `OpenProcess` now requests `PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION`,
and liveness is decided with `WaitForSingleObject(handle, 0)`: `WAIT_OBJECT_0` (signaled) means dead,
anything else (`WAIT_TIMEOUT` or `WAIT_FAILED`) means alive. This closes F9 exactly as that finding's
"documented fix" section proposed: the process object's own signaled state, not its numeric exit code,
now decides liveness, so a process that genuinely exited with the value `259` no longer collides with
"still running."

### Regression proof

A new non-ignored test, `process_is_alive_reports_dead_for_a_process_that_exited_with_code_259`, spawns
a real `cmd /c exit 259` child, waits for it (`status.code() == Some(259)` asserted first, so the test
is meaningless unless the child really did exit 259), then calls the real `process_is_alive` against its
PID:

```
test pty::windows::conpty::tests::process_is_alive_reports_dead_for_a_process_that_exited_with_code_259 ... ok
```

Confirmed by inspection (not re-run against the pre-rework code in this session, since the pre-rework
source is not on this branch to build): on `main @fcff0cf3`'s `process_is_alive`, `GetExitCodeProcess`
on this same exited child would return `259`, which equals `STILL_ACTIVE as u32`, so `still_active`
would be `true` — the exact false-alive misclassification F9 named. `WaitForSingleObject` cannot make
this mistake: it reports the kernel object's own signaled state, which is set at process termination
regardless of the numeric exit code.

### Full regression suite, still green

`pty::windows::conpty::tests`: 16 passed, 0 failed, 1 ignored (`concurrent_bundled_host_race_does_not_spuriously_fail`,
unaffected by this change, not re-run by hand this pass since the sweep's own liveness mechanism did
not change — only the low-level signal used to decide it did). `is_stale`'s four unit tests,
`sweep_keeps_a_sibling_named_after_a_live_pid`, `conpty_api_prefers_the_bundled_host`, and
`conpty_api_falls_back_to_the_system_host` all still pass unmodified.

### Static gates

```
$ cargo clippy -p oneterm-vt --all-targets -- -D warnings
(exit 0)

$ cargo fmt --all -- --check
(exit 0)

$ python scripts/check-ignored-tests.py
check-ignored-tests: 19 ignored tests, all recorded

$ python scripts/vt-public-api.py --check --no-doc   (fresh cargo doc -p oneterm-vt --no-deps --all-features)
public API surface unchanged (public-api.windows.txt)

$ python scripts/vt-public-api.py --diff-platforms
(only the pre-existing 6-line pty delta; unchanged)

$ (rustdoc self-containment check over crates/vt/src, python-based since rg is not installed on this
   box) -- 0 hits: every BUG-0082 citation in the new code is a plain `//` line inside
   `#[cfg(test)] mod tests`, never `///`/`//!`
```

### Gate

Combined with `BUG-0083` (same branch, same session): see that packet's evidence for the full
`pwsh scripts/ci-local.ps1` run covering both changes together.

### What was not independently re-verified this pass

This is the implementer's own self-verification recorded alongside the fix, not a separate verifier's
pass (unlike passes 1 and 2). No second reviewer ran the concurrent two-process race probe or the F1a
live-PID probe again, since neither mechanism changed in this rework — only `process_is_alive`'s
internal decision procedure did, and that is covered by the new regression test plus the unchanged
passing suite above.
