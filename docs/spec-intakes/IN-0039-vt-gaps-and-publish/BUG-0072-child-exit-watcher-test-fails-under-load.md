# Work: A child that exits before its poller is registered never wakes it

ID: BUG-0072
Intake: IN-0039
Created: 2026-09-21

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

- Change type: bug (**product**, not the test — see Context)
- Risk lane: normal
- Spec Intake, when required: `IN-0039` —
  `docs/spec-intakes/IN-0039-vt-gaps-and-publish/IN-0039.md`, the intake that owns
  `crates/vt`'s public surface and its gate. The `pty` transport ships inside that surface.

## Reported by

`pwsh scripts/ci-local.ps1` on 2026-09-21, while another worktree was compiling on the same
machine:

```
pty::windows::child::tests::instant_exit_is_not_missed
panicked at crates\vt\src\pty\windows\child.rs:256:
the exit must arrive on the child token
```

6/6 standalone runs of the same test passed immediately afterwards. The report came in as
"a test that only passes on an idle machine", which is how `BUG-0070` in `IN-0032` started;
unlike that one, this test was right and the product was wrong.

## Outcome

`ChildExitWatcher` delivers the child's exit on `PTY_CHILD_EVENT_TOKEN` **whatever the order**
in which the wait callback and `register` happen to run. A child that has already exited when
the embedder registers its poller still produces exactly one wake carrying exactly one
`ChildEvent::Exited`, and the test that says so fails when the product stops doing it.

## Scope

- [ ] In scope:
  - `crates/vt/src/pty/windows/child.rs` — the callback / `register` hand-off and its tests.
  - `crates/vt/src/pty/windows/child.rs` module doc and `docs/terminal-backend.md` § 6.3 —
    the ordering the fix makes a contract.
- [ ] Out of scope:
  - The Unix reaper (`crates/vt/src/pty/unix.rs`). It writes to a socket the poller watches
    level-triggered, so a late `register` finds the byte still there; different mechanism,
    no equivalent hole. Checked, not changed.
  - `CHILD_EXIT_GRACE` and the drop-time escalation (`DEC-0016`, `BUG-0055`). Untouched.
  - The consumers' loops (`crates/local-shell/src/event_loop.rs`,
    `crates/tools/src/bin/vt-esctest.rs`). Both already tolerate a wake with no event; the
    defect is that they got no wake at all.

## Acceptance

- [x] The root cause is established by a probe against the unfixed product, and written down
      here, before either the product or the assertion is touched.
- [x] A child that exits before `register` wakes the poller on the child token.
- [x] The exit is still reported exactly once, with its code.
- [x] A test makes the lost wake observable **deterministically** — no sleep-and-hope, no
      dependence on which poll iteration the callback lands in.
- [x] 30 standalone runs and 3 `cargo test --workspace` runs, both under a parallel
      `cargo build` in this worktree, with the tallies recorded here.
- [x] `docs/terminal-backend.md` § 6.3 states the ordering.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` § 6.2 (the poller waits without a timeout — every wake is a
  posted packet, so a lost packet is a lost event forever) and § 6.3 "Child exit"
  (`oneterm_vt::pty` watches the child handle "(race-free)" and reports `ChildEvent::Exited`
  on `PTY_CHILD_EVENT_TOKEN`). The word this packet had to earn is *race-free*.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md` — the transport's design: the
  embedder owns the poller, the transport only records where to post.
- `crates/vt/src/pty/mod.rs` module doc and `EventedPty` — "child exit must be observable
  without reading", "Both are race-free".
- `docs/decisions/DEC-0016-*` — the drop-time grace period. Adjacent, unchanged.

### Documentation Action

Update required:

- `docs/terminal-backend.md` § 6.3 "Child exit" — say what "race-free" now rests on: the
  exit is queued before the wake is posted, and a registration that arrives after the exit
  posts the wake itself.
- `crates/vt/src/pty/windows/child.rs` module doc — the same two sentences at the code.

Reason: the fix makes an ordering load-bearing that the docs only asserted as an adjective.
A future reader who moves the `send` below the `post`, or who drops the "already exited"
branch out of `register`, must be able to see from the docs that they are breaking a promise.

### Reconciliation

Changed: `docs/terminal-backend.md` § 6.3 (the "Child exit" bullet) and the module doc of
`crates/vt/src/pty/windows/child.rs`. `crates/vt/src/pty/mod.rs`'s "Both are race-free"
was re-read and is now true as written, so it stays. `docs/agents/*` and the `IN-0029` LLD
describe the seam, not the ordering inside the watcher, and needed no edit.

