# US-0147 independent verification

- Date: 2026-09-28
- Commit under test: `6a69384a` (branch `perf/pty-chunk-cost`, one commit on main `42c44b0f`);
  baseline for measurements `b101026a` (main `42c44b0f` plus the zero-cost hotpath sites only)
- Packet: [`US-0147`](../US-0147-pty-chunk-cost.md); intake [`IN-0046`](../IN-0046.md); owning
  docs `docs/terminal-backend.md` § 5.3 and § 6.4; contracts `BUG-0070` (pump yield),
  `BUG-0074` (child exit ordering), `IN-0022` (broadcast input)
- Host: Windows 11 Enterprise 10.0.26200, toolchain 1.96.0, the worktree's own `target/`,
  `CARGO_BUILD_JOBS=3`, another agent building in parallel. Every app instance was launched
  by this verification with a private `USERPROFILE` and driven only through its own pid.

## Verdict: PASS (F1 must be fixed in the merge commit)

No lost release was found. Every path that can leave the hint flag claimed has a release
that runs without a frame (the view's 16 ms catch-up, driven by gpui's timer thread), and
every visible repaint is covered by the snapshot release, which runs before the snapshot
takes the engine lock. Hidden-tab and minimised-window output keeps being announced in the
GUI and is current on show/restore. The measured direction holds: PTY owner CPU per chunk
down, hints collapse from one per chunk to about two per frame, the conout allocation is
gone, UI thread down, frames and echo latency unchanged.

One defect appears only after merging main: US-0145's hidden-tab guard does not cover the
new catch-up timer, so a hidden tab with streaming output notifies its view (and so draws a
whole-window frame) once per hint again (F1). The merge with main is textually clean, so it
would land silently.

## Merge with main

`git merge --no-commit --no-ff main` on `6a69384a` (main at `27c97f5f`, BUG-0081 and US-0145
merged): automatic, no conflicts (`view.rs`, `IN-0046.md`, `high-level-design.md` and
`terminal-backend.md` auto-merged). Aborted after the F1 test below.

## Lost-release analysis

Set: `SharedSessionState::claim_repaint_hint` (swap, in `TerminalPump::post_repaint_hint`,
both `finish_batch` variants). Clear: (1) `PtySession::snapshot[_into]` before the engine
lock; (2) the view's detached catch-up task, `OUTPUT_CATCH_UP` (16 ms) after each handled
`Output`; (3) the pump, when `post_repaint` did not queue (full or closed).

Why the visible case cannot go stale: a batch whose claim fails had a claim before it with no
release in between. The next release is either a render snapshot (it takes the lock after
the release, so it holds the batch: the mutex orders the pump's unlock before it, and
coherence makes the swap read `false` otherwise) or the catch-up of a hint the view already
handled, whose `cx.notify()` has requested a frame that takes a snapshot later. The catch-up
repaints only on new stamps, but no visible frame depends on it. The frame's snapshot is
unconditional (`TerminalElement::prepaint`, `element.rs:143`).

