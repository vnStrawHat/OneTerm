# OSC (Operating System Command) Sequences — OneTerm Support Checklist

> Reference document for OSC escape sequences (both **common** and **vendor-specific**),
> with a checklist by group and, for each group, what **OneTerm** does and where the
> behaviour lives in the codebase.

---

## ⚠️ Methodology & confidence level

Every ✅ / ◐ / ❌ below is read from the OneTerm codebase, not from memory:
`crates/terminal/src/osc.rs`, `crates/terminal/src/osc_color.rs`,
`crates/terminal/src/backend/osc_router.rs`, `crates/core/src/config/shell.rs`.
When the code changes, update this document from the code.

---

## 0. OSC basics

### 0.1 General syntax

```
ESC ] Ps ; Pt ST
```

- `ESC ]` = `\x1b]` — OSC opener.
- `Ps` — command number (can have multiple parameters separated by `;`).
- `Pt` — payload (text/color spec/URI/...).
- `ST` (String Terminator) — ends the OSC, one of two forms:
  - `BEL` = `\x07` (most common, xterm de-facto).
  - `ESC \` = `\x1b\\` (ECMA-48 standard).

> When writing an emitter, prefer `BEL` for maximum compatibility. OSC 8 per spec should use `ESC \`.

### 0.2 Query mode

Many OSCs support **query**: send `Pt = ?` to request the terminal report its current value.
Example: `ESC ] 10 ; ? BEL` → asks for the default foreground color.
**Note**: an emitter must never assume a query is answered — not every terminal replies.

### 0.3 Color spec format

- `rgb:RRRR/GGGG/BBBB` — 16-bit/channel (full, recommended).
- `rgb:RR/GG/BB` — 8-bit/channel.
- `#RRGGBB` — hex.
- `?` — query current value.

---

## Group A — Window / Icon / Title

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **0** | Set **both** icon name + window title | `ESC]0;title ST` | ✅ — drives the tab title. |
| ☐ | **1** | Set **icon name** (title unchanged) | `ESC]1;name ST` | ❌ — X11 legacy, no icon-name concept. |
| ☑ | **2** | Set **window title** | `ESC]2;title ST` | ✅ — equivalent to OSC 0. |

---

## Group B — Color Palette (indexed colors 0–255)

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **4** | Set/query 1+ palette colors | `ESC]4;idx:spec ST` | ✅ set + query (`idx;?`). |
| ☐ | **5** | Set/query "special" colors | `ESC]5;idx:spec ST` | ❌ |
| ☑ | **104** | Reset 1+ palette colors | `ESC]104;idx ST` or `ESC]104 ST` (all) | ✅ single + all. |
| ☐ | **105** | Reset special colors | `ESC]105;idx ST` | ❌ |

> ✅ **OneTerm**: OSC 4 **set + query (`idx;?`)** and OSC 104 **reset** (single/all).
> Shares infrastructure with OSC 10/11/12 via `ColorRequest`: set → `Term.colors[0..256]`, query → reply after
> parse batch (fallback to default palette via `default_color_for_index` + `set_default_colors`), rendered through
> `dynamic_colors().indexed` + `TerminalPalette.indexed`. OSC 5/105 (special colors) still ❌.

---

## Group C — Default & Special Colors (fg/bg/cursor/selection)

| Check | OSC | Purpose | Query | Reset OSC | OneTerm |
|:-----:|-----|----------|:-----:|:---------:|---------|
| ☑ | **10** | Default foreground | `10;?` | **110** | ✅ |
| ☑ | **11** | Default background | `11;?` | **111** | ✅ |
| ☑ | **12** | Text cursor color | `12;?` | **112** | ✅ |
| ☐ | **13** | Mouse pointer fg color | `13;?` | **113** | ❌ |
| ☐ | **14** | Mouse pointer bg color | `14;?` | **114** | ❌ |
| ☐ | **17** | Selection (highlight) bg | `17;?` | **117** | ❌ |
| ☐ | **19** | Selection (highlight) fg | `19;?` | **119** | ❌ |
| ☑ | **110–112** | Reset fg/bg/cursor | — | — | ✅ |
| ☐ | **117/119** | Reset selection bg/fg | — | — | ❌ |
| ☐ | **39** | Default fg (xterm alias for OSC 10) | — | — | ❌ |

