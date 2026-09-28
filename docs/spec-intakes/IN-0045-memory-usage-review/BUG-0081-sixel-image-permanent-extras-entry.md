# Work: Every Sixel image still takes a permanent extras entry

ID: BUG-0081
Intake: IN-0045
Created: 2026-09-28
Reworked: 2026-09-28, same day, three times. First after adversarial verification failed the first
design (release-time freeing, High). Second after a further adversarial pass failed the
mark-and-sweep design's own orchestration and trigger (High, twice: S1, S2). Third after a further
pass found the second rework's own sweep-and-retry had no rate limit (High: T1). See
[`evidence/BUG-0081-verify.md`](evidence/BUG-0081-verify.md) (all three passes, one file) and the
Decisions and Evidence and Gaps sections below.

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

Implemented four times: `3796cef0` (first design, release-time freeing, adversarially verified FAIL,
High), `5b45add0` (second design, mark-and-sweep, adversarially verified FAIL, High, twice: S1/S2),
`19a29175` (sweep-before-intern plus counted reuses, adversarially verified FAIL, High: T1, an
unrated retry), and the rework this packet now describes. This box tracks the packet's current
state; the Decisions and Evidence and Gaps sections carry the history the box cannot.

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
**and** 1,500,000 resends of one image the extras table settles near the sweep interval plus what is
genuinely live (measured at 4,098 entries, both times — not the small constant a naive reading of
"bounded" might suggest, and not a mark that keeps climbing either — see Decisions), the last resend
still places, and a later explicit `OSC 8` link and a later distinct image in the same terminal both
still get their cells. An id is never freed while any cell — on either screen, in scrollback, or on
a cursor's pen or erase cell — still names it, **and** a sweep never frees the very value it was
triggered by, before that value's own caller has had a chance to write it anywhere.

## Scope

- [x] In scope: `oneterm-vt`'s `InternTable` (a proven-safe `free`/reuse path: a free set, an
  idempotent `free`, and a periodic `sweep_unreferenced` that reads a caller-supplied live set
  rather than trusting one call site's bookkeeping; `since_sweep` counting every index miss, not only
  table growth); `Screen::collect_live_extras_ids` and `TerminalGrid::live_extras_ids`, which build
  that live set from the whole grid; `State::intern_extras`, which sweeps **before** interning and
  orchestrates from the hot call sites rather than once per `feed`, with a sweep-and-retry before
  accepting the table-full fallback; their rustdoc; the `IN-0029` LLD rows on graphics and extras;
  `crates/vt/docs/guide/10-limits.md` and `04-events.md`; the `docs/terminal-backend.md` risk row
  `BUG-0079` added; `graphics::assert_integrity`'s pre-existing column-vs-row false positive (F4),
  narrowed while here.
- [x] Out of scope: the hyperlink table and its own ladder (already fixed by `BUG-0079`); adding a
  content hash so two simultaneously placed copies of the same image bytes could share a
  `GraphicId` — rejected in the LLD note, not deferred; a total-pixel-bytes budget across live
  placements (`IN-0029` `graphics.md` already notes this is unbounded and calls it a separate,
  later concern); a compacting sweep that renumbers ids to give a tighter bound — rejected, see
  Decisions; restructuring `stamp`'s merged-hyperlink path to sweep-check inline — its borrow
  conflict with the row it is writing is unchanged, and `feed`'s end-of-batch check remains its
  backstop.

## Acceptance

- [x] A failing `oneterm-vt` test first: resend one Sixel image 70,000 **and** 1,500,000 times at a
  fixed cursor position; assert the extras table's `entries()` settles near the sweep interval both
  times, the last resend's cell still resolves to a graphic, `exhausted()` stays `0`, and a following
  explicit `id=` link and a distinct Sixel image both still get their cells.
  `terminal::tests::repainting_a_sixel_image_does_not_grow_the_extras_table`,
  `resending_1_5_million_times_keeps_the_extras_table_near_the_sweep_interval`.
