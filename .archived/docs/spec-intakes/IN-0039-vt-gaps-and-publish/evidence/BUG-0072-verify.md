# Independent verification: BUG-0072, a child that exits before its poller is registered

Verifier: independent agent, worktree `.claude/worktrees/agent-a53cdf51e7938d13d`, reset to
`fix/child-exit-watcher-late-registration` `b3a148ca` (base `main` `410136e0`, two commits:
`b2e3eeff` the packet, `b3a148ca` the fix). Nothing pushed, nothing touched outside this
worktree, `harness.db` not read or written. `CARGO_BUILD_JOBS=6` throughout. Windows 11,
`cargo 1.96.0`, `polling 3.11.0`.

Diffstat against `main`: 4 files, +446 / -27 -- `crates/vt/src/pty/windows/child.rs` +148/-27
(production +30/-15 and the rest tests and doc), the packet, the intake row, and
`docs/terminal-backend.md` § 6.3. Matches the packet's claim.

## Verdict

**PASS WITH NOTES.** The defect is real, is a product defect, and is closed. I reproduced the
lost wake on unfixed `main` with the implementer's own probe shape (`woken=false` with the exit
already sitting in the channel, 4/4 runs), and the same probe on `b3a148ca` reports
`woken=true` 5/5. Every structural claim in the packet holds when read against the code: the
`send` still precedes the `post`, one mutex covers both halves of the hand-off, `deregister`
clears the interest and keeps the flag, `Interest` holds an `Arc<Poller>` so the callback can
never post into a freed poller, and `Drop`'s `UnregisterWaitEx(.., INVALID_HANDLE_VALUE)`
cannot deadlock against the new lock. The Unix reaper has no equivalent hole. The docs are
accurate about what they changed. Tallies below all reproduce.

The notes are one gap that the packet declared closed-by-argument and that I measured open and
cheaply closable (F1), and three record/latent items. None is a defect in the shipped fix.

## Findings

