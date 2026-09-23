# Work: the local event loop throws the child exit away when its notification arrives hung up

ID: BUG-0074
Intake: IN-0029
Created: 2026-09-23

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: IN-0029 (`US-0083`, the local-shell pump and its event loop)

## Outcome

A local shell that exits on its own reaches `SharedSessionState::alive == false` on
every platform, whether or not the poller reports the exit notification together with a
hang-up on the source that carries it.

The reported failure, "Full workspace quality gate" on ubuntu-latest (2 vCPU),
`cargo test --workspace`, the owner's push of `main` carrying `IN-0044`
(`US-0133`..`US-0136`, `BUG-0073`):

```text
session::session_tests::spawned_shell_exit_is_detected FAILED (session_tests.rs:441)
shell exit not detected in 15.004306202s (bound 15s); alive=true at the end; terminal
snapshot: ["exit   runner@runnervmtr4k5:~$ exit   logout"]
28 passed; 1 failed; 1 ignored
```

That message is `BUG-0066`'s instrument working exactly as designed, and its gap 2 named
this branch in advance: *"a populated snapshot with `alive=true` means the exit never came
back through the reaper thread and the `PTY_CHILD_EVENT_TOKEN` branch, which is a real
defect and a new packet."* This is that packet.

## Scope

- [x] In scope: the per-event dispatch in `ShellEventLoop::run`
      (`crates/local-shell/src/event_loop.rs`) — which poll event means "ask the watcher
      for a child event", which means "drain the PTY", and in which order those two
      questions are asked. One regression guard for that order. The one sentence in
      `docs/terminal-backend.md` §6.2 that described the old order.
- [x] Out of scope: the reaper thread and the socket pair in `crates/vt/src/pty/unix.rs`
      (they behave correctly; see Context), the Windows watcher
      (`crates/vt/src/pty/windows/child.rs`, `BUG-0072`), the bash shell-integration
      environment from `US-0136` (investigated and cleared — see Context), the
      `SHELL_ROUND_TRIP` bound (`BUG-0066`), and the embedder loop published in
      `crates/vt/docs/guide/13-pty.md`, which already has the right order.

## Acceptance

- [x] A poll event on `PTY_CHILD_EVENT_TOKEN` is dispatched to the child watcher whether
      or not it also reports a hang-up.
- [x] A hang-up on the PTY itself still suppresses I/O on it — the guard keeps the job it
      was written for.
- [x] Restoring the old order fails a test in this repository, rather than only a CI job on
      one runner OS.
