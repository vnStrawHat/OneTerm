# BUG-0055 independent verification

Verifier: separate agent, worktree
`D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a0315858a63eb58ae`
Diff under review: `2f47366..a3f180e` (10 files). Host: Windows 11 Enterprise 26200.
Date: 2026-09-14. Not committed.

## Verdict

**PASS WITH NOTES.**

The diagnosis holds, the fix is in the right place, and it is proven to work on this
machine: with the one call removed the regression test fails on run 1, and with it in
place every 0 ms close ends in a terminate at ~2.0 s with exit code `0x1`. The
`warn` that the packet promises was captured verbatim with its pid. `pwsh
scripts/ci-local.ps1` reproduces the implementer's totals exactly.

Five notes, none of them blocking, one of which will cost somebody a flaky CI run
(D2) and one of which is a documentation claim broader than the code delivers (D1).

## What DEC-0016 makes the app do, in plain language

When you close a terminal tab (or OneTerm discards one for you), OneTerm tells the
Windows console host to end that session and then watches the shell process it
started. If that shell has not exited two seconds later, OneTerm kills it, and writes
a line into its log saying which process id it killed. It only ever kills a process it
started itself, through the handle it has held since it created it; it never searches
the machine for processes by name, so no other program's shell or console window can
be hit. In normal use this never fires: a shell that is running and even one that is
busy with a long-running command exits by itself about 20 ms after the tab closes
(measured below, including a `ping -t` still running inside it). It only fires for a
shell that was killed off before it had finished starting, which is exactly the case
that today leaves a `cmd.exe` and a console host running invisibly forever. The cost of
saying yes is that OneTerm now terminates a process on your machine; the cost of saying
no is one stranded shell plus one console host per affected tab close, recoverable only
through Task Manager.

## 1. Thread analysis: where can a `PseudoConsole` drop, and can the UI stall?

Every construction and destruction site under `crates/` (grep for `PseudoConsole`,
`ChildExitWatcher`):

| # | Drop site | Thread | Can it wait out the 2 s? | UI thread affected |
| --- | --- | --- | --- | --- |
| 1 | `ShellEventLoop::run` returns; the loop (owning the PTY) drops at the end of the owner closure (`crates/local-shell/src/event_loop.rs:196-217`) | "PTY owner" | yes | no. `LocalSession::shutdown_owner` / `Drop` hand the join handle to a detached reaper (CORR-10) |
| 2 | `spawn_owned`'s `and_then` closure fails **after** `PseudoConsole::spawn` succeeded: `child_pid()` is `None` (`event_loop.rs:195`), or `ShellEventLoop::new` fails (`event_loop.rs:206`, `Poller::new`) | "PTY owner", but the PTY drops **before** `ready_tx.send(Err(..))` | yes | **yes** - the caller is parked in `ready_rx.recv()` (`event_loop.rs:224`), i.e. `LocalSession::spawn` on the UI thread, and now waits up to 2 s longer. See D3 |
| 3 | `conpty::spawn` error path after `CreateProcessW` succeeded (`crates/pty/src/windows/conpty.rs:310-324`) | same as 1/2 | only if the watcher was built; if `ChildExitWatcher::new` itself fails the child is leaked with no watcher at all (pre-existing, recorded in the packet) | as 2 |
| 4 | `crates/pty` own tests: `pseudo_console_tests.rs` (5 tests drop a live console at end of test), `child.rs` unit tests | test thread | yes | n/a |
| 5 | `crates/local-shell` tests through `LocalSession` | "PTY owner" | yes | n/a (caller unaffected, measured below) |
| 6 | `crates/tools/src/bin/pty-throughput.rs:47` | tool main thread | yes | n/a |
| 7 | Application quit (`crates/app/src/window.rs:72`, `cx.on_release -> cx.quit()`) | none - process exit kills the owner threads | **the escalation does not run at all** | n/a. See D1 |

Nothing joins the owner threads at quit (`reap_owner_thread` detaches one reaper per
session and no code joins the reapers), so N open tabs do **not** cost N x 2 s at exit.
The reverse is true: at exit the grace period is simply never served.

Measured, 3 sessions opened and dropped from the calling thread (temporary probe
`crates/local-shell/tests/drop_timing_probe.rs`, since deleted):

