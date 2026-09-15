# Low-Level Design: Input encoding and scrollback search

Intake: IN-0038
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: encoding-and-search
Date: 2026-09-15

## Concern

Moving `key_encode.rs`, `mouse_encode.rs` and `search.rs` from `crates/terminal` into
`crates/vt`, the optional `regex` feature that comes with search, which tests travel with the code,
and what `crates/terminal` keeps.

## Why these three and not others

`crates/terminal` holds 5 700 production lines. Three files in it are pure terminal semantics with
no OneTerm in them:

| File | Lines (with tests) | OneTerm-specific content | Already depends on `oneterm-vt` |
| --- | ---: | --- | --- |
| `key_encode.rs` | 572 | none. `KeySpec`, `KeyMods`, `NamedKey` are deliberately neutral types; its own module doc says "no dependency on `keyboard_types` or GPUI" | no (it has **zero** imports) |
| `mouse_encode.rs` | 475 | none. It reads `ModeSnapshot::mouse` and emits X10 / 1005 / SGR-1006 bytes | yes (`ModeSnapshot`, `MouseEncoding`) |
| `search.rs` | 418 | none. It copies grid text under the caller's lock and matches characters, keyed by `RowId` | yes (`CellWidth`, `RowId`, `Terminal`) |

All three answer questions only the engine can answer -- "what does `? 1006` mean for this click",
"what bytes does Ctrl+Left send under `DECCKM`", "which `RowId` holds this text" -- and two of them
already import the engine to do it. `key_encode.rs` is the clean case: it is a pure function from
(key, modifiers, application-cursor-mode) to bytes, and the `app_cursor` argument is engine state
the caller has to fetch and pass. After the move it can read `Mode::AppCursor` itself, and the
three-argument call becomes two.

