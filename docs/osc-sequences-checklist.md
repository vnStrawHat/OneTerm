# OSC (Operating System Command) Sequences — Checklist & Support Matrix

> Reference document for OSC escape sequences (both **common** and **vendor-specific**),
> with a checklist by group and a support-level matrix for terminals:
> **OneTerm** (this project), **Windows Terminal**, **Zed**, **Ghostty**
> (with references to iTerm2, Kitty, Alacritty, xterm, VTE, VS Code).

---

## ⚠️ Methodology & confidence level

The support matrix below is **verified in practice** (not guessed from old knowledge).
Each column has a different source — the reliability is noted here:

| Terminal | Verify source | Confidence | Test date |
|----------|--------------|:------------:|----------|
| **OneTerm** | Codebase (`crates/terminal/src/osc.rs`, `osc_color.rs`, `backend/osc_router.rs`, `crates/core/src/config/shell.rs`) | 🟢 Very high | current code |
| **Windows Terminal** | MS Learn docs + GitHub PRs (#15727, #18449, #5823, color-query PR) + ansicode.eversources.app | 🟢 High | docs + PRs 2023–2025 |
| **Zed** | zed.dev/docs/terminal + source `terminal_hyperlinks.rs` + issue #17848 | 🟢 High | 2025–2026 |
| **Ghostty** | terminfo.dev (live test, v1.3.1) + `src/terminal/osc.zig` | 🟢 Very high | test 2026-06-18 |
| **iTerm2** | terminfo.dev (live test, v3.6.9) | 🟢 Very high | test 2026-06-18 |
| **Kitty** | terminfo.dev (live test, v0.46.2) | 🟢 Very high | test 2026-06-18 |
| **Alacritty** | `docs/escape_support.md` (v0.13.2) official + PR #5769 + config docs | 🟢 Very high | v0.13.2 |
| **VS Code** | terminfo.dev (live test, xterm.js) | 🟢 Very high | test 2026-06-18 |
| **xterm** | prior knowledge (xterm is the *origin* of many OSCs; ctlseqs doc) | 🟡 Medium | — |
| **VTE** (gnome-terminal) | prior knowledge | 🟡 Medium | — |

> 🟡 = not individually web-verified per cell, based on general knowledge. The 🟢 columns were verified by live test/docs/source.
> **Important**: many values in older versions of this document were **WRONG** — corrected based on verification
> (e.g. Alacritty **has** OSC 52/4/10-12/8 but **not** OSC 7/133; Ghostty **does not** have OSC 17/19 set;
> iTerm2/Kitty/Ghostty/VS Code **all have** OSC 633; VS Code **has** OSC 1337 image).

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

> ⚠️ Ghostty/iTerm2 try to echo back the exact terminator the request used, for maximum compatibility.
> When writing a library, prefer `BEL` for maximum compatibility. OSC 8 per spec should use `ESC \`.

### 0.2 Query mode

Many OSCs support **query**: send `Pt = ?` to request the terminal report its current value.
Example: `ESC ] 10 ; ? BEL` → asks for the default foreground color.
**Note**: not every terminal answers queries (e.g. Alacritty has no OSC 7/133;
Ghostty/Kitty **do not** respond to OSC 52 read — write only).

### 0.3 Color spec format

- `rgb:RRRR/GGGG/BBBB` — 16-bit/channel (full, recommended).
- `rgb:RR/GG/BB` — 8-bit/channel.
- `#RRGGBB` — hex (accepted by most terminals).
- `?` — query current value.

---

## Group A — Window / Icon / Title

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☐ | **0** | Set **both** icon name + window title | `ESC]0;title ST` | Most common, used for tab title. |
| ☐ | **1** | Set **icon name** (title unchanged) | `ESC]1;name ST` | X11 legacy. Alacritty **REJECTED**. |
| ☐ | **2** | Set **window title** | `ESC]2;title ST` | Equivalent to OSC 0 for most modern terminals. |

