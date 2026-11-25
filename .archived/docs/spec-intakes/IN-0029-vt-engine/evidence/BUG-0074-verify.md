# Evidence: independent verification of BUG-0074

Subject: `4bcaad6f` on `fix/child-exit-hangup`, one commit on top of `main @773fe693`.
Packet: `docs/spec-intakes/IN-0029-vt-engine/BUG-0074-child-exit-lost-when-its-notification-hangs-up.md`.
Verifier host: Windows 11, MSVC target only (no Linux target, no WSL, no container).
Date: 2026-09-23.

## Verdict

**PASS.** The defect is real, the causal chain holds link by link on Linux as far as this
host can establish it, the fix is minimal and changes exactly the two rows of the dispatch
truth table that carried the defect, the negative control reproduces here, the routing is
correct, and the whole gate is green.

Three findings are worth the owner's attention before the ubuntu job is read as the
platform proof: the "race the owner's eight-core host wins" sentence in the packet is
factually wrong (F2), the race as characterised predicts a *deterministic* Linux failure
rather than a week of green runs (F3), and there is a better-supported "why now" than
ambient CPU load (F4). None of them changes the fix; all of them change what a repeat
failure would mean.

## Findings

### F1 — Confirmed: the causal chain holds on Linux (severity: info, verified)

Every link was read in the source rather than taken from the packet.

1. **The reaper really drops its half.** `crates/vt/src/pty/unix.rs`: `UnixStream::pair()`
   binds `(exit_signal, waker)`; `exit_signal` is stored in `PseudoConsole`, and `waker`
   is **moved by value** into `reap_in_background`, which moves it again into the thread
   closure (`fn reap_in_background(mut child, events, mut waker: UnixStream)`). The
   closure's last statement is `write_all(&mut waker, &[1])`, so `waker`'s drop glue — a
   `close(2)` — runs the instant the closure returns. Ownership confirmed; no `ManuallyDrop`,
   no leak, no longer-lived clone.
2. **Linux really reports `EPOLLHUP` on the survivor.** `unix_release_sock`, for a
   `SOCK_STREAM` peer, does `WRITE_ONCE(skpair->sk_shutdown, SHUTDOWN_MASK)` and then
   `skpair->sk_state_change(skpair)` / `sk_wake_async(skpair, SOCK_WAKE_WAITD, POLL_HUP)`.
   `unix_poll` (the `->poll` of `unix_stream_ops`) reads that field and sets
   `mask |= EPOLLHUP` when `shutdown == SHUTDOWN_MASK`, and additionally
   `mask |= EPOLLRDHUP | EPOLLIN | EPOLLRDNORM` for `RCV_SHUTDOWN`. Two consequences the
   packet does not spell out and that matter below: the hang-up is a **level** condition —
   it is reported by *every* later `epoll_wait`, not once — and the socket stays
   **readable forever** even after its byte is drained. `sk_err = ECONNRESET` is *not* set
   here, because that branch needs the closing half's receive queue to be non-empty and
   nothing ever writes to the reaper's half, so the event is `EPOLLIN|EPOLLHUP` without
   `EPOLLERR`.
3. **`polling` really maps it to `is_interrupt()`.** `polling-3.11.0/src/lib.rs:313-317`:
   `is_interrupt()` → `self.extra.is_hup()`; `src/epoll.rs:388-391`:
   `is_hup()` → `self.flags.contains(epoll::EventFlags::HUP)`. Note also
   `src/epoll.rs:337-346`: `readable` is `flags.intersects(IN|HUP|ERR|PRI)`, so a hang-up
   alone already sets `readable`.
4. **The registration really is level-triggered.** `src/epoll.rs:293-299`:
   `PollMode::Level => epoll::EventFlags::empty()` (no `ET`, no `ONESHOT`).
   `crates/vt/src/pty/unix.rs:245-252` and `:265-272` pass `PollMode::Level`
   **explicitly** for `exit_signal` — it is not merely inherited from the caller — and
   `crates/local-shell/src/event_loop.rs:280` passes `PollMode::Level` for the master.
   So a discarded event is re-delivered and re-discarded forever, exactly as claimed.
5. **There is no second way out.** `alive` is cleared only by
   `SharedSessionState::record_exit` / `set_alive(false)`
   (`crates/terminal/src/backend/state.rs:148-156`,
   `crates/terminal/src/backend/pump.rs:199-224`), reached from `publish_child_exit` in the
   child arm, or by `PtySession::close()` (`crates/terminal/src/session.rs:750-753`). The
   read arm's `Ok(0)`/`Err` paths only `break` the inner read loop. So losing the child
   event loses the exit permanently — the packet's claim, confirmed.

