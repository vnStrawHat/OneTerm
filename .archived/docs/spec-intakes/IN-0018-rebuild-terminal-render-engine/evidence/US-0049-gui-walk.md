# US-0049 — Manual GUI parity walk (new terminal render engine)

Date: 2026-09-09
Branch: `refactor/terminal-render-engine` @ `302fa47`
Binary: `target/fast-dev/oneterm.exe` (built 07:17:32, not rebuilt during this walk)
Platform: Windows 11 Enterprise 10.0.26200, Zed One Dark theme, Lilex 15 px, scale 1.0,
grid ≈ 122 × 45 (cell ≈ 9.0 × 18.4 device px), local `cmd.exe` shell.

## Result summary

| # | Item | Result |
| --- | --- | --- |
| 1 | Box drawing | PASS |
| 2 | Blocks and shades | PASS |
| 3 | Braille and powerline | PASS |
| 4 | SGR attributes / wide chars / emoji / combining | PASS |
| 5 | Cursor (block focused, hollow unfocused) | PASS |
| 6 | Selection, double-click word, copy/paste | PASS |
| 7 | Search bar | **PARTIAL — focus is not returned to the terminal when the bar closes** |
| 8 | Scrollback, scrollbar, fade | PASS |
| 9 | URL underline | PASS (hover pointer / Ctrl+click not exercised) |
| 10 | Gutter timestamps | PARTIAL (`[HH:MM:SS] N` verified; `[--:--:--]` fallback not observable) |
| 11 | Completion overlay | PASS |
| 12 | Bell badge | PASS |
| 13 | OSC 9;4 progress bar | PASS |
| 14 | Split / placeholder / New Terminal Here / Close Space | PASS |
| 15 | Font zoom in / reset | PASS |
| 16 | Performance sanity | PASS |

No rendering defect was found in the new engine. The single behavioural defect is a focus
handoff issue in the search bar (item 7).

## Environment note (how the walk was driven)

The Windows workstation was **locked** for the whole session (`OpenInputDesktop` → `ERROR_ACCESS_DENIED`,
`GetForegroundWindow()` → `NULL`, two `LogonUI.exe` processes). Consequences:

- `SendKeys` / `SendInput` / real mouse input could not be delivered (there is no foreground window).
  All input was therefore **posted directly to the window** with `PostMessage`:
  `WM_CHAR` for text, `WM_KEYDOWN`/`WM_KEYUP` for named keys, and
  `AttachThreadInput` + `SetKeyboardState` around the posted key for Ctrl/Shift chords
  (verified working: Ctrl+F, Ctrl+Shift+C/V, Ctrl+=, Ctrl+0, Shift+PageUp, Shift+End, Ctrl+C).
  Mouse was `WM_MOUSEMOVE` / `WM_?BUTTONDOWN` / `WM_?BUTTONUP` in client coordinates;
  the context menu additionally needed a `WM_CONTEXTMENU` after the right-button pair.
- Screenshots are `PrintWindow(hwnd, hdc, PW_RENDERFULLCONTENT)`; the window rect and the client
  rect differ by (8, 0), so image *x* = client *x* + 8.
- `target/terminal.json` is read **only at process start** — it does not hot-reload. `show_gutter`
  therefore required an app restart (item 10). The file was backed up, modified
  (`layout.show_gutter = true`, `cursor.blink = false` for that run) and restored byte-identical
  at the end (`git status` clean for `target/terminal.json`; the `.bak` was removed).
- `cmd.exe`'s `type` with `chcp 65001` mangles some multi-byte characters (`ト` came out as two
  U+FFFD). This is a console decode artifact, **not** a renderer bug — the same line printed via
  `powershell … Get-Content -Encoding utf8` renders correctly (`04c-cjk-zoom.png`).

## Findings

### FAIL / PARTIAL

**F1 — Search bar does not return keyboard focus to the terminal when it closes (item 7).**
Reproduced twice, on two separate app instances, for both close paths:

1. click the terminal (cursor is a solid block → focused),
2. `Ctrl+F` (bar opens, terminal cursor becomes hollow → unfocused),
3. close the bar with `Esc` **or** with a second `Ctrl+F`,
4. type — nothing reaches the shell, and the terminal cursor stays hollow.

The search bar is gone from the screen at this point, so there is no visible focus owner; the
keyboard is dead until the user clicks into the grid. Evidence: `07e-focus-after-esc.png`
(typed `AFTER-ESC`, nothing appears, hollow cursor at the prompt) and `07f-focus-after-toggle.png`
/ `07i-focus-after-toggle-zoom.png` (typed `TOGGLE-CLOSE`, same). Everything else in the search
flow is correct (see item 7 below). Suggested area: `terminal_view/search.rs` close path should
`focus` the view's `FocusHandle` (parity item 2.13 "clear on close" / 2.4.1 toggle).

