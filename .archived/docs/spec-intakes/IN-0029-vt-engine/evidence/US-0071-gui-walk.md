# US-0071 — GUI walk

Date: 2026-09-12
Build: `cargo build -p oneterm-app --profile fast-dev` from the worktree
`.claude/worktrees/agent-a665224a2944fdbdd` (branch `worktree-agent-a665224a2944fdbdd`,
off `feat/vt-engine` @ `852206d`).
Driver: `gui-wt.ps1` (the scratchpad `gui.ps1` with the working directory as a parameter, so
the app runs from this worktree and reads its own `target/*.json`), posted `WM_CHAR` /
`WM_KEYDOWN`, `PrintWindow` captures.

## How this walk was recorded

**The Windows session was disconnected (`quser` reports state `Disc`) for the whole walk.**
Two consequences, both visible below:

1. **Real keystrokes cannot be injected** and posted `WM_KEYDOWN` does not set the modifier
   state GPUI reads, so a posted Ctrl+C is ignored (confirmed: the `ping` kept running through
   both a posted `VK_CONTROL`+`C` pair and a `WM_CHAR 0x03`). Ctrl-C was therefore sent as a
   console `CTRL_C_EVENT` into the console OneTerm's bundled host owns, exactly as `US-0070`'s
   walk had to.
2. **GPUI stops presenting frames for the terminal pane**, so `PrintWindow` returns the last
   frame it did present. The window chrome and the layout do repaint (the resize screenshot is
   a real, fresh frame), but terminal *content* in the screenshots is stale.

So the walk's terminal evidence is the **session log** instead of pixels: OneTerm's own
printable-output logging (`docs/terminal-logging.md`) was switched on for this run
(`logging.local = true` in the worktree's `target/terminal.json`, restored afterwards), which
records every printable line the pseudo-console delivered. The full transcript is
[`us0071-session-log.txt`](us0071-session-log.txt); the steps below quote it.

## Steps

### 1. A local shell opens and echoes

```
[2026-09-12 14:00:22] …\scratchpad\home71>echo hi
[2026-09-12 14:00:22] hi
```

Screenshot: [`us0071-01-prompt-echo.png`](us0071-01-prompt-echo.png) (prompt visible; the
`echo` output is not in this frame — see "How this walk was recorded").

### 2. The console host child is the bundled OpenConsole, not System32 conhost

Children of the OneTerm process during the walk:

```
2940   conhost.exe        C:\WINDOWS\system32\conhost.exe          <- the debug build's own console window
916    OpenConsole.exe    …\agent-a665224a2944fdbdd\target\fast-dev\x64\OpenConsole.exe
8412   cmd.exe            C:\WINDOWS\system32\cmd.exe              <- "cmd.exe /K chcp 65001 >nul"
```

`OpenConsole.exe` and `conpty.dll` both report file version **1.24.2607.10001**, and the app
log carries the crate's resolution line exactly once per process:

```
[2026-09-12T07:00:15Z INFO  oneterm_pty::windows::conpty] conpty: bundled
```

That is `ConptyApi::resolve()` choosing the bundled DLL over `kernel32` (DEC-0013), which the
unit test `conpty_api_prefers_the_bundled_host` pins in both directions.

### 3 and 4. Resize reaches the child, and the grid reflows

`mode con` reports the size the child sees, before and after the window was resized from
maximized (2560x1032) to 1100x700:

```
[2026-09-12 14:00:26]     Lines:          52        Columns:        229
[2026-09-12 14:00:35]     Lines:          33        Columns:         65
```

`ResizePseudoConsole` therefore reached the host and the child. The reflow itself is visible in
[`us0071-11-after-resize.png`](us0071-11-after-resize.png) — a fresh frame at the new size, with
the prompt re-wrapped for the narrower grid.

### 5. Ctrl-C interrupts `ping -t 127.0.0.1`

```
[2026-09-12 14:00:43] …>ping -t 127.0.0.1
[2026-09-12 14:00:43] Pinging 127.0.0.1 with 32 bytes of data:
[2026-09-12 14:00:43] Reply from 127.0.0.1: bytes=32 time<1ms TTL=128
…
[2026-09-12 14:00:48] Ping statistics for 127.0.0.1:
[2026-09-12 14:00:48]     Packets: Sent = 6, Received = 6, Lost = 0 (0% loss),
[2026-09-12 14:00:48] Control-C
[2026-09-12 14:00:48] ^C
```

Afterwards: `ping` gone, `cmd.exe` alive, OneTerm alive — the interrupt reached only the child
group, and the post-interrupt output still flowed through the transport.

**Sent as a console `CTRL_C_EVENT`, not as a keystroke** (see above). This exercises the console
host's control-event routing and the PTY read path, **not** OneTerm's own key encoding.

### 6. A Sixel image through the pseudo-console

```
[2026-09-12 14:00:57] …>type …\scratchpad\snake.six
[2026-09-12 14:01:06] …>echo after-sixel
[2026-09-12 14:01:06] after-sixel
```

The 257 KiB DCS payload passed through without an error from `type` and without wedging the
session: the next command ran normally. The session log records no Sixel bytes because only
printable text is logged, which is what a consumed DCS looks like.

**The rendered image was not seen** ([`us0071-20-sixel.png`](us0071-20-sixel.png) is a stale
frame). The transport-side property Sixel depends on — the bundled host, which the inbox
`conhost.exe` cannot substitute for — is proven by step 2 and by
`conpty_api_prefers_the_bundled_host`.

### 7. `exit` ends the session

```
[2026-09-12 14:01:10] …>exit
```

After it, the OneTerm process's only remaining child is its own debug console window:

```
2940   conhost.exe        C:\WINDOWS\system32\conhost.exe
```

`cmd.exe` **and** the bundled `OpenConsole.exe` are both gone, and OneTerm is still running.
That is the whole shutdown path: the child-exit wait callback → `ChildEvent::Exited` on
`PTY_CHILD_EVENT_TOKEN` → the session closes → `PseudoConsole` drops →
`ClosePseudoConsole` (which blocks until the conout pipe drains) → the host exits. A wrong
field order in `PseudoConsole` would have hung here instead.
Screenshot: [`us0071-30-after-exit.png`](us0071-30-after-exit.png) (stale frame).

## Not verified

- **The Sixel image rendering and the echoed text on screen** — the disconnected session stops
  GPUI presenting frames for the terminal pane. Re-run on a connected desktop to confirm
  visually.
- **Ctrl-C as a keystroke** — sent as a console control event instead, for the reason above.
  OneTerm's key-to-`0x03` encoding is unchanged by this packet and is covered by
  `crates/terminal`'s key-encoding tests.
- **Unix** — this box is Windows; the `openpty` backend is exercised only by CI's ubuntu and
  macOS jobs.
