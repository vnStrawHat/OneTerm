# Work: crates/ssh goes native on the new engine

ID: US-0084
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

**As shipped: 1, 2 and 5 hold. 3 and 4 are blocked in `crates/terminal`, which this packet
may not edit** — the macro expands `::alacritty_terminal` paths into the backend and
declares `resize_policy()` as returning the adapter enum. Both are measured, both name the
API wanted and its owner, and both are worked around inside `crates/ssh`: see gaps 1 and 2.

## Scope

- [x] In scope:
  - `crates/ssh/src/task.rs` — the batch boundary and the demand check.
  - `crates/ssh/src/session_terminal.rs` — the `ResizePolicy` selection.
  - `crates/ssh/Cargo.toml` — the `alacritty_terminal` line.
  - `crates/ssh` tests, including the handshake test ported from
    `crates/vt/src/render/render_tests.rs:785` onto the real `ssh_main_task`.
  - `docs/terminal-backend.md` § 5.1 and § 7 — the two clauses this packet makes false.
- [x] Out of scope:
  - `crates/terminal` (`US-0082` shipped the adapter; `US-0085` owns what is left of the
    compatibility surface), `crates/local-shell` (`US-0083`) and `crates/terminal-view`
    (`US-0085`) — all three run concurrently with this packet, per the N-04 may-touch table.
  - Any SFTP, tunnel, agent-forwarding or auth behaviour: this packet does not touch the
    connect path.

## Acceptance

- [x] `ssh_main_task` calls `take_render_demand()` once per data chunk, after
      `process_chunk` (which writes the batch's replies) and after `finish_batch`, and
      yields the task when it answers `true`.
- [x] A test drives the **real** `ssh_main_task` against a loopback russh server under
      sustained output, raises the demand from another thread, and proves both halves: the
      renderer gets the lock in bounded time, and the flag was taken by the loop (it is
      clear once the render lock is held).
- [x] The policy that reaches `Terminal::resize` from an SSH session is `BottomAnchor`,
      asserted by its behaviour (a row grow pulls scrollback into the viewport top and
      moves the cursor down), not by naming the adapter enum.
- [~] `grep -rn alacritty_terminal crates/ssh` is empty, manifest included — or the packet
      records why it cannot be, with the exact API wanted and its owner.
- [x] `pwsh scripts/ci-local.ps1` is green, with raw totals recorded.
- [x] Every rewritten or deleted `crates/ssh` test is named with its reason.

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

Changed:

- `docs/terminal-backend.md` § 5.1 — the demand check is wired in `ssh_main_task`; the
  sentence now says which loop has it and why the local one (`US-0083`) is the shape the
  354 ms starvation was measured on.
- `docs/terminal-backend.md` § 7 — the per-chunk bullet replaces the one that still named
  `processor.advance`, the old engine's parser.

Unchanged, and re-read to confirm it: `docs/ssh-client-connect.md` (this packet touches no
part of the connect, auth, host-key or forwarding path) and `migration.md` § "The adapter
contract, as `US-0082` shipped it" (the contract is correct; two of its **claims about what
is already possible** are not — see gaps 1 and 2, which the LLD's owner should fold into
the deletion list and the `US-0085` row).

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

- [x] Packet first (this file), then the harness story row.
- [x] `task.rs`: the demand check at the chunk boundary, with the reason in a comment.
- [x] `session_terminal.rs` / `Cargo.toml`: the policy selection and the fork's manifest
      line, both proven by a build and a grep.
- [x] Tests: the handshake against the real task loop; the `BottomAnchor` behaviour.
- [x] `docs/terminal-backend.md` § 5.1 / § 7.
- [x] `pwsh scripts/ci-local.ps1`, then the SSH walk if `quser` shows an Active desktop.

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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What changed

