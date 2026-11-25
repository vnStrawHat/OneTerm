# 04 — Suggestion engine

> Part of [Terminal auto-completion design](../auto-completion.md). How the
> gpui-free engine turns the current input line into a ranked, deduped suggestion
> list: line parsing, command vs option context, matching, and ranking.

## 1. Responsibility

`oneterm_completion::Engine::suggest(history, ctx, cfg) -> Vec<Suggestion>` is a
**pure function** of its inputs (loaded catalogs + the passed history + the
context). No I/O, no gpui, no clock reads beyond frecency timestamps supplied by
the caller. This makes it fully unit-testable (see §7).

Inputs:

- `history: &CompletionHistory` — the global, cross-tab ring buffer.
- `ctx: &CompletionContext { family, line, cursor_col }` — the current prompt line.
- `cfg: &CompletionParams` — user config projection (enabled kinds, min prefix
  length, max items, fuzzy on/off).

## 2. Input-line parsing

The engine parses `ctx.line` up to `cursor_col` into:

- **`head`** — the first token on the line (the command name), e.g. `dir` in
  `dir /Q`. Used to look up options in catalogs.
- **`token`** — the token currently under the cursor (the thing being edited):
  the run of non-whitespace characters ending at `cursor_col`. This is what we
  match against.
- **`token_start`** — byte offset where `token` begins (drives `match_start` and
  the accepted-remainder computation).

Tokenization rules (Phase 1, intentionally simple):

- Split on ASCII whitespace.
- Respect single/double quotes so `"C:\Program Files"` is one token (prevents a
  space inside a quoted path from being read as a new token).
- A trailing space means the cursor starts a **new empty token** → in option
  context this lists *all* options of `head`; in command context it does nothing
  (we do not suggest every command in the catalog on an empty prompt unless
  `cfg.suggest_on_empty` is set — default off to avoid noise).

```rust
struct ParsedLine<'a> {
    head: Option<&'a str>,   // command name (first token), if any
    token: &'a str,          // token under cursor (may be empty)
    token_start: usize,      // byte offset of `token` in `line`
    is_first_token: bool,    // cursor is editing the command name itself
}
```

## 3. Command context vs option context

The engine decides which candidate set to draw from:

```
resolve (active_node, path) from the tokens left of the cursor   # see 10 §3

if token starts with one of family.option_triggers()  →  OPTION context      (options of active_node)
else if is_first_token                                 →  COMMAND context     (top-level commands)
else if active_node has subcommands                    →  SUBCOMMAND context  (children of active_node)
else                                                   →  ARGUMENT context    (Phase 1: history only)
```

Command trees (e.g. `git commit`, `git remote add`) are resolved by walking the
catalog from the top-level command down to the **active node**; the full algorithm,
option inheritance, and examples live in
[10-subcommands.md](10-subcommands.md).

### 3.1 Command context (`is_first_token`)

Match `token` against:

- **History** command lines / first tokens (tag `H`).
- **Catalog** command names for the family (tag `C`) — from `manual` then
  `external`.

Example: `d` → `date C`, `dir C`, `del C`, plus any `H` history commands starting
with `d` (`docker`, `deploy.sh`, …) ranked by frecency.

### 3.2 Option context (`token` starts with a trigger)

1. Resolve the **active node** — the deepest command/subcommand matched by walking
   the catalog over the tokens left of the cursor ([10](10-subcommands.md) §3). For
   a flat command this is just the command itself. If the command is unknown to any
   catalog, fall back to **history-derived options**: options the user has used with
   that command this session (tag `H`).
2. Match `token` (including its `/`/`-`/`--` prefix) as a prefix of the active
   node's options — merged with ancestor options ([10](10-subcommands.md) §3.2) —
   (tag `O`).