```
3 sessions dropped 0 ms after spawn: the caller thread blocked 321us
3 started sessions dropped:          the caller thread blocked 142.2us
```

So on the path the app actually uses, the caller pays microseconds, and the 2 s is
served by the detached owner thread. The escalation nevertheless did fire for those
three 0 ms sessions - on the owner threads, invisibly to the caller.

## 2. Semantics: what happens to a shell that is busy, and to its grandchild

Temporary probe `crates/pty/tests/busy_close_probe.rs` (since deleted): spawn
`cmd.exe /K <command>` inside a real pseudo-console, let it settle 1.5 s, record the
console host and the shell (children of the test process) and the grandchild (child of
the shell), then drop the `PseudoConsole` and poll all three.

With the fix (a3f180e):

```
== ping -t (busy, grandchild) == pids: [(9972,"conhost.exe"),(1952,"cmd.exe"),(12240,"PING.EXE")]
drop() blocked the calling thread for 6.8365ms
conhost.exe   (pid 9972)  exited 27 ms after the drop, code 0x0
cmd.exe       (pid 1952)  exited  6 ms after the drop, code 0xc000013a
PING.EXE      (pid 12240) exited  6 ms after the drop, code 0xc000013a

== timeout /t 30 (busy, grandchild) == pids: [(12172,"conhost"),(6548,"cmd"),(5688,"timeout")]
drop() blocked the calling thread for 4.2743ms
conhost.exe   (pid 12172) exited 24 ms, code 0x0
cmd.exe       (pid 6548)  exited  4 ms, code 0xc000013a
timeout.exe   (pid 5688)  exited  4 ms, code 0xc000013a

== idle cmd /K (baseline) == pids: [(2572,"conhost.exe"),(1712,"cmd.exe")]
drop() blocked the calling thread for 2.2011ms
conhost.exe   (pid 2572)  exited 22 ms, code 0x0
cmd.exe       (pid 1712)  exited  2 ms, code 0xc000013a
```

With `self.terminate_if_still_running();` commented out (same binary, nothing else
changed):

```
ping -t:      conhost 13024 / cmd 4908 / PING 5268   all exited 20 ms after the drop
timeout /t30: conhost 1160 / cmd 15072 / timeout 8088 all exited 20 ms after the drop
idle:         conhost 14192 / cmd 7788                all exited 20 ms after the drop
drop() blocked the calling thread for 94.5us / 193.7us / 265.5us
```

Findings:

- A busy shell is **not** terminated by the new code. The console host's close request
  reaches every client attached to the console, so `cmd.exe` *and* its grandchild
  (`PING.EXE`, `timeout.exe`) both exit with `0xc000013a` (`STATUS_CONTROL_C_EXIT`)
  within ~20 ms, two orders of magnitude inside the grace period. DEC-0016's
  "a shell that would have exited cleanly a moment later is never hit" is **supported**
  by measurement, including for the grandchild case DEC-0016 does not mention.
- Behaviour before and after the change is identical for these three shapes. The only
  difference is that the drop now returns a few ms later, because it now waits for the
  exit it previously did not observe (2-7 ms instead of 0.1-0.3 ms).
- A grandchild that had detached from the console (`start /b`, a GUI child, a service)
  would survive both before and after; the change neither helps nor hurts there.
  DEC-0016 does not claim it does.

Processes this probe started: 9972, 1952, 12240, 12172, 6548, 5688, 2572, 1712 (with the
fix) and 13024, 4908, 5268, 1160, 15072, 8088, 14192, 7788 (tampered run). All exited on
their own; the probe terminated none. No process was enumerated by name.

## 3. The `session.rs` change

Four added lines, and they are exactly the module declaration for the new test file:

```rust
#[cfg(all(test, windows))]
#[path = "session_orphan_tests.rs"]
mod session_orphan_tests;
```

No production change in `crates/local-shell`. It is needed (the file has to be declared
somewhere) and correctly gated: `session_orphan_tests.rs` uses `windows-sys`, which is
only a dev-dependency under `cfg(windows)`. `session_tests.rs`'s two-line change widens
`wait_until` and `spawn_default` to `pub(super)` so the new module can reuse them; both
are still used un-gated by ~14 call sites in `session_tests.rs`, so no `dead_code`
warning appears on the Linux and macOS CI jobs (`.github/workflows/ci.yml:54, 206`).

