# DEC-0015 Absolute row ids and an incremental render state are the engine-to-view contract

> Naming note (`US-0101`, IN-0038): what this record calls the *render state* is now
> `SnapshotState`, reached through `Terminal::snapshot_update`. The decision itself is unchanged;
> the record keeps its original wording as the historical document it is.

Date: 2026-09-12

## Status

accepted

## Context

`DEC-0014` decides that OneTerm owns its VT engine. Two shapes of that engine's public surface
are inherited by everything built on top of it — search, selection, OSC 133 marks, Sixel
anchors, the gutter, the agent panel, the render element, and any future consumer — and both
are painful to retrofit. They are recorded once, here, so no packet re-litigates them.

**Coordinates.** The vendored engine addresses rows with a signed `Line(i32)` where negative
values are scrollback, relative to a viewport whose origin moves on every scroll. Every consumer
therefore converts with `line + display_offset` and reasons about a moving origin:
`crates/terminal/src/search.rs:59-61`, `crates/terminal-view/src/render/frame.rs:525`, `:541`,
`:550-557`, `crates/terminal/src/model.rs:430`, with two different fallbacks at
`frame.rs:514-532`. The engine also has no absolute output-line counter — `total_lines()`
saturates at the scrollback cap — which is why `crates/terminal/src/backend/line_accounting.rs`
exists at all, reconstructing the count by rescanning every chunk for `\n` once history is full
(a second full byte pass, tracked as PERF-19, active in most of the numbers in
`research/perf-baseline.md` § 4). Every other engine in the survey has a stable row space:
wezterm's `stable_row_index_offset`, Rio's `total_lines_scrolled: u64`, Contour's
`_stableBase` / `_generation`, Ghostty's per-node `serial: u64`
(`research/prior-art.md` § 9.2).

**Renderer hand-off.** Today `TerminalContent::refill` (`crates/terminal/src/content.rs:173-222`)
clones every visible cell out of the grid under the lock, once per painted frame, and the view
converts each one again into its own `Cell` (`crates/terminal-view/src/render/frame.rs:298-319`).
The measurement says this is not a bottleneck — 29.9 us per frame at 200x50, 0.18 % of a 60 Hz
budget (`research/perf-baseline.md` § 4) — so it is not being changed for speed. It is being
changed because it scales with viewport area rather than with change, because it is the reason
there is "deliberately no damage-free full-grid snapshot" and `query_line_range_cells` had to be
invented instead (`docs/terminal-backend.md:151-155`), and because damage is single-consumer
(`Term::damage()` + `reset_damage()`) while OneTerm already has several consumers of change.

## Decision

**1. Rows are named by an absolute, monotonically increasing position in the output stream.**

- `RowId(u64)` names a **position**, not a piece of content. It is never reused, never
  decremented, and it is the ring index: `slot = id & mask`.
- **Two lanes.** The primary screen allocates from `0` and the alternate from `1 << 63`, each
  keeping a contiguous run, because one shared counter would leave gaps in both runs once the two
  screens allocate independently. The lanes never meet, so an id is still unambiguous and the high
  bit routes it to its screen. Everything that compares an id against a bound — trimming above
  all — must be lane-scoped.
- **What "stable" means, precisely.** A `RowId` keeps naming the same content across the
  operation that dominates a terminal's life — pushing rows into scrollback as output arrives —
  and across `RIS`, `ED 2`, `ED 3` and viewport scrolling. It does **not** survive an operation
  that moves content between positions: `IL`, `DL`, `SU`, `SD`, `RI` at the region top, any
  scroll whose region is not the whole viewport, or reflow. Those copy content between fixed
  ids, which is the normal case inside tmux, vim and htop.
- **Anchors are therefore engine-owned, not consumer-owned.** One tracked-anchor list holds the cursor,
  the saved cursor and the viewport top **per screen**, both selection anchors, every graphics
  placement and every OSC 133 mark (the canonical `AnchorKind` list lives in
  `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md`). Every row-moving
  primitive adjusts it, and reflow remaps it through the same mechanism. A consumer
  never re-derives an anchor from a `RowId` it stored earlier; it reads the anchor back from the
  engine, or registers one and is told where it went.
- Content motion that a consumer must still react to is reported: `VtEvent::RowsScrolled`
  (region, delta) and `VtEvent::RowsTrimmed { oldest }`.
- The viewport is a **scroll offset from the newest row**, not an absolute top. Offset `0` is
  sticky: the view follows output. A non-zero offset holds the viewed content still while rows
  are pushed, and is capped by the available history. There are no negative row indices anywhere
  in the public API.
- The absolute output-line count the gutter needs is its own counter
  (`Terminal::lines_produced() -> u64`, incremented on line feeds, not on wraps and not on
  reflow), because the row-id counter answers a different question.
  `crates/terminal/src/backend/line_accounting.rs` is deleted rather than ported.

**2. The renderer receives an incremental render state, not a per-frame copy of the viewport.**

- `Terminal::render_update(&mut self, &mut RenderState, now) -> RenderUpdate` runs with the
  engine lock held and copies **only rows whose sequence number exceeds the render state's
  watermark**. It copies them as run-length runs of **resolved style values**, never as interned
  ids: an id indexes engine-owned tables the render thread cannot read once the lock is released,
  and the grapheme arena may renumber on a later batch. (Style ids themselves are immutable for the
  life of the terminal, so this is a rule about ownership, not a race fix.)
