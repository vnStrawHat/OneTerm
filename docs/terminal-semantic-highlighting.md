# Terminal Semantic Highlighting — Rust-native design for OneTerm

> **Status:** Historical design record. Current implementation paths are listed in [`docs/architecture.md`](architecture.md).

> **Goal**: colorize **plain-text terminal output** (logs, `cat`, router `show`,
> `systemctl status`, compile output — anything that emits no SGR codes) by *meaning*,
> on top of the existing ANSI/SGR color layer.
>
> **Core idea**: run a **second semantic pass** over the rendered text that
classifies tokens and overrides the *default* foreground. Programs that already
colorize themselves keep their ANSI colors.
>
> **What we deliberately avoid**: TextMate-grammar JSON, a string-scope model, and
> Oniguruma/PCRE regexes. They are a poor fit for Rust and for terminal output (which is
> line-oriented and flat, not recursively nested source code). This doc specifies a
> bespoke, Rust-optimized design built around a **closed `u8` class enum**, a
> **single-pass line scanner**, **Aho-Corasick** keyword sets, the native **`regex`
> crate** (no C dependency), and an **OSC 133 fast path** that OneTerm already parses.
>
> **Created**: 2026-06-23 · **Status**: proposal

---

## 1. Why a bespoke design

| Rejected approach | Problem for OneTerm/Rust | OneTerm choice |
|---|---|---|
| TextMate `.lexer` JSON (recursive `begin/end/include`, `repository`) | Terminal output is **flat & line-oriented** — recursive grammar machinery is unused complexity. Interpreting JSON patterns per line is slow. | A **single-pass line scanner** with a 2-state machine (prompt-line vs output-line). No recursive descent. |
| Oniguruma/PCRE (C dep, backtracking, `\g<X>` subroutines, `++` possessive) | C dependency on a Windows-first app; backtracking = ReDoS risk; Rust can't borrow-check the patterns. | Native **`regex` crate** (DFA, no backtracking, ReDoS-safe) + **`aho-corasick`** for keyword sets. Zero C. |
| Open-ended **string scopes** (`token.error-token.linux`, …) + prefix-selector matching at render time | String allocs + selector resolution per token per frame. | **Closed `#[repr(u8)] enum Class`** (~20 variants). Theme mapping pre-resolved into a **`[Style; N]` flat array** → O(1) index, branchless, no selector engine. |
| Regex-guessed prompts (gnarly regex for `$/#%/>/❯/λ/U+E0B0`) | Fragile, slow, wrong on exotic prompts. | **OSC 133 authoritative fast path** (OneTerm already parses it) → exact prompt/command/output row roles for free; regex prompt detection is only the fallback for shells without integration. |
| Verbose per-shell grammar files (6 near-duplicate `.lexer`) | Maintenance burden, duplicated keyword regex. | One Rust **`ShellProfile`** enum carrying only the 2 things that actually differ (prompt pattern + path separator). Keyword/IP/number rules are **shared compiled statics**. |

The result: a small pure-Rust crate, no C, no per-line JSON interpretation, no string
scope engine, and a render-time cost of *one array index per cell*.

---

## 2. Core design decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | **Closed `Class` enum, `#[repr(u8)]`** (~20 variants, `0 = Default`). | Terminal output has a small, stable semantic vocabulary. A closed enum → `u8` per cell → flat-array theme lookup. |
| D2 | **Single-pass line scanner**, 2-state (PromptLine / Output). | Terminal lines are flat; no recursive grammar needed. One left-to-right scan per line. |
| D3 | **OSC 133 fast path** for row roles; regex prompt detection only as fallback. | OneTerm already parses `Osc133Kind` (`PromptStart`/`PromptEnd`/`CommandStart`/`OutputEnd{exit_code}`). Use it to know *exactly* which rows are prompt/command/output — no guessing. |
| D4 | **Aho-Corasick** for keyword sets (error/warn/success/info/debug words). | ~40 literal keywords → one SIMD-accelerated pass, no backtracking, no per-keyword regex. |
| D5 | **`regex` crate** for structural patterns (IP v6, datetime, MAC). Hand-written probes for trivial ones (IPv4, path, number, prompt sign). | DFA, no C, ReDoS-safe. Hand-written where it's both faster and simpler than regex. |
| D6 | **`[Style; Class::COUNT]` theme array**, pre-resolved at theme-build. | Render-time lookup = `styles[class as usize]` — one index, branchless. No TextMate selector matching. |
| D7 | **Per-cell `Class` as `Box<[u8]>` per row**, same shape as the existing `url_mask: &[bool]`. | Reuses the established overlay pattern in `layout_row`. Batch key gains one `u8`. |
| D8 | **Merge policy: explicit-ANSI fg wins; semantic overrides only default-fg.** Prompt-line bg + decorations are additive. | Programs that colorize themselves are respected; plain text gets colored. |
| D9 | **Visible-viewport only**, per-line hash cache (extend `RowLayoutCache` key). | Never lex scrollback; unchanged lines skip. Matches the existing perf bar. |
| D10 | **Pure crate `crates/highlight`** (no GPUI); `ui` bridges `Class`→`gpui::Hsla`. | Honors OneTerm's "pure core, GPUI only in ui" layering. |

