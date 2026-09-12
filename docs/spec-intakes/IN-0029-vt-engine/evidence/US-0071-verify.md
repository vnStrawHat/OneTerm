# US-0071 — independent verification

Verifier: separate agent session, 2026-09-12. Branch `worktree-agent-a665224a2944fdbdd` @ `f729107`,
worktree `.claude/worktrees/agent-a665224a2944fdbdd` (its own `target/`; the main checkout was only
read). Nothing was fixed and nothing was committed.

**Verdict: merge after fixes.** The Windows path is a faithful, cleaner re-implementation of the
fork and the packet is unusually honest. Two one-line code fixes are wanted first (one Unix
correctness regression against the code being replaced, one drop-order hazard on a spawn error
path) plus two evidence corrections. Nothing found is a blocker for the Windows behaviour.

## Pass / fail

| # | Check | Result | Raw evidence |
|---|---|---|---|
| 1a | Scope: new crate + local-shell/tools/docs/policy/NOTICE only | **pass** | `git diff feat/vt-engine...HEAD --stat` = 35 files: `crates/pty/*` (new, 7 files), `crates/local-shell/*` (6), `crates/tools/*` (2), `Cargo.{toml,lock}`, `NOTICE`, `README.md`, `scripts/dependency-graph-policy.json`, `docs/*` + the packet and its evidence. |
| 1b | Nothing under `crates/terminal`, `terminal-view`, `ssh`, `vendor/` | **pass** | `git diff feat/vt-engine...HEAD --stat -- crates/terminal crates/terminal-view crates/ssh vendor/` → empty. |
| 1c | `grep -rn "alacritty_terminal::tty" crates/` empty | **pass** | no match. |
| 1d | `alacritty_terminal` still compiled where `Term` is used | **pass** | `crates/local-shell/Cargo.toml:21 alacritty_terminal.workspace = true`; `event_loop.rs` still uses `alacritty_terminal::{sync::FairMutex, term::Term}`. Also kept by `terminal`, `ssh`, `terminal-view`. |
| 2a | `pwsh scripts/ci-local.ps1` green | **pass** | ran here, `EXIT=0`, `ci-local: all checks passed.` — fmt, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, graph policy, doc paths, English checks, completion catalogs, notices. |
| 2b | Raw `test result:` lines summed | **pass** | 47 sections, all `ok`: **1153 passed, 0 failed, 5 ignored** — exactly the packet's figure. `oneterm-pty` section: `running 22 tests … test result: ok. 22 passed; 0 failed; 0 ignored`. |
| 2c | `verify-dependency-graph.py` with the new L0 crate | **pass** | `Dependency graph policy passed for 20 workspace packages and 20 explicit members.` `oneterm-pty: []` in the policy; `crates/pty/Cargo.toml` has no OneTerm dependency and only `polling`, `log`, `windows-sys` (win), `libc` (unix). |
| 3a | Windows pipes / handle ownership | **pass, improved** | `anonymous_pipe()` (conpty.rs:327) returns `OwnedHandle`s with `bInheritHandle: 0`; the two host ends are dropped right after `CreatePseudoConsole` (conpty.rs:249-250) — the fork leaked them via `into_raw_handle()`. `process.hThread` is closed (conpty.rs:314), `hProcess` moves into an `OwnedHandle` owned by the watcher. Every early return drops owned handles. |
| 3b | `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` setup | **pass** | `ProcThreadAttributeList::with_capacity(1)` + `set_pseudoconsole` + `STARTUPINFOEXW`/`EXTENDED_STARTUPINFO_PRESENT`/`STARTF_USESTDHANDLES` with null std handles — same sequence as the fork, with the two-call size probe error-checked. |
| 3c | `DeleteProcThreadAttributeList` | **minor finding M3** | never called (parity with the fork, but the type's doc comment claims RAII freeing). |
| 3d | `ClosePseudoConsole` ordering vs draining | **pass for the struct, minor finding M2 for one error path** | `PseudoConsole.conpty` is the first field with the invariant comment (windows.rs:33-43) and `drop_order_drains_the_output_pipe` proves it (20 s budget, passes). The `spawn` early-return path inverts it — see M2. |
| 3e | Child-exit detection + `PTY_CHILD_EVENT_TOKEN` | **pass, improved** | `RegisterWaitForSingleObject(WT_EXECUTEINWAITTHREAD \| WT_EXECUTEONLYONCE)` → `GetExitCodeProcess` → `mpsc` + IOCP `CompletionPacket`; `UnregisterWaitEx(INVALID_HANDLE_VALUE)` in `Drop` (child.rs:158) blocks until a running callback returned, so the callback's raw `Arc` pointer is sound — the fork used `UnregisterWait` plus `Box::from_raw` in the callback, which leaks when the callback never runs and races when it does. `register()` sets key = `PTY_CHILD_EVENT_TOKEN` (windows.rs:88-89) and `ShellEventLoop` dispatches on exactly that key (event_loop.rs:352). Tests: `child_exit_is_reported_once_with_its_code`, `instant_exit_is_not_missed`, `deregistering_keeps_the_exit_observable`. |
| 3f | `ResizePseudoConsole` error handling | **pass** | returns `io::Error::other("ResizePseudoConsole to {cols}x{rows} failed (HRESULT …)")` (conpty.rs:184-194) instead of the fork's `assert_eq!(result, S_OK)`; the caller logs at `warn` and keeps the session (event_loop.rs:308-315). Tests `a_rejected_resize_is_an_error_not_a_panic` on both platforms. |
| 3g | env / working directory / `escape_args` parity | **pass** | `environment_block` is the fork's algorithm (case-insensitive dedup, user wins, parent appended, double NUL); `push_escaped_arg` and `cmdline` are logically byte-for-byte the fork's (diffed against `vendor/alacritty_terminal/src/tty/windows/mod.rs`), with the fork's quoting table ported. `local-shell` still passes `escape_args: true`. |
| 3h | `conpty.dll` → `kernel32` order + the test that pins it | **pass** | `ConptyApi::resolve()` → `resolve_in(exe_dir)` → `LoadLibraryW(<exe dir>\conpty.dll)`, `unwrap_or_else(Self::system)`; missing export logs `warn` and falls back. `conpty_api_prefers_the_bundled_host` stages the real `crates/app/assets/conpty.dll`, `conpty_api_falls_back_to_the_system_host` uses an empty directory — both pass. Confirmed live: `[INFO oneterm_pty::windows::conpty] conpty: bundled`, once. Note (not a defect): the fork used a bare `LoadLibraryW("conpty.dll")`, i.e. the full DLL search order including `PATH`; the new code loads only the absolute exe-dir path, which matches pty.md and removes a PATH-hijack vector. |
| 3i | `CreatePseudoConsole` flags: 0 → 0x10 | **behaviour change confirmed, and it IS declared — but the packet's gap text is wrong (M6)** | 0x10 is `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH`, not GRAPHEMES (0x08); the constant names in `conpty.rs:40-47` are right and `glyph_width_selects_the_documented_flag` pins the bits. `local-shell/src/session.rs:59` passes `GlyphWidth::WcsWidth` explicitly (also the `Default`), so **every OneTerm local shell now passes 0x10 where the fork passed 0**. Measured A/B, same bundled host 1.24.2607.10001 (see below). |
| 3j | Unix `openpty` / session / ctty | **pass** | `libc::openpty` → `setsid` → optional `chdir` → `TIOCSCTTY` → close both pty fds in the child → optional mask → `SIG_DFL` for SIGCHLD/HUP/INT/QUIT/TERM/ALRM. Runs after std's stdio `dup2`, so closing the originals is safe; `slave` is dropped in the parent so the master sees EOF. |
| 3k | Unix reaper thread (zombie reaping, drop) | **major finding M1** | the thread owns the `Child` and blocks in `wait()` (reaps, no `SIGCHLD` handler — a genuine improvement over `signal-hook`), but `Drop` kills by *pid* after that reap may already have happened. |
| 3l | `set_nonblocking`, `close_on_exec` | **pass / parity note** | `set_nonblocking` is `F_GETFL`+`F_SETFL\|O_NONBLOCK`, both results checked (the fork ignored them). Neither implementation sets `FD_CLOEXEC` on the master/slave: the child closes both explicitly, but a *concurrent* `Command::spawn` elsewhere in the app can still inherit the master fd. Parity with the fork; worth an `O_CLOEXEC` follow-up, not this packet. |
| 4 | Error policy | **pass, with M4** | `grep -n "unwrap()\|expect(\|panic!\|assert"` over the five non-test source files → **no match** (only `unwrap_or_else(PoisonError::into_inner)`). Every failure is an `io::Error` carrying context (`cannot start '<program>': …`, `ResizePseudoConsole to …`, `ioctl TIOCSWINSZ to …`); logging is `info` once for the backend, `warn` for a degraded bundled host and a dropped duplicate env key, `debug` for a pipe that ended, `error` only for a failed thread spawn (see M4). Every `unsafe` block carries a SAFETY comment (code-style). |
| 5 | Attribution | **pass, with M5** | `windows.rs:1-6` and `windows/conpty.rs:1-6` name alacritty, Apache-2.0 and "modified" (Apache-2.0 §4(b)); `NOTICE` gains the matching paragraph; `python scripts/third-party-notices.py --check` → `THIRD-PARTY-NOTICES.md is up to date.` (no new external crate). Consistent with `docs/license-analysis.md` §3. |
| 6 | GUI walk, repeated | **partial — same blocker as the implementer, plus new numeric evidence** | see below. |
| 7 | Packet completeness | **pass** | every `docs/templates/work.md` heading present and in order, both HARNESS blocks intact; acceptance ticks each map to something I re-ran; `harness.db` row `US-0071` exists in the main checkout (read-only sqlite3): status `implemented`, risk_lane `high_risk`, all four proofs = 1, `last_verified_result = pass`, contract/packet doc paths correct, intake_id 34. The Gaps section is honest about the disconnected session, the console `CTRL_C_EVENT`, Unix being CI-only and the five pty.md deviations — except M6. |

## GUI walk (re-run by the verifier)

Build: `cargo build -p oneterm-app --profile fast-dev` in this worktree (up to date at `f729107`).
Driver: the scratchpad `gui-wt.ps1` (posted `WM_CHAR`/`WM_KEYDOWN`, `PrintWindow`), `Start-Process`
with redirected output, `-WorkingDirectory` = this worktree, scratch `HOME`/`USERPROFILE`, always
targeting my own pid. Transcripts: [`US-0071-verify-session-log.txt`](US-0071-verify-session-log.txt).

**The desktop is still NOT interactive.** `quser` → `trunglt … 1 Disc`; `GetForegroundWindow()` → `0`;
`GetSystemMetrics(SM_REMOTESESSION)` → `1`. For my launches GPUI presented *no* frame at all:
`US-0071-verify-01/20/25/30-*.png` are 100 % black (12 KB each), `-11-after-resize.png` likewise.
So this run is *weaker* than the implementer's on pixels (they at least captured stale frames) and
stronger on numbers. Anything that needs eyes is still unverified.

| Required observation | Result |
|---|---|
| prompt + `echo hi` visible | **transport yes, pixels no** — `…\home71v>echo hi` / `hi` in the transcript; the screenshot is black. |
| host child is the bundled OpenConsole.exe 1.24 | **pass** — children of my app pid: `OpenConsole.exe  …\agent-a665224a2944fdbdd\target\fast-dev\x64\OpenConsole.exe` + `cmd.exe`; `conpty: bundled` logged once; both bundled files report 1.24.2607.10001. |
| Ctrl-C keystroke interrupts `ping -t 127.0.0.1` | **not reproducible here** — a posted `VK_CONTROL`+`C` left `ping` running (confirmed by process check); a console `CTRL_C_EVENT` interrupted it (`Control-C` / `^C` in the transcript) and left cmd.exe and OneTerm alive. Exactly the implementer's finding; the limitation is the disconnected session, not the packet. |
| resize reflows | **child yes, pixels no** — `mode con` reports 229x52 before and 65x33 after the window resize, so `ResizePseudoConsole` reached the host and the child. |
| `type snake.six` shows the image with the prompt below | **transport yes, pixels no** — the 257 KiB DCS went through and the session stayed healthy (`echo after-sixel` → `after-sixel`); the image itself was not seen, so no comparison with `IN-0028/evidence/US-0067-rework-prompt-below-image.png` was possible. |
| CJK/emoji line keeps the cursor aligned | **pass, measured numerically** — see below. |
| `exit` closes the tab and the host process | **pass** — after `exit` the app's only remaining child is its own debug `conhost.exe`; `cmd.exe` **and** the bundled `OpenConsole.exe` are gone, OneTerm alive. That is the full `ChildEvent::Exited` → drop → `ClosePseudoConsole` path, and a wrong field order would have hung it. |

### The glyph-width flag, measured instead of guessed

Pixels were not available, so I read the **host's own cursor column** (`[Console]::CursorLeft`)
after writing a CJK line and a ZWJ family cluster inside the OneTerm shell, and ran the identical
probe against the **pre-US-0071 binary** (`D:\…\myTerm2\target\fast-dev\oneterm.exe`, built
2026-09-11, i.e. `alacritty_terminal::tty` with flags = 0) copied into the scratchpad next to the
**same** `conpty.dll` + `x64\OpenConsole.exe` 1.24.2607.10001:

| Host configuration | `[日本語 🙂 x]` | `[👨‍👩‍👧]` |
|---|---|---|
| previous implementation, flags = 0 | 13 | **4** |
| US-0071, flags = 0x10 `GLYPH_WIDTH_WCSWIDTH` | 13 | **8** |
| system conhost 10.0.26100 (no ConPTY) | 13 | 4 |

Reading: the bundled host *does* honour the bit, its default (flag 0) is grapheme measurement, and
0x10 switches it to per-codepoint `wcswidth`. Per-codepoint is what the current engine does
(`unicode-width` per `char`: 👨 2 + ZWJ 0 + 👩 2 + ZWJ 0 + 👧 2 = 6, i.e. 8 with the brackets), so
**the change moves the host into agreement with the engine and removes a 4-column drift that
existed before this packet**. Plain CJK and single emoji are unaffected. This is a real default
change, it is declared in the packet's Context, and it is an improvement — but see M6.

## Findings

### Major

**M1 — Unix: `Drop` can signal a recycled pid.** `crates/pty/src/unix.rs:219-227`

```rust
impl Drop for PseudoConsole {
    fn drop(&mut self) {
        unsafe { libc::kill(self.pid as libc::pid_t, libc::SIGHUP) };
    }
}
```

The reaper thread owns the `Child` and calls `wait()` (unix.rs:205), which reaps the zombie and
releases the pid for reuse. If the child exited earlier in the session — the common case, since the
session is usually dropped *because* the child exited — this `kill` is aimed at a pid the kernel may
have reassigned. The code being replaced was safe by construction: the fork owned the `Child` and
killed *before* waiting (`vendor/alacritty_terminal/src/tty/unix.rs:355-366`). This is the one place
where the rewrite is less correct than the original.

*Fix (laziest that is right):* delete the `kill` and let the last close of the master fd hang up the
session — `self.master` is dropped immediately after `Drop::drop` returns and the tty layer sends
`SIGHUP` to the foreground process group for us. If an explicit signal is still wanted, gate it on an
`Arc<AtomicBool>` the reaper sets *before* `wait()` returns, and accept that the window is only
narrowed, not closed.

### Minor

**M2 — the drop-order invariant does not hold on `spawn`'s last error path.**
`crates/pty/src/windows/conpty.rs:318-323`

```rust
Ok(PseudoConsole {
    conpty,
    conout: PipeReader::new(conout, PIPE_CAPACITY),
    conin: PipeWriter::new(conin, PIPE_CAPACITY),
    child: ChildExitWatcher::new(process_handle)?,   // <- early return here
})
```

If `ChildExitWatcher::new` fails, the already-evaluated field values are dropped in reverse order of
evaluation — `conin`, then `conout`, then `conpty` — so `ClosePseudoConsole` runs *after* the conout
reader end has been told to close: precisely the deadlock the struct comment at `windows.rs:33-43`
forbids. *Fix:* one line — `let child = ChildExitWatcher::new(process_handle)?;` before the struct
literal. Locals then drop in reverse *declaration* order, i.e. `conpty` (line 254) before `conout`
(line 226), which is the correct order.

**M3 — `DeleteProcThreadAttributeList` is never called.** `crates/pty/src/windows/conpty.rs:350-403`.
The type's doc comment says "sized and freed by RAII" but only the `Box<[u8]>` is freed; MSDN
requires the paired delete. Parity with the fork, so not a regression. *Fix:* `impl Drop for
ProcThreadAttributeList { fn drop(&mut self) { unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) } } }`,
or correct the comment.

**M4 — a failed thread spawn degrades into a silently dead session.**
`crates/pty/src/windows/pipe.rs:332-339` and `crates/pty/src/unix.rs:211-216` log at `error` and
carry on: on Windows the session would never read or write a byte, on Unix the child exit would
never be reported and the tab would never close. `docs/agents/error-policy.md` (transport row) wants
a typed error here. *Fix:* return `io::Result` from `PipeReader::new` / `PipeWriter::new` /
`reap_in_background` and `?` them in `spawn` — the caller already surfaces a spawn failure as a
notification.

**M5 — attribution is narrower than the derivation.** `NOTICE` and the packet say "two fragments".
`crates/pty/src/windows/child.rs` reproduces the fork's design closely (`Interest` /
`ChildExitSender` / `extern "system"` callback → `mpsc` + IOCP `CompletionPacket`, the same
`register`/`deregister` shape), and `pipe.rs` mirrors `blocking.rs`'s registration semantics
(readable/writable gating, oneshot clearing, the priming packet on first registration) — neither
file carries a header. `win32_string` is the fork's line. *Fix (cheap insurance, no legal opinion
needed):* add the same one-line "adapted from alacritty_terminal, Apache-2.0, modified" header to
`child.rs` and `pipe.rs` and widen the `NOTICE` sentence from "two fragments" to "several
fragments and the Windows backend's structure".