## Context

### The root cause: a wake with nowhere to go

`RegisterWaitForSingleObject` fires the callback **immediately** for a handle that is already
signalled — that is the whole point of `instant_exit_is_not_missed`. The callback runs on a
thread-pool wait thread, concurrently with the thread that is still constructing the session,
and it does two things (`crates/vt/src/pty/windows/child.rs:88` and `:96` before the fix):

```rust
let _ = sender.events.send(ChildEvent::Exited(status));   // 1. queue the event
let interest = sender.interest.lock()...;
if let Some(interest) = interest.as_ref() {
    let _ = interest.poller.post(CompletionPacket::new(interest.event));   // 2. wake
}
```

Step 1 is correctly ordered before step 2 — the hypothesis that the product notifies before it
enqueues is **refuted**. The defect is step 2's `if let`: when the callback wins the race
against the embedder's `register`, `interest` is `None`, **nothing is posted, and nothing ever
posts it later**. `register` (`:159`) only stored the interest; it never looked at whether the
exit it was registering for had already happened.

So the event sits in the `mpsc` channel with no wake attached to it. `wait_for_exit`'s poll
then times out at 200 ms, `next_event()` returns `Some`, and the assertion that the exit
arrived on the child token fails — **correctly**. The test was accurate; only its failure mode
(after the 200 ms timeout, on the iteration after the one that should have carried the token)
made it look like a load-dependent flake.

### The probe

Against unfixed `main` (`410136e0`), a temporary test: exit a child, reap it, build the
watcher, sleep 250 ms so that the callback has provably run, register, then poll for 2 s.

```
PROBE woken=false event=Some(Exited(Some(ExitStatus(ExitStatus(5)))))
panicked at crates\vt\src\pty\windows\child.rs:327: the exit must wake the poller on the child token
```

Deterministic, not probabilistic: with the callback given a 250 ms head start it fails every
time. That is the same state the loaded `ci-local` run reached by accident.

### Why it is a product defect and not a test defect

The real consumer reads the child event **only** under the token
(`crates/local-shell/src/event_loop.rs:377-378`):

```rust
if event.key == PTY_CHILD_EVENT_TOKEN {
    if let Some(ChildEvent::Exited(status)) = self.pty.next_child_event() { ... }
    continue;
}
```

and § 6.2 of `docs/terminal-backend.md` has that loop waiting on the poller **without a
timeout**. A lost packet is therefore not a delayed exit report — it is a permanent one. The
session stays `alive`, the tab never shows the shell exited, and the `PseudoConsole` is not
dropped until the user closes the tab. The broken conout pipe does not rescue it: the read
loop treats `Ok(0)` as "nothing to read" and goes back to the poller.

The window in the real loop is not narrow. Between `PseudoConsole::spawn` (which installs the
watcher) and `register` (`event_loop.rs:281`, inside `run`) the owner thread reads the child
pid, sets the log identity, **creates and opens the session log file**, and signals the
spawner over a channel. A shell that exits at once — a bad custom-shell path, `pwsh -Command
exit`, a profile that fails — can easily finish first. It is exactly the case
`instant_exit_is_not_missed` exists to cover, which is why that test is the one that caught it.

A wake with no event, the other half of the hypothesis, is harmless and stays legal: both
consumers `continue` on a `None`, and the fix can post one extra packet in a benign race.

### The fix

One lock now covers both "where to post" and "has it already exited", so whichever of the two
sides runs **second** is the one that posts:

- the callback queues the event, then, under the lock, records the exit and posts if an
  interest is installed;
- `register` installs the interest and, under the same lock, posts at once if the exit was
  already recorded.

Exactly one of the two branches can see the other's write first, so the ordinary path still
posts exactly one packet. An embedder that registers, deregisters and registers again after
the exit gets a second, eventless wake — legal, and cheaper than tracking consumption.

## Plan

- [x] Read the callback, `register`, `next_event`, the consumers, and the Unix half.
- [x] Probe the unfixed product; decide product vs. test.
- [x] Fix the hand-off; keep `wait_for_exit`'s contract assertion.
- [x] Add the deterministic regression test.
- [x] Re-shape `wait_for_exit` so it cannot fail for the *remaining* benign reason (see below).
- [x] Update the module doc and `docs/terminal-backend.md` § 6.3.
- [x] 30 standalone + 3 workspace runs under load; record the tallies.

### The remaining benign gap in `wait_for_exit`

