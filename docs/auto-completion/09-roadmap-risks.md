# 09 — Roadmap, risks & value-add features

> Part of [Terminal auto-completion design](../auto-completion.md). Delivery
> phases, known risks/mitigations, and suggested additional features that would
> add value beyond the Phase 1 requirement.

## 1. Delivery phases

### Phase 1 — MVP (this spec)

1. `completion` engine crate: model (incl. **recursive command nodes /
   subcommands**), catalog load/merge with **subcommand-tree resolution**
   ([10](10-subcommands.md)) and the category **search path** per shell
   ([02](02-data-sources.md) §4.1), line parsing, matching + frecency ranking,
   `CompletionHistory` ring, redaction, and the embedded catalogs (`external/` +
   `manual/`) as **one JSON file per command** (`assets/**/*.json` + a `build.rs`
   index + `catalog.schema.json`). Full unit tests.
2. `scripts/completion-catalog.py`: `download` + `generate` (+ `update`) for the
   `external` sources (`cmd`, `coreutils`); the `manual` categories are hand-authored
   (flagship subcommand tool: `manual/common/git.json`).
3. `state`: global `CompletionHistory` entity + `init()`.
4. `settings`: `completion` group in `terminal.json` + live `TerminalSettings`.
5. `terminal-view`: `CompletionController` (input tracking, gating via
   alt-screen + OSC 133), `CompletionOverlay` (cursor-anchored list, tags, keys,
   optional command-path breadcrumb), accept = apply exact suggestion text under
   the active family's case rule, history capture on OSC 133 `C`/`D`.
6. `settings-ui`: Completion section; `oneterm-actions` `ToggleCompletion` /
   `ClearCompletionHistory` / `TriggerCompletion` (default binding
   `Ctrl+Shift+Space`) actions.
7. Quality gate: `cargo fmt --all --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test
   --workspace`.

**Phase 1 acceptance:** typing `d` at a `cmd` prompt lists `date`/`dir`/… with `C`
tags; `dir /` lists `/A`/`/B`/`/Q`…with `O` tags; `git ` lists subcommands
(`commit`/`remote`/…) and `git commit --` lists commit's options; `git remote add -`
lists add's options (nested); a used command shows an `H` tag; a `bash` prompt shows
coreutils, not Windows commands; secrets are never suggested; the overlay never
appears inside `vim`/`less`; Tab-accept respects `accept_tab`.

### Phase 2 — enrichment

- **Frecency-first "recent commands" palette** (`RecentCommands` action) — §3.2.
- **Broader subcommand-tool coverage:** more hand-authored `manual/common/*.json`
  (`docker`, `kubectl`, `cargo`, `npm`, `az`, `gcloud`, …) beyond the Phase 1
  flagship `git`. The subcommand *engine* already ships in Phase 1
  ([10](10-subcommands.md)).
- **PowerShell cmdlet catalog** generated into `external/powershell/` from
  `Get-Command` / help metadata.
- **User-extensible secret vocabulary/patterns**.

### Phase 3 — advanced

- **User-editable manual catalogs**: read hand-authored `<command>.json` from a
  config directory (e.g. `config_dir()/completions/`) in addition to the bundled
  `manual/`, so users extend/override catalogs without rebuilding. (Phase 1 keeps
  all data in-crate and reads no external files.)
- **Optional persistent history** (opt-in), with the same redaction guarantees and
  the persistence rules in [`docs/agents/persistence.md`](../agents/persistence.md).

  (Path / argument completion is **out of scope** — see §3.3.)

## 2. Risks & mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Overlay interferes with a TUI that does not use the alternate screen | Suggestions pop up during a full-screen program | OSC 133 region gate (§3 of [06](06-configuration.md)); `disable_in_alt_screen` covers the common case; user can disable per-shell |
| Shell/host emits no OSC 133 | Weaker history capture + region gating | Prompt-regex fallback (reused from the highlighter); graceful degradation to catalog-only |
| Accept sends wrong bytes (line-editor mismatch across shells) | Corrupted command line | Append-only remainder (no backspaces); restrict Tab/Enter accept to prefix matches in Phase 1 |
| Secret leaks into history | Credential exposure | Redact at capture **and** guard at suggestion time; extensive tests ([08](08-security-redaction.md)) |
| Large catalog slows matching | Input lag | Catalogs are bounded and cached; sync match target < 1 ms; move to background task if needed ([01](01-architecture.md) §6) |
| Wrong shell family (SSH / custom) | Slightly-off command list | Family-tolerant option triggers; `force_family` override; SSH default `Unix` refined by OSC 133 |
| External source layout changes upstream | `generate` breaks | Script validates against `catalog.schema.json` and fails loudly; catalogs are committed so a broken fetch never ships |
| Split Spaces / multiple terminals | Overlay confusion | One overlay per terminal; only the focused terminal shows it ([05](05-ui.md) §7) |

## 3. Value-add features — decisions

Decisions on the originally-suggested value-add features.