- [x] Two images placed at once stay distinct (no shared `GraphicId`, no merged placement).
  `graphics::tests::two_different_images_stay_distinct`.
- [x] A released placement's cells are never recycled into an unrelated value while any cell still
  names them — eviction while still on screen (`v1`), eviction while in scrollback (`v8`), a row
  the sweep released after `IL`/`SD` split it outside its tracked extent (`v9`), a released
  **merged** hyperlink-and-graphic entry (`v4`), and a history trim that drops the anchor row while
  lower rows survive (`r3`). `graphics::tests::v1_*`, `v8_*`, `v9_*`, `v4_*`, `r3_*`.
- [x] A recycled id can never alias a *different*, still-live image and hide it from the renderer's
  paint loop. `graphics::tests::v2_every_live_placement_is_reachable_through_its_own_cells`.
- [x] The value whose own creation crosses the sweep threshold keeps its own cell, on both hot
  `intern_extras` call sites — an explicit link (`r1`) and a placed image (`r2`) — and a screen's
  cells and saved-cursor pen survive sweeps triggered entirely by the *other*, active screen (`r4`).
  `terminal::tests::r1_*`, `r2_*`, `r4_*`.
- [x] Each of `r1`-`r4` was confirmed to fail when the live-set collector returns an empty set
  (mutation A) and to pass when the sweep is disabled (mutation B); the two bound tests (70,000 and
  1,500,000 resends) were confirmed to pass under mutation A and fail under mutation B. Recorded in
  Evidence and Gaps.
- [x] A table stuffed with entries nothing references any more recovers via `intern_extras`'s
  sweep-and-retry at the exhausted step, without waiting for `RIS`.
  `terminal::tests::a_table_full_of_dead_entries_recovers_via_sweep_and_retry`.
- [x] A table stuffed with entries a real cell still references costs at most one scan per
  `TABLE_SWEEP_INTERVAL` failed `intern_extras` calls over 1,000 further failing image placements,
  not one scan per call, and `exhausted()` counts each failing call exactly once.
  `terminal::tests::intern_extras_bounds_its_scan_rate_when_the_table_is_live_full`, confirmed to
  fail when the `needs_sweep` gate on the retry is removed.
- [x] That same live-full table leaves a new image genuinely unplaced, with the one-time warning,
  and only `RIS` recovers it -- restoring the terminal-level coverage the table full of *dead*
  entries above replaced.
  `terminal::tests::a_table_full_of_live_entries_leaves_an_image_unplaced_until_ris`.
- [x] Freeing the same id twice never hands it out twice, in every build, not only under
  `debug_assert!`. `intern::tests::freeing_the_same_id_twice_does_not_alias_it`.
- [x] `vt-public-api.py --check` is unchanged (the new methods are all `pub(crate)`); recorded in
  `crates/vt/CHANGELOG.md` as a "Fixed" entry, no signature.
- [x] Headless re-measure: extras table entries and live allocator bytes (drained and undrained) at
  70,000 **and** 1,500,000 resends of one image, before and after each design; the scan cost of
  building the live set, at the coordinator's requested worst-case shape (100,000 rows x 200
  columns) and at the test terminal's own shape.
- [x] `cargo test -p oneterm-vt --features vt-paranoid` (the whole-history integrity walk) still
  passes, and now can actually fail on this bug's class of error: a new check
  (`Screen::assert_interned_ids_resolve`) asserts every cell's, and every cursor pen/erase cell's,
  extras id is not on the free list, not only that it was once issued.
  `graphics::tests::integrity_rejects_an_extras_id_freed_while_a_cell_still_names_it` proves the
  check fires, and it is what caught S1 immediately once the rework's code was written.

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
- [`evidence/BUG-0081-verify.md`](evidence/BUG-0081-verify.md) — both adversarial passes: the first
  (release-time freeing, F1/F2/F3/F4/F5/F6) and the second (`5b45add0`'s mark-and-sweep,
  S1/S2/S3/S4/S5/S6), reviewed before this rework.