Example: `dir /` → `/A O`, `/B O`, `/Q O`, …; `dir /Q` narrows to `/Q O`.
Example: `grep --` → `--all O`, `--color O`, … (long options only); `grep -` also
includes `-a`, `-i`, ….
Example: `git commit --` → `--amend O`, `--no-verify O` (commit's options, not
git's globals first — see [10](10-subcommands.md) §3.2).

### 3.3 Subcommand context

The cursor is on a non-option token and the active node **has subcommands**. Match
`token` against the active node's child subcommand names (tag `C`), plus history.
Example: `git ` → `commit C`, `checkout C`, `remote C`, …; `git remote a` →
`add C`. Full design in [10-subcommands.md](10-subcommands.md).

### 3.4 Argument context

The cursor is on a non-first token that is not an option (e.g. a filename). Phase 1
offers **history-only** suggestions (whole prior command lines whose corresponding
token matches). Path/value completion is **out of scope** (skipped —
[09](09-roadmap-risks.md) §3.3).

## 4. Matching and ranking

### 4.1 Matching

- **Prefix match** is the primary mode: case-insensitive for `Cmd`/`PowerShell`
  families, case-sensitive for `Unix` (POSIX commands are case-sensitive).
- **Fuzzy match** (subsequence) is applied as a secondary pass when
  `cfg.fuzzy` is on and prefix matching yields few results; fuzzy hits rank below
  prefix hits. Reuse a small fuzzy scorer (e.g. the same style used elsewhere in
  the UI) rather than inventing one.
- `match_start` / `match_len` record the matched span so the UI can highlight it
  ([05](05-ui.md) §4). For prefix matches this is the leading `token.len()` chars.

### 4.2 Ranking (score, higher = better)

A weighted blend, computed per candidate:

```
score = w_kind   * kind_weight(kind)
      + w_frec   * frecency(candidate)         // history only; 0 for catalog
      + w_prefix * prefix_bonus(match)          // exact-prefix > fuzzy
      + w_len    * short_bonus(text.len())      // shorter completions rank a bit higher
```

- **Kind weight:** `History > manual Command/Option > external Command/Option`.
  History wins ties because "the user did this before" is the strongest signal.
- **Frecency:** `frecency = use_count * recency_decay(now - last_used)`. Recent +
  frequent history entries float to the top. `now` is passed in by the caller so
  the engine stays clock-free.
- **Prefix bonus:** exact prefix matches beat fuzzy subsequence matches.
- **Short bonus:** mild preference for shorter completions to surface the common
  case first (`ls` before `lsblk`).

Weights live in `CompletionParams` with sensible defaults; they are not
user-exposed in Phase 1 (tunable in code / tests).

### 4.3 Dedup

After scoring, collapse candidates with identical `text` (case-normalized per
family), keeping the highest-scoring one and the highest-precedence tag
([02](02-data-sources.md) §6). This is what makes an often-used catalog command
appear once, tagged `H`.

### 4.4 Truncation

Sort by score descending, then take the top `cfg.max_visible_items` (default 8).
The UI scrolls if the user pages beyond the visible window, but the engine caps the
returned vector to a bounded multiple (e.g. `4 × max_visible_items`) to keep render
cost predictable.

## 5. Accept: computing the remainder

On accept the caller needs the exact bytes to send to the PTY. The engine exposes:

```rust
impl Suggestion {
    /// The bytes to append to the PTY given the token the user already typed.
    /// e.g. suggestion "dir" with typed "di" → "r"; "/Q" with typed "/" → "Q".
    pub fn remainder<'a>(&'a self, typed_token: &str) -> &'a str;
}
```

For an exact-case prefix, acceptance appends `self.text` with the typed prefix
stripped. Unix requires this exact-case relationship. Cmd and PowerShell match
prefixes case-insensitively, but acceptance must reproduce the selected suggestion
exactly: if casing differs, the terminal-view controller sends plain Backspace
bytes from the first differing character through the cursor, then writes the exact
suggestion suffix from that point (`cd p` + `cd Project` → `cd Project`). This is a
bounded case correction within `Suggestion::replace_from`, not fuzzy acceptance.

If the suggestion is not a prefix under the active family's case rule (a fuzzy or
non-prefix match), Phase 1 does not accept it unless `cfg.allow_fuzzy_accept` is
explicitly enabled (default off).

## 6. Suppression / empty results

`suggest` returns an empty vector (→ overlay hidden) when:

- `token.len() < cfg.min_prefix_len` in command context (default 1; the request's
  "type `d`" example implies 1).
- No candidate matches.
- Controller post-processing finds exactly one suggestion whose text is already
  byte-identical to the typed text in its `replace_from..cursor` range. Multiple
  results, prefix extensions, and case-only differences remain visible.
- The caller already decided completion is gated off (alt-screen / not at prompt) —
  though the caller typically skips calling `suggest` entirely in that case (see
  [06](06-configuration.md) §3).

## 7. Testability

The engine crate ships unit tests covering:

- Parsing: quoted tokens, trailing space, cursor mid-token.
- Context selection: first-token → command; `/`/`-`/`--` → option; family-specific
  triggers.
- Matching: case sensitivity per family; prefix vs fuzzy ordering.
- Ranking: history frecency beats catalog; dedup keeps the `H` tag.
- Redaction integration ([08](08-security-redaction.md)): a recorded secret line
  never produces a suggestion containing the secret value.
- Fixtures live under `crates/completion/tests/fixtures/` and small inline
  catalogs in the engine tests.
