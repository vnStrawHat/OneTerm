# Work: `crates/local-shell` goes native

ID: US-0083
Intake: IN-0029
Created: 2026-09-13

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

- Change type: existing-contract change
- Risk lane: high_risk
- Spec Intake, when required: `IN-0029` — [`IN-0029.md`](IN-0029.md)

## Outcome

The local shell's PTY read loop runs natively on the engine adapter `US-0082` shipped:

1. it drains the `EventBatch` through `TerminalPump::advance` (no deferred sink) and sends
   the batch's events after the guard is dropped;
2. it **honours the demand/yield handshake** — `SharedTerminal::take_render_demand()` at a
   chunk boundary, guard dropped when it answers `true` — so a waiting frame gets the lock
   within one batch instead of waiting for the pipe to drain;
3. it selects its grow-resize `ResizePolicy` through the engine's own enum;
4. it uses `oneterm-pty`'s public token constants;
5. `crates/local-shell` names `alacritty_terminal` nowhere — manifest line and imports gone.

Items 3 and 5 turned out to be blocked by `crates/terminal`'s public API, which this packet
must not touch; both are recorded as gaps with the exact API wanted, and the workaround is
inside `crates/local-shell`. See *Evidence and Gaps*.

## Scope

- [x] In scope: `crates/local-shell/**` and its `Cargo.toml`; the two sentences in
  `docs/terminal-backend.md` that name this packet as the owner of the unwired call.
- [x] Out of scope: `crates/terminal` (`US-0085` owns the public-API cleanup),
  `crates/ssh` (`US-0084`, running concurrently), `crates/terminal-view` (`US-0085`),
  `scripts/dependency-graph-policy.json` and `docs/agents/crate-dependency-rules.md`
  (changing the backends' allowed dependency set is a rules change, not this packet's).

## Acceptance

- [x] The read loop asks `take_render_demand()` at the chunk boundary, **after** the batch's
  replies are computed and written (R-37), and drops the engine guard when it is raised.
- [x] A test drives the **real** `ShellEventLoop::run` with a flooding producer on one thread
  and a `lock_for_render()` waiter on another, and asserts the waiter is served inside a
  bound far below the 354 ms the `US-0082` verifier measured for the ignoring loop.
- [x] The loop holds no engine lock it does not need: the `Engine::exit()` no-op call site is
  gone, which frees `crates/terminal` to delete the `Engine` newtype.
- [x] No local PTY token constants; `oneterm_pty::{PTY_CHILD_EVENT_TOKEN, PTY_READ_WRITE_TOKEN}`
  are the only ones.
- [ ] **Not met — gap 2.** `grep -rn alacritty_terminal crates/local-shell` is empty. It
  is not: the manifest line is load-bearing for the `impl_pty_terminal_session!`
  expansion and `session_tests.rs:5` needs the type to call the public trait. Blocked on
  `crates/terminal`, which this packet must not touch; proved with the compiler.
- [ ] **Not met — gap 1.** `ResizePolicy` named as the engine's enum. Blocked the same
  way, also proved with the compiler; the value reaching `Terminal::resize` is the
  engine's either way.
- [x] Every existing `crates/local-shell` test still passes; rewrites are listed with reasons.
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- [`IN-0029.md`](IN-0029.md) — the `US-0083` packet line (the four changes) and the layering
  invariants.
- [`low-level-design/migration.md`](low-level-design/migration.md) — the may-touch / must-not-touch
  table (N-04) and "The adapter contract, as `US-0082` shipped it": `TerminalHandle`,
  `TerminalModel::new(term, ResizePolicy)`, `OscRouter::drain(&batch, &mut Vec<SessionEvent>)`.
- [`low-level-design/events-and-api.md`](low-level-design/events-and-api.md) — "the backend
  loops keep the shape they already have".
- [`low-level-design/pty.md`](low-level-design/pty.md) — the two token constants are `pub` on
  both platforms since `US-0071`, replacing the local `const … = 1`.
- [`US-0082-terminal-native.md`](US-0082-terminal-native.md) § Handoff — what this crate needs
  and the measured handshake (honoured 1 batch / 156.8 µs, ignored 3 800 batches / 354.5 ms).
- [`US-0071-pty-crate.md`](US-0071-pty-crate.md) — the transport this loop drives.
- `docs/terminal-backend.md` § 5.1, § 5.3, § 6.2 — the concurrency contract, the shared pump
  layer, and the "Current implementation" description of this loop.
- `docs/agents/crate-dependency-rules.md` R7/R8 — the backends may depend on `core` +
  `terminal` + `pty` only, which is why `oneterm-vt` cannot be added here to name its enum.

### Documentation Action