| Where | Change |
| --- | --- |
| `crates/ssh/src/task.rs` | `if term.take_render_demand() { tokio::task::yield_now().await; }` at the chunk boundary, after `process_chunk` (replies queued) and `finish_batch` (events sent). Plus the module doc and two comments that still described the fork's callbacks and the deleted deferred tier. |
| `crates/ssh/src/task_tests.rs` | New. The handshake against the real loop. |
| `crates/ssh/src/session.rs` | `ssh_session_keeps_the_default_grow_policy` → `ssh_grow_resize_pulls_scrollback_into_the_viewport_top`. |
| `crates/ssh/src/session_terminal.rs`, `Cargo.toml` | Comments recording what the engine actually receives and why the two deletions this packet owed could not be made. |
| `crates/ssh/src/transport.rs` | Two doc comments that still said "Alacritty `EventListener`" and "`Term`". |

**The batch drain needed no change.** `US-0082` moved it into `TerminalPump`, and
`ssh_main_task` has called `process_chunk` + `finish_batch` since: `Terminal::feed` fills
the batch, `OscRouter::drain` empties it under the same lock with replies first, and the
collected events are sent once the guard is dropped. There is no deferred sink left in the
crate to remove — verified by reading every `SessionEventSink` use in `crates/ssh`
(`session.rs:232`, the constructor, and nothing else).

### Tests

- **Added** `task::task_tests::the_task_yields_the_engine_to_a_waiting_frame` — the
  2-thread shape of `crates/vt/src/render/render_tests.rs:785` against the real
  `ssh_main_task`: a loopback russh server floods the shell channel, the task pumps it, a
  `spawn_blocking` task asks for the engine through `lock_for_render()`. Asserts the frame
  waits under 2 s **and** that the flag is clear afterwards, which only the loop's
  `take_render_demand()` can do.
- **Rewritten** `session::tests::ssh_session_keeps_the_default_grow_policy` →
  `ssh_grow_resize_pulls_scrollback_into_the_viewport_top`. Reason: the old test asserted
  the name of the adapter enum, which is exactly the thing this packet was meant to stop
  using. The new one feeds 40 lines through the pump, grows 24 → 30 rows and asserts the
  `BottomAnchor` contract by behaviour — the cursor moves down 6 and `total_lines` does not
  change, so the rows came out of history rather than being appended blank
  (`KeepViewportTop` fails both).
- Nothing else deleted or changed. `crates/ssh` was 66 tests and is 67.

### Negative control

With the `take_render_demand()` call commented out, the new test fails on
`the loop never took the render demand` (15.1 s: the 5 s poll plus the flood). The wait for
the lock itself passed in that run — the expected result of the SSH shape, see gap 3 — so
the flag assertion is the one carrying the proof.

### Commands

- `cargo test -p oneterm-ssh`: 67 passed / 0 failed / 0 ignored (66 at the base).
- `pwsh scripts/ci-local.ps1`: **exit 0, `ci-local: all checks passed`** — all ten steps
  (fmt `--check`, clippy `--workspace --all-targets -D warnings`, `cargo test --workspace`,
  `cargo test -p oneterm-vt --features vt-paranoid`, and the six Python policy checks).
  **Raw totals over its two test steps: 62 sections, 1919 passed / 0 failed / 13 ignored**
  — `cargo test --workspace` 58 / 1550 / 0 / 10 and vt-paranoid 4 / 369 / 0 / 3. `US-0082`
  recorded 1918 passed on the same gate; the +1 is this packet's net new test, and no other
  crate's count moved. The `us0081_parity` differential (7 tests, 2 `#[ignore]`d) is still
  green, so the seam still produces the same snapshot.
- `grep -rn alacritty_terminal crates/ssh` — two hits, both in `Cargo.toml`: the dependency
  line and the comment saying why it is still there. **No `crates/ssh` source file names
  the fork.**

### Gaps

1. **The `alacritty_terminal` manifest line could not be deleted, and this packet was
   wrong to be asked for it.** `impl_pty_terminal_session!` expands
   `::alacritty_terminal::vte::ansi::Rgb` (the four parameters of
   `TerminalRender::set_default_colors`) and `::alacritty_terminal::selection::SelectionType`
   (`TerminalInput::mouse_down`) **into the calling crate**, so the name must be in
   `crates/ssh`'s extern prelude. Measured: with the line removed,
   `cargo check -p oneterm-ssh --all-targets` fails with `E0433: cannot find
   alacritty_terminal in the crate root` (5 errors). A hand-written impl would not help —
   those are the **trait's** signatures in `crates/terminal/src/session.rs:515-518`, `:611`,
   part of the compatibility surface `migration.md` itself says only `US-0085` can remove.
   *Exact API wanted:* the macro must stop naming `::alacritty_terminal`, which needs those
   two trait signatures to move to `oneterm_vt::Rgb` / `SelectionKind`. *Owner:* `US-0085`
   (it moves `input/mouse.rs` and `theme/palette.rs`, the consumers that pin them).
   `crates/local-shell` hits the identical wall at `US-0083`. The migration LLD's deletion
   row and the `US-0081` / `US-0082` notes calling the line "already dead" are right about
   imports and wrong about macro expansion.
