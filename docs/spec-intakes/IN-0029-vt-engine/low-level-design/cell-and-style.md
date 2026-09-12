# Low-Level Design: Cell, style and grapheme storage

Intake: IN-0029
HLD: ../high-level-design.md
Topic: cell-and-style
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/cell.rs` and
> `crates/vt/src/intern.rs`.

## Concern

What one grid position stores, how styles and rarely-set attributes are interned, how
multi-codepoint graphemes are stored and collected, and how character width is decided.

Replaces `vendor/alacritty_terminal/src/term/cell.rs`: a 24-byte struct with an
`Option<Arc<CellExtra>>` that costs one heap allocation and one atomic refcount per decorated
cell, copy-on-write through `Arc::make_mut` on every later write, and an **unbounded**
`Vec<char>` of zero-width characters per cell.

## Design

### `Cell` — eight bytes, packed

```rust
/// 8 bytes. `Cell(0)` is a valid empty cell: a space with the default style and no extras.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell(u64);

// bits  0..21   content   Unicode scalar value, or a GraphemeId when `is_grapheme`
//       21      is_grapheme
//       22..24  width     0 Narrow · 1 Wide · 2 WideSpacer · 3 LeadingWideSpacer
//       24..26  semantic  0 None · 1 Prompt · 2 Input · 3 Output      (OSC 133)
//       26      protected                                             (DECSCA, reserved)
//       27..32  reserved  5 bits — kitty virtual placeholder, and headroom
//       32..48  style_id  u16 into the terminal's StyleSet   (0 = default, never evicted)
//       48..64  extras_id u16 into the terminal's ExtrasTable (0 = none)
```

- `content == 0` reads back as `' '`, so a zeroed row is a blank row. The engine never stores a
  literal `NUL`.
- 21 content bits cover the whole Unicode scalar range (`0x10FFFF` needs 21) and the grapheme id
  space, which are disjoint by the `is_grapheme` bit.
- `width` replaces the reference's three separate flags with one exhaustive enum, so "wide char
  with no spacer" is unrepresentable.
- **There is no `has_extras` bit** (R-34): `extras_id != 0` is the same test in one compare, and
  the freed bit joins the reserved four, giving the design five bits of headroom — its only
  headroom for Kitty placeholders.
- `WRAPPED` is a row flag, not a cell flag ([`grid-and-scrollback.md`](grid-and-scrollback.md)
  deviation G1).
- `size_of::<Cell>() == 8` is a compile-time assertion.

### `Style` — interned, `u16` id, one table per terminal

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    pub fg: Color,                        // Named | Palette(u8) | Rgb
    pub bg: Color,
    pub underline_color: Option<Color>,   // SGR 58/59 — stored, not yet rendered
    pub attrs: Attrs,                     // bitflags u16
}

bitflags! { pub struct Attrs: u16 {
    const BOLD, DIM, ITALIC, INVERSE, HIDDEN, STRIKEOUT,
          BLINK_SLOW, BLINK_FAST,
          UNDERLINE, DOUBLE_UNDERLINE, UNDERCURL, DOTTED_UNDERLINE, DASHED_UNDERLINE,
          OVERLINE;
    const ALL_UNDERLINES = UNDERLINE | DOUBLE_UNDERLINE | UNDERCURL
                         | DOTTED_UNDERLINE | DASHED_UNDERLINE;
} }
```

**Ownership (R-20).** The interner lives on `Terminal`, **not** inside a screen:

```rust
pub struct Interner {
    pub styles: StyleSet,            // content-hash interned, FxHashMap
    pub extras: ExtrasTable,
    pub graphemes: GraphemeArena,
    pub hyperlinks: HyperlinkTable,
}
```

One table pair shared by the primary and the alternate screen. This removes the aliasing problem
(`grid.interner.sweep_all([&mut grid, &mut alt])` cannot borrow-check), removes the question of
which screen owns an id after `swap_alt`, and makes a sweep a method on `Terminal` that
destructures its own fields.

**Id 0 is the default style and can never be evicted or fail to resolve.** That invariant is what
makes the ladder safe: the worst case is wrong colours, never a panic and never a lost cell.

**Overflow ladder — three steps, no sweep (R-52):**

| Step | Condition | Action |
| --- | --- | --- |
| 1 | style already interned | reuse the id |
| 2 | `entries.len() < 65_535` | insert |
| 3 | full | use id 0 (default style), `log::warn!` **once per session**, count in `FeedStats::style_table_exhausted` |

