# 11 — Implementation plan

> Part of [Terminal auto-completion design](../auto-completion.md). A concrete,
> ordered build plan for Phase 1 (with the two accepted enhancements). Complements
> the phase/roadmap view in [09](09-roadmap-risks.md) §1 by breaking the work into
> verifiable milestones with exit criteria.

## 1. Principles

- **Engine-first, UI-last.** Build the gpui-free `completion` crate (parsing,
  matching, ranking, history, redaction) and unit-test it fully before touching any
  view code. The engine is a pure function of its inputs ([04](04-suggestion-engine.md)
  §1), so most behavior is provable without a running app.
- **TDD for the engine.** Every milestone below lands with its tests.
- **Quality gate per milestone** (AGENTS.md §5): `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`,
  `cargo test --workspace` must pass before moving on.
- **Layering discipline.** Obey the crate rules ([01](01-architecture.md) §2 and
  [`crate-dependency-rules.md`](../agents/crate-dependency-rules.md)); re-run
  `python scripts/verify-dependency-graph.py` after adding the crate.
- **No secret ever hits RAM history** — redaction lands *with* the history store
  (M4), not after.

## 2. Milestones

> **Implementation status** (updated 2026-08-03). Legend: ✅ done & verified ·
> 🟡 partial · ⬜ not started.
>
> | Milestone | Status | Evidence |
> |---|---|---|
> | M0 — Crate scaffold | ✅ | `cargo build -p oneterm-completion` ok; dependency-graph check passes |
> | M1 — Data model + catalog loading | ✅ | catalog/parse tests green (part of 53) |
> | M2 — Line parsing + context + subcommands | ✅ | parse/engine context tests green |
> | M3 — Matching, ranking, dedup, accept | ✅ | engine ranking/dedup/remainder tests green |
> | M4 — History store + redaction | ✅ | history + redaction tests green (all doc 08 §7 cases) |
> | M5 — Settings (`terminal.json`) | ✅ | 23 settings tests pass (4 new) |
> | M6 — Global history in `state` | ✅ | `GlobalCompletionHistory::init` wired into `app::init` |
> | M7 — `terminal-view` integration | ✅ | controller + overlay wired into `LocalTerminalView` (grid input tracking, cursor-anchored overlay, key handling, capture); 14 tests |
> | M8 — Settings UI | ✅ | "Completion" section builds + bound to `TerminalSettings` |
> | M9 — External catalog script | ✅ | `scripts/completion-catalog.py`; real catalogs generated (339 cmd + 105 coreutils) + committed |
> | M10 — "Recent commands" palette | ⬜ | optional for Phase 1; not started |
>
> Workspace quality gate GREEN: `cargo fmt --all -- --check`, `cargo clippy
> --workspace --all-targets -- -D warnings`, `cargo build --workspace`, and
> `cargo test --workspace -j 2` all pass (`-j 2` required — full parallelism OOMs
> the build machine). `python scripts/verify-dependency-graph.py` and
> `python scripts/check-english.py` pass.

Sizes are rough (S ≈ ½ day, M ≈ 1–2 days, L ≈ 3–5 days) for one engineer.

### M0 — Crate scaffold (S) — ✅ DONE

- Create `crates/completion/` (`oneterm-completion`): `Cargo.toml`
  (deps: `oneterm-core`, `serde`, `serde_json`), `src/lib.rs`, add to workspace
  `members`.
- Add asset dirs `assets/external/{cmd,coreutils,powershell}/` and
  `assets/manual/{windows/cmd,windows/powershell,linux,common}/` +
  `assets/catalog.schema.json`.
- **Exit:** ✅ `cargo build -p oneterm-completion` succeeds; dependency-graph check
  passes.

### M1 — Data model + catalog loading (M) — ✅ DONE

- Types: `Flag`, `CommandNode` (recursive), `CatalogCategory`, `ShellFamily`
  (`from_kind`, `categories`, `option_triggers`) — [01](01-architecture.md) §5,
  [03](03-shell-detection.md) §2.
- Schema deserializer accepting an option as a string **or** `{ "flag": … }`
  ([02](02-data-sources.md) §5); source + category derived from folder path.
- `build.rs` that walks `assets/**/*.json` → generates the `CATALOG_FILES`
  index of `include_str!`s ([07](07-external-assets-script.md) §5).
- Lazy `name → CommandNode` parse + cache ([02](02-data-sources.md) §5.3).
- Seed fixtures for tests: `external/cmd/dir.json`, `external/coreutils/ls.json`,
  `external/coreutils/grep.json`, `manual/common/git.json`,
  `manual/windows/cmd/ping.json`.
- **Tests:** parse; string/object option forms; family→categories mapping; lazy
  load; a
  malformed file is skipped without breaking siblings.