2. **`oneterm_vt::ResizePolicy::BottomAnchor` cannot be named by a backend.**
   `TerminalModel::new(term, impl Into<oneterm_vt::ResizePolicy>)` is necessary but not
   sufficient: the macro also generates
   `pub(crate) fn resize_policy(&self) -> $crate::model::ResizePolicy`, and that return
   type is what the `$resize_policy` token is checked against. Measured: with `oneterm-vt`
   added to `crates/ssh` and the token set to the engine value,
   `cargo check -p oneterm-ssh` fails with `E0308: expected oneterm_terminal::ResizePolicy,
   found oneterm_vt::ResizePolicy … expected because of return type`. Both experiments were
   reverted. *Exact API wanted:* `fn resize_policy(&self) -> impl Into<::oneterm_vt::ResizePolicy>`
   in the macro (or `oneterm_terminal::ResizePolicy` becoming a re-export of the engine
   enum, which deletes the `From` at the same time). *Owner:* the next packet that may edit
   `crates/terminal` — `US-0085`, since `US-0083` needs the same change for
   `KeepViewportTop`. *Workaround here:* the selection is unchanged and still reaches the
   engine as `BottomAnchor` through the `From`; the test now pins the engine **behaviour**
   rather than the adapter enum's name, which is the stronger assertion anyway.
3. **The yield is insurance, not a fix, in the SSH shape — and that is a finding.** SSH
   locks **per chunk** (`TerminalPump::process_chunk` takes and drops the guard around one
   `feed`), so it is not the loop `US-0082` measured at 3 800 batches / 354 ms: that is the
   local one, which holds the lock while the pipe keeps delivering. **Measured on this
   test** (assertion temporarily tightened to print the value, then restored): the frame
   waited **394 µs** for the engine under the flood — the same order as the 157 µs the
   `US-0082` verifier measured for a loop that honours the flag, and nowhere near 354 ms.
   With the call removed the frame still got the lock inside the 2 s bound —
   `parking_lot`'s fair unlock does that part — but the flag stayed raised. So what
   `US-0084` fixes here is the handshake's bookkeeping and the relock race, not a measured
   354 ms stall. The 2 260x number in the `US-0082` handoff belongs to `US-0083`.
4. **No GUI or E2E SSH walk.** `quser` reports the only session (`trunglt`, id 1) as
   `Disc`, idle 11:17 — there is no interactive desktop to run the app on. The loopback
   `sftp-dev-server` cannot substitute: its shell is a one-line echo banner, not a shell
   (`crates/tools/src/bin/sftp-dev-server.rs:102-110`), so there is no prompt, no `yes |
   head -c 5M` and no resize to observe. No other host is configured. The owner's own
   `oneterm.exe` (pid 27376) was left strictly alone — never enumerated by window, never
   signalled. *Substitute evidence:* the new test is a real SSH session end to end —
   loopback handshake, host-key policy, password auth, `channel_open_session`,
   `request_shell`, a sustained flood of at least 256 KiB through the real loop, then
   `pty_close` and the task's teardown block — which covers every step of the asked-for walk
   except the resize and the pixels.

## Handoff

Branch `worktree-agent-a34259a83971f85d8`, off `feat/vt-engine` @ `d3c537b`. The worktree
tool based it on `main` @ `c936ac0` (no `crates/vt`); `git reset --hard d3c537b` was run
before any file was read or written — the same correction `US-0076` / `0078` / `0079` /
`0080` / `0081` / `0082` recorded. Not merged, not pushed.
