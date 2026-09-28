# Work: Every Sixel image still takes a permanent extras entry

ID: BUG-0081
Intake: IN-0045
Created: 2026-09-28

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
resends of one image the extras table holds a small, constant-order number of entries (bounded by
the live-placement ceiling, not by how many images were ever sent), the last resend still places,
and a later explicit `OSC 8` link and a later distinct image in the same terminal both still get
their cells.

## Scope

- [x] In scope: `oneterm-vt`'s `InternTable` (a `free`/reuse path for one id at a time, alongside
  the existing three-step ladder), `GraphicsState`'s bookkeeping of which extras entries a
  placement owns, and the placement-release paths (`sweep`, `evict_oldest`) that now free them;
  their rustdoc; the `IN-0029` LLD rows on graphics and extras; `crates/vt/docs/guide/10-limits.md`;
  the `docs/terminal-backend.md` risk row `BUG-0079` added.
- [x] Out of scope: the hyperlink table and its own ladder (already fixed by `BUG-0079`); adding a
  content hash so two simultaneously placed copies of the same image bytes could share a
  `GraphicId` — rejected in the LLD note, not deferred; a total-pixel-bytes budget across live
  placements (`IN-0029` `graphics.md` already notes this is unbounded and calls it a separate,
  later concern).

## Acceptance

- [x] A failing `oneterm-vt` test first: resend one Sixel image 70,000 times at a fixed cursor
  position; assert the extras table's `entries()` stays well below the 65,535 ceiling, the last
  resend's cell still resolves to a graphic, and a following explicit `id=` link and a distinct
  Sixel image both still get their cells.
  `terminal::tests::repainting_a_sixel_image_does_not_grow_the_extras_table`.
- [x] Two images placed at once stay distinct (no shared `GraphicId`, no merged placement).
  `graphics::tests::two_different_images_stay_distinct`.
- [x] A released placement's extras entry is reused by a later, different image rather than
  growing the table further. `graphics::tests::a_released_placements_extras_entry_is_reused`.
- [x] `vt-public-api.py --check` is unchanged (`InternTable::free` is `pub(crate)`); recorded in
  `crates/vt/CHANGELOG.md` as a "Fixed" entry, no signature.
- [x] Headless re-measure: extras table entries after 70,000 resends of one image, before and
  after the fix.
- [x] `cargo test -p oneterm-vt --features vt-paranoid` (the whole-history integrity walk) still
  passes: a freed-and-reused `ExtrasId` never leaves a cell resolving to a dangling placement.

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

Update required: `crates/vt/src/intern.rs` rustdoc (the free/reuse path and why it does not violate
the "styles and extras never move" rule), `crates/vt/src/graphics/mod.rs` and
`crates/vt/src/graphics/placement.rs` rustdoc, the `IN-0029` LLD rows named above,
`docs/terminal-backend.md` § 13, and `crates/vt/docs/guide/10-limits.md`. Write
[`low-level-design/sixel-extras-release.md`](low-level-design/sixel-extras-release.md) under
`IN-0045` first, choosing between the two mechanisms the coordinator's brief allowed:

1. **Release the extras entry when its placement is released.** The sweep and the eviction path
   both already detect the moment a placement dies; teach them to free every extras entry the
   placement's cells resolved to, tracked as they are created rather than found by scanning the
   grid.
2. **Content-hash reuse**, generalizing `BUG-0079`'s URI-keyed dedupe to image bytes: the same
   image resent would reuse the same `GraphicId`.

Reason: the current docs describe one extras entry per image, correctly, and do not describe what
(if anything) happens to that entry when the image's placement is released — because until this
packet, nothing did.

### Reconciliation

Done:

- [`low-level-design/sixel-extras-release.md`](low-level-design/sixel-extras-release.md) (new):
  identity key, why option 2 is rejected outright (not merely deferred), the free-list mechanism,
  and why it does not reopen the render-copy hazard the "styles and extras never move" rule exists
  for.
- `crates/vt/src/intern.rs` rustdoc: `InternTable`'s module doc and struct doc now state the
  `free`/reuse exception and its scope (owner-proven, one id at a time, never a sweep).
- `crates/vt/src/graphics/mod.rs` rustdoc: `GraphicsState::extras_by_graphic` and
  `GraphicsState::reset`.
- `crates/vt/src/graphics/placement.rs` rustdoc: file-level doc, `sweep`, `evict_oldest`, and the
  new `release` helper they share.
- `IN-0029` `graphics.md` § "Liveness and the release signal": a new paragraph making the table-slot
  claim literal. `cell-and-style.md` § "Extras": the exception to "ids never change."
- `docs/terminal-backend.md` § 13 Risks: the `BUG-0079` row extended with the Sixel case.
- `crates/vt/docs/guide/10-limits.md`: a new paragraph under "Where the last three of those live."
- `crates/vt/CHANGELOG.md` `[Unreleased]` "Fixed": one entry.
- `docs/spec-intakes/IN-0045-memory-usage-review/IN-0045.md`: a bullet under the `BUG` branch,
  matching `BUG-0079`'s and `BUG-0080`'s.
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
- `crates/vt/src/terminal/dispatch.rs` `reset_state` (`RIS`): calls `graphics.reset()` before
  `interner.hyperlinks.clear()` and `interner.extras.clear()`. The ordering matters: `reset()` must
  clear `extras_by_graphic` too, or the sweep that `RIS`'s own row-blanking triggers at the end of
  that `feed` would try to free ids the wholesale `clear()` already invalidated (an out-of-bounds
  index otherwise, since `clear()` shrinks `entries` back to length 1).
