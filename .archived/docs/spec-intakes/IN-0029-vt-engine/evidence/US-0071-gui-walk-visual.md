# US-0071 — GUI walk, seen (connected desktop)

Date: 2026-09-12
Branch/commit: `feat/vt-engine` @ `1ef1414`, main checkout `D:\TrungKFC-Research\Rust\myTerm2`.
Build: `cargo build -p oneterm-app --profile fast-dev` (finished in 47 s), run as
`target\fast-dev\oneterm.exe`, pid **20264**.
Bundled console host next to the exe: `target\fast-dev\conpty.dll` and
`target\fast-dev\x64\OpenConsole.exe`, both file version **1.24.2607.10001**.
Driver: the scratchpad `gui.ps1` family (`guiv.ps1` — posted `WM_CHAR` typing, `PrintWindow`
captures, `MoveWindow` resize) plus `realinput.ps1` for one **real** injected keystroke.
Scratch `HOME`/`USERPROFILE`: `…\scratchpad\home71g`; working directory: the repo root.

## Session was interactive this time

This is the re-run the earlier walk ([`US-0071-gui-walk.md`](US-0071-gui-walk.md)) asked for.

- `quser` → `trunglt  rdp-tcp#0  1  Active` (the earlier runs reported `Disc`).
- `GetForegroundWindow()` → `3278732`, non-zero and equal to OneTerm's own `hwnd` after the
  driver focused it (the earlier runs got `0`).

Consequence: GPUI presents frames for the terminal pane, so every screenshot below is a live
frame, and real keystrokes can be injected. Each PNG was re-read and checked after capture.

## Steps

### (a) Prompt and `echo hi` — [`US-0071-visual-a-prompt-echo.png`](US-0071-visual-a-prompt-echo.png)

Visible: `…\scratchpad\home71g>echo hi`, the output line `hi`, and a fresh prompt with the block
cursor on it. A local `cmd` tab, opened by the app at startup.

### (b) The console host child is the bundled `OpenConsole.exe` — [`US-0071-visual-b-process-tree.png`](US-0071-visual-b-process-tree.png)

The screenshot shows the child list printed **inside the terminal** by
`powershell -NoProfile -c "gcim win32_process | ? ParentProcessId -eq 20264 | ft ProcessId,Name,ExecutablePath -Auto"`:

```
ProcessId Name            ExecutablePath
    19032 conhost.exe     C:\WINDOWS\system32\conhost.exe
    11928 OpenConsole.exe D:\TrungKFC-Research\Rust\myTerm2\target\fast-dev\x64\OpenConsole.exe
    18156 cmd.exe         C:\WINDOWS\system32\cmd.exe
```

