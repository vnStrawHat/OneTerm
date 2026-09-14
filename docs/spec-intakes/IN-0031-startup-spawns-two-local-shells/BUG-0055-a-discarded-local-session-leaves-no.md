# Work: A discarded local session leaves no orphan shell

ID: BUG-0055
Intake: IN-0031
Created: 2026-09-14

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

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: `IN-0031`

## Outcome

A local session that is discarded — pruned from the dock, closed by the user, or dropped while its
shell is still starting — leaves no `cmd.exe` and no `OpenConsole.exe` behind. When the child
cannot be shut down through the pseudo-console, the session says so in the log instead of leaking
silently.

This is the same symptom as `BUG-0054` seen from the other end. `BUG-0054` removes the spawn that
should not happen; this packet is about the shell that survives when a session goes away for any
reason. Fixing the first makes the second rare at startup, not impossible: closing a tab, a
failed spawn, and a window closed during startup all reach the same teardown.

## Scope

- [x] In scope: the teardown path — `LocalSession::shutdown_owner` / `Drop`
  (`crates/local-shell/src/session.rs`), `LocalTransport::pty_close` and the PTY owner loop's
  response to it (`crates/local-shell/src/{transport,event_loop}.rs`), and `Conpty::drop` /
  `PseudoConsole` field ordering (`crates/pty/src/windows/{conpty,mod}.rs`).
- [x] Out of scope:
  - The double spawn itself — `BUG-0054`.
  - The SSH session teardown (`crates/ssh`): a remote shell has no local process to orphan.
  - Reaping shells left by a previous run or by a crash of the app itself. A process-tree kill at
    startup is a different outcome with a different risk profile, and it is not this packet.
  - Changing what `ClosePseudoConsole` does or bypassing it.

## Acceptance

- [x] Root cause named in this packet, with the observation that proves it, before any code
  changes. If the shell turns out to exit correctly and the probe was wrong, that is a valid
  outcome: record it and retire the packet.
- [x] A session dropped immediately after spawn — before its shell has finished starting — leaves
  no `cmd.exe` and no `OpenConsole.exe` within a bounded wait.
- [x] A session dropped after its shell is fully up leaves neither process either (the case that
  already works today; pinned so the fix cannot regress it).
- [x] A child that does not exit within the bounded wait is reported at `warn` with its pid, so a
  future leak is diagnosable from `~/.OneTerm/logs` instead of Task Manager.
- [x] `cargo test -p oneterm-local-shell` and `cargo test -p oneterm-pty` green; the existing
  30-test local-shell baseline does not shrink.
