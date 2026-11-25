# 10 — Subcommands & per-subcommand options

> Part of [Terminal auto-completion design](../auto-completion.md). How the engine
> supports tools that have **subcommands**, each with its **own options** — e.g.
> `git commit --amend`, `git remote add -f`, `docker container ls`,
> `kubectl get pods`. Schema is defined in [02](02-data-sources.md) §5; this doc
> defines the parsing/resolution algorithm and UI behavior.

## 1. Motivation

Many modern CLIs are **command trees**, not flat command+flags:

```
git                      → top-level command, has global options (-C, --version)
git commit               → subcommand, own options (-m, --amend, -a)
git remote               → subcommand, own options (-v)
git remote add           → nested subcommand, own options (-t, -f, --tags)
```

Auto-completion must therefore suggest the **right level**: after `git ` it should
offer subcommands (`commit`, `checkout`, `remote`, …); after `git commit -` it
should offer *commit's* options (`-m`, `--amend`), not git's global options or
some unrelated command's flags. This applies to `git`, `docker`, `kubectl`,
`cargo`, `npm`, `az`, `gcloud`, `systemctl`, and similar tools.

## 2. Data model (recap)

A catalog file is a recursive **command node** (full schema in
[02](02-data-sources.md) §5):

```
CommandNode {
    name: String,
    options: Vec<Flag>,            // options valid at THIS node
    subcommands: Vec<CommandNode>, // children, nested to any depth
}
```

`git.json` is one such tree; `dir.json` is a degenerate tree (root only, no
children). Each command is its **own file** ([02](02-data-sources.md) §5.2), so
`git.json` is authored/generated independently of `docker.json`.

```rust
// crates/completion/src/catalog.rs
pub struct CommandNode {
    pub name: String,
    pub options: Vec<Flag>,
    pub subcommands: Vec<CommandNode>,
}
impl CommandNode {
    /// Find a direct child subcommand by name (family-aware case rules).
    pub fn child(&self, name: &str, family: ShellFamily) -> Option<&CommandNode>;
}
```

## 3. Resolution algorithm

Given the parsed input line ([04](04-suggestion-engine.md) §2), the engine walks
the tree from the top-level command down, consuming tokens to the **left of the
cursor**:

```
resolve(line_tokens_before_cursor):
    node   ← catalog.lookup(tokens[0])        # the top-level command (git)
    if node is None: return (None, path=[])   # unknown command → history/argument only
    path   ← [node]
    for tok in tokens[1..]:                    # tokens after the command name
        if tok starts with an option trigger:  # options don't advance the path
            continue
        else if node.child(tok) exists:        # a subcommand name → descend
            node ← node.child(tok); path.append(node)
        else:                                   # a positional argument → stop descending
            break
    return (node = deepest matched, path)
```

The result is the **active node** (deepest matched command/subcommand) and the
**path** from the root to it (used for the UI breadcrumb, §5).

### 3.1 Which candidates to show (context selection)

Extends [04](04-suggestion-engine.md) §3 with a subcommand context:

| Cursor token | Context | Candidates |
|---|---|---|
| first token | **Command** | top-level command names ([04](04-suggestion-engine.md) §3.1) + history |
| non-option token, and `active_node` has children | **Subcommand** | `active_node`'s child subcommand names (tag `C`) + history |
| starts with an option trigger | **Option** | options resolved for `active_node` (§3.2) (tag `O`) + history-derived options |
| non-option token, `active_node` is a leaf | **Argument** | history only — path/value completion is **out of scope** ([09](09-roadmap-risks.md) §3.3) |

Examples:

- `git ` (trailing space, active node = `git`, has children) → **Subcommand**:
  `commit C`, `checkout C`, `remote C`, … + any `H` history like `git commit -m`.
- `git c` → subcommands starting with `c`: `commit C`, `checkout C`, `clone C`,
  `config C` (+ history `H`).
- `git commit --` → **Option** for node `commit`: `--amend O`, `--no-verify O`.
- `git remote a` → subcommands of `remote`: `add C` (+ history).
- `git remote add -` → **Option** for node `add`: `-t O`, `-f O`.

### 3.2 Option resolution & inheritance

Options offered in **Option context** come from the active node, then its
ancestors:

1. `active_node.options` — highest rank (most specific to what the user is typing).
2. Ancestors' options (`remote`, then `git`) — included but ranked **below** the
   active node's own options, because global flags like `git -C` remain valid
   after a subcommand but are less likely what the user wants right now.
3. Deduplicate by flag text ([04](04-suggestion-engine.md) §4.3); the most-specific
   node wins the tag/rank.

`CompletionParams.inherit_ancestor_options` (default `true`) can turn off ancestor
inheritance for users who prefer strictly-scoped option lists.

### 3.3 Unknown / partial trees

- **Unknown top-level command** (no catalog file): no subcommand/option catalog
  data; the engine falls back to **history-only** for that line (a `git` the user
  typed before still surfaces from history even without `git.json`).
- **Unknown subcommand** (typed a subcommand the catalog doesn't list): the walk
  stops at the last known node; option context then offers that node's options.
  History still contributes.

## 4. Where subcommand data comes from

- **`manual/common/` (hand-authored, bundled):** subcommand-rich cross-platform
  tools live under `assets/manual/common/*.json` ([02](02-data-sources.md) §5.2).
  Phase 1 ships a small curated set (flagship: **`git`**; then `docker`, `cargo`);
  more tools are added over time (see [09](09-roadmap-risks.md)). The `external`
  cmd/coreutils commands are mostly flat, one leaf file each.
- **Other manual categories:** a subcommand tool specific to Windows or Linux can
  instead live under `manual/windows/cmd/` or `manual/linux/` — the `subcommands`
  tree works in any category.
- **`memory` (history):** naturally subcommand-aware already, because it stores
  **whole command lines** — typing `git com` surfaces the prior `git commit -m`
  regardless of catalog coverage.

Generating accurate subcommand trees from upstream docs is harder than flat flag
lists, so subcommand tools are **hand-authored** in `manual/common/` rather than
produced by the [script](07-external-assets-script.md) (which only emits the flat
`external` cmd/coreutils catalogs).

## 5. UI

- Subcommand suggestions use the **`C` (Command)** tag — a subcommand *is* a
  command — keeping the three-tag scheme ([05](05-ui.md) §5) intact. (An optional
  distinct `S` tag is noted as a possible future refinement but is **not** added in
  Phase 1, to honor the fixed `H`/`C`/`O` set.)
- **Breadcrumb (optional, recommended):** when the active node is below the root,
  the overlay shows a dim header with the resolved path, e.g. `git › remote ›`,
  so the user sees which level the options/subcommands belong to. Low effort,
  high clarity; can ship in Phase 1 or shortly after.
- Accept semantics are unchanged ([00](00-overview.md) §5): accepting a subcommand
  or option appends only the remainder to the PTY.

## 6. Testing

`crates/completion` unit tests for nested resolution:

- `git ` → offers `commit`/`remote`/… (subcommand context).
- `git remote ` → offers `add`/`remove` (nested subcommand context).
- `git commit --` → offers commit's long options, not git's globals first.
- `git remote add -` → offers add's options; with `inherit_ancestor_options`,
  `remote`/`git` options appear ranked lower.
- Unknown subcommand `git frobnicate -` → falls back to `git`'s options + history.
- Per-file loading: `git.json` parses independently; a malformed `docker.json`
  does not break `git` completion.