The earlier design ran a mark-and-sweep renumbering at 75 % occupancy. It is deleted. Ghostty's
telemetry says real workloads use fewer than sixteen distinct styles, so the trigger is
unreachable outside a deliberately hostile stream; meanwhile a renumbering sweep is a full
mutating walk of both screens and is the mechanism behind the design's own "a missed live
reference corrupts colours silently" risk, and behind the stale-id hazard in the render hand-off.
Step 3 is already bounded, observable and degraded-but-correct. Style ids therefore **never
change** once assigned, for the life of the terminal.

Blink and overline are stored even though the current renderer draws neither: the bits are
already allocated and today's engine drops them entirely, so a later renderer packet needs no
engine change.

### `Extras` — the rare per-cell attachments, one entry per image

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Extras {
    pub hyperlink: Option<HyperlinkId>,   // OSC 8
    pub graphic:   Option<GraphicId>,     // WHICH image — not where in it
}
```

Interned exactly like `Style`, `u16` id, id 0 means "none", same three-step ladder.

**Why the per-cell offset is gone (R-21).** The earlier design stored
`GraphicRef { id, col, row }`, where `(col, row)` is the offset inside the image. Every covered
cell then holds a *distinct* triple, so interning deduplicates nothing and each cell consumes an
`ExtrasId`: a 4096x4096 Sixel at the 10x20 virtual cell covers about 410 x 205 = **84 050 cells**,
more than the whole `u16` id space, from one image — and step 3's fallback to id 0 means "no
extras", so the image would silently lose its cells. Storing only the `GraphicId` makes it **one
extras entry per image** (or two, if some of its cells also carry a hyperlink), and the painter
derives the offset from the placement record: `offset = cell_pos - placement.origin`
([`graphics.md`](graphics.md)).

`HyperlinkTable` interns `(id: Box<str>, uri: Box<str>)`. A link without an explicit `id=` gets a
**per-terminal** counter, not the reference's process-global `AtomicU32`, so two sessions cannot
collide and a test is deterministic. The view's identity hash (today
`crates/terminal-view/src/render/frame.rs:306-312`) becomes the `HyperlinkId`, resolved to its
strings under the lock by `render_update`
([`damage-and-render-state.md`](damage-and-render-state.md)).

### Graphemes — interned arena with GC by remap

```rust
pub const GRAPHEME_MAX_LEN: usize = 16;        // codepoints per cell
pub const GRAPHEME_SWEEP_ENTRIES: usize = 65_536;   // absolute trigger, see below
pub const GRAPHEME_SWEEP_CHARS: usize = 1 << 20;

