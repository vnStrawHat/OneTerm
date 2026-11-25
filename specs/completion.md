# Completion

The engine of OneTerm's command completion, in `crates/completion` (package
`oneterm-completion`): from the line at the prompt and its cursor, a short ranked list of what
may be typed next. It has no view; the terminal's overlay asks it and draws what it gives.
OneTerm 0.6's completion engine (`.archived/crates/completion` of this
repository) is the reference for behaviour; the crate has its name since 2026-11-25
(`oneterm-complete` before).

## Families

`Family::{Cmd, PowerShell, Unix}`, the kind of shell a line is typed into. It decides:

- the catalogs searched: Cmd and PowerShell search `windows`, then `common` (then `unix` with
  `windows_allow_coreutils`); Unix searches `unix`, then `common`;
- what starts an option: `/` or `-` for Cmd, `-` for the others;
- case: names and options match without case for Cmd and PowerShell, with case for Unix.

`Family::detect(text)` reads a program's path (its file name) or a shell's name by its words:
`pwsh` or `powershell` is PowerShell, `cmd` or "command prompt" is Cmd, anything else (bash,
zsh, WSL, an SSH session) is Unix. A live title is no good to it (one showing a folder named
`cmd` reads as Cmd): the app passes the program it launched or its menu label. `Family::parse` reads a setting's name (`cmd`, `powershell`, `pwsh`,
`unix`, `bash`, `zsh`, `sh`). `Config::family(text)` is `force_family` when it is set, else
the detected one.

## Catalogs

A catalog file is a command: `{ "schema": 1, "name", "options": [..], "subcommands": [..] }`,
subcommands nested to any depth. An option is a string (`"-a"`) or `{ "flag", "description" }`,
the description a short hint shown after it (`-b new-branch`). A schema above 1 is refused;
unknown keys are ignored.

The bundled files are in `assets/<source>/<group>/<name>.json`, source `manual` (written by
hand) or `external` (generated from the tools' help), group `windows`, `unix` or `common`, each
embedded with `include_str!`; a test holds the list to the folder. `Catalog::bundled()` reads
them all when the engine is made; `Catalog::add` takes another file and refuses a malformed one without touching the
rest. A command is found by its name in the family's groups: manual before external, then the
earlier group.

## A suggestion

`Engine::suggest(line, cursor, family)` parses the line left of the cursor (a byte offset,
moved back to a character's start) into quote-aware tokens and answers by what the cursor is on:

| The cursor is on | Candidates |
|---|---|
| the first token | the catalog's commands and the first words of the history |
| an option token after a command | the active node's options, then its ancestors' (lower); for a command no catalog knows, the options it was run with in the history |
| a later token of a node with subcommands | the node's subcommands and whole lines of the history |
| any other token | whole lines of the history that start with what is typed |

The active node is found by walking the tokens after the command: an option is passed over, a
known subcommand is gone into, anything else stops the walk.

Each candidate matches by prefix, or, with `fuzzy`, as a subsequence. All prefix matches rank
above all fuzzy ones; within each, the score is `10 * source + 6 * frecency + 1 / (1 + len)`,
source 3 for the history, 2 for manual, 1 for external, an ancestor's option at frecency -1.
The same text twice is kept once, with the stronger source and the description either has. A
history line that looks like a secret is dropped (`redact_sensitive`). In the command context
nothing is suggested before `min_prefix_len` characters, unless the trigger asks
(`suggest_now`). The list is at most four times `max_visible_items`, which the overlay shows at
once; a list of one that is already what is typed is empty.

`Suggestion { text, range, kind, source, detail, matched, prefix }`: `range` is the bytes of the line
it replaces (the token, or the line from its first non-blank for a history line), `kind` is
`Command`, `Subcommand`, `Option` or `Line`, `source` is `History`, `Manual` or `External`,
`matched` the bytes of `text` that match by prefix (0 for fuzzy). `Suggestion::edit(line)` is
what accepting it types: characters to erase before the cursor, then text. A prefix match
erases nothing; one that differs in case erases from the first difference; `prefix` says
whether it extends what is typed, and the overlay refuses one that does not.

## History

`Engine::record(line, family)` keeps a line that was run, per family, in the engine's memory
alone: never written, gone when the app exits. Nothing is kept when the engine is off, the
memory source is off or `max_history` is 0. With `redact_sensitive` the line is redacted
first: the command and every flag stay, the values of secret flags (`--password x`,
`/PASSWORD:x`, `mysql -pX`, `curl -u a:b`, `docker login -p x`), secret assignments
(`AWS_SECRET_ACCESS_KEY=..`), credentials in URLs and tokens shaped like keys (known prefixes,
JWTs, long high-entropy words) go. A line run again is counted, not added twice; past
`max_history` the least recently used goes. Time is the count of lines recorded: frecency is
`uses / (1 + age / 100)`, age the lines recorded since its last use.

The app makes one engine and shares it between its tabs, so the history is the session's;
`suggest` takes `&self`.

## Config

`Config` has the user's settings, with their defaults: `enabled` (on), `accept_tab` (on),
`max_history` (500), `min_prefix_len` (1), `max_visible_items` (8), `sources { memory, manual,
external }` (all on), `fuzzy` (on), `inherit_ancestor_options` (on), `disable_in_alt_screen`
(on), `require_prompt_region` (on), `windows_allow_coreutils` (off), `force_family` (none),
`redact_sensitive` (on). `accept_tab`, `disable_in_alt_screen` and `require_prompt_region` are
the view's: when it shows the list and what Tab does. `Engine::set_config` applies a change at
once; a smaller `max_history` drops the least recently used.
