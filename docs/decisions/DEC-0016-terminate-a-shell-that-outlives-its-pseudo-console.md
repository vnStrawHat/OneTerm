# DEC-0016 Terminate a shell that outlives its pseudo-console

Date: 2026-09-14

## Status

Proposed

## Context

Closing a local session drops its `PseudoConsole`, whose first field runs
`ClosePseudoConsole`. That asks the console host to end the session, and the
host asks its client to exit. Nothing in OneTerm waits for the client.

Measured on Windows 11 with the shipped default shell
(`cmd.exe /K chcp 65001 >nul`, system console host), dropping the session at a
parameterised delay after spawn and polling the liveness of every process the
spawn added as a child of the test process (`BUG-0055`):

| drop delay | runs | `cmd.exe` outcome |
| --- | --- | --- |
| 0–5 ms | 9 | still alive after **15 s** in 8 runs; 1 run exited with `0xc0000142` |
| 10–500 ms, after first output | 21 | exited within 20 ms, code `0xc000013a` |

`0xc000013a` is `STATUS_CONTROL_C_EXIT` — the host's exit request, processed
normally. `0xc0000142` is `STATUS_DLL_INIT_FAILED`, the code in the owner's
`IN-0031` screenshot. So a client whose start-up has not finished when its
pseudo-console closes never processes the exit request, and then never exits at
all: 15 s is 750 times the normal path, which rules out "it was merely
mid-exit". Its console host (`conhost.exe`, or the bundled `OpenConsole.exe`)
outlives its client in exactly the same runs, because the host exits when its
last client does.

An orphan is not recoverable by the user through OneTerm: its pseudo-console is
gone, so nothing reads its output or writes its input, and it is invisible
outside Task Manager. One is leaked per affected close.

## Decision

When a pseudo-console has been closed and its child has not exited within a
bounded grace period, OneTerm terminates that child.

- Bound: **2 seconds**, `CHILD_EXIT_GRACE` in `crates/pty/src/windows/child.rs`.
  Two orders of magnitude above the measured 20 ms normal path, and short enough
  that a closed tab's console host does not visibly linger.
- Where: `ChildExitWatcher::drop`, which already owns the process handle
  `CreateProcessW` returned and is the last field of `PseudoConsole` — so it runs
  after `ClosePseudoConsole` and before the handle is closed. The field order of
  `PseudoConsole` is unchanged.
- Off the UI thread: the watcher drops on the "PTY owner" thread, which
  `LocalSession::shutdown_owner` hands to a detached reaper (`CORR-10`). No UI
  thread waits out the grace period. `ShellEventLoop::spawn_owned` builds its
  `Poller` before the pseudo-console exists and `ShellEventLoop::new` is
  infallible, so no spawn-failure path drops a live console while
  `LocalSession::spawn` is parked waiting for the result either.
- Reported: a child that has to be terminated is logged at `warn` with its pid
  before the call, so a future leak is diagnosable from `~/.OneTerm/logs`.
- Scope: only this process's own child, only through the handle this process
  holds for it. Two separate rules, and only one of them is inherited: never
  matching a process by name comes from
  `DEC-0005-terminate-only-oneterm-s-own.md`; reaching no further than a child
  this process created is **new here and stricter** than `DEC-0005`, which
  deliberately terminates processes it did not create and scopes them by
  install-directory image path instead.

Future work that touches local-session teardown must keep this guarantee: **while
OneTerm is running**, a discarded session leaves no shell process and no console
host behind. The qualifier is load-bearing — see Consequences.

## Alternatives

- [x] Selected: bounded wait on the child handle after `ClosePseudoConsole`, then
  `TerminateProcess`.
- [ ] Wait without a bound — rejected: the measurement says an affected child
  never exits, so this leaks a blocked thread per closed session instead of a
  process.
- [ ] Delay `ClosePseudoConsole` until the client has finished starting —
  rejected: there is no signal for "finished starting", so it would be a sleep
  tuned against an unbounded start-up, and it would slow every normal close.
- [ ] Leave the orphan and only log it — rejected: the leak is what the user
  reported. One shell and one console host per affected launch, invisible in the
  application.
- [ ] Reap stray shells at the next startup — rejected: a different outcome with
  a different risk profile (it would have to identify processes it did not
  create), and explicitly out of scope for `BUG-0055`.

## Consequences

- [x] Benefit to confirm: a session dropped before its shell has started leaves
  no `cmd.exe` and no console host — pinned by
  `a_session_dropped_before_its_shell_starts_leaves_no_orphan`, which fails
  without the escalation.
- [ ] Tradeoff: OneTerm now terminates a process on the user's machine. It only
  ever does so after that process's pseudo-console is already destroyed, so the
  shell is unusable and unreachable by then, and only for a child it created
  itself. A shell that would have exited cleanly a moment later is never hit —
  the normal path finishes 100 times inside the grace period, and a **busy**
  shell is not hit either: the host's close request reaches every client on the
  console, so `cmd.exe` and a foreground grandchild (`ping -t`, `timeout /t 30`)
  both exit with `STATUS_CONTROL_C_EXIT` in ~20 ms. A grandchild that detached
  from the console survives, as it did before this decision.
- [ ] Limit, not a follow-up to fix here: the guarantee holds only while the
  application process lives. The grace period is served on a detached owner
  thread, so a session dropped as OneTerm exits — or when OneTerm is killed —
  gets no escalation, and the orphan survives exactly as it did before. That is
  the second half of the `IN-0031` observation ("two `cmd.exe` remained after the
  app pid had been terminated"). Closing it means either blocking quit on every
  open session's grace period or reaping at the next startup, which is a
  different outcome with a different risk profile (`BUG-0055`, Scope).
- [ ] Follow-up: the grace period is only measured against the system console
  host. The bundled `OpenConsole.exe` (`DEC-0013`) resolves from the executable's
  directory, which a test binary does not have, so the same table has not been
  taken against it. The bound is generous enough that a slower bundled host is
  not expected to matter.