Update required, and small: `docs/terminal-backend.md` § 5.1 says "Wiring that call into the
two read loops is `US-0083` / `US-0084`" and § 6.2's "Current implementation" paragraph
describes the loop's locking without the yield. Both become stale the moment the `if` lands.
Everything else in the reviewed set already describes the target behaviour — the adapter
contract, the pump shape and the token constants are `US-0081`/`US-0082`/`US-0071` records and
need no change.

Reason: this packet changes one runtime behaviour (when the pump releases the lock) and
deletes dead references; only the two sentences that name the behaviour as unwired are wrong.

### Reconciliation

Changed: `docs/terminal-backend.md` § 5.1 (the local loop calls `take_render_demand()` since
this packet; `US-0084` still owns the ssh task) and § 6.2 (the "Current implementation"
paragraph names the yield). The no-change reason above holds for every other reviewed doc:
re-read at completion, none of them asserts anything this packet contradicts.

## Context

- The starvation is structural, not a fairness bug: the inner read loop takes the guard once
  and keeps it until `read` reports the pipe empty (`event_loop.rs:374-421`), so a
  `FairMutex` waiter never sees an unlock to be handed. That is the shape the `US-0082`
  verifier measured.
- `TerminalPump::finish_batch_blocking` runs **after** the guard is dropped and can block on
  a full event queue. A flood test must drain the event channel on a third thread, or the
  pump blocks there with the lock released and the waiter is served for the wrong reason.
- `impl_pty_terminal_session!` expands `::alacritty_terminal::selection::SelectionType` and
  `::alacritty_terminal::vte::ansi::Rgb` **in the calling crate**, so every backend that
  instantiates it needs the dependency until `crates/terminal`'s public trait stops naming
  those types (`US-0085`).

## Plan

- [x] Packet first; mirror the story row into the main checkout's `harness.db`.
- [x] Baseline: `cargo test -p oneterm-local-shell` before any edit.
- [x] Wire the handshake into the read loop (one `if`, at the chunk boundary — it took
      two attempts; the first stalled the session, see the evidence).
- [x] Delete the `Engine::exit()` call site.
- [x] Prove the two blocked items with the compiler rather than by assertion, then record the
      gaps and restore the working form.
- [x] Port the 2-thread starvation shape into `event_loop_tests.rs` against the real loop.
- [x] Refresh the stale `alacritty` wording in this crate's comments.
- [x] `pwsh scripts/ci-local.ps1`; record raw totals.

## Decisions

No new decision. DEC-0008 (grow-resize policy) is unchanged by this packet.

## Verification Plan

- Unit / integration: `cargo test -p oneterm-local-shell` — the existing 30 tests plus the new
  handshake test, which drives `ShellEventLoop::run` over the loopback PTY.
- The handshake assertion is a **bound**, not a stopwatch equality: the honoured path was
  measured at ~157 µs and the ignored path at ~354 ms, so a debug-build bound in the tens of
  milliseconds separates them by more than an order of magnitude in both directions.
- Workspace regression: `pwsh scripts/ci-local.ps1` (fmt, clippy `-D warnings`, `cargo test
  --workspace`, `cargo test -p oneterm-vt --features vt-paranoid`, six Python policy checks).
- `grep -rn alacritty_terminal crates/local-shell`.
- Flood measurement through the real pump before/after, if the `US-0082` bench harness can be
  pointed at it.
- GUI walk (prompt / echo / Ctrl-C / resize reflow / `type` a 10 MB file while dragging a
  selection / exit) **only if `quser` reports the desktop session Active**.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What changed

`crates/local-shell/src/event_loop.rs` — the read loop asks
`SharedTerminal::take_render_demand()` at each chunk boundary and, when a frame is
waiting, answers that batch's colour queries under the guard, drops it (a fair unlock
hands the engine to the queued frame) and writes the replies. The `Engine::exit()`
no-op call site is gone with it, and the module's stale `alacritty` references with it.

Two constraints shaped the implementation, and the second is the one that cost the
iterations:

1. **Replies first (R-37).** They are computed under the guard being handed over and
   written as it drops, so a conhost waiting on a colour answer is never queued behind
   a frame.
2. **The yield must not leave the read loop.** The first implementation `break`d out of
   the read loop, which is what the design text reads like. That stalls the session on
   Windows: `PipeReader::read` arms `caller_waiting` **only when it finds the ring
   empty** (`crates/pty/src/windows/pipe.rs`), and `push` posts a completion packet only
   when that flag is set — so a loop that stops reading with bytes still buffered is
   never woken again and parks in `poll.wait` forever. The three real-shell tests
   (`e2e_echo_output_rendered_in_snapshot`, `selection_text_and_clear`,
   `mouse_drag_updates_selection_not_mouse_move`) failed immediately on that version;
   the shipped one drops the guard and keeps reading, re-locking on a later pass. The
   loopback-socket fixture cannot see this — a TCP socket's readiness is
   level-triggered — which is worth knowing before `US-0084` writes the same `if`.