> ✅ **OneTerm**: OSC 10/11/12 **set + query (`?`)** and OSC 110/111/112 **reset**.
> `Event::ColorRequest` is enqueued in `LocalListener`/`SshListener` then answered after each parse batch
> (reads `Term.colors()`, falls back to theme default via `set_default_colors`); set/reset rendered via `dynamic_colors()`.

---

## Group D — Clipboard

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **52** | Set/query clipboard (base64) | `ESC]52;c;base64 ST` | ✅ write, ◐ read (opt-in). |

### D.1 Security notes

OSC 52 read hands the local clipboard to whatever program is running in the terminal — including a
remote program over SSH — so OneTerm keeps write and read on separate switches.

- **write**: ✅ always on. `OscSink` parses the base64 payload; the set path goes through the
  `alacritty_terminal` `ClipboardStore` → `SessionEvent::Clipboard`.
- **read** (`52;c;?`): ◐ default **off**. `SessionEvent::ClipboardRead` → the UI replies
  `52;c;<base64>` (`encode_osc52`) **only when** `security.allow_clipboard_read = true`
  (default `false`).

---

## Group E — Hyperlinks (OSC 8)

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **8** | Open/close hyperlink | `ESC]8;params;URL ST text ESC]8;; ST` | ✅ — `id=ID` param groups link cells. |

```
ESC ] 8 ; params ; URL ST   ← open link
  <displayed text>
ESC ] 8 ; ; ST               ← close link
```

- **OneTerm**: ✅ — the vendored `alacritty_terminal` VT engine stores the hyperlink on the cell; the view reads
  `cell.hyperlink()` and every target passes the external-target policy
  (`crates/terminal/src/url_policy.rs`, display-text vs target check via `validate_target_with_display`).

---

## Group F — Current Working Directory (CWD)

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **7** | Set CWD (file:// URI) | `ESC]7;file://host/path ST` | ✅ — de-facto standard. |
| ☐ | **9;9** | Set CWD (Windows path) | `ESC]9;9;C:\path ST` | ❌ |

- **OneTerm**: ✅ OSC 7 — `OscSink` parses `file://` → `parse_cwd_url`. **No** 9;9.
- `alacritty_terminal` does not handle OSC 7, which is why OneTerm parses it itself via `OscSink`
  in parallel with the VT engine.

---

## Group G — Notifications & Progress

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **9** | Desktop notification | `ESC]9;msg ST` | ✅ → toast. |
| ☑ | **9;4** | Progress bar | `ESC]9;4;state;pct ST` | ✅ — state 0–4. |
| ☑ | **9;7** | Agent status | see [`osc-agent-status.md`](osc-agent-status.md) | ✅ — OneTerm proposal. |
| ☐ | **9;1/2/3** | ConEmu misc (sleep/msgbox/tabtitle) | `ESC]9;1;ms ST` etc. | ❌ |
| ☐ | **99** | Extended notification protocol | `ESC]99;i=ID;payload ST` | ❌ |
| ☐ | **777** | urxvt notification | `ESC]777;notify;title;body ST` | ❌ |

> ✅ **OneTerm**: OSC 9 (notification → toast via `window.push_notification`) and OSC 9;4
> (progress → thin progress bar at the top edge of the terminal, state 0-4). Forwarded by the vendored
> `alacritty_terminal` fork's `Event::Osc` hook (upstream drops OSC 9) to `OscRouter` →
> `OscPayload::Notification`/`Progress` → `SessionEvent`. OSC 9;7 (agent status) is also supported, see
> [`osc-agent-status.md`](osc-agent-status.md). Still ❌: 9;1/2/3, 99, 777.

---

## Group H — Shell Integration / Prompt Markers

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **133** | Shell integration prompt markers (FinalTerm semantics) | `133;A`/`B`/`C`/`D;exit` | ✅ A/B/C/D + exit code. |
| ☐ | **133;P** | Prompt properties (extension) | `133;P;k=i ST` | ❌ |
| ☐ | **633** | VS Code shell integration | `633;A`..`D;exit`/`E`/`P` | ❌ |
| ☐ | **633;SetMark** | VS Code mark | `633;SetMark ST` | ❌ |

### Standard OSC 133 — 4 markers:

```
ESC]133;A ST      ← Prompt start
ESC]133;B ST      ← Command start
ESC]133;C ST      ← Command output start
ESC]133;D;exit ST ← Block end (exit code optional)
```

- **OneTerm**: ✅ OSC 133 A/B/C/D (code: `Osc133Kind` enum + exit code). **No** 133;P/633.
- `alacritty_terminal` does not handle OSC 133, so OneTerm parses it itself via `OscSink`.

---

## Group I — Font

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☐ | **50** | Set/query font | `ESC]50;font-spec ST` | ❌ — xterm origin; font is a settings concern. |

---

## Group J — Vendor-specific & Misc

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☐ | **1337** | Inline image + subcodes | `ESC]1337;File=...;inline=1:base64 ST` | ❌ |
| ☐ | **20** | Background opacity | `ESC]20;alpha ST` | ❌ |
| ☐ | **21** | Extended color protocol | `ESC]21;... ST` | ❌ |
| ☐ | **22** | Mouse pointer shape | `ESC]22;name ST` | ❌ |
| ☐ | **46** | Log file (xterm) | `ESC]46;path ST` | ❌ |
| ☐ | **66** | Text sizing | `ESC]66;... ST` | ❌ |
| ☐ | **3008** | systemd context signal (UAPI) | `ESC]3008;... ST` | ❌ |

- **OneTerm**: ❌ all vendor-specific OSCs. Inline images: **Sixel** (`DCS P1;P2;P3 q ... ST`, not an
  OSC) is implemented (IN-0028: decoded in the vendored `Term`, anchored to cells, DA1 answers
  `CSI ? 62 ; 4 c`); OSC 1337 and the Kitty APC protocol are not.

---

## Quick summary table — top commonly-used OSCs

| Check | OSC | Name | Popularity |
|:-----:|:---:|-----|:-----------:|
| ☑ | 0/2 | Window title | ⭐⭐⭐⭐⭐ |
| ☑ | 7 | CWD (file://) | ⭐⭐⭐⭐⭐ |
| ☑ | 8 | Hyperlinks | ⭐⭐⭐⭐ |
| ☑ | 4 | Color palette set/query | ⭐⭐⭐⭐ |
| ☑ | 10/11/12 | Default FG/BG/cursor | ⭐⭐⭐⭐ |
| ☑ | 52 | Clipboard | ⭐⭐⭐ |
| ☑ | 133 | Shell integration markers | ⭐⭐⭐⭐ |
| ☑ | 9 | Desktop notification | ⭐⭐⭐ |
| ☑ | 104/110-112 | Reset colors | ⭐⭐⭐ |
| ☐ | 633 | VS Code shell integration | ⭐⭐⭐ |

### OneTerm — current status summary

| OSC group | OneTerm | Assessment |
|----------|:-------:|----------|
| 0/2 title | ✅ | OK |
| 7 CWD | ✅ | OK (self-parsed; the VT engine does not handle it) |
| 8 hyperlink | ✅ | OK (via the VT engine's cell hyperlink) |
| 52 clipboard | ✅ | OK (self-parse + `EventListener`) |
| 133 shell integration | ✅ | OK (A/B/C/D + exit code) |
| 10/11/12 + 110–112 colors | ✅ | OK (set + query + reset fg/bg/cursor) |
| 4 + 104 palette colors | ✅ | OK (set + query + reset index 0–255) |
| 5/13–19/105/117–119 colors | ❌ | **Gap** — special/pointer/selection not mapped |
| 9 + 9;4 notification/progress | ✅ | OK (toast + progress bar) |
| 99/777 notifications | ❌ | **Gap** |
| 633 shell integration | ❌ | **Gap** (133 only) |
| 1337 image | ❌ | **Gap** (Sixel via DCS is supported instead) |

> OneTerm currently **covers** the 5 core groups (title/CWD/hyperlink/clipboard/shell-integration) **+ default colors
> (OSC 10/11/12/110-112) + color palette (OSC 4/104) + notification/progress (OSC 9, 9;4) + Sixel images (DCS)**, but **lacks**
> special colors (5), pointer/selection (13–19), notification 99/777, 633, OSC 1337 / Kitty images.

---

## Legend

| Symbol | Meaning |
|:-------:|---------|
| ✅ | Fully supported. |
| ◐ | Partially supported: only a subset of parameters, write/read only, requires config, or reset only without set. |
| ❌ | Not supported. |
| ⭐ | Popularity (1–5, subjective assessment). |

---

## Practical experience

1. **ST terminator**: Use `BEL` (`\x07`) for maximum compatibility. OSC 8 per spec should use `ESC \`.
2. **Query response**: an emitter must not block on a query reply — support is uneven across terminals.
3. **OSC 52 clipboard**: always expect rejection. **write** is common, **read** is rare and usually opt-in.
4. **OSC 7 CWD**: must be a full `file://` URI (including host). `alacritty_terminal` does not handle OSC 7,
   so OneTerm parses it itself in parallel with the VT engine.
5. **Shell integration**: 133 (FinalTerm semantics) is the common standard; 633 is a VS Code extension of it.
   Wrap the 4 markers A/B/C/D correctly.
6. **Color spec**: prefer `rgb:RR/GG/BB` or `rgb:RRRR/GGGG/BBBB`. Avoid `#hex` if you need old-xterm compatibility.
7. **Vendor-specific**: only use when you are sure of the target terminal. Detect via `TERM`, `TERM_PROGRAM`,
   and terminal-specific environment variables.
8. **Do not nest OSC**: close one OSC before opening another.
9. **OneTerm** (VT engine = `alacritty_terminal`): supports **OSC 0/2, 7, 8, 52 (base64+query), 133 (A/B/C/D+exit),
   4 (set+query) + 104 (reset), 10/11/12 (set+query) + 110/111/112 (reset), 9 (notification), 9;4 (progress),
   9;7 (agent status)**.
   - 133/9/9;4 are parsed in parallel via `OscSink` (the VT engine drops OSC 7/9/133); `OscSink` uses a FIFO
     queue so multiple OSCs in the same read batch are all kept + processed in order.
   - OSC 8 stored in cell; OSC 52 goes through `EventListener` + `OscSink`.
   - OSC 4/104 + 10/11/12/110-112: the VT engine already parses these (set → `Term.colors`, reset → clear);
     OneTerm renders via `dynamic_colors()` (`TerminalPalette.indexed` for index 0-255) and answers queries via
     `Event::ColorRequest` (enqueue → reply after parse batch, fallback default palette via `set_default_colors`
     + `default_color_for_index`).
   - OSC 9 → `SessionEvent::Notification` → toast `window.push_notification`; OSC 9;4 →
     `SessionEvent::Progress(TerminalProgress)` → thin progress bar at the top edge of the terminal view.
   - **No** special colors (5/105), pointer/selection (13–19/113–119): not mapped yet.
   - **No** notification 99 / 777 / 9;1-3, font (50), 633, 1337.
   - Self-generates OSC 7 + 133 A via `PROMPT_COMMAND` (bash) / `PS1` (zsh) / `PROMPT` (cmd).

---

## References

### Specifications
- xterm ctlseqs — <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html>
- FinalTerm OSC 133 spec — <https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md>
- VS Code shell integration (OSC 633) — <https://code.visualstudio.com/docs/terminal/shell-integration>
- OSC family overview — <https://terminfo.dev/osc>

### OneTerm codebase (internal verification)
- `crates/terminal/src/osc.rs` — `OscPayload`, `Osc133Kind`, `parse_cwd_url`, `decode_osc52`/`encode_osc52`
- `crates/terminal/src/osc_color.rs` — `DynamicColors`, `PendingColorQuery`, `default_color_for_index` (OSC 10/11/12/110-112)
- `crates/terminal/src/backend/osc_router.rs` + `backend/pump.rs` — shared `OscRouter` (both backends): OSC routing, `ColorRequest` enqueue → reply after the parse batch
- `crates/core/src/config/shell.rs` — `resolve_shell` generates OSC 7/133 by shell kind
- `crates/terminal/src/url_policy.rs` — OSC 8 / plain-text target policy (scheme allowlist, display-text mismatch)