## 4. Are the regression tests any good?

**Tamper result (the important one).** With
`crates/pty/src/windows/child.rs:216` commented out and nothing else changed:

```
thread 'session::session_orphan_tests::a_session_dropped_before_its_shell_starts_leaves_no_orphan'
panicked at crates\local-shell\src\session_orphan_tests.rs:223:13:
run 1: dropping a session after 0 ms left conhost.exe (pid 10920) alive for 5s;
the probe had to terminate it
test result: FAILED. 0 passed; 1 failed; 34 filtered out; finished in 5.03s
```

Restored, the same test passes. The test bites on this machine, on run 1, exactly as the
packet claims.

**Vacuous-pass risk.** The test asserts "nothing survived", which is satisfied both by a
working fix and by a run in which the orphan never occurred. Note that a *slower* box
makes the orphan window wider, not narrower (the window is "`cmd.exe` has not finished
initialising"), so load is not the hazard; hardware where `cmd.exe` initialises faster
than the spawn path returns is. There is no assertion that the escalation path was
exercised at all. See D4.

**Platform gating.** `#[cfg(all(test, windows))]` on the module plus
`[target.'cfg(windows)'.dev-dependencies]` for `windows-sys`. Linux and macOS CI compile
and run `-p oneterm-local-shell` without either. Correct.

**Cleanup on the panic path.** `probe()` terminates every survivor it is responsible for
*before* returning, and `assert_no_orphan` asserts only on the returned rows, so the
assertion failure above still cleaned up (it terminated pid 10920 and its client). The
one hole is the `assert!` *inside* `probe()` ("the shell produced no output within 10 s"):
that unwinds with the watched handles still open, though the session itself is dropped by
the unwind and takes its shell with it. Acceptable.

**Cross-test contamination.** See D2 - this is the one real test defect.

## 5. Handle safety

No defect. `Drop::drop` runs `UnregisterWaitEx(wait, INVALID_HANDLE_VALUE)` first, which
by contract returns only after any running callback has returned and guarantees no
further callback; only then does `terminate_if_still_running` wait and terminate; the
`process` field (and therefore the handle) is destroyed after the `Drop::drop` body, so
the handle is live for both calls. The exit that `TerminateProcess` causes therefore
cannot re-enter `child_exited`, and `child_exited`'s raw `Arc` pointer cannot outlive its
allocation. `UnregisterWaitEx` is not called from inside the callback, which would
deadlock. `WaitForSingleObject` returning `WAIT_FAILED` falls into the terminate branch,
where the failure is logged rather than ignored - acceptable.

## 6. Dependency policy

`python scripts/verify-dependency-graph.py`: `Dependency graph policy passed for 21
workspace packages and 21 explicit members.` `windows-sys` is a **dev**-dependency of
`oneterm-local-shell` under `cfg(windows)`, already a workspace dependency at the same
0.59 pin, and the workspace feature union already carries
`Win32_System_Diagnostics_ToolHelp`, `Win32_System_Threading` and
`Win32_Storage_FileSystem`. The only `Cargo.lock` change is one line adding
`windows-sys 0.59.0` to the `oneterm-local-shell` dependency list - no new package, no
new version. The packet's claim that "no crate-graph edge changes and R1-R12 are
unaffected" is correct as stated (R1 is the DAG rule, R12 the feature-registration rule;
neither is touched by a dev-dependency).

The dev-dependency is justified: the probe has to enumerate the processes a spawn added
as children of the test process and then wait on each by handle. The `std` library has
no way to open or wait on a process it did not spawn, and `oneterm-pty`'s own watcher
cannot substitute - it watches only the one child it created, exposes neither a handle
nor a liveness query outside the crate, and would not see the console host at all (which
is the process the tamper run actually caught surviving).

## 7. Documentation

- `docs/decisions/DEC-0016-...md`: Status **Proposed**; bound (2 s, named constant),
  owner (`ChildExitWatcher::drop`, PTY owner thread), reporting (`warn` with the pid) and
  scoping (own child, by handle, never by name) are all stated. Alternatives and
  consequences are filled in. Correct and honest, with two exceptions: D1 and D5 below.
- `docs/terminal-backend.md` 6.2 and 6.3: both updated, and 6.3's Close bullet is an
  accurate summary of the code. 6.2's sentence is the overclaim in D1.