### F2 — The "eight-core host wins the race" sentence is wrong (severity: medium, narrative)

Packet, Context: *"That is why this is a race the owner's eight-core host wins and a
two-vCPU runner loses."* The owner's host is Windows with the MSVC target only — the
packet's own gap 1 says so — and `crates/vt/src/pty/unix.rs` has never been compiled
there. `5a0106f6` ("unbreak the Linux build") states it outright: the file "has never been
compiled here". The Unix notification path runs on exactly one machine in this project,
the ubuntu runner. The host comparison therefore explains nothing and should be struck; it
is the kind of sentence a later reader would build on.

### F3 — As characterised, the chain predicts deterministic failure, not a rare race (severity: medium, analysis)

Packet, Context: *"The window is the few hundred nanoseconds between the reaper's
`write_all` and its own return."*

That is not the window. `EPOLLHUP` is a level condition (F1.2): once the reaper's `close`
has run, **every** subsequent `epoll_wait` on `exit_signal` reports it, for the life of the
tab. The only way the loop ever sees the byte *without* the hang-up is for the loop
thread's `ep_send_events` → `ep_item_poll` → `unix_poll` to compute the mask **before** the
reaper's `close(2)` lands. So the race is between:

- the reaper: return from `unix_stream_sendmsg` to userspace, run one drop glue, enter
  `close(2)`, take `unix_state_lock` — order of 1-3 µs; and
- the loop: be woken by `sk_data_ready` from inside the reaper's own `write`, be *scheduled*
  (on a 2-vCPU VM that usually means an IPI to a halted vCPU, plus any host steal), return
  from `ep_poll`, then re-poll the item — typically µs to tens of µs.

The reaper is heavily favoured. A chain that predicts "the loop nearly always loses" cannot
also account for a week of green `spawned_shell_exit_is_detected` runs on the same runner.
Either the race framing is incomplete (see F4) or the green history means something other
than "the loop won the race". The packet's Handoff hedges this correctly — *"a repeat
failure … would mean the hang-up is not the only way the notification is lost"* — but
Context presents the race as settled. It is not.

This does not weaken the fix. The fix is correct whether the loss is deterministic or
occasional; it only changes what the next ubuntu run proves.

### F4 — A better-supported "why now" (severity: low, analysis; my own view)

Two corrections to *"IN-0044 added one more live shell to a two-vCPU runner"*.

**Timeline.** The green history is about one week, not weeks. The ubuntu job could not
build the Unix PTY at all until `5a0106f6` (2026-09-16, BUG-0063 — `SignalMask`'s
`PartialEq` over `libc::sigset_t`). Before that this path never ran in CI. Worth saying,
because "it passed for weeks" is load-bearing in the packet's reasoning.

**Mechanism.** The variable that decides F3's race is not how many cores are busy; it is
**whether the loop thread is parked in `epoll_wait` at the instant of the reaper's write.**
Parked, the wake happens inline inside `unix_stream_sendmsg` and the loop has its only real
chance to return before the `close`. Busy — parsing a batch, holding the `Term` lock,
inside `finish_batch_blocking` — the loop reaches `poller.wait()` long after the `close`
and sees `EPOLLIN|EPOLLHUP` with certainty.

US-0136 did not only add one more shell. It added per-prompt output *to the exiting shell
itself*: `PS0`, a `PROMPT_COMMAND` that forks (`( exit $__ot )`), a raw-byte `PS1` mark, and
the full OSC 133 set. At the moment `exit` is typed and bash prints `logout` and dies, the
loop is now far more likely to be mid-batch than parked. That is a direct causal
contribution from IN-0044, not ambient load, and it fits the failing snapshot
(`exit` / `logout` had reached the grid, i.e. output was flowing when the child died).

Not testable on this host. If gap 4's strace experiment is ever run, record one more thing
besides whether the first reporting `epoll_wait` carries `EPOLLHUP`: whether the loop
thread was *inside* `epoll_wait` when the reaper's `write` landed.

### F5 — Alternatives the brief asked about: checked, and cleared (severity: info)

- **Reaper ordering.** `events.send(ChildEvent::Exited(status))` precedes
  `write_all(&mut waker, &[1])` (`crates/vt/src/pty/unix.rs`, `reap_in_background`). That
  order is required and correct: `mpsc`'s send/`try_recv` carry their own release/acquire,
  so a visible byte implies a visible item. No lost-exit window here. The implementer got
  this right, and the fix does not disturb it.
