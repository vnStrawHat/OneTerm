# OSC (Operating System Command) Sequences — OneTerm Support Checklist

> Reference document for OSC escape sequences (both **common** and **vendor-specific**),
> with a checklist by group and, for each group, what **OneTerm** does and where the
> behaviour lives in the codebase.

---

## ⚠️ Methodology & confidence level

Every ✅ / ◐ / ❌ below is read from the OneTerm codebase, not from memory.
Since `US-0098` an OSC is parsed in **one** place, the engine, and met in the adapter by policy
only, so there are two files to read and one table:

* `crates/vt/src/terminal/dispatch.rs` — the built-in arms, one per number in
  `OscRoutes::BUILTIN`, each emitting a typed `VtEvent`;
* `crates/vt/src/terminal/osc.rs` — `OscRoutes`, the per-number routing table;
* `crates/terminal/src/handle.rs` — `adapter_config`, the three calls that say which numbers are
  OneTerm's rather than a terminal's;
* `crates/terminal/src/backend/osc_router.rs`, `crates/terminal/src/osc_color.rs`,
  `crates/terminal/src/security_policy.rs`, `crates/core/src/config/shell.rs` — the policy half.

The **Route** column below is what `Config::osc_routes` says about the number:

| Route | Meaning |
| --- | --- |
| `Builtin` | the engine parses it and emits a typed event; the default for every number it implements |
| `BuiltinAndForward` | the engine parses it **and** hands over the raw sequence |
| `Forward` | the engine does not touch it; the raw sequence goes to OneTerm |
| `Drop` | parsed, counted in `FeedStats::unhandled_sequences`, discarded; the default for everything else |

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
| ◐ | **1** | Set **icon name** (title unchanged) | `ESC]1;name ST` | ◐ — parsed (`VtEvent::IconName`); OneTerm has no icon-name concept, so nothing displays it. |
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
| ☑ | **17** | Selection (highlight) bg | `17;?` | **117** | ✅ set + query (`US-0102`) |
| ☑ | **19** | Selection (highlight) fg | `19;?` | **119** | ✅ set + query (`US-0102`) |
| ☑ | **110–112** | Reset fg/bg/cursor | — | — | ✅ |
| ☐ | **117/119** | Reset selection bg/fg | — | — | ❌ — `RIS` clears them, nothing else does |
| ☐ | **39** | Default fg (xterm alias for OSC 10) | — | — | ❌ |

> ✅ **OneTerm**: OSC 10/11/12 and OSC 17/19 **set + query (`?`)**, and OSC 110/111/112 **reset**.
> OSC 17 / 19 are `ColorKey::SelectionBackground` / `SelectionForeground`, two slots in the same
> override table, handled in `crates/vt/src/terminal/dispatch.rs` (`osc_selection_color`) with the
> keys in `crates/vt/src/terminal/color.rs`. They take **one** parameter each, not xterm's advancing
> multi-parameter form: advancing from 17 lands on OSC 18, the Tektronix cursor, which the engine
> does not have. There is no OSC 117 / 119 reset yet, so `RIS` is the only thing that clears them.
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

- **write**: ✅ always on. The VT engine decodes the base64 payload and reports it
  (`ClipboardStore` → `SessionEvent::Clipboard`); the engine never applies a policy, which stays in
  `crates/terminal/src/security_policy.rs`.
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

- **OneTerm**: ✅ OSC 7, route `Builtin`. **No** 9;9.
- There is exactly **one** parser. `oneterm-vt` handles the number itself and reports
  `VtEvent::Cwd { host, path }` — percent-decoded, with a `file:///C:/...` drive URL's leading
  slash stripped, and otherwise **unresolved**: no `PathBuf`, no host check, no filesystem. Whether
  a remote shell's directory may be trusted is policy, and stays in `OscRouter` →
  `TerminalSecurityPolicy::sanitize_cwd` → `SessionEvent::Cwd`. Neither
  `alacritty_terminal` nor `rio-vt` handles OSC 7 at all.