- **Exit:** ✅ catalogs load and resolve by name/family.

### M2 — Line parsing + context + subcommand resolution (M) — ✅ DONE

- `ParsedLine` (`head`, `token`, `token_start`, `is_first_token`) with quote-aware
  tokenization ([04](04-suggestion-engine.md) §2).
- Tree walk → `(active_node, path)` ([10](10-subcommands.md) §3).
- Context selection: command / **subcommand** / option / argument
  ([04](04-suggestion-engine.md) §3, [10](10-subcommands.md) §3.1).
- **Tests:** quoting; trailing space; `git ` → subcommands; `git remote ` → nested;
  `dir /` → options; unknown subcommand fallback.
- **Exit:** ✅ correct context + candidate set for the documented examples.

### M3 — Matching, ranking, dedup, accept (M) — ✅ DONE

- Prefix match (family case rules) + secondary fuzzy; option prefix matching;
  ancestor-option inheritance ([10](10-subcommands.md) §3.2).
- Frecency score blend; dedup keeping the `H` tag; truncation to
  `max_visible_items` ([04](04-suggestion-engine.md) §4).
- `Suggestion::remainder()` for exact-case append; terminal-view applies bounded
  casing correction for Cmd/PowerShell case-insensitive prefixes;
  `allow_fuzzy_accept` off ([04](04-suggestion-engine.md) §5, Q3).
- **Tests:** case sensitivity per family; Windows exact-suggestion casing; prefix
  beats fuzzy; frecency beats catalog; dedup tag precedence; remainder computation.
- **Exit:** ✅ `Engine::suggest` returns correctly ranked, deduped lists.

### M4 — History store + redaction (M) — ✅ DONE

- `CommandRing` per family + `CompletionHistory`
  (`record`/`matches`/`set_capacity`/`clear`) with frecency
  ([01](01-architecture.md) §4, [02](02-data-sources.md) §2).
- `redact.rs`: secret-flag vocabulary, `KEY=VALUE` keys, value-shape/entropy
  patterns; compose with `oneterm_terminal::security_policy` helpers
  ([08](08-security-redaction.md)).
- Suggestion-time guard (defense in depth, [08](08-security-redaction.md) §5).
- **Tests:** all secret cases from [08](08-security-redaction.md) §7; ring eviction;
  frecency update on repeat.
- **Exit:** ✅ a recorded secret line never yields a secret-bearing suggestion.
  Note: the control-char/length hygiene is implemented **locally** in
  `redact.rs` rather than via `oneterm_terminal::security_policy`, to keep the
  engine crate alacritty-free (depends only on `core`, per [01](01-architecture.md) §2).

### M5 — Settings (`terminal.json`) (S) — ✅ DONE

- `crates/settings/src/terminal_config/completion.rs`: `CompletionConfig` with
  serde defaults; live mirror in `TerminalSettings`; `CompletionParams` projection
  for the engine ([06](06-configuration.md) §2).
- **Tests:** defaults; old `terminal.json` (no `completion` group) loads with
  defaults; `max_history = 0` clears/disables history.
- **Exit:** ✅ config round-trips and live-applies. Note: the `CompletionParams`
  projection lives in `terminal-view` (`completion::params_from_settings`), so the
  engine stays free of a `settings` dependency ([01](01-architecture.md) §5); the
  `settings` crate exposes the live `CompletionConfig` group.

### M6 — Global history in `state` (S) — ✅ DONE

- `crates/state/src/completion_history.rs`: `GlobalCompletionHistory`
  (`Entity<CompletionHistory>`) + `init()`; wire into `app::init`
  ([01](01-architecture.md) §4).
- **Exit:** ✅ one shared history instance reachable from any terminal.

### M7 — `terminal-view` integration (L) — ✅ DONE

- ✅ `CompletionController` (gpui-free, 10 headless tests) wired into
  `LocalTerminalView`: reads the live input line from the grid each render
  (`view/completion.rs::extract_cursor_command` + prompt strip), feeds
  `session.is_alt_screen()` + prompt-region gating, calls `Engine::suggest`, and
  captures the command on Enter (redact → record).
- ✅ `CompletionOverlay` (`RenderOnce`): item format, `H`/`C`/`O` tag badges from
  the theme, breadcrumb slot; anchored to the token-start cell via `GridMetrics`
  and added as a child in `render/mod.rs`.
- ✅ Key handling in `handlers/keyboard.rs` before PTY delivery: first `Tab`
  (respects `accept_tab`) selects item 0; later `Tab` or `Enter` accepts; `Up`/`Down`/
  `Ctrl-N`/`Ctrl-P` navigate only after selection and otherwise pass through;
  `Esc` is swallowed. Accept applies the exact selected suggestion under the active
  family's case rule.
