# 03 — Shell detection

> Part of [Terminal auto-completion design](../auto-completion.md). How the engine
> knows which shell is running, maps it to a catalog family, and picks the correct
> option trigger characters. Covers local shells and remote SSH.

## 1. Why it matters

The suggestion set must match the running shell:

- A `bash` prompt must **not** suggest `dir /Q` or other `cmd` commands.
- A `cmd` prompt must **not** suggest coreutils flags like `ls --color`.
- Option triggers differ: `cmd` uses `/`, POSIX shells use `-`/`--`, PowerShell
  uses `-`.

The engine keys everything off a single value: the **`ShellFamily`**.

## 2. `ShellKind` → `ShellFamily` mapping

OneTerm already resolves the local shell to
[`core::config::shell::ShellKind`](../terminal-backend.md) (`Cmd`, `PowerShell`,
`Pwsh`, `Bash`, `Zsh`, `Sh`, `Custom`). The engine collapses that into a
completion-relevant family:

| `ShellKind` | `ShellFamily` | Option trigger(s) | Categories searched (high → low) |
|---|---|---|---|
| `Cmd` | `Cmd` | `/`, `-` | `cmd` → `windows` → `common` |
| `PowerShell` | `PowerShell` | `-` | `powershell` → `cmd` → `windows` → `common` |
| `Pwsh` | `PowerShell` | `-` | `powershell` → `cmd` → `windows` → `common` |
| `Bash` | `Unix` | `-`, `--` | `coreutils` → `linux` → `common` |
| `Zsh` | `Unix` | `-`, `--` | `coreutils` → `linux` → `common` |
| `Sh` | `Unix` | `-`, `--` | `coreutils` → `linux` → `common` |
| `Custom` | see §5 | resolved heuristically | resolved heuristically |

The categories map to bundled `external`/`manual` folders per
[02](02-data-sources.md) §4.1. When `windows_allow_coreutils` is on
([06](06-configuration.md)), the `Cmd` and `PowerShell` families additionally append
`coreutils` → `linux` at the lowest precedence.

```rust
// crates/completion/src/family.rs

/// The catalog categories (each backed by an external and/or manual folder — 02 §4.1).
pub enum CatalogCategory { Cmd, Coreutils, PowerShell, Windows, Linux, Common }

/// The running shell's completion-relevant family.
pub enum ShellFamily { Cmd, PowerShell, Unix }

impl ShellFamily {
    pub fn from_kind(kind: ShellKind) -> Self {
        match kind {
            ShellKind::Cmd => ShellFamily::Cmd,
            ShellKind::PowerShell | ShellKind::Pwsh => ShellFamily::PowerShell,
            ShellKind::Bash | ShellKind::Zsh | ShellKind::Sh => ShellFamily::Unix,
            ShellKind::Custom => ShellFamily::Unix, // conservative default; refined in §5
        }
    }

    /// Catalog categories searched, high → low precedence (02 §4.1).
    /// `allow_coreutils` appends `Coreutils`/`Linux` for the Windows families.
    pub fn categories(self, allow_coreutils: bool) -> Vec<CatalogCategory> {
        use CatalogCategory::*;
        let mut c = match self {
            ShellFamily::Cmd => vec![Cmd, Windows, Common],
            ShellFamily::PowerShell => vec![PowerShell, Cmd, Windows, Common],
            ShellFamily::Unix => vec![Coreutils, Linux, Common],
        };
        if allow_coreutils && matches!(self, ShellFamily::Cmd | ShellFamily::PowerShell) {
            c.extend([Coreutils, Linux]);
        }
        c
    }

    /// Characters that start an option token for this family.
    pub fn option_triggers(self) -> &'static [char] {
        match self {
            ShellFamily::Cmd => &['/', '-'],
            ShellFamily::PowerShell => &['-'],
            ShellFamily::Unix => &['-'],
        }
    }
}
```

## 3. Option trigger detection

The suggestion engine enters **option context** (see [04](04-suggestion-engine.md)
§3) when the token under the cursor starts with one of the family's triggers:

- **Windows / `cmd`:** `dir /` → list `/A`, `/B`, `/Q`, … (also `-` tolerated
  because some Windows ports accept POSIX-style flags).
- **Unix:** `grep -` → short options; `grep --` → long options. Since options in
  the catalog carry their full prefix (`-a`, `--all`), typing `-` matches both
  `-a` and (loosely) `--all`, while `--` narrows to long options only.
- **PowerShell:** `Get-ChildItem -` → parameters (from history/manual only in
  Phase 1).

## 4. Local shell identity — the authoritative source

For a **local** session the shell identity is known exactly: `local-shell` spawns
the PTY from a `LocalShellConfig { kind, .. }`. That `ShellKind` is threaded to the
`TerminalPanel` and handed to the engine as `ShellFamily::from_kind(kind)`. No
guessing needed.

`Pwsh`/`PowerShell` → `PowerShell` family: Phase 1 has an empty `external/powershell`
catalog (the cmdlet/verb/parameter space is enormous and low-value as a static
list), so PowerShell prompts lean on `memory` + hand-authored `manual` — but they
also search the `cmd`, `windows`, and `common` categories
([02](02-data-sources.md) §4.1), so cmd commands, Windows utilities, and
cross-platform tools all complete. A future phase can generate a full cmdlet catalog
(see [09](09-roadmap-risks.md)).

## 5. `Custom` and unknown shells

`ShellKind::Custom` (and any case where the kind is ambiguous) resolves the family
heuristically, in order:

1. **Program name** of the resolved executable: if it ends with `cmd`(.exe) →
   `Cmd`; `powershell`/`pwsh` → `PowerShell`; `bash`/`zsh`/`sh`/`fish`/`dash`
   → `Unix`.
2. **OS default** if the program name is uninformative: Windows host → `Cmd`,
   otherwise `Unix`.
3. The user can **override** the detected family per session via settings
   (`CompletionConfig.force_family`, see [06](06-configuration.md)).

## 6. Remote SSH shell detection

An SSH session runs a shell on the **remote** host; the local `ShellKind` does not
apply. Detection strategy, best-effort and non-blocking:

1. **OSC 133 + prompt shape (primary).** If the remote shell emits shell
   integration, the prompt/command structure confirms a POSIX-style shell → `Unix`.
   This is the common case for Linux/macOS SSH targets.
2. **Announced shell (opportunistic).** If OneTerm's SSH integration snippet is
   installed on the remote (the same mechanism that sets `PROMPT_COMMAND` for OSC
   7/133), it can also emit the shell name; when present, use it directly.
3. **Heuristic default.** SSH targets are overwhelmingly Unix, so the default
   family for an SSH session is **`Unix`** (coreutils catalog) unless (1)/(2) say
   otherwise or the user overrides it. Windows-over-SSH (OpenSSH on Windows) is
   rare; the user override handles it.

Rationale: OneTerm must never *block* on a network round-trip to detect the shell
(that would stall the prompt). Defaulting to `Unix` for SSH and refining via the
signals OneTerm already parses (OSC 133) gives correct behavior for the vast
majority of sessions with zero added latency.

## 7. Family changes mid-session

A shell family is fixed for a local session but can, in principle, change (e.g. the
user runs `bash` inside `cmd`, or `wsl`). Phase 1 keeps the family **fixed to the
session's spawn kind** for simplicity; nested-shell detection via OSC 133 prompt
changes is a [roadmap](09-roadmap-risks.md) item. History is already partitioned by
family, so a wrong guess only means slightly-off command suggestions, never
incorrect option triggers for what the user is actually typing (the trigger set is
a superset-tolerant match — `/` and `-` both recognized on Windows).
