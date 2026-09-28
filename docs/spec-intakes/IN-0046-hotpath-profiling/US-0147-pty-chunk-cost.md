# Work: the PTY owner loop pays less per ConPTY chunk

ID: US-0147
Intake: IN-0046
Created: 2026-09-28

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

- Change type: maintenance (performance; no behaviour change the user sees)
- Risk lane: normal. The loop is on the interactive-latency path (keystroke echo, frame
  hand-over), so latency is measured before and after, not assumed.
- Spec Intake, when required: [`IN-0046`](IN-0046.md), candidate 5 of
  [`research/hotpath-evaluation.md`](research/hotpath-evaluation.md) § 5

## Outcome

1. The PTY owner loop's cost per ConPTY chunk is **attributed**: hotpath sites on the poll
   wait, the command servicing, the engine lock and unlock, the batch finish and its repaint
   hint, the whole per-wake drain, and on the Windows conout side (the pipe read, the push
   into the ring, the completion-packet post), measured under the 300k-line flood.
2. The top attributed item(s) are cut where that can be done without adding latency: no
   timer on the output path, a lone chunk still fed and announced immediately.

## Scope

- [x] In scope:
  - `cfg_attr(feature = "hotpath-profiling", ...)` sites in `crates/local-shell/src/event_loop.rs`
    (a `site!` macro over `hotpath::measure_block!`), `crates/terminal/src/backend/{pump.rs,event_sink.rs}`
    and `crates/vt/src/pty/windows/pipe.rs`. Zero cost without the feature, like US-0142's.
  - The fix: at most one repaint hint out (`crates/terminal`), its release by the view
    (`crates/terminal-view`), one completion packet per registration (`crates/vt`).
  - A keystroke-echo latency measurement through a real ConPTY shell (ignored test).
  - Raw reports under `research/raw/us0147-*`; `research/us0147-per-chunk.py`.
- [x] Out of scope: the UI thread's other per-frame work (US-0145), the SSH pump's own
  loop (it shares the hint change through the pump), the 1 MiB buffer and
  `MAX_LOCKED_READ`, anything that delays a chunk to batch it.

## Acceptance

- [x] Per-chunk attribution table from a release hotpath build under the 300k flood, raw
  JSON committed.
- [x] PTY owner CPU per chunk down (-37 % in the verifier's pair, -60 % in the medians
  here); conout thread CPU unchanged (its per-chunk allocation is gone); feed count reported
  (unchanged, see "Why not coalescing").
- [x] BUG-0070's `a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks` 20/20
  standalone; `a_flooding_loop_hands_the_engine_to_a_waiting_frame` 5/5; `classify_event`
  and the child-exit loop tests pass; the real-shell `session_tests.rs` (conout re-arm) pass.
- [x] Keystroke echo latency does not grow (10 interleaved pairs, see Evidence).
- [x] TUI pacing: frames drawn in the two-tab TUI load identical before and after (1307 /
  1308 vs 1308 / 1310); `frame_time_under_output` unchanged within noise (it runs none of
  this code).
- [x] Unit tests: the hint collapses and is re-posted after a release; a hint dropped on a
  full queue does not block the next; a snapshot releases it; the view releases it and
  stamps a frame later; several wake-ups on one reused packet all arrive.