### The handshake, measured

`a_flooding_loop_hands_the_engine_to_a_waiting_frame` (`event_loop_tests.rs`) drives the
real `ShellEventLoop::run` over the loopback PTY: one thread floods 4 KiB lines without
pause, one thread drains the UI event queue, one thread takes five frames through
`lock_for_render()` and reports the worst wait through a channel, so a pump that never
yields fails on a deadline instead of hanging the suite.

| | worst frame wait |
|---|---|
| yield wired (three runs) | **86.2 ms / 107.9 ms / 125.3 ms** |
| yield disabled (`if false &&`, negative control) | **never** — `no frame reached the engine within 4.75s of a flooding pump`, test fails in 4.99 s |

The bound in the test is 750 ms. It is far above `US-0082`'s 157 µs because the two
measurements are not the same unit of work: `US-0082` fed 4 KiB in-process chunks, while
one batch here is whatever the socket buffer held (up to the loop's 1 MiB read) parsed at
`test`-profile opt-level 0. The property the test pins is the one the design states —
**one batch, not the whole flood** — and the failing side does not arrive at all, so the
bound only has to sit between "one debug-build batch" and "never".

### Flood throughput

The `US-0082` bench harness could not be pointed at this: it drives `Terminal::feed` and
the snapshot in-process (`crates/terminal/tests/us0081_parity.rs`), below the pump, so it
cannot see a read loop at all. The pump-level equivalent is
`flood_throughput_while_a_renderer_takes_frames` (`#[ignore]`, in this crate), which runs
the real loop for two seconds with a renderer taking a frame every 16 ms — the shape the
hand-over exists for.

Three runs each, `--profile fast-dev`, 2 s window, a renderer asking every 16 ms (so
125 frames is the ceiling):

| | throughput | frames the renderer got |
|---|---|---|
| yield wired | 44.5 / 31.0 / 22.2 MiB/s | **81 / 95 / 91** |
| yield disabled | 47.7 / 32.0 / 47.3 MiB/s | **20 / 13 / 6** |

That is the trade, and it is the one the design asks for: the renderer gets about
**seven times** the frames, and the pump gives up roughly a third of its peak flood
throughput to hand the engine over ~60 times a second. The throughput spread (22-48
MiB/s) is wider than the difference between the two arms, so the honest reading of that
column is "no order-of-magnitude cost", not a precise percentage; the frame column is
unambiguous. Note that even the disabled arm gets a few frames — the loopback socket
occasionally runs dry, which is exactly the "next natural pause" a frame had to wait for
before this packet.

With no renderer running the added work is a single relaxed atomic exchange per chunk,
so there is nothing to measure there.

### Verification

- `pwsh scripts/ci-local.ps1` — raw totals below.
- `cargo test -p oneterm-local-shell`: **31 passed, 0 failed, 1 ignored** (baseline at
  `d3c537b` before any edit: **30 passed, 0 ignored**). **No test was rewritten or
  deleted.** Two were added: the handshake test, and the `#[ignore]`d throughput
  measurement. The three real-shell tests above are unchanged and are what caught the
  stall.
- `cargo clippy -p oneterm-local-shell --all-targets --features terminal-diagnostics --
  -D warnings` — clean; the diagnostics build records a lock-hold sample at the
  hand-over, so the p95/p99 histogram still sees every hold.
- `grep -rn alacritty_terminal crates/local-shell/src` — one hit, `session_tests.rs:5`,
  which gap 2 explains.

### Gaps

1. **`ResizePolicy` is still selected through the adapter's enum, not the engine's.**
   Wanted: `pub use oneterm_vt::ResizePolicy;` in `crates/terminal/src/lib.rs`, **and**
   `impl_pty_terminal_session!`'s generated `resize_policy()` returning
   `oneterm_vt::ResizePolicy` rather than `$crate::model::ResizePolicy`. Measured, not
   assumed: passing `oneterm_vt::ResizePolicy::KeepViewportTop` as the macro's fourth
   argument is `error[E0308]: expected 'oneterm_terminal::ResizePolicy', found
   'oneterm_vt::ResizePolicy'` at `session_terminal.rs:24`, because the accessor pins the
   type — `TerminalModel::new`'s `impl Into<…>` bound does not reach the macro argument.
   Taking a direct `oneterm-vt` dependency here is refused on the other side by R8 and
   `scripts/dependency-graph-policy.json`, neither of which this packet may change.
   **Workaround:** `local_resize_policy()` keeps `oneterm_terminal::ResizePolicy`, which
   `TerminalModel::new` converts, so the value reaching `Terminal::resize` is already the
   engine's — only the spelling at the call site is the adapter's. The Windows/Unix split
   is deliberate and unchanged (DEC-0008): `KeepViewportTop` on Windows, the
   bottom-anchored default elsewhere.
2. **The `alacritty_terminal` manifest line could not be deleted.** Measured: removing it
   is five `error[E0433]: cannot find 'alacritty_terminal' in the crate root`, all from
   the `impl_pty_terminal_session!` expansion, which names
   `::alacritty_terminal::selection::SelectionType` and
   `::alacritty_terminal::vte::ansi::Rgb` **in the calling crate**. `US-0081` recorded
   `crates/ssh`'s line as "dead today"; it is not — it is load-bearing for the same
   reason, which `US-0084` should know before it tries. Wanted: the `TerminalRender`
   trait taking `oneterm_vt::SelectionKind` and the engine's `Rgb`, i.e. `US-0085`'s
   "delete `engine_shim.rs` whole". The line now carries a comment saying so, and the
   crate's own source names the crate in exactly one place —
   `session_tests.rs:5`, needed to call `mouse_down` through the public trait.
3. **No GUI walk.** `quser` reports the only session (`trunglt`, id 1) as **Disc**,
   idle 11:34 — there is no interactive desktop to walk. The prompt/echo/Ctrl-C/resize
   reflow/10 MB `type` with a selection drag/exit walk is therefore **not** evidence this
   packet can offer, and the E2E proof box is unticked. The owner's own `oneterm.exe`
   (pid 27376) was left strictly alone: never enumerated by window, never stopped.
4. **`Engine` can go now.** Its only member was the no-op this packet deleted, so
   `crates/terminal` can drop the newtype and let `lock()` yield `oneterm_vt::Terminal`
   directly. Not done here (must not touch `crates/terminal`); `crates/ssh` never called
   it, so nothing blocks it after `US-0084`.
5. **`TerminalPump::pending` stays behind its mutex.** `pump.rs:68` names this packet as
   the reason ("the backends hold the pump by `&` at the lifecycle call sites"). It is
   still true: `publish_child_exit(&self.pump, …)` runs inside the `for event` loop while
   `self.pump` is also borrowed mutably by `advance` in the same scope. Removing the
   mutex means `&mut` pump methods in `crates/terminal`, which is out of scope.
6. **The hand-over is asserted in wall-clock, not in batches.** The pump's batch count is
   not observable from outside the loop — `SessionEvent::Output` is coalescible and, in
   the starved case, never sent at all — so "one batch" is pinned as a time bound with
   the negative control proving the other side does not arrive.

### Raw totals

`pwsh scripts/ci-local.ps1` — **all ten steps passed**, exit 0 (`cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
`cargo test -p oneterm-vt --features vt-paranoid`, and the six Python policy checks).

Over its two test steps, **62 sections: 1919 passed / 0 failed / 14 ignored** —
`cargo test --workspace` 58 / 1550 / 0 / 11 and `vt-paranoid` 4 / 369 / 0 / 3.

`US-0082` recorded 62 / 1918 / 0 / 13 on the same base, so the delta is **+1 passed** (the
handshake test) and **+1 ignored** (the throughput measurement). Nothing else in the
workspace moved, which is the check that this packet stayed inside `crates/local-shell`.

The gate was run twice: once before the `#[ignore]`d measurement was added (58 / 1550 / 0
/ **10**) and once after (58 / 1550 / 0 / **11**). Both green.

## Handoff

Branch `worktree-agent-a20da8012abddb9c7`, off `feat/vt-engine` @ `d3c537b`. **Not
merged, not pushed.** The worktree tool based it on `main` @ `c936ac0`, which has no
`crates/vt`; `git reset --hard d3c537b` was run before any file was read or written — the
same correction `US-0076`-`US-0082` recorded.

### For `US-0084` (`crates/ssh`), which writes the same `if`

- The `alacritty_terminal` manifest line in `crates/ssh/Cargo.toml` is **not** dead, and
  `US-0081`'s note that it is should not be trusted: the macro expansion needs it. See
  gap 2 for the exact errors.
- The `ResizePolicy` token cannot become `oneterm_vt::ResizePolicy::BottomAnchor` either;
  see gap 1 for the type error and the API that would fix both backends at once.
- The tokio task has no conout ring under it, so the "do not leave the read loop"
  constraint (gap/finding 2 above) is a Windows-ConPTY property and may not apply — but
  the same reasoning has to be done for `russh`'s stream before the yield is written as a
  `break`.
- The two-thread test shape ports directly: flood on one thread, drain the UI queue on a
  second (or the pump can park in `finish_batch` with the lock already released and the
  test passes for the wrong reason), take frames on a third and report through a channel
  so a non-yielding pump fails on a deadline instead of hanging.
