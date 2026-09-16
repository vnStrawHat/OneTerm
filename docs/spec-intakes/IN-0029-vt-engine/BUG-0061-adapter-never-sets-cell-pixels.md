# Work: the adapter never tells the engine its cell size, so `CSI 14 t` answers zero

ID: BUG-0061
Intake: [`IN-0029`](IN-0029.md)
Created: 2026-09-16

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: **bug**
- Risk lane: normal
- Spec Intake, when required: [`IN-0029`](IN-0029.md)

Normal lane. No trust boundary, no persisted schema, no data loss path. The change adds one
method to an internal trait (`oneterm_terminal::TerminalInput`, not a published surface) and
makes two numbers in one reply non-zero. The engine itself is untouched: `set_cell_pixels` and
the `CSI 14 t` handler already exist and are already tested in `crates/vt`.

## Outcome

A program running in OneTerm that asks `CSI 14 t` ("how many pixels is the text area?") gets the
real answer instead of `CSI 4;0;0 t`, from the first frame onwards, and gets a fresh answer after
a font-size change or a DPI-scale change.

The owner's report (2026-09-16, OpenTUI examples v0.5.10, "Native Image Lab"): OneTerm draws the
dragon as a quadrant-block mosaic while Windows Terminal draws it as a Sixel image. The selection
logic in OpenTUI is `DA1 contains 4` **and** `CSI 14 t` returns a non-zero pixel size. OneTerm
passes the first test -- `CSI ? 62;4;22 c` -- and fails the second, so the renderer falls back to
half/quadrant blocks. Nothing is wrong with the engine's Sixel support; the program is told the
window has no pixels, so it never sends a Sixel at all.