- `IN-0031.md`: the `BUG-0055` line is ticked and the open question is ticked with the
  answer ("Neither"), which matches the measurement.
- Packet acceptance boxes: all backed. The one that was backed only by code reading -
  "reported at `warn` with its pid" - I confirmed directly by installing a collecting
  logger in a temporary probe:
  `WARN oneterm-pty: the shell (pid 3032) did not exit within 2s of its pseudo-console
  closing; terminating it`. Note that tests install no logger, so the packet's own runs
  could not have seen this line.
- Decision index: there is none to update. `docs/README.md:74` links the `decisions/`
  directory as a whole; no file enumerates decision records (DEC-0015 is not listed
  anywhere either).

## 8. `pwsh scripts/ci-local.ps1`

Run from this worktree at a3f180e, `CARGO_BUILD_JOBS=4`. Exit 0, `ci-local: all checks
passed.` All ten sections ran:

```
==> cargo fmt --all -- --check                      ok
==> cargo clippy --workspace --all-targets -D warnings  ok
==> cargo test --workspace                          ok
==> cargo test -p oneterm-vt --features vt-paranoid ok
==> python scripts/verify-dependency-graph.py       Dependency graph policy passed for 21 workspace packages and 21 explicit members.
==> python scripts/check-doc-paths.py               Doc path check passed for 120 current paths in 10 documents.
==> python -m unittest scripts/test_check_english.py  Ran 2 tests, OK
==> python scripts/check-english.py                 English contributor-text check passed for 771 files.
==> python scripts/completion-catalog.py validate   all catalogs valid
==> python scripts/third-party-notices.py --check   THIRD-PARTY-NOTICES.md is up to date.
```

Totals across all suites: **60 suites, 1933 passed, 0 failed, 14 ignored** - identical to
the implementer's reported numbers. `oneterm-local-shell` alone: 33 passed, 0 failed,
2 ignored, 10.21 s, both orphan tests green.

## 8b. `orphan_liveness_table` (with the fix in place)

`cargo test -p oneterm-local-shell --lib -- --ignored --nocapture orphan_liveness_table`,
two runs. Exit code `0x1` is OneTerm's own `TerminateProcess(handle, 1)`; `0xc000013a` is
the console host's exit request processed normally.

Run A (head) and run B (tail), merged - every row, no row omitted for a delay that is
shown:

| drop delay | run | process | pid | exited after | exit code | probe had to kill |
| --- | --- | --- | --- | --- | --- | --- |
| 0 ms | 1 | conhost.exe / cmd.exe | 6664 / 2536 | 2032 ms | 0x0 / 0x1 | no |
| 0 ms | 2 | conhost.exe / cmd.exe | 12720 / 5552 | 2031 ms | 0x0 / 0x1 | no |
| 0 ms | 3 | conhost.exe / cmd.exe | 3628 / 12060 | 2011 ms | 0x0 / 0x1 | no |
| 0 ms | 4 | conhost.exe / cmd.exe | 1996 / 10088 | 2022 ms | 0x0 / 0x1 | no |
| 0 ms | 5 | conhost.exe / cmd.exe | 10624 / 14876 | 2012 ms | 0x0 / 0x1 | no |
| 5 ms | 1 | conhost.exe / cmd.exe | 5304 / 15380 | 2017 ms | 0x0 / 0x1 | no |
| 5 ms | 2 | conhost.exe / cmd.exe | 16276 / 10216 | 20 ms | 0x0 / 0xc000013a | no |
| 5 ms | 3 | conhost.exe / cmd.exe | 1112 / ... | 2017 ms | 0x0 / 0x1 | no |
| 10 ms | 1-5 | conhost.exe / cmd.exe | see run B | 20 ms | 0x0 / 0xc000013a | no |
| 50 ms | 1-5 | conhost.exe / cmd.exe | see run B | 20 ms | 0x0 / 0xc000013a | no |
| 500 ms | 1-5 | conhost.exe / cmd.exe | see run B | 20 ms | 0x0 / 0xc000013a | no |
| first output | 1-5 | conhost.exe / cmd.exe | see run B | 20 ms | 0x0 / 0xc000013a | no |

