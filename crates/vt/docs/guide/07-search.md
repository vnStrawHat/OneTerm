# 7. Searching the scrollback

Search is two phases, and the split is the whole design. `GridText::from_terminal`
copies one `char` per cell for the entire grid -- history and viewport -- and is
the only phase that needs the terminal. `search_grid_text` then matches against
that copy, so a search over a million rows of scrollback never holds the lock
your byte pump wants.

The copy is deliberate. Search is user-initiated and rare, unlike a per-frame
snapshot, so paying one copy to keep the engine available is the right trade.

```rust
use std::time::Instant;
use oneterm_vt::search::{GridText, SearchOptions, SearchPattern, search_grid_text};
use oneterm_vt::{Config, EventBatch, Size, Terminal};

let mut term = Terminal::new(Size { rows: 2, cols: 11 }, Config::default());
term.feed(b"Hello World", &mut EventBatch::new(), Instant::now());

// Phase one, under your lock.
let text = GridText::from_terminal(&term);

// Phase two, without it.
let hits = search_grid_text(&text, SearchPattern::Literal("world"), SearchOptions::default());
assert_eq!(hits.len(), 1);
assert_eq!(hits[0].start_col, 6);
assert_eq!(hits[0].end_col, 11);
```

## What a match means

A `SearchMatch` names a **row identity**, not a coordinate: `row` is a `RowId`,
which keeps the match correct while new output scrolls the grid underneath it.
That is the point of publishing an id rather than a line number, and it is what
lets a search overlay survive a busy stream.

Turn one into something you can draw with the two helpers:

- `grid_line(screen_top)` is the match's row relative to the viewport top at
  scroll offset zero. Negative is scrollback: `-1` is the newest history row.
- `display_row(screen_top, display_offset)` adds the current scroll offset and
  gives a zero-based viewport row. It may be negative or past the bottom when
  the match is scrolled out of view, so filter before you draw.

`start_col` and `end_col` are cell columns, zero-based, `end_col` exclusive.
Matching is character-based -- one `char` per cell -- so a column index is a
cell index and never a byte offset.

Neither pattern form matches across a row boundary, wrapped continuations
included. Each grid row is matched on its own. That is a real limitation: a
command line that wrapped is two rows, and a search for the whole of it finds
nothing.

## Literal search, with no extra dependency

`SearchPattern::Literal` needs no feature and no dependency. `SearchOptions`
carries two flags for it:

```rust
use std::time::Instant;
use oneterm_vt::search::{GridText, SearchOptions, SearchPattern, search_grid_text};
use oneterm_vt::{Config, EventBatch, Size, Terminal};

let mut term = Terminal::new(Size { rows: 1, cols: 20 }, Config::default());
term.feed(b"cargo and cargofmt", &mut EventBatch::new(), Instant::now());
let text = GridText::from_terminal(&term);

let loose = search_grid_text(&text, SearchPattern::Literal("cargo"), SearchOptions::default());
assert_eq!(loose.len(), 2);

// `SearchOptions` is `#[non_exhaustive]`, so build it by mutating a default
// rather than with a struct expression.
let mut options = SearchOptions::default();
options.whole_word = true;

let whole = search_grid_text(&text, SearchPattern::Literal("cargo"), options);
assert_eq!(whole.len(), 1, "cargofmt is not the word cargo");
```

Case-insensitive matching -- the default -- uses ASCII case folding, which is
1:1 per character, so column positions stay exact. It covers the dominant
terminal case: commands, logs and paths are ASCII. A word character is ASCII
alphanumeric or `_`. An empty literal finds nothing.

## Regular expressions, behind a feature

The `regex` feature adds `SearchPattern::Regex(&regex::Regex)` and with it the
`regex` crate and its three dependencies. It is off by default because the
default dependency set is a promise.

```rust,ignore
// `ignore`: `SearchPattern::Regex` does not exist in a default build, and the
// same chapter text is compiled with the feature both on and off.
use oneterm_vt::search::{GridText, SearchOptions, SearchPattern, search_grid_text};

let text = GridText::from_terminal(&term);
let pattern = regex::Regex::new(r"error\s+\d+").unwrap();
let hits = search_grid_text(&text, SearchPattern::Regex(&pattern), SearchOptions::default());
```

Three things to know before you reach for it.

**`SearchOptions` is ignored for a regex, in every build.** The pattern owns its
own case folding and word boundaries -- write `(?i)` and `\b` -- so the two
fields would be a second, conflicting spelling of the same thing. A debug
assertion fires when either is non-default, so the mistake is loud while you
test and silent in release.

**A row is exactly as wide as the grid.** `^` and `$` therefore anchor to the
first and last *cell*, not to the end of the text somebody typed, and the blank
cells to the right of the last glyph are spaces. A wide glyph's spacer cell
reads as `'\0'`.

**Compilation is yours**, and so is any `RegexBuilder::size_limit`. The crate
takes a reference to a finished `Regex` rather than a pattern string, which is
what keeps a hostile pattern from becoming this crate's problem. A pattern that
can match the empty string yields one zero-width match per position, including
the position past the last cell, so a match from that variant can be empty and
can start one column past the end of the row -- safe as a half-open range, but
skip the empty ones before you index a row to paint a highlight.

`SearchPattern` is `#[non_exhaustive]`, so a `match` on it always needs a
wildcard arm. That is what lets the same code compile whether or not the feature
is on.

## Searching a live grid

Nothing stops the terminal moving between phase one and phase two -- that is the
point of the split. What you get is a search over the grid as it stood at the
copy, with matches keyed to row ids that are still valid for as long as those
rows are live. Rows that fell out of history in the meantime announce themselves
as `VtEvent::RowsTrimmed`, and a match on a trimmed row simply has no viewport
position any more. Re-copy and re-search when the user asks for the next match
and the stream has moved; do not try to patch the old result.