**P2 — `[--:--:--]` gutter fallback not observed (item 10).**
With `show_gutter = true` every visible row carries a real stamp: rows below the last output reuse
the newest stamp, which matches §2.12 "newest/oldest reuse". The `[--:--:--]` form can only appear
before the first `Output` event, a state the shell leaves within milliseconds of the window
appearing, so it could not be captured. The `[HH:MM:SS] N` two-colour column itself is correct.

**P3 — Not exercised (out of scope / not feasible here):** URL hover pointer shape and Ctrl+click
open (deliberately not clicked); IME composition (no IME on a locked desktop); SSH-closed banner
(no SSH session); drag of a tab into a Space; scrollbar thumb drag.

### Notable positives worth recording

- Deviation 1 (all 16 powerline code points have real geometry) is visible and correct:
  filled/outlined triangles, chevrons, filled/outlined half discs, quadrant triangles and
  diagonals — `03b-braille-powerline-zoom.png`.
- Deviation 3 (`╒/╘`, `╓/╙`, `╤/╧`) — up/down variants are exact mirrors — `01c-box-updown-zoom.png`.
- Deviation 4 (SGR 8 hidden) — `SECRETWORD` between `HIDDEN>>` and `<<END` occupies 10 blank cells
  and paints no glyph — `04b-sgr-zoom.png`.
- Braille is covered as required by DEC-0007: individual dot quads, `⣿` full 2×4 grid, spinner and
  ramp sequences all distinct.
- Hollow cursor (parity item 18) confirmed as a 1-device-px outline in the terminal's cursor colour
  (`#528BFF`) whenever the view is unfocused — `05b-cursor-hollow-unfocused.png`,
  and in the non-active pane of a split (`14d-new-terminal-here.png`).

## Item-by-item

### 1. Box drawing — PASS
`01-box-drawing.png`, `01b-box-boxes-zoom.png`, `01c-box-updown-zoom.png`

Light / heavy / double / mixed / rounded / dashed rows all render as geometry. In the three drawn
boxes (light, double, rounded) the corners meet the rails with no gap and no overshoot, the
horizontal rails of adjacent cells form one continuous stroke (no seam at the cell boundary), and
`├`/`┤` tees land on the same centre line as `┌`/`└`. Heavy strokes are ~3 device px against ~1 px
light, consistently on both axes. Double lines are two real rails, not a thick single rail. Rounded
corners are smooth arcs that meet the straight rails tangentially. `╒` vs `╘`, `╓` vs `╙` and
`╤` vs `╧` are exact vertical mirrors of each other. Diagonals `╱╲╳` render as anti-aliased paths.

### 2. Blocks and shades — PASS
`02-blocks-shades.png`, `02b-blocks-zoom.png`

A 40-cell run of `█` paints as one uninterrupted band (coalesced run, no seams). `▀▄▀▄…` alternates
upper/lower halves with the boundary exactly at the cell mid-line and no gap between cells.
`▁▂▃▄▅▆▇█` is a clean monotonic staircase, `▏▎▍▌▋▊▉█` likewise horizontally; `▌`/`▐` fill exactly
the left/right half of their own cell. Quadrant blocks correct. `░▒▓` render as three visibly
different dither densities (sparse dots / checkerboard / near-solid) — deviation 2 (scaled
pattern) works. `▬` is a single half-height centred bar.

### 3. Braille and powerline — PASS
`03-braille-powerline.png`, `03b-braille-powerline-zoom.png`

All 16 of U+E0B0–E0BF have distinct real geometry. With `\x1b[44m\x1b[31m…` the triangle fill is
red on a blue cell background and the separator shape is clearly visible. A two-segment powerline
prompt (`green seg1 → blue → dark`) joins with no seam or colour bleed between the segment cell and
its separator.

### 4. SGR attributes — PASS
`04-sgr-attributes.png`, `04b-sgr-zoom.png`, `04c-cjk-zoom.png`

Bold, italic, dim (alpha-reduced), single underline, undercurl (`\x1b[4:3m`, wavy), strikethrough,
inverse (fg/bg swapped with a background rect), hidden (nothing painted), truecolor fg and bg,
a 36-step 256-colour background ramp, and bright vs normal ANSI pairs that differ correctly.
CJK: each of `日本語テキスト`, `中文`, `한국어` occupies exactly two columns and the following ASCII
stays on the grid (verified against a digit ruler on the line above). Emoji `😀🚀` paint in colour
and consume two cells each. Combining marks (e-acute, a-umlaut written as base + combining code point) compose into one cell.

### 5. Cursor — PASS
`05a-cursor-focused.png`, `05a-cursor-block-zoom.png`, `05b-cursor-hollow-unfocused.png`

