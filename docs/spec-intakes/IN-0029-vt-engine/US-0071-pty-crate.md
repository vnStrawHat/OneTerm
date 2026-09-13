# Work: Extract the pseudo-console transport into `oneterm-pty`

ID: US-0071
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (a new workspace crate carved out of a vendored dependency)
- Risk lane: high_risk (public contract: a new crate's API; external effect: child-process spawning; the bundled ConPTY host is load-bearing for Sixel — DEC-0013)
- Spec Intake, when required: IN-0029 (`docs/spec-intakes/IN-0029-vt-engine/IN-0029.md`)

## Outcome

A workspace crate `crates/pty` (`oneterm-pty`) owns the pseudo-console transport, written from
scratch against [`low-level-design/pty.md`](low-level-design/pty.md):

- ConPTY on Windows, resolving the **bundled `conpty.dll` next to the executable first** and
  falling back to `kernel32!CreatePseudoConsole` only when the bundled pair is missing (DEC-0013),
  with a test that a refactor cannot silently drop the DLL path.
- `openpty` on Unix.
- One public surface on both platforms: `Options`, `Shell`, `WindowSize`, `GlyphWidth`,
  `PseudoConsole`, `EventedReadWrite`, `EventedPty`, `OnResize`, `ChildEvent`,
  `PTY_READ_WRITE_TOKEN`, `PTY_CHILD_EVENT_TOKEN`, and a uniform `PseudoConsole::child_pid()`.

`crates/local-shell` and `crates/tools` move onto it. Stop condition:
`grep -rn "alacritty_terminal::tty" crates/` is empty and the workspace quality gate is green.

## Scope

- [ ] In scope: the new crate and its tests; the three API defects pty.md lists (public child
  token on both platforms, one `child_pid()`, no process-global `setup_env`); moving
  `crates/local-shell` and `crates/tools/src/bin/pty-throughput.rs` onto it; registering the crate
  in `Cargo.toml`, `scripts/dependency-graph-policy.json`, `docs/agents/structure.md`,
  `docs/agents/crate-dependency-rules.md`, `docs/agents/dependencies.md`, `docs/architecture.md`.
- [ ] Out of scope: any engine (grid/parser) work — `crates/terminal`, `crates/ssh` and
  `crates/terminal-view` keep importing `alacritty_terminal` and are not touched; the
  `ConptyBackend::Spawned` (`OpenConsole.exe --headless`) option pty.md records for later;
  `crates/tools` bench/corpus files, which `US-0072` adds; bumping or re-hashing the bundled
  ConPTY pair, which IN-0030 owns.

## Acceptance

- [x] `crates/pty` builds on Windows and is a leaf: it depends on no OneTerm crate and on no
      dependency outside `polling`, `log`, `windows-sys` (Windows) and `libc` (Unix).
- [x] `ConptyApi::resolve()` loads the bundled `conpty.dll` when it sits next to the executable
      and reports `System` only when it does not, proven by a test that exercises both directories.
- [x] The resolved backend is logged once at `info` (`conpty: bundled` / `conpty: system`).
- [x] `PTY_CHILD_EVENT_TOKEN` and `PTY_READ_WRITE_TOKEN` are public on both platforms and
      `crates/local-shell` no longer redeclares either.
- [x] `PseudoConsole::child_pid()` replaces the two cfg'd helpers in
      `crates/local-shell/src/event_loop.rs`.
- [x] A failed `ResizePseudoConsole` returns `io::Result` instead of asserting; the caller logs
      at `warn` and keeps the session (`docs/agents/error-policy.md`, transport row).
- [x] `grep -rn "alacritty_terminal::tty" crates/` is empty.
- [x] `pwsh scripts/ci-local.ps1` is green.
- [x] A local shell opens, echoes, keeps the bundled `OpenConsole.exe` as its console host,
      interrupts a running command (a real Ctrl-C keystroke), reflows on resize, renders a Sixel
      image, and on shell `exit` takes the shell **and its console host** down while the app
      stays up (GUI walk on Windows, every step seen on screen:
      [`evidence/US-0071-gui-walk-visual.md`](evidence/US-0071-gui-walk-visual.md)).
      *Wording corrected 2026-09-12*: this line used to read "closes its tab on shell exit". The
      app does not close the tab — `SessionEvent::Closed` in `crates/terminal-view` only marks the
      agent ended — so the original phrasing described behaviour OneTerm never had. The
      transport half of the claim is what this packet owns and is proven.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md` — the crate's contract: types,
  traits, the ConPTY resolution order, the edge cases and the test list. Authoritative.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` — packet order, the
  deletion list rows owned by this packet, and the doc-reconciliation table (R-46).
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` — crate layout (`pty` is an L0 leaf),
  the consumer map, and the rule impact on R6/R7/R8.
- `docs/spec-intakes/IN-0029-vt-engine/research/api-surface.md` § 2.2, § 2.5, § 3.8 — every
  `tty` item OneTerm actually consumes, with call sites.
- `docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md` — the bundled pair is kept and
  `kernel32` stays the fallback; nothing here may hard-code a `kernel32`-only path.
- `docs/agents/crate-dependency-rules.md`, `docs/agents/structure.md` — layering and the crate
  tables that gain a row.
- `docs/agents/dependencies.md` § 3 — the allowed auxiliary crates table.
- `docs/agents/error-policy.md` — transport row (typed error, no panic) and the "no `unwrap` in
  PTY error paths" review rule.
- `docs/terminal-backend.md` § 6.2 — the local-shell spawn path description.
- `docs/license-analysis.md` § 3 — Apache-2.0 source may be reused with the notice retained.

### Documentation Action

Update required, in this commit (R-46: the graph allow-list must move with the crate or CI fails):

- `Cargo.toml` — new member + `oneterm-pty` workspace dependency.
- `scripts/dependency-graph-policy.json` — `crates/pty` member, `oneterm-pty: []`, and the two
  consumers' internal-dependency lists.
- `docs/agents/structure.md` § 1 (tree) and § 3 (crate table).
- `docs/agents/crate-dependency-rules.md` — the layer diagram, R7 and R8 name `pty`;
  `crates/tools` may depend on it.
- `docs/agents/dependencies.md` § 3 — the "Local shell PTY" row becomes `oneterm-pty`, and the
  new crate's direct dependencies (`polling` **public**, `windows-sys`, `libc`) are recorded.
- `docs/architecture.md` — the new crate.
- `docs/terminal-backend.md` — the two sentences that name `alacritty_terminal::tty` as the local
  transport.
- `NOTICE` / `THIRD-PARTY-NOTICES.md` — only if attribution changes; see Context.

Reason: the crate is new, so every document that enumerates crates is stale the moment it lands,
and `python scripts/verify-dependency-graph.py` fails on an unlisted member.

### Reconciliation

Changed: `Cargo.toml`, `scripts/dependency-graph-policy.json`, `docs/agents/structure.md`,
`docs/agents/crate-dependency-rules.md`, `docs/agents/dependencies.md`, `docs/architecture.md`,
`docs/terminal-backend.md` (§ 1 invariant 3, the § 2 diagram, the § 3 crate table, the § 6.2
heading and the § 6.3 ConPTY and child-exit rows), `docs/PROJECT.md`, `README.md`, and `NOTICE`.
`THIRD-PARTY-NOTICES.md` is unchanged and
`python scripts/third-party-notices.py --check` confirms it: no new external crate enters
`Cargo.lock` and § 1 (the bundled ConPTY pair, owned by IN-0030) is untouched.

## Context

- **No new external dependency.** `polling`, `log`, `windows-sys` and `libc` are already workspace
  dependencies. `windows-sys` gains two features in the workspace union — `Win32_System_Pipes`
  (`CreatePipe`) and `Win32_Security` (`SECURITY_ATTRIBUTES`, which `CreatePipe` takes) — which is
  the extension `docs/agents/dependencies.md` § 3 already anticipates ("a workspace-wide feature
  union").
- **Three vendored dependencies are deliberately not carried over**, so the crate is written
  against the platform APIs rather than re-vendoring the fork's dependency set:
  - `miow` (anonymous pipes) → `CreatePipe` through `windows-sys` plus `OwnedHandle`;
  - `piper` (the lock-free ring feeding the reader/writer threads) → one `Mutex<VecDeque<u8>>` +
    `Condvar` per pipe end. PTY rates are ~30 MiB/s, far below what a single lock costs;
  - `signal-hook` + `rustix-openpty` on Unix → `libc::openpty` plus a reaper thread that owns the
    `Child` and wakes the poller through a `UnixStream` pair. That removes a process-global
    `SIGCHLD` handler from a library crate, which is a better neighbour for an app that also runs
    a tokio runtime and a crash handler.
- **Attribution.** Two fragments are reused close to verbatim from the Apache-2.0
  `alacritty_terminal` fork: the MSVCRT argument-escaping routine with its test table, and the
  case-insensitive environment-block builder. Both carry a source header naming the origin,
  licence and the fact that they were modified, per `docs/license-analysis.md` § 3 and Apache-2.0
  § 4(b). `NOTICE` gains one line; `THIRD-PARTY-NOTICES.md` is generated from `Cargo.lock` and
  does not change.
- **Loopback test placement.** `migration.md` moves the loopback PTY "to `oneterm-pty`". The loop
  tests in `crates/local-shell/src/event_loop_tests.rs` drive `ShellEventLoop::run` and cannot
  lose their fake PTY, so `oneterm-pty` gets its own `loopback_implements_the_evented_contract`
  test and `crates/local-shell` keeps its `LoopbackPty` — now implementing the `oneterm-pty`
  traits, which is the actual out-of-crate proof that the contract is implementable.
- **`GlyphWidth` is a declared, intentional behaviour change.** The fork passes `0` for
  `CreatePseudoConsole`'s flags; this crate passes `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH (0x10)` by
  default, which pty.md requires so the host and the engine can never disagree about cluster
  width. The bundled host **does** honour the bit, measured A/B on 1.24.2607.10001 by reading
  `[Console]::CursorLeft` after writing each string (the verifier's numbers, recorded in
  [`evidence/US-0071-verify.md`](evidence/US-0071-verify.md)):

  | Host configuration | `[日本語 🙂 x]` | `[👨‍👩‍👧]` |
  |---|---|---|
  | before this packet, flags = `0` | 13 | **4** |
  | after this packet, flags = `0x10` | 13 | **8** |

  The host's default is grapheme measurement; `0x10` switches it to per-codepoint `wcswidth`,
  which is what the engine does today (`unicode-width` per `char`: 👨 2 + ZWJ 0 + 👩 2 + ZWJ 0 +
  👧 2 = 6, i.e. 8 with the brackets). **The change removes a 4-column drift between host and
  engine that existed before this packet**; plain CJK and single emoji are unaffected. The flag
  must be flipped to `GlyphWidth::Graphemes` in the same packet that gives the engine grapheme
  width (mode 2027, a later intake) — otherwise the drift comes back inverted. Hosts older than
  the flag ignore it.

## Plan

- [x] Write this packet; mirror the story row into `harness.db`.
- [x] Create `crates/pty` with the public surface, the Windows backend (ConPTY resolution, pipes,
      child watcher) and the Unix backend.
- [x] Register the crate: `Cargo.toml`, the graph policy, the docs above.
- [x] Move `crates/local-shell` (event loop, session, transport, tests) and
      `crates/tools/src/bin/pty-throughput.rs` onto it; delete the redeclared token and the two
      cfg'd child-pid helpers.
- [x] `pwsh scripts/ci-local.ps1`.
- [x] GUI walk from a `fast-dev` build of this worktree; save screenshots and
      `evidence/US-0071-gui-walk.md`.

## Decisions

- [`DEC-0013`](../../decisions/DEC-0013-bundled-conpty-host-and-bump-script.md) — the bundled
  ConPTY pair is kept, `kernel32` is the fallback. This packet reproduces that order and pins it
  with a test.

No new decision: every choice here is already fixed by pty.md and DEC-0013.

## Verification Plan

- `cargo test -p oneterm-pty` — the pty.md test list: the ConPTY backend preference in both
  directions, the resolve log, the argument-escaping table, the environment-block dedup, the glyph
  flag bits, the child-exit lifecycle (with a code, without a code, an instantly exiting child),
  resize-failure-is-an-error, the drop-order drain, `child_pid()`, and the loopback contract.
- `cargo test -p oneterm-local-shell` — the loop tests, unchanged in substance, now over the
  `oneterm-pty` traits.
- `cargo run -p oneterm-tools --bin pty-throughput -- cmd /c ver` — the diagnostic still spawns.
- `pwsh scripts/ci-local.ps1` — fmt, clippy `-D warnings`, `cargo test --workspace`, the graph
  policy, the doc-path check, the English check, the completion catalogs, the notices check.
- `grep -rn "alacritty_terminal::tty" crates/` — empty.
- GUI walk on Windows from a `fast-dev` build: prompt, `echo hi`, the console host
  child is the bundled `OpenConsole.exe`, Ctrl-C interrupts `ping -t 127.0.0.1`, resize reflows,
  `type snake.six` renders, shell `exit` takes the shell and its console host down.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Commands

- `rtk proxy cargo test -p oneterm-pty` — raw: `test result: ok. 22 passed; 0 failed; 0 ignored`
  (plus the empty doc-test target).
- `pwsh scripts/ci-local.ps1` — green end to end on 2026-09-12: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `verify-dependency-graph.py` (20 packages, 20 members), `check-doc-paths.py` (120 paths in
  10 documents), the English checks, `completion-catalog.py validate`, and
  `third-party-notices.py --check` ("THIRD-PARTY-NOTICES.md is up to date").
- `rtk proxy cargo test --workspace`, raw `test result:` lines summed over 47 sections:
  **1153 passed, 0 failed, 5 ignored** (the pre-change baseline was 1131 over 45 sections; the
  22 new tests are `oneterm-pty`'s).
- `grep -rn "alacritty_terminal::tty" crates/` — no match. The stop condition is met.
- `cargo build -p oneterm-app --profile fast-dev` — clean; the build still copies `conpty.dll`
  and `x64/OpenConsole.exe` (both 1.24.2607.10001) next to the exe.

### GUI walk

[`evidence/US-0071-gui-walk.md`](evidence/US-0071-gui-walk.md), with the session transcript
[`evidence/us0071-session-log.txt`](evidence/us0071-session-log.txt) and four screenshots.
Reproduced on the `fast-dev` build of this worktree: the shell prompt and `echo hi`; the
bundled `OpenConsole.exe` (1.24.2607.10001) from this worktree's `target/fast-dev/x64/` as the
console-host child, with `conpty: bundled` logged once; `mode con` reporting 229x52 before the
window resize and 65x33 after it, with the grid reflowed in the screenshot; `ping -t 127.0.0.1`
interrupted, leaving the shell and OneTerm alive; `type snake.six` passing a 257 KiB DCS payload
without wedging the session; and `exit` taking both the shell and its console host away while
OneTerm stayed up.

### Visual GUI walk (2026-09-12, connected desktop)

[`evidence/US-0071-gui-walk-visual.md`](evidence/US-0071-gui-walk-visual.md), nine screenshots
`evidence/US-0071-visual-*.png`. Re-run on `feat/vt-engine` @ `1ef1414` in the main checkout with
the desktop **interactive** (`quser` → `Active`, `GetForegroundWindow()` non-zero and equal to
OneTerm's `hwnd`), so every frame is live and each PNG was re-read and checked against the claim:
the prompt and `echo hi` output; the in-terminal child list naming
`target\fast-dev\x64\OpenConsole.exe` (1.24.2607.10001) as the console host with `conpty: bundled`
logged once; `ping -t 127.0.0.1` stopped by **a real injected Ctrl-C keystroke** (`Control-C`,
`^C`, prompt back, `ping.exe` gone, shell and host alive); twelve ~110-column lines reflowed from
one row each to two when the window was resized from 2560x1032 to 1084x752; `type ..\snake.six`
rendering the image with the prompt on the row below it, matching
`IN-0028`'s `US-0067-rework-prompt-below-image.png`; `日本語 🙂 x` printed at code page 65001 with
the trailing `x` and every following prompt aligned at column 0; and `exit` removing `cmd.exe`
and the bundled `OpenConsole.exe` while OneTerm stayed up.

### Gaps

- **Closed 2026-09-12: the visual GUI walk is done.** The earlier runs captured black/stale
  frames because the Windows session was disconnected (`quser` `Disc`, `GetForegroundWindow() = 0`);
  re-run on a connected desktop, every step was seen on screen —
  [`evidence/US-0071-gui-walk-visual.md`](evidence/US-0071-gui-walk-visual.md). The rendered Sixel
  image, the on-screen echo and the reflowed grid are no longer taken on trust. The earlier
  transport-level evidence (session transcript, child process tree, exit behaviour) stands
  unchanged alongside it; the black captures `evidence/US-0071-verify-*.png` are superseded.
- **Closed 2026-09-12: Ctrl-C is now proven as a keystroke.** The interrupt was injected as a
  real Ctrl+C (`keybd_event` VK_CONTROL + `C` into the focused window), not a console
  `CTRL_C_EVENT`: `ping -t` stopped, `Control-C` / `^C` printed, the prompt returned, and the
  shell and its host survived. The earlier failure was the disconnected session, not the key path.
- **Typed non-ASCII input is still unverified.** In the visual walk, `日本語 🙂` typed through the
  harness's posted `WM_CHAR` reached the shell low-byte-truncated (`å,ž =B`), so the wide-glyph
  step was driven from shell **output** (a UTF-8 file) instead — the side the glyph-width flag
  governs. Whether the truncation is the harness or OneTerm's key path was not chased down; it is
  keyboard-side, outside this packet's transport scope, and no real-keyboard/IME entry of
  non-ASCII has been tested here.
- **`exit` does not close the tab** (found in the visual walk): the shell and the bundled
  `OpenConsole.exe` both go away and the app stays up, but the `Terminal` tab remains with its
  final scrollback — OneTerm has no close-on-exit path (`SessionEvent::Closed` only marks the
  agent ended). The acceptance line that claimed otherwise has been corrected above.
- **Unix is compile-and-CI-tested only.** This box is Windows. The `openpty` backend, the
  reaper thread and the `SignalMask` port have never run here; CI's ubuntu and macOS jobs are
  their only proof, and `docs/PROJECT.md` already records that Linux/macOS are not QA-tested.
- **The glyph-width default changed, on purpose and measured.** See Context: the bundled host
  honours `0x10`, and a ZWJ cluster that the host used to report as 4 columns is now 8, which is
  what the engine already assumed. This is an intended fix, not an unverified risk — but
  `GlyphWidth::Graphemes` itself is still only asserted at flag-bit level, and the flag must flip
  when the engine gains grapheme width (mode 2027, a later intake).
- **Deviations from `pty.md`, all deliberate and all narrower than the design:**
  `Options::drain_on_exit` is not ported (it only ever configured alacritty's own `EventLoop`);
  `ConptyApi::resolve()` is infallible rather than `io::Result` (the `kernel32` fallback cannot
  fail); `ConptyBackend::Bundled` carries no `HMODULE` (the module is deliberately never
  unloaded, so nothing would read it); the Unix `ShellUser`/`getpwuid_r`/macOS `login` path is
  replaced by `$SHELL` (every caller passes an explicit program, resolved by
  `oneterm_core::config::resolve_shell`); and the loopback contract test lives in `oneterm-pty`
  while `crates/local-shell` keeps its own `LoopbackPty` — moving it out would have left the
  local-shell loop tests without a fake PTY, and the local-shell one is the real out-of-crate
  proof.
- **Verifier findings applied after the first commit** (report:
  [`evidence/US-0071-verify.md`](evidence/US-0071-verify.md)): the Unix `Drop` no longer signals a
  pid the reaper may already have released (closing the master hangs the session up instead);
  `spawn` binds the watcher before the struct literal so an error there cannot invert the
  `ClosePseudoConsole` drop order; `DeleteProcThreadAttributeList` is now paired; a failed pipe or
  reaper thread spawn returns `io::Error` instead of logging and leaving a dead session; and
  `child.rs` / `pipe.rs` carry the Apache-2.0 derivation header, with `NOTICE` widened to match.
  Left open by agreement: `ConptyApi::resolve()` still runs per spawn (M7 — one extra module
  reference, no behavioural effect) and neither implementation sets `FD_CLOEXEC` on the Unix pty
  fds (parity with the fork).
- **The 1 MiB pipe ring can still park its thread while `ClosePseudoConsole` drains**, the same
  shape of hazard the fork has. The drop-order invariant is commented and covered by
  `drop_order_drains_the_output_pipe`, but a consumer that stops reading entirely while output
  continues is not covered by a test.

## Rework (2026-09-13): level-triggered wake

**Why.** `US-0083`'s independent verification (§ 4.3 of `evidence/US-0083-verify.md`, which lands
with that packet's own merge and is not on this branch yet) found the Windows ring
registers `PollMode::Level` — `Ring::wake` even keeps the interest registered for it — but
delivers its wake **edge-once**: `push` posts a completion packet only when `caller_waiting` is
set, and `read` set that flag only when it found the ring EMPTY. A caller that honours the
documented contract — read once per readiness, return to the poller — is never woken again and
parks in `poll.wait` forever with output in hand and no diagnostic. `US-0083` hit it for real
(three real-shell tests failed) and worked around it caller-side by never leaving its read loop,
which left the obligation invisible in the type and waiting for the next consumer. This is
acceptance rework of this packet, not a new `BUG`: the ring was never accepted with edge delivery
behind a `PollMode::Level` registration.

**What changed.** `crates/pty/src/windows/pipe.rs`, both directions, no new state and no new
field:

- `PipeReader::read` — one exit path instead of two. After the drain it sets
  `caller_waiting = (bytes left == 0)` and, when bytes **are** left, posts the packet itself.
  That closes two holes at once: the caller's buffer filling (the stall above), and a read that
  drains the ring *exactly*, which previously returned without arming the push-side wake at all.
- `PipeWriter::write` — the mirror: `caller_waiting = (room left == 0)`, and a post when room
  remains. Nothing registers writable interest today (`Ring::wake` gates on
  `registered.event.writable`), so this is contract symmetry rather than an observed defect — but
  it is the same three lines and the same trap for the next consumer.
- The module header now states the level-triggered rule instead of leaving it to the caller.

Cost: one extra IOCP packet per read that leaves bytes behind. `left > 0` implies the caller's
buffer filled, which on ConPTY means a read of ≥ 1 MiB — the `US-0083` verification measured 82 B
at the median and 9.6 KB at the worst, so the shipped local shell posts none of them.

**Unix already honoured the contract and is unchanged.** `crates/pty/src/unix.rs:232-265`
registers the pty master fd itself (`poller.add_with_mode(&self.master, interest, mode)`), so
`PollMode::Level` is epoll/kqueue's own level trigger and bytes left in the kernel buffer
re-deliver on the next `wait`. Only the Windows ring *emulated* readiness, and the emulation was
edge-triggered. The two platforms now make the same promise, which is what makes the
`EventedReadWrite` contract true for a caller written against either.

**Tests** — `crates/pty/src/windows/pipe_tests.rs`, both of which **fail against the pre-rework
code** (verified by reverting the two bodies with the tests in place):

- `a_reader_left_with_bytes_buffered_is_woken_again` — registers `Level`, drains to arm, lets the
  child write 32 bytes, waits for the wake, then reads **8 of them** and polls again: the exact
  "stop reading with bytes buffered" shape. It then drains the remaining 24 exactly and proves a
  further write still wakes. Pre-rework: `a level-triggered reader was not woken with bytes still
  buffered`.
- `a_writer_with_room_left_is_woken_again` — writes 5 bytes into a 4 KiB ring and polls.
  Pre-rework: `a level-triggered writer was not woken with room still left`.

Both use `std::io::pipe()` for the handle pair, so no child process and no ConPTY is involved;
they run in 2 s worst case and are deterministic (the helper waits for the pipe thread to hand
the ring the whole chunk before the partial read).

**Contract text for [`low-level-design/pty.md`](low-level-design/pty.md)**, recorded here for the
design owner to apply — this packet does not edit the LLD:

> **Readiness is level-triggered on both platforms.** A source registered with `PollMode::Level`
> re-announces itself for as long as it stays usable: a caller may read once per readiness, or
> stop reading with bytes still buffered (to yield the engine, or because its buffer filled), and
> its next `poll.wait` still returns. On Unix that is epoll/kqueue on the pty master fd. On
> Windows the ring emulates it: `push`/`pull` post a packet when the caller is waiting on a ring
> that has become usable, and `PipeReader::read` / `PipeWriter::write` re-post when they leave the
> ring usable and arm the thread-side wake when they leave it unusable. Draining to empty before
> returning to the poller is therefore an optimisation — one fewer completion packet — never a
> correctness requirement.

**Verification.** `pwsh scripts/ci-local.ps1` green, exit 0, all ten steps. Raw totals over its
two test steps: **62 sections, 1921 passed / 0 failed / 13 ignored** — `cargo test --workspace`
58 / 1552 / 0 / 10 and `vt-paranoid` 4 / 369 / 0 / 3. The base (`feat/vt-engine` @ `26ca48d`) was
1919 passed, so the delta is exactly the two new tests and nothing else in the workspace moved.
`cargo test -p oneterm-local-shell` — which drives real `cmd.exe` sessions through this reader —
three consecutive runs: `30 passed; 0 failed; 0 ignored` each time, no flake.

**Limits.** `crates/local-shell` is untouched: its loop still drains to empty, which is now an
optimisation rather than the thing holding the session up, and rewriting it belongs to `US-0083`.
No GUI walk — the change is a transport-internal wake, below anything the UI can observe, and its
coverage is the real-shell suite above.

## Handoff

None. `US-0072` (benchmark and parity harness) is independent and touches different files in
`crates/tools`; the engine packets (`US-0073`+) consume nothing from here.
