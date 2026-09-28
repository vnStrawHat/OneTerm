# Low-Level Design: Sixel placement release frees its extras entries

Intake: IN-0045
HLD: [high-level-design.md](../high-level-design.md)
Topic: `oneterm-vt` extras table growth from a resent Sixel image, and what a placement's release
now does to the entries its cells held
Date: 2026-09-28

## Concern

`BUG-0079` fixed the same extras table filling from repainted `OSC 8` links, by keying an implicit
link's id to its URI so a resend reuses the same `HyperlinkId`. Its own Evidence and Gaps (F7) found
that Sixel images share the collateral but not the fix: `place` interns a fresh `Extras { graphic:
Some(id), .. }` for **every** placement, because `id` itself (`GraphicId`) is a fresh counter value
every time, even when the resent bytes are byte-for-byte the same image. A program that redraws a
Sixel preview every frame — a file manager, an Ink-style TUI — therefore took one permanent extras
entry per resend, same as the hyperlink case, until the table filled at 65,535 entries and no later
link or image got its cells, until `RIS`.

## Design

**Content-hash reuse (BUG-0079's fix, generalised) does not fit graphics.** Deduplicating by the
image's pixel bytes would let two placements share one `GraphicId`. `GraphicsState::placement`
resolves an id to a placement with `.find(|p| p.id == id)` — the **first** match — so if a program
ever shows the same image at two positions **at once** (a real case: two identical icons side by
side), sharing an id would silently give the second occurrence the first one's anchor, cols and
rows. Hyperlinks tolerate two simultaneous occurrences of one URI merging (`osc8-interning.md`
accepts merged hover groups as a UX simplification); graphics do not tolerate two simultaneous
placements merging into one, because the painter would draw the wrong rectangle. Content-hash reuse
is therefore rejected outright, not chosen as the fallback the coordinator's brief allowed for.

**Chosen: release the extras entry when its placement is released.** `GraphicId` stays exactly as it
is — a fresh counter value per placement, never reused, never renumbered — so there is no ambiguity
between simultaneous placements. What changes is what happens when a placement dies. The engine
already detects that moment twice:

1. `sweep`, at the end of `feed`, when no row in a placement's extent still carries
   `RowFlags::HAS_GRAPHIC` (a row reset, a history trim, a reflow that drops the anchor).
2. `evict_oldest`, inside `place`, when `MAX_PLACEMENTS` (256) live placements would be exceeded —
   the case a redraw loop that keeps overwriting the same still-`HAS_GRAPHIC` rows falls into,
   because `sweep` never sees that placement's flag go false (a documented false positive, unchanged
   by this packet).

Both already knew the placement was gone; neither told the extras table. Both now go through one
`release` function that frees every extras entry the placement's cells resolved to before queuing the
`GraphicReleased` event.

**Freeing needs more than the one `graphic_only` entry `place` computes.** `stamp` merges a covered
cell's existing hyperlink into a second, distinct extras value (`Extras { hyperlink: Some(h), graphic:
Some(id) }`) when the cell already carried one; a placement can therefore own several extras entries,
not one. `GraphicsState::extras_by_graphic: FxHashMap<GraphicId, FxHashSet<ExtrasId>>` records every
extras id a placement's own `place`/`stamp` calls interned for it, as they are created — no grid scan
is needed at release time, only a map lookup and remove.

**`InternTable` gains a free list, not a sweep.** `free(id)` resets the slot at `id` to the value's
default, removes it from the content-hash index, and pushes `id` onto a small `free: Vec<u16>`;
`intern` checks that list before growing the table. No other id moves or changes meaning — the design
rule the module doc already states ("styles and extras never move") is about renumbering, and this is
not a renumbering: it is the *owner* of one id proving, without help from the table, that nothing
reaches it any more, and handing it back. Id 0 (the default) can never be freed. The invariant
`entries.len() == index.len() + free.len()` replaces the old `entries.len() == index.len()`, checked
in `debug_assert_integrity` on every mutation, including `free`. `free` is generic on `InternTable<T>`
but only ever called on the extras table; nothing calls it on styles.

**Why this is safe against the render-copy hazard the module doc warns about.** The hazard the "no
renumbering sweep" rule exists for is a stale id resolving to someone else's value while a render copy
still holds it. `SnapshotRow` resolves `cell.extras_id()` into `hyperlink: Option<HyperlinkId>` and
`graphic: Option<GraphicId>` **while the lock is still held**, and never stores a raw `ExtrasId` past
that point — confirmed by reading `snapshot/row.rs`. A freed slot's id therefore never has to resolve
correctly for anyone after the lock that saw it released, because nothing outside the lock ever held
the raw id to begin with. Freeing degrades a freed-but-not-yet-reused slot to the default value rather
than leaving the stale one in place, so even a caller that mishandles this invariant sees "nothing"
instead of a different placement's data.

**Ids visible to embedders are not renumbered.** `GraphicId` keeps its counter semantics unchanged —
per terminal, starts at 1, never reset, never reused. `ExtrasId` is the id freed and reused, and it is
`pub`, but nothing outside the lock ever holds one (previous paragraph), so no embedder-visible value
changes meaning underneath anything that still references it.

## Interfaces

No public signature changes. `InternTable::free` is `pub(crate)`, mirroring `InternTable::clear`.
`GraphicsState::extras_by_graphic` and `GraphicsState::track_extras` are private to the graphics
module.

## Edge Cases and Failure Modes

- [x] Two distinct images placed at once never share an extras entry (identity stays the fresh
  `GraphicId`, unaffected by this packet).
- [x] A released placement's freed extras entries are reused by the next distinct image, not
  re-grown.
- [x] `RIS` still empties the whole extras table at once (`InternTable::clear`, now also clearing the
  free list); `GraphicsState::reset` (the `RIS` handler) clears `extras_by_graphic` in the same call,
  so the sweep the reset's own row-blanking triggers at the end of that `feed` never tries to free an
  id the wholesale clear already invalidated.
- [x] A cell that carries both a hyperlink and a graphic frees its merged entry too, not only the
  graphic-only one.
- [x] A redraw loop that never resets the rows it overwrites (the `HAS_GRAPHIC` false positive) is
  still bounded, by `MAX_PLACEMENTS` eviction rather than by the sweep.

## Verification

- [x] `intern::tests`: a freed id is the next one `intern` reuses; freeing id 0 is a no-op.
- [x] `graphics::tests`: two distinct images stay distinct; a released placement's extras entry is
  reused by a later, different image.
- [x] `terminal::tests`: 70,000 resends of one Sixel image at a fixed cursor position keep the extras
  table bounded near `MAX_PLACEMENTS`, the last resend still places, and a later explicit link and a
  later distinct image both still get their cells.