---

## Group G — Notifications & Progress

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☑ | **9** | Desktop notification | `ESC]9;msg ST` | ✅ → toast. |
| ☑ | **9;4** | Progress bar | `ESC]9;4;state;pct ST` | ✅ — state 0–4. |
| ☑ | **20308;1** | Agent status | see [`osc-agent-status.md`](osc-agent-status.md) | ✅ — OneTerm proposal. |
| ☑ | **20308;0** | Agent-protocol support query | `ESC]20308;0 ST` → `ESC]20308;0;1;OneTerm;<ver> ST` | ✅ |
| ☑ | **9;7** | Agent status — **deprecated alias** | see [`osc-agent-status.md`](osc-agent-status.md) § 3.1 | ⚠️ — accepted for one release, then dropped. |
| ☐ | **9;1/2/3** | ConEmu misc (sleep/msgbox/tabtitle) | `ESC]9;1;ms ST` etc. | ❌ |
| ☐ | **99** | Extended notification protocol | `ESC]99;i=ID;payload ST` | ❌ |
| ☐ | **777** | urxvt notification | `ESC]777;notify;title;body ST` | ❌ |

> ✅ **OneTerm**: OSC 9 and OSC 9;4 are the engine's, parsed into `VtEvent::Notification` and
> `VtEvent::Progress(Progress)`. `OscRouter` applies the policy — the 8 KiB notification cap and the
> ten-per-second rate limit — and emits `SessionEvent::Notification` (toast via
> `window.push_notification`) or `SessionEvent::Progress` (a thin bar at the top edge, states 0–4).
>
> **OSC 20308** is the agent channel — see [`osc-agent-status.md`](osc-agent-status.md) — and is
> OneTerm's own proposal rather than a terminal standard, so its route is **`Forward`**: the engine
> never parses it and contains no mention of the number. Sub-code `1` is the status event and `0`
> the support query, answered on the transport; `2` and above are reserved, and an unrecognised
> sub-code is ignored and counted. `OscRoutes::large(20308, true)` raises its payload ceiling to the
> 8 KiB §3.4 publishes.
>
> ⚠️ **OSC 9;7** carried the agent channel until `US-0088` and is kept as a deprecated alias for
> **one release** — parsed identically, counted, and logged once per session. Routing is per number
> and sub-codes are payload, so OSC 9 carries the route **`BuiltinAndForward`**: the engine keeps
> parsing notifications and `9;4` progress *and* hands over the raw sequence, and OneTerm discards
> the notification whose first forwarded parameter is `7` and parses the alias itself. The engine
> has never heard of the sub-code. `9;7` is ConEmu's "run some process with arguments" (§ 2.1),
> which is why it is going away: an agent emitting it under ConEmu or cmder asks that terminal to
> spawn a process. Agents should emit `20308;1` only.
>
> Still ❌: 9;1/2/3, 99, 777.

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

- **OneTerm**: ✅ OSC 133 A/B/C/D, route `Builtin` (`VtEvent::ShellMark(ShellMark)`, exit code
  included). **No** 133;P/633.
- `alacritty_terminal` does not handle OSC 133 at all. `oneterm-vt` both records the mark **in the
  engine** — the A/B/C/D state lands on the cell as `Semantic::{Prompt, Input, Output}` and A and C
  each register a tracked `AnchorKind::Mark`, so a mark survives a reflow — and reports it, because
  the prompt counter and the last exit code live above the seam in `SharedState`.

---

## Group I — Font

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ◐ | **50** | Set/query font (xterm) / **cursor shape** (rxvt, `CursorShape=0\|1\|2`) | `ESC]50;CursorShape=1 ST` | ◐ — cursor shape ✅ (`VtEvent::CursorStyleChanged`); the font half ❌, font is a settings concern. |

---

## Group J — Vendor-specific & Misc