| Path | Who releases | What if that path never runs | Result |
| --- | --- | --- | --- |
| (a) hidden tab (no render, US-0145: no notify) | catch-up timer | Timer runs off gpui's `ThreadedDispatcher` timer thread (`gpui-pre` 0.3.7 `threaded_dispatcher.rs:463`), independent of frames. On show, the new panel is painted fresh and snapshots. | PASS. Merged-state throwaway test: 5 hints, 5 releases. GUI: 6 s hidden while printing 10 lines/s; screenshot 0.4 s after the tab click shows `tick 102` (current); hints kept flowing (below). |
| (b) minimised window (no frames) | catch-up timer | As (a); `handle_event` notifies, no frame comes, the timer still fires. | PASS. GUI: 6 s minimised (`IsIconic` true) while printing; screenshot 0.4 s after restore shows `tick 185` (current); output kept repainting to `PRINTER-DONE` and the prompt. |
| (c) queue full | pump (`post_repaint` returned `false`) | A dropped hint is released at once, so the next batch posts again; an older `Output` already in the queue is handled and arms its own catch-up. | PASS. `a_hint_dropped_on_a_full_queue_does_not_block_the_next`. |
| (d) view dropped / tab closed with a hint out | none (task's `this.update` fails) | The view owns the only event receiver (`take_events` hands it out once; `TerminalView::new` has one caller, `terminal_panel.rs:386`), and `shutdown` closes the session. Nobody is left to announce to. | PASS (no consumer). |
| (e) Space split | each view its own | A split spawns a new session per Space; two views of one session do not exist (the second would get no receiver, pre-existing log path). | PASS (not reachable). |
| (f) SSH | same three | `ssh_main_task` calls the same `finish_batch(true).await`; `SshSession` is a `PtySession<SshSession>` (`ssh/src/session.rs:872`), so its snapshot releases too. | PASS by code; unit-covered only (packet gap). |
| (g) broadcast input | per session | Each session has its own flag and view; broadcast only writes. `tab_channel_helpers_*` and `oneterm-state` tests pass. | PASS. |
| (h) restart / reattach | n/a | No restart path; every spawn builds a new `SharedSessionState::new_alive()`. | n/a. |
| (i) child exit with a hint out | n/a | `publish_child_exit`: `Exited` and `Closed` are reliable (blocking send), independent of the flag; the suppressed final hint is covered by the notify those events cause (visible) or by the next show (hidden). | PASS. `child_exit_*` 3/3, `loop_child_exit_ends_the_session_and_stops_the_thread`. |

No busy loop: a catch-up task is spawned only per handled `Output`, never re-arms itself,
and a hint is posted only when a batch is fed with the flag free. Idle output = no tasks.

Latency (claim 2): lines that arrive while a hint is out are drawn by the frame that is
already pending (its snapshot runs after the release), so visible content is not delayed.
The 16 ms catch-up only bounds (i) the gutter stamp of such lines and (ii) how soon a
hidden/minimised view re-arms. Worst case for a stamp: 16 ms plus UI-thread delay, one frame
at 60 fps; the gutter shows seconds. Acceptable.

## Findings

### F1 (Medium, fix at merge): the catch-up re-notifies hidden tabs after merging US-0145

On main, `handle_event` skips `cx.notify()` for a view in a hidden tab (US-0145: the tab strip
reads that view, so every notify costs a whole-window frame). The catch-up task added here
calls `cx.notify()` whenever it stamped a new line, without that guard. Under streaming output
in a hidden tab that is one notify per hint, up to about 60 per second.

Evidence: throwaway `#[gpui::test] zz_verify_catch_up_in_a_hidden_tab` in
`crates/terminal-view/src/panel/tests.rs` on the merged tree (two tabs, the hidden one's
probe fed a line, `Output` emitted, a line fed, clock advanced 16 ms, five rounds): FAILED,
`hidden view: notifies over 5 hints = 5, releases = 5`. With the guard below: `notifies = 0,
releases = 5`, and `output_in_a_hidden_tab_does_not_notify_its_view` and
`output_that_came_without_a_hint_is_caught_up_a_frame_later` still pass. Not committed.

Fix (one line, in the merge commit, and keep the throwaway as a regression test):

```rust
if view.catch_up_with_output(cx) && !view.in_hidden_tab(cx) {
    cx.notify();
}
```

### F2 (Low, records): echo p50 improvement not reproduced

Commit message and the brief state echo p50 481 -> 383 µs. Two interleaved pairs of the
saved `fast-dev` test binaries here: before p50 256.8 / 263.2 µs, p95 2.22 / 2.19 ms; after
p50 250.3 / 265.4 µs, p95 2.29 / 2.17 ms. The packet already reads its own data as noise
("no growth"); the commit message should say "unchanged", not quote a p50 gain.

### F3 (Low, records): conout CPU per chunk not reduced here

Packet: conout 6.7 -> 5.7 µs per chunk. Here 5.49 -> 5.54 µs (5.6 % -> 5.7 % of a core).
`Ring::wake` avg 1.92 -> 1.72 µs and allocations 0.996 -> 0.000 per chunk are confirmed;
the thread-CPU gain is within noise. Also, the packet's after medians mix `after-1/-2` (the
superseded view variant) with `after-3`; the PTY side is identical, so the owner figures
stand, but the UI-thread median should be quoted from `after-3` (11.1 %) alone.

### F4 (Info): `docs/terminal-backend.md` § 5.3 `SessionEventSink` row