- [x] `pwsh scripts/ci-local.ps1` exits 0.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` §6.2 (spawn via `oneterm-pty`) and §6.3 (Windows-specific) — the
  owner-thread model and what closing a session is specified to do.
- `docs/agents/error-policy.md` — the transport row, which is why a pipe-thread spawn failure is
  reported rather than logged; the same policy decides whether a child that will not die is a
  `warn` or an error.
- `crates/pty/src/windows.rs` — the `PseudoConsole` field-order invariant: `conpty` is first so
  `ClosePseudoConsole` runs while `conout` still exists, "Reordering those fields deadlocks on
  close."
- `crates/local-shell/src/session.rs` — `shutdown_owner`, the detached reaper thread (`CORR-10`:
  joining on the UI thread can deadlock with a pump waiting for the UI to drain), and `Drop`.
- `crates/pty/src/windows/child.rs` — `ChildExitWatcher`, which owns the process handle and is
  what a bounded wait would have to consult.

### Documentation Action

**Branch selected by the diagnosis: the orphan is a missing step in teardown, so the update is
required.** `docs/terminal-backend.md` §6.2 now states what closing a local session guarantees,
and §6.3 carries the Windows mechanism, the bound and the escalation.

`DEC-0016` records the escalation itself: terminating a child process is a side effect on the
user's machine, which the Decisions section below said would need a decision record before it
landed.

Reason: `docs/terminal-backend.md` describes the spawn and the owner thread in detail but does not
state a guarantee about the child on close, so the correct documentation action depends on which
guarantee turns out to be true. Recording a documentation decision before the diagnosis would be
guessing.

### Reconciliation

- `docs/terminal-backend.md` §6.2 — new paragraph: closing a local session leaves no process
  behind, and where the teardown runs.
- `docs/terminal-backend.md` §6.3 — new **Close** bullet: the measurement, `CHILD_EXIT_GRACE`,
  the `warn`, the `DEC-0005` scoping and the owner-thread placement.
- `docs/decisions/DEC-0016-terminate-a-shell-that-outlives-its-pseudo-console.md` — new,
  status `Proposed` pending the owner's confirmation.
- `docs/agents/error-policy.md` — reviewed, no change: the escalation is a best-effort cleanup
  inside a `Drop` with no caller to return a typed error to, which is the "log at `warn` and
  continue" row, and the message carries the operation and the pid as the review rules require.
- `crates/pty/src/windows.rs` — reviewed, no change: the `PseudoConsole` field-order invariant
  is what makes `ChildExitWatcher::drop` the correct place for the wait, and the order is
  untouched.
- `docs/agents/crate-dependency-rules.md` — reviewed, no change: the new `windows-sys` entry in
  `oneterm-local-shell` is a **dev**-dependency, so no crate-graph edge changes and R1–R12 are
  unaffected.

## Context

Observed while probing `IN-0031` on the packaged 0.5.2 binary: in several launches **both**
`cmd.exe` processes were still alive seconds after `reconcile` had pruned the panel owning one of
them, and in one launch two `cmd.exe` remained after the app pid had been terminated. In other
launches one of the pair was already gone. So the teardown is at best racy.

What is known about the intended path:

- `LocalSession::drop` calls `self.transport().pty_close()`, then hands the owner thread's join
  handle to a detached reaper. Neither step waits for the child.
- `Conpty::drop` calls `ClosePseudoConsole`, which is documented to end the session and blocks
  until the conout pipe drains — hence the field ordering.
- `ChildExitWatcher` owns the process handle but is dropped as part of `PseudoConsole` teardown;
  nothing consults it during close.

Two plausible mechanisms, not yet distinguished:

1. `ClosePseudoConsole` signals the host, the host asks its client to exit, and a `cmd.exe` that
   has not finished initialising never processes that request — so the pseudo-console goes away
   and the client stays. This is consistent with the 0xc0000142 dialog: the client is alive
   enough to fault, which means it is alive enough to linger.
2. Teardown returns before the host has actually reaped the client, and the survivors seen were
   simply mid-exit. This is the "no defect" branch and must be ruled out by measurement, not by
   assumption.

The first observation to make is therefore the cheapest one: spawn a session, drop it at several
delays after spawn (0 ms, 50 ms, 500 ms, after first output), and record whether the child pid is
still alive at increasing waits. That separates "never dies" from "dies late" without touching any
code.

### Diagnosis: mechanism 1 holds

Measured with `crates/local-shell/src/session_orphan_tests.rs::orphan_liveness_table` on
Windows 11 26200, `HEAD` (`feat/vt-engine`), the shipped default shell
`cmd.exe /K chcp 65001 >nul`, no production edit. The probe snapshots the processes the
spawn adds as children of the test process, opens a handle to each before the drop so the
pid cannot be recycled, drops the session at the delay, and polls
`WaitForSingleObject(handle, 0)` every 20 ms up to the bound.

First sweep, bound 5 s, five runs per delay (40 rows):

| drop delay | runs | `cmd.exe` | `conhost.exe` |
| --- | --- | --- | --- |
| 0 ms | 5 | alive at 5 s in **5 of 5** | alive at 5 s in 5 of 5 |
| 50 ms | 5 | exited at 20 ms, `0xc000013a` | exited at 20 ms, `0x0` |
| 500 ms | 5 | exited at 20–21 ms, `0xc000013a` | exited at 20–21 ms, `0x0` |
| after first output | 5 | exited at 20 ms, `0xc000013a` | exited at 20 ms, `0x0` |

Second sweep, bound raised to **15 s** to separate "never" from "late", three runs per
delay (36 rows):

| drop delay | runs | `cmd.exe` outcome |
| --- | --- | --- |
| 0 ms | 3 | alive at 15 s in 2; 1 exited at 20 ms with **`0xc0000142`** |
| 2 ms | 3 | alive at 15 s in 3 of 3 |
| 5 ms | 3 | alive at 15 s in 3 of 3 |
| 10 ms | 3 | exited at 20 ms, `0xc000013a` |
| 20 ms | 3 | exited at 20 ms, `0xc000013a` |
| 50 ms | 3 | exited at 20 ms, `0xc000013a` |

`conhost.exe` followed its client in every single row: it exited when the client exited and
survived when the client survived, which is expected — a console host exits with its last
client. The console host is therefore a consequence of the orphan, not a second defect.

**Mechanism 1 holds; mechanism 2 is ruled out.** A survivor is still alive 15 s after the
drop, which is 750 times the 20 ms the normal path takes, so it is not "mid-exit". `0xc000013a`
is `STATUS_CONTROL_C_EXIT` — the host's exit request, processed normally. `0xc0000142` is
`STATUS_DLL_INIT_FAILED`, the code in the owner's `IN-0031` screenshot, observed here in one
0 ms run: the same race has three outcomes — the client faults (and Windows raises the dialog),
or it exits cleanly, or it never processes the request at all and stays forever. The vulnerable
window closes between 5 ms and 10 ms after spawn on this machine.

Two limits of this measurement, both recorded rather than resolved:

- A `#32770` dialog is owned by `csrss.exe`, not by the client, so the probe cannot see one.
  That the fault code appears in the exit status is the strongest link to the dialog available
  from a test.
