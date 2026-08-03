# 00 — Overview

> Part of [Terminal auto-completion design](../auto-completion.md). Motivation,
> scope, non-goals, and shared vocabulary.

## 1. Motivation

When a user types at the shell prompt inside a OneTerm terminal, there is no
in-app assistance: they must remember command names and option flags, or rely on
whatever completion the underlying shell provides (which differs wildly between
`cmd`, PowerShell, and POSIX shells, and is unavailable over a bare SSH channel).

**Auto-completion** adds a lightweight, shell-aware overlay that suggests
**command names** and **option flags** as the user types, ranked by what they have
already used this session and backed by curated catalogs. It is inspired by
fish-shell / Warp / Fig-style completion but implemented natively inside the
OneTerm terminal view, and it works identically for local shells and remote SSH
sessions because it never depends on the shell's own completion engine.

## 2. User requirements

Captured from the original request and the follow-up decisions. IDs match the
[requirements → design map](../auto-completion.md#requirements--design-map);
`Q*` items are resolved design decisions ([09](09-roadmap-risks.md) §4).

**Core suggestion behavior**

- (R1) Suggest command names and option flags as the user types at the prompt.
- (R2) Typing a prefix such as `d` shows an overlay of matching commands
  (`date`, `dir`, `del`, …).
- (R3) Typing an option trigger (`-`, `--`, or `/`) switches to listing the current
  command's options (e.g. `dir /` → `/A`, `/B`, `/Q`, …).
- (R17) Support apps with subcommands and per-subcommand options
  (e.g. `git commit --amend`, `git remote add -f`).
- (Q1) `Enter` runs the command (run-first); the overlay does not hijack Enter
  until the user navigates it.
- (Q3) Fuzzy matches are display/navigation aids; Tab/Enter accept is prefix-only
  by default (`allow_fuzzy_accept` off).

**Shell awareness**

- (R4) Detect the running shell and use a matching catalog: a `bash` prompt must
  not suggest Windows commands; a `cmd` prompt must not suggest coreutils flags.
- (Q2) Do not over-separate families — every family accepts `-`/`--`; the Windows
  family also accepts `/` (both Windows and Linux have commands using `-`).

**Data sources**

- (R5) `memory` — commands typed **this session**: non-persistent, shared across
  all Terminal Tabs, reset when OneTerm exits.
- (R7) `manual` — a hand-authored catalog (in a defined format) covering the
  commands `external` does not. This phase it is **bundled in-crate**; a
  user-editable config directory is a later phase ([09](09-roadmap-risks.md)).
- (R8) `external` — script-generated catalogs from Windows Commands (`cmd`) + Unix
  coreutils (`coreutils`) docs; more sources (PowerShell) may come later.
- (R9) Convert raw external data into one simple JSON format (no descriptions in
  Phase 1).
- (R18) Store each command as its **own JSON file**.

**Security & privacy**

- (R6) Never store or suggest sensitive values (tokens, passwords, API keys); still
  suggest the command/option **names**, minus the secret.
- Telemetry-free — history and usage never leave the machine (no network).

**UI / presentation**

- (R11) List-style overlay with cursor-aware placement (top/bottom).
- (R12) Item format `<highlight_text><suggest_text>␣␣␣␣␣<tag>`, matched prefix
  highlighted.
- (R13) Tags `History → H`, `Command → C`, `Option → O`, each with its own
  background color.

**Configuration & gating**

- (R14) Use OSC 133 + alternate screen to detect TUIs and turn completion off.
- (R15) Settings: enable/disable, accept-with-Tab on/off, max command history,
  and more.

**Tooling**

- (R10) A script with at least `download` + `generate` to build the external
  catalogs.

**Accepted enhancements**

- Manual trigger key — force-open the overlay (default `Ctrl+Shift+Space`).
- Frecency-first "recent commands" palette (`RecentCommands` action).

**Out of scope (rejected / skipped)**

- Inline ghost text; path / argument completion; per-host or per-session catalogs;
  description-on-select detail line; "learn from output" (`--help` scraping);
  i18n of UI strings. See [09](09-roadmap-risks.md) §3.3.

## 3. Scope (what we build — Phase 1)

- An **overlay list** that appears at the prompt as the user types, showing
  suggestions that match the token currently under the cursor.
  - Typing `d` lists commands starting with `d`: `date`, `dir`, `del`, …
  - Typing an **option trigger** (`-`, `--`, or `/`, depending on the shell)
    switches to listing **options** of the command already on the line.
    Example: `dir /` → `/A`, `/B`, `/Q`, …
- **Shell detection**: the suggestion set is chosen from the running shell's
  family (Windows vs Unix) so a `bash` prompt never suggests `dir /Q` and a
  `cmd` prompt never suggests coreutils flags. See [03](03-shell-detection.md).
- **Three data sources** merged into one ranked list (see [02](02-data-sources.md)):
  - `memory` — commands the user typed **this session**, shared across every
    Terminal Tab, **not persisted** (reset when OneTerm exits).
  - `manual` — a hand-authored catalog, bundled in-crate, covering commands
    `external` does not (Windows extras, non-coreutils Unix, cross-platform tools).
  - `external` — script-generated catalogs from Windows Commands (`cmd`) and Unix
    coreutils (`coreutils`) documentation.
- **Sensitive-data safety**: history never stores or suggests secret values
  (tokens, passwords, API keys). The command and option **names** are still
  learned and suggested; only the secret argument value is dropped. See
  [08](08-security-redaction.md).
- **Cursor-aware placement**: the overlay opens below the prompt when there is
  room, above it otherwise (see [05](05-ui.md)).
- **Tagged items**: each row is `<matched-prefix><rest-of-suggestion>␣␣␣␣␣<tag>`,
  the matched prefix highlighted, and a colored tag badge — `H` (history),
  `C` (command), `O` (option).
- **TUI-safe**: completion suppresses itself when the app is on the alternate
  screen or when OSC 133 says the terminal is not at a command-input prompt, so
  it never interferes with `vim`, `htop`, `less`, etc. See [06](06-configuration.md) §3.
- **Configurable**: enable/disable, accept-with-Tab on/off, max history size,
  and more, stored in `terminal.json` and surfaced in the Settings UI.
- **A build script** (`download` + `generate`) that turns raw upstream docs into
  the common JSON format. See [07](07-external-assets-script.md).

## 4. Non-goals (Phase 1)

- **Descriptions** for commands/options in the overlay (schema leaves room for
  them, but rendering is **not planned** — see [09](09-roadmap-risks.md) §3.3).
- **Persisting** the `memory` history across restarts (deliberate — the request
  requires non-persistence; opt-in persistence is a later phase).
- **Argument/value completion** (file paths, git branches, env values):
  **out of scope** — argument context stays history-only ([09](09-roadmap-risks.md)
  §3.3). Note: command **subcommands** (`git commit`, `git remote add`) **are** in
  scope — see [10-subcommands.md](10-subcommands.md).
- **Executing** the shell's own completion (`compgen`, PowerShell
  `TabExpansion`, `pwsh` predictors). OneTerm supplies its own catalogs instead;
  we do not shell out to the remote/local completion engine.