pub struct GraphemeArena {
    chars: Vec<char>,                  // one flat arena
    spans: Vec<(u32, u8)>,             // (offset, len) indexed by GraphemeId
    index: FxHashMap<Box<[char]>, u32>,
}
```

- A cell needing more than one scalar stores a `GraphemeId` in its content bits. Identical
  sequences deduplicate for free, which is the reason to intern rather than to box per cell.
- Sequences longer than `GRAPHEME_MAX_LEN` are **truncated**, not rejected, and counted. This is
  the direct fix for the unbounded `Vec<char>` in the pinned fork.
- **This is the one place a sweep is kept**, because unbounded growth here is attacker-reachable:
  a stream of unique multi-codepoint cells grows the arena without bound (kitty's header
  documents exactly this failure). GC by remap: steal the arena, walk both screens skipping rows
  without `RowFlags::HAS_GRAPHEME`, re-intern only referenced sequences, rewrite each cell's
  content bits.
- **The trigger is an absolute constant, not a fraction of an id space (R-27).** The id space is
  the cell's 21 content bits (2 097 152 entries), not 65 536; the earlier "half the id space"
  figures were wrong. `GRAPHEME_SWEEP_ENTRIES = 65_536` and `GRAPHEME_SWEEP_CHARS = 1 MiB` are
  chosen so the arena stays inside a few megabytes and a sweep is rare; both have a test that
  drives them.
- The sweep runs only at the end of `feed()`, never mid-sequence, so no borrowed id is live
  across it. Because grapheme ids live in the cell's content bits and **style ids never move**
  (see above), a render copy taken under the lock cannot be invalidated by a later sweep — it
  carries resolved values, not ids ([`damage-and-render-state.md`](damage-and-render-state.md)).

### Width

`unicode-width` 2.x for scalar width, `unicode-segmentation` 1.x for cluster boundaries. Both are
already in `Cargo.lock`.

**Mode 2027 is not implemented in this intake (R-56, R-38).** The research asks that the
*storage* decision — intern, and cap the cluster length — be made early, and it is, above. The
rest of 2027 (a `GraphemeCursor` print path, a cross-chunk pending-cluster buffer, and a ConPTY
glyph-width flag that cannot follow a mode set after `CreatePseudoConsole`) is deferred:

- `CSI ? 2027 h/l` is recognised and inert; `DECRQM` answers `NotSupported`, which is the honest
  answer and means "do not use it".
- `cluster_width(&[char]) -> u8` is implemented and unit-tested now, so the later packet is a
  print-path change, not a design change.
- `oneterm-pty` spawns with `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH`, matching the engine
  unconditionally. The column-drift failure mode that would have followed from a mid-session mode
  change cannot occur, because there is no mode change ([`pty.md`](pty.md)).

Width today is therefore per scalar: `UnicodeWidthChar::width(c)`; `None` (control, unassigned)
means the character is dropped; width 0 attaches to the previous cell's grapheme. A ZWJ family
emoji lands as base plus a ZWJ tail and the next emoji starts a new cell — visibly wrong, and
exactly what the engine does today.

### Zero-width characters

Reference rules, reproduced:

1. Take `column = cursor.col`; if the pending-wrap flag is clear, `column = column.saturating_sub(1)`.
2. If that cell is a `WideSpacer`, step back once more.
3. Append the scalar to that cell's grapheme (promoting a scalar cell to a grapheme cell).
4. **At column 0 with no pending wrap the mark attaches to column 0** (trap 8).

### Cell predicates

Three questions, three answers, because they are genuinely different (R-12, R-13):

| Predicate | Definition | Used by |
| --- | --- | --- |
| `is_blank()` | content is `' '` **and** `style_id == 0` **and** `extras_id == 0` | the renderer's fast paths, `Cell::EMPTY` comparisons |
| `is_erasable()` | the reference's looser `is_empty` rule — content in `{' ', '\t'}`, default fg/bg, none of `INVERSE`, any underline, `STRIKEOUT`, `WideSpacer`, `LeadingWideSpacer`, and no grapheme — **and additionally false when the cell carries a `GraphicId`** (R-13) | `Row::shrink`, the `ED 2` occupancy scan |
| `text_char()` | `' '` for a blank, `'\t'` for a tab cell, the scalar or the grapheme otherwise | `row_text`, search, URL detection, the session log, the corpus snapshot |

**The `\t` cell (R-12).** `put_tab` writes a literal `\t` into a cell that held a space, which the
`tab_rendering` recording pins. Downstream: `text_char()` returns `'\t'`; `row_text` emits it
verbatim so search and URL detection see the same text the reference produces; `is_erasable()` is
true for it (the reference treats `'\t'` as empty); `is_blank()` is **false** for it, so the
renderer does not take a blank fast path over a tab cell; the corpus snapshot records it as `\t`.

`is_blank()` is deliberately stricter than `is_erasable()`: the reference's rule ignores
`WIDE_CHAR`, `BOLD`, `DIM`, `ITALIC`, `HIDDEN` and hyperlinks (trap 37), so a bold space counts as
empty there. Keeping two functions is cheaper than discovering the difference from a red
recording.

## Interfaces

```rust
// crates/vt/src/cell.rs
impl Cell {
    pub const EMPTY: Cell = Cell(0);
    pub fn content(self) -> CellContent;       // Scalar(char) | Grapheme(GraphemeId)
    pub fn width(self) -> CellWidth;
    pub fn style_id(self) -> StyleId;
    pub fn extras_id(self) -> ExtrasId;        // 0 = none
    pub fn semantic(self) -> Semantic;
    pub fn is_blank(self) -> bool;
    pub fn is_erasable(self, ex: &ExtrasTable) -> bool;   // needs the table to see a GraphicId
    pub fn with_style(self, id: StyleId) -> Cell;
    pub fn with_content(self, c: CellContent) -> Cell;
}

// crates/vt/src/intern.rs
impl Interner {
    pub fn style(&mut self, s: &Style) -> StyleId;          // never fails; falls back to 0
    pub fn resolve_style(&self, id: StyleId) -> &Style;     // id 0 always resolves
    pub fn extras(&mut self, e: &Extras) -> ExtrasId;
    pub fn resolve_extras(&self, id: ExtrasId) -> &Extras;
    pub fn grapheme(&mut self, cluster: &[char]) -> GraphemeId;
    pub fn resolve_grapheme(&self, id: GraphemeId) -> &[char];
    pub fn needs_grapheme_sweep(&self) -> bool;
}
// on Terminal, because the sweep touches both screens and the interner:
impl Terminal { pub(crate) fn sweep_graphemes(&mut self) -> SweepStats; }