| # | Severity | Finding |
| --- | --- | --- |
| F1 | medium | **The packet's first Gap is wrong on its own terms: pinning "enqueue before wake" needs no production seam, and a test for it works.** `BUG-0072-...md:283-288` says pinning the ordering "would mean injecting the callback's two steps behind a test seam in production code ... Not taken". It would not. `child.rs:135-138` already sequences `send` -> `exited = true` under the lock, and the new test already reads that flag (`child.rs:375`). So "the flag implies the event" is an observable, seam-free statement of exactly the ordering: spin on `sender.notify().exited`, then require `next_event()` to be `Some` **before** registering anything. Measured: 30/30 iterations pass on `b3a148ca`, and against a notify-before-enqueue mutation (m3 below) the assertion **fails 10/10** while the five shipped watcher tests detect it **0/10**. One looped test (~15 lines, no production change) converts a documented gap into a guarded one. Recommended, not required for acceptance. |
| F2 | low | **`docs/terminal-backend.md` § 6.2 says the child watcher calls `poller.notify()`; it does not, and if it did, BUG-0072 would still be open.** § 6.2 ("**Current implementation**"): "every `ShellNotifier::send` and the child watcher call `poller.notify()`". `ShellNotifier::send` does (`crates/local-shell/src/event_loop.rs:111-113`); the watcher posts a keyed `CompletionPacket` (`child.rs:92`). A `Poller::notify()` wake arrives as an interrupt with no key, and `event_loop.rs:373-375` `continue`s past `event.is_interrupt()` before it ever compares the token -- so the sentence describes a mechanism that would lose the exit exactly the way this packet's defect did. Pre-existing on `main`, and now sitting one paragraph above the § 6.3 bullet the packet rewrote. One sentence to fix while the file is open. |
| F3 | low | **After the exit, every `reregister` posts another wake -- a busy-loop an external embedder can hit, and the docs do not say so.** On Windows `register` *is* `reregister` (`crates/vt/src/pty/windows.rs:66-73`), and `reregister` calls `ChildExitWatcher::register`, which now posts unconditionally when `exited` is set. No shipped consumer is affected: `crates/local-shell/src/event_loop.rs:281`, `crates/tools/src/bin/vt-esctest.rs:148` and `crates/tools/src/bin/pty-throughput.rs:51` each register exactly once and nothing in the workspace calls `reregister` at all. But `oneterm-vt` is consumed as a git dependency and `EventedReadWrite::reregister` is part of the surface `IN-0039` owns, and the alacritty-shaped loop this API descends from calls `reregister` every iteration to toggle write interest. An embedder that does that *and* keeps polling after the exit (to drain trailing output) now spins at 100% CPU where before the fix it merely parked. § 6.3 says "registering twice around an exit posts a second, eventless wake" -- true, but it does not say that `reregister` is a registration, which is the form an embedder would actually write. A sentence in § 6.3, or making `register` idempotent for an interest that is already installed, closes it. |
| F4 | low | **Mutation m2 fails on the test's own precondition, not on the product's contract.** With `notify.exited = true` deleted, `an_exit_before_registration_still_wakes_the_poller` panics at `child.rs:376` -- "the wait callback never ran for an already-exited child" -- i.e. on the spin that the test uses to *order* itself, 5 s before it ever reaches the wake assertion. Reproduced 3/3, exactly as the packet's table says. The behaviour that a user would see under m2 (a late `register` never waking) is pinned by m1, not by m2. The packet presents the two as equal halves; they are not. Record accuracy only -- the fix and the test are both fine. |
| F5 | info | **No deadlock is structurally reachable, and I measured it.** `Notify::post` is one `PostQueuedCompletionStatus` (`polling-3.11.0/src/os/iocp.rs:197` -> `iocp/mod.rs:514` -> `iocp/port.rs:199`): no user-space lock, no callback re-entry, and it cannot block. `ChildExitWatcher` is `!Sync` (its `mpsc::Receiver`), so `register`/`deregister` can only run on the thread that owns the watcher; the only cross-thread contention on the new mutex is wait-thread vs owner-thread. `Drop` takes no lock before `UnregisterWaitEx(.., INVALID_HANDLE_VALUE)`, and the owner thread that drops cannot simultaneously hold the lock. 40 children x 2000 register/deregister cycles racing live callbacks: no hang, no panic. |
| F6 | info | **Every line citation in the packet checks out.** `child.rs:88` (the pre-fix `send`), `:96` (the pre-fix `post`), `:159` (the pre-fix `register`) and `:256` (the `assert!` in the reported panic) are all correct against `main:crates/vt/src/pty/windows/child.rs`. `event_loop.rs:281` and `:377-378` are correct. The spawn-to-register window is as described: `event_loop.rs:189-221` runs `child_pid()`, `set_identity`, `logging().start()` (which opens the session log file) and a `sync_channel` send between `PseudoConsole::spawn` and `run()`'s `register`. |

## What was verified, and how

### 1. The defect, reproduced on unfixed `main` (attack 1)

`main`'s `crates/vt/src/pty/windows/child.rs` restored into this worktree, plus the
implementer's probe shape as a temporary test (exit a child, reap it, build the watcher, sleep
250 ms, register, poll 2 s):

```
PROBE woken=false event=Some(Exited(Some(ExitStatus(ExitStatus(5)))))
panicked at crates\vt\src\pty\windows\child.rs:357:9:
the exit must wake the poller on the child token
```

4 runs, `woken=false` on all 4. The exit is in the channel; no packet was ever posted. That is
the packet's diagnosis, verbatim, and it is a product defect and not a test defect for the
reason the packet gives and that I re-checked in the code: `event_loop.rs:311` is
`self.poll.wait(&mut events, None)` -- **no timeout** -- and `event_loop.rs:376-386` reads
`next_child_event()` only under `PTY_CHILD_EVENT_TOKEN`. A packet that is never posted is an
exit that is never reported, for the life of the tab.

The same probe on `b3a148ca`: `woken=true` **5/5**.

