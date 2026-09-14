# US-0089 — Acceptance GUI walk of the new VT engine (IN-0029)

Date: 2026-09-13 / 2026-09-14 (one session, interrupted by a rate limit between step 8 and step 9)
Build commit: `feat/vt-engine` @ `65139c5`
("docs(vt): close the deviation tables after US-0086 and queue the two cleanup rows for US-0087"),
built in the isolated worktree `.claude/worktrees/agent-ac609f4dd2d342d81` with
`cargo build -p oneterm-app --profile fast-dev` into that worktree's own `target/`.
Steps 4b and 9 used the same tree rebuilt with `--features terminal-diagnostics`
(the feature only adds the throttled frame-time log line).

Host / platform:

| | |
| --- | --- |
| OS | Windows 11 Enterprise 10.0.26200, `ver` reports **Microsoft Windows [Version 10.0.26200.9168]** |
| Session | RDP, `quser` state **Active**, `GetForegroundWindow()` non-NULL on every step |
| Console host | bundled `target/fast-dev/x64/OpenConsole.exe`, **FileVersion 1.24.2607.10001** — confirmed as the live child of the app in the process tree |
| Theme / font | Zed One Dark, Lilex 15 px, `font.ligatures = true` |
| Grid | maximized client 2560x1032 (day 1) and 1920x1032 (day 2, RDP resized), local `cmd.exe` |
| Scratch home | `$scratchpad\home89*`, so `~` and `known_hosts` never touched the real profile |
| Driver | `gui89.ps1` (the session's `gui81.ps1` plus `rclick`, real-input `rchord` / `rdrag2` / `rwheel`), one instance per run launched by `Start-Process`, its pid recorded, acted on and stopped by pid only |

Every PNG below was read back after capture and is named
`US-0089-acceptance-<step>.png` in this folder.

## Result summary

| # | Step | Evidence | Observed | Verdict |
| --- | --- | --- | --- | --- |
| 1 | Build + bundled console host | — | `fast-dev` build clean; `x64\OpenConsole.exe` 1.24.2607.10001 next to `oneterm.exe`, and that exact path is the app's live child in every process tree | **PASS** |
| 2 | Launch, prompt, `echo hi`, `ver` | `02a-launch.png`, `02b-echo-ver.png` | Prompt at the scratch home; `hi`; `Microsoft Windows [Version 10.0.26200.9168]` | **PASS** |
| 3 | Render walk (IN-0018 sampler) | `03-render-sampler.png`, `03b-in0018-sampler.png` | See §3 — **0 differing pixels** against the IN-0029/US-0081 reference band; the full IN-0018 sampler matches `IN-0018/evidence/04b-sgr-zoom.png` attribute for attribute | **PASS** |
| 4 | Font walk, ligatures (IN-0027) | `04-font-ligatures.png` | `=>`→⟹, `->`→→, `!=`→≠, `<=`→≤, `>=`→≥, `===`→≡, `www` ligature; `\|ab\|` stays exactly above `\|cd\|` | **PASS** |
| 4b | Font walk, fallback (IN-0027) | `04b-font-fallback-segoe.png` | With `font.fallbacks = ["Segoe Fluent Icons"]`, U+E700/E70F/E8A5/E77B render as the menu / pen / document / person icons — identical to `IN-0027/evidence/US-0064-fallback-segoe-fluent-icons.png`, same overflow into the next cell | **PASS** |
| 5 | Sixel walk (IN-0028) | `05a-sixel-snake.png`, `05b-cls-removes-image.png`, `05c-sixel-card.png` | `type snake.six` → photo intact, `echo hi after image` and the prompt on the rows **below** it (the `US-0067-rework-prompt-below-image.png` case); `cls` → image gone with its cells (`US-0067-cls-removes-image.png`); `type card.six` → four bands + white diagonal at the cursor, `before` above / `after` on the row below, third row partly covered exactly as `US-0067-sixel-at-cursor.png` | **PASS** |
| 5d | Sixel via `cat` (known defect) | `05d-sixel-cat-byte-loss.png` | Git's `cat.exe` still loses bytes inside the DCS: the image renders but with light horizontal artefact streaks across several bands. Same ConPTY 32 KiB write defect IN-0028 recorded; `type` of the same file is clean | **KNOWN DEFECT, reproduced** |
| 6a | `ping -t` + real Ctrl-C | `06a-ctrlc.png` | `PING.EXE` pid alive before, gone after a real `keybd_event` Ctrl+C; pane shows `Ping statistics`, `Control-C`, `^C`, fresh prompt; `cmd.exe` and OpenConsole survive | **PASS** |
| 6b | Reflow narrow ↔ wide | `06b1-wide-lines.png`, `06b2-reflow-narrow.png`, `06b3-reflow-wide-again.png` | 12 × ~110-col lines: one row each maximized; at client 1084x792 **each wraps to two rows**, `-END1`…`-END12` in order, command line and prompt re-wrapped, nothing truncated; maximized again they **re-join** onto single rows | **PASS** |
| 6c | `echo 日本語 🙂 x` under cp65001 | `06c-cjk-emoji.png` | Echoed from a UTF-8 `.cmd` after `chcp 65001`: each CJK glyph 2 cells, emoji 2 cells, the following `ABCDEFGH-ALIGN` and the next prompt both start at column 0 — no drift | **PASS** |
| 6d | Drag-select, Ctrl+Shift+C, Ctrl+Shift+V | `06d1-selection.png`, `06d2-paste.png` | Drag paints the selection quad over exactly the dragged span; copy-on-select and `Ctrl+Shift+C` both put `ELECTME-ROW3-ALPHA-BRAVO` on the clipboard over a sentinel; `Ctrl+Shift+V` pastes **the same string** at the prompt | **PASS** |
| 6e | Wheel scrollback over a `dir /s` flood | `06e1-flood-bottom.png`, `06e2-scrolled-back.png`, `06e3-scrolled-forward.png` | 10 real wheel notches up land mid-listing (`…\drivers\SEP`, `…\UMDF`); 14 notches down move forward again (`…\drivers\wd`). Content consistent in both directions | **PASS** |
| 6f | 10 MB `type`, window dragged during it | `06f1-10mb-midstream.png`, `06f2-10mb-after.png` | `Responding = True` throughout; the window was moved 16 times mid-stream and the capture taken while moving shows a freshly painted full frame; after the flood the prompt returns and `echo AFTER-10MB` works | **PASS** |
| 6g | `title foo` → tab title | `06g-title.png` | Tab stays `Terminal`. `title foo`, a raw `OSC 0;…BEL` and a raw `OSC 2;…BEL` all fail to change it — **but an `OSC 9;4;1;50` in the same byte stream did paint the progress bar**, so OSC generally crosses ConPTY and only 0/2 is lost. No `OSC 0`/`OSC 2` ever reaches the app (the debug log shows only `7` and `133`). The wiring above the engine is present (`VtEvent::Title` → `SessionEvent::Title` → `resolve_tab_label`) | **FAIL** (see D1) |
| 6h | `exit` | `06h-exit.png` | `cmd.exe` **and** the bundled `OpenConsole.exe` both leave the process tree; only the app's own `conhost.exe` remains; app alive. The tab and its scrollback stay on screen | **PASS** (tab retained — known limitation) |
| 7 | Agent panel via OSC 9;7 | `07a-agent-working.png`, `07b-agent-blocked.png` | Right-dock **Agents** panel: `All 1 / Work 1 / Block 0` with a `working` card `claude #0: live`; the seq-2 `blocked` event then flips it to `All 1 / Work 0 / Block 1` with a `blocked` card. Base64 payload per `docs/osc-agent-status.md` §3.1 | **PASS** |
| 8 | SSH walk | — | `target/ssh_session.json` (main checkout) holds two hosts: `192.168.13.128:22` **unreachable** on both days; `pam.vpbanks.com.vn:4422` reachable but the session record carries **no username and no credentials**, so no shell can be opened. The loopback `sftp-dev-server` answers `shell_request` with `"sftp-dev-server: shell is an echo only; use the SFTP browser."` (`crates/tools/src/bin/sftp-dev-server.rs:102`) — no usable shell channel | **NOT RUN** |
| 9 | Frame time (diagnostics build) | `09-frame-time-flood.png` | See §9 | **PASS (recorded, not gated)** |

No rendering defect was found in the new engine. The one functional failure (6g) is a
title-propagation gap that the evidence localizes **outside** the VT engine's parse path.

## 3. The render walk, measured rather than eyeballed

Two samplers were printed.

**a. The US-0081 sampler** (`us0081-sampler.txt`: BOX / BLK / BRL+PL / SGR / COL / CJK rows
with raw ESC bytes), printed with cmd's `type` after `chcp 65001`. The six glyph rows were
diffed pixel-for-pixel against the same band of
[`US-0081-02-render-sampler-cmd-type.png`](US-0081-02-render-sampler-cmd-type.png):

| Band | Differing pixels |
| --- | --- |
| 620 × 96 px covering BOX, BLK, BRL, SGR, COL, CJK | **0 of 59 520** |

`US-0081` established that that reference is itself pixel-identical to the **old** engine over
the same rows, so this build is byte-identical to the old engine on the sampler by transitivity.

**b. IN-0018's own, richer sampler** was rebuilt from `IN-0018/evidence/04b-sgr-zoom.png`
(`mksampler18.py` in the scratchpad) and printed through
`powershell … Get-Content -Encoding utf8` — `03b-in0018-sampler.png`. Every attribute in the
IN-0018 reference is present and renders the same way:

- bold / italic / dim (alpha-reduced);
- single underline, **undercurl** (`\x1b[4:3m`, visibly wavy), strikethrough;
- inverse video (fg/bg swapped over a background rect);
- **hidden** (`SGR 8`): `SECRETWORD` between `HIDDEN>>` and `<<END` occupies 10 blank cells and paints nothing;
- truecolor fg (`RED GREEN BLUE`) and truecolor bg (three swatches);
- a 36-step 256-colour background ramp (16–51), monotonic black → blue → cyan → green;
- `bright: B0…B7` vs `normal: N0…N7`, each pair visibly different;
- wide CJK: `日本語テキスト`, `中文`, `한국어` each 2 columns per glyph, `align2: 日本語|ab中文|cd한국|ef` keeps its column stops;
- emoji `😀🚀` in colour, 2 cells each, `after-emoji` back on the grid;
- combining marks `é ä ô ñ` composed into one cell each.

One improvement over the reference: IN-0018 recorded `ト` coming through as two U+FFFD
because it used cmd's `type`; through the PowerShell path it renders correctly here.
That is a console decode artefact in the old capture, not a renderer difference.

## 9. Frame time

Method: the app's own `crates/terminal-view/src/render/diagnostics.rs` line, emitted at most
every 5 s from `prepaint`/`paint`, plus `p95`/`p99` over a 512-frame ring of
`prepaint + paint`. Because the line is only emitted while a frame is being painted, an idle
terminal produces very few samples. Run: idle at a clear prompt for 45 s, then
`flood89.cmd` (8 × `type big10mb.txt`, ~80 MB) for ~46 s of continuous output.
Grid 1920x1032, scrollback at the shipped default.

| Phase | prepaint p50 | prepaint max | paint p50 | paint max | frame total (prepaint+paint) p50 | frame total max | ring p95 / p99 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Idle prompt** | **21 µs** | **70 µs** | 113 µs | 222 µs | **134 µs** | **292 µs** | — (ring still holds flood samples) |
| **10 MB `type` flood** | **2.71 ms** | **5.04 ms** | 1.15 ms | 1.65 ms | **3.87 ms** | **6.68 ms** | **6.8–8.0 ms / 7.4–9.8 ms** |

- The very first frame after launch cost 1.27 ms prepaint (shaper warm-up) and is excluded from the idle row.
- A separate earlier run at 2560x1032 saw a single 10.06 ms prepaint frame at the instant a flood started (the full-viewport rebuild on the first flooded frame); steady state there matched the table above.
- Every flood frame stays inside a 16.7 ms budget; the flood sustained ~1.7 MB/s of `type` output.
- Recorded as a finding. `IN-0029.md`'s acceptance explicitly contains no performance number, so nothing here gates the packet.

## Defects and deviations

**D1 — `title` / `OSC 0` / `OSC 2` never reach the terminal, so the tab title cannot change (step 6g). FAIL.**
Reproduced three ways in one session: cmd's `title foo`, a raw `ESC ] 0 ; … BEL` and a raw
`ESC ] 2 ; … BEL`, all through a local `cmd.exe` on the bundled OpenConsole 1.24. The tab
label stayed `Terminal` every time. The same payload's `ESC ] 9 ; 4 ; 1 ; 50 BEL` **did**
paint the progress bar, and the app's `OSC recv:` debug log over the whole session lists only
sub-codes `7` and `133` — never `0` or `2`. The consumer chain above the engine is intact and
unit-tested (`VtEvent::Title` → `OscRouter::set_title` → `SessionEvent::Title` →
`TerminalViewEvent::TitleChanged` → `resolve_tab_label`). The evidence therefore points at the
ConPTY host consuming OSC 0/2 rather than at the VT engine, but that was not proven from
inside the host and **no previous walk covered `title`**, so this is a new, open observation
rather than a known limitation. Needs its own BUG packet: either drive the tab title from
ConPTY's title channel, or confirm and document that a local Windows shell cannot set it.

**D2 — `exit` leaves the tab open (step 6h). Known limitation, unchanged.**
`cmd.exe` and the bundled `OpenConsole.exe` are both reaped, which is the part that matters;
the tab and its scrollback deliberately stay.

**D3 — ConPTY drops bytes inside a >32 KiB DCS write (step 5d). Known defect, unchanged.**
`cat.exe snake.six` renders the image with horizontal artefact streaks where bytes were lost;
`type` of the same file is clean. Same behaviour IN-0028 recorded in
`US-0067-rework-conpty-cat-byte-loss.png`. Not an engine defect — the bytes never arrive.

**D4 — harness limitation, not a product defect.** A drag started in the pane's left padding
(client `x < ~8`) begins no selection. Starting the drag inside the glyph area works with both
posted and real mouse input. Cost three attempts before it was diagnosed; worth remembering
for the next walk.

## Not walked

- **SSH** (step 8) — no host with credentials. See the table.
- **A real Nerd Font** — none installed on this machine; IN-0027's `Segoe Fluent Icons` stand-in was used again, which is what the reference PNG shows.
- **Split panes, search bar, gutter timestamps, bell, URL Ctrl+click, IME** — outside this walk's brief; covered by the IN-0018 walk.

## Housekeeping

`target/terminal.json` in the worktree was backed up before the step-4b fallback edit and
restored afterwards. Only the pid this walk launched was ever signalled; the owner's own
`oneterm.exe` was never enumerated by name or title and its pid was unchanged at the end of
every run.