- [x] `docs/terminal-backend.md` no longer states the order that caused the defect.
- [x] `cargo test -p oneterm-core -p oneterm-local-shell` passes.
- [x] `pwsh scripts/ci-local.ps1` passes.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` §6.2 "Current implementation" (`:653-670`) — the owning
  description of this loop. It stated the defective order as a fact and relied on it:
  *"a `notify()` wake arrives as a keyless interrupt, which the loop `continue`s past
  before it ever compares the token"*. That clause was written by `BUG-0072`'s verifier
  (finding F2) to correct a different error in the same paragraph, and it recorded the
  hoisted guard accurately — as a property, not as a defect. **Updated:** the paragraph now
  states the order this packet establishes and why.
- `docs/spec-intakes/IN-0029-vt-engine/BUG-0066-shell-exit-detection-bound-fails-on-ci.md`
  — the previous failure of this same test, and the source of the diagnostic message that
  made this one readable. Its gap 2 predicted this packet and named the two files. **No
  change needed:** it is an accurate record of a past run and its own remedy (the bound)
  is untouched.
- `docs/spec-intakes/IN-0039-vt-gaps-and-publish/BUG-0072-child-exit-watcher-test-fails-under-load.md`
  and its `evidence/BUG-0072-verify.md` finding F2 — the Windows half of the same class
  ("the child exit must be reachable"). F2 is where the hoisted guard was first written
  down, as a pre-existing hazard left in place. **No change needed:** F2 is evidence of
  what was observed then, and this packet is the follow-up it implies.
- `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md` — the owning packet
  for the loop. It describes the two tokens and the dedicated owner thread; it states no
  order between the hang-up guard and the token comparison. **No change needed.**
- `crates/vt/docs/guide/13-pty.md` (`:107-127`) — the embedder loop OneTerm's is a port
  of, and the crate's public contract for consumers who vendor `oneterm-vt`. It matches
  one `event.key` first and never asks about hang-ups outside the PTY arm, so the
  published contract was already correct and no embedder was misled. **No change needed**,
  and this is the reason the fix is OneTerm's alone.
- `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/US-0136-shell-integrations-emit-the-full-osc-133-set.md`
  — the packet whose push surfaced the failure. Reviewed in full because the brief's first
  lead was its bash environment. **No change needed:** see Context, "What `US-0136` did
  and did not do".

### Documentation Action

Update required: `docs/terminal-backend.md` §6.2.

Reason: that paragraph is the only place the loop's event dispatch is described, and it
described the wrong order as the intended one. Everything else reviewed is either evidence
of a past run (not rewritten) or already correct.

### Reconciliation

Docs changed: `docs/terminal-backend.md` §6.2 — the sentence about the keyless `notify()`
wake now says the loop reads a child exit only on the token, and the two sentences after it
state that the token is compared before the hang-up guard and why. The no-change reasons
above stand.

## Context

### The causal chain

1. `crates/vt/src/pty/unix.rs:205-219` — the reaper thread waits for the child, posts
   `ChildEvent::Exited` on an `mpsc`, writes one byte to its half of a `UnixStream` pair to
   wake the poller, and then **returns**, which drops that half.
2. Closing one end of an `AF_UNIX` stream pair sets the peer's `sk_shutdown` to
   `SHUTDOWN_MASK`, and `unix_poll` turns that into `EPOLLHUP`. So from the instant the
   reaper thread finishes, the loop's half (`exit_signal`,
   `crates/vt/src/pty/unix.rs:184-192`) reports `EPOLLIN | EPOLLHUP`: the notification and
   the hang-up are **one event**.
3. `polling` maps `EPOLLHUP` to `Event::is_interrupt()`
   (`polling-3.11.0/src/lib.rs:315-317` → `epoll.rs:389-391`).
4. `crates/local-shell/src/event_loop.rs:373-375` asked `event.is_interrupt()` and
   `continue`d **before** it compared `event.key == PTY_CHILD_EVENT_TOKEN` at `:377`. The
   exit was discarded.
5. The pair is registered level-triggered (`crates/vt/src/pty/unix.rs:243-248`), so the
   same event is re-delivered on every poll and discarded again. The loss is permanent,
   not a missed wake-up: `alive()` never flips, `Exited`/`Closed` never reach the UI, and
   the loop spins on the hung-up master (which is also permanently `EPOLLHUP` once the
   slave is gone) for the life of the tab.

**How often this loses is not settled, and the packet does not claim it is** (verified,
`evidence/BUG-0074-verify.md` F2-F4). `EPOLLHUP` is a *level* condition, not an edge: once
the reaper's `close(2)` has landed, every later `epoll_wait` on `exit_signal` reports it.
So the loop sees the byte without the hang-up only when its own
`ep_send_events` → `ep_item_poll` → `unix_poll` computes the mask **before** that `close`.
The two sides of that are the reaper returning from `unix_stream_sendmsg` to userspace,
running one drop glue and taking `unix_state_lock` in `close(2)` — single-digit
microseconds — against the loop being woken from inside the reaper's own `write`, being
*scheduled* (on a two-vCPU VM, usually an IPI to a halted vCPU plus any host steal),
returning from `ep_poll` and re-polling the item — microseconds to tens of microseconds.
The reaper is heavily favoured, so the chain as written predicts near-deterministic loss
whenever the loop thread is not already parked in `epoll_wait` at the instant of the write,
and that does **not** by itself account for the green runs that preceded the failure.
Either the framing is incomplete or the green history means something other than "the loop
won the race"; see "Why it surfaced now". Nothing here weakens the fix, which is correct
whichever it is — it only changes what the next ubuntu run proves.

There is no host comparison to draw. This project has exactly one machine that runs the
Unix notification path at all: the ubuntu runner. `crates/vt/src/pty/unix.rs` has never
been compiled on the owner's Windows host (`5a0106f6` says so outright), so "it passes
locally" is not evidence about this code and is not offered as any.

### Why it surfaced now

The green history is about **one week**, not weeks: the ubuntu job could not build the Unix
PTY at all until `5a0106f6` (2026-09-16, `BUG-0063`, `SignalMask`'s `PartialEq` over
`libc::sigset_t`). Before that, this path never ran in CI.

The variable that decides the window above is not how many cores are busy. It is
**whether the loop thread is parked in `epoll_wait` at the instant of the reaper's write.**
Parked, the wake happens inline inside `unix_stream_sendmsg` and the loop has its only real
chance to return before the `close`. Busy — parsing a batch, holding the `Term` lock,
inside `finish_batch_blocking` — it reaches `poller.wait()` long after the `close` and sees
`EPOLLIN|EPOLLHUP` with certainty.

On that reading `IN-0044` contributes directly rather than as ambient load: `US-0136` added
per-prompt output *to the exiting shell itself* — `PS0`, a forking `PROMPT_COMMAND`, the
raw-byte `PS1` mark and the full OSC 133 set — so at the moment `exit` is typed and bash
prints `logout` and dies, the loop is far more likely to be mid-batch than parked. It fits
the failing snapshot, where `exit` and `logout` had both reached the grid, i.e. output was
flowing when the child died. **This is the first thing to instrument if ubuntu fails
again**, ahead of gap 4's strace: record not only whether the first reporting `epoll_wait`
carries `EPOLLHUP`, but whether the loop thread was *inside* `epoll_wait` when the reaper's
write landed.

Three cross-checks that the diagnosis is the right one, not merely a plausible one:

- **Exactly one test in that binary depends on this path, and exactly that test failed.**
  Every other real-shell test ends its session with `PtySession::close()`, which clears
  `alive` synchronously (`crates/terminal/src/session.rs:746-753`) and never consults the
  watcher. `spawned_shell_exit_is_detected` is the only one that asks the child to exit by
  itself. `bash_prompt_draws_no_stray_bracket`, which `US-0136` added and which spawns the
  same `bash -l` on the same runner, passed.
- **The Windows job passed.** Its watcher posts a keyed IOCP completion packet
  (`crates/vt/src/pty/windows/child.rs`), which carries no hang-up flag, so the hoisted
  guard never fired there.
- **The upstream loop this one is a port of asks `is_interrupt()` inside the PTY arm**, and
  so does the loop OneTerm publishes for embedders
  (`crates/vt/docs/guide/13-pty.md:107-127`). The hoist is a defect of this port, not of
  the design.

### What `US-0136` did and did not do

The brief's leads were investigated in order and none of them is the cause.

- **The bash environment** (`crates/core/src/config/shell.rs:215-225`, `:494-529`). The
  trailing `( exit $__ot )` subshell is a real fork per prompt, and the only fork this
  shell performs. It does not outlive bash, does not hold the PTY slave after it exits, and
  does not change what the reaper sees — the reaper waits on bash itself
  (`child.wait()`), never on `-1`, so a grandchild is invisible to it. `PS0` and the raw
  four-byte `PS1` mark add output, not processes. Measured, not reasoned: see Evidence,
  "Driving Git-for-Windows bash through the real PTY".
- **`~/.bash_logout` / `clear_console`.** `-l` is not new — it predates `IN-0044`
  (`crates/core/src/config/shell.rs:465-470`, unchanged in `3c77a6e1..773fe693`), so this
  shell has always run `~/.bash_logout` on exit and the test has always passed with it.
  Nothing in `IN-0044` touches `SHLVL`, which is what gates the `clear_console` line, and
  the env the child gets is the process env plus overrides (`crates/vt/src/pty/unix.rs:113-119`),
  so `SHLVL` arrives from the CI step's own shell exactly as it always did. The probe below
  also ran an Ubuntu-shaped `.bash_logout` at `SHLVL == 1` and exit was still detected in
  79 ms.
- **Whether `alive()` waits on EIO from the master.** It does not. `alive` is cleared only
  by `publish_child_exit` on the watcher's `ChildEvent::Exited`
  (`crates/local-shell/src/event_loop.rs:377-387`, `:609-617`) or by `close()`. A lingering
  grandchild holding the slave would delay the master's hang-up, not the exit — and
  `publish_exit_blocking` clears `alive` *before* it blocks on the UI channel
  (`crates/terminal/src/backend/pump.rs:199-205`), so UI backpressure cannot hide an exit
  either.
- **The new local-shell test.** `bash_prompt_draws_no_stray_bracket` is not `cfg(windows)`;
  it runs on Unix and it did run on the failing job. It takes `SPAWN_GUARD` for its spawn,
  uses its own PTY, and closes its session. It shares no TTY state. What it does add is one
  more live `bash -l`, its event-loop thread and its reaper thread, on a two-vCPU runner.
  That is ambient load, and ambient load is the *weaker* of the two ways `IN-0044` reaches
  this defect — "Why it surfaced now" above gives the better-supported one. Either way it
  did not create the defect.
- **`BUG-0072`** changed `crates/vt/src/pty/windows/child.rs` only. Its **verifier**,
  however, is where the hoisted guard was written down (finding F2), and that finding is
  the strongest prior evidence for this packet.

### The change

`classify_event(key, interrupt, readable) -> PollAction` in
`crates/local-shell/src/event_loop.rs`, asked once per event: the child token first, the
hang-up guard second, `readable` last. The loop keeps its shape — the two `if`s in `run`
now test the returned action instead of the raw flags.

It is a free function over three plain values rather than a method on `&PollEvent` for one
reason: the hang-up flag cannot be constructed outside the poller crate, and a live child
cannot be asked to exit with and without a hang-up on demand. Taking the three values makes
the ordering testable on any host, which is what the old code had no way to be.

## Plan

- [x] Read the failure, the loop, the reaper, the watcher on both platforms, and the poller
      crate's flag mapping.
- [x] Clear the brief's five leads, by measurement where a measurement is possible.
- [x] Reproduce the shell half on this host: Git-for-Windows bash, login shell, injected
      environment, Ubuntu-shaped `.bash_logout`, through the real Windows PTY.
- [x] Fix the order, with the reason next to it.
- [x] Negative control: restore the old order and watch the guard fail.
- [x] Update `docs/terminal-backend.md` §6.2.
- [x] `cargo test -p oneterm-core -p oneterm-local-shell`, then `pwsh scripts/ci-local.ps1`.

## Decisions

None. The order this packet establishes is the one the published embedder guide already
documents (`crates/vt/docs/guide/13-pty.md`); nothing future work must inherit changes.

## Verification Plan

- Unit proof: `a_hung_up_child_notification_is_still_a_child_notification`
  (`crates/local-shell/src/event_loop_tests.rs`) — the child token wins over the hang-up
  flag, and the PTY token still loses to it.
- Negative control: restore the old order and confirm that test fails.
- Integration proof: `cargo test -p oneterm-core -p oneterm-local-shell`, then the whole
  gate, on Windows — where the code path is shared but the failing platform's flag is not.
- Platform proof: **not available on this host.** The defect needs `EPOLLHUP` on an
  `AF_UNIX` pair; there is no Linux target, no WSL and no container here. See gap 1.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Driving Git-for-Windows bash through the real PTY

A temporary `#[ignore]`d probe in `crates/local-shell/src`, spawning
`C:\Program Files\Git\usr\bin\bash.exe` (bash 5.3.15) through `LocalSession` and the real
Windows PTY, writing `exit\r` and waiting up to 20 s for `!alive()`. `HOME` pointed at a
temporary directory per case. Removed before the gate run; it is not in the committed diff.