pub fn scalar_width(c: char) -> Option<u8>;
pub fn cluster_width(cluster: &[char]) -> u8;   // implemented now, used when 2027 lands
```

## Edge Cases and Failure Modes

- [ ] **Trap 5 — wide character at the last column.** With wrap on: a `LeadingWideSpacer`
  carrying the current style goes into the last column, the line wraps, the glyph lands in
  columns 0-1 of the next row. With wrap off: the glyph is dropped and the column stays.
- [ ] **Trap 6 — overwriting half a wide pair.** Writing a narrow character over a `Wide` cell
  clears the `WideSpacer` to its right; writing over a `WideSpacer` clears the `Wide` cell to its
  left (dropping its grapheme, leaving a space); at column 0 or 1 the previous row's trailing
  `LeadingWideSpacer` is cleared too.
- [ ] **Trap 7 — insert mode over a wide character.** The reference shifts with raw swaps, never
  repairs the pair, and skips the shift entirely when `col + width >= cols`. Reproduced, because
  `vttest_insert` pins it.
- [ ] **Trap 8 — zero-width character at column 0** attaches to column 0.
- [ ] **Trap 37 — two predicates**, above.
- [ ] **Style table exhaustion** degrades to the default style and logs once; no sweep, no
  renumbering, so no id a render copy might hold can ever move.
- [ ] **Grapheme arena exhaustion** drives one sweep; every surviving cell still resolves and
  rendered text is unchanged.
- [ ] **Truncated over-long grapheme** increments `FeedStats::grapheme_truncated` and logs at
  `debug` once.
- [ ] **A cell carrying both a hyperlink and a graphic** is one extras entry shared by every such
  cell of that image and that link — still `O(1)` entries per image.

## Verification

`cargo test -p oneterm-vt cell::` and `intern::`

- [ ] `cell::tests::cell_is_eight_bytes` — `const _: () = assert!(size_of::<Cell>() == 8);`
- [ ] `cell::tests::zero_cell_is_a_blank_space_with_the_default_style`
- [ ] `cell::tests::content_roundtrips_for_max_scalar_and_max_grapheme_id`
- [ ] `cell::tests::width_enum_covers_every_wide_pair_shape`
- [ ] `cell::tests::wide_char_at_last_column_wrap_on_and_off` — trap 5.
- [ ] `cell::tests::wide_pair_repair_on_overwrite` — trap 6, all three sub-cases.
- [ ] `cell::tests::insert_mode_over_wide_char_leaves_orphan_spacer` — trap 7.
- [ ] `cell::tests::zero_width_at_column_zero_attaches_to_column_zero` — trap 8.
- [ ] `cell::tests::blank_and_erasable_predicates_differ_on_a_bold_space` — trap 37.
- [ ] `cell::tests::tab_cell_is_erasable_but_not_blank_and_reads_back_as_tab` — R-12.
- [ ] `cell::tests::graphic_cell_is_not_erasable` — R-13.
- [ ] `intern::tests::identical_styles_intern_to_one_id`
- [ ] `intern::tests::default_style_is_id_zero`
- [ ] `intern::tests::style_ids_never_change_once_assigned` — the property that makes the render
  hand-off safe; drives 70 000 distinct styles and asserts every previously issued id still
  resolves to the same value.
- [ ] `intern::tests::style_table_exhaustion_falls_back_to_default_and_logs_once`
- [ ] `intern::tests::one_extras_entry_per_image_not_per_cell` — R-21; stamps a 400 x 200-cell
  image and asserts the extras table grew by one.
- [ ] `intern::tests::hyperlink_ids_are_per_terminal_not_global`
- [ ] `intern::tests::identical_clusters_dedupe`
- [ ] `intern::tests::cluster_longer_than_cap_is_truncated_and_counted`
- [ ] `intern::tests::grapheme_gc_preserves_every_live_cell` — a stream of unique clusters, a
  forced sweep, then a full-grid text comparison against a pre-sweep snapshot.
- [ ] `intern::tests::grapheme_sweep_trigger_is_the_documented_constant` — R-27.
- [ ] `width::tests::scalar_widths_cjk_emoji_combining`
- [ ] `width::tests::zwj_family_splits_without_mode_2027` — pins today's behaviour.
- [ ] `width::tests::cluster_width_is_correct_for_emoji_flags_and_skin_tones` — the function is
  tested now even though nothing calls it until 2027 lands.
