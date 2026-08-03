# Terminal auto-completion — design spec

> Design specification for the OneTerm **terminal auto-completion** feature: an
> overlay that suggests commands and options as the user types at the shell
> prompt, sourced from in-session history, a user-defined manual catalog, and
> curated external catalogs (Windows Commands + Unix coreutils).
>
> This file is the **index**. The design is split into numbered parts under
> [`docs/auto-completion/`](auto-completion/). Read [00-overview](auto-completion/00-overview.md)
> first, then follow the requirement → design map below.

## Status

- **Phase:** 1 (MVP) — command + option name suggestions, no descriptions.
- **Primary platform:** Windows (`cmd` / PowerShell / `pwsh`), then Unix shells
  (`bash` / `zsh` / `sh`).
- **New code:** a `crates/completion/` engine crate (matching + ranking +
  redaction + embedded external catalogs), a global history store in `state`, the
  overlay in `terminal-view`, and a `completion` settings group.

## Document map

| # | Part | Contents |
|---|---|---|
| 00 | [Overview](auto-completion/00-overview.md) | Motivation, **categorized user requirements**, scope, non-goals, interaction model, glossary |
| 01 | [Architecture](auto-completion/01-architecture.md) | New crates, dependency-rule (R1–R12) compliance, end-to-end data flow, key types |
| 02 | [Data sources](auto-completion/02-data-sources.md) | `memory` / `manual` / `external` sources, the per-command JSON catalog schema (one file per command), load/merge order |
| 03 | [Shell detection](auto-completion/03-shell-detection.md) | `ShellKind` → catalog family mapping, option trigger chars per shell, remote SSH detection |
| 04 | [Suggestion engine](auto-completion/04-suggestion-engine.md) | Input-line parsing, command / subcommand / option context, matching, ranking (frecency), dedup |
| 05 | [UI](auto-completion/05-ui.md) | Overlay list, cursor-aware positioning, item format `<highlight><suggest>  <tag>`, tag colors, keys |
| 06 | [Configuration](auto-completion/06-configuration.md) | `terminal.json` `completion` group, OSC 133 / alternate-screen gating, Settings UI |
| 07 | [External catalog script](auto-completion/07-external-assets-script.md) | `download` + `generate` subcommands, raw-source parsing, per-command output layout |
| 08 | [Security & redaction](auto-completion/08-security-redaction.md) | Sensitive-data detection, redaction of history values, reuse of `TerminalSecurityPolicy` |
| 09 | [Roadmap & risks](auto-completion/09-roadmap-risks.md) | Phasing, risks, and value-add **decisions** (accepted: manual trigger, recent-commands palette; rejected: ghost text, path completion, i18n, …) |
| 10 | [Subcommands](auto-completion/10-subcommands.md) | Nested command trees (`git commit`, `git remote add`) and per-subcommand options: schema, resolution, UI |
| 11 | [Implementation plan](auto-completion/11-implementation-plan.md) | Ordered build milestones (M0–M10), exit criteria, parallelism, QA checklist, definition of done |

## Requirements → design map

| # | Requirement (from the request) | Where it is designed |
|---|---|---|
| R1 | Suggest commands and options as the user types | [04](auto-completion/04-suggestion-engine.md), [05](auto-completion/05-ui.md) |
| R2 | Typing `d` shows an overlay of matching commands (`date`, `dir`…) | [04](auto-completion/04-suggestion-engine.md), [05](auto-completion/05-ui.md) |
| R3 | Typing `-` / `--` / `/` switches to option suggestions for the current command | [03](auto-completion/03-shell-detection.md) §3, [04](auto-completion/04-suggestion-engine.md) §3 |
| R4 | Detect the running shell to pick the right catalog (bash ≠ cmd) | [03](auto-completion/03-shell-detection.md) |
| R5 | `memory` source: commands typed this session, non-persistent, shared across all tabs, reset on app exit | [02](auto-completion/02-data-sources.md) §2, [01](auto-completion/01-architecture.md) §4 |
| R6 | Never suggest sensitive data (tokens, passwords, API keys); still suggest the command/option, minus the secret | [08](auto-completion/08-security-redaction.md) |
| R7 | `manual` source: user-defined catalog in a defined format | [02](auto-completion/02-data-sources.md) §3 |
| R8 | `external` source: Windows Commands + Unix coreutils, curated | [02](auto-completion/02-data-sources.md) §4, [07](auto-completion/07-external-assets-script.md) |
| R9 | Convert raw external data to one simple JSON format (no descriptions in Phase 1) | [02](auto-completion/02-data-sources.md) §5, [07](auto-completion/07-external-assets-script.md) |
| R10 | A script with at least `download` + `generate` | [07](auto-completion/07-external-assets-script.md) |
| R11 | List UI, cursor-aware placement (top/bottom) | [05](auto-completion/05-ui.md) §2–3 |
| R12 | Item format `<highlight_text><suggest_text>␣␣␣␣␣<tag>`, prefix highlighted | [05](auto-completion/05-ui.md) §4 |
| R13 | Tags History→`H`, Command→`C`, Option→`O`, each with its own background color | [05](auto-completion/05-ui.md) §5 |
| R14 | Use OSC 133 + alternate screen to detect TUIs and turn completion off | [06](auto-completion/06-configuration.md) §3 |
| R15 | Settings: enable/disable, accept-tab on/off, max command history, … | [06](auto-completion/06-configuration.md) §2 |
| R16 | Suggest additional valuable features | [09](auto-completion/09-roadmap-risks.md) §3 |
| R17 | Support apps with subcommands and per-subcommand options (e.g. `git`) | [10](auto-completion/10-subcommands.md), [02](auto-completion/02-data-sources.md) §5, [04](auto-completion/04-suggestion-engine.md) §3.3 |
| R18 | Store each command as its own JSON file | [02](auto-completion/02-data-sources.md) §5–5.2, [07](auto-completion/07-external-assets-script.md) |

## Related design docs

- [`docs/terminal-backend.md`](terminal-backend.md) — terminal engine, PTY, `ShellKind`, OSC 133 shell integration wiring.
- [`docs/terminal-semantic-highlighting.md`](terminal-semantic-highlighting.md) — OSC 133 row-role model (`RowRole`, `RowRoles`) this feature reuses.
- [`docs/agents/structure.md`](agents/structure.md) + [`docs/agents/crate-dependency-rules.md`](agents/crate-dependency-rules.md) — crate layering and the R1–R12 rules that constrain the crate layout in [01](auto-completion/01-architecture.md).
- [`docs/agents/persistence.md`](agents/persistence.md) — persistence ownership (this feature's `memory` source is deliberately **non-persistent**).