### Documentation Action

Update required, three times over. The first pass (before `3796cef0`) chose release-time freeing
over content-hash reuse (rejected, see Decisions) and wrote the docs for it; adversarial verification
found that premise false (a placement's release does not mean its cells are gone), so every doc it
touched needed a second pass for the mark-and-sweep design (`5b45add0`): the same list, plus two the
first pass missed — `crates/vt/docs/guide/04-events.md` (F6: `GraphicReleased`'s doc already
overstated "the last cell is gone") and
[`low-level-design/sixel-extras-release.md`](low-level-design/sixel-extras-release.md) itself,
rewritten rather than amended. A second adversarial pass then failed the mark-and-sweep design's own
orchestration and trigger (S1: sweeping after interning could free the value just interned; S2:
counting only growth let the table fill and then stop sweeping forever), including one specific
overclaim in the packet's own Decisions section (the bound's scaling direction, and "a compacting
sweep" framed as the only way to tighten it) — a third pass corrects every doc that described the
bound as "a high-water mark that ratchets up" or used the word "quadratically" the wrong way round.

### Reconciliation

Done (this rework):

- [`low-level-design/sixel-extras-release.md`](low-level-design/sixel-extras-release.md): a new
  "Second design flaw (S1/S2, second adversarial pass)" section between "First design (rejected...)"
  and "Design (rework)", recording S1/S2/S3 and how each was fixed; the orchestration paragraph and
  the bound paragraph rewritten (sweep-before-intern, sweep-and-retry, `O(interval + live)` in place
  of the "ratchets... indefinitely" claim); Edge Cases, Measurements and Verification extended with
  S1/S2/S4 and R1-R4.
- `crates/vt/src/intern.rs` rustdoc: `since_sweep`'s field doc and `needs_sweep`'s doc now state that
  every index miss counts, not only growth, and that the check runs before interning, not after.
- `crates/vt/src/terminal/mod.rs`: `State::intern_extras`'s doc rewritten around the corrected
  ordering (sweep first) and the sweep-and-retry addition.
- `docs/terminal-backend.md` § 13 Risks and `crates/vt/CHANGELOG.md` `[Unreleased]` "Fixed": both
  name the second adversarial finding (S1/S2) alongside the first (release-time freeing) and replace
  the ~20,000/258 figures with the settled ~4,100-entry bound measured at both 70,000 and 1,500,000
  resends.
  `IN-0029` `graphics.md` § "Liveness and the release signal": the orchestration sentence corrected
  (swept before interning; a reused slot counts toward the trigger the same as growth).
- `crates/vt/docs/guide/10-limits.md`: the extras paragraph rewritten to describe the interval
  counting reuses and the table settling, not ratcheting.
- `crates/vt/src/terminal/terminal_tests.rs`: the 70,000-resend test's own doc comment and bound
  (`< 5,000`, down from `< 30,000`) corrected to match; `crates/vt/CHANGELOG.md`,
  `10-limits.md` and `IN-0029` `graphics.md`/`terminal-backend.md` all had this second, corrected
  pass; `cell-and-style.md` needed no change (its exception paragraph never stated a bound or
  scaling claim to correct).
- `docs/spec-intakes/IN-0045-memory-usage-review/IN-0045.md`: the `BUG-0081` bullet updated with
  both reworks and the settled numbers.
- Reviewed, no change from the previous pass: `crates/vt/src/graphics/mod.rs` and
  `crates/vt/src/graphics/placement.rs` rustdoc (unchanged by this rework, already correct);
  `crates/vt/docs/guide/04-events.md` (F6's fix from the previous pass is still accurate — S1/S2 are
  orchestration and trigger defects, not release-semantics ones); `crates/vt/README.md`; `IN-0029`
  `high-level-design.md`; the parity corpus.

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
- `crates/vt/src/terminal/mod.rs` `State::intern_extras` is the one path `place`'s graphic-only
  entry and `set_hyperlink` go through; both write the id it returns to a cell or a pen
  **synchronously**, before control returns to anything that could call `intern_extras` again — the
  fact that makes "sweep before interning" sufficient: nothing exists to race it, because nothing
  else runs between one call's sweep and its own write.

## Plan

- [x] LLD note with the choice above (release-time freeing vs. content-hash reuse), decided against
  content-hash reuse and, at first, for release-based freeing.
- [x] First implementation: `InternTable::free`, `GraphicsState::extras_by_graphic` / `track_extras`,
  a shared `release` helper in `sweep` / `evict_oldest`. Committed `3796cef0`.
- [x] First adversarial verification: FAIL, High (`evidence/BUG-0081-verify.md`, F1/F2). Reopened as
  acceptance rework of this same packet, not a new `BUG`.
- [x] Second implementation (mark-and-sweep): `extras_by_graphic`/`track_extras`/the shared `release`
  helper removed; `graphics/mod.rs` and `graphics/placement.rs` reverted to `main` except the F4
  column-scoping fix. `InternTable` gains a `free`/`sweep_unreferenced` pair; `Screen`/`TerminalGrid`
  gain the live-set scan; `State::intern_extras` orchestrates the sweep from the hot call sites.
  V1/V4/V8/V9/V2 regression tests; the F5 double-free test; the F3 paranoid-check regression.
  Committed `5b45add0`.
- [x] Second adversarial verification: FAIL, High, twice (`evidence/BUG-0081-verify.md`, "Second
  pass", S1/S2). Reopened again as acceptance rework of the same packet.
- [x] This rework: `State::intern_extras` sweeps before interning, not after (S1), with a
  sweep-and-retry before the table-full fallback; `InternTable::intern` counts a free-list reuse
  toward `since_sweep`, not only growth (S2). `R1`-`R4` regressions (S3), each confirmed to fail
  under an empty-live-set mutation and pass under a sweep-disabled mutation; a 1,500,000-resend bound
  test alongside the existing 70,000-resend one.
- [x] Headless re-measure (entries, live bytes drained/undrained, at both resend counts) and the
  scan-cost measurement.
- [x] Owning docs reconciled a third time (list above), including correcting the previous rework's
  own wrong claim about the bound's scaling direction.

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
bookkeeping being complete — the exact thing the first design got wrong. This survived the second
adversarial pass (S4: the live set is complete) and stays in this rework.

**The mark-and-sweep design's first shipped version (`5b45add0`) is also rejected**, by a second
adversarial verification pass, on two independent High findings in its own orchestration:
`State::intern_extras` swept *after* interning, so the value that crossed the sweep threshold was
freed before its own caller could write it anywhere (S1); and `since_sweep` counted only table
growth, so a sweep's own free list was consumed silently by reuse and the table still filled to
65,535 — around 558,000 resends of one image — after which `since_sweep` could never reach the
interval again and no sweep ever ran again, reproducing the original bug's exact symptom, only about
eight times later (S2). **This rework's own previous text here was also wrong**, and is corrected
rather than quietly replaced: it described the (then-unknown-buggy) bound as "ratchet[ing] up ...
indefinitely" and claimed the resends-to-reach-the-cap relationship was `sqrt`-shaped in a way that
had the direction backwards — the table's *entries* grew as the square root of the resend count
under the S2 defect, not the other way round, and that whole framing is moot now that S2 counts
reuses.

**Chosen: count every index miss toward the sweep trigger, not only growth, and sweep before
interning, not after.** Both are one-line changes to the already-chosen design, not a new direction:
`since_sweep` increments on the free-list-reuse branch of `intern` exactly as it already did on the
growth branch, and `State::intern_extras` calls the sweep check before `interner.extras(value)`
instead of after. Measured: 4,098 entries at both 70,000 and 1,500,000 resends — `O(interval + live)`,
not a climbing mark and not the unsafe 258 the *first* design measured. `State::intern_extras` also
sweeps once and retries before accepting the table-full fallback, so a counter that happens to fall
short of the interval right as the table genuinely fills cannot strand it for the rest of the
session. A compacting sweep that renumbers live ids would still be the only way to tighten the bound
below `interval + live` for a workload whose live set itself is large, and that remains rejected for
the same reason as before — it is the exact hazard the "styles and extras never move" rule exists to
prevent — but it is no longer needed to fix the bug this packet is about, only to shrink an already-
safe margin. No DEC record: the interval (4,096) is a tuning knob within an accepted design, not a
choice future work must inherit, and the LLD note carries the reasoning if it needs revisiting.

**The sweep-and-retry that same rework added is also rejected as written (`19a29175`)**, by a third
adversarial verification pass, on one High finding: it swept and retried unconditionally on every
failing `intern_extras` call, with no rate limit. When the table is genuinely full of entries a real
cell still references -- hostile-reachable in one burst, or plausible from a long session that
prints many distinct `OSC 8` URLs -- a sweep frees nothing, the retry fails the same way, and the
next failing call does the identical unconditional sweep again: every failure became a full
`O(history)` scan, where `main` fell back to id 0 in `O(1)`. Measured: 5.7 ms per image at the
default 10,000-row scrollback, 47.7 ms at 100,000 rows; 10,000 resends against a live-full table
took 57-60 s (measured independently by the verifier and by this rework's own re-run) where `main`
took a constant amount of time regardless of table size; a hostile 64 KB read of minimal Sixels held
the terminal lock for roughly 18 s at the default scrollback, or about 150 s at 100,000 rows. This
rework's own earlier suggestion ("sweep once and retry") was the right shape without a rate limit,
which was the part actually missing.

**Chosen: count the exhausted branch's own failure toward `since_sweep` too, and gate the retry on
`needs_sweep`.** `InternTable::intern` is split into `try_intern` (the ladder alone, `Option<u16>`,
counting every miss including a final failure) and `record_exhausted` (the counting and the
warning, called exactly once per logical failed attempt); `intern` itself calls both in sequence, so
its existing behaviour and signature are unchanged. `State::intern_extras` calls `try_intern`
directly for both the first attempt and the post-sweep retry, so `record_exhausted` runs once per
`intern_extras` call regardless of how many `try_intern` calls happened inside it -- fixing the
double count the verifier also found (`exhausted()` read 20,002 for 10,000 failed resends before
this fix, not 10,000). Gating the retry on `needs_sweep` bounds the extra scan to once per
`TABLE_SWEEP_INTERVAL` (4,096) failed attempts: measured after the fix, the same 10,000-resend
live-full run took 25.3 ms across 3 sweeps, not 60.6 s across 10,001. **The per-`intern_extras` cost
when the table is genuinely live-full is therefore `O(1)` amortised** -- roughly 1.4 µs per intern at
the default scrollback and 12 µs at 100,000 rows (the measured per-scan cost divided by the
interval) -- **not the `O(1)` worst-case `main` had**, and that remains an accepted trade: `main`
could never recover once its tables filled with content that later went dead; this design can,
at the cost of an occasional bounded scan instead of none, ever. No DEC record, for the same reason
as the interval choice above: this is a correctness fix to an already-accepted design, not a new
choice future work must inherit.

## Verification Plan

- Focused: the new `oneterm-vt` tests above (V1/V4/V8/V9/V2, R1/R2/R3/R4, the double-free test, the
  paranoid-check regression, the 70,000- and 1,500,000-resend bounds, the dead-table recovery test,
  the live-full scan-rate bound, the live-full unplaced-until-`RIS` test).
- Mutation: each of R1-R4 confirmed to fail with the live-set collector returning an empty set, and
  to pass with the sweep disabled; the two bound tests confirmed to pass under the first mutation
  and fail under the second; the live-full scan-rate test confirmed to fail (1,001 sweeps instead of
  at most 2) when the `needs_sweep` gate on the retry is removed. All mutations applied and reverted
  locally, never committed.
- Unit: `cargo test -p oneterm-vt --lib`, `--features vt-paranoid`, `--features regex`,
  `--no-default-features`.
- Package/API: `cargo build -p oneterm-vt --no-default-features --examples`,
  `cargo build -p oneterm-vt --all-features --examples`, `cargo run -p oneterm-vt --example
  headless`, `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features`,
  `python scripts/vt-public-api.py --check --no-doc`, `--diff-platforms`,
  `cargo package -p oneterm-vt --allow-dirty --list | python scripts/verify-dependency-graph.py
  --package-list -`.
- Integration: a headless measure of 70,000 **and** 1,500,000 resends of one Sixel image, extras
  table entries and live allocator bytes (drained and undrained) before and after each design; the
  live-set scan cost at two shapes; a release-profile re-measure of the live-full case (65,535
  entries each on a real cell, 10,000 further resends at the default 10,000-row scrollback), before
  (the unrated retry, reproduced locally) and after; throwaway counting allocators and throwaway
  `#[ignore]`d timing tests, all removed before commit.
- Full local gate: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

**First design (`3796cef0`), adversarially FAILED.** Headline: 258 extras entries after 70,000
resends (a tight bound), but V1/V8/V9/V4 each turned a released-but-still-live cell into an unrelated
link, and V2/F2 showed a recycled id hiding a live image from the renderer.

**Second design (`5b45add0`, mark-and-sweep), adversarially FAILED, twice.** S1 (High): a debug build
panics with `extras id ExtrasId(4096) is on the free list but a cell or pen still names it` on every
4,096th distinct value; a release build silently resolves that cell or pen to whatever is interned
next. S2 (High): the table fills to 65,535 at about 558,000 resends of one image and then never
sweeps again — the original bug's exact symptom, delayed by roughly a factor of eight (about 5.2
hours at 30 resends a second).

**Third design (`19a29175`, sweep-before-intern plus counted reuses), adversarially FAILED once
more.** S1, S2 and S3 closed cleanly (the live set is complete; the merged path is safe; the two
bound tests hold at 70,000 and 1,500,000 resends). T1 (High): the sweep-and-retry this rework's own
previous Decisions text proposed as S2's fix had no rate limit, so a table full of *live* entries
turned every failing intern into a full history scan -- 5.7 ms per image at the default scrollback,
57-60 s for 10,000 resends, where `main` took `O(1)` regardless. Full findings, reproductions and
all three passes' measurements in
[`evidence/BUG-0081-verify.md`](evidence/BUG-0081-verify.md).

**This rework, headless, release profile, one 20x4 terminal, a throwaway counting global allocator
(removed before commit), resending one 1x6 Sixel at a fixed cursor position (`CSI H` then the same
`DCS q` body, matching both adversarial passes' probe shape):**

| | Extras entries at 70,000 resends | Extras entries at 1,500,000 resends |
| --- | --- | --- |
| Before (main, `eed33058`) | 65,535 (full) | not run (already full well before this point) |
| First design (`3796cef0`, rejected: frees on release) | 258 | not run |
| Second design (`5b45add0`, rejected: sweeps after interning, counts growth only) | 20,481 | 65,535 (full, stalled at `since_sweep = 4,094`) |
| Third design (`19a29175`, rejected: unrated retry, T1) | 4,098 | 4,098 |
| This rework (S1/S2/T1 fixed) | 4,098 | 4,098 |

| | Live bytes, undrained | Live bytes, drained |
| --- | --- | --- |
| Before (main) | 13.42 MB (70,000 resends) | not separately measured |
| First design | 7.85 MB (70,000 resends) | not separately measured |
| This rework, 70,000 resends | 7.674 MB | 1.334 MB |
| This rework, 1,500,000 resends | 131.775 MB | 1.334 MB |

"Drained" calls `take_graphics()` after every feed, as an embedder does; "undrained" never does —
the growth in the undrained 1,500,000-resend figure is the `pending: Vec<Arc<GraphicData>>` queue an
embedder that never drains accumulates, a pre-existing characteristic unrelated to this bug (the
same queue, at the same rate, existed before any of these three designs). The last resend still
places at both resend counts, in every run (`exhausted()` stays `0`).

**Live-full case (T1), headless, release profile, one 200-column terminal, default (10,000-row)
scrollback, the extras table filled to 65,535 entries each written onto a real cell (never trimmed:
placed in the oldest rows still in history), then 10,000 further Sixel resends at a fixed `CSI H`:**

| | Total time | Sweeps | `exhausted()` |
| --- | --- | --- | --- |
| Verifier's own measure of `19a29175` as committed (unrated retry **and** double-counted exhaustion) | 57.0 s | 10,000 | 20,002 |
| This rework's re-run, unrated retry only (this rework's `try_intern`/`record_exhausted` split -- which alone fixes the double count -- left in place, only the `needs_sweep` gate on the retry reverted) | 60.6 s | 10,001 | 10,000 |
| This rework, as committed | 25.3 ms | 3 | 10,000 |

The two "before" rows isolate T1's two defects separately: the verifier's own number on the commit as
it stood shows both (the count is double, matching their 20,002); this rework's own re-run of the
scan-rate defect alone, with the counting fix already in place, confirms `exhausted()` reads
correctly (10,000, not double) with or without the rate limit -- the counting fix and the rate-limit
fix are independent, and both are needed, but neither depends on the other having been re-broken to
demonstrate.

**Scan cost** (`TerminalGrid::live_extras_ids`, release profile; the 100,000 x 200 and 20x4 rows
measured with a throwaway `#[ignore]`d test removed before commit; the 10,000 x 200 row is the
second adversarial pass's own measurement, not independently re-run this pass, and agrees with the
live-full-case total above: 3 sweeps in 25.3 ms is 8.4 ms/sweep, the same order as 5.7-7.6 ms
measured elsewhere at this shape):

| Shape | Scan | Scan + `sweep_unreferenced` |
| --- | --- | --- |
| 10,000 x 200 (the default scrollback) | 3.0 ms (second pass) | 3.2 ms (second pass) |
| 100,000 x 200 columns (the coordinator's requested worst case) | 32.4 ms | not separately measured |
| 20x4 (this packet's own test terminal) | 700 ns | not separately measured |

The 1,500,000-resend test itself ran in about 1.3 s in release, comfortably under the coordinator's
~10 s threshold for using the full size rather than falling back to 300,000. At
`TABLE_SWEEP_INTERVAL = 4,096`, this cost is paid at most once per 4,096 new values handed out (a
push, a reuse, **or a failed attempt against a live-full table** — the last is what T1 fixed) —
rare for an ordinary session, and the worst realistic cost (a full-scrollback terminal under a
sustained distinct-hyperlink-or-image stream) rather than the common one. Amortised per
`intern_extras` call when the table is genuinely live-full: about 1.4 µs at the default scrollback,
about 12 µs at 100,000 rows (scan cost divided by the interval).

**Mutation testing (S3): which test kills which mutation.** Mutation A stubs
`TerminalGrid::live_extras_ids` to return an empty set (the sweep frees every live id too); mutation
B stubs `InternTable::needs_sweep` to always return `false` (the sweep never runs). Each applied
locally to this rework's own source, one at a time, then reverted; neither is committed.

| Test | Mutation A (empty live set) | Mutation B (sweep disabled) |
| --- | --- | --- |
| `r1_the_value_that_triggers_a_sweep_keeps_its_own_cell` | **fails** (paranoid panic) | passes |
| `r2_sweeping_to_make_room_never_frees_the_new_images_own_entry` | **fails** (paranoid panic) | passes |
| `r3_a_trimmed_anchors_surviving_lower_rows_are_not_recycled_by_a_sweep` | **fails** (paranoid panic) | passes |
| `r4_the_other_screens_cells_and_saved_pen_survive_sweeps_on_the_active_screen` | **fails** (paranoid panic) | passes |
| `v1`/`v2`/`v4`/`v8`/`v9` | passes (none of them creates 4,096 distinct values, so no sweep runs to be mis-fed) | passes |
| `repainting_a_sixel_image_does_not_grow_the_extras_table` (70,000 resends) | passes (over-freeing does not corrupt this test's single-cell check) | **fails** (table fills to 65,535) |
| `resending_1_5_million_times_keeps_the_extras_table_near_the_sweep_interval` | passes | **fails** (table fills to 65,535) |

R1-R4 are what proves S1 fixed; the two bound tests are what proves S2 fixed. Neither pair would
catch the other's defect, which is why both were required rather than either alone.

**A third mutation, this pass: the `needs_sweep` gate removed from `intern_extras`'s retry** (`if
self.interner.extras.needs_sweep() { ... }` replaced with an unconditional `if true`, reproducing
`19a29175` as it was committed). `intern_extras_bounds_its_scan_rate_when_the_table_is_live_full`
**fails** (1,001 sweeps over 1,000 attempts against a live-full table, instead of at most 2) and
passes with the gate restored; `a_table_full_of_live_entries_leaves_an_image_unplaced_until_ris`
passes either way, because it checks only one resend's correctness, not the sweep rate — it is T5's
regression, not T1's, and the two are deliberately different tests for that reason.

`v1`/`v4`/`v8`/`v9`/`v2`, the double-free test, and the paranoid-check regression all still pass
(`crates/vt/src/graphics/graphics_tests.rs`, `crates/vt/src/intern_tests.rs`), unaffected by this
rework's changes.

Gate: `pwsh scripts/ci-local.ps1`, `CARGO_BUILD_JOBS=3`, `target/release` deleted first, run from
the worktree's own `target` directory (a parallel agent was building elsewhere at the same time).
Final line pasted into the commit message and the handoff to the coordinator.

Gaps:

- No live 5-minute two-tab `measure.ps1 -AllocLog` re-measure on the built app, and no run of a
  real Sixel-emitting program (`img2sixel`, `chafa`, a file manager). The headless figure covers the
  mechanism, the same limitation `BUG-0079`'s own re-measure and both earlier designs' evidence
  carried.
- A hostile stream can still fill the extras table with distinct simultaneous placements up to
  `MAX_PLACEMENTS` combined with distinct hyperlinks and distinct `OSC 8` URIs; the unchanged ladder
  drops further values past 65,535. Not new: the ladder's fallback was always there.
  `intern_extras`'s sweep-and-retry makes that case recover once any of that content has gone dead,
  and now (T1) does so at a bounded, amortised cost rather than a full scan per failing call: about
  1.4 microseconds per call at the default 10,000-row scrollback, about 12 microseconds at 100,000
  rows, whether the table is genuinely live-full or merely dead-full — see Decisions.
- The total-pixel-bytes budget across live placements (`IN-0029` `graphics.md` already notes 256
  placements have no such budget) is unchanged and out of this packet's scope.
- `evidence/BUG-0081-verify.md`'s first-pass gap (the history-trim path reasoned, not run) is now
  closed by `r3`, which runs it directly and passes.

## Handoff

Implemented, reworked three times — once after the first design's release-time freeing failed
adversarial verification (High), once after the second design's own mark-and-sweep orchestration and
trigger failed a further adversarial pass (High, twice: S1/S2), and once after that rework's own
sweep-and-retry ran an unrated `O(history)` scan on every failing intern against a live-full table
(High: T1) — and self-verified against all three passes' reproductions plus mutation testing
confirming the new regressions actually exercise the sweep and the retry's rate limit. A fourth
independent verification pass is the natural next step before this is treated as accepted/shipped;
none is recorded yet in this packet. Three prior passes each found a real, serious defect the
previous implementer's own testing had missed; a fourth pass earns no presumption of safety from that
history alone.
