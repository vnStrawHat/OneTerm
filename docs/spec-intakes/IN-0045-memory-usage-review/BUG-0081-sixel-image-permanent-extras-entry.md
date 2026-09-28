# Work: Every Sixel image still takes a permanent extras entry

ID: BUG-0081
Intake: IN-0045
Created: 2026-09-28
Reworked: 2026-09-28, same day, after adversarial verification failed the first design (High). See
[`evidence/BUG-0081-verify.md`](evidence/BUG-0081-verify.md) and the Decisions and Evidence and Gaps
sections below.

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

Implemented twice: `3796cef0` (first design, adversarially verified FAIL, High) and the rework this
packet now describes. This box tracks the packet's current state; the Decisions and Evidence and
Gaps sections carry the history the box cannot.

## Classification

- Change type: bug (shipped behavior since the `IN-0029` engine's Sixel support)
- Risk lane: normal, with a public-contract edge: `oneterm-vt` is an external contract, and
  `ExtrasId` and `Interner::extras` are visible to embedders. An LLD note precedes the code, the
  same way `BUG-0079` handled the sibling case in the hyperlink table.
- Spec Intake, when required: `IN-0045`. The harness row was opened by the coordinator as a
  planned follow-up to `BUG-0079`'s Evidence and Gaps (F7).

## Reported by

`BUG-0079`'s own independent verification
([`evidence/BUG-0079-verify.md`](evidence/BUG-0079-verify.md) F7, Medium, residual, pre-existing,
out of that packet's scope): `place` interns a fresh `Extras { graphic: Some(id), .. }` for every
placement, because `id` (`GraphicId`) is always a fresh counter value, even when the resent bytes
are byte-for-byte the same image. The probe `v_sixel_repaint_fills_extras` there confirmed the
table fills after 65,534 images, after which an explicit `id=` link printed next gets no cells.
`BUG-0079`'s Handoff named this packet as the coordinator's next open item.

## Outcome

Resending the same Sixel image does not consume extras table entries without bound: after 70,000
resends of one image the extras table stays well below the 65,535-entry ceiling (measured near
20,000, not a small constant — see Decisions), the last resend still places, and a later explicit
`OSC 8` link and a later distinct image in the same terminal both still get their cells. An id is
never freed while any cell — on either screen, in scrollback, or on a cursor's pen or erase cell —
still names it.

## Scope

- [x] In scope: `oneterm-vt`'s `InternTable` (a proven-safe `free`/reuse path: a free set, an
  idempotent `free`, and a periodic `sweep_unreferenced` that reads a caller-supplied live set
  rather than trusting one call site's bookkeeping); `Screen::collect_live_extras_ids` and
  `TerminalGrid::live_extras_ids`, which build that live set from the whole grid; `State::intern_extras`,
  which orchestrates the sweep from the hot call sites rather than once per `feed`; their rustdoc;
  the `IN-0029` LLD rows on graphics and extras; `crates/vt/docs/guide/10-limits.md` and
  `04-events.md`; the `docs/terminal-backend.md` risk row `BUG-0079` added; `graphics::assert_integrity`'s
  pre-existing column-vs-row false positive (F4), narrowed while here.
- [x] Out of scope: the hyperlink table and its own ladder (already fixed by `BUG-0079`); adding a
  content hash so two simultaneously placed copies of the same image bytes could share a
  `GraphicId` — rejected in the LLD note, not deferred; a total-pixel-bytes budget across live
  placements (`IN-0029` `graphics.md` already notes this is unbounded and calls it a separate,
  later concern); a compacting sweep that renumbers ids to give a tighter bound — rejected, see
  Decisions.

## Acceptance

- [x] A failing `oneterm-vt` test first: resend one Sixel image 70,000 times at a fixed cursor
  position; assert the extras table's `entries()` stays well below the 65,535 ceiling, the last
  resend's cell still resolves to a graphic, and a following explicit `id=` link and a distinct
  Sixel image both still get their cells.
  `terminal::tests::repainting_a_sixel_image_does_not_grow_the_extras_table`.
- [x] Two images placed at once stay distinct (no shared `GraphicId`, no merged placement).
  `graphics::tests::two_different_images_stay_distinct`.
- [x] A released placement's cells are never recycled into an unrelated value while any cell still
  names them — eviction while still on screen (`v1`), eviction while in scrollback (`v8`), a row
  the sweep released after `IL`/`SD` split it outside its tracked extent (`v9`), and a released
  **merged** hyperlink-and-graphic entry (`v4`). `graphics::tests::v1_*`, `v8_*`, `v9_*`, `v4_*`.
- [x] A recycled id can never alias a *different*, still-live image and hide it from the renderer's
  paint loop. `graphics::tests::v2_every_live_placement_is_reachable_through_its_own_cells`.
- [x] Freeing the same id twice never hands it out twice, in every build, not only under
  `debug_assert!`. `intern::tests::freeing_the_same_id_twice_does_not_alias_it`.
- [x] `vt-public-api.py --check` is unchanged (the new methods are all `pub(crate)`); recorded in
  `crates/vt/CHANGELOG.md` as a "Fixed" entry, no signature.
- [x] Headless re-measure: extras table entries and live allocator bytes (drained and undrained)
  after 70,000 resends of one image, before and after the fix; the scan cost of building the live
  set, at the coordinator's requested worst-case shape (100,000 rows x 200 columns) and at the test
  terminal's own shape.
- [x] `cargo test -p oneterm-vt --features vt-paranoid` (the whole-history integrity walk) still
  passes, and now can actually fail on this bug's class of error: a new check
  (`Screen::assert_interned_ids_resolve`) asserts every cell's, and every cursor pen/erase cell's,
  extras id is not on the free list, not only that it was once issued.
  `graphics::tests::integrity_rejects_an_extras_id_freed_while_a_cell_still_names_it` proves the
  check fires.

## Documentation

### Owning Docs Reviewed

- `crates/vt/src/intern.rs` rustdoc (`InternTable`, its module doc) — stated the three-step ladder
  and "an id, once issued, is valid for the life of the terminal"; silent on any release path,
  because none existed.
- `crates/vt/src/graphics/mod.rs` and `crates/vt/src/graphics/placement.rs` rustdoc (`GraphicsState`,
  `place`, `sweep`, `evict_oldest`) — documented one extras entry per image and the row-derived
  release sweep, but not that release ever touched the extras table.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md` § "Liveness and the release
  signal" — states release "costs a table slot ... until eviction reclaims it," which described the
  placement table slot, not the extras table entry; the two were conflated until this packet made
  them the same claim in fact.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md` § "Extras" — "same
  three-step ladder" as styles, silent on any exception.
- `docs/terminal-backend.md` § 13 Risks — the row `BUG-0079` added named the hyperlink fix only.
- `crates/vt/docs/guide/10-limits.md` — described the hyperlink and grapheme release paths, not a
  graphics one (there wasn't one).
- [`evidence/BUG-0079-verify.md`](evidence/BUG-0079-verify.md) F7 — the report this packet closes.

### Documentation Action

Update required, twice. The first pass (before `3796cef0`) chose between the two mechanisms the
coordinator's brief allowed — release-time freeing, or content-hash reuse (rejected, see Decisions)
— and wrote the docs for release-time freeing. Adversarial verification (`evidence/BUG-0081-verify.md`)
found that premise false, so every doc it touched needed a second pass with the rework: the same
list, `crates/vt/src/intern.rs` rustdoc, `crates/vt/src/graphics/mod.rs` and
`crates/vt/src/graphics/placement.rs` rustdoc, the `IN-0029` LLD rows, `docs/terminal-backend.md`
§ 13, `crates/vt/docs/guide/10-limits.md`, plus two the first pass missed:
`crates/vt/docs/guide/04-events.md` (F6: `GraphicReleased`'s doc already overstated "the last cell
is gone", which is exactly the premise the first design's bug leaned on) and
[`low-level-design/sixel-extras-release.md`](low-level-design/sixel-extras-release.md) itself,
rewritten rather than merely amended.

### Reconciliation

Done (rework):

- [`low-level-design/sixel-extras-release.md`](low-level-design/sixel-extras-release.md): rewritten
  with a "First design (rejected by adversarial verification)" section recording what `3796cef0` did
  and why F1/F2 broke it, then the mark-and-sweep design, the orchestration that checks after every
  hot intern rather than once per `feed`, and why the bound is not a small constant.
- `crates/vt/src/intern.rs` rustdoc: `InternTable`'s module doc and struct doc now describe the
  sweep (a caller-supplied, whole-grid-proven live set), not a single call site's bookkeeping.
- `crates/vt/src/graphics/mod.rs`: reverted to `main` (no rustdoc claim to fix — `extras_by_graphic`
  is gone).
- `crates/vt/src/graphics/placement.rs` rustdoc: file-level doc now states release does **not** free
  anything; `assert_integrity`'s doc records the F4 column-scoping fix.
- `IN-0029` `graphics.md` § "Liveness and the release signal": the table-slot paragraph rewritten to
  say release does not free the extras entry, with the sweep as what actually bounds it.
  `cell-and-style.md` § "Extras": the exception restated as the sweep, not a release-time free.
- `docs/terminal-backend.md` § 13 Risks: the `BUG-0081` half of the row rewritten with the sweep
  mechanism and the ~20,000-of-65,535 measured bound.
- `crates/vt/docs/guide/10-limits.md`: the added paragraph rewritten for the sweep; a new sentence
  that a raw `ExtrasId` must never outlive the borrow that read it (a sweep can free it later).
- `crates/vt/docs/guide/04-events.md` (F6, missed by the first pass): `GraphicReleased`'s table row
  and Chapter 4's prose no longer claim "the last cell is gone"; both now say the engine's own
  placement tracking ended, and note a released image's cells can still exist, harmlessly, because
  the id is never reused.
- `crates/vt/CHANGELOG.md` `[Unreleased]` "Fixed": the entry rewritten to describe the sweep
  mechanism and name the rejected first design.
- `docs/spec-intakes/IN-0045-memory-usage-review/IN-0045.md`: the `BUG-0081` bullet updated with the
  rework and the new measured numbers.
- Reviewed, no change: `crates/vt/README.md` (does not describe the tables, as `BUG-0079` also
  found); `IN-0029` `high-level-design.md` (still true: one entry per image, or two with a
  hyperlink); the parity corpus (this is engine-internal bookkeeping with no observable byte-output
  change, so no new deviation).

## Context

- `crates/vt/src/graphics/placement.rs` `place` interns the graphic-only `Extras` once per
  placement; `stamp` interns one more, distinct, merged value for each covered cell that already
  carried a hyperlink. A placement can therefore own more than one extras entry.
- `crates/vt/src/terminal/mod.rs` `feed`: `graphics::sweep` runs once at the end, after the parser
  has advanced; `evict_oldest` runs inside `place`, per image, before the new placement is pushed.
  Neither touches the extras table in the rework.
- `crates/vt/src/snapshot/row.rs` resolves a cell's `ExtrasId` into `Option<HyperlinkId>` /
  `Option<GraphicId>` while the terminal's lock is still held, and stores neither the raw `ExtrasId`
  nor anything else that could still name a freed slot afterward. True, and cited by the first
  design's LLD note as its safety argument — but it answers "is a *snapshot* consumer safe", and the
  actual long-lived holder the bug lived in is the grid itself, which
  `evidence/BUG-0081-verify.md` F1 found and the rework's live-set scan now reads directly.
- `crates/vt/src/grid/screen.rs` `Screen::integrity_lo()`: the ordinary debug-integrity walk covers
  only the current batch's touched range (O(rows)) unless `vt-paranoid` is on, which is why the
  live-set scan (`collect_live_extras_ids`) cannot reuse it and must always walk
  `oldest..=newest` — a caller building the live set with the wrong range is exactly how a second
  version of this bug would reappear.

## Plan

- [x] LLD note with the choice above (release-time freeing vs. content-hash reuse), decided against
  content-hash reuse and, at first, for release-based freeing.
- [x] First implementation: `InternTable::free`, `GraphicsState::extras_by_graphic` / `track_extras`,
  a shared `release` helper in `sweep` / `evict_oldest`. Committed `3796cef0`.
- [x] Adversarial verification: FAIL, High (`evidence/BUG-0081-verify.md`, F1/F2). Reopened as
  acceptance rework of this same packet, not a new `BUG`.
- [x] Rework: `extras_by_graphic`/`track_extras`/the shared `release` helper removed;
  `graphics/mod.rs` and `graphics/placement.rs` reverted to `main` except the F4 column-scoping fix.
  `InternTable` gains a proven-safe `free`/`sweep_unreferenced` pair; `Screen`/`TerminalGrid` gain the
  live-set scan; `State::intern_extras` orchestrates the sweep from the hot call sites.
- [x] V1/V4/V8/V9/V2 regression tests reproducing the adversarial findings; the F5 double-free test;
  the F3 paranoid-check regression.
- [x] Headless re-measure (entries, live bytes drained/undrained) and the scan-cost measurement.
- [x] Owning docs reconciled a second time (list above).

## Decisions

Content-hash reuse (generalizing `BUG-0079`'s URI-keyed dedupe to image bytes) is **rejected**, not
deferred: `GraphicsState::placement` resolves an id to a placement by `.find(|p| p.id == id)`, the
first match, so two images placed **at once** that shared a `GraphicId` would make the painter draw
the second occurrence at the first occurrence's position — a rendering bug, not a UX simplification
like the hyperlink hover-merge `BUG-0079` accepted. This holds in both designs; the rework does not
revisit it.

**Release-based freeing (the first design, `3796cef0`) is rejected**, by adversarial verification,
not by this packet's own second thoughts: `evidence/BUG-0081-verify.md` F1 (High) found it frees an
extras id while cells in the grid still carry it, on three ordinary paths (`MAX_PLACEMENTS` eviction
of an image still on screen or in scrollback, a sweep after `IL`/`SD` split an image outside its own
tracked extent, and — reasoned, not separately run — a history trim that drops a tall image's anchor
row while its lower rows survive), and F2 found the recycled id can hide a *different*, still-live
image from the renderer's paint loop. A placement's release was never a proof that its cells are
gone; the design mistook "this table stopped tracking it" for "nothing references it any more".

**Chosen instead: free an id only once a whole-grid scan proves it unreferenced** (`InternTable::sweep_unreferenced`,
fed by `Screen::collect_live_extras_ids`/`TerminalGrid::live_extras_ids`). This is the LLD note's own
suggested rework direction (its first option, "a grapheme-arena-style collection"), and it is the
only one of the two the coordinator's brief offered that does not depend on any one call site's
bookkeeping being complete — the exact thing the first design got wrong.

**The resulting bound is a high-water mark that ratchets up under sustained load, not a small
constant, and that trade is accepted rather than chased further.** Freeing never shrinks the table
(a live id must never move, so there is nowhere to compact a survivor to), so a stream that always
creates a genuinely new value — every Sixel resend does — grows the table's peak by
`TABLE_SWEEP_INTERVAL` (4,096) every time the previous sweep's free list runs out, indefinitely under
a long enough sustained adversarial stream. A smaller interval tightens this (the peak scales with
`sqrt(interval)`) at the cost of more frequent whole-history scans (measured at 32 ms for the
coordinator's 100,000 x 200 worst case); a true constant bound would need a compacting sweep that
renumbers live ids, which is the exact hazard the "styles and extras never move" rule exists to
prevent (a render copy or an embedder holding an id across the renumber). 4,096 is chosen to keep
70,000 resends — the size this bug was measured and is tested at — nowhere near the 65,535 ceiling
(measured near 20,000) while keeping the scan rare for an ordinary session. No DEC record: the
interval is a tuning knob within an accepted design, not a choice future work must inherit, and the
LLD note carries the reasoning if it needs revisiting.

## Verification Plan

- Focused: the new `oneterm-vt` tests above (V1/V4/V8/V9/V2, the double-free test, the paranoid-check
  regression, the 70,000-resend bound).
- Unit: `cargo test -p oneterm-vt --lib`, `--features vt-paranoid`, `--features regex`,
  `--no-default-features`.
- Package/API: `cargo build -p oneterm-vt --no-default-features --examples`,
  `cargo build -p oneterm-vt --all-features --examples`, `cargo run -p oneterm-vt --example
  headless`, `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features`,
  `python scripts/vt-public-api.py --check --no-doc`, `--diff-platforms`,
  `cargo package -p oneterm-vt --allow-dirty --list | python scripts/verify-dependency-graph.py
  --package-list -`.
- Integration: a headless measure of 70,000 resends of one Sixel image, extras table entries and
  live allocator bytes (drained and undrained) before and after the fix; the live-set scan cost at
  two shapes; a throwaway counting allocator and a throwaway `#[ignore]`d timing test, both removed
  before commit.
- Full local gate: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

**First design (`3796cef0`), adversarially FAILED.** Full findings, reproductions and measurements
in [`evidence/BUG-0081-verify.md`](evidence/BUG-0081-verify.md). Headline: 258 extras entries after
70,000 resends (a tight bound), but V1/V8/V9/V4 each turned a released-but-still-live cell into an
unrelated link, and V2/F2 showed a recycled id hiding a live image from the renderer.

**Rework, headless, release profile, one 20x4 terminal, a throwaway counting global allocator
(removed before commit), resending one 1x6 Sixel at a fixed cursor position (`CSI H` then the same
`DCS q` body, matching the `BUG-0079`-verify F7 / `BUG-0081`-verify probe shape):**

| | Extras entries | Live bytes, undrained | Live bytes, drained |
| --- | --- | --- | --- |
| Before (main, `eed33058`) | 65,535 (full) | 13.42 MB | -- (not separately measured) |
| First design (`3796cef0`, rejected) | 258 | 7.85 MB | -- (not separately measured) |
| This rework, after 70,000 resends | 20,481 | 9.080 MB | 2.740 MB |

"Drained" calls `take_graphics()` after every feed, as an embedder does; "undrained" never does, and
matches the figures the packet and `BUG-0081-verify.md` both reported for the earlier designs. The
last resend still places in the rework (`GraphicId` resolves on the cell), the same as both earlier
measurements.

**Scan cost** (`TerminalGrid::live_extras_ids`, release profile, measured with a throwaway
`#[ignore]`d test removed before commit):

| Shape | Time |
| --- | --- |
| 100,000 rows x 200 columns (the coordinator's requested worst case) | 32.4 ms |
| 20x4 (this packet's own test terminal) | 700 ns |

At `TABLE_SWEEP_INTERVAL = 4,096`, the 100,000 x 200 cost is paid at most once per 4,096 new extras
entries — rare for an ordinary session, and the worst realistic cost (a full-scrollback terminal
under a sustained distinct-hyperlink-or-image stream) rather than the common one. See Decisions for
the bound-vs-scan-frequency trade this number is part of.

`v1`/`v4`/`v8`/`v9`/`v2`, the double-free test, and the new paranoid-check regression all pass
(`crates/vt/src/graphics/graphics_tests.rs`, `crates/vt/src/intern_tests.rs`); each was confirmed to
fail against the first design's committed source before being folded into the rework (the same
reproductions `evidence/BUG-0081-verify.md` used).

Gate: `pwsh scripts/ci-local.ps1`, `CARGO_BUILD_JOBS=3`, `target/release` deleted first, run from
the worktree's own `target` directory (a parallel agent was building elsewhere at the same time).
Final line pasted into the commit message and the handoff to the coordinator.

Gaps:

- No live 5-minute two-tab `measure.ps1 -AllocLog` re-measure on the built app, and no run of a
  real Sixel-emitting program (`img2sixel`, `chafa`, a file manager). The headless figure covers the
  mechanism, the same limitation `BUG-0079`'s own re-measure and the first design's evidence both
  carried.
- A hostile stream can still fill the extras table with distinct simultaneous placements up to
  `MAX_PLACEMENTS` combined with distinct hyperlinks and distinct `OSC 8` URIs; the unchanged ladder
  drops further values past 65,535. Not new: the ladder's fallback was always there.
- The total-pixel-bytes budget across live placements (`IN-0029` `graphics.md` already notes 256
  placements have no such budget) is unchanged and out of this packet's scope.
- The bound is not a small constant (Decisions): a sustained enough adversarial resend stream still
  reaches the 65,535 ceiling eventually, just far more slowly (quadratically, in the number of
  resends) than the pre-fix one-entry-per-resend rate. Not measured beyond 70,000 resends.
- `evidence/BUG-0081-verify.md`'s own gaps carry over unresolved: the history-trim path (a
  multi-row image whose anchor row is trimmed first, leaving lower rows) is reasoned, not run as its
  own test, though `v9` exercises the mechanistically similar `IL`/region-scroll split.

## Handoff

Implemented, reworked once after its own adversarial verification failed (High), and self-verified
against that failure's reproductions. A second independent verification pass, checking specifically
that the rework's live-set scan is as complete as the first design's failure demanded (every holder
F1 named: eviction while on screen, eviction while in scrollback, an `IL`/`SD`-split extent, a
merged hyperlink-and-graphic entry, both cursors' pen and erase cell) and that the new paranoid check
actually catches a regression, is the natural next step before this is treated as accepted/shipped;
none is recorded yet in this packet.