- `RenderState::map_colors(...)` runs **outside** the lock and does the one thing that genuinely
  needs no engine state: mapping named and palette colours through the theme.
- The result is tri-state: `Unchanged` (the element skips layout and paint entirely),
  `Partial` (a changed-row list plus an explicit scroll delta, over a render state that always
  holds the full viewport), `Full`.
- Scroll is reported as a distinct delta, not as N dirty rows, so a consumer with a per-row
  cache shifts it instead of rebuilding it.
- Damage is a monotonic per-row sequence number plus a dirty bit in the row header, read through
  a watermark the consumer owns. Nothing clears damage for anyone else and there is no reset pass,
  so a second consumer becomes possible without an engine change; none is scoped in this intake.
- Under sustained output the writer yields on an explicit demand signal raised by the reader,
  with a byte cap as a backstop. Fairness is not left to the mutex alone.
- Synchronized output (mode 2026) is honoured by **skipping frames in the renderer**, never by
  buffering unparsed bytes. A program that opens an update and never closes it costs frames, not
  memory.

**3. Events are values, not callbacks.**

`feed()` returns a batch of owned events drawn from a per-batch arena. Nothing runs inside the
engine while the caller holds a lock it cannot release, so an embedder can never deadlock itself
in a callback.

## Alternatives

- [x] Selected: positional `RowId(u64)` plus a sticky-bottom viewport offset, engine-owned
  tracked anchors, and an incremental render state carrying resolved styles.
- [ ] **`RowId` names content rather than a position**, so an anchor survives `IL` / `DL` / a
  region scroll. Rejected: it decouples the id from the ring slot, so every row access becomes a
  lookup instead of a mask, and it does not remove the need for anchor bookkeeping — it only
  moves it into a map that every scroll must still update. The tracked-anchor list does the same
  job for the handful of anchors that exist, instead of for every row.
- [ ] **Report row motion and let each consumer re-anchor** (`RowsShifted` only, no engine-side
  anchors). Rejected as the primary mechanism: four consumers would each reimplement the same
  adjustment, and one of them (graphics) lives inside the engine anyway. The event is kept as a
  notification, not as the anchor mechanism.
- [ ] **Keep signed `Line(i32)` with negative history.** Rejected: it forces the
  `line + display_offset` conversion on every consumer, keeps `LineAccounting` alive, and makes
  every anchor (mark, search hit, graphic, selection end) invalid the moment the viewport moves.
  The migration cost is real — every `Line(-1)` assertion in
  `crates/terminal/src/model.rs:616-910` and in `search.rs` is rewritten — but it is paid once,
  in packets that are rewriting those tests anyway.
- [ ] **Borrow the grid under a guard held during layout and paint.** Rejected: it is the one
  model `docs/terminal-backend.md:173-175` forbids outright, because paint takes milliseconds
  and the parser would block behind it.
- [ ] **A full double-buffered render buffer (Contour's model).** Rejected: it decouples
  perfectly but flattens every changed frame into a fresh buffer. The incremental model does
  strictly less work for the same guarantee, and GPUI's element model already owns the paint
  side.
- [ ] **Keep the per-frame viewport clone.** Rejected on shape, not on speed: it scales with
  viewport area rather than with change, it is single-consumer, and it is the reason a general
  damage-free snapshot cannot exist.
- [ ] **Keep callbacks under the lock (`EventListener::send_event`).** Rejected: it is what
  forces the two-tier deferred/reliable machinery in `crates/terminal/src/backend/pump.rs:163-178`
  and the CORR-01 deadlock test, and it is what makes `Event::Osc` deep-copy its parameters into
  a `Vec<Vec<u8>>` on the hot path just to cross a channel.

## Consequences

- [ ] Benefit to confirm: `crates/terminal/src/backend/line_accounting.rs` and the PERF-19
  newline rescan disappear; the gutter reads `lines_produced()` directly, with the same
  meaning it has today (output lines, not rows created).
- [ ] Benefit to confirm: `crates/terminal-view/src/render/frame.rs:514-557` loses both
  display-offset fallbacks, and `plan_cache` / `row_plan` key on `RowId` plus sequence number
  instead of on a hash of a copied row.
- [ ] Benefit to confirm: the `SessionEventSink` deferred/reliable split
  (`crates/terminal/src/backend/event_sink.rs`, driven by `pump.rs:163-178`) becomes a plain
  drain of a returned batch.
- [ ] Tradeoff: `RowId` is 8 bytes per row of bookkeeping and one more concept for a reader to
  learn. Accepted: it is strictly less arithmetic than the moving origin it replaces.
- [ ] Tradeoff: the tracked-anchor list must be updated by **every** row-moving primitive, and a
  primitive that forgets it produces a silently misplaced mark or selection. Mitigated by making
  it the single mechanism (reflow uses it too, so it is exercised by the reflow property tests)
  and by a debug assertion that every anchor points inside the live row range.
- [ ] Tradeoff: copying resolved styles instead of interned ids costs more bytes per changed
  row. Accepted: it is bounded by *changed* rows, and it removes a whole class of races between
  the render copy and the engine's table maintenance.
- [ ] Tradeoff: the render state is stateful and must be invalidated correctly on resize, alt
  screen swap and reflow, or the view shows stale rows. Mitigated by returning `Full` from every
  one of those paths and by a debug-build integrity assertion that the watermark never moves
  backwards.
- [ ] Follow-up: search, the gutter, OSC 133 marks and the agent panel each migrate to `RowId`
  in their own packet; until then the adapter translates at the seam.
