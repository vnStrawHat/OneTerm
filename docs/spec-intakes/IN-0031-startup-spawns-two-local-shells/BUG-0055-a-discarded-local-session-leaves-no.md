# Work: A discarded local session leaves no orphan shell

ID: BUG-0055
Intake: IN-0031
Created: 2026-09-14

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

- [ ] Root cause named in this packet, with the observation that proves it, before any code
  changes. If the shell turns out to exit correctly and the probe was wrong, that is a valid
  outcome: record it and retire the packet.
- [ ] A session dropped immediately after spawn — before its shell has finished starting — leaves
  no `cmd.exe` and no `OpenConsole.exe` within a bounded wait.
- [ ] A session dropped after its shell is fully up leaves neither process either (the case that
  already works today; pinned so the fix cannot regress it).
- [ ] A child that does not exit within the bounded wait is reported at `warn` with its pid, so a
  future leak is diagnosable from `~/.OneTerm/logs` instead of Task Manager.
- [ ] `cargo test -p oneterm-local-shell` and `cargo test -p oneterm-pty` green; the existing
  30-test local-shell baseline does not shrink.
- [ ] `pwsh scripts/ci-local.ps1` exits 0.

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

To be decided when the mechanism is known; the choice belongs after the diagnosis, not before it.

- If the orphan is a missing step in teardown: update required —
  `docs/terminal-backend.md` §6.2/§6.3 must state what closing a local session guarantees about
  the child process and its console host.
- If the child is expected to exit on its own and the probe caught it mid-exit: no contract
  change, and the packet retires with the measurement recorded.

Reason: `docs/terminal-backend.md` describes the spawn and the owner thread in detail but does not
state a guarantee about the child on close, so the correct documentation action depends on which
guarantee turns out to be true. Recording a documentation decision before the diagnosis would be
guessing.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

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

Constraint to respect: whatever the fix is, it must not join the owner thread on the UI thread —
`CORR-10` and the detached reaper exist for that reason — and must not reorder `PseudoConsole`'s
fields.

## Plan

- [ ] Diagnose first, with no production edit: a focused test in `crates/local-shell` that spawns
  a real `cmd.exe` session, drops it at a parameterised delay, and polls the child pid for
  liveness up to a bound. Record the table.
- [ ] From that table, decide which mechanism holds and write it into this packet's Context.
- [ ] Only then choose the fix. If a bounded wait plus an escalation is needed, it belongs off the
  UI thread (the existing reaper thread is the natural owner) and must log the pid it gave up on.
- [ ] Re-run the diagnosis test as the regression proof, plus `-p oneterm-local-shell`,
  `-p oneterm-pty`, and `cargo test --workspace`.
- [ ] Update `docs/terminal-backend.md` per whichever documentation branch the diagnosis selects.
- [ ] `pwsh scripts/ci-local.ps1`.

## Decisions

None yet. If the fix escalates to terminating a child process that will not exit, that is a
consequential choice about a side effect on a user's machine and needs a `DEC` record before it
lands — it is not a detail this packet can absorb silently.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Not implemented, and deliberately not yet diagnosed — the packet exists so the observation is not
lost while `BUG-0054` is fixed.

Baseline observation (packaged 0.5.2, `dist/oneterm-x86_64-pc-windows-msvc/oneterm.exe`): across
eight launches with a saved center terminal, two `cmd.exe` children were sampled in five of them
and both were still alive at the end of a 6–7 s sampling window in three; in one launch two
`cmd.exe` survived termination of the app pid and had to be killed by hand.

Gaps:

- Mechanism unknown; the two candidates in Context are hypotheses, not findings.
- The sampler polled at 20–60 ms and cannot distinguish "still alive" from "exiting", which is
  exactly what the planned liveness table is for.
- Whether this reproduces on `HEAD` (newer bundled ConPTY 1.24.2607.10001 plus the `crates/pty`
  transport) is untested; the baseline is from the 0.5.2 package.

## Handoff

Next action: run the liveness diagnosis and write its table into Context before proposing any
change. Blocked on nothing. Do not implement a fix while the mechanism is still one of two
hypotheses.