---

## 3. The `Class` enum

```rust
// crates/highlight/src/class.rs
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Class {
    Default      = 0,  // no semantic class → ANSI only
    // ── line roles ──
    PromptSign   = 1,  // the $ # > ❯ ➜ glyph
    Command      = 2,  // command word (first token after prompt)
    Option       = 3,  // --flag / -x / /flag
    // ── semantic log tokens ──
    Error        = 4,  // error/fail/denied/refused/…
    Success      = 5,  // ok/success/passed/valid/…
    Warn         = 6,  // warning/closed/exited/terminated/…
    Info         = 7,  // info/login/access/connection/…
    Debug        = 8,  // debug / ls -l file-type char
    // ── structural ──
    Path         = 9,  // /usr/bin, C:\Users\…
    Ip           = 10, // IPv4 / IPv6
    Mac          = 11, // aa:bb:cc:dd:ee:ff
    DateTime     = 12, // 2026-06-23, 14:30, Jan, Mon
    Number       = 13, // 0x1F, 42, 1.5e3
    String       = 14, // "…" / '…'
    Operator     = 15, // = ; | ? * $ < > & + - :
    Bracket      = 16, // ( ) [ ] { }
    Url          = 17, // subsumes the existing url_mask
    Permission   = 18, // rwx bits (ls -l)
    // reserved 19..255 for future / Custom(u8) escape hatch
}

impl Class {
    pub const COUNT: usize = 19;
}
```

> **Closed vs open**: users don't author new *categories* of terminal output — they tune
> *which words* map to `Error` (a **data** change, not a scope change). Closed enum + open
> keyword data is the right balance. If a custom category is ever needed, reserve
> `19..=31` as `Class::Custom(u8)`; themes still index a flat array.

---

## 4. The scanner

A single left-to-right pass over one line's text, producing a `&mut [u8]` of `Class` per
*column* (aligned to the grid, wide chars handled like `layout_row` already does).

### 4.1 Two states

```
            ┌─────────────── prompt detected at col 0 ───────────────┐
            ▼                                                        │
     ┌─────────────┐    sign+space    ┌──────────────────┐           │
     │ PromptLine  │ ───────────────► │   CommandMode    │  EOL ─┐   │
     │ mark Prompt │  mark PromptSign │ 1st token=Cmd    │       │   │
     │ region bg   │                  │ then Option until │       ▼   │
     └─────────────┘                  │ EOL               │ ┌────────┐
                                      └──────────────────┘ │  done  │
                                            ▲              └────────┘
            no prompt at col 0 ─────────────┘                 ▲
            ▼                                                 │
     ┌──────────────┐  run flat matcher set (priority order)  │
     │ OutputMode   │ ───────────────────────────────────────►│
     └──────────────┘
```

**PromptLine state**: the prompt region (cols 0..sign_end) is tagged `PromptSign` for the
glyph and the whole line is tagged for `Prompt`-background (a *line-level* decoration,
stored separately — see §7). After the sign + one space, switch to `CommandMode`.

**CommandMode**: first non-space token → `Command`; subsequent `--x`/`-x`/`/x` tokens →
`Option`; `;`/`|`/`&&`/`||` reset to expect a new `Command`. End at EOL.

**OutputMode** (no prompt): run the flat matcher set in **priority order**, first match
wins per span (non-overlapping):

1. `String` (quoted) — a `begin/end` mini-state for `'…'`/`"…"`.
2. `Permission` — if the line matches an `ls -l` perms prefix (`[bcdlps-][r-][w-][xs-][xs-][xt-]`), tag the 10-char block; per-bit nuance deferred (§11).
3. Keyword sets (Error/Success/Warn/Info/Debug) — **one Aho-Corasick pass** over the whole line tags all keyword spans at once.
4. Structural regexes (IPv6, MAC, DateTime) + hand-written probes (IPv4, Path, Number) — run in order, skip columns already claimed.

> Priority + "skip claimed columns" replaces TextMate's recursive `include` ordering.
> Because terminal output is flat, this is sufficient and far cheaper.

### 4.2 The OSC 133 fast path (the OneTerm advantage)

