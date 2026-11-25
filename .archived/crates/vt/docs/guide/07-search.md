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

## Searching a live grid

Nothing stops the terminal moving between phase one and phase two -- that is the
point of the split. What you get is a search over the grid as it stood at the
copy, with matches keyed to row ids that are still valid for as long as those
rows are live. Rows that fell out of history in the meantime announce themselves
as `VtEvent::RowsTrimmed`, and a match on a trimmed row simply has no viewport
position any more. Re-copy and re-search when the user asks for the next match
and the stream has moved; do not try to patch the old result.