Focused: filled block in the theme cursor colour `#528BFF` covering exactly one cell.
Blink is honoured (sampled captures alternate on/off at ~500 ms with `cursor.blink = true`).
Unfocused (search bar or the other split pane holding focus): the cursor is always painted and is
a hollow 1-device-px outline — parity items 17 and 18.

### 6. Selection and clipboard — PASS
`06a-selection-drag.png`, `06b-selection-done.png`, `06c-double-click-word.png`,
`06d-word-selection-zoom.png`, `06e-paste-zoom.png`

Drag from column 0 to column 9 paints the selection quad over exactly those cells, under the text
and over the row background; `Get-Clipboard` returned `"SELECTME "` (copy-on-select).
Double-click on `gamma` selected exactly that word; clipboard `"gamma"`.
`Ctrl+Shift+C` re-copied the active selection over a sentinel value. `Ctrl+Shift+V` pasted
`echo PASTED-OK` into the shell line.

### 7. Search — PARTIAL (see F1)
`07a-search-open.png`, `07b-search-matches.png`, `07c-search-step.png`, `07d-search-closed.png`,
`07g-search-highlight-zoom.png`, `07h-search-counter-zoom.png`, `07e-focus-after-esc.png`,
`07f-focus-after-toggle.png`, `07i-focus-after-toggle-zoom.png`

Working: `Ctrl+F` opens the bar (`Aa`, `W`, input, `n/total`, ↑ ↓ ×) at the top-right of the pane;
typing `a` (after the 150 ms debounce) shows `1/10` and highlights every match; the active match
uses a brighter gold than the other matches; highlight quads cover the full cell and are painted
under the selection quad (a selected match's highlight is correctly hidden by the selection);
`Enter` steps 1/10 → 3/10 and moves the active highlight; `Esc` closes the bar and clears all
highlights while leaving the selection intact.
Broken: focus is not handed back to the terminal on close (F1).

### 8. Scrollback and scrollbar — PASS
`08a-scroll-bottom.png`, `08b-scroll-pageup.png`, `08c-scrollbar-faded.png`, `08d-scroll-end.png`

200 numbered lines printed. `Shift+PageUp` scrolled to lines 113–157 and the scrollbar thumb
appeared at the right edge of the pane (grey `#6D6E70`, ~8 px wide, proportional length and
position). After ~4 s idle the thumb had faded out completely (pixel back to the pane background).
`Shift+End` returned to the bottom (line 200 + prompt).

### 9. URL — PASS
`09-url-underline.png`, `09b-url-zoom.png`

`https://example.com/path`, `www.rust-lang.org` and `http://localhost:8080/a?b=c` are all detected
and underlined without hover. Trailing-punctuation stripping is correct: in `(https://example.com/x).`
the underline covers `https://example.com/x` and stops before `)` and `.`.
Hover pointer and Ctrl+click were not exercised (instructed to skip the click; the pointer shape is
not capturable through `PrintWindow`).

### 10. Gutter timestamps — PARTIAL (see P2)
`10-gutter-timestamps.png`, `10b-gutter-zoom.png`

With `layout.show_gutter = true` the gutter renders `[HH:MM:SS]` in the muted gutter colour and the
line number in teal, right-aligned, with the grid shifted right by the gutter width and re-measured
correctly (no clipping of column 0). Rows past the last output reuse the newest stamp as designed.

### 11. Completion overlay — PASS
`11a-completion-overlay.png`, `11b-completion-dismissed.png`, `11c-completion-zoom.png`

Typing `d` at the `cmd` prompt opens the overlay anchored under the cursor cell with
`cd`, a history entry, `del`, `dir`, `date`, `rd`, `find`; the matched characters are highlighted
inside each candidate. No item is pre-selected, so `Enter` is forwarded to the shell (confirmed
throughout the walk). `Esc` dismisses the overlay and the typed `d` stays on the command line
(i.e. `Esc` was consumed by the overlay, not forwarded to `cmd`, which would have cleared the line).

### 12. Bell — PASS
`12-bell-badge.png`, `12b-bell-zoom.png`

A raw `BEL` byte shows the 🔔 badge in the top-right corner of the terminal pane.

### 13. OSC 9;4 progress — PASS
`13a-progress-on.png`, `13c-progress-zoom.png`, `13b-progress-cleared.png`

`\x1b]9;4;1;50\x07` paints a blue bar along the top edge of the pane at ~50 % of the pane width.
`\x1b]9;4;0\x07` removes it.

### 14. Split — PASS
`14a-context-menu.png`, `14f-terminal-menu-zoom.png`, `14b-split-right-placeholder.png`,
`14c-placeholder-menu.png`, `14g-placeholder-menu-zoom.png`, `14d-new-terminal-here.png`,
`14h-split-menu-zoom.png`, `14e-split-closed.png`

Right-click gives the full menu: New Terminal, Duplicate Session ▸, Split Right/Left/Up/Down,
Copy (disabled with no selection), Paste, Select All, Clear, Log ▸, Close Terminal Tab — and
**no** Close Space while `leaf_count == 1` (guard correct). Split Right creates the placeholder
Space with its icon, `Space #1` label and "Drag a terminal tab here / or right-click to split" hint;
the left pane re-measures and re-wraps immediately. The placeholder's own menu offers
New Terminal Here + the four splits + Close Terminal Tab + Close Space. New Terminal Here spawns a
`cmd` shell that renders in the right pane while the left pane keeps rendering (with a hollow
cursor). Close Space collapses back to a single pane and the text re-wraps to full width.

### 15. Font zoom — PASS
`15a-zoom-before.png`, `15b-zoom-in.png`, `15d-zoom-in-boxes.png`, `15c-zoom-reset.png`,
`15e-zoom-reset-boxes.png`

Two `Ctrl+=` steps enlarge the font and re-measure the grid; `Ctrl+0` restores the base size.
Box drawing was re-captured at both sizes: corners still meet, rails still join across cells with
no seam or double coverage, rounded arcs stay smooth, and double lines stay two rails.

### 16. Performance sanity — PASS
`16b-after-ctrlc.png`, `16c-bigtype-midstream.png`, `16d-bigtype-done.png`

- `dir /s C:\Windows\System32` (19 902 files, 4 025 dirs) ran to completion while the UI stayed
  responsive; `Process.Responding` stayed `True` throughout and each `PrintWindow` readback (which
  forces a synchronous redraw) returned in 257–658 ms including bitmap allocation and PNG encoding.
- `type big.txt` (30 000 lines / 2.19 MB): the whole file was drained and rendered in well under a
  second (the third readback, ~1.0 s after `Enter`, already shows line 29 999 and the prompt);
  total process CPU for the burst was ~0.9 s. No dropped rows, no torn frames, semantic colouring
  applied on every line.
- DOOM-fire was not run (different build required, per instruction).

## Screenshot list

All under `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/evidence/`.

| File | Shows |
| --- | --- |
| `00-baseline.png` | fresh window, prompt, docks |
| `01-box-drawing.png` / `01b-box-boxes-zoom.png` / `01c-box-updown-zoom.png` | item 1 |
| `02-blocks-shades.png` / `02b-blocks-zoom.png` | item 2 |
| `03-braille-powerline.png` / `03b-braille-powerline-zoom.png` | item 3 |
| `04-sgr-attributes.png` / `04b-sgr-zoom.png` / `04c-cjk-zoom.png` | item 4 |
| `05a-cursor-focused.png` / `05a-cursor-block-zoom.png` / `05b-cursor-hollow-unfocused.png` | item 5 |
| `06a-selection-drag.png` / `06b-selection-done.png` / `06c-double-click-word.png` / `06d-word-selection-zoom.png` / `06e-paste-zoom.png` | item 6 |
| `07a-search-open.png` … `07d-search-closed.png`, `07g`, `07h` | item 7 (working parts) |
| `07e-focus-after-esc.png` / `07f-focus-after-toggle.png` / `07i-focus-after-toggle-zoom.png` | **F1 defect** |
| `08a-scroll-bottom.png` … `08d-scroll-end.png` | item 8 |
| `09-url-underline.png` / `09b-url-zoom.png` | item 9 |
| `10-gutter-timestamps.png` / `10b-gutter-zoom.png` | item 10 |
| `11a-completion-overlay.png` / `11b-completion-dismissed.png` / `11c-completion-zoom.png` | item 11 |
| `12-bell-badge.png` / `12b-bell-zoom.png` | item 12 |
| `13a-progress-on.png` / `13b-progress-cleared.png` / `13c-progress-zoom.png` | item 13 |
| `14a`–`14e`, `14f`–`14h` zooms | item 14 |
| `15a-zoom-before.png` / `15b-zoom-in.png` / `15c-zoom-reset.png` / `15d` / `15e` | item 15 |
| `16b-after-ctrlc.png` / `16c-bigtype-midstream.png` / `16d-bigtype-done.png` | item 16 |

## Cleanup performed

- `target/terminal.json` restored from the backup taken at the start; the backup file was deleted.
  `git status` reports no change to it (it is untracked build output either way; content verified
  identical: `cursor.blink = true`, `cursor.shape = "block"`, `layout.show_gutter = false`).
- All OneTerm processes started by this walk were stopped. No source file was edited, no cargo
  command was run, nothing was committed.