OneTerm **already** parses OSC 133 in `crates/core/src/terminal/osc.rs` (`Osc133Kind`)
and forwards `SessionEvent::ShellIntegration(kind)`:
`PromptStart` / `PromptEnd` / `OutputStart` / `OutputEnd { exit_code }` (note: the code enum is `OutputStart`, i.e. OSC 133;C — the command-input region is `PromptEnd..OutputStart`; see §13 Q1).

Today these only update `prompt_count` and `last_exit_code` (`crates/local/src/state.rs`).
The proposal: **attach the markers to grid rows** so the renderer knows each row's role
*authoritatively* — no prompt regex needed for integrated shells:

```rust
// new, alongside the grid snapshot
pub struct RowRoles {
    /// Per display row: Prompt | Command | Output  (from OSC 133 boundaries)
    pub role: Box<[RowRole]>,
    /// Exit code of the command that produced each Output row (for failure tinting)
    pub exit_code: Box<[Option<i32>]>,
}
#[repr(u8)]
pub enum RowRole { Output = 0, Prompt = 1, Command = 2 }
```

When `RowRoles` is present (shell emits OSC 133):

- Rows with `RowRole::Prompt` → scanner starts in `PromptLine` state **and** the prompt
  sign region is known (between `PromptStart` and `PromptEnd` columns if we track them,
  else fall back to sign-glyph detection within the row).
- Rows with `RowRole::Command` → scanner starts in `CommandMode` directly.
- Rows with `RowRole::Output` → `OutputMode`.
- `OutputEnd { exit_code }` → tint the preceding command/prompt `PromptSign` `Success`
  (code 0) or `Error` (non-zero) — a reliable, regex-free success/failure cue that is
  only possible with shell-integration markers.

When `RowRoles` is absent (shell without integration — raw serial, router, bare `sh`),
fall back to the **`ShellProfile` prompt regex** to detect prompt lines. The scanner is
the same; only the row-role source differs.

---

## 5. Rule data (compiled once, not interpreted per line)

```rust
// crates/highlight/src/rules.rs
pub struct RuleSet {
    pub keywords: AhoCorasick,           // ~40 words → Class via a side table
    pub keyword_class: Vec<Class>,       // parallel to AhoCorasick patterns
    pub ipv6:   regex::Regex,
    pub mac:    regex::Regex,
    pub datetime: regex::Regex,
    // IPv4, Path, Number, PromptSign, Option, Operator, Bracket: hand-written probes
}

pub struct ShellProfile {
    /// Fallback prompt detector (used only without OSC 133).
    pub prompt: regex::Regex,            // e.g. r"^[^\n]*[$#%>❯➜λ]\s"
    pub path_sep: PathSep,               // '/' (unix) | '\\' or '/' (windows)
    pub option_prefix: fn(char) -> bool, // '-' (unix) | '-' or '/' (win cmd)
}
```

- **Keyword sets** are `&'static [&str]` compiled into one `aho_corasick::AhoCorasick`
  at startup (`OnceLock`). One pass per line tags every keyword span. Adding a word =
  editing a static array (data, not engine).
- **Structural regexes** are `regex::Regex` (DFA-compiled, no backtrack), built once.
- **Hand-written probes** (IPv4 octets, path scan, number lex, prompt-sign char, option
  prefix, operator/bracket char classes) are tiny `fn(&str, usize) -> Option<usize>` —
  faster and simpler than regex for these.

No JSON grammar is interpreted at runtime. The "vocabulary" (keywords, prompt regex per
shell) can optionally live in a small TOML loaded at startup for user tunability, but the
engine that consumes it is compiled Rust. v1 ships the vocabulary as Rust statics.

---

## 6. The `ShellProfile` enum

Only two things actually differ per shell: the **prompt pattern** and the **path/option
syntax**. Everything else is shared.

```rust
pub enum ShellProfile {
    Unix,       // bash/sh/zsh/fish, Linux/macOS/WSL local + SSH on Linux
    Cmd,        // cmd.exe — path: \ /, option: - /
    PowerShell, // pwsh — path: \ /, option: -
    Dumb,       // unknown / serial / router — most permissive prompt regex
}
```

Selected from session settings (shell kind). Unknown → `Dumb` (permissive prompt regex
matching `$`/`#`/`%`/`>`/`->`/powerline `U+E0B0`). One shared `RuleSet` + one
`ShellProfile` per view — not 6 duplicate grammars.

---

## 7. Theme mapping — flat array, pre-resolved

```rust
// crates/highlight/src/theme.rs
#[derive(Clone, Default)]
pub struct ClassStyles {
    /// Indexed by `Class as u8` — resolved at theme-build, never at render.
    pub fg:   [Option<gpui::Hsla>; Class::COUNT],
    pub bg:   [Option<gpui::Hsla>; Class::COUNT],
    pub font: [FontStyle; Class::COUNT],        // additive OR with cell flags
    pub deco: [Decoration; Class::COUNT],       // None | Underline | Box | LineBg(color)
    /// Line-level (not per-cell): prompt-line background.
    pub prompt_line_bg: Option<gpui::Hsla>,
}
```