Run B rows at 0 ms (pids): conhost 10952 / cmd 15648, 5448 / 15168, 16288 / 13336, all
2005-2026 ms, cmd code `0x1`. Run B at 5 ms: runs 1, 2, 4 exited at 20 ms with
`0xc000013a`; runs 3 and 5 were terminated at ~2029 ms with `0x1` (pids 9476 / 10180 and
1800 / 7464). Every 10 ms and later row in both runs: 20 ms, `0xc000013a`.

Reading: the escalation fires on every 0 ms close and on roughly half the 5 ms closes,
which is exactly the window the packet's pre-fix table said never exits. The console host
follows its terminated client within ~20 ms in every row, so "no console host behind"
holds. The `terminated` column is "no" in every row of both runs: the probe itself never
had to kill anything, i.e. the fix left no orphan for it to clean up.

Every pid listed in this section was created by the probe's own spawn as a child of the
test process. None was terminated by the probe; the ones that ended with code `0x1` were
terminated by the code under review, through the handle `CreateProcessW` returned. No
process was enumerated by name. The owner's own `oneterm.exe` and its shells were never
listed or touched.

## 9. Commit trailers

Both commits carry, verbatim:

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

`a3f180e` and `b507716`, plus `Refs: BUG-0055, IN-0031` on both. Correct.

## Defects

### D1 - Medium - the close guarantee is written wider than the code delivers

`docs/terminal-backend.md:462` states, without qualification: "**Closing a local
session** is guaranteed to leave no process behind", and
`DEC-0016` ("## Decision", last paragraph) instructs: "Future work that touches
local-session teardown must keep this guarantee: a discarded session leaves no shell
process and no console host behind."