Root cause, established by grep over `crates/`: `Terminal::set_cell_pixels`
(`crates/vt/src/terminal/mod.rs:506`) has exactly one caller in the whole workspace, and it is
`crates/vt/src/terminal/terminal_tests.rs:1000`. `Terminal::new` starts `cell_pixels` at `(0, 0)`
(`mod.rs:272`), the `CSI 14 t` arm multiplies it by the grid
(`crates/vt/src/terminal/dispatch.rs:948-952`), and no embedder code ever sets it. The engine
this one replaced had the same hole, so this is not a regression from `IN-0029`; it is a contract
the adapter was never wired for. `crates/vt/docs/guide/08-graphics.md:112` states the embedder's
duty in one sentence ("Tell the engine your real cell size with `Terminal::set_cell_pixels`
whenever the font changes"), and OneTerm, the engine's own first embedder, does not do it.

## Scope

- [x] In scope:
  - `TerminalInput::set_cell_pixels(&self, width: u16, height: u16)` -- one new trait method,
    implemented by `PtySession<O>` and by `FakeTerminalSession`.
  - `TerminalModel::set_cell_pixels` -- the engine-lock hop, beside `resize_grid`.
  - `crates/terminal-view/src/render/element.rs` prepaint: push the cell size whenever the
    measured metrics change, **before** the grid-size check that calls `resize`.
  - Tests at both levels (see Verification Plan), including a probe helper that returns the
    engine's reply bytes so a test can assert the `CSI 14 t` answer rather than an internal field.
  - `docs/terminal-backend.md`: the embedder-contract sentence, where the adapter's engine-lock
    duties are listed.
- [x] Out of scope:
  - **XTSMGRAPHICS (`CSI ? Pi;Pa;Pv S`).** Deferred, not forgotten: `chafa` and `notcurses` use
    it to size an image, and the engine answers nothing today. It belongs to
    `US-0106` (conformance queries, `IN-0039`), which owns the query-reply surface, its
    deviation table and the checklist rows; adding a second author to that surface from a
    `terminal-view` bug packet is how two half-answers get shipped. Follow-up recorded below.
  - **The Sixel placement scale** (see "The one thing this does not fix"). The engine places at
    the VT340 virtual cell and the painter mirrors it; changing that changes covered-cell counts
    and cursor placement, which is an engine packet, not this one.
  - `CSI 16 t` (cell size in pixels) and `CSI 18 t` (grid in cells). `18 t` already answers
    correctly and must keep answering exactly the same bytes; `16 t` is unhandled today and is
    `US-0106`'s to add.

## The signature: a new method, not a wider `resize`

The two candidates were `resize(rows, cols, cell_px)` and a separate `set_cell_pixels`. The
separate method wins on both counts that matter.

**Correctness first.** `PtySession::resize` opens with `if self.model().needs_resize(rows, cols)`
and returns `Ok(())` when the grid is unchanged (`crates/terminal/src/session.rs:634-640`). A
DPI-scale change is exactly that case: the window's scale factor moves, `CellMetrics::device`
moves with it, the *logical* cell size and therefore the row and column counts do not. Folding
the pixel size into `resize` would drop the update in the one scenario the packet exists to
cover, or force the guard to be rewritten to compare pixels too -- at which point the two
concerns are entangled in a method whose documented order (`pty_resize` first, then
`resize_grid`) has nothing to do with them.

**Then size.** The trait method is implemented twice and called once. Widening `resize` edits
every call site instead: `element.rs:111`, `session.rs:826` and `:832`, `ssh/session.rs:897`,
`local-shell/src/session_tests.rs:230`, and `plan_cache.rs:883`, `:993`, `:1105` -- eight sites,
none of which has anything to say about pixels, plus the same two implementations.

## The one thing this does not fix

`CSI 14 t` will report the real cell; the **Sixel placement math does not use it**, and that is
deliberate in the accepted design. `crates/vt/src/graphics/placement.rs:45-46` computes an
image's cell extent in `VIRTUAL_CELL = (10, 20)`, and
[`low-level-design/graphics.md`](low-level-design/graphics.md) says so in as many words: "The
engine never learns the real font cell size. The renderer rescales by `cell_width / 10` and
`line_height / 20`." `crates/terminal-view/src/render/element.rs:359` is that rescale, through
`oneterm_terminal::SIXEL_VIRTUAL_CELL`.

So a program that sizes an image from the new `CSI 14 t` answer -- one cell = `w x h` real
pixels -- has it drawn at `w/10` by `h/20` of the size it intended; for a typical 9x18 cell that
is a uniform 0.9 shrink, and for a 9x19 cell a 0.90 x 0.95 one. The image appears, at very
slightly the wrong size, which is what this packet promises and all it promises.

Reporting `(10, 20)` instead would make the geometry exact and was rejected: it tells a
DPI-aware program the window is a different size than it is, it would make `chafa` and any
pixel-budget renderer pick the wrong resolution, and it papers over the real defect. The real
fix is for the placement to use the embedder's cell size once it has one -- an engine change,
recorded as a follow-up, with `crates/terminal/src/sixel_tests.rs` as the behaviour that must
be re-pinned.

**There is one source of truth for the cell size and this packet does not add a second.**
`RenderState::metrics` (`crates/terminal-view/src/render/state.rs:217`) is the cache, keyed on
font, size, line-height factor, width override and scale factor; the painter, the geometry and
now the engine all read that one `CellMetrics`. What is reported is `CellMetrics::device`, the
snapped whole-device-pixel size (`metrics.rs:72` `snap_device`) -- the same value the quad edges
and the shape rasteriser use, not a second rounding of the logical value.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/graphics.md`](low-level-design/graphics.md) -- the `VIRTUAL_CELL` placement
  contract and "the engine never learns the real font cell size". Still correct after this
  change: `cell_pixels` is a *report*, not a placement input. Not changed; the interaction is
  recorded here and in the follow-up instead.
- `docs/terminal-backend.md` §5.2-5.3 -- what the adapter owes the engine under the lock (feed,
  drain, `take_graphics`, `resize`). It does not mention the cell size at all, which is the doc
  half of this defect. **Changed.**
- `crates/vt/docs/guide/08-graphics.md:104-114` -- the embedder contract, already correct and
  already explicit. **Not changed**; the code is what was wrong.
- `crates/vt/src/terminal/mod.rs:503-508` and `dispatch.rs:946-957` -- `set_cell_pixels` and the
  `14 t` / `18 t` arms. Correct as written; no engine change in this packet.

### Documentation Action

Update required: `docs/terminal-backend.md` only. It is the doc an adapter author reads, it lists
the engine calls the adapter owns, and it omitted this one -- so the omission was reproduced in
the code. `graphics.md` and guide chapter 8 already describe the correct contract and are left
alone (a no-change reason, not an oversight).

Reason: the defect is a missing call, not a wrong contract. Exactly one doc described the
adapter's duties without naming this call.

### Reconciliation

Changed: `docs/terminal-backend.md` §5.3 -- a paragraph beside the resize hop naming the cell-size
duty, where it is pushed from, why it is not folded into `resize`, and that the reported size is a
report rather than a placement input.

The three no-change reasons still hold. `low-level-design/graphics.md` describes placement, which
this packet does not touch; `crates/vt/docs/guide/08-graphics.md` already states the embedder
contract correctly and is what the fix now obeys; the engine's `set_cell_pixels` and the `14 t` /
`18 t` arms are unchanged, and `crates/vt` has no edit in this branch.

## Context

The push point is `TerminalElement::prepaint` (`element.rs:104-118`): `state.metrics(window, cx)`
is already computed there, before the `state.last_grid != Some(geometry.size)` check that calls
`session.resize`. Putting the cell-size push immediately after the metrics call and before that
check satisfies "before the first resize", so the first `CSI 14 t` after spawn is already
correct, and gives the same change-guarded shape the grid already has (`last_grid` gains a
sibling, `last_cell_pixels`).

`FakeTerminalSession` owns a real `oneterm_vt::Terminal` (`GridFixture`), so mirroring the trait
method there is two lines and makes the reply assertable from the view's own tests.
`GridFixture::feed` drops the `EventBatch`, so replies are unobservable today; the test needs a
`feed_replies` that collects `VtEvent::Reply` spans -- the same thing `crates/vt`'s own test
`Session::replies` does.

## Plan

- [x] `TerminalModel::set_cell_pixels`, beside `resize_grid`, same lock discipline.
- [x] `TerminalInput::set_cell_pixels` + the `PtySession` implementation.
- [x] `FakeTerminalSession::set_cell_pixels` mirrors it into its own engine; add
      `GridFixture::feed_replies` / `FakeSessionProbe::feed_replies`.
- [x] `element.rs` prepaint pushes on a metrics change, before the resize check;
      `RenderState::last_cell_pixels`.
- [x] Tests (below), `docs/terminal-backend.md`, gate.

## Decisions

No new decision record. The choice of a separate trait method is a local API shape recorded
above, not a rule future work must inherit; `DEC-0012` (cell-anchored graphics) and the
`VIRTUAL_CELL` contract are unchanged.

## Acceptance

Measurable by a hostile verifier from a clean checkout of this branch:

- [x] `grep -rn "set_cell_pixels" crates/ --include=*.rs` lists a caller outside `crates/vt`.
- [x] `cargo test -p oneterm-terminal-view cell_pixels` passes, and its assertion is on reply
      **bytes**: after the view's own resize path, feeding `\x1b[14t` to the session returns
      exactly `ESC [ 4 ; <rows*h> ; <cols*w> t`, with `rows`/`cols` the grid the view pushed and
      `h`/`w` the cell's device pixels, and both numbers strictly greater than zero.
- [x] The same test asserts `\x1b[18t` still returns `ESC [ 8 ; <rows> ; <cols> t` -- unchanged
      bytes, so the fix did not disturb the neighbouring arm.
- [x] A metrics change (the scale-factor path's mechanism: the shared `MetricsKey` cache) pushes
      a new cell size and the `CSI 14 t` reply changes accordingly, without a grid change being
      required to trigger it.
- [x] `cargo test -p oneterm-terminal cell_pixels` proves the real `PtySession` path, not only
      the fake: `set_cell_pixels` on the session reaches the engine behind the model lock.
- [x] `pwsh scripts/ci-local.ps1 -Full` is green.

## Verification Plan

- Unit (`crates/terminal`): on a `PtySession` over a fake owner, `set_cell_pixels(9, 18)` then
  `CSI 14 t` fed to the shared engine replies `\x1b[4;432;720t` at 24x80; `CSI 18 t` replies
  `\x1b[8;24;80t`.
- Integration (`crates/terminal-view`, GPUI headless): through `Harness` -- a real window, real
  prepaint, the view's own metrics and resize path -- the reply matches the grid and the device
  cell, both non-zero; then a metrics change updates it.
- E2E: **not run here** (`e2e_proof 0`). The owner runs it after merge; steps below.
- Platform: not applicable; no platform-specific code path.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

### E2E criterion (owner, after merge)

1. Build OneTerm from `main` with this packet merged (`cargo build -p oneterm-app --profile
   fast-dev`) and start it; do not reuse the running instance.
2. In a local shell tab, run `opentui-examples.exe` (OpenTUI examples v0.5.10) and open
   **Native Image Lab**.
3. Expected: the dragon is drawn as a **pixel image**, not as a quadrant-block mosaic -- the
   same thing Windows Terminal shows. The `WEBP 384x576` label is unchanged, because the image
   itself is unchanged.
4. Known and accepted for this packet: the image may appear about 10 % smaller than in Windows
   Terminal ("The one thing this does not fix"). A *mosaic* is a failure; a slightly small
   picture is this packet's success.
5. Cross-check with a second program if one is at hand: `sixel` output from `img2sixel` must be
   unaffected, since it does not consult `CSI 14 t`.

## Follow-ups

- **The placement scale.** Let the engine place images by the embedder's cell size when one has
  been set, falling back to `VIRTUAL_CELL` when it is `(0, 0)`. Touches
  `crates/vt/src/graphics/placement.rs`, `sixel.rs`'s `cursor_rows` rule (the conhost agreement
  is stated in 20-pixel units), `element.rs`'s rescale and
  `crates/terminal/src/sixel_tests.rs`. Owner: a new `IN-0029` packet.
- **XTSMGRAPHICS.** `CSI ? 2;1;0 S` -> `CSI ? 2;0;<maxw>;<maxh> S` (maximum image geometry) and
  `CSI ? 1;1;0 S` -> colour registers, for `chafa` and `notcurses`. Owner: `US-0106`
  (`IN-0039`), which already owns the query-reply surface.

## Evidence and Gaps

Commits on `fix/adapter-cell-pixels`: the packet, then one implementation commit
(`crates/terminal/src/{model,session,test_support}.rs`,
`crates/terminal-view/src/render/{element,state,element_tests}.rs`, `docs/terminal-backend.md`;
169 insertions, 1 deletion).

- `cargo test -p oneterm-terminal cell_pixels` -- passed.
  `session::tests::cell_pixels_reach_the_engine_and_csi_14_t` pins the defect first
  (`CSI 14 t` -> `ESC [ 4;0;0 t` before the call), then `set_cell_pixels(9, 18)` ->
  `ESC [ 4;432;720 t` at 24x80, and `CSI 18 t` -> `ESC [ 8;24;80 t` unchanged.
- `cargo test -p oneterm-terminal-view cell_pixels` -- passed.
  `render::element::element_tests::cell_pixels_reach_the_session_so_csi_14_t_answers` drives a
  real headless GPUI window: after the view's own prepaint the reply is
  `ESC [ 4;<rows*h>;<cols*w> t` for the grid the view pushed and the device cell the painter used,
  both non-zero; `CSI 18 t` is unchanged; then a metrics change alone updates the reply.
- `pwsh scripts/ci-local.ps1 -Full` -- passed (`ci-local: all checks passed`, `cargo deny`
  included: advisories ok, bans ok, licenses ok).

Gaps:

- **E2E not run here** (`e2e_proof 0`). The criterion and its steps are above; the owner runs
  `opentui-examples.exe` after merge. Nothing in this branch has been seen by a real program.
- **The GPUI test window pins `scale_factor` at 2.0**, so the DPI-scale change cannot be simulated
  directly. The integration test drives the same mechanism (the one `MetricsKey` cache) through a
  font-size change instead, which proves the push is keyed on the metrics rather than on the grid
  -- the property the scale case needs -- but not the scale change itself.
- **A flake, not a regression.** `cargo test --workspace` failed once on
  `handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks` ("the renderer
  waited 10 chunks, not one") while two other worktrees were compiling on the same machine. It is
  a lock-fairness timing test over `crates/terminal/src/handle.rs`, which this branch does not
  touch; it passed on its own and the whole gate passed on the re-run.
- The placement-scale and XTSMGRAPHICS follow-ups above are open by design, not overlooked.

## Handoff

Independent of the `IN-0039` and `IN-0040` packets in flight. Touches
`crates/terminal/src/{model,session,test_support}.rs` and
`crates/terminal-view/src/render/{element,state}.rs` plus `element_tests.rs`; a rebase conflict
with the concurrent `terminal-view` input packet is possible in `element.rs` and is mechanical.

### `harness.db` row

Not inserted from this branch (`harness.db` is edited only by the coordinator). Run this once
against the database at the repository root; `intake_id` is the `intake` row whose
`document_number` is 29.

```python
#!/usr/bin/env python3
"""Insert the BUG-0061 story row. Run once, from the repository root."""
import sqlite3

ROW = dict(
    id="BUG-0061",
    title="The adapter never tells the engine its cell size, so CSI 14 t answers zero",
    created_at="2026-09-16",
    risk_lane="normal",
    contract_doc="docs/terminal-backend.md",
    packet_doc=(
        "docs/spec-intakes/IN-0029-vt-engine/"
        "BUG-0061-adapter-never-sets-cell-pixels.md"
    ),
    status="planned",
    unit_proof=0,
    integration_proof=0,
    e2e_proof=0,
    platform_proof=0,
    evidence=None,
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=None,
    last_verified_result=None,
    notes=(
        "Normal lane. Engine untouched: set_cell_pixels and the CSI 14 t arm already "
        "exist and are tested in crates/vt; the adapter never called them, so OpenTUI "
        "and any CSI-14-t-gated renderer fell back to block mosaics. E2E is the owner's "
        "opentui-examples run after merge. Sixel placement still uses the VT340 virtual "
        "cell, so images land about 10% small -- follow-up in the packet."
    ),
    intake_id=<IN-0029 row id, coordinator fills>,
)

with sqlite3.connect("harness.db") as db:
    columns = ", ".join(ROW)
    placeholders = ", ".join("?" for _ in ROW)
    db.execute(f"INSERT INTO story ({columns}) VALUES ({placeholders})", tuple(ROW.values()))
print("inserted BUG-0061")
```