Theme JSON gains an optional block (themes without it → all `None` → layer 2 is a no-op,
fully backwards compatible):

```jsonc
{
  "name": "Molokai",
  "terminal": {
    "semantic": {
      "promptLineBg": "#262626",
      "styles": {
        "promptSign": { "foreground": "#F92672" },
        "command":    { "foreground": "#66D9EF" },
        "option":     { "foreground": "#FD971F" },
        "error":      { "foreground": "#f44747" },
        "success":    { "foreground": "#A6E22E" },
        "warn":       { "foreground": "#D0A500" },
        "info":       { "foreground": "#6796e6" },
        "debug":      { "foreground": "#b267e6" },
        "path":       { "foreground": "#E6DB74" },
        "ip":         { "foreground": "#A6E22E" },
        "dateTime":   { "foreground": "#A6E22E" },
        "number":     { "foreground": "#AE81FF" },
        "string":     { "foreground": "#E6DB74" },
        "operator":   { "foreground": "#A6E22E" },
        "bracket":    { "foreground": "#A6E22E" },
        "url":        { "foreground": "#66D9EF", "decoration": "underline" }
      }
    }
  }
}
```

A shipped **default `semantic` block** (in `crates/ui/assets/highlight/default.json`) is
merged under any per-theme overrides, so every theme gets sane colors automatically —
only the ANSI palette + accents come from the gpui-component theme.

**Render-time cost**: `styles.fg[class as usize]` — one array index. No string scope, no
selector matching, no hashmap. This is the headline Rust win over the TextMate model.

---

## 8. Integration into OneTerm's render path

Current path (`crates/ui/src/views/terminal/`):

```
TerminalContent (grid)
  → layout_row() builds RowLayout { rects, runs, box_draws }
      • cell_colors(cell, &TerminalTheme) → (fg, bg) Hsla   ← THE CHOKEPOINT
      • batches adjacent same-style cells → BatchedTextRun
  → RowLayoutCache (per-frame, reused)
  → paint_terminal()
```

Changes:

1. **`cell_colors` gains an optional `class: Class`** (or a sibling `cell_colors_semantic`).
   After ANSI resolution, apply the merge policy (§9) using `ClassStyles`.

2. **`layout_row` gains `cell_class: &[u8]`** (same shape as the existing `url_mask:
   &[bool]`). The batching key becomes `(merged_fg, merged_bg, font_style, class)` —
   runs break on token boundaries automatically. `url_mask` is replaced by
   `cell_class` (with `Class::Url` one variant), generalizing the existing overlay.

3. **`RowLayoutCache` key gains `line_text_hash`** so unchanged lines skip both lex and
   layout. The scanner writes into a reused `Vec<u8>` scratch (like the existing
   `box_probe`).

4. **`TerminalTheme` gains `class_styles: ClassStyles`**, populated in
   `build_terminal_theme()` from the theme JSON's `terminal.semantic` block.

5. **A `SemanticOverlay` per view** holds `(ShellProfile, &'static RuleSet, RowRoles)`
   and produces `cell_class` for the visible viewport each frame.

6. **Prompt-line background**: paint under the whole prompt+command row (a `LayoutRect`
   with `prompt_line_bg`), inserted before per-cell backgrounds — orthogonal to per-cell
   fg, emitted as one rect.

```
snapshot → viewport rows
   for each visible row:
     role = row_roles[row] OR prompt_regex(line)         // §4.2
     cell_class = scan_line(line, rules, profile, role)   // single pass
     cache by line_text_hash
   layout_row(cells, theme, cell_class, …)
     per cell: (fg,bg) = cell_colors(cell, theme)
               merged  = merge(cell, fg, bg, cell_class[col], theme.class_styles)
               batch by (merged_fg, merged_bg, font, class)
   paint_terminal()
```

---

## 9. Merge policy

| Cell state | `Class` | Resulting fg | Resulting bg / decoration |
|---|---|---|---|
| default fg (no SGR) | non-Default | **class fg** | class bg/deco (if any) — *the headline case* |
| explicit ANSI fg | non-Default | **keep ANSI fg** | class bg/deco apply additively (prompt-line bg, error underline) |
| explicit ANSI fg | Default | ANSI fg | ANSI bg |
| default fg | Default | theme fg | theme bg |

Rules:

- **Prompt-line bg** always paints (line-level, not per-cell) — doesn't touch fg.
- **Decorations** (underline/box for `Error`/`Warn`/`Url`) are *additive* on top of ANSI
  fg — they never replace color.