**M6 — two evidence sentences in the packet are wrong, in opposite directions.**
`US-0071-pty-crate.md`, Context: "The GUI walk includes a CJK line so a width regression would be
visible" — the walk in `evidence/US-0071-gui-walk.md` contains no CJK line. Gaps: "`GlyphWidth::Graphemes`
is unverified behaviourally … no width change was observed in the walk, but no host known to honour
the bit was tested" — the host OneTerm ships *does* honour the bit and the default *did* change
(4 → 8 columns for a ZWJ cluster). *Fix:* replace both with the A/B table above; the change stays
declared and is an improvement, but the packet currently understates what it does.

**M7 (nit) — `ConptyApi::resolve()` runs per spawn, not per process.** `conpty.rs:92`. pty.md says
"called once per process"; only the log line is `Once`. `LoadLibraryW` on an already-loaded module
is cheap but takes another module reference that is never released. A `OnceLock<…>` (or just
copying the resolved function pointers) would match the design.

## What the verifier did not check

- Linux/macOS at runtime: this box is Windows; the Unix backend was read, not executed (M1 is from
  reading, not from a reproduction).
- Anything requiring pixels: the terminal image, the on-screen echo, the reflow, and the
  Sixel-vs-`US-0067` comparison. `quser` state `Disc` + `GetForegroundWindow() = 0` on both the
  implementer's run and mine; a connected desktop is the only way to close that gap.
- Sustained-throughput behaviour of the new ring (`pty-throughput` was not run here; CI's tests are
  the only coverage) and the "consumer stops reading entirely" hazard the packet already records.