- `crates/vt/src/snapshot/row.rs` resolves a cell's `ExtrasId` into `Option<HyperlinkId>` /
  `Option<GraphicId>` while the terminal's lock is still held, and stores neither the raw `ExtrasId`
  nor anything else that could still name a freed slot afterward — the fact the LLD note relies on
  to say freeing an id mid-session is safe.

## Plan

- [x] LLD note with the choice above, decided against content-hash reuse and for release-based
  freeing, because `GraphicId` identity is embedder-visible and two simultaneous placements of one
  image must never be able to collide.
- [x] `InternTable::free`, `GraphicsState::extras_by_graphic` / `track_extras`, and the shared
  `release` helper in `sweep` / `evict_oldest`.
- [x] Failing tests, then the fix, then the headless re-measure.
- [x] Owning docs reconciled (list above).

## Decisions

Content-hash reuse (generalizing `BUG-0079`'s URI-keyed dedupe to image bytes) is **rejected**, not
deferred: `GraphicsState::placement` resolves an id to a placement by `.find(|p| p.id == id)`, the
first match, so two images placed **at once** that shared a `GraphicId` would make the painter draw
the second occurrence at the first occurrence's position — a rendering bug, not a UX simplification
like the hyperlink hover-merge `BUG-0079` accepted. Release-based freeing keeps `GraphicId` exactly
as it is (a fresh counter value, per placement, per terminal, never reused) and adds no ambiguity.
No DEC record: this is the coordinator's brief already deciding the fallback was conditional
("if the extras id space cannot be freed piecemeal"), and it can be, so the condition never fires.

## Verification Plan

- Focused: the three new `oneterm-vt` tests above.
- Unit: `cargo test -p oneterm-vt --lib`, `--features vt-paranoid`, `--features regex`,
  `--no-default-features`.
- Package/API: `cargo build -p oneterm-vt --no-default-features --examples`,
  `cargo build -p oneterm-vt --all-features --examples`, `cargo run -p oneterm-vt --example
  headless`, `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features`,
  `python scripts/vt-public-api.py --check --no-doc`, `--diff-platforms`,
  `cargo package -p oneterm-vt --allow-dirty --list | python scripts/verify-dependency-graph.py
  --package-list -`.
- Integration: a headless measure of 70,000 resends of one Sixel image, extras table entries and
  live allocator bytes before and after the fix, with a throwaway counting allocator removed before
  commit.
- Full local gate: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Headless, release profile, one 20x4 terminal, a throwaway counting global allocator (removed
before commit), resending one 1x6 Sixel at a fixed cursor position (`CSI H` then the same `DCS q`
body, matching the `BUG-0079`-verify F7 probe shape):

| Resends | Before: extras entries | Before: live bytes | After: extras entries | After: live bytes |
| --- | --- | --- | --- | --- |
| 70,000 | 65,535 (full) | 13.42 MB | 258 | 7.85 MB |

After the 70,000 resends:

| Check | Before | After |
| --- | --- | --- |
| The last resend places | No (extras table full) | Yes |

258 is `MAX_PLACEMENTS` (256) plus the default entry plus one transient slot the eviction ordering
inside `place` costs on every cycle (a new extras value is interned before that call's own
`evict_oldest` frees the oldest placement's); the exact number is an implementation detail of that
ordering, not a contract, so the tests assert a generous bound (under 500) rather than this exact
figure.

`two_different_images_stay_distinct` and `a_released_placements_extras_entry_is_reused`: both pass
(`crates/vt/src/graphics/graphics_tests.rs`).

Gate: `pwsh scripts/ci-local.ps1`, `CARGO_BUILD_JOBS=3`, `target/release` deleted first, run from
the worktree's own `target` directory (a parallel agent was building elsewhere at the same time).
Final line recorded in the commit message and the handoff to the coordinator.

Gaps:

- No live 5-minute two-tab `measure.ps1 -AllocLog` re-measure on the built app, and no run of a
  real Sixel-emitting program (`img2sixel`, `chafa`, a file manager). The headless figure covers the
  mechanism, the same limitation `BUG-0079`'s own re-measure carried.
- A hostile stream can still fill the extras table with distinct simultaneous placements up to
  `MAX_PLACEMENTS` combined with distinct hyperlinks and distinct `OSC 8` URIs; the unchanged ladder
  drops further values past 65,535. Not new: the ladder's fallback was always there.
- The total-pixel-bytes budget across live placements (`IN-0029` `graphics.md` already notes 256
  placements have no such budget) is unchanged and out of this packet's scope.

## Handoff

Implemented and self-verified. Next: an independent verification pass, the way `BUG-0079` had one
(`evidence/BUG-0079-verify.md`), would be the natural follow-up before this is treated as
accepted/shipped; none is recorded yet in this packet.
