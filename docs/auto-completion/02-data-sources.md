# 02 — Data sources

> Part of [Terminal auto-completion design](../auto-completion.md). The three
> suggestion sources (`memory`, `manual`, `external`), the common JSON catalog
> schema, and how they are loaded and merged.

## 1. Overview

Every suggestion comes from one of three sources, each mapping to a
[`SuggestionKind`](01-architecture.md#5-key-public-types-engine-crate) / tag:

| Source | Tag | Storage | Scope | Origin |
|---|---|---|---|---|
| `memory` (history) | `H` | RAM only (non-persistent) | all Terminal Tabs (per shell family) | commands the user ran this session |
| `manual` | `C` / `O` | **bundled in-crate, embedded** | global | hand-authored catalog files covering what `external` does not |
| `external` | `C` / `O` | **bundled in-crate, embedded** | global | script-generated from the two upstream doc sources (`cmd` + `coreutils`; see [07](07-external-assets-script.md)) |

> **This phase: everything is inside the crate and compiled into the binary.**
> Both `manual` and `external` JSON live under `crates/completion/assets/` and are
> embedded via `build.rs` ([§5.2](#52-file-layout--embedding)); OneTerm reads **no**
> JSON from any external/on-disk folder. User-editable manual catalogs (a config
> directory) are a later phase — see [09](09-roadmap-risks.md).

`manual` and `external` share **one JSON schema** (§5). `memory` is not a catalog
— it is a runtime ring buffer (§2).

## 2. `memory` source (in-session history)

### 2.1 What is captured

The **command line the user actually ran** — i.e. the text between the OSC 133
`PromptEnd` (`B`) marker and the `OutputStart` (`C`) marker. This reuses the
row-role model already built for semantic highlighting
([`RowRole::Command`](../terminal-semantic-highlighting.md), see
`crates/highlight/src/role.rs`).

Capture trigger and path:

1. When OSC 133 `OutputStart` (`C`) or `OutputEnd` (`D`) arrives, `terminal-view`
   extracts the command-input text (the `RowRole::Command` rows since the last
   `PromptEnd`).
2. The raw line is passed through **redaction** ([08](08-security-redaction.md))
   which drops secret values but keeps command + option names.
3. The redacted line is recorded into the global `CompletionHistory` under the
   session's `ShellFamily` (see [01](01-architecture.md) §4).

### 2.2 Fallback when the shell emits no OSC 133

Not every shell/host emits shell-integration markers (e.g. a bare SSH login shell
with no `PROMPT_COMMAND`). Fallback capture:

- Use the same prompt-regex fallback the highlighter uses
  (`ShellProfile`, `crates/highlight/src/profile.rs`) to detect the command row,
  **or**
- Capture the input line locally: OneTerm already tracks the token stream the user
  types at the prompt for the overlay; when the user presses Enter and the
  terminal is in the command-input region, record that buffered line.

The fallback is best-effort. When neither OSC 133 nor a confident prompt match is
available, history capture for that session is simply skipped (the `manual` /
`external` sources still work). This keeps history quality high and avoids
recording program output as if it were commands.

### 2.3 What is stored

- A bounded ring buffer **per shell family**, capacity = `CompletionConfig.max_history`
  (default 500). Oldest entries evicted first.
- Each entry stores the redacted **full command line** plus a small frecency
  record (last-used timestamp + use count) so ranking can favor recent/frequent
  commands (see [04](04-suggestion-engine.md) §4).
- Deduplication: recording an existing line updates its frecency instead of
  adding a duplicate.
- **Never** written to disk. Cleared on process exit, on "Clear history" action,
  and when `max_history` is set to 0.

### 2.4 What history suggests

Both **whole command lines** and **first tokens** are matchable:

- Typing `git c` can surface the history entry `git commit -m` (tag `H`).
- Typing `d` can surface `docker` from history (tag `H`) ranked above the static
  catalog `date`/`dir` if the user uses `docker` a lot.

## 3. `manual` source (hand-authored, bundled)

`manual` is a set of **hand-authored** per-command JSON files that cover the
commands the two `external` doc sources do **not** (§4). It ships **inside the
crate** under `assets/manual/` and is embedded into the binary — this phase does
not read user files. It is organised by OS/shell:

```
assets/manual/
├── windows/
│   ├── cmd/          # Windows/cmd commands missing from external/cmd (e.g. ping.json)
│   └── powershell/   # (future) hand-authored PowerShell cmdlets
├── linux/            # non-coreutils Unix commands (e.g. ifconfig.json, vi.json)
└── common/           # cross-platform, usually subcommand-rich tools (git.json, cargo.json, docker.json)
```

Behavior:

- Same **one-file-per-command** schema as `external` (§5), including `subcommands`
  for tool trees (e.g. `common/git.json`).
- A malformed file is logged and skipped **per file** — the rest of `manual`, plus
  `external` + `memory`, keep working; it never crashes the terminal
  ([`docs/agents/error-policy.md`](../agents/error-policy.md)).
- When `manual` and `external` declare the **same command name**, **`manual` wins**
  (§6) — so a hand-authored `common/git.json` overrides any generated `git`.
- Entries are tagged `C`/`O`.

## 4. `external` source (script-generated, bundled)

`external` is generated by [`scripts/completion-catalog.py`](07-external-assets-script.md)
from upstream documentation and ships embedded under `assets/external/`. There are
currently **two** sources (a third, PowerShell, may come later):

| Folder | Category | Built from |
|---|---|---|
| `external/cmd/` | `cmd` | `https://github.com/MicrosoftDocs/windowsserverdocs` → `WindowsServerDocs/administration/windows-commands` |
| `external/coreutils/` | `coreutils` | `https://manpages.debian.org/bookworm/coreutils/index.html` |
| `external/powershell/` | `powershell` | **future** (empty for now) |

- The raw docs are **not** committed; only the generated per-command JSON is. The
  [generator script](07-external-assets-script.md) downloads the raw sources and
  writes one file per command into the matching folder.
- Both `external` and `manual` are embedded at compile time via the `build.rs`
  index ([§5.2](#52-file-layout--embedding)) — no runtime file dependency.

### 4.1 Categories and which folders a running shell uses

The bundled data is grouped into **categories** (each backed by an `external`
and/or `manual` folder):

| Category | Source → folder |
|---|---|
| `cmd` | external → `external/cmd/` |
| `coreutils` | external → `external/coreutils/` |
| `powershell` | external → `external/powershell/` (+ future manual → `manual/windows/powershell/`) |
| `windows` | manual → `manual/windows/cmd/` |
| `linux` | manual → `manual/linux/` |
| `common` | manual → `manual/common/` |

The running shell's `ShellFamily` ([03](03-shell-detection.md)) selects an **ordered
set of categories** searched high → low precedence:

| Running shell | Categories searched (high → low) |
|---|---|
| `Cmd` | `cmd` → `windows` → `common` |
| `PowerShell` | `powershell` → `cmd` → `windows` → `common` |
| `Unix` | `coreutils` → `linux` → `common` |

So a `cmd` prompt sees the MicrosoftDocs cmd commands, the manual Windows extras,
and the common tools — never Linux commands; a `bash` prompt sees coreutils, the
manual Linux extras, and common tools. `common/` is shared by every shell — that is
where `git` lives, so `git ` completes in `cmd`, `powershell`, and `bash` alike.

**Coreutils on Windows.** Some Windows setups also run coreutils (Git-Bash,
busybox, MSYS, scoop/choco installs). The setting
`CompletionConfig.windows_allow_coreutils` (default `false`,
[06](06-configuration.md)) appends `coreutils` → `linux` at the **lowest**
precedence for the `Cmd` and `PowerShell` families, so those commands are offered
without ever outranking native cmd/Windows commands.

## 5. Catalog schema — one JSON file per command

Each command is its **own JSON file** (`<command>.json`). A file describes a single
top-level command as a recursive **command node**: a name, its options, and
optionally its subcommands (each itself a command node with its own options and,
recursively, its own subcommands). This models tools like `git`
(`git commit --amend`, `git remote add -f`); see [10-subcommands.md](10-subcommands.md).

```jsonc
// crates/completion/assets/manual/common/git.json
{
  "schema": 1,                 // schema version (integer, for migrations)
  "generated": "2026-08-03",   // optional; provenance for `external` files
  "name": "git",
  "options": ["-C", "--version", "--help"],   // options valid for `git` itself
  "subcommands": [
    { "name": "commit",   "options": ["-m", "-a", "--amend", "--no-verify"] },
    { "name": "checkout", "options": ["-b", "-f", "--track"] },
    {
      "name": "remote",
      "options": ["-v"],
      "subcommands": [
        { "name": "add",    "options": ["-t", "-f", "--tags"] },
        { "name": "remove", "options": [] }
      ]
    }
  ]
}
```

A leaf command with no subcommands is just the top of the recursion:

```jsonc
// crates/completion/assets/external/cmd/dir.json
{ "schema": 1, "name": "dir",
  "options": ["/A", "/B", "/O", "/P", "/Q", "/S", "/W", "/X"] }
```

Field rules (a **command node** = `{ name, options, subcommands? }`):

- `schema` — integer; the engine rejects unknown major versions (logs + ignores).
- `name` — the command/subcommand token (case-insensitive match for `cmd`/
  `powershell`/`windows`, case-sensitive for `coreutils`/`linux`).
- `options` — option flags **including their trigger prefix** (`/A`, `--all`,
  `-a`). Each node has its **own** option set; resolution merges a node's options
  with its ancestors' (see [10](10-subcommands.md) §3.2).
- `subcommands` — optional array of child command nodes, nested to any depth.
  Absent/empty ⇒ a leaf command.
- The file's **source and category are derived from its folder path** ([§5.2](#52-file-layout--embedding)),
  so a command node carries **no** `family`/`category` field — moving or copying a
  file between folders is all it takes to recategorise it.
- **Reserved (Phase 2, optional, ignored in Phase 1):** `description` on any node,
  and options written as objects `{ "flag": "--all", "description": "…" }`. The
  parser accepts an option that is either a string or such an object so Phase 2
  data does not break Phase 1 readers.

### 5.1 Why one file per command

- **Independent authoring/generation:** `git.json` is produced and reviewed on its
  own; refreshing `docker.json` never touches unrelated commands, so git diffs stay
  small and targeted.
- **Lazy loading:** the engine parses a command's file only when that command is
  first referenced (§5.3), so a large catalog (hundreds of commands) costs nothing
  at startup.
- **Fault isolation:** a malformed `docker.json` is logged and skipped without
  affecting `git` or `ls` completion.
- **Clear ownership:** `manual` and `external` map cleanly — a hand-authored
  `manual/common/git.json` overrides a generated `git` of the same name (§6).

### 5.2 File layout & embedding

Per-command files live under two top-level source folders (`external/` generated,
`manual/` hand-authored); the path encodes **source + category**:

```
crates/completion/assets/
├── external/                       # generated by scripts/completion-catalog.py
│   ├── cmd/         dir.json  copy.json  del.json  ipconfig.json  …   → category cmd
│   ├── coreutils/   ls.json   cp.json    grep.json  …                 → category coreutils
│   └── powershell/  (future — empty)                                  → category powershell
├── manual/                         # hand-authored; covers what external does not
│   ├── windows/
│   │   ├── cmd/        ping.json  … (windows/cmd commands external misses) → category windows
│   │   └── powershell/ (future — empty)                                    → category powershell
│   ├── linux/       ifconfig.json  vi.json  …                          → category linux
│   └── common/      git.json  cargo.json  docker.json  …               → category common
└── catalog.schema.json                                                  (validates every file)
```

A `build.rs` in the `completion` crate walks `assets/**/*.json` at compile time and
generates a static index — a `&[(name, source, category, json_str)]` built from
`include_str!`, where `source` (external/manual) and `category` are derived from the
folder path — so every command is **embedded in the binary** with no runtime file
dependency (the same build-script-embeds-assets pattern the `theme` crate uses for
icons).

### 5.3 Lazy parse

The generated index maps `command name → embedded &str`. The engine parses a
command node from its embedded JSON only on **first reference** to that command and
caches the result, keeping startup cheap regardless of catalog size (see
[01](01-architecture.md) §6).

### 5.4 Why options carry their prefix

The three shells use different option triggers (`/` for cmd, `-`/`--` for unix,
`-` for PowerShell). Rather than encode the trigger separately, each option string
includes it. The engine's option-context matcher (see [04](04-suggestion-engine.md)
§3) then just prefix-matches the token — `dir /` matches every `dir` option
starting with `/`, and `grep --` matches `--all`, `--color`, … but not `-a`.

## 6. Merge order and precedence

For a given query the engine gathers candidates from all enabled sources across the
active shell's categories ([§4.1](#41-which-folders-a-running-shell-uses)), then
ranks them together ([04](04-suggestion-engine.md) §4). When the **same text**
appears from multiple sources, the engine keeps one entry and picks the tag by this
precedence:

1. `H` (history) — the user actually used it → most relevant, shown with the `H`
   tag even if it also exists in a catalog.
2. `C`/`O` from `manual` — hand-authored data. **`manual` always beats `external`**
   for the same command name, regardless of category order (per the requirement).
3. `C`/`O` from `external` — generated default. When the same name appears in
   several `external` categories, the earlier one in the shell's search order
   ([§4.1](#41-which-folders-a-running-shell-uses)) wins.

So `dir` typed often this session shows as `dir  H`; a never-used catalog command
shows as `dir  C`; a `git` present in both `manual/common` and a future generated
catalog resolves to the `manual` entry.