```text
[1 plain -l (no injection)]                    detected=true elapsed=78.1ms  alive=false
[2 injected -l]                                detected=true elapsed=63.5ms  alive=false
[3 injected -l + ubuntu-shaped .bash_logout]   detected=true elapsed=79.1ms  alive=false
[4 injected -l, exit written before the prompt] detected=true elapsed=442.9ms alive=false
[5 plain -l, exit written before the prompt]   detected=true elapsed=403.0ms alive=false
```

Case 2 is `US-0136`'s environment verbatim — `PROMPT_COMMAND` with the trailing
`( exit $__ot )` subshell, `PS0`, and the raw-byte `PS1` append — on a login shell, and it
is *faster* than the uninjected case 1. Case 3 adds `if [ "$SHLVL" = 1 ]; then clear; fi`
as `~/.bash_logout`; the shell's `SHLVL` was 1 here, so that branch really ran (its grid
comes back empty, which is the `clear`), and the exit was still detected in 79 ms. Case 4
reproduces the CI ordering, where `exit` is written before bash has drawn a prompt and the
line is echoed twice — the shape of the failing snapshot,
`["exit", "runner@…:~$ exit", "logout"]` — and it too exits cleanly.

**So the shell half of every lead in the brief is cleared by measurement, and no fix to the
generated environment is warranted.** What this host cannot reproduce is the notification
half, because Git-for-Windows bash is driven through ConPTY and the Windows watcher, not
through a `UnixStream` pair.