### 3.1 Accepted (in scope)

- **Subcommand + rich flag catalogs** (`git`/`docker`/`kubectl`/`cargo`/…). The
  subcommand *engine* ships in Phase 1 ([10](10-subcommands.md)) with `git` as the
  flagship; more hand-authored `manual/common/*.json` follow in Phase 2.
- **Manual trigger key** — a `TriggerCompletion` action that force-opens the overlay
  at the cursor even below `min_prefix_len` (and re-opens after dismiss).
  **Default binding: `Ctrl+Shift+Space`.** See [06](06-configuration.md) §6.
- **Frecency-first "recent commands" palette** — see §3.2.
- **Telemetry-free by design** — history and usage never leave the machine (no
  network); documented as an explicit guarantee.

### 3.2 "Recent commands" palette — sketch

- New `oneterm-actions` action `RecentCommands` (unbound by default — the natural
  analog `Ctrl+R` is left unbound to avoid clashing with the shell's own
  reverse-search; the user can bind it).
- Opens the same overlay UI in a mode seeded with the top history entries for the
  active `ShellFamily`, sorted by frecency; typing filters them.
- Reuses `CompletionHistory` + redaction; accept = append the line to the prompt
  (**never auto-run**, per §4 Q1). Works identically over SSH.

### 3.3 Rejected / out of scope

Explicitly **not** built (recorded so they are not reconsidered by accident):

| Feature | Decision | Note |
|---|---|---|
| Inline ghost text (fish-style) | **Rejected** | The overlay is the only suggestion surface; no inline gray preview. |
| Path / argument completion | **Skipped** | Argument context stays history-only ([04](04-suggestion-engine.md) §3.4); no filesystem/CWD walk. |
| Per-host / per-session manual catalogs | **Rejected** | One global manual directory only ([02](02-data-sources.md) §3). |
| Description-on-select detail line | **Rejected** | No description UI; the `description` schema field stays reserved/unused. |
| "Learn from output" (`--help` scraping) | **Rejected** | Catalogs come only from bundled/manual/history sources. |
| i18n of UI strings | **Rejected** | English-only for this feature (workspace i18n is a separate, unrelated roadmap item). |

## 4. Resolved decisions

- **Q1 — What should `Enter` do while the overlay is open? → Run-first (decided).**
  `Enter` in a terminal is sacred: it runs the current command line. Two models:
  - *Preselect-first* (IDE-style): row 0 is selected on open; `Enter` accepts it.
  - *Run-first* (this spec's default): nothing is selected on open; `Enter` runs
    the typed line; a selection only exists after `Up`/`Down`, and then `Enter`
    accepts (and still does **not** run — see below).

  **Decision: run-first**, with `preselect_first` (default `false`) to opt into the
  IDE model. Rationale:
  - Avoids the `ls`-vs-`lsblk` footgun: with preselect, pressing `Enter` to run a
    short command that is a prefix of a longer suggestion would complete instead of
    run — and the command token is exactly where this hurts most.
  - Preserves muscle memory (type → `Enter` → run) without a two-`Enter` /
    `Esc`+`Enter` dance to run what you actually typed.
  - `accept_tab` already provides a dedicated accept key (`Tab`), so `Enter` need
    not double as accept.

  **Companion invariant (regardless of the choice): accept never auto-runs.**
  Accepting a suggestion — including a full history line like
  `rm -rf build && deploy prod` — only places it on the line; the user presses
  `Enter` again to run. Auto-running a remembered command would be dangerous.

  **Selection rules (run-first):** only Tab engages the list. First Tab selects
  item 0 without applying it; second Tab or Enter applies the selected suggestion
  without running it. Before selection, Enter and Up/Down/Ctrl+P/Ctrl+N pass through
  to the shell. After selection, navigation moves within the suggestion list.

  Precedent: fish, PSReadLine `MenuComplete`, and VSCode terminal-suggest are all
  effectively run-first until the user engages the list; Warp preselects and had to
  add a setting after user pushback — hence: default safe, make it configurable,
  revisit after dogfooding.
- **Q2 — For `cmd`, offer `-` as an option trigger, or strictly `/`? → Offer both
  (decided).** Do **not** over-separate families: both Windows and Linux have
  commands that use `-` for options (and Windows ports of POSIX tools are common),
  so every family treats `-`/`--` as valid option triggers and the `Cmd` family
  additionally accepts `/`. Matching stays prefix-based on the option's stored
  prefix, so mixing does not mislabel flags. See [03](03-shell-detection.md) §2.
- **Q3 — Fuzzy-accept: allow it, or keep it display-only? → `allow_fuzzy_accept`
  default `off` (decided).** Tab/Enter accept remains restricted to prefix matches;
  fuzzy matches are display/navigation aids. Cmd/PowerShell case-insensitive prefix
  matches are not fuzzy: acceptance may backspace only the case-mismatched suffix
  within the suggestion replacement range so the result exactly matches the
  displayed suggestion. Unix remains exact-case and append-only.
