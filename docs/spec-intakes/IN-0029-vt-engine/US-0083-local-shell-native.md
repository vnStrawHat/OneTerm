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
- [x] `grep -rn alacritty_terminal crates/local-shell/src` is empty (the manifest line is a
  recorded gap — see below).
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
- [x] Wire the handshake into the read loop (one `if`, at the chunk boundary).
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

Filled in at completion — see the sections added below.

## Handoff

Filled in at completion.