### The regression guard, and the negative control

After the fix:

```text
test event_loop::event_loop_tests::a_hung_up_child_notification_is_still_a_child_notification ... ok
test result: ok. 35 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

With the two `if`s in `classify_event` swapped back to the shipped order, and nothing else
changed:

```text
thread '…a_hung_up_child_notification_is_still_a_child_notification' panicked at
crates\local-shell\src\event_loop_tests.rs:50:9:
assertion `left == right` failed: a hung-up child notification (readable=true)
  left: Ignore
 right: ChildEvent
test result: FAILED. 0 passed; 1 failed
```

The guard fails on exactly the row the runner failed on, and the fix was restored
immediately afterwards.

### `cargo test -p oneterm-core -p oneterm-local-shell`

```text
test result: ok. 89 passed; 0 failed; 0 ignored   (oneterm-core)
test result: ok. 35 passed; 0 failed; 2 ignored   (oneterm-local-shell)
```

### `pwsh scripts/ci-local.ps1`

Run in the worktree, Windows host, no `--full`. Every step passed; the script's own last
line, which is all it prints as a summary:

```text
ci-local: all checks passed.
```

### Independent verification

`docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0074-verify.md` — **PASS** on the code,
with narrative findings. It re-read every link of the chain in the source rather than from
this packet (F1), reproduced the negative control here down to the line and column (F9),
confirmed the full dispatch truth table changes exactly the two `child + interrupt` rows
and nothing else (F6), confirmed Windows is untouched (F8), confirmed the routing against
`docs/HARNESS.md` and the live `harness.db` (F10), and confirmed the gate. F2, F3, F4, F12
and F13 are applied above and below. Two of its observations are deliberately left as
adjacent, pre-existing and out of scope: `next_child_event()` draining an empty `mpsc`
would spin rather than end the session (prevented by the reaper's send-before-write
ordering, which the fix does not disturb), and a master that hangs up with bytes still
buffered has its tail dropped (identical on `main`; the mirror of this packet if a
truncated final line is ever reported on Linux).

### Gaps

1. **The failure itself is not reproduced on this host, and cannot be.** The trigger is
   `EPOLLHUP` on an `AF_UNIX` socket pair; this machine has the MSVC target only, no WSL
   and no container runtime. The causal chain is established from the four sources it runs
   through — the reaper (`crates/vt/src/pty/unix.rs:205-219`), the kernel's `unix_poll`
   behaviour on a closed peer, `polling`'s `EPOLLHUP` → `is_interrupt()` mapping (read in
   the vendored `polling-3.11.0` source), and the loop's own dispatch — plus the three
   cross-checks in Context. The proof that the ubuntu job is green again is the owner's
   next push.
2. **The guard is a unit test of the ordering, not an end-to-end test of a hung-up
   notification.** The loopback PTY in `event_loop_tests.rs` carries its child signal over
   a `TcpStream` pair, and a graceful peer close on TCP yields `EPOLLRDHUP`, not
   `EPOLLHUP`, on Linux; forcing a reset needs `TcpStream::set_linger`, which is still
   unstable. So there is no portable way to make the *loop* see a hung-up notification in a
   test. The ordering is the whole of the defect and the guard pins it exactly, but an
   end-to-end version would be stronger and is a candidate if the socket pair ever changes.
3. **The 100 % CPU spin on a hung-up master is left alone.** Between the child's exit and
   the watcher's event, `classify_event` returns `Ignore` for a level-triggered
   `EPOLLHUP | EPOLLIN` master and the loop re-polls immediately. With this fix that window
   is bounded by the watcher, which now always gets through, so it is microseconds; before
   it was the life of the tab. Narrowing it further (deregistering the master on hang-up)
   is a separate change and is not needed to close this defect.
4. **The smallest experiment for the runner**, if a repeat is ever needed: on a two-vCPU
   ubuntu runner, `cargo test -p oneterm-local-shell --lib spawned_shell_exit_is_detected`
   in a loop, with `strace -f -e trace=epoll_wait,close` on the test binary, and read
   whether the `epoll_wait` that first reports the exit socket also reports `EPOLLHUP`. A
   cheaper version needs no strace at all: the same loop before and after this commit.

## Harness Record

This packet was written in an isolated worktree, so `harness.db` in the main checkout was
not touched. Run this from the repository root to insert the row (Python 3, standard
library only):

```python
import sqlite3

