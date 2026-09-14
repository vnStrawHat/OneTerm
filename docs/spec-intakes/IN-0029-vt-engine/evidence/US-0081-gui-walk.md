# US-0081 — GUI walks: the new engine behind the running application

Date: 2026-09-12
Branch: `worktree-agent-a53e46421076a2d27`, off `feat/vt-engine` @ `458aa78`, with
`US-0080` merged at `3538047`.
Build: `cargo build -p oneterm-app --profile fast-dev` from this worktree
(`target/fast-dev/oneterm.exe`); the measurement runs add `--features terminal-diagnostics`.
Platform: Windows 11 Enterprise 10.0.26200, Zed One Dark, Lilex 15 px, maximized client
2560x1032 (grid 52 rows), local `cmd.exe`, bundled console host
(`conpty: bundled` in every log).
Driver: the scratchpad `gui81.ps1` (posted window messages + `PrintWindow`), plus
`realinput.ps1` for the one **real** Ctrl-C keystroke. Every run launches its own
`oneterm.exe`, keeps that pid, and touches no other OneTerm process. Every PNG below was
read back after capture.

## Result summary

| # | Walk | Item | Result |
| --- | --- | --- | --- |
| 1 | US-0071 | Prompt, `echo`, shell alive | PASS |
| 2 | IN-0018 | Render sampler: box, blocks, braille, powerline, SGR, 256/truecolor, CJK, emoji, combining | **PASS — pixel-identical to the old engine** |
| 3 | US-0071 | Real Ctrl-C stops `ping -t` | PASS |
| 4 | US-0071 | Twelve long lines reflow narrow, then re-join on widen | PASS |
| 5 | IN-0018 | Selection drag, double-click word | PASS |
| 6 | IN-0028 | Sixel: image at the cursor, prompt below it, `cls` removes it | PASS |
| 7 | US-0071 | `exit` | **NOT SEEN** — the driver's posted PageUp reached `cmd` as history recall and spoiled the step (harness, not engine) |
| 8 | IN-0027 | Font fallbacks and ligatures | **NOT RUN** — the RDP session disconnected mid-walk; see Gaps |

No rendering or behaviour defect was found in the new engine.

## 1-2. Prompt, and the render sampler — the parity result

[`US-0081-01-prompt-echo.png`](US-0081-01-prompt-echo.png),
[`US-0081-02-render-sampler-cmd-type.png`](US-0081-02-render-sampler-cmd-type.png),
[`US-0081-04-sampler-via-powershell.png`](US-0081-04-sampler-via-powershell.png)

`us0081-sampler.txt` holds six rows with raw ESC bytes: box drawing, block elements and
shades, braille and two powerline code points, every SGR attribute the renderer draws
(bold, dim, italic, underline, curly underline, strikeout, inverse), colours (named,
256-palette, truecolor, background) and a CJK + emoji + combining-mark row. It was printed
both with cmd's `type` and through `Get-Content -Encoding utf8`, with identical results.

**The decisive check.** The same file was printed on the **old** engine — a copy of the
main checkout's `fast-dev` binary at `feat/vt-engine` @ `458aa78`, run against this
worktree's `target/` so both read the same `terminal.json` — and the two captures were
compared pixel by pixel over the sampler block:

| Row band (original px) | Differing pixels |
| --- | --- |
| y 125-149 (the tail of the wrapped command line) | 82 of 17 500 |
| y 130-149, y 150-174, 175-199, 200-224, 225-249, 250-274 (BOX, BLK, BRL, SGR, COL, CJK) | **0** |

Only scanlines y 126-129 differ, and they belong to the two runs' **different prompt
paths** (`home81b>` versus `home81base>`), which wrap at different columns. Every glyph row
of the sampler is byte-identical between the old engine and the new one. That is the
`zero behaviour diff` acceptance criterion, measured rather than eyeballed.

## 3. Ctrl-C — [`US-0081-05-ctrlc-stopped.png`](US-0081-05-ctrlc-stopped.png)

`ping -t 127.0.0.1`, then a **real** `keybd_event` Ctrl+C into the focused window
(`realinput.ps1 ctrlc`). `PING.EXE` was alive before and gone after; the pane shows the
`Ping statistics` block, `Control-C`, `^C` and a fresh prompt, and `cmd.exe` plus the
bundled `OpenConsole.exe` are still alive, so the interrupt reached only the child.