- `ConptyApi::resolve` looks for `conpty.dll` next to the running executable, and a test binary
  has none, so every row above is the **system** console host. The bundled
  `OpenConsole.exe` (`DEC-0013`) is untested here; see `DEC-0016`'s follow-up.

Constraint to respect: whatever the fix is, it must not join the owner thread on the UI thread —
`CORR-10` and the detached reaper exist for that reason — and must not reorder `PseudoConsole`'s
fields.

## Plan

- [x] Diagnose first, with no production edit: a focused test in `crates/local-shell` that spawns
  a real `cmd.exe` session, drops it at a parameterised delay, and polls the child pid for
  liveness up to a bound. Record the table.
- [x] From that table, decide which mechanism holds and write it into this packet's Context.
- [x] Only then choose the fix. If a bounded wait plus an escalation is needed, it belongs off the
  UI thread (the existing reaper thread is the natural owner) and must log the pid it gave up on.
- [x] Re-run the diagnosis test as the regression proof, plus `-p oneterm-local-shell`,
  `-p oneterm-pty`, and `cargo test --workspace`.
- [x] Update `docs/terminal-backend.md` per whichever documentation branch the diagnosis selects.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

`DEC-0016` — **Terminate a shell that outlives its pseudo-console.** The diagnosis showed the
survivor never exits, so the fix does escalate to `TerminateProcess`, which is the side effect
this section said would need a record before it lands. The decision fixes the bound (2 s), the
owner of the wait (`ChildExitWatcher::drop`, on the PTY owner thread), the `warn` with the pid,
and the scoping rule inherited from `DEC-0005`: only this process's own child, only through the
handle it holds for it, never a name match. Its status is `Proposed` until the owner confirms
terminating a user's shell process is acceptable.

## Verification Plan

Focused proof:

- The liveness table above, kept as a regression test at the delay that reproduced the orphan.
  It must fail before the fix; if no delay reproduces it, the packet retires instead.
- A session dropped after its shell is fully up still leaves nothing behind.

Regression:

- `cargo test -p oneterm-local-shell` (baseline 30 tests, plus this packet's) and
  `cargo test -p oneterm-pty`.
- `cargo test --workspace`, then `pwsh scripts/ci-local.ps1`.

E2E, Windows with a real desktop:

- With `BUG-0054` in place, open and close terminal tabs repeatedly and confirm no
  `cmd.exe /K chcp 65001 >nul` accumulates; then close the app and confirm none survives.

Cleanup obligation for every probe and test in this packet: kill only processes it started, by pid.
The owner's own running `oneterm.exe` and its shells must never be enumerated for termination —
earlier packets in this repository recorded the same constraint.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Implemented. The diagnosis in Context ruled mechanism 1 in and mechanism 2 out, so the packet
took the "missing step in teardown" branch rather than retiring.

**Fix.** `ChildExitWatcher::drop` (`crates/pty/src/windows/child.rs`) now waits
`CHILD_EXIT_GRACE` (2 s) on the process handle it already owns and terminates the child if it
is still running, logging the pid at `warn` first. The watcher is the last field of
`PseudoConsole` and the pseudo-console is the first, so the wait runs after
`ClosePseudoConsole` and before the handle is closed — no field was reordered. The watcher
drops on the "PTY owner" thread, which `LocalSession::shutdown_owner` hands to a detached
reaper, so no UI thread waits out the grace period (`CORR-10`). `DEC-0016` carries the
rationale and the bound. Nothing in `crates/local-shell` needed to change: its teardown already
delivered `ShellMsg::Shutdown` reliably and the loop already returned on it.

**Independent verification** (separate agent, `2f47366..a3f180e`): PASS WITH NOTES — the
tamper bites, `ci-local` totals reproduce exactly, and the `warn` line was captured verbatim
with its pid by a probe that installed a collecting logger (the tests install none, so this
packet's own runs could not have seen it). Its six notes are addressed in the follow-up
commit:

| # | Note | Resolution |
| --- | --- | --- |
| D2 | The probe adopted **every** new child of the test process, so a sibling test's shell starting inside its 15–25 ms snapshot window would be waited out, reported as an orphan and terminated — a latent CI flake with collateral damage inside the test binary. | `session_tests::SPAWN_GUARD`: every real-shell spawn in the binary takes one lock, and the probe holds it across both snapshots. |
| D1 | `docs/terminal-backend.md` §6.2 and `DEC-0016` stated the close guarantee unconditionally; it holds only while the app runs, because the grace is served on a detached thread. | Both scoped to "while OneTerm is running"; `DEC-0016` Consequences now names the app-exit hole this packet's Gaps already recorded. |
| D3 | On the `child_pid() == None` and `Poller::new` failure paths the pseudo-console dropped **before** `ready_tx.send(Err(..))`, so `LocalSession::spawn` on the UI thread could wait out the grace on top of the failure. | `Poller::new` moved ahead of the pseudo-console and out of the owner thread; `ShellEventLoop::new` is now infallible; the remaining failure path reports before the PTY drops. `DEC-0016`'s "no UI thread waits out the grace" is now literally true. |
| D4 | The regression test asserted only "nothing survived", which a run that never reproduced the orphan also satisfies. | Exit codes are now asserted against `{0x0, 0x1, 0xc000013a, 0xc0000142}`, and the vacuous-pass risk is written into the test's doc comment: `0x1` is the escalation, and a run without one proves only that the orphan did not occur. The ignored `orphan_liveness_table` remains the manual proof. |
| D5 | `DEC-0016` credited `DEC-0005` with the "own child only" rule, which `DEC-0005` explicitly rejected; there are also two `DEC-0005` files. | Split: never-by-name is inherited from `DEC-0005-terminate-only-oneterm-s-own.md` by filename, own-child-only is named as new and stricter. Same correction in `terminal-backend.md` §6.3 and in `child.rs`. |
| D6 | `ChildExitWatcher` and the `oneterm-pty` crate doc still described a purely passive object. | Both updated: dropping one is an action with an external side effect. |

The verifier also measured what the packet had not: the caller thread blocks **321 µs** for
three 0 ms drops and 142 µs for three started ones, so the 2 s is paid entirely by the
detached owner thread; and a **busy** shell (`ping -t`, `timeout /t 30`) plus its foreground
grandchild both exit with `0xc000013a` ~20 ms after the close, before and after the fix — the
escalation never touches them. Both facts are now in `terminal-backend.md` §6.3 and
`DEC-0016`.

**Regression proof** (`crates/local-shell/src/session_orphan_tests.rs`, Windows only):

| Test | Before the fix | After |
| --- | --- | --- |
| `a_session_dropped_before_its_shell_starts_leaves_no_orphan` (0 ms, 5 runs) | FAILED on run 1 — `conhost.exe` pid 18352 alive for 5 s | ok |
| `a_session_dropped_after_its_shell_is_up_leaves_no_orphan` (after first output) | ok | ok |

The "before" column is a real run with the one call in `ChildExitWatcher::drop` commented out
and nothing else changed. Five repetitions at 0 ms is what makes the first test deterministic:
one run in nine survived the race on its own in the diagnosis sweep, so a single-run test could
pass against the defect.

**Test counts.** `cargo test -p oneterm-local-shell`: 33 passed, 2 ignored (baseline 31 passed
+ 1 ignored; the packet's "30 tests" does not shrink). `cargo test -p oneterm-pty`: 24 passed.
`pwsh scripts/ci-local.ps1`: exit 0, all ten sections, 60 suites, **1933 passed, 0 failed,
14 ignored**.

**Processes.** Every process this packet started was a child of its own test process, opened by
handle before the drop. Across the two sweeps and the pre-fix proof run, **14** orphan pairs had
to be terminated by the probe — a `cmd.exe` plus the `conhost.exe` serving it, 28 processes:
5 pairs in the 5 s sweep (0 ms, runs 1–5), 8 in the 15 s sweep (0 ms runs 1 and 3, 2 ms runs
1–3, 5 ms runs 1–3) and 1 in the pre-fix proof run (0 ms, run 1, `conhost.exe` 18352). Each was
terminated through the handle the probe opened for that pid. No process was ever
matched by name, and no pid the probe did not observe being created was touched. The owner's
own `oneterm.exe` and its shells were never enumerated.

Gaps:

- The bundled `OpenConsole.exe` (`DEC-0013`) is not covered. `ConptyApi::resolve` looks for
  `conpty.dll` beside the running executable and a test binary has none, so every row above is
  the system console host. The fix is host-independent (it acts on the client handle), but the
  timing table is not.
- The `#32770` dialog cannot be observed from a test: it belongs to `csrss.exe`. The link to it
  is the `0xc0000142` exit status seen in one 0 ms run, which is the code from the owner's
  screenshot.
- No GUI E2E: the app-level walk belongs to `BUG-0054`, and this worktree deliberately does not
  build `oneterm-app`. The E2E proof box is therefore unticked.
- A child whose watcher fails to be created at all (`ChildExitWatcher::new` returning an error
  inside `conpty::spawn`) still leaks — a pre-existing path on a failure that has not been
  observed, and out of this packet's scope.
- The regression test can pass vacuously on hardware where `cmd.exe` finishes initialising
  before the spawn path returns: it would then never exercise the escalation. The exit-code
  assertion narrows this (a `0x1` proves the path ran) but cannot require one without becoming
  flaky in the other direction. The ignored `orphan_liveness_table` is the manual proof.
- App exit is not covered: the reaper thread is detached, so a shell that outlives its
  pseudo-console during process shutdown can still escape the grace period. `BUG-0054` removes
  the startup case that produced it.

## Handoff

`DEC-0016` is `Proposed`, not `Accepted`: terminating a user's shell process is a side effect
the owner should confirm before this merges. Everything else is done — diagnosis, fix,
regression proof, docs, `pwsh scripts/ci-local.ps1` green.
