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
- [ ] Changed
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
      interrupts a running command, reflows on resize, renders a Sixel image, and closes its tab
      on shell exit (GUI walk on Windows).

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
- GUI walk on Windows from this worktree's `fast-dev` build: prompt, `echo hi`, the console host
  child is the bundled `OpenConsole.exe`, Ctrl-C interrupts `ping -t 127.0.0.1`, resize reflows,
  `type snake.six` renders, shell exit closes the tab.

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

### Gaps

- **The visual GUI walk is unverified.** The Windows desktop session was disconnected for both
  the implementer's run and the verifier's re-run (`quser` state `Disc`,
  `GetForegroundWindow() = 0`, `SM_REMOTESESSION = 1`); the verifier's captures came out fully
  black. Nothing that needs eyes — the rendered Sixel image, the on-screen echo, the reflowed
  text — has been seen. **The transport-level evidence stands** (session transcript, child
  process tree, `mode con` dimensions, exit behaviour) and is what every claim below rests on;
  a connected desktop is the only way to close the pixel gap.
- **The Windows session was disconnected for the walk** (`quser` state `Disc`). GPUI stops
  presenting frames for the terminal pane, so the screenshots hold stale frames: **the Sixel
  image and the on-screen echo were not seen**, only proven to have passed through the
  transport. The window chrome and the post-resize reflow did repaint and are genuine.
- **Ctrl-C was a console `CTRL_C_EVENT`, not a keystroke** — the same limitation `US-0070`
  recorded. A posted `VK_CONTROL`+`C` and a `WM_CHAR 0x03` were both tried first and ignored,
  because GPUI reads the modifier state a posted message does not set. OneTerm's own
  key-to-`0x03` encoding is untouched by this packet.
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

## Handoff

None. `US-0072` (benchmark and parity harness) is independent and touches different files in
`crates/tools`; the engine packets (`US-0073`+) consume nothing from here.