- **`next_child_event()` draining an empty `mpsc`.** Cannot happen on the normal path, for
  the reason above. If it ever did, the child arm `continue`s — and the socket stays
  permanently readable (`RCV_SHUTDOWN` keeps `EPOLLIN` set even after the byte is drained,
  F1.2), so the loop would spin at 100 % CPU forever instead of ending the session. The
  invariant that prevents it lives in `unix.rs`; nothing next to the `continue` in
  `event_loop.rs` records it. Latent, pre-existing, unchanged by this fix; one comment
  would close it.
- **Key collision with the poller's own wake-up.** None. `PTY_CHILD_EVENT_TOKEN = 1`,
  `PTY_READ_WRITE_TOKEN = 2` (`crates/vt/src/pty/mod.rs:94,97`); `polling` reserves
  `NOTIFY_KEY = usize::MAX` (`lib.rs:125`), rejects it in `add`/`modify`, and
  `Events::iter()` **filters it out** (`lib.rs:895`). A bare `notify()` therefore yields no
  event at all in the loop's iteration — see F13.
- **An EOF/EIO fallback.** There is none (F1.5), so the packet is right that the child arm
  is the only route.

### F6 — The fix is correct and complete for the arms that exist (severity: info, verified)

Full truth table, `main` vs `4bcaad6f` (`other` = any key that is neither token; none is
ever registered):

| key | `interrupt` | `readable` | main | 4bcaad6f |
| --- | --- | --- | --- | --- |
| child | true | true | **skipped (the defect)** | `ChildEvent` |
| child | true | false | **skipped (the defect)** | `ChildEvent` |
| child | false | true | child arm | `ChildEvent` |
| child | false | false | child arm | `ChildEvent` |
| master | true | true | skipped | `Ignore` |
| master | true | false | skipped | `Ignore` |
| master | false | true | read arm | `ReadWrite` |
| master | false | false | skipped | `Ignore` |
| other | false | true | read arm | `ReadWrite` |
| other | true / not readable | any | skipped | `Ignore` |

Exactly the two child+interrupt rows change. Specifically:

- **The master's hang-up guard is intact.** `classify_event(PTY_READ_WRITE_TOKEN, true, _)`
  is `Ignore`, which is byte-for-byte what main's `continue` did. No I/O is attempted on a
  hung-up master.
- **No busy-spin regression.** `Ignore` and `continue` have the same cost; the loop's
  re-poll behaviour on a level-triggered hung-up master is unchanged. The packet's gap 3
  states this honestly, and it is now *better* than main, where that spin lasted the life
  of the tab.
- **No lost EOF/Closed on the master arm.** The read arm never published lifecycle events
  in the first place (F1.5); `Ok(0)` only breaks the inner read loop. Nothing to lose.
- **Shape preserved.** The loop still runs the child arm before the read arm, still
  `continue`s when `next_child_event()` yields nothing, still `deregister_pty()`s and
  returns on `Exited`.

The classifier is also the right *size*: three plain values, one function, no trait, no
wrapper type. The reason given for not taking `&PollEvent` — that the hang-up flag cannot
be constructed outside the poller crate — is correct (`EventExtra::set_hup` exists but
`Event`'s `extra` field is crate-private, and `polling` exposes no constructor that sets
it).

**The "upstream" claim checks out.** `alacritty_terminal`'s `EventLoop::run` — the loop
this one is a port of — matches `event.key` first and asks `is_interrupt()` only inside the
`PTY_READ_WRITE_TOKEN` arm, under the comment *"Don't try to do I/O on a dead PTY."*
(from knowledge; the vendored fork is no longer in the tree, so this was not re-read here).
So the hoist was introduced by this port, as the packet says, and the fix restores the
original shape rather than inventing one.

**No sibling caller is left broken.** `event.is_interrupt()` has exactly one call site in
the whole workspace — `crates/local-shell/src/event_loop.rs:373` — and the only other
place that dispatches on `PTY_CHILD_EVENT_TOKEN`, `crates/tools/src/bin/vt-esctest.rs:170`,
already compares the key first and asks about hang-ups nowhere. `crates/ssh` does not use
`polling` at all. So this one guard is the root-cause fix, not a patch of the path the
ticket named.

### F7 — Two rows of the classifier are untested (severity: low)

`a_hung_up_child_notification_is_still_a_child_notification` covers all four child rows and
three master rows. It omits `(PTY_READ_WRITE_TOKEN, true, false)` and any unknown key.
Both are unreachable in practice (only two keys are registered and `NOTIFY_KEY` is
filtered) and behaviourally trivial. Completeness nit, not a defect — the master loop could
have taken `readable` the way the child loop does, for one more line.