`conhost.exe` is the debug build's own console window; the pseudo-console host is the **bundled**
`OpenConsole.exe` from `target\fast-dev\x64\`, not the inbox one. `Get-CimInstance Win32_Process`
filtered by `ParentProcessId` from outside the app returns the same three rows. The app log
carries the resolution line once:

```
[2026-09-12T10:18:14Z INFO  oneterm_pty::windows::conpty] conpty: bundled
```

### (c) Ctrl-C interrupts `ping -t 127.0.0.1` — [`US-0071-visual-c1-ping-running.png`](US-0071-visual-c1-ping-running.png), [`US-0071-visual-c2-ctrlc-stopped.png`](US-0071-visual-c2-ctrlc-stopped.png)

`c1`: `ping -t 127.0.0.1` running, nine `Reply from 127.0.0.1 …` lines on screen, `ping.exe`
pid 4236 alive.

`c2`: after **a real Ctrl-C keystroke** — `keybd_event` VK_CONTROL + `C` down/up into the focused
OneTerm window (`realinput.ps1 ctrlc`), not a console `CTRL_C_EVENT` and not a posted message —
the pane shows the `Ping statistics` block, `Control-C`, `^C`, and the prompt back. `ping.exe` is
gone; `cmd.exe` (18156) and `OpenConsole.exe` (11928) are still alive, so the interrupt reached
only the child. **This closes the "Ctrl-C was a console control event" gap from the earlier
walk** — the previous failure was the disconnected session, not the key path.

### (d) Twelve long lines reflow on resize — [`US-0071-visual-d1-wide-lines.png`](US-0071-visual-d1-wide-lines.png), [`US-0071-visual-d2-reflow-narrow.png`](US-0071-visual-d2-reflow-narrow.png)

`for /L %i in (1,1,12) do @echo LINE-%i the-quick-brown-fox-…-END%i` printed twelve ~110-column
lines.

`d1` (maximized, client 2560x1032): each `LINE-n` occupies one row, the command itself wraps onto
a second row.

`d2` (driver `resize 1100 760`, client 1084x752): the same twelve lines, each now wrapped across
**two** rows ending `…-END1` … `…-END12` in order, the command line re-wrapped over four rows and
the trailing prompt re-wrapped over three. Existing scrollback was reflowed, not truncated, and
the grid kept its order.

### (e) Sixel through the pseudo-console — [`US-0071-visual-e-sixel.png`](US-0071-visual-e-sixel.png)

`type ..\snake.six` (a 257 KiB DCS payload) renders the snake photograph in the pane, with the
next shell prompt on the row directly below the image — the same layout as
[`../../IN-0028-sixel-graphics/evidence/US-0067-rework-prompt-below-image.png`](../../IN-0028-sixel-graphics/evidence/US-0067-rework-prompt-below-image.png)
(same image, prompt below, no overlap and no scroll jump). **The rendered image has now been
seen**; the earlier walk could only prove the bytes passed through.

### (f) Wide glyphs and the following prompt — [`US-0071-visual-f-wide-glyphs.png`](US-0071-visual-f-wide-glyphs.png)

`chcp` reports `Active code page: 65001` (the local-shell spawn already passes `/K chcp 65001`).
The shell printed `日本語 🙂 x` (`type ..\wide.txt`, a UTF-8 file), then `echo abc`.

Visible: the three CJK glyphs and the emoji render at their double-width cells, the trailing `x`
follows them on the same row, and the next prompt, the `abc` output and the final prompt all
start at column 0, aligned with every other prompt in the pane. Nothing is shifted, overwritten
or left behind by a column-count mismatch, which is what a wrong `CreatePseudoConsole` glyph-width
flag (`0x10`) would produce here.

### (g) `exit` — [`US-0071-visual-g-after-exit.png`](US-0071-visual-g-after-exit.png)

After `exit`: OneTerm's children are its own debug `conhost.exe` plus the transient `git.exe`
processes of the status bar — **`cmd.exe` and the bundled `OpenConsole.exe` are both gone** —
and the app is alive with the pane holding its final output.

**Finding: the tab does not close.** The acceptance wording "closes its tab on shell exit" does
not match the app: `SessionEvent::Closed` in `crates/terminal-view/src/terminal_view/view.rs`
only marks the agent ended (plus an SSH-only notification); there is no close-on-exit path, by
design and unchanged by this packet. The screenshot shows the `Terminal` tab still present, with
the dead session's scrollback, ten seconds after `exit`. What US-0071 owns — the child exit
reaching the app and `ClosePseudoConsole` taking the host down without a hang — is what the
process list proves.

## What failed / was not seen

- **Typed non-ASCII input is truncated.** Typing `echo 日本語 🙂 x` through the driver's posted
  `WM_CHAR` reached the shell as `å,ž =B x`, i.e. every UTF-16 unit arrived as its low byte
  (`0x65E5` → `0xE5`). It was not chased down: it is an input-path/harness artifact on the
  keyboard side, not the pseudo-console transport this packet owns, and step (f) was therefore
  driven from shell **output** (a UTF-8 file), which is the side the glyph-width flag governs.
  Real-keyboard/IME entry of non-ASCII is still unverified here.
- **Unix** is untouched: this box is Windows.
- The earlier black-screen captures (`US-0071-verify-*.png`) are superseded by the PNGs above.
