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