> Method note for anyone reproducing this: `Copy-Item` preserves the source file's
> `LastWriteTime`, so dropping a file into the tree can leave cargo with a source **older**
> than the last build and no rebuild happens. My first "the fix also fails" reading was that,
> not a defect; every run recorded here touched the file's mtime afterwards.

### 2. Lock discipline, drop ordering, poller liveness (attacks 2, 4, 5)

Read, then measured with temporary tests (removed before committing this file).

- `post()` **is** called with the mutex held, on both sides (`child.rs:139`, `:211`). That is
  safe here: `Poller::post` bottoms out in a single `PostQueuedCompletionStatus`
  (`polling-3.11.0/src/iocp/port.rs:199-216`) -- it takes no lock the poller's `wait` could be
  holding, cannot block, and cannot re-enter the wait-thread callback. `Poller::notify()` is
  never called by this module.
- Deadlock between the wait-callback thread and `register`: not reachable. Neither side takes a
  second lock while holding this one, and `ChildExitWatcher` is `!Sync`, so `register` and
  `deregister` are confined to the owning thread. Stress: **40 children x 2000
  register/deregister cycles** on a second thread while the wait callbacks fired -- no hang, no
  panic.
- `Drop`: `UnregisterWaitEx(.., INVALID_HANDLE_VALUE)` waits for a running callback, and that
  callback may be mid-`post`. It cannot deadlock -- `Drop` acquires no lock -- and it cannot be
  a use-after-free of the poller: `Interest` holds an **`Arc<Poller>`**, not a raw handle
  (`child.rs:68-71`). Measured: register a poller, drop the caller's only other `Arc`
  (`Arc::strong_count == 2` before the drop, so the recorded interest is the last owner), then
  let the child exit -- the callback posts into a poller kept alive solely by the interest, and
  the exit is still readable. No crash. The `Arc<Poller>` is released when `deregister` clears
  the interest or when `sender` drops, which is **after** `UnregisterWaitEx` returns, because
  `Drop::drop` precedes field destruction.
- `exited` vs `terminate_if_still_running`: no interaction. The grace-period path reads the
  process handle (`WaitForSingleObject`, `child.rs:240`), never the flag, and takes no lock.
  `DEC-0016`'s behaviour is untouched by this change.

### 3. Deregister/re-register around an exit, and consumer tolerance (attack 3)

Measured wake counts against `b3a148ca`:

```
VERIF wakes: first=1 first_event=Some(Exited(Some(ExitStatus(ExitStatus(5)))))
             after_deregister=0 second=1 second_event=None
```

Exactly what § 6.3 promises: the first `register` after an exit posts one wake carrying the
event, `deregister` posts nothing, a second `register` posts one more wake that is eventless.
Both real consumers tolerate the eventless wake -- `crates/local-shell/src/event_loop.rs:377-385`
(`if let Some(ChildEvent::Exited(..)) = ... { .. return; } continue;`) and
`crates/tools/src/bin/vt-esctest.rs:169-171` have the same shape. Neither ever calls
`reregister`; see F3 for the embedder that might.

### 4. Enqueue before wake (attack 6)

The ordering still holds in the shipped code: `send` at `child.rs:135`, `post` at `:139`. The
implementer declined to pin it. A cheap seam **does** exist and I measured it -- see F1. The
numbers:

| | shipped `b3a148ca` | mutation m3 (notify before enqueue) |
| --- | --- | --- |
| the five shipped `pty::windows::child::` tests | pass | **pass 10/10 -- the regression is invisible** |
| "the `exited` flag implies the event is queued", 30 iterations | pass 30/30 | **fail 10/10** |

So the packet's Gap is real as written (the shipped suite does not catch m3) and is avoidable
without touching production code.

### 5. The Unix reaper has no equivalent hole (attack 7)

Confirmed by reading `crates/vt/src/pty/unix.rs`. `reap_in_background` (`:205-220`) does
`events.send(ChildEvent::Exited(status))` **then** `write_all(&mut waker, &[1])` -- the same
enqueue-before-wake ordering. The wake is a byte on a `UnixStream` pair registered
`PollMode::Level` (`:247-251`, and identically in `reregister` at `:263-267`), so a
registration that arrives after the byte was written still finds the socket readable and wakes
immediately: level-triggered readiness is state, not an edge, which is precisely what the
Windows completion packet is not. `next_child_event` (`:285-295`) drains the byte before
`try_recv`, which is safe for the same ordering reason. Nothing to change, as the packet's
Scope says. Not run -- no Unix host here.

### 6. Documentation (attack 8)

`docs/terminal-backend.md` § 6.3 "Child exit" is accurate: both orderings match the code, the
"no timeout / only on the token" justification matches `event_loop.rs:311` and `:377`, the
spawn-to-register window ("the owner thread still opens the session log file") matches
`event_loop.rs:189-221`, and the "second, eventless wake" is measured above. The module doc
(`child.rs:15-26`) says the same two things at the code. `crates/vt/src/pty/mod.rs`'s "Both are
race-free" is now true as claimed. See F2 for the one sentence in § 6.2 that is not.

## Commands and tallies

All under load: a second `pwsh` looping `cargo build --workspace --all-targets` then
`cargo clean` in this worktree into a scratch `CARGO_TARGET_DIR`, six build jobs, running
throughout sections 1-4 and the gate.

```
probe on main 410136e0 (unfixed)                          woken=false 4/4   (defect reproduced)
probe on b3a148ca (fixed)                                 woken=true  5/5

cargo test -p oneterm-vt --lib pty::windows::child::      20 runs x 5 tests
                                                          passed=20 failed=0   (100 executions)

cargo test -p oneterm-vt -p oneterm-local-shell           22 test binaries, 0 failed
                                                          (vt lib 561 passed, local-shell 33 passed)

mutation m1  register stops posting a recorded exit       an_exit_before_registration_still_wakes_the_poller
                                                          FAIL 3/3 "registering after the exit must still wake the poller"
mutation m2  callback stops recording the exit            FAIL 3/3 "the wait callback never ran for an already-exited child"
mutation m3  callback posts before it sends               shipped suite: FAIL 0/10 (undetected)
                                                          F1's assertion:  FAIL 10/10

verifier probes (temporary, removed before commit)
  wake count across deregister/re-register                1 / 0 / 1, second eventless -- pass
  40 children x 2000 register/deregister vs live callback no deadlock -- pass
  interest keeps the poller alive (strong_count 2)        no UAF -- pass
  exited-implies-queued, 30 iterations                    pass 30/30

pwsh scripts/ci-local.ps1                                 30 steps, exit 0
                                                          "ci-local: all checks passed."
```

The gate was run **without** the load loop, after one attempt with it had to be discarded:
`cargo fmt` and both `cargo clippy` passes were green, then `cargo test --workspace` lost five
compilations at once -- `oneterm-vt` (lib test and `verify_us0105`), `oneterm-ssh`, and the
third-party `windows` 0.61.3 and 0.62.2 -- every one of them a `std::alloc::rust_oom` abort
inside `rustc`. Twelve concurrent `rustc` jobs (the gate's six and the load loop's six) is a
machine limit, not a finding about this branch; nothing in the failure touched the changed
file. Re-run on an idle machine below.

## Gaps in this verification

- **Windows only.** The Unix reaper was read, not run; no Unix host was available. CI covers it.
- **Watcher-level, not session-level.** Like the packet, I proved the fix at `ChildExitWatcher`.
  I did not drive a real `LocalSession` whose shell exits inside the spawn-to-register window;
  reproducing that through a shell is the timing dependence the packet exists to remove.
  `crates/local-shell`'s loop tests use a stub PTY with no such race.
- **`reregister` (F3) is reasoned, not run.** No workspace consumer calls it, so there was
  nothing to measure; the busy-loop is derived from `windows.rs:66-73` plus the new
  unconditional post, not observed.
- **`harness.db` untouched.** Status and proof rows were not read or written from here.
- The temporary probe and mutation sources live only in this session's scratchpad; the worktree
  is clean apart from this file.
