# 01 — Architecture

> Part of [Terminal auto-completion design](../auto-completion.md). Crate layout,
> dependency-rule compliance, end-to-end data flow, and the key public types.

## 1. Crate layout

Auto-completion splits into a **gpui-free engine + data layer** (unit-testable,
no UI) and a **thin overlay** that lives in the existing terminal feature crate.
This keeps the matching/ranking/redaction logic testable in isolation and obeys
the workspace layering (see [`docs/agents/crate-dependency-rules.md`](../agents/crate-dependency-rules.md)).

| Crate | Status | Layer | Responsibility |
|---|---|---|---|
| `completion` (`oneterm-completion`) | **new** | engine (shared) | The suggestion engine **and** the embedded `external` catalogs: data model (`Suggestion`, `SuggestionKind`, `ShellFamily`), catalog loading/merging, input-line parsing, matching + ranking, the in-session `CompletionHistory` store, and sensitive-value redaction. Ships the generated Windows Commands + Unix coreutils JSON under `crates/completion/assets/`, embedded via `include_str!`. gpui-free, alacritty-free — depends only on `core` (for `ShellKind`). |
| `state` (`oneterm-state`) | existing | shared | Owns the **process-global** `CompletionHistory` entity (shared across all Terminal Tabs, non-persistent) — mirrors the existing `AgentRegistry` global pattern. Provides `init()`. |
| `settings` (`oneterm-settings`) | existing | shared | Adds the `CompletionConfig` group to `terminal.json` + the live mirror in `TerminalSettings`. See [06](06-configuration.md). |
| `terminal-view` (`oneterm-terminal-view`) | existing | feature | Hosts the **overlay view**, wires keystrokes/cursor/OSC-133 into the engine, captures accepted commands into history, and renders results. Gains a dependency on `completion`. |
| `settings-ui` (`oneterm-settings-ui`) | existing | feature | Adds the completion controls to the Terminal settings page. |

> **Why embed the catalogs in `completion` (no separate data crate)?** The
> generated JSON is small, read-only, and only ever consumed by the engine.
> Embedding it in the engine crate via `include_str!` (the same pattern
> `terminal-view` uses for `assets/highlight/default.json`) keeps the crate count
> down and needs no runtime asset files. The [generator script](07-external-assets-script.md)
> writes the JSON into `crates/completion/assets/`.

> **Why not a `completion-view` feature crate?** The overlay is inseparable from
> the terminal's cursor bounds, keyboard handler, alternate-screen state, and
> OSC 133 row roles — all internal to `terminal-view`. A separate feature crate
> would need `terminal-view` internals, which the rules forbid (no feature↔feature
> edge except `session-ui → terminal-view`). Keeping the overlay **inside**
> `terminal-view` and all reusable logic in the gpui-free `completion` engine is
> the correct split.

## 2. Dependency-rule (R1–R12) compliance

New/changed edges, all pointing **downward** (DAG preserved):

```
completion  ── depends on ─▶ core (ShellKind)   [external catalogs embedded in-crate]
        ▲
        ├───────────── state          (holds the global CompletionHistory)
        └───────────── terminal-view  (overlay + wiring)
settings  ── gains CompletionConfig (no new crate dep)
```

- **R1 (no cycles):** `completion` sits below `state` and `terminal-view`; it
  depends only on `core`. No cycle.
- **R2/R5 (features depend on shared layers only):** `terminal-view` adds a
  dependency on the shared engine crate `completion`, not on another feature.
- **R3 (no UI→backend edge):** `completion` never touches `ssh`/`local-shell`;
  shell identity arrives as a plain `ShellKind` value from `core`.
- **R6/R7 (`core`/`terminal` stay gpui/alacritty-free):** unaffected — the engine
  is a new crate; `core` only exports the already-existing `ShellKind`.
- **R10 (new shared type in the lowest crate that needs it):** `ShellFamily` maps
  from `core::ShellKind`; it lives in `completion` (the lowest crate that needs
  completion-specific grouping), not in `core`.
- After adding the crate, re-run the doc's full-graph `cargo tree` verification
  and `python scripts/verify-dependency-graph.py`.

## 3. End-to-end data flow

```
                      ┌──────────────────────── terminal-view ───────────────────────┐
 keystrokes ─┬─▶ PTY (unchanged: echoed to the shell as normal)                       │
             │                                                                        │
             └─▶ CompletionController                                                  │
                   │  • tracks the current input line + token under cursor            │
                   │  • reads gating signals (alt-screen, OSC133 row role)            │
                   │  • asks the engine for suggestions                               │
                   ▼                                                                  │
             oneterm_completion::Engine::suggest(ctx) ──────────────────────────────┐ │
                   │      ctx = { shell_family, line, cursor_col, option_context }   │ │
                   │  merges: history ⊕ manual catalog ⊕ external catalog            │ │
                   │  matches token, redacts, ranks, dedups                          │ │
                   ▼                                                                  │ │
             Vec<Suggestion>  ──▶  CompletionOverlay (RenderOnce list, cursor-anchored)│
                                                                                       │
   on accept ─▶ compute remainder ─▶ write remainder to PTY ─▶ dismiss                 │
   on OSC133 D (command finished) ─▶ capture the just-run command ─▶ redact ─▶         │
                                     CompletionHistory (global, cross-tab)             │
                      └───────────────────────────────────────────────────────────────┘
```