Nothing else in `crates/terminal` qualifies. `paste.rs` (bracketed-paste plus OneTerm's sanitising),
`url_policy.rs`, `logging.rs`, `palette.rs` (theme resolution), `content.rs` (the adapter's owned row
model), `completion` support and `security_policy.rs` all encode product choices.

## Design

### `input` module

```text
crates/vt/src/input/
    mod.rs          pub use key::*; pub use mouse::*;  module docs
    key.rs          KeySpec, KeyMods, NamedKey, encode_key       (from key_encode.rs)
    key_tests.rs                                                 (from its #[cfg(test)] mod)
    mouse.rs        TerminalMouseButton, MouseModifiers, encode_mouse_*, encode_wheel_event
    mouse_tests.rs
```

Signature changes, both narrowing:

```rust
// before, in crates/terminal
pub fn encode_key(key: &KeySpec, mods: KeyMods, app_cursor: bool) -> Option<Vec<u8>>;

// after, in crates/vt::input
pub fn encode_key(key: &KeySpec, mods: KeyMods, modes: &ModeSnapshot) -> Option<Vec<u8>>;
```

`ModeSnapshot` already carries application-cursor state and is already what `mouse_encode` takes, so
one argument type serves both and a caller that has a snapshot needs nothing else. `Terminal` gains
a convenience `pub fn encode_key(&self, key: &KeySpec, mods: KeyMods) -> Option<Vec<u8>>` that reads
its own modes -- which is what an embedder wants and what removes the "fetch the mode, then encode"
dance from `crates/terminal-view/src/input/keys.rs`.

`KeyboardFlags` (Kitty keyboard protocol state) already lives in `crates/vt/src/terminal/mode.rs`
and is *not* consumed by `encode_key` today. It stays where it is; wiring the two together is a
future packet and is called out as out of scope in `US-0099` so it does not sprawl.

### `search` module

```text
crates/vt/src/search/
    mod.rs          SearchOptions, SearchPattern, SearchMatch, GridText, search_grid_text
    literal.rs      the character scanner that exists today
    regex.rs        #[cfg(feature = "regex")]
    search_tests.rs
```

`GridText` becomes `pub` (it is `pub(crate)` today) because the two-phase shape is the whole point
and an embedder needs to name the intermediate:

```rust
/// One `char` per cell for the whole grid, copied so the search itself can run
/// without the embedder's lock.
pub struct GridText { /* oldest: RowId, num_cols: usize, chars: Vec<char> */ }

impl GridText {
    /// Copy the grid. Call this under your lock; it is the only part that needs it.
    pub fn from_terminal(term: &Terminal) -> GridText;
    pub fn rows(&self) -> usize;
    pub fn oldest(&self) -> RowId;
}

/// What to look for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SearchPattern<'a> {
    /// A literal run of characters.
    Literal(&'a str),
    /// A compiled regular expression.
    #[cfg(feature = "regex")]
    #[cfg_attr(docsrs, doc(cfg(feature = "regex")))]
    Regex(&'a regex::Regex),
}

/// Match `pattern` against a copied grid. Runs without any lock.
pub fn search_grid_text(text: &GridText, pattern: SearchPattern<'_>, options: SearchOptions)
    -> Vec<SearchMatch>;
```

`SearchOptions` keeps `case_sensitive` and `whole_word`. Both apply to `Literal`; for `Regex` they
are documented as ignored (the pattern owns its own `(?i)` and `\b`), and `search_grid_text`
debug-asserts that they are default when the pattern is a regex, so an embedder finds out rather
than silently getting the wrong matches. `SearchOptions`'s doc comment loses the line "Regex is
intentionally omitted for the MVP".

The regex implementation matches **per row**, against the row's `num_cols` characters joined into a
`String`, and maps byte offsets back to column indices through a single pass. It does **not** match
across a wrapped line boundary; that limitation is documented on `SearchPattern::Regex` and is the
same limitation the literal scanner has today. Fixing it is not this intake's job.

`search_term`, the `#[cfg(test)]` convenience wrapper, moves and stays test-only.

### The `regex` feature

```toml
[features]
default = []
regex = ["dep:regex"]

[dependencies]
regex = { workspace = true, optional = true }
```

Three properties the packet must prove:

1. **Default stays at six dependencies.** `cargo tree -p oneterm-vt -e normal` is still seven lines.
2. **A feature never changes behaviour.** `search_grid_text` with `SearchPattern::Literal` returns
   identical matches with and without the feature. Enforced by running the whole literal test suite
   under both.
3. **OneTerm does not enable it.** `crates/terminal/Cargo.toml` takes `oneterm-vt` without features.
   OneTerm's search UI is literal today and stays literal; regex search is a separate future packet
   with its own UI question.

Licence impact: none in either direction. `regex 1` is already declared in root
`[workspace.dependencies]` and `regex 1.12.4` is already listed in `THIRD-PARTY-NOTICES.md` via
`oneterm-highlight`, so even if OneTerm turned the feature on, `python scripts/third-party-notices.py
--check` would not move.

### What `crates/terminal` keeps

| Kept | Why |
| --- | --- |
| `crates/terminal-view/src/input/keys.rs` and `mouse.rs` | GPUI event -> `KeySpec` / button mapping; they call the engine's encoder and keep every scroll, selection and shift-tracking side effect, which the encoder never had |
| `TerminalSession::search` and its `SessionKind` plumbing | the trait is OneTerm's contract |
| `crates/terminal-view/src/terminal_view/search.rs` | the search overlay, its highlight state and its navigation |
| `SearchMatch::display_row` callers | unchanged; the method moves with the type |
| `paste.rs`, `url_policy.rs`, `security_policy.rs`, `logging.rs`, `palette.rs`, `content.rs` | product policy; decision (f) |

`crates/terminal` re-exports the moved names from `oneterm_vt` for one release, exactly as it
already re-exports 26 engine names in `lib.rs`, so `terminal-view` compiles unchanged and the
packets stay independently reviewable:

```rust
pub use oneterm_vt::input::{KeyMods, KeySpec, MouseModifiers, NamedKey, TerminalMouseButton,
                            encode_key, encode_mouse_move, encode_mouse_press,
                            encode_mouse_release, encode_wheel_event};
pub use oneterm_vt::search::{SearchMatch, SearchOptions};
```

## Interfaces

```rust
// crates/vt/src/input/key.rs
pub struct KeyMods { pub shift: bool, pub ctrl: bool, pub alt: bool }
#[non_exhaustive] pub enum NamedKey { Enter, Backspace, Delete, Tab, Escape, ArrowUp, ..., F20 }
#[non_exhaustive] pub enum KeySpec { Named(NamedKey), Char(char), Text(String) }
pub fn encode_key(key: &KeySpec, mods: KeyMods, modes: &ModeSnapshot) -> Option<Vec<u8>>;

// crates/vt/src/input/mouse.rs
#[non_exhaustive] pub enum TerminalMouseButton { Left, Middle, Right }
pub struct MouseModifiers { pub shift: bool, pub alt: bool, pub ctrl: bool }
pub fn encode_mouse_press(modes: &ModeSnapshot, button: TerminalMouseButton,
                          mods: MouseModifiers, col: usize, row: usize) -> Option<Vec<u8>>;
pub fn encode_mouse_release(..) -> Option<Vec<u8>>;
pub fn encode_mouse_move(..) -> Option<Vec<u8>>;
pub fn encode_wheel_event(..) -> Option<Vec<u8>>;

// crates/vt/src/terminal/mod.rs
impl Terminal {
    pub fn encode_key(&self, key: &KeySpec, mods: KeyMods) -> Option<Vec<u8>>;
}

// crates/vt/src/search/mod.rs -- see Design above
```

Exact parameter lists for the four mouse functions are taken verbatim from
`crates/terminal/src/mouse_encode.rs:125-187`; the move must not change them.

## Edge Cases and Failure Modes

- [ ] `encode_key` returns `None` for Ctrl plus a non-ASCII character and for multi-codepoint text.
  Unchanged; the caller ignores it. Documented on the function rather than in a comment above it,
  because it is now public API.
- [ ] `encode_key`'s `app_cursor` came from the caller and now comes from `ModeSnapshot`. The two
  must agree at the move: `US-0099`'s first commit keeps the boolean form and adds the snapshot
  form, and a test feeds `ESC [ ? 1 h` then asserts both forms produce identical bytes for every
  arrow key. Only then does the boolean form go.
- [ ] Regex with `case_sensitive: false` or `whole_word: true` -- debug assertion plus a documented
  "ignored"; the pattern owns its own flags.
- [ ] A regex that matches the empty string -- `search_grid_text` advances at least one column per
  match so it cannot loop. Test case.
- [ ] A regex whose compilation is slow or whose match is catastrophic -- the engine takes an
  already-compiled `&regex::Regex`, so compilation cost and any `RegexBuilder::size_limit` are the
  embedder's. `regex` has no backtracking, so there is no catastrophic-backtracking class here.
- [ ] `GridText::from_terminal` on a 100 000-row scrollback at 200 columns -- 20 million `char`, 80
  MB. This is the existing behaviour and the existing risk (`R-19` deliberately keeps the copy).
  `US-0100` documents the cost on `from_terminal` in `char`s rather than leaving it implicit, and
  does **not** change it.
- [ ] Wide characters -- spacer cells are `'\0'` in `GridText`. Preserved exactly; a match cannot
  start or end on a spacer. Existing tests cover it and move with the code.

## Verification

- [ ] Every test in `crates/terminal/src/key_encode.rs::tests`, `mouse_encode.rs::tests` and
  `search.rs::tests` moves to `crates/vt` **unchanged in input and expectation**. A reviewer diffs
  the moved modules against `git show main:crates/terminal/src/<file>` and the assertion text must
  match line for line except for the `use` paths. This is the measurable acceptance criterion.
- [ ] `cargo test -p oneterm-vt` and `cargo test -p oneterm-vt --features regex` both green; the
  literal suite runs identically under both.
- [ ] `cargo tree -p oneterm-vt -e normal` is seven lines (R7 and the dependency budget).
- [ ] `cargo tree -p oneterm-vt -e normal --features regex` shows exactly `regex`,
  `regex-automata`, `regex-syntax` and `aho-corasick` added, and nothing else.
- [ ] `python scripts/third-party-notices.py --check` passes with no regeneration.
- [ ] `cargo test --workspace` green with `crates/terminal` and `crates/terminal-view` unmodified
  apart from the re-export block.
- [ ] Manual Windows walk: arrow keys and function keys in `vim` over SSH, mouse selection and wheel
  scrolling in `htop`, and a scrollback search hit navigated forwards and backwards.