with sqlite3.connect("harness.db") as db:
    db.execute(
        """INSERT OR REPLACE INTO story (
            id, title, created_at, risk_lane, contract_doc, packet_doc, status,
            unit_proof, integration_proof, e2e_proof, platform_proof, evidence,
            verify_command, last_verified_at, last_verified_result, notes, intake_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
        (
            "BUG-0074",
            "The local event loop throws the child exit away when its notification arrives hung up",
            "2026-09-23T00:00:00Z",
            "normal",
            "docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md",
            "docs/spec-intakes/IN-0029-vt-engine/BUG-0074-child-exit-lost-when-its-notification-hangs-up.md",
            "implemented",
            1, 1, 0, 0,
            "docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0074-verify.md",
            "pwsh scripts/ci-local.ps1",
            "2026-09-23T00:00:00Z",
            "pass",
            "ShellEventLoop asked event.is_interrupt() before it compared PTY_CHILD_EVENT_TOKEN. On Unix the reaper drops its half of the notification socket pair immediately after posting, so the peer reports EPOLLIN|EPOLLHUP and the exit arrived wearing a hang-up; the pair is level-triggered, so the loss was permanent and alive() never flipped. classify_event now asks the token first. Guard: a_hung_up_child_notification_is_still_a_child_notification, which fails on the shipped order. US-0136's bash environment was cleared by measurement through the real Windows PTY (login shell, injected env, Ubuntu-shaped .bash_logout: exit detected in 63-443 ms in five configurations). Not reproduced on Linux: no Linux target, WSL or container on this host, and pty/unix.rs has never been compiled here, so the owner's next ubuntu job is the platform proof. How often the race loses is not settled (verify F3): EPOLLHUP is a level condition, so the chain predicts near-deterministic loss whenever the loop thread is not parked in epoll_wait at the instant of the reaper's write. If ubuntu fails again, instrument that first (verify F4). The loopback PTY cannot carry a hung-up notification portably (TCP gives EPOLLRDHUP, and set_linger is unstable), so the guard pins the ordering rather than the end-to-end path.",
            34,
        ),
    )
```

The `evidence` column carries the verification document's path, as every other story under
this intake does; the prose is in `notes`. Verified against the live schema in
`evidence/BUG-0074-verify.md` F12: 17 columns in order, every `CHECK` satisfied, and
`intake_id=34` resolves to `IN-0029`.

## Handoff

Next action is the owner's: push, and read the ubuntu-latest "Full workspace quality gate"
job. `spawned_shell_exit_is_detected` green there is the platform proof this host cannot
give. A repeat failure with `alive=true` and a populated snapshot would mean the hang-up is
not the only way the notification is lost, and the strace experiment in gap 4 is the next
step.