| Check | OSC | Purpose | Format | OneTerm |
|:-----:|-----|----------|--------|---------|
| ☐ | **1337** | Inline image + subcodes | `ESC]1337;File=...;inline=1:base64 ST` | ❌ |
| ☐ | **20** | Background opacity | `ESC]20;alpha ST` | ❌ |
| ☐ | **21** | Extended color protocol | `ESC]21;... ST` | ❌ |
| ◐ | **22** | Mouse pointer shape | `ESC]22;name ST` | ◐ — parsed and reported by name (`VtEvent::Pointer`); the view does not change the pointer yet. |
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
| 17/19 selection colors | ✅ | OK (set + query; no 117/119 reset) |
| 5/13–16/18/105/113–119 colors | ❌ | **Gap** — special/pointer/Tektronix not mapped |
| 9 + 9;4 notification/progress | ✅ | OK (toast + progress bar) |
| 99/777 notifications | ❌ | **Gap** |
| 633 shell integration | ❌ | **Gap** (133 only) |
| 1337 image | ❌ | **Gap** (Sixel via DCS is supported instead) |

> OneTerm currently **covers** the 5 core groups (title/CWD/hyperlink/clipboard/shell-integration) **+ default colors
> (OSC 10/11/12/110-112) + color palette (OSC 4/104) + notification/progress (OSC 9, 9;4) + Sixel images (DCS)**, but **lacks**
> special colors (5), the pointer pair (13–14), the 113–119 resets, notification 99/777, 633,
> OSC 1337 / Kitty images.

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
4. **OSC 7 CWD**: must be a full `file://` URI (including host). No VT engine interprets OSC 7
   itself; it is forwarded out of the single VT pass and OneTerm interprets it.
5. **Shell integration**: 133 (FinalTerm semantics) is the common standard; 633 is a VS Code extension of it.
   Wrap the 4 markers A/B/C/D correctly.
6. **Color spec**: prefer `rgb:RR/GG/BB` or `rgb:RRRR/GGGG/BBBB`. Avoid `#hex` if you need old-xterm compatibility.
7. **Vendor-specific**: only use when you are sure of the target terminal. Detect via `TERM`, `TERM_PROGRAM`,
   and terminal-specific environment variables.
8. **Do not nest OSC**: close one OSC before opening another.
9. **OneTerm** (VT engine = `oneterm-vt`): the engine implements **OSC 0, 1, 2, 4, 7, 8, 9 (and
   `9;4`), 10/11/12, 22, 50, 52, 104, 110/111/112 and 133** — the set `OscRoutes::BUILTIN`
   publishes — each parsed once and reported as a typed `VtEvent`. OneTerm adds **OSC 20308** (the
   agent channel) and its deprecated `9;7` alias through `Config::osc_routes`, with no engine
   change.
   - There is no second parser anywhere. Events arrive in the `feed` batch in byte order, and that
     batch is a `Vec` — the ordering promise is a property of the batch, not of a queue.
   - OSC 8 is stored on the cell; OSC 52 is decoded by the engine and the **policy** — clipboard,
     notifications, cwd, rate limits — stays in `security_policy.rs`.
   - OSC 4/104 + 10/11/12/110-112: the VT engine already parses these (set → `Term.colors`, reset → clear);
     OneTerm renders via `dynamic_colors()` (`TerminalPalette.indexed` for index 0-255) and answers queries via
     `Event::ColorRequest` (enqueue → reply after parse batch, fallback default palette via `set_default_colors`
     + `default_color_for_index`).
   - OSC 9 → `SessionEvent::Notification` → toast `window.push_notification`; OSC 9;4 →
     `SessionEvent::Progress(TerminalProgress)` → thin progress bar at the top edge of the terminal view.
   - OSC 17/19 (selection bg/fg): parsed by the engine into `ColorKey::Selection*`, answered through
     the same `queue_color_query` path as 10/11/12 (`US-0102`).
   - **No** special colors (5/105), pointer colors (13–14), or any of the 113–119 resets.
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