Command capture (feeding the `memory` source) reuses the **OSC 133 row-role**
machinery already used by semantic highlighting: the text between `PromptEnd`
(`B`) and `OutputStart`/`OutputEnd` is the command line the user ran. See
[02](02-data-sources.md) §2 for the capture path and its fallback when the shell
emits no OSC 133.

## 4. The global history store (cross-tab, non-persistent)

`memory` suggestions must be **shared across every Terminal Tab** but **reset when
OneTerm exits**. This is exactly the lifetime of a process-global GPUI entity, so
`CompletionHistory` is registered as a global in `state` alongside `AgentRegistry`:

```rust
// crates/completion/src/history.rs  (gpui-free data + logic)
pub struct CompletionHistory {
    // One ring buffer per shell family so a bash tab never surfaces cmd history.
    per_family: HashMap<ShellFamily, CommandRing>,
    capacity: usize,           // from CompletionConfig.max_history
}

impl CompletionHistory {
    pub fn record(&mut self, family: ShellFamily, redacted_line: &str);
    pub fn matches(&self, family: ShellFamily, token: &str) -> impl Iterator<Item = HistoryHit>;
    pub fn set_capacity(&mut self, n: usize);
    pub fn clear(&mut self);   // used by "clear history" action + on config change
}
```

```rust
// crates/state/src/completion_history.rs  (the global wrapper)
pub struct GlobalCompletionHistory(pub Entity<CompletionHistory>);
impl Global for GlobalCompletionHistory {}
// state::init() (called from app::init) creates it once.
```

- **Not persisted:** nothing writes it to `config_dir()`. It lives only in RAM and
  dies with the process — satisfying the non-persistence requirement without any
  extra work (contrast [`docs/agents/persistence.md`](../agents/persistence.md),
  which governs the files we *do* persist).
- **Cross-tab:** every `TerminalPanel`/Space reads and writes the same entity, so
  a command typed in tab A is instantly suggestable in tab B.
- **Redacted at write time:** `record` is only ever called with an
  already-redacted line (see [08](08-security-redaction.md)), so secrets never
  enter RAM history in the first place.

## 5. Key public types (engine crate)

```rust
// crates/completion/src/lib.rs

/// The completion-relevant class of the running shell (see 03-shell-detection.md).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellFamily { Cmd, PowerShell, Unix }

/// The kind of a suggestion → drives its tag badge + color.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind { History, Command, Option }

impl SuggestionKind {
    pub fn tag(self) -> char { match self { Self::History => 'H', Self::Command => 'C', Self::Option => 'O' } }
}

/// One candidate returned to the UI.
pub struct Suggestion {
    pub text: String,          // full text to display / the completion target
    pub kind: SuggestionKind,
    pub match_start: usize,     // byte offset where the matched prefix begins
    pub match_len: usize,       // matched-prefix length (for highlight)
    pub score: f32,             // ranking score (higher = better)
}

/// The query the UI hands to the engine each keystroke.
pub struct CompletionContext<'a> {
    pub family: ShellFamily,
    pub line: &'a str,          // the full input line (prompt-relative)
    pub cursor_col: usize,      // byte offset of the cursor within `line`
}

pub struct Engine { /* lazily-parsed manual + external catalogs from the embedded index */ }

impl Engine {
    /// Build the engine from the compile-time embedded catalog index
    /// (`external/` + `manual/`); no filesystem access.
    pub fn from_embedded() -> Self;
    /// Produce a ranked, deduped, redaction-safe suggestion list.
    pub fn suggest(&self, history: &CompletionHistory, ctx: &CompletionContext, cfg: &CompletionParams) -> Vec<Suggestion>;
}
```

`CompletionParams` is the engine-side view of the user config (max visible items,
which kinds are enabled, min prefix length) — a plain struct the `terminal-view`
layer fills from `TerminalSettings` so the engine never depends on `settings`.

## 6. Threading & performance

- `suggest()` runs on the UI thread but is cheap: it operates on already-loaded
  in-memory catalogs and a bounded history ring; matching is a prefix/fuzzy scan
  over at most a few thousand entries. Target < 1 ms for typical catalogs.
- Catalogs are **loaded once** (external embedded in the `completion` crate and
  parsed at startup, manual on first use / on file change) and cached in the `Engine`.
- If a catalog grows large enough to matter, matching can move to a background
  task via `cx.background_spawn` and post results back — but Phase 1 keeps it
  synchronous for simplicity (see [09](09-roadmap-risks.md) risks).
- Debounce: recompute at most once per input change; coalesce rapid keystrokes on
  the next frame.