- [x] Gates below, full `ci-local` included.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` § 5.1 (demand/yield handshake, `MAX_LOCKED_READ`, the conout
  re-arm constraint), § 5.3 (the pump layer: one repaint hint per batch, events before
  the hint), § 6.2 "Current implementation" (the owner loop), § 6.4 (re-render perf: one
  coalescible `Output` per batch).
- `docs/spec-intakes/IN-0032-terminal-crate-tidy/BUG-0070-pump-yield-test-only-passes-under-load.md`
  — the yield-bound contract and its test.
- `docs/spec-intakes/IN-0029-vt-engine/BUG-0074-child-exit-lost-when-its-notification-hangs-up.md`
  — token-before-hang-up ordering in `classify_event`.
- [`high-level-design.md`](high-level-design.md) — where sites go (batch granularity).
- `crates/vt/src/pty/windows/pipe.rs` module docs — the level-triggered ring and its re-arm.

### Documentation Action

- Update required: `docs/terminal-backend.md` § 5.3 (pump row; the new "at most one
  `Output` hint is out" paragraph: who claims, the three releases, why) and § 6.4 (one hint
  per batch -> at most one, none while one is out); the HLD diagram (new sites);
  `IN-0046.md` candidate list.
- No change: § 5.1 (the yield and the re-arm are untouched: the loop still reads until the
  pipe is empty and yields at chunk boundaries); § 6.2 (the loop's steps are unchanged; the
  hint rule lives in § 5.3, which § 6.2 already defers to); `pipe.rs` module docs (the
  ring's semantics are unchanged; the reused packet is documented on the field).

### Reconciliation

- Changed: `docs/terminal-backend.md` § 5.3 and § 6.4; [`high-level-design.md`](high-level-design.md)
  diagram; [`IN-0046.md`](IN-0046.md) candidate list (US-0147 linked and done).
- The no-change reasons above still hold. `oneterm-vt`: no public item changed (the
  field is private), so its CHANGELOG, README and API surface are untouched.

## Context

- US-0142 flood: 298,706 feeds for 300,000 lines; `PTY owner` 7.3 % of a core over 29.1 s.
- The conout ring already concatenates whatever the pipe thread pushed before a read, so a
  read takes every queued chunk at once; feeds equal non-empty reads.

## Plan

- [x] Sites; release hotpath build of the sites alone (the "before"); flood timing and
  allocation-count runs; per-chunk table.
- [x] Decide the fix from the table (below).
- [x] Fix, tests, after-measurements (flood, echo latency, frame time, TUI, BUG-0070 x20).
- [x] Docs; gates; commit on `perf/pty-chunk-cost`.

## Decisions

None recorded as a DEC: the hint rule is a pump-layer contract and is written where the
pump's contract lives (`docs/terminal-backend.md` § 5.3).

## Verification Plan

- Release `oneterm-app --features hotpath-profiling` (and `hotpath-profiling-alloc` with
  `HOTPATH_ALLOC_METRIC=count`), `research/hotpath-measure.ps1 -Mode Flood`, own pid and a
  private home; before (sites only) and after (sites + fix), interleaved.
- Echo latency: ignored real-shell test, the two test binaries run interleaved.
- `frame_time_under_output` (fast-dev) before and after; the two-tab TUI load (30 s);
  BUG-0070 test 20 runs.
- `cargo test -p oneterm-local-shell -p oneterm-terminal -p oneterm-vt`; clippy with and
  without the feature; `check-doc-paths`, `check-english`; full `ci-local`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Windows 11, toolchain 1.96.0, release profile (fat LTO), the worktree's own `target/`,
`CARGO_BUILD_JOBS=2..3`, another agent building on the same machine throughout (so
absolute figures move 30-50 % between runs; compare interleaved pairs). Flood protocol:
`research/hotpath-measure.ps1 -Mode Flood` (300,000 `echo` lines from `cmd`, 1280x800, one
tab, private home, own pid). Per-chunk tables: `python research/us0147-per-chunk.py <time>
[<count>]`. A chunk is one `Pump::advance`.

### Step 1: attribution (before, sites only)

`raw/us0147-time-before-1.json` + `raw/us0147-count-before.json` (298,458 chunks, 33.8 s).
Times are wall time per call (inclusive of nested sites); allocations are exclusive.

| Thread | Site | Calls / chunk | Avg | µs / chunk | Allocs / chunk |
| --- | --- | ---: | ---: | ---: | ---: |
| PTY owner | `loop::poll_wait` (blocked in `GetQueuedCompletionStatusEx`) | 1.00 | 104.75 µs | 104.77 (wall, idle) | 0 |
| PTY owner | `loop::commands` (resize, input queue, writes) | 1.00 | 95 ns | 0.10 | 0 |
| PTY owner | `loop::drain_pty` (everything below) | 1.00 | 7.24 µs | **7.24** | 0 |
| PTY owner | `pty_read` (ring read; the second one finds it empty) | 2.07 | 96 ns | 0.20 | 0 |
| PTY owner | `loop::lock` (`try_lock`) | 1.07 | 77 ns | 0.08 | 0 |
| PTY owner | `Pump::advance` (feed + drain) | 1.00 | 1.38 µs | 1.38 | 0 (0.034 in `Parser::advance`: scrollback filling) |
| PTY owner | `loop::unlock` | 1.00 | 45 ns | 0.05 | 0 |
| PTY owner | `Pump::finish_batch_blocking` | 1.00 | 4.99 µs | 4.99 | 0 |
| PTY owner | ↳ `SessionEventSink::post_repaint` | 1.00 | 4.83 µs (p50 4.10) | **4.82** | 0 |
| conout | `read_pipe` (blocked in `ReadFile`) | 1.01 | 108.44 µs | 108.99 (wall, idle) | 0 |
| conout | `push` | 1.01 | 3.10 µs | 3.12 | 0 |
| conout | ↳ `Ring::wake` (`PostQueuedCompletionStatus`) | 1.00 | 2.81 µs | **2.82** | **1.00** |

Thread CPU (hotpath thread table): `PTY owner` 10.4 % of a core = **11.79 µs per chunk**,
`oneterm-vt-pty-conout` 6.3 % = **7.14 µs per chunk**. The owner's CPU outside
`drain_pty` + `commands` (about 4.4 µs) is the wake-up and the park in the poll wait
(kernel time, not attributable by a wall-clock site) plus the profiler's own cost.

Reading: two thirds of the loop body is `post_repaint` — `async_channel::try_send` waking
the view's parked event task, i.e. gpui's `PostMessageW` to the UI thread, **once per
line**, since ConPTY hands `cmd`'s output over one line per chunk and the UI is idle
enough to be parked for each. The only per-chunk allocation is on the conout thread:
`polling::os::iocp::CompletionPacket::new` builds a pinned `Arc` per wake-up (freed on the
owner thread).

### Why not coalescing (a), and why (b) had to mean "until the UI caught up"

- (a) Feeds per wake-up are 1.00 (`drain_pty` 298,459 vs `Pump::advance` 298,458): when the
  owner wakes, the ring holds one line, and the ring already hands over everything queued
  in one read. There is nothing queued to coalesce without waiting, and waiting is out.
- (b) with "consumed" meaning "the UI received the hint" saves nothing here: the UI is
  parked at almost every post (`post_repaint` p50 4.1 µs is the wake itself), so the
  previous hint has always been received. What makes a second hint redundant is the frame:
  it reads everything fed up to its snapshot. So the hint is released by the **snapshot**
  (and by the view a frame later, for a view that draws none).
- (c)/(d) The one per-chunk allocation is the completion packet; the hand-over granularity
  itself is the producer's (one `ReadFile` per line) and was left alone.

### What changed

- `SharedSessionState::claim_repaint_hint` / `release_repaint_hint` (`crates/terminal`):
  one `AtomicBool`. `TerminalPump::finish_batch[_blocking]` posts `Output` only on a
  successful claim, and releases it again when the queue had no room
  (`SessionEventSink::post_repaint` now returns whether it queued).
- `PtySession::snapshot[_into]` releases the hint before it takes the engine lock;
  `TerminalRender::release_repaint_hint` (default no-op) lets the view release it too.
- `TerminalView` (`crates/terminal-view`): on an `Output` it stamps as before and schedules
  a catch-up `OUTPUT_CATCH_UP` (16 ms) later: release, re-stamp the gutter for lines that
  arrived without a hint, and repaint **only** when that stamped something new. Without
  that last condition the first build of this change drew a frame per hint in a TUI the UI
  keeps up with (2,118 frames vs 1,307 in the 30 s two-tab TUI load; that report was
  overwritten by the re-run below).
- `Ring` (`crates/vt/src/pty/windows/pipe.rs`): the `CompletionPacket` is built once per
  registration and every wake-up posts a clone, the way `polling` posts its own notify
  packet.

Ordering and latency: a batch with no hint out posts at once, so the first output after a
quiet spell (a keystroke echo) is announced exactly as before; reliable events still leave
every batch, before any hint; a batch fed after a snapshot's release posts again, one fed
before it is in that snapshot (the release happens before the snapshot takes the lock).

### Step 2: before / after (flood, timing build)

Interleaved release builds of `b101026a` (sites only) and this change. `raw/us0147-time-*.json`.
`after-1`/`after-2` ran the first view variant (repaint on every catch-up); `after-3` the final
one. The PTY-side code is the same in all three.

| Run | Run s | Frames | Feeds | Hints posted | UI thread | PTY owner | µs / chunk | conout | µs / chunk | `drain_pty` avg | `post_repaint` avg | `Ring::wake` avg |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| before-1 | 33.8 | 1214 | 298,458 | 298,376 | 20.4% | 10.4% | 11.79 | 6.3% | 7.14 | 7.24 µs | 4.83 µs | 2.81 µs |
| before-2 | 27.4 | 895 | 298,325 | 298,234 | 15.9% | 8.5% | 7.81 | 6.0% | 5.51 | 5.15 µs | 3.45 µs | 1.86 µs |
| before-3 | 29.3 | 997 | 296,261 | 296,123 | 17.9% | 8.7% | 8.61 | 6.4% | 6.33 | 6.70 µs | 4.06 µs | 2.20 µs |
| before-4 | 35.9 | 1342 | 296,612 | 296,482 | 23.8% | 11.3% | 13.69 | 8.1% | 9.81 | 9.98 µs | 6.89 µs | 3.53 µs |
| after-1 | 29.5 | 1040 | 299,294 | 2,006 | 12.7% | 4.2% | 4.14 | 5.8% | 5.71 | 1.96 µs | 12.34 µs | 2.04 µs |
| after-2 | 29.2 | 948 | 299,116 | 1,786 | 10.8% | 4.2% | 4.09 | 5.3% | 5.17 | 1.87 µs | 12.57 µs | 2.24 µs |
| after-3 | 31.3 | 1047 | 299,299 | 2,051 | 11.1% | 4.8% | 5.02 | 6.0% | 6.27 | 2.20 µs | 16.64 µs | 2.24 µs |

Medians, before -> after: **PTY owner 9.6 % -> 4.2 % of a core, 10.2 -> 4.1 µs per chunk
(-60 %)** (the PTY side is the same code in all three after runs; the independent
verification measured 7.36 -> 4.66 µs, -37 %, in one pair); `drain_pty` 7.0 -> 2.0 µs;
hints posted 298k -> 2k (about two per frame). Conout thread CPU: **unchanged** (6.7 -> 5.7
µs per chunk here, 5.49 -> 5.54 µs in the verification pair: within noise). UI thread
19.2 % (median of the four before runs) -> 11.1 % (`after-3` only; `after-1`/`-2` ran the
superseded view variant and are left out of this figure); the verification pair gave 16.2
-> 10.4 %. The event task no longer wakes per line. Frames drawn and feeds unchanged. Allocation count (`raw/us0147-count-*.json`):
`Ring::wake` 298,215 -> **0** allocations; no other loop, pump or conout site allocates per
chunk before or after.

### Latency and pacing

- Keystroke echo (`keystroke_echo_latency`, 400 keys each, `raw/us0147-echo-latency.txt`):
  10 interleaved pairs of the two saved test binaries: **unchanged**. p50 median of the 10
  runs: before 481 µs, after 383 µs, but the verification's two pairs gave 257 / 263 µs
  before and 250 / 265 µs after, so the p50 difference is noise, not a gain; p95 median:
  before 2.20 ms, after 2.27 ms (verification: 2.22 / 2.19 vs 2.29 / 2.17 ms). Paired p95 differences
  (after - before) run from -0.20 to +1.01 ms with a median of +0.03 ms; the two large ones
  are pairs whose *before* p95 was unusually low (1.31 and 1.77 ms against 2.2 ms
  everywhere else). The sequential runs before the pairs gave p95 2.25-2.33 ms before and
  2.20-2.30 ms after. Read as no growth; the harness has no path through which the change
  can delay an echo (a hint is posted whenever none is out, and the test's own snapshot
  releases it after each key).
- Two-tab TUI load, 30 s (`raw/us0147-tui-*.json`, IN-0045's `tui-mimic.py`: 30 full-screen
  frames/s plus Ink-style repaints): frames 1307 / 1308 before, 1308 / 1310 after; feeds
  2181 / 2184 vs 2181 / 2177; `prepaint` p50 552 / 554 µs vs 522 / 583 µs. Hints are about
  one per feed there (the UI keeps up with 30 frames/s), so nothing is collapsed and nothing
  is lost. UI thread 6.8 / 8.6 % before, 8.7 / 10.1 % after: within the spread of the two
  before runs, but not shown to be equal (see Gaps).
- `frame_time_under_output` (`raw/us0147-frame-time.txt`): drives `TerminalElement` over
  `FakeTerminalSession` and runs none of this change; interleaved runs of the saved binaries
  overlap (flood p50 1711-1793 µs before, 1714-1935 µs after, both with load spikes).

### Tests

- `backend_tests::no_second_hint_is_posted_until_the_first_is_released` (three batches ->
  their titles and one `Output`; after a release the next batch posts),
  `a_hint_dropped_on_a_full_queue_does_not_block_the_next`,
  `each_chunk_posts_exactly_one_output_after_its_reliable_events` (now releases between
  chunks, as a snapshot would); `session::tests::a_snapshot_releases_the_repaint_hint`;
  `view_tests::output_that_came_without_a_hint_is_caught_up_a_frame_later`;
  `pipe_tests::wake_ups_posted_before_a_poll_all_arrive`;
  `panel::tests::the_catch_up_in_a_hidden_tab_releases_without_notifying` (after the merge
  with US-0145, below).
- BUG-0070 test 20/20 standalone; the local hand-over test 5/5; `cargo test -p
  oneterm-terminal` 221, `-p oneterm-terminal-view` 389 + 1, `-p oneterm-local-shell` 35.

### Gate

`cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings`
clean without a feature, with `oneterm-app/hotpath-profiling` and with
`oneterm-app/hotpath-profiling-alloc`; `cargo test -p oneterm-local-shell -p oneterm-terminal
-p oneterm-vt` green; `check-doc-paths` and `check-english` pass; the ignored-test census
re-recorded for `keystroke_echo_latency` (a measurement). Full `pwsh scripts/ci-local.ps1`
after deleting `target/release`: exit 0, final line `ci-local: all checks passed.`

### Verification and the merge with main

Independent verification: [`evidence/US-0147-verify.md`](evidence/US-0147-verify.md), PASS
with one fix owed at merge. Main (BUG-0081, US-0145) merged in without conflicts, then:

- **F1** (fixed): US-0145 skips `cx.notify()` for a view in a hidden tab, because the tab
  strip reads that view and every notify costs a whole-window frame. The catch-up task
  notified whenever it stamped a line, without that guard, so a hidden tab with streaming
  output cost a frame per hint again. It now notifies only outside a hidden tab. The
  verifier's throwaway test is committed as
  `panel::tests::the_catch_up_in_a_hidden_tab_releases_without_notifying`: five hints to a
  hidden tab give 5 releases and 0 notifies; with the guard removed it fails with 5
  notifies. `output_in_a_hidden_tab_does_not_notify_its_view` and
  `output_that_came_without_a_hint_is_caught_up_a_frame_later` still pass.
- **F2/F3** (records): the echo p50 and conout CPU differences did not reproduce and are
  stated as unchanged above; the UI-thread median no longer mixes in the superseded runs.
  The original commit message (`6a69384a`) quotes both as gains; it sits under the
  verification commit and the merge, so it was left as is and corrected here and in the
  fix commit's message.
- **F4** (docs): `docs/terminal-backend.md` § 5.3's `SessionEventSink` row says
  `post_repaint()` returns whether it queued.

### Gaps

- **No human look at a fast TUI.** Pacing is shown by frame counts and `prepaint`
  percentiles under the TUI load, not by eye; no GUI session was watched.
- **TUI-load UI thread CPU** is not shown equal: 8.7 / 10.1 % after against 6.8 / 8.6 %
  before, on a loaded machine. The added work is one 16 ms timer task and one
  `terminal_info` per hint (about 30 per second there).
- Machine shared with another building agent: absolute figures vary 30-50 % run to run;
  the conclusions rest on interleaved pairs and on counts (hints, allocations) that do not
  depend on load.
- The SSH pump shares the new hint rule (it goes through the same `finish_batch`); it was
  covered by the unit tests only, not measured.
- An inactive tab's lines are stamped up to one catch-up (16 ms) late; the gutter shows
  seconds.
- The per-chunk CPU left on the owner (about 4 µs) is the wake-up and park per line, which
  the producer's pacing dictates; the conout thread's (about 5.5 µs) is the `ReadFile` and
  the post per line. Neither can shrink without waiting for more output.

## Handoff

Done, verified, merged with main (F1 fixed on top). Remaining IN-0046 candidate: US-0146.
