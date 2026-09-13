# Work: crates/ssh goes native on the new engine

ID: US-0084
Intake: IN-0029
Created: 2026-09-13

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: high_risk (IN-0029's lane: the SSH read loop is the transport path of a shipped product surface)
- Spec Intake, when required: `IN-0029` — [`IN-0029.md`](IN-0029.md), contract
  [`low-level-design/migration.md`](low-level-design/migration.md)

## Outcome

`crates/ssh` runs on the native adapter with nothing of the fork left in it:

1. `ssh_main_task` feeds through `TerminalPump` and drains the engine's `EventBatch` —
   replies to the transport first, UI events sent after the lock is released — with no
   deferred sink anywhere in the crate.
2. The loop honours the **demand/yield handshake**: it calls
   `SharedTerminal::take_render_demand()` at the chunk boundary, **after** that batch's
   replies have left (R-37), and yields when the answer is `true`.
3. The grow-resize policy the engine receives is `oneterm_vt::ResizePolicy::BottomAnchor`,
   selected through the engine API rather than through an adapter-local enum.
4. `alacritty_terminal` is gone from `crates/ssh/Cargo.toml` and from every import.
5. Every `crates/ssh` test still passes; each rewritten test is listed with its reason.

## Scope

- [ ] In scope:
  - `crates/ssh/src/task.rs` — the batch boundary and the demand check.
  - `crates/ssh/src/session_terminal.rs` — the `ResizePolicy` selection.
  - `crates/ssh/Cargo.toml` — the `alacritty_terminal` line.
  - `crates/ssh` tests, including the handshake test ported from
    `crates/vt/src/render/render_tests.rs:785` onto the real `ssh_main_task`.
  - `docs/terminal-backend.md` § 5.1 and § 7 — the two clauses this packet makes false.
- [ ] Out of scope:
  - `crates/terminal` (`US-0082` shipped the adapter; `US-0085` owns what is left of the
    compatibility surface), `crates/local-shell` (`US-0083`) and `crates/terminal-view`
    (`US-0085`) — all three run concurrently with this packet, per the N-04 may-touch table.
  - Any SFTP, tunnel, agent-forwarding or auth behaviour: this packet does not touch the
    connect path.

## Acceptance

- [ ] `ssh_main_task` calls `take_render_demand()` once per data chunk, after
      `process_chunk` (which writes the batch's replies) and after `finish_batch`, and
      yields the task when it answers `true`.
- [ ] A test drives the **real** `ssh_main_task` against a loopback russh server under
      sustained output, raises the demand from another thread, and proves both halves: the
      renderer gets the lock in bounded time, and the flag was taken by the loop (it is
      clear once the render lock is held).
- [ ] The policy that reaches `Terminal::resize` from an SSH session is `BottomAnchor`,
      asserted by its behaviour (a row grow pulls scrollback into the viewport top and
      moves the cursor down), not by naming the adapter enum.
- [ ] `grep -rn alacritty_terminal crates/ssh` is empty, manifest included — or the packet
      records why it cannot be, with the exact API wanted and its owner.
- [ ] `pwsh scripts/ci-local.ps1` is green, with raw totals recorded.
- [ ] Every rewritten or deleted `crates/ssh` test is named with its reason.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md` — the `US-0084` packet line (the three
  changes plus `BottomAnchor` and the manifest deletion).
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` — the N-04 may-touch
  table, "The adapter contract, as `US-0082` shipped it", the event-order rule, the
  deletion list (the manifest-line row naming `crates/ssh` at `US-0084`).
- `docs/spec-intakes/IN-0029-vt-engine/US-0082-terminal-native.md` § Handoff — the adapter
  API table and the measured handshake (honoured 1 batch / 157 µs, ignored 3 800 batches /
  354 ms).
- `docs/terminal-backend.md` § 5.1 (the handshake and who wires it), § 5.3 (the pump layer
  and the `ResizePolicy` selection), § 7 (the SSH backend and `ssh_main_task`).
- `docs/ssh-client-connect.md` — the connect/auth path this packet must not disturb.
- `docs/agents/crate-dependency-rules.md` R7/R8 — `ssh` depends on `core` + `terminal` +
  `pty` and no UI crate; the manifest change must keep that true.
- `docs/agents/error-policy.md` — transport closure returns typed state, best-effort
  operations log with their operation name.

### Documentation Action

Update required:

- `docs/terminal-backend.md` § 5.1 — the sentence "Wiring that call into the two read loops
  is `US-0083` / `US-0084`" becomes true for SSH only, and the `lock_unfair` /
  `try_lock_unfair` survival note now hangs on `US-0083` alone.
- `docs/terminal-backend.md` § 7 — the bullet that still names `processor.advance` (the old
  engine's parser) and the demand check the loop now performs.

No other contract changes: the pump layer, the event order and the resize policy are
already described correctly by § 5.3 — this packet only makes `crates/ssh` use what is
documented there.

Reason: the two clauses above describe the wiring this packet performs, so they are stale
the moment it lands; everything else about the seam was reconciled by `US-0081` / `US-0082`
and stays correct.

### Reconciliation

To be completed before implementation is marked done.

## Context

- The SSH read loop already locks **per chunk** (`TerminalPump::process_chunk` takes and
  drops the engine lock around one `Terminal::feed`), so it is not the starvation shape
  `US-0082` measured — that is the local loop, which holds the lock while the pipe keeps
  delivering. The handshake is still wired here because the contract is the loop's, not the
  transport's, and because an unanswered flag stays raised forever.
- SSH replies do not leave on the calling thread: `SshTransport::pty_write` queues
  `Cmd::Write` for this same task's `select!`. "After the replies have left" therefore means
  after `process_chunk` has queued them and `finish_batch` has sent the batch's events.
- `impl_pty_terminal_session!` generates `resize_policy() -> $crate::model::ResizePolicy`.
  The macro lives in `crates/terminal`, which this packet may not touch.

## Plan

- [ ] Packet first (this file), then the harness story row.
- [ ] `task.rs`: the demand check at the chunk boundary, with the reason in a comment.
- [ ] `session_terminal.rs` / `Cargo.toml`: the policy selection and the fork's manifest
      line, both proven by a build and a grep.
- [ ] Tests: the handshake against the real task loop; the `BottomAnchor` behaviour.
- [ ] `docs/terminal-backend.md` § 5.1 / § 7.
- [ ] `pwsh scripts/ci-local.ps1`, then the SSH walk if `quser` shows an Active desktop.

## Decisions

None new. `DEC-0008` (the grow-resize policy per backend) and `DEC-0015` (per-view frame
ownership) are inherited; this packet changes neither.

## Verification Plan

- Unit: the `BottomAnchor` behaviour through a detached `SshSession` (cursor moves down by
  the rows pulled out of history on a grow; `KeepViewportTop` would not).
- Integration: the ported handshake test — a loopback russh server flooding data into the
  real `ssh_main_task`, a second thread raising the demand through `lock_for_render()`,
  asserting bounded wait **and** that the loop took the flag.
- Regression: `cargo test -p oneterm-ssh` (33 tests at the base) and the full
  `cargo test --workspace`.
- Static: `grep -rn alacritty_terminal crates/ssh` empty; `cargo tree -p oneterm-ssh -e
  normal` free of the fork; `python scripts/verify-dependency-graph.py`.
- Platform: `pwsh scripts/ci-local.ps1` (all ten steps) on Windows.
- E2E: an SSH session from this worktree's `fast-dev` build — prompt, echo, resize, a
  `yes | head -c 5M` flood, disconnect — only with an Active desktop (`quser`) and a
  reachable host; otherwise recorded as a gap.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To be completed after implementation.

## Handoff

Branch `worktree-agent-a34259a83971f85d8`, off `feat/vt-engine` @ `d3c537b`. The worktree
tool based it on `main` @ `c936ac0` (no `crates/vt`); `git reset --hard d3c537b` was run
before any file was read or written — the same correction `US-0076` / `0078` / `0079` /
`0080` / `0081` / `0082` recorded. Not merged, not pushed.