- **`font` (bold/italic)** ORs with the cell's existing `Flags`, never removes.
- **Min-contrast** (`ensure_minimum_contrast`) re-runs on the *merged* fg/bg, treating
  the semantic fg as the new fg — keeps readability on any theme bg.
- Optional per-class `"override": true` forces class fg even over explicit ANSI (off by
  default) — for users who want maximum semantic coloring.

This respects programs that colorize themselves while delivering plain-text-log
colorization — overriding only the *default* foreground, never explicit SGR colors.

---

## 10. Performance budget

| Cost | Source | Estimate |
|---|---|---|
| Keyword pass | one `aho_corasick` find_iter over ≤200 chars | < 1 µs/line (SIMD) |
| Structural regexes | ~3 `regex` is_match over short strings | ~1-2 µs/line |
| Hand-written probes | char-class scans | < 0.5 µs/line |
| Per-cell theme lookup | `styles.fg[class as usize]` × cells | branchless, negligible |
| Cache hit | hash compare | skip lex+layout entirely |

Only **visible viewport** lines are lexed (≤~50/frame). With the hash cache, steady
output re-lexes only the newly-appended lines + the active input line. Target: zero
perceptual cost at 120 fps scroll (the existing
`terminal-rendering-optimization.md` bar). No C dependency, no backtracking (ReDoS-safe),
no per-line JSON interpretation, no string-scope hashing.

---

## 11. Phased rollout

| Phase | Scope | Deliverable |
|---|---|---|
| **0** | Spike | `crates/highlight`: `Class`, `RuleSet` (keywords via aho-corasick + IPv4/path/number probes), `scan_line` for `OutputMode` only. Hardcode one shell. Show error/warn/success/path/IP colored on `cat`/log output. Validate merge policy. |
| **1** | Core | `ShellProfile` + prompt detection (regex fallback). `ClassStyles` + theme JSON `terminal.semantic` + default asset. Merge into `cell_colors` / `layout_row` batching. Per-line hash cache. `terminal.semantic_highlighting: auto/on/off` setting. |
| **2** | OSC 133 fast path | Attach `RowRoles` (+ exit code) to the snapshot from `SessionEvent::ShellIntegration`. Scanner consumes roles; regex prompt detection becomes fallback. Success/failure tint on `PromptSign` from `OutputEnd{exit_code}`. |
| **3** | Shells & polish | `Cmd` / `PowerShell` profiles. Decorations (underline/box for error/warn/url). Per-theme overrides. Contrast-on-merged. `Permission` block (ls -l); optional per-bit coloring (`r`=info,`w`=warn,`x`=error). |
| **4** (opt-in) | Substrate | Prompt-block folding, command outline (scroll-to-prompt already implied by `prompt_count`), bracket/quote pair matching — all read the same `RowRoles`/`cell_class`. |
| **5** (opt-in) | Triggers | Generalize `url_mask`→`Class::Url` into a small configurable regex→action list (open URL, ping-IP menu, number tooltip). Shares the overlay plumbing, independent of the scanner. |

Phases 0–3 are the core proposal.

---

## 12. Crate layout

```
crates/highlight/                 # pure Rust, no GPUI
  src/
    class.rs          # enum Class (u8), Class::COUNT
    rules.rs          # RuleSet: AhoCorasick + regex::Regex + probe fns
    profile.rs        # ShellProfile (Unix/Cmd/PowerShell/Dumb)
    scanner.rs        # scan_line(line, rules, profile, role) -> &mut [u8]
    theme.rs          # ClassStyles { [Option<Hsla>; N], … }  (Hsla is a plain mirror struct)
    role.rs           # RowRole, RowRoles (from OSC 133)
  Cargo.toml          # aho-corasick, regex, (serde for optional TOML vocab)

crates/ui/
  src/views/terminal/
    highlight/
      mod.rs          # SemanticOverlay (holds profile, rules, row_roles)
      bridge.rs       # Class → gpui::Hsla; ClassStyles → TerminalTheme
    theme/mod.rs      # + class_styles: ClassStyles in TerminalTheme
    cell/color.rs     # cell_colors merges ClassStyles
    layout/row.rs     # + cell_class: &[u8]; batching key += class; url_mask → Class::Url
  assets/highlight/
    default.json      # default semantic style block
  themes/*.json       # + optional terminal.semantic override
```

`crates/highlight` depends only on `aho-corasick`, `regex`, and (optionally) `serde` for
a tunable keyword TOML. It does **not** depend on GPUI — `theme.rs` uses a plain
`Rgba`/`Hsla` mirror so the pure layering holds; `ui/bridge.rs` converts to
`gpui::Hsla`.