Even with the fix, `send` and `post` are two steps: a poll that times out in the microseconds
between them sees no token while `next_event()` already returns `Some`. The old loop asserted
on that iteration and would still fail, now for a reason that is not a defect. The loop now
returns only from an iteration whose poll **carried** the child token, and demands the event
be present in that same iteration — which is precisely the "enqueue before wake" contract, and
is what a regression to notify-before-enqueue breaks. An iteration that finds neither simply
polls again.

## Decisions

None. The ordering is recorded in the module doc and § 6.3 next to the code that must keep it;
it constrains this one file, not future work elsewhere, so it does not earn a `DEC`.

## Verification Plan

- Focused: `cargo test -p oneterm-vt --lib pty::windows::child::` — all five watcher tests.
- The flake itself: 30 standalone runs of `instant_exit_is_not_missed` **with a parallel
  `cargo build` running in this worktree**, plus 3 `cargo test --workspace` runs under the
  same load.
- Mutation: revert each half of the fix and confirm the new test fails.
- Regression: `cargo test -p oneterm-vt`, then `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### The change

`crates/vt/src/pty/windows/child.rs`, production:

- `ChildExitSender::interest: Mutex<Option<Interest>>` becomes `notify: Mutex<Notify>`, where
  `Notify` holds the optional `Interest` and an `exited` flag, and owns the one-line `post`
  both sides call;
- the callback sets `exited` and posts under that lock, after the `send` as before;
- `register` posts under the same lock when `exited` is already set;
- `deregister` clears the interest only — the flag is the record that the exit happened, and
  a re-registration after a deregistration must still wake.

Tests: `an_exit_before_registration_still_wakes_the_poller` (new, deterministic — it spins on
the internal `exited` flag under a 5 s deadline, so `register` is provably second, with no
sleep), and `wait_for_exit` re-shaped as described above. The 200 ms poll timeout is kept: it
is what lets the loop re-poll rather than block forever if a wake is genuinely lost.

### Runs

Load: `cargo build --workspace --all-targets`, then `cargo clean`, looping in this worktree
throughout — into a scratch `CARGO_TARGET_DIR`, so the loop never touched the artefacts the
tests under measurement were running from. Six build jobs on the same machine, which is a
heavier load than the `ci-local` run that reported the bug.

```
30x  cargo test -p oneterm-vt --lib pty::windows::child::tests::instant_exit_is_not_missed
     passed=30 failed=0
 3x  cargo test --workspace
     passed=3  failed=0
 5x  cargo test -p oneterm-vt --lib pty::windows::child::   (the five watcher tests)
     passed=5  failed=0
```

### What fails when the product breaks — measured

Both mutations keep `Notify` (the test reads its `exited` flag), so they break one half of the
fix each rather than reverting the type:

| Mutation | Result |
|---|---|
| `register` stops posting when the exit is already recorded | `an_exit_before_registration_still_wakes_the_poller` **FAIL 3/3**, "registering after the exit must still wake the poller", each run spending the full 5 s on the poll |
| The callback stops recording the exit (`notify.exited = true` deleted) | **FAIL 3/3** on the 5 s deadline, "the wait callback never ran for an already-exited child" |
| The callback posts **before** it sends (notify-before-enqueue) | Not deterministically detectable; not run. See Gaps. |

The unfixed product itself is covered by the probe in Context: `woken=false` with the event
already in the channel, reproducible on every run once the callback is given a head start.

### Gaps

- **The enqueue-before-wake half of the contract is documented, not pinned by a test.** Moving
  the `send` below the `post` leaves a window of a few instructions; `wait_for_exit` would fail
  only when a 200 ms poll expires inside it. Pinning it would mean injecting the callback's two
  steps behind a test seam in production code — a seam for a two-line ordering that the module
  doc and § 6.3 now both state at the point of change. Not taken; recorded here instead.
- **Windows only.** The Unix reaper was read and has no equivalent hole (a level-triggered
  socket byte survives a late registration), but nothing on Unix was run from here. CI covers it.
- **The benign double-post is untested.** Registering twice around an exit posts two packets
  and the second carries no event. Both consumers `continue` on that path, which is
  pre-existing behaviour (`deregistering_keeps_the_exit_observable` already covers the
  channel's side), so no test was added for it.
- **No consumer-level regression test.** `crates/local-shell`'s loop is unit-tested against a
  loopback PTY (`event_loop_tests.rs`) whose stub child watcher has no such race, so the fix is
  proved at the watcher, not through the session. Reproducing it through a real shell would
  need a shell that exits inside the spawn-to-register window — timing-dependent, which is what
  this packet exists to remove.

## Handoff

Complete. The product changed, the two owning docs changed with it, and the intake's packet
list carries the row.