### F8 — Windows is unaffected, confirmed (severity: info, verified)

`crates/vt/src/pty/windows/child.rs:100-103` posts
`CompletionPacket::new(interest.event)` where `interest.event` is the
`Event::readable(PTY_CHILD_EVENT_TOKEN)` handed to `ChildExitWatcher::register` — a default
`EventExtra`, no hang-up flag. On the IOCP backend `is_interrupt()` is false for that
packet, so the hoisted guard never fired on Windows. The packet's "why only the ubuntu job
failed" is right, and the Windows path is untouched by the commit (`git show --stat`: four
files, none of them under `pty/windows/`).

### F9 — Negative control reproduced independently (severity: info, verified)

Ran here, not taken from the packet. `classify_event`'s two `if`s restored to main's order
(hang-up first, token second), nothing else changed:

```text
test event_loop::event_loop_tests::a_hung_up_child_notification_is_still_a_child_notification ... FAILED

thread '…' panicked at crates\local-shell\src\event_loop_tests.rs:50:9:
assertion `left == right` failed: a hung-up child notification (readable=true)
  left: Ignore
 right: ChildEvent

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 36 filtered out
```

Identical to the packet's record, down to the line and column. The worktree was restored
with `git checkout --` and `git status` was clean before the gate run.

### F10 — Routing is correct (severity: info, verified)

A new `BUG-0074` under `IN-0029`, rather than reopening `US-0136`, is what `docs/HARNESS.md`
prescribes:

- The acceptance-rework row applies to *"work that was just built but not yet accepted"*,
  and *"a new BUG is only for defects found in behavior that was already accepted/shipped."*
  `harness.db` (main checkout, read-only) has `US-0136` at `status='implemented'`,
  `last_verified_result='pass'`, notes *"Merged into main @020fdabc 2026-09-22"*, and
  `IN-0044` is recorded closed. Accepted and shipped, so the rework row does not apply.
- The defect is in `crates/local-shell/src/event_loop.rs`, which `US-0136` never touched.
  That loop is owned by `US-0083`, which `IN-0029` owns — the same intake that already owns
  `BUG-0066`, the previous failure of this very test, whose gap 2 named this branch in
  advance. `IN-0029` is the right intake.
- The bug/maintenance row's obligation ("locate the owning Intake, review the owning docs,
  update stale docs or record a no-change reason") is met in full: six owning docs reviewed,
  each with an explicit update-or-no-change verdict.

### F11 — Packet completeness vs `docs/templates/work.md` (severity: low)

Every template section is present, in order, dated `2026-09-23`, with well-formed
`HARNESS:STATUS` and `HARNESS:PROOF` blocks (`Implemented`; unit + integration + verify
ticked, e2e and platform correctly left unticked). Outcome, Scope, Acceptance,
Documentation (Owning Docs Reviewed / Documentation Action / Reconciliation), Verification
Plan and Evidence and Gaps are all substantive rather than placeholder. Two gaps:

- **The gate's own output is not in the packet.** The Acceptance box
  *"`pwsh scripts/ci-local.ps1` passes"* is ticked and `verify_command` claims it, but
  Evidence records only the two `cargo test` results. The gate's final line belongs there.
  It is supplied below and the claim is confirmed.
- **"Harness Record" is an extra section** not in the template. Harmless and, for a packet
  written in an isolated worktree, the only honest way to hand the row over. But
  `docs/HARNESS.md` says `harness.db` is authoritative and warns against maintaining the
  same operational fact in Markdown and the database; the block should be treated as a
  one-shot insert script, not kept in sync afterwards.

### F12 — The proposed SQL row is well-formed (severity: low; two convention deviations)

Checked against the live schema in the **main** checkout, read-only
(`D:\TrungKFC-Research\Rust\myTerm2\harness.db`; not written, not copied):

```text
story: 17 columns — id, title, created_at, risk_lane, contract_doc, packet_doc, status,
unit_proof, integration_proof, e2e_proof, platform_proof, evidence, verify_command,
last_verified_at, last_verified_result, notes, intake_id
```

- Column list matches the schema **exactly and in order**; 17 names, 17 placeholders,
  17 values. ✔
- Every `CHECK` satisfied: `risk_lane='normal'` ∈ {tiny, normal, high_risk};
  `status='implemented'`; the four proof flags are `1,1,0,0`; `last_verified_result='pass'`.
  `packet_doc` is `NOT NULL` and supplied; its path exists. ✔