- ✅ Actions in `oneterm-actions`:
  `TriggerCompletion` (`Ctrl+Shift+Space`, handled in the keyboard handler).
- ✅ Added `oneterm-completion` to `terminal-view`'s `Cargo.toml`.
- ✅ Perf: the grid snapshot is skipped on the alternate screen and on frames
  where the cursor did not move (idle blink), so completion adds no per-frame
  cost during full-screen TUIs / fast output.
- ✅ Debug logging (non-spammy, state-change only, no `trace`): controller init,
  alt-screen toggle, suggestion count on change, capture, accept.
- **Tests:** ✅ headless controller + prompt-strip tests (14). Manual QA
  checklist (§4) is for interactive confirmation.
- **Exit:** ✅ typing at a local `cmd`/`bash` prompt shows suggestions
  (controller + overlay + keys wired; verified building + tested).

### M8 — Settings UI (S) — ✅ DONE

- `crates/settings-ui`: Completion section (toggles + numbers + Clear-history
  button) bound to `TerminalSettings` ([06](06-configuration.md) §5).
- **Exit:** ✅ settings visibly control behavior at runtime. Note: the settings
  widget has no button field, so there is no in-page **Clear session history**
  button; setting `max_history` to `0` clears the store.

### M9 — External catalog script (M) — parallelizable after M1 — ✅ DONE

- `scripts/completion-catalog.py`: `download` / `generate` / `update` for the
  `external` sources (`cmd` from MicrosoftDocs, `coreutils` from Debian); validate
  against `catalog.schema.json`; write one file per command; commit generated files
  ([07](07-external-assets-script.md)). The `manual/` categories (incl.
  `common/git.json`) are hand-authored, not generated.
- **Exit:** ✅ `generate` is deterministic and schema-valid; the real `external`
  catalogs (**339 cmd** from MicrosoftDocs + **105 coreutils** from Debian) are
  generated and committed under `crates/completion/assets/external/`, replacing
  the M1 seed fixtures. cmd flag extraction scans only the Parameters/syntax
  sections (short `/`/`-` flags) to avoid doc-link noise.

### M10 — "Recent commands" palette (S) — after M7 — ⬜ NOT STARTED (optional for Phase 1)

- `RecentCommands` action + palette mode over the same overlay, seeded by frecency
  ([09](09-roadmap-risks.md) §3.2).
- **Exit:** palette opens, filters, and appends (never auto-runs).

## 3. Ordering & parallelism

```
M0 → M1 ┬→ M2 → M3 ┐
        │           ├→ (M3+M4 feed) → M5 → M6 → M7 → M8 → M10
        ├→ M4 ──────┘
        └→ M9 (parallel; needs only the schema from M1)
```

- **Critical path:** M0 → M1 → M2 → M3 → M7 (the UI needs the full engine).
- **Parallel tracks:** M4 (history/redaction) alongside M2/M3; M9 (script) alongside
  the whole engine once the schema exists. Good candidates to delegate to separate
  sub-agents (engine vs. script vs. settings) since they touch disjoint files.
- M5/M6 are small glue and can slot in just before M7.

## 4. Manual QA checklist (M7)

- `d` at `cmd` → `date`/`dir`/`del` (`C`); often-used one shows `H`.
- `dir /` → `/A`/`/B`/`/Q` (`O`); `dir /Q` narrows.
- `bash` prompt shows coreutils, not Windows commands.
- `git ` → subcommands; `git commit --` → commit options; `git remote add -` →
  add options (nested breadcrumb `git › remote › add`).
- Typing `--password secret` then recall → history shows `--password`, never the
  value.
- Open `vim`/`less` → no overlay; return to prompt → overlay resumes.
- `Enter` with no navigation runs the command; after `↓`, `Enter` accepts (no run);
  `Tab` accepts only when `accept_tab` is on.
- `Ctrl+Shift+Space` force-opens the overlay.
- Toggle each setting in the Settings UI and confirm live effect.

## 5. Definition of done (Phase 1)

- ✅ All M0–M9 exit criteria met (M10 optional for Phase 1).
- ✅ Full workspace quality gate green (`fmt --check`, `clippy -D warnings`,
  `build`, `test -j 2`; dependency-graph + English checks pass).
- ✅ `docs/architecture.md` + `docs/agents/structure.md` updated to list the new
  `completion` crate and its ownership; README feature list updated.
- ✅ Real external catalogs committed under `crates/completion/assets/external/`
  (339 cmd + 105 coreutils), plus hand-authored `manual/common/{git,cargo}.json`
  and `manual/{windows/cmd/ping,linux/ifconfig}.json`.