- **Learning across machines / sync**. Out of scope.
- **Inline ghost text** (fish-style gray preview). **Rejected** — the overlay is
  the only suggestion surface (see [09](09-roadmap-risks.md) §3.3).

## 5. What "suggest" means here (interaction model)

Auto-completion is **non-intrusive** and does not type anything into the PTY until
the user accepts:

1. The user types characters. OneTerm mirrors those keystrokes to the PTY as
   normal (nothing changes about how input reaches the shell).
2. In parallel, OneTerm tracks the **current input line** and the **token under
   the cursor** (see [04](04-suggestion-engine.md) §2), and renders the overlay.
3. On **accept** (Enter on a highlighted item, or Tab if `accept_tab` is on), the
   engine computes the *completion remainder* (the suggestion minus what the user
   already typed) and writes only that remainder to the PTY, then dismisses the
   overlay. It never rewrites text already sent to the shell.
4. On **Esc**, or when the token stops matching anything, the overlay dismisses
   and typing continues normally.

Because acceptance only ever **appends** the remainder, OneTerm does not need to
send backspaces or manipulate the shell's line editor — a key simplification that
makes the feature safe across `cmd`, PowerShell, and remote POSIX shells.

## 6. Glossary

- **Overlay** — the floating suggestion list rendered above the terminal grid,
  anchored to the cursor. Not a dock panel, not a popup window.
- **Suggestion** — one candidate the engine offers, with a display string, a
  **kind/tag** (`History` / `Command` / `Option`), the source it came from, and a
  rank score.
- **Tag** — the one-letter badge for a suggestion's kind: `H`, `C`, or `O`.
- **Token under cursor** — the whitespace-delimited word the cursor is currently
  editing on the input line; what the engine matches against.
- **Option context** — the state where the token under the cursor begins with an
  **option trigger** (`-`/`--`/`/`), so the engine lists the *current command's*
  options instead of commands.
- **Catalog** — the set of commands (each with option flags and optional
  subcommands) available to the engine. Sourced from `manual` or `external`,
  organised into **catalog categories**.
- **Catalog category** — one of the bundled groupings a command belongs to:
  `cmd` / `coreutils` / `powershell` (from `external/`) or `windows` / `linux` /
  `common` (from `manual/`). A running shell searches an ordered subset
  ([02](02-data-sources.md) §4.1).
- **Shell family** — the completion-relevant class of the running `ShellKind`:
  `Cmd` / `PowerShell` / `Unix` (`bash`/`zsh`/`sh`). Selects which catalog
  categories are searched. See [03](03-shell-detection.md).
- **`memory` / history source** — the non-persistent, cross-tab ring buffer of
  commands the user ran this session.
- **Command-input region** — the prompt area between OSC 133 `PromptEnd` (`B`) and
  `OutputStart` (`C`); the only place completion is allowed to appear.
- **Redaction** — dropping a secret argument value from a remembered command line
  while keeping the command and option names (see [08](08-security-redaction.md)).