- `intake_id=34` resolves to `IN-0029` (`intake.id=34`,
  `doc_path='docs/spec-intakes/IN-0029-vt-engine/IN-0029.md'`). ✔
- **Deviation 1:** every other story under intake 34 puts an *evidence document path* in
  `evidence` (`docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-00NN-verify.md`); this row
  puts a prose blob there. It should be
  `docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0074-verify.md` — this file — with the
  prose folded into `notes`.
- **Deviation 2:** `created_at` / `last_verified_at` are date-only. `BUG-0064` and
  `BUG-0066` in the same intake do the same, so it is precedented, but every row since
  2026-09-16 uses a full ISO timestamp.
- `INSERT OR REPLACE` would silently overwrite a pre-existing `BUG-0074`. No such row
  exists today, so it is harmless.

### F13 — `docs/terminal-backend.md` §6.2 is accurate, with one surviving imprecision (severity: low)

The added sentences are correct and match the code: the token is compared before the
hang-up guard (`classify_event`); the reaper drops its half of the pair immediately after
posting; a closed peer puts `EPOLLHUP` on the loop's half; `is_interrupt()` means "do not
do I/O on a dead PTY"; the source is level-triggered, which is why the loss was permanent.
The removal of the old clause — *"which the loop `continue`s past before it ever compares
the token"* — is exactly the sentence that had to go.

One residue: *"a `notify()` wake arrives keyless"*. It uses `NOTIFY_KEY = usize::MAX`, and
`Events::iter()` filters that key out (`polling-3.11.0/src/lib.rs:895`), so such a wake
does not arrive in the loop's iteration **at all**. The paragraph's conclusion ("the exit
has to come in on the key or it is not read at all") is right either way, but the wording
still implies the loop sees a keyless event and must step over it. One clause.

Cosmetic: the edit leaves an over-long line ("… (BUG-0074). Being generic over the PTY, the
loop is unit-tested with a").

### F14 — Adjacent, pre-existing, out of scope: the master's tail output is dropped at hang-up (severity: low)

Not a regression and not this packet's business, but it falls out of the same table.
`readable` is `flags.intersects(IN|HUP|ERR|PRI)` (F1.3), so on Linux a master that hangs up
with bytes still buffered in the line discipline arrives as `EPOLLIN|EPOLLHUP` — one event,
same shape as the notification. `classify_event(PTY_READ_WRITE_TOKEN, true, true)` is
`Ignore`, so that tail is never drained and the last output the shell produced before it
died does not reach the grid. Main behaved identically; the fix neither causes nor cures
it. If it is ever worth fixing, the shape is the mirror of this packet: drain first, then
honour the hang-up. Worth a packet only if a user ever reports a truncated final line on
Linux.

## Verification run here

Worktree at `4bcaad6f`, clean, `CARGO_BUILD_JOBS=6`.

### `cargo test -p oneterm-local-shell -p oneterm-core -p oneterm-vt`

All sections green; no failures, no unexpected ignores. The regression guard passes:

```text
test event_loop::event_loop_tests::a_hung_up_child_notification_is_still_a_child_notification ... ok
```

### `pwsh scripts/ci-local.ps1`

Green, exit code 0. Final line:

```text
ci-local: all checks passed.
```

(The script's own last line. Packets in this repository quote a "N steps / N test
sections" summary; `scripts/ci-local.ps1` does not print one, so what is quoted here is
what it actually emits.)

## Gaps in this verification

1. **No Linux here either.** MSVC target only, no WSL, no container. F1.2 (`unix_release_sock`
   / `unix_poll`) is cited from kernel source knowledge, not executed. The ubuntu job is
   still the only platform proof, and the packet's gap 1 stands unchanged.
2. **The original failure is not reproduced, by either party.** F3 and F4 are analysis of
   the mechanism, not measurement of it. If the next ubuntu run is green, that is consistent
   with the fix but does not by itself settle F3; if it fails again with `alive=true`, F4's
   "parked vs busy" question is the first thing to instrument, ahead of gap 4's strace.
3. **The five bash-through-ConPTY measurements were not re-run.** The `#[ignore]`d probe was
   removed before the commit, so those numbers (63-443 ms across five configurations) are
   not reproducible from the tree. They are plausible and consistent with `e2e_echo_output_rendered_in_snapshot`
   timings, and nothing in the fix depends on them — they only clear US-0136's environment,
   which F4 partly re-opens on a different axis (output volume, not shell behaviour).
4. **The `#[ignore]`d probe's removal was verified only from the diff.** `git show --stat`
   lists four files and no test-support file, and the ignored-test census step of the gate
   is green, so nothing stray was left behind. No stronger check was made.