---

## 13. Open questions — resolved

Each question below is stated, then given a **Decision**, **Rationale** (grounded in the
actual OneTerm code), and **Implementation** (concrete symbols). These are the spec the
implementer should follow; they are no longer open.

### Q1. OSC 133 granularity — row roles, not column boundaries

**Question.** OneTerm already parses OSC 133 (`crates/core/src/terminal/osc.rs`) into
`Osc133Kind { PromptStart, PromptEnd, OutputStart, OutputEnd { exit_code } }` and
forwards it via `SessionEvent::ShellIntegration` (`crates/local/src/listener.rs`). These
are **row-level** events — they arrive *between* rendered grid rows, with no column
index. Do we need column-level prompt/sign boundaries, or is row-level role + an in-row
sign-glyph probe enough?

**Decision.** **Row-level roles only.** Attach a `RowRole` to each display row from the
OSC 133 stream; detect the prompt *sign glyph* with a tiny in-row probe (not a column
boundary from OSC 133). Keep `RowRoles` as a cheap `Box<[u8]>`.

**Rationale.**
- OSC 133 carries no column data — it is emitted by the shell *before/after* it prints a
  region, so the natural unit is the grid row. Recovering column boundaries would require
  correlating the event with the cursor column at event time, which alacritty's
  `Event::Osc` does not expose reliably.
- The only column-level thing we need is *where the sign glyph is* (for
  `Class::PromptSign`). That is a single char-class probe (`$ # > ❯ ➜ λ` or powerline
  `U+E0B0`) on the prompt row — cheap and exact enough.
- Row roles give the high-value wins for free: `OutputEnd { exit_code }` tints the
  preceding prompt/command `Success` (code 0) or `Error` (non-zero) — a regex-free
  success/failure cue unavailable without shell-integration markers.

**Implementation.**
```rust
// crates/highlight/src/role.rs
#[repr(u8)] #[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RowRole { #[default] Output = 0, Prompt = 1, Command = 2 }

pub struct RowRoles {
    pub role:      Box<[u8]>,           // per display row -> RowRole
    pub exit_code: Box<[Option<i32>]>,  // exit code of the command owning each Output row
}
```
Map the OSC 133 stream to roles by tracking the *current* region as rows are appended:

| Event | Effect on subsequent rows |
|---|---|
| `PromptStart` (A) | next rows -> `RowRole::Prompt` until `PromptEnd` |
| `PromptEnd` (B) | next rows -> `RowRole::Command` until `OutputStart` (B doubles as command-input-start) |
| `OutputStart` (C) | next rows -> `RowRole::Output` until `OutputEnd` |
| `OutputEnd { exit_code }` (D) | record `exit_code` onto the just-finished `Output` rows; tint the preceding `Prompt`/`Command` rows' `PromptSign` with `Success`/`Error` |

`RowRoles` is rebuilt in the pump (where `SessionEvent::ShellIntegration` is already
handled in `listener.rs`) alongside the grid snapshot, then read by the scanner. When
`RowRoles` is absent (no shell integration), the scanner falls back to the
`ShellProfile` prompt regex to derive a best-effort `RowRole::Prompt` for the row.

> **Correction to §4.2**: the code enum is `OutputStart` (OSC 133;C), not `CommandStart`.
> The command-input region is `PromptEnd..OutputStart`; `OutputStart` begins the command
> *output*. The table above is authoritative.

### Q2. Keyword vocabulary — Rust statics for v1

**Question.** Ship the keyword sets (error/warn/success/info/debug words) as `&'static
[&str]` compiled into the binary, or as a `keywords.toml` loaded at startup so users
can tune which words map to `Error`?

**Decision.** **Rust statics for v1.** Add a tunable TOML only if a user asks.

**Rationale.**
- The vocabulary is small and stable (~40 words across 5 classes). It is the *engine*
  that is valuable, not per-user word lists.
- Statics compile straight into one `aho_corasick::AhoCorasick` via `OnceLock` — zero
  runtime parse cost, no file path / error handling, no settings wiring.
- A TOML layer adds: a file format, a loader, a merge policy with the built-ins, a
  settings UI, and tests — all for a feature nobody has requested yet.

**Implementation.**
```rust
// crates/highlight/src/rules.rs
pub const ERROR_WORDS:   &[&str] = &["bad","cannot","denied","deprecated","disabled",
    "error","errors","fail","failed","failure","false","important","incorrect",
    "invalid","no","none","not","refused","unknown","unsupported","warning","wrong"];
pub const SUCCESS_WORDS: &[&str] = &["can","correct","correctly","known","ok","pass",
    "passed","success","successful","successfully","supported","true","valid","yes"];
pub const WARN_WORDS:    &[&str] = &["closed","debug","disconnected","exited","skipped",
    "stopped","sudo","terminated","warning","warn"];
pub const INFO_WORDS:    &[&str] = &["access","any","authentication","connection",
    "disconnection","info","login","operation","password","permission"];
// Build ONE AhoCorasick over all patterns + a parallel Vec<Class> mapping pattern -> Class.
```
If TOML is added later, it only rewrites the `&'static` tables into
`OnceLock<Vec<&'static str>>` loaded from disk — the scanner and theme mapping are
unchanged. The escape hatch is cheap, so deferring it costs nothing.

