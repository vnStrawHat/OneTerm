## Regular expressions, behind a feature

The `regex` feature adds `SearchPattern::Regex(&regex::Regex)` and with it the
`regex` crate and its three dependencies. It is off by default because the
default dependency set is a promise.

```rust
use std::time::Instant;
use oneterm_vt::search::{GridText, SearchOptions, SearchPattern, search_grid_text};
use oneterm_vt::{Config, EventBatch, Size, Terminal};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
term.feed(b"error 404 not found", &mut EventBatch::new(), Instant::now());

let text = GridText::from_terminal(&term);
let pattern = regex::Regex::new(r"error\s+\d+").unwrap();
let hits = search_grid_text(&text, SearchPattern::Regex(&pattern), SearchOptions::default());
assert_eq!(hits.len(), 1);
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