### A.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 0 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 1 | ❌ | ◐ | ◐ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ |
| 2 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

---

## Group B — Color Palette (indexed colors 0–255)

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☑ | **4** | Set/query 1+ palette colors | `ESC]4;idx:spec ST` | Query: `idx:?`. ✅ OneTerm. |
| ☐ | **5** | Set/query "special" colors | `ESC]5;idx:spec ST` | iTerm2/VS Code/Alacritty **do not** support. |
| ☑ | **104** | Reset 1+ palette colors | `ESC]104;idx ST` or `ESC]104 ST` (all) | xterm origin. ✅ OneTerm. |
| ☐ | **105** | Reset special colors | `ESC]105;idx ST` | Rare. |

### B.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 4   | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 5   | ❌ | ◐ | ❌ | ◐ | ❌ | ✅ | ❌ | ✅ | ◐ | ❌ |
| 104 | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 105 | ❌ | ◐ | ❌ | ◐ | ◐ | ◐ | ❌ | ✅ | ◐ | ◐ |

> ✅ **OneTerm** (from the latest version): OSC 4 **set + query (`idx;?`)** and OSC 104 **reset** (single/all) are supported.
> Shares infrastructure with OSC 10/11/12 via `ColorRequest`: set → `Term.colors[0..256]`, query → reply after
> parse batch (fallback to default palette via `default_color_for_index` + `set_default_colors`), rendered through
> `dynamic_colors().indexed` + `TerminalPalette.indexed`. OSC 5/105 (special colors) still ❌.

---

## Group C — Default & Special Colors (fg/bg/cursor/selection)

| Check | OSC | Purpose | Query | Reset OSC | Notes |
|:-----:|-----|----------|:-----:|:---------:|---------|
| ☑ | **10** | Default foreground | `10;?` | **110** | Common. ✅ OneTerm. |
| ☑ | **11** | Default background | `11;?` | **111** | Common. ✅ OneTerm. |
| ☑ | **12** | Text cursor color | `12;?` | **112** | ✅ OneTerm. |
| ☐ | **13** | Mouse pointer fg color | `13;?` | **113** | Rare; reset 113 present in many terminals. |
| ☐ | **14** | Mouse pointer bg color | `14;?` | **114** | Rare; reset 114 present. |
| ☐ | **17** | Selection (highlight) bg | `17;?` | **117** | Kitty/iTerm2/VS Code have reset 117. |
| ☐ | **19** | Selection (highlight) fg | `19;?` | **119** | Kitty/iTerm2/VS Code have reset 119. |
| ☑ | **110–112** | Reset fg/bg/cursor | — | — | ✅ OneTerm. |
| ☐ | **117/119** | Reset selection bg/fg | — | — | |
| ☐ | **39** | Default fg (xterm alias for OSC 10) | — | — | Less common. |

### C.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 10/11 | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 12 (cursor) | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 13–14 (pointer) | ❌ | ❌ | ❌ | ◐ | ◐ | ✅ | ❌ | ✅ | ❌ | ◐ |
| 17/19 (selection set) | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ❌ | ✅ | ❌ | ❌ |
| 110–112 (reset) | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 117/119 (reset sel) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | ❌ | ✅ |

> ✅ **OneTerm** (from the latest version): OSC 10/11/12 **set + query (`?`)** and OSC 110/111/112 **reset** are supported.
> `Event::ColorRequest` is enqueued in `LocalListener`/`SshListener` then answered after each parse batch
> (reads `Term.colors()`, falls back to theme default via `set_default_colors`); set/reset rendered via `dynamic_colors()`.
> ⚠️ Ghostty has **reset** 117/119 but **does not** support **set** 17/19 (terminfo: "No OSC 17/19 response").
> Many terminals have reset 113/114 (pointer) without explicitly listing set 13/14 → marked ◐.

---