### Q3. `Class::Url` — reuse the existing unified URL mask, do not reimplement

**Question.** OSC 8 hyperlinks are Layer-1 (program-emitted, in `cell.hyperlink()`).
Plain-text URLs are Layer-2 (regex). Should the scanner re-detect URLs, or feed off the
existing detection?

**Decision.** **Do not re-detect.** Feed `Class::Url` from the existing
`url_masks_wrapped` output (`crates/ui/src/views/terminal/layout/cache.rs` ->
`url/detect.rs`), which *already* unifies OSC 8 (`cell.hyperlink()`) and plain-text URL
regex into one per-line boolean mask. Reinterpret that mask as `Class::Url`.

**Rationale.**
- `url/detect.rs::detect_url_at` already handles both sources (step 1: OSC 8 by
  `hyperlink()` ID; step 2: plain-text URL regex). `url_masks_wrapped` precomputes the
  per-line mask for the whole grid.
- Reimplementing URL detection in the scanner would duplicate regex, double the per-line
  cost, and risk the two detectors disagreeing on click-vs-render boundaries.
- `Class::Url` is just the boolean mask reinterpreted as one `Class` variant. The merge
  is `if url_mask[col] { cell_class[col] = Class::Url; }` — one pass, after the scanner,
  overriding any `Default` the scanner left (URL wins over plain text).

**Implementation.** In `layout/cache.rs`, where `url_masks = url_masks_wrapped(...)` is
already computed, pass it into the semantic step *alongside* `cell_class`. The scanner
fills `cell_class` for non-URL classes; then `url_mask` sits on top:
```rust
for col in 0..num_cols {
    if url_mask[col] { cell_class[col] = Class::Url; }  // URL is authoritative
}
```
This keeps `handlers/mouse.rs` (Ctrl+click -> open URL) and `handlers/url.rs` (hover)
working unchanged — they already read the same detection. The existing `url_mask:`
parameter of `layout_row` becomes `cell_class: &[u8]` (a strict generalization: the old
`mask[col] == true` is now `cell_class[col] == Class::Url`).

### Q4. Wide-char / CJK column alignment — iterate cells, not chars

**Question.** `scan_line` produces `Class` per *char index*, but `layout_row` and the
renderer work per *grid column*, and wide chars occupy 2 columns + a `WIDE_CHAR_SPACER`
that is skipped, plus zerowidth chars appended to the preceding cell. How do the two
align?

**Decision.** **The scanner emits a `char -> Class` map; a cell-driven flatten step
(inside the same loop `layout_row` already runs) maps each cell to its source char's
class.** Wide char -> both columns get the class; spacer -> skipped (as today);
zerowidth -> inherits the preceding cell's class.

**Rationale.**
- `layout_row` (`layout/row.rs`) already iterates cells in display order and already
  handles `WIDE_CHAR_SPACER` (skip) and `zerowidth()` (append). Reusing that iteration
  order for the flatten guarantees alignment by construction — no separate index math.
- Doing the flatten in char-space then translating to column-space separately would
  duplicate the wide-char/spacer logic and drift.

**Implementation.**
```rust
// In layout_row: `char_class: &[u8]` is the scanner output, indexed by char position
// in the line string. Build `cell_class: Vec<u8>` of length = number of *columns*.
let mut char_idx = 0;
for ic in line_cells {
    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) { continue; }      // no column
    let cls = char_class[char_idx.min(char_class.len() - 1)];
    cell_class.push(cls);                                               // primary column
    if is_wide(cell.c) { cell_class.push(cls); }                        // 2nd column = same class
    char_idx += 1;
    // zerowidth chars: do NOT advance a column; they inherit the preceding cell's
    // class (the run they are appended to), so no cell_class entry is pushed for them.
}
```
The line string fed to the scanner is built by the *same* cell iteration (skip spacer,
emit one `char` per non-spacer cell, append zerowidth), so `char_idx` and the scanner's
char index stay in lockstep. This is the single source of truth for char<->column mapping.

### Q5. Re-lex cost & cold scroll — ride the existing cache, viewport-only

**Question.** Re-lexing on every scroll could be expensive; a 100k-line jump into
un-lexed scrollback needs throttling. Do we need a separate semantic cache?