Still describes `post_repaint()` as fire-and-forget; it now returns whether it queued, which
the new paragraph relies on (the pump's third release). One clause.

## Measurements (one run each, release + `hotpath-profiling`, `research/hotpath-measure.ps1`)

Flood, 300,000 `echo` lines, `b101026a` -> `6a69384a`:

| | before | after |
| --- | ---: | ---: |
| Run / chunks | 29.4 s / 299,544 | 29.2 s / 300,084 |
| PTY owner | 7.5 % = **7.36 µs/chunk** | 4.8 % = **4.66 µs/chunk** (-37 %) |
| `loop::drain_pty` avg | 5.28 µs | 1.79 µs |
| `finish_batch_blocking` avg | 3.64 µs | 132 ns |
| Hints posted (`post_repaint` calls) | 299,428 | **2,025** |
| conout thread | 5.6 % = 5.49 µs/chunk | 5.7 % = 5.54 µs/chunk |
| `Ring::wake` allocs/chunk (`-alloc` build, count) | 0.996 | **0.000** |
| UI thread | 16.2 % | 10.4 % |
| Frames (`TerminalElement::prepaint`) | 987 | 1,019 |

No loop, pump or conout site allocates per chunk after the change (the packet did not move
to the owner: registration is once per session, `PollMode::Level`, `event_loop.rs:293`).

Two-tab TUI load, 30 s (tab switch every 10 s): frames 1,532 -> 1,555; feeds 2,954 / 3,032;
hints 2,929 -> 2,307; UI thread 9.6 % -> 9.2 %. Hints keep flowing across the hidden
intervals.

GUI walk (`6a69384a`, second tab printing 200 lines at 100 ms, hidden 6 s then minimised 6 s):
377 feeds, 289 hints, 246 frames; had the flag stuck when the tab was hidden (after about 30
lines) hints would have stopped near that count. Screenshots after show, after restore and at
the end all show the current line.

Keystroke echo: see F2.

## Contracts

- BUG-0070 `a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`: **20/20**
  standalone; `a_flooding_loop_hands_the_engine_to_a_waiting_frame` 5/5.
- BUG-0074: `classify_event` table and `child_exit_*` / `loop_child_exit_*` pass.
- IN-0022: panel channel tests and `oneterm-state` 41/41 pass.
- `cargo test -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh -p oneterm-terminal-view
  -p oneterm-state`: 221 / 35 (+3 ignored) / 109 / 390 (+3 ignored) / 41, all pass.
- Ignored-test census: `session::session_tests::keystroke_echo_latency` is `cfg(windows)` and
  recorded without a platform tag, which is what the census expects: it is a union, the check
  is a subset test, and a Linux/macOS runner that does not collect it reports it as
  informational only.
- `frame_time_under_output` drives `FakeTerminalSession` and runs none of this code
  (confirmed: `TerminalRender::release_repaint_hint` is a no-op counter there).

## Windows ring packet reuse

`CompletionPacket` is `Pin<Arc<IoStatusBlock>>` with `derive(Clone)` (`polling` 3.11.0
`iocp/mod.rs:713`); `post` hands one strong count to the port via `Arc::into_raw` and the
waiter takes it back with `from_raw` (`iocp/port.rs:95-100`), so N queued clones are N counts
on one allocation. `polling`'s own `notify` posts `self.notifier.clone()` the same way
(`mod.rs:510`). The packet is built from the same `event` in `register`, so the key is
identical to before. `deregister` sets the interest to `None` and `wake` returns early on
`None`, so nothing is posted after deregistration; oneshot modes still clear it after a post.
A packet still queued when the poller drops keeps its own count, as before the change.
`wake_ups_posted_before_a_poll_all_arrive` passes.

## Records

- Packet follows `docs/templates/work.md` (all sections, `Created: 2026-09-28`, owning docs,
  evidence, gaps, Handoff). F2/F3 wording to adjust.
- `docs/terminal-backend.md` § 5.3 paragraph and § 6.4 bullet are accurate against the code
  (F4 aside); after merging, § 5.3's "the view's release keeps an inactive tab hearing from
  its pump" still holds.
- `scripts/ignored-tests.txt`: entry present and sorted.

## Gate

`pwsh scripts/ci-local.ps1` on `6a69384a` (plus this file), `CARGO_BUILD_JOBS=3`, after
deleting `target/release`: exit 0, final line `ci-local: all checks passed.` The first attempt
stopped in `cargo test --workspace` on `rustc-LLVM ERROR: out of memory` while compiling the
`windows` crate (another agent building at the same time); the re-run passed unchanged.

## Gaps

- F1 is proven by a gpui unit test, not by a frame count in the GUI on the merged build.
- SSH not exercised against a server; the argument is by code (same pump, same session type).
- One run per measurement on a shared machine; directions are consistent with the packet's
  interleaved runs, magnitudes smaller (owner -37 % here vs -60 %).
- Linux/macOS not run (the Windows ring change is Windows-only; the flag is platform-neutral).