## Group D — Clipboard

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☑ | **52** | Set/query clipboard (base64) | `ESC]52;c;base64 ST` | `c`=clipboard, `p`=primary. Query: `c?`. ✅ OneTerm (write+read). |

### D.1 Security notes & support level

OSC 52 is security-controversial (reads clipboard). Many terminals are **write-only, no read** or require config.

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 52 | ✅ | ✅ | ❌ | ◐ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ |

- **OneTerm**: ✅ write (always on), ◐ read (default **off**). `OscSink` parses base64 (set) + query `?`;
  set goes through alacritty `ClipboardStore` → `SessionEvent::Clipboard`; read (`52;c;?`) →
  `SessionEvent::ClipboardRead` → UI replies `52;c;<base64>` (`encode_osc52`) **only when** setting
  `security.allow_clipboard_read = true` (default `false`, because read exposes the local clipboard to a program,
  including remotely over SSH).
- **Windows Terminal**: ✅ — merged (PR #18449/#5823); has a disable setting.
- **Zed**: ❌ — still an open feature request (issue #17848), not implemented.
- **Ghostty**: ◐ — **write ✅, read ❌** (terminfo: "No OSC 52 read response").
- **iTerm2**: ✅ — read + write both OK (requires enabling "Allow clipboard access").
- **Kitty**: ◐ — **write ✅, read ❌** (terminfo).
- **Alacritty**: ✅ — config `terminal.osc52 = "OnlyCopy"|"OnlyPaste"|"CopyPaste"|"Disabled"`.
- **VS Code**: ✅ — read + write (terminfo).

---

## Group E — Hyperlinks (OSC 8)

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☐ | **8** | Open/close hyperlink | `ESC]8;params;URL ST text ESC]8;; ST` | `id=ID` param to group link cells. |

```
ESC ] 8 ; params ; URL ST   ← open link
  <displayed text>
ESC ] 8 ; ; ST               ← close link
```

### E.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 8 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |

- **OneTerm**: ✅ — alacritty VTE stores hyperlink in cell; the view reads `cell.hyperlink()` and every target passes `ExternalTargetPolicy` (`crates/terminal/src/url_policy.rs`, display-text vs target check via `validate_with_display`).
- **Zed**: ✅ — `terminal_hyperlinks.rs` reads `cell.hyperlink()` + `try_osc8_url_to_path`.
- **Alacritty**: ✅ — commit "Fixes #922" added OSC 8 (the old ansicode page was wrong in listing alacritty ❌).
- **xterm**: ❌ — does not support OSC 8.

---

## Group F — Current Working Directory (CWD)

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☐ | **7** | Set CWD (file:// URI) | `ESC]7;file://host/path ST` | De-facto standard (VTE origin). |
| ☐ | **9;9** | Set CWD (ConEmu/Windows path) | `ESC]9;9;C:\path ST` | ConEmu/Windows Terminal. |

### F.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 7 (file URI) | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ✅ |
| 9;9 (ConEmu) | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

- **OneTerm**: ✅ OSC 7 — `OscSink` parses `file://` → `parse_cwd_url`. **No** 9;9.
- **Alacritty**: ❌ OSC 7 — `escape_support.md` **does not** list OSC 7 → alacritty_terminal **drops** it.
  (That is why OneTerm must parse OSC 7 itself via `OscSink` in parallel with VTE.)
- **Windows Terminal**: ✅ both 7 and 9;9 (MS docs).
- **Zed**: ◐ — uses alacritty_terminal (drops OSC 7); may have its own parser (not confirmed in docs).

---

## Group G — Notifications & Progress

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☑ | **9** | Desktop notification (iTerm2/WT) | `ESC]9;msg ST` | iTerm2 origin. ✅ OneTerm. |
| ☑ | **9;4** | Progress bar (ConEmu/WT) | `ESC]9;4;state;pct ST` | state: 0/1/2/3/4. WT 1.18+. ✅ OneTerm. |
| ☐ | **9;1/2/3** | ConEmu misc (sleep/msgbox/tabtitle) | `ESC]9;1;ms ST` etc. | ConEmu-specific. |
| ☐ | **99** | Kitty notification (extended) | `ESC]99;i=ID;payload ST` | icon/focus/urgency. |
| ☐ | **777** | urxvt notification | `ESC]777;notify;title;body ST` | urxvt origin. |

### G.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 9 (notif) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ◐ | ✅ |
| 9;4 (progress) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ◐ | ✅ |
| 9;1/2/3 (ConEmu) | ❌ | ◐ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 99 (kitty) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 777 (urxvt) | ❌ | ◐ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ◐ | ✅ |

> ✅ **OneTerm** (from the latest version): OSC 9 (notification → toast via `window.push_notification`) and OSC 9;4
> (progress → thin progress bar at the top edge of the terminal, state 0-4). Forwarded by the vendored
> alacritty fork's `Event::Osc` hook (upstream alacritty drops OSC 9) to `OscRouter` → `OscPayload::Notification`/`Progress` → `SessionEvent`. OSC 9;7 (agent status) is also supported, see [`osc-agent-status.md`](osc-agent-status.md). Still ❌: 9;1/2/3
> (ConEmu misc), 99 (kitty), 777 (urxvt).
> - **Ghostty**: ✅ all (osc.zig has `conemu_*` for 9;1–9;11 + `show_desktop_notification` for 9/777/99).
> - **VS Code**: ✅ 9/9;4/99/777 (terminfo); 9;1/2/3 ❌.
> - **Alacritty/Zed**: ❌ all notifications.

---

## Group H — Shell Integration / Prompt Markers

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☐ | **133** | FinalTerm prompt markers | `133;A`/`B`/`C`/`D;exit` | De-facto shell integration standard. |
| ☐ | **133;P** | Prompt properties (kext) | `133;P;k=i ST` | Kitty/Ghostty/iTerm2/VS Code. |
| ☐ | **633** | VS Code shell integration | `633;A`..`D;exit`/`E`/`P` | VS Code own; many terminals adopt. |
| ☐ | **633;SetMark** | VS Code mark | `633;SetMark ST` | Bookmark in scrollback. |

### Standard OSC 133 — 4 markers:

```
ESC]133;A ST      ← Prompt start
ESC]133;B ST      ← Command start
ESC]133;C ST      ← Command output start
ESC]133;D;exit ST ← Block end (exit code optional)
```

### H.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 133 (A/B/C/D) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ✅ |
| 133;P | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 633 | ❌ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 633;SetMark | ❌ | ◐ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |

- **OneTerm**: ✅ OSC 133 A/B/C/D (code: `Osc133Kind` enum + exit code). **No** 133;P/633.
- **Alacritty**: ❌ OSC 133 — `escape_support.md` **does not** list 133 → alacritty_terminal **drops** it
  (reason OneTerm must parse it itself via `OscSink`).
- **iTerm2/Kitty/Ghostty/VS Code**: ✅ both 133 + 633 + 133;P (terminfo).
- **Windows Terminal**: ✅ 133 + 633 (PR #15727 alias); 133;P ❌.
- **Zed**: ✅ 133 (discussion #44359); ❌ 633 (Zed uses 133, 633 is VS Code-specific).

---

## Group I — Font

| Check | OSC | Purpose | Format | Notes |
|:-----:|-----|----------|--------|---------|
| ☐ | **50** | Set/query font | `ESC]50;font-spec ST` | xterm origin. Alacritty only CursorShape. |

### I.1 Support level

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 50 | ❌ | ❌ | ❌ | ❌ | ◐ | ❌ | ◐ | ✅ | ❌ | ❌ |

- **Alacritty**: ◐ — OSC 50 IMPLEMENTED but **CursorShape only**, not font.
- **Kitty/iTerm2**: use OSC 710/7770/7777 (own font) instead of 50.

---

## Group J — Vendor-specific & Misc

| Check | OSC | Terminal/Context | Purpose | Notes |
|:-----:|-----|------------------|----------|---------|
| ☐ | **1337** | iTerm2 | Inline image + subcodes | `ESC]1337;File=...;inline=1:base64 ST`. |
| ☐ | **20** | (kext) | Background opacity | `ESC]20;alpha ST`. |
| ☐ | **46** | xterm | Log file | `ESC]46;path ST`. |
| ☐ | **21** | Kitty | Kitty color protocol | `ESC]21;... ST`. |
| ☐ | **22** | Kitty/Ghostty | Mouse pointer shape | `ESC]22;name ST`. |
| ☐ | **66** | Kitty | Text sizing | `ESC]66;... ST`. |
| ☐ | **3008** | systemd | Context signal (UAPI) | `ESC]3008;... ST`. |

### J.1 Support level (selected)

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 1337 (image) | ❌ | ❌ | ◐ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| 21 (kitty color) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 22 (mouse shape) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 20 (opacity) | ❌ | ❌ | ❌ | ❌ | ◐ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 46 (logfile) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ |

- **Inline image**: competition between iTerm2 (OSC 1337), Kitty graphics (APC), Sixel.
  Ghostty/VS Code/iTerm2 ✅ OSC 1337; Kitty ❌ OSC 1337 (uses own Kitty graphics APC);
  Windows Terminal ❌ (uses Sixel from 1.22).
- **OneTerm**: ❌ all vendor-specific.

---

## Quick summary table — top commonly-used OSCs

> "Must-have" checklist when targeting multiple terminals. OneTerm column for project reference.

| Check | OSC | Name | Popularity |
|:-----:|:---:|-----|:-----------:|
| ☐ | 0/2 | Window title | ⭐⭐⭐⭐⭐ |
| ☐ | 7 | CWD (file://) | ⭐⭐⭐⭐⭐ |
| ☐ | 8 | Hyperlinks | ⭐⭐⭐⭐ |
| ☑ | 4 | Color palette set/query | ⭐⭐⭐⭐ |
| ☑ | 10/11/12 | Default FG/BG/cursor | ⭐⭐⭐⭐ |
| ☑ | 52 | Clipboard | ⭐⭐⭐ |
| ☐ | 133 | Shell integration markers | ⭐⭐⭐⭐ |
| ☑ | 9 | Desktop notification | ⭐⭐⭐ |
| ☑ | 104/110-112 | Reset colors | ⭐⭐⭐ |
| ☐ | 633 | VS Code shell integration | ⭐⭐⭐ |

### OneTerm — current status summary

| OSC group | OneTerm | Assessment |
|----------|:-------:|----------|
| 0/2 title | ✅ | OK |
| 7 CWD | ✅ | OK (self-parses because alacritty drops it) |
| 8 hyperlink | ✅ | OK (via alacritty cell) |
| 52 clipboard | ✅ | OK (self-parse + alacritty EventListener) |
| 133 shell integration | ✅ | OK (A/B/C/D + exit code) |
| 10/11/12 + 110–112 colors | ✅ | OK (set + query + reset fg/bg/cursor) |
| 4 + 104 palette colors | ✅ | OK (set + query + reset index 0–255) |
| 5/13–19/105/117–119 colors | ❌ | **Gap** — special/pointer/selection not mapped |
| 9 + 9;4 notification/progress | ✅ | OK (toast + progress bar) |
| 99/777 notifications | ❌ | **Gap** (kitty/urxvt) |
| 633 VS Code | ❌ | **Gap** (133 only) |
| 1337 image | ❌ | **Gap** |

> OneTerm currently **covers** the 5 core groups (title/CWD/hyperlink/clipboard/shell-integration) **+ default colors
> (OSC 10/11/12/110-112) + color palette (OSC 4/104) + notification/progress (OSC 9, 9;4)**, but **lacks**
> special colors (5), pointer/selection (13–19), kitty/urxvt notification (99/777), VS Code 633, inline image.

---

## Legend

| Symbol | Meaning |
|:-------:|---------|
| ✅ | Fully supported (verified in practice). |
| ◐ | Partially supported: only a subset of parameters, write/read only, requires config, or reset only without set. |
| ❌ | Not supported (verified in practice or officially documented as REJECTED/missing). |
| 🟢/🟡 | Source column confidence (see Methodology table). |
| ⭐ | Popularity (1–5, subjective assessment). |

---

## Practical experience

1. **ST terminator**: Use `BEL` (`\x07`) for maximum compatibility. OSC 8 per spec should use `ESC \`.
2. **Query response**: not every terminal answers queries. Ghostty/Kitty **do not** read OSC 52;
   Alacritty has no OSC 7/133; Ghostty does not respond to OSC 5/17/19 queries.
3. **OSC 52 clipboard**: always expect rejection. Distinguish **write** (common) vs **read** (rare, Ghostty/Kitty ❌).
4. **OSC 7 CWD**: must be a full `file://` URI (including host). Upstream Alacritty **does not** support OSC 7
   → apps using alacritty_terminal (like OneTerm/Zed) must **parse it themselves** in parallel.
5. **Shell integration**: 133 (FinalTerm) is the common standard; 633 is VS Code-specific but iTerm2/Kitty/Ghostty
   also adopt it. Must wrap the 4 markers A/B/C/D correctly.
6. **Color spec**: prefer `rgb:RR/GG/BB` or `rgb:RRRR/GGGG/BBBB`. Avoid `#hex` if you need old-xterm compatibility.
7. **Vendor-specific**: only use when you are sure of the target terminal. Detect via `TERM`, `TERM_PROGRAM`,
   `WT_SESSION`, `KITTY_WINDOW_ID`, `GHOSTTY_RESOURCES_DIR`...
8. **Do not nest OSC**: close one OSC before opening another.
9. **Windows Terminal**: good cross-OSC support (133+633+9;9+9;4+52+4/10/11/12). Uses Sixel (1.22+) for images, not Kitty graphics.
10. **Ghostty**: highly standards-compliant + extensions (133;P, 633, 9;1–11, 21, 22, 66, 3008, iTerm2 1337 image).
    **No** Sixel, **no** OSC 17/19 set, **no** OSC 52 read.
11. **Zed**: 133 + 8 + 7 (via alacritty VTE + own parser). **No** OSC 52 (open feature request),
    **no** 633, **no** notifications. Uses alacritty_terminal so inherits its strengths/weaknesses.
12. **Alacritty**: has OSC 4/8/10/11/12/52/104/110-112 (config `terminal.osc52`).
    **No** OSC 7/133/9/633/777. OSC 50 CursorShape only. Intentionally minimal.
13. **Kitty**: very wide support (4/5/7/8/10-19/21/22/52-write/66/99/104/110-119/133+P/633/777/3008...).
    **No** OSC 1337 image (uses Kitty graphics APC), **no** OSC 52 read, **no** Sixel.
14. **iTerm2**: near-complete (4/7/8/9/9;4/10-19/21/22/52/99/104/110-119/133+P/633/777/1337/3008...).
    **No** OSC 5, **no** Sixel render (DA1 advertises but does not render).
15. **VS Code (xterm.js)**: surprisingly wide support (4/7/8/9/9;4/10-12/52/99/104/110-119/133+P/633/777/1337/3008...).
    **No** OSC 5/17/19, **no** Kitty graphics display, **no** Sixel.
16. **OneTerm** (VTE = `alacritty_terminal`): Supports **OSC 0/2, 7, 8, 52 (base64+query), 133 (A/B/C/D+exit),
    4 (set+query) + 104 (reset), 10/11/12 (set+query) + 110/111/112 (reset), 9 (notification), 9;4 (progress)**.
    - 133/9/9;4 are parsed in parallel via `OscSink` (alacritty VTE drops OSC 7/9/133); `OscSink` uses a FIFO
      queue so multiple OSCs in the same read batch are all kept + processed in order.
    - OSC 8 stored in cell; OSC 52 goes through `EventListener` + OscSink.
    - OSC 4/104 + 10/11/12/110-112: alacritty already parses (set → `Term.colors`, reset → clear); OneTerm renders
      via `dynamic_colors()` (`TerminalPalette.indexed` for index 0-255) and answers queries via
      `Event::ColorRequest` (enqueue → reply after parse batch, fallback default palette via `set_default_colors`
      + `default_color_for_index`).
    - OSC 9 → `SessionEvent::Notification` → toast `window.push_notification`; OSC 9;4 →
      `SessionEvent::Progress(TerminalProgress)` → thin progress bar at the top edge of the terminal view.
    - **No** special colors (5/105), pointer/selection (13–19/113–119): not mapped yet;
    - **No** notification 99 (kitty) / 777 (urxvt) / 9;1-3 (ConEmu misc), font (50), 633, 1337.
    - Self-generates OSC 7 + 133 A via `PROMPT_COMMAND` (bash) / `PS1` (zsh) / `PROMPT` (cmd).

---

## References (verified)

### Live tests (terminfo.dev — test matrix, June 2026)
- Ghostty — <https://terminfo.dev/terminals/ghostty> (v1.3.1, 231/254)
- iTerm2 — <https://terminfo.dev/terminals/iterm2> (v3.6.9, 238/254)
- Kitty — <https://terminfo.dev/terminals/kitty> (v0.46.2, 218/254)
- VS Code — <https://terminfo.dev/terminals/vs-code> (xterm.js, 223/254)
- OSC family — <https://terminfo.dev/osc>, <https://ansicode.eversources.app/en/family/osc>

### Official docs / source
- Alacritty escape support — <https://github.com/alacritty/alacritty/blob/master/docs/escape_support.md> (v0.13.2)
- Alacritty OSC 52 config — <https://alacritty.org/config-alacritty.html> (`terminal.osc52`)
- Alacritty OSC 4 query PR — <https://github.com/alacritty/alacritty/pull/5769>
- Ghostty OSC source — <https://github.com/ghostty-org/ghostty/blob/main/src/terminal/osc.zig>
- Ghostty OSC 52 docs — <https://ghostty.org/docs/vt/osc/52>
- Windows Terminal shell integration — <https://learn.microsoft.com/en-us/windows/terminal/tutorials/shell-integration>
- Windows Terminal OSC 633 PR — <https://github.com/microsoft/terminal/pull/15727>
- Windows Terminal OSC 52 PRs — <https://github.com/microsoft/terminal/pull/5823>, <https://github.com/microsoft/terminal/pull/18449>
- VS Code shell integration — <https://code.visualstudio.com/docs/terminal/shell-integration>
- Zed terminal docs — <https://zed.dev/docs/terminal>
- Zed OSC 52 request — <https://github.com/zed-industries/zed/issues/17848>
- Zed hyperlinks source — `crates/terminal/src/terminal_hyperlinks.rs`
- xterm ctlseqs — <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html>
- FinalTerm OSC 133 spec — <https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md>

### OneTerm codebase (internal verification)
- `crates/terminal/src/osc.rs` — `OscPayload`, `Osc133Kind`, `parse_cwd_url`, `decode_osc52`/`encode_osc52`
- `crates/terminal/src/osc_color.rs` — `DynamicColors`, `PendingColorQuery`, `default_color_for_index` (OSC 10/11/12/110-112)
- `crates/terminal/src/backend/osc_router.rs` + `backend/color_reply.rs` — shared `OscRouter` (both backends): OSC routing, `ColorRequest` enqueue → reply after the parse batch
- `crates/core/src/config/shell.rs` — `resolve_shell` generates OSC 7/133 by shell kind
- `crates/terminal/src/url_policy.rs` — OSC 8 / plain-text target policy (scheme allowlist, display-text mismatch)