**Decision.** **No separate cache.** The semantic scan rides the *existing* per-line
dirty decision in `RowLayoutCache` (`layout/cache.rs`), which already hashes each line
with `line_hash` and only re-lays-out dirty/damaged lines. The scanner runs only for
lines that are already going to be re-laid-out, and only within the visible viewport.
Cold scroll into un-lexed scrollback is allowed to be one frame behind (progressive).

**Rationale.**
- `RowLayoutCache` already maintains `prev_hash` per display line and a damage set
  (`TermDamageInfo::Partial`), re-computing `layout_row` only for dirty lines
  (`cache.rs:102-136`). Adding a second, parallel cache would duplicate this exact
  invalidation logic.
- The semantic scan is cheap (§10) and is only invoked for lines already marked dirty
  -> it adds no new invalidation surface, just one extra step inside the existing dirty
  branch.
- Viewport-only is the correct contract: only the visible screen is guaranteed
  highlighted. Scrollback highlights lazily as it scrolls into view.

**Implementation.**
- Inside the existing `if is_dirty { ... }` block in `cache.rs`, *before* calling
  `layout_row`, run `scan_line` -> `char_class`, flatten to `cell_class` (Q4), apply
  `url_mask` -> `Class::Url` (Q3), then pass `cell_class` into `layout_row`. The scan
  result is not cached separately — it is consumed immediately and the line's
  `prev_hash` already gates re-computation.
- For a cold 100k-line scroll: only the ~50 visible rows are scanned this frame; rows
  scrolled into view next frame are scanned then. Worst case a row is unhighlighted for
  one frame as it enters the viewport — imperceptible. No background task, no
  throttling heuristic needed.
- The active input row (cursor line) is already special-cased in the cache
  (`cursor_display_line` re-hash) — the semantic scan naturally follows, so the prompt
  being typed re-highlights live.

### Q6. Custom-class escape hatch — reserve the range now

**Question.** `Class` is a closed enum. What if a future feature needs a user-defined
category (e.g. a trigger that paints a custom color)?

**Decision.** **Reserve `Class 19..=31` now, unused.** Themes still index a flat
`[Style; N]`; `N` is fixed at build time.

**Rationale.**
- Reserving 13 slots costs nothing (the array is `Class::COUNT = 32` instead of 19 —
  32 bytes/theme, trivial) and preserves the enum ABI + flat-array theme layout if a
  custom category is ever added.
- 13 reserved slots is more than enough for any realistic extension (triggers,
  user-defined keyword classes). If ever exhausted, `Class 32..` would require a theme
  format bump — far in the future.
- This keeps the headline Rust win intact: theme lookup stays `styles[class as usize]`,
  one array index, even for custom classes.

**Implementation.**
```rust
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Class {
    Default = 0, PromptSign = 1, /* ... 2..=18 ... */ Permission = 18,
    // 19..=31 reserved for future user-defined categories. Themes may set styles for
    // these indices; the engine never emits them until a feature uses them.
    // A `Custom(u8)` variant constructor would be added if/when needed.
}
impl Class { pub const COUNT: usize = 32; }   // fixed; flat array stays O(1)
```
Do **not** add `Custom(u8)` as a real variant yet — just reserve the numeric range via
`COUNT = 32` and leave the variants unconstructed. A future feature adds the variant and
begins emitting it; themes that don't define it get `None` (no-op), same as any
unstyled class.

---

## Appendix — scanner pseudocode (`OutputMode`)

```
fn scan_output(line, rules, classes: &mut [u8]):
    # 1. strings (mini begin/end)
    quote_scan(line, classes)                       # '…' / "…" → String
    # 2. permission block (ls -l prefix)
    if line starts with perms_prefix: tag cols 0..10 → Permission; return
    # 3. keywords — one Aho-Corasick pass
    for m in rules.keywords.find_iter(line):
        if classes[m.col].is_default():
            classes[m.col..m.end] = rules.keyword_class[m.pattern]
    # 4. structural (priority order, skip claimed)
    for (regex, cls) in [(rules.ipv6,Ip),(rules.mac,Mac),(rules.datetime,DateTime)]:
        for m in regex.find_iter(line):
            if all_default(classes, m.col..m.end): classes[m.col..m.end] = cls
    # 5. hand-written probes
    ipv4_probe(line, classes); path_probe(line, classes, profile.path_sep)
    number_probe(line, classes)
    # 6. single-char classes (last, only on default cells)
    for (i,c) in line.chars():
        if classes[i].is_default():
            classes[i] = match c { operator → Operator, bracket → Bracket, _ → Default }
```

One pass + a few small regexes + char-class sweep. No recursion, no JSON, no string
scopes, no C.