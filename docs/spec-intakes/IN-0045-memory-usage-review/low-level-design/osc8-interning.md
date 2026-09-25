# Low-Level Design: OSC 8 implicit-link interning

Intake: IN-0045
HLD: [high-level-design.md](../high-level-design.md)
Topic: `oneterm-vt` `HyperlinkTable` identity for links without `id=`, and what else a full
extras table takes down
Date: 2026-09-25

## Concern

`HyperlinkTable::intern(None, uri)` issued a fresh `HyperlinkId` on every `OSC 8` open, and
`set_hyperlink` interned a fresh `Extras` value for it. A program that repaints a line holding
one link (Ink-style TUIs, `claude`) therefore consumed one entry in each table per repaint
until both were full at 65,535 (`BUG-0079`, measured in
[`research/agent-load-phase-4.md`](../research/agent-load-phase-4.md) section 5).

## Design

**Identity key for an implicit link: the URI, exactly as the stream spelled it** (after the
`;` rejoin the dispatcher already does). The engine parses no `OSC 8` parameter other than
`id=` and stores none, so parameters cannot be part of the key. The table keeps a second map,
`implicit: FxHashMap<Box<str>, u32>`, from URI to the id first issued for it. An implicit
open looks the URI up there first; only a URI never seen before takes a new entry and a new
implicit counter value. Explicit links keep their `(id, uri)` key in the existing `index` map.

Consequences:

- Repainting is idempotent: the same `HyperlinkId`, hence the same `Extras` value, hence the
  same `ExtrasId`. The tables grow only with distinct URIs (and distinct explicit ids), still
  bounded by the same 65,535 cap and the same drop-the-attribute ladder.
- Ids stay stable. No id is renumbered, so nothing an embedder holds changes meaning.
- The two maps are separate, so an implicit link can no longer be matched by an explicit
  `id=<n>` whose `<n>` equals an implicit counter value (the overlap the `Hyperlink::implicit`
  flag already documents). Before, `OSC 8 ; id=1 ; X` sent after the first implicit link to
  `X` resolved to that implicit entry.
- **Merged hover groups (accepted).** Two separate implicit occurrences of one URI now share
  an id. The only consumer that groups by id is `terminal-view`'s hover, which walks a
  contiguous same-id run **within one row** (`crates/terminal-view/src/url/detect.rs`). It
  merges two occurrences only when they touch on the same row with no unlinked cell between
  them; the merged span opens the same URI either way, and its label is the two labels
  joined. A program that wants separate groups for one URI has the spec's tool for it: `id=`.

**Rejected: option B, sweeping unreferenced ids** (remap live cells the way the grapheme arena
is swept). It keeps per-occurrence identity, but it renumbers `HyperlinkId` and `ExtrasId`
values across a sweep, which breaks the no-move rule in `intern.rs` ("styles and extras never
move") in a way embedders can see, costs a scan of grid and history, and still lets a repaint
loop churn the tables between sweeps. Option A removes the growth at its source with one map
lookup.

**`RIS` resets the extras table too.** `RIS` blanks both screens, clears both histories, and
resets both cursors' pens and saved cursors; the graphics placements die through the ordinary
release sweep because every row they cover was blanked. After it no cell and no pen holds an
`ExtrasId` other than 0, so the whole table (link and image entries alike) is unreferenced and
is reset to its default, keeping the exhaustion counter and the warn-once flag as session
telemetry. This is the reasoning `HyperlinkTable::clear` already relies on; the two are
cleared together so a link-owned extras entry cannot outlive its link. Snapshots are
unaffected: they resolve extras to values under the lock. `DECSTR` (soft reset) keeps the
screen, so it clears neither table.

**A full extras table and images.** Images still take their extras entry from the shared
table (one entry per image, `R-21`); making the graphic independent of that table would need
a cell bit or a second id space, which is not a small change. With the link growth gone the
table fills only from 65,534 distinct link and image combinations. When it does fill,
placement logs a dedicated warning once per terminal instead of relying on the generic extras
warning, and the behaviour stays as before: the image is decoded and handed to the embedder,
but no cell carries it, so it is not painted. Until the next `RIS`.

## Interfaces

No public signature changes. `HyperlinkTable::intern(id: Option<&str>, uri: &str)` keeps its
shape; its rustdoc states the new identity rule. `InternTable::clear` is `pub(crate)`.

## Edge Cases and Failure Modes

- [x] Two different implicit URIs stay distinct.
- [x] Explicit ids with different URIs stay distinct; explicit `id=1` never aliases implicit
  link `1`.
- [x] A full hyperlink table still drops the attribute (unchanged ladder).
- [x] `RIS` releases both tables.

## Verification

- [x] `intern::tests` and `terminal::tests`: 70,000 repaints keep both tables at a constant
  size, the link resolves, an explicit link and a Sixel image placed afterwards still work,
  and `RIS` releases both tables.