The guarantee is only served while the application process is alive. The grace period is
waited out on a detached owner thread, so when OneTerm exits (window close -> `cx.quit()`,
`crates/app/src/window.rs:72`) or is killed, every owner thread is destroyed with the
process and no escalation runs - which is the second half of the observation the packet
quotes in its own Context ("in one launch two `cmd.exe` remained after the app pid had
been terminated"). The packet records this correctly under Gaps; the two durable
documents do not, and they are what the next agent reads.

Repro: none needed - it follows from the detached reaper. My drop-timing probe had to
sleep 4 s before returning for exactly this reason, or the test binary would have exited
before its own owner threads finished the grace period.

Fix: scope the sentence ("while OneTerm is running") in both places, and let DEC-0016's
Consequences name the app-exit hole the packet already names.

### D2 - Medium - the orphan probe can adopt, fail on, and kill another test's shell

`crates/local-shell/src/session_orphan_tests.rs:145-152`: `probe()` snapshots every child
of the **test process**, spawns its session, snapshots again, and treats every new child
as its own. `cargo test -p oneterm-local-shell` runs this in parallel with ~14 other
tests that spawn real shells through the same `spawn_default()` helper (plus the two
PowerShell prompt tests, which hold a session alive for up to 15 s). Any shell started by
another test inside the probe's snapshot window is adopted, waited on for
`LIVENESS_BOUND` (5 s), reported as an orphan (`assert!(!row.terminated)`) and
**terminated** - failing this test and, very likely, the innocent one as well.

Measured window, same machine: the ToolHelp scan takes 3.3 ms and
`PseudoConsole::spawn` 7.5-13.4 ms (`LocalSession::spawn` adds a thread and a terminal on
top), so each probe leaves roughly 15-25 ms open, six times per full run. The CI log
confirms the tests really are concurrent: `a_session_dropped_*` finish in the same block
as `pwsh_prompt_emits_cwd_without_parser_errors`.

It did not fire in my run or in the implementer's, and it cannot produce a false *pass*.
It is a latent flake with collateral damage inside the test binary only - the owner's own
processes are never at risk, since everything is scoped to children of the test process.

Fix (cheapest): have `spawn_default()` and the probe take one shared
`static SPAWN: Mutex<()>` across the spawn plus the two snapshots, so no other test's
spawn can land inside the window. Alternatively narrow the probe to the session's own
child pid, which would mean exposing it from `LocalSession`.

### D3 - Low - one spawn-failure path does make the UI thread wait out the grace period

`crates/local-shell/src/event_loop.rs:194-207`: inside `spawn_owned`'s closure, if
`pty.child_pid()` is `None` or `ShellEventLoop::new` fails, the `?` drops the
`PseudoConsole` **before** `ready_tx.send(Err(..))` runs, while the caller
(`LocalSession::spawn`, on the UI thread) is parked in `ready_rx.recv()`. The caller
therefore waits for the full grace period on top of the failure it is already waiting
for. DEC-0016 states flatly "No UI thread ever waits out the grace period"; that is true
of every path the app takes in practice, but not of this one.

Both triggers are close to unreachable (`GetProcessId` returning 0; `Poller::new`
failing), so this is a wording defect more than a runtime one. If it is worth fixing at
all, the fix is to send the error before dropping the PTY.

### D4 - Low - the regression test cannot tell "fixed" from "did not reproduce"

`a_session_dropped_before_its_shell_starts_leaves_no_orphan` only asserts that nothing
survived 5 s. On hardware where the 0 ms drop always lands after `cmd.exe` has finished
initialising, it passes without ever exercising the escalation, and would keep passing if
the escalation were deleted. The tamper run proves it bites *here*; nothing proves it
bites on a different machine or a future Windows build. Consider asserting that at least
one of the five runs took longer than, say, 1 s to clear (i.e. went through the grace
period), or record in the test's doc comment that a green run is not proof the path ran.

### D5 - Nit - DEC-0016 attributes a stricter rule to DEC-0005 than DEC-0005 contains

DEC-0016: "Never a process matched by name, and never a pid this process did not create -
the same rule `DEC-0005` set for update-time console cleanup." DEC-0005 sets the
never-by-name half, but it deliberately does terminate processes it did not create (it
rejected the "walk the process tree to kill only direct children" alternative and scopes
by install-directory image path instead). The "only its own child" half is new in
DEC-0016 and is stricter, which is the right call - it just is not inherited. Also worth
knowing when citing: there are two DEC-0005 files in `docs/decisions/`, and the relevant
one is `DEC-0005-terminate-only-oneterm-s-own.md`.

### D6 - Nit - a type named `ChildExitWatcher` now kills the child

`crates/pty/src/windows/child.rs:93-100`: the doc comment on the `process` field still
reads "Kept alive for the callback; closed after `UnregisterWaitEx`", and
`crates/pty/src/lib.rs:3-5` still describes the crate as "a **passive pollable object**".
Dropping this watcher is now an action with an external side effect. The method itself is
well documented; the surrounding descriptions are not, and a future reader who trusts
them will be surprised.

## Gaps in this verification

- No GUI or E2E run: this worktree deliberately does not launch `oneterm-app`, and the
  owner runs Claude Code inside their own `oneterm.exe`.
- The bundled `OpenConsole.exe` (DEC-0013) is untested here, for the same reason the
  packet gives: `ConptyApi::resolve` looks for `conpty.dll` beside the running
  executable, and a test binary has none. Every measurement above is the system console
  host.
- The app-exit hole (D1) was reasoned from the code and from my probe needing a sleep,
  not reproduced against the real application.

---

# Re-verification of 5d32b1c

Same verifier, same worktree, reset to `5d32b1c` (one commit on `a3f180e`).
Diff re-reviewed: `a3f180e..5d32b1c`, 9 files, 202 insertions.

## Verdict: PASS

All six notes are closed, and closed the way the notes asked. Nothing new was
introduced that I can find. The one change that went past the note - D3 became a
production change in `crates/local-shell` - is correct, and it closes the hole more
completely than the note suggested.

Three residual nits are listed at the end. None is worth a commit on its own.

## D2 - `SPAWN_GUARD`: complete, and it cannot deadlock

Coverage. There is exactly one `LocalSession::spawn` call in the test binary
(`session_tests.rs:85`, inside `spawn_unguarded`), and exactly one other
process-spawning call anywhere in `crates/local-shell`
(`event_loop.rs:200`, `PseudoConsole::spawn`, which is only reached through that same
path). Its callers:

| Caller | Takes the lock? |
| --- | --- |
| `spawn_guarded` (`session_tests.rs:76`) | yes, `lock_spawns()` |
| `spawn_default` -> `spawn_guarded` (20 call sites) | yes |
| `assert_powershell_prompt_emits_cwd` -> `spawn_guarded` (`:104`) | yes |
| `spawn_failure_is_a_typed_shell_resolution_error` -> `spawn_guarded` (`:202`) | yes |
| `probe` (`session_orphan_tests.rs:163`) | holds it already, across both snapshots |

`event_loop_tests.rs` spawns no process at all (loopback transport), so the binary has
no unguarded spawn left. Coverage is complete.

Deadlock. The guard is a single, non-reentrant `Mutex<()>` with no other lock in its
ordering. The probe holds it across `children_of` -> `spawn_unguarded` -> `children_of`
and releases it (`drop(guard)`, `:169`) **before** the delay, the drop and the liveness
polling. `spawn_unguarded` blocks in `ready_rx.recv()` waiting for the PTY owner thread,
and the owner thread never takes the guard - so nothing the lock holder waits for needs
the lock. The `DropAfter::FirstOutput` wait (up to 10 s) and the 5 s liveness bound are
both outside the guarded region. A panic inside the guarded region (`.expect("spawn")`)
poisons the mutex, and `lock_spawns` recovers with `PoisonError::into_inner`. No
deadlock path exists.

Flake runs, `cargo test -p oneterm-local-shell`, three consecutive times:

```
--- run 1 --- test result: ok. 33 passed; 0 failed; 2 ignored; finished in 10.29s
--- run 2 --- test result: ok. 33 passed; 0 failed; 2 ignored; finished in 10.31s
--- run 3 --- test result: ok. 33 passed; 0 failed; 2 ignored; finished in 10.31s
```

Against 10.21 s for the same suite before the guard: serialising the spawns costs
nothing measurable, because only the spawn call is serialised, not the waits that
dominate the suite.

Residual (theoretical, pre-existing): the probe still identifies its processes by pid
diff, so a recycled pid that happened to match an entry in `before` would hide one of
its own processes. Windows does not reuse a pid that fast in practice.

## D3 - `Poller` moved to the caller thread; `ShellEventLoop::new` infallible

This went beyond the note, which only asked for the report-before-drop ordering. The
larger change is correct:

- **Is `Poller` `Send`/`Sync`?** It must be, and the build proves it: the `Arc<Poller>`
  is captured by a `'static` closure handed to `thread::Builder::spawn`, which requires
  `Send`, and `clippy --workspace --all-targets -- -D warnings` passes. It was already
  crossing threads before this change - `ShellNotifier` holds an `Arc<Poller>` and is
  cloned onto the UI thread, which calls `poller.notify()` on every keystroke.
- **Handle affinity?** None. The Windows backend is an IOCP; creating the port on one
  thread and waiting on it from another is the model IOCP is built for, and no
  `Poller` state is thread-local. `crates/tools/src/bin/pty-throughput.rs:49` already
  created one on `main` and used it there; `event_loop_tests.rs:24` already created one
  on the test thread for a loop that runs elsewhere.
- **Can a live `PseudoConsole` still drop while `LocalSession::spawn` waits?** No. After
  `PseudoConsole::spawn` returns `Ok`, the steps before `ready_tx.send(Ok(..))` are:
  `child_pid()` (a query), `set_identity`, `logging().start()` (fallible, but only
  `warn`s - `event_loop.rs:222-227`), the now-infallible `Self::new`, and
  `set_notifier`. The single early return left, `child_pid() == None`
  (`event_loop.rs:210-218`), sends the error **first** and only then falls out of the
  block where `pty` drops - the comment says exactly that, and the code matches.
  `Poller::new()?` now fails before any pseudo-console exists, on the caller's own
  thread. So DEC-0016's "no UI thread waits out the grace period" is now literally true
  for every non-panicking path.
- **Call sites.** `ShellEventLoop::new` has two: `spawn_owned` and
  `event_loop_tests.rs:379-380`, which was updated to build its own poller. No other
  consumer exists.

Two things this change also does, neither harmful: a `Poller` is now created on the UI
thread (an IOCP handle, microseconds) and is wasted when the shell fails to start; and a
`Poller::new` failure is reported as `AppError::ShellResolution`, which is the
pre-existing mapping `LocalSession::spawn` applies to any `io::Error` out of
`spawn_owned`.

## D4 - the exit-code assertion, and the tamper

`EXPECTED_EXIT_CODES = [0x0, 0x1, 0xc000_013a, 0xc000_0142]` is the right set, and every
member of it was observed in my own liveness tables at `a3f180e`: `0x0` on every
`conhost.exe` row, `0xc000013a` on every normal-path `cmd.exe` row, `0x1` on every
escalated row (it is `TerminateProcess(handle, 1)` and nothing else in this system
produces it), and `0xc0000142` in the packet's own 0 ms diagnosis run. The assertion is
set membership, so it still does not *require* that the escalation ran - which is now
stated plainly in the test's doc comment, together with the vacuous-pass condition. That
is the honest resolution of D4; requiring a `0x1` would make the test flaky in the other
direction on fast hardware.

Tamper re-run at `5d32b1c` (`self.terminate_if_still_running();` commented out,
`crates/pty/src/windows/child.rs:226`, nothing else changed):

```
thread 'session::session_orphan_tests::a_session_dropped_before_its_shell_starts_leaves_no_orphan'
panicked at crates\local-shell\src\session_orphan_tests.rs:240:13:
run 1: dropping a session after 0 ms left conhost.exe (pid 13452) alive for 5s;
the probe had to terminate it
test result: FAILED. 0 passed; 1 failed; 34 filtered out; finished in 5.04s
```

Restored with `git checkout --`; the tree is back at `5d32b1c` exactly. The test still
bites on run 1. Processes: the probe spawned and then terminated pid 13452 and its
client, both children of the test process, both by handle. Nothing else was touched.

## D1, D5, D6 - documentation wording

Checked against the code, line by line:

- `docs/terminal-backend.md:462-466`: the guarantee now carries "**while OneTerm is
  running**" and names the detached-thread reason and the Gaps entry. Accurate.
- `docs/terminal-backend.md:497-504`: the DEC-0005 / DEC-0016 split is stated correctly
  (never-by-name inherited, own-child-only new and stricter), and a new bullet records
  the busy-shell measurement including the detached-grandchild exception. Matches what I
  measured.
- `DEC-0016` "Decision": the `Poller` / infallible-`new` sentence matches the code as
  reviewed above; the scoping bullet now separates the inherited rule from the new one
  and cites `DEC-0005-terminate-only-oneterm-s-own.md` by filename, which resolves the
  two-files ambiguity; the guarantee sentence carries the qualifier and points at
  Consequences.
- `DEC-0016` "Consequences": the new limit bullet states the app-exit hole and what
  closing it would cost. Status is still **Proposed**, correctly - the owner has not
  accepted it.
- `crates/pty/src/windows/child.rs:1-7` and `:104-107`, `crates/pty/src/lib.rs:9-15`:
  both now say that dropping is an action with an external side effect, and the crate
  doc says the drop blocks and belongs on an owner thread. D6 closed.
- `BUG-0055` packet: the verification table records all six notes and their resolutions
  accurately, including the two measurements I contributed, and Gaps gains the
  vacuous-pass entry. It describes the verification as PASS WITH NOTES, which is what it
  was.

## Trailer on 5d32b1c

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

Present, verbatim, plus `Refs: BUG-0055, IN-0031`. Correct.

## `pwsh scripts/ci-local.ps1` at 5d32b1c

`CARGO_BUILD_JOBS=4`, from this worktree. Exit 0, `ci-local: all checks passed.`, all ten
sections. Totals: **60 suites, 1933 passed, 0 failed, 14 ignored** - unchanged from
`a3f180e`, as expected, since the rework adds no test. `verify-dependency-graph.py`
passed for 21 packages; `check-english.py` passed for 772 files (771 plus this evidence
file).

## Residual nits at 5d32b1c

- N1. `session_orphan_tests.rs:248`: `row.exit_code.expect("an exited process must have
  an exit code")` turns a `GetExitCodeProcess` failure into a panic whose message hides
  what actually happened. The handle carries `PROCESS_QUERY_LIMITED_INFORMATION`, so this
  is close to unreachable; an `unwrap_or` plus the existing assertion message would read
  better if the file is touched again.
- N2. The tightened exit-code set is a new, small flake surface: a future Windows build
  that ends a console host with some other status would fail the test rather than the
  code. That is the intended trade and is worth keeping, but it should be recognised as
  a test that now asserts an OS detail.
- N3. A panic between `PseudoConsole::spawn` and `ready_tx.send` (for example an
  allocation failure inside `ShellEventLoop::new`) would still drop a live console while
  the caller is parked in `ready_rx.recv()`. Panic paths are out of scope for DEC-0016,
  and the claim in it is about failure paths, not panics; noted only so a future reader
  does not read "literally true" as "true under panic".