A *posted* Ctrl+C does not work — `WM_KEYDOWN` carries no modifier state — which is the
same harness limitation the US-0071 walk recorded.

## 4. Reflow — [`US-0081-06a-wide-lines.png`](US-0081-06a-wide-lines.png),
[`US-0081-06b-reflow-narrow.png`](US-0081-06b-reflow-narrow.png),
[`US-0081-06c-reflow-wide-again.png`](US-0081-06c-reflow-wide-again.png)

Twelve ~110-column lines (`LINE-1 … LINE-12`, each ending `-ENDn`).

- Maximized (client 2560x1032): one row each.
- Narrowed to client 1084x752: **each line wrapped across two rows**, `…-END1` through
  `…-END12` in order, the command line re-wrapped over two rows and the prompt over three.
  Existing scrollback was reflowed, not truncated.
- Maximized again: all twelve **re-joined onto single rows**, content and order unchanged.

This is the `KeepViewportTop` path (Windows local shell) running natively in the engine —
`crates/terminal/src/model.rs`'s 61 lines of grid surgery are gone from the product path.

## 5. Selection — [`US-0081-07a-selection-drag.png`](US-0081-07a-selection-drag.png),
[`US-0081-07b-double-click-word.png`](US-0081-07b-double-click-word.png)

A drag from (120, 200) to (700, 200) highlights exactly one line's span, ending on the
drag's end column; a double-click selects one word. The highlight comes from the engine's
`SelectionRange` through the shim's grid-line conversion.

## 6. Sixel (IN-0028's walk) — [`US-0081-08a-sixel-snake.png`](US-0081-08a-sixel-snake.png),
[`US-0081-08b-sixel-card.png`](US-0081-08b-sixel-card.png),
[`US-0081-08c-cls-removes-image.png`](US-0081-08c-cls-removes-image.png)

- `type snake.six` (a 257 KiB libsixel DCS payload): the photograph renders, and
  `echo hi after image` prints on the row **below** the image with the prompt below that —
  the exact case IN-0028's acceptance rework was accepted on
  (`../IN-0028-sixel-graphics/evidence/US-0067-rework-prompt-below-image.png`). No overlap,
  no scroll jump.
- `type card.six`: the test card at the cursor column.
- `cls`: both images gone with their cells.

The whole path is new: the engine decodes (`US-0080`), the adapter drains
`take_graphics()` once per snapshot, and the per-cell `GraphicCell { id, col, row }` the
painter reads is derived from the placement rather than stored per cell.

## 7. What was not seen, and why

**`exit`.** The scrollback step posted Shift+PageUp; a posted key carries no modifier, so
`cmd.exe` received a bare PageUp, which recalls a history entry into the input line. The
`exit` typed next was appended to the recalled `for` command and ran it again. The
capture is kept as [`US-0081-06c-reflow-wide-again.png`](US-0081-06c-reflow-wide-again.png)
(it is a valid widen-reflow frame) but the exit step itself is unproven here. Shell exit
and child teardown are what `US-0071`'s walk proved and this packet does not touch them.

**IN-0027's font walk.** Two problems, neither in the engine:

1. `terminal.json` was rewritten with PowerShell's `ConvertTo-Json`, which unwraps a
   one-element array: `fallbacks` came out as the string `"Segoe Fluent Icons"`, the app
   logged `cannot load terminal.json: invalid type: string … expected a sequence — using
   defaults`, and config A was never applied. Whoever redoes it must write the JSON by
   hand or use `ConvertTo-Json -AsArray` for that field.
2. By the time that was diagnosed the RDP session had gone from `Active` to `Disc`
   (`quser`), the log filled with `unable to get cursor position: Access is denied`, the
   window stopped honouring maximize and posted `WM_CHAR` stopped reaching the pane. The
   walks above all ran while the session was `Active`.

`terminal.json` was restored to the value it had before the attempt (`fallbacks`:
`Symbols Nerd Font Mono`, `Symbols Nerd Font`; `ligatures`: true).

**SSH.** Not walked: `sftp-dev-server` serves SFTP only and opens no shell channel, and no
SSH host is available on this machine. The SSH backend's only change in this packet is the
shared-terminal type and its construction line; `ResizePolicy::BottomAnchor` selection is
covered by `model_tests::resize_grid_applies_the_backend_policy` and by
`ssh_session_keeps_the_default_grow_policy`, both green.
