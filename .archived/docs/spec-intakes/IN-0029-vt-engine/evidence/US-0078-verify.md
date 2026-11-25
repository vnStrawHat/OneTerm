# US-0078 (selection) — independent verification

Verifier: independent agent. Date: 2026-09-12.
Under test: `worktree-agent-a3a509ddbf185b930` @ `9ef7d9c` (merge of `feat/vt-engine` @ `02962f4`).
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a3a509ddbf185b930`,
built into its own `target/` (`CARGO_TARGET_DIR` set explicitly). `Get-PSDrive D` → **53.7 GB free**.
Nothing was fixed and nothing was committed. Probe sources:
`<scratchpad>/US-0078-verify_probes.rs` (they were run inside the crate as
`selection::verify_probes`, then removed; the worktree is back to `git status` clean).

**Verdict: merge after fixes.** One real correctness defect (F1, one line + one
test case), two documentation-accuracy findings on a high-risk lane (F2, F3).
The engine semantics themselves are the reference's, verified exhaustively.

---

## 1. Pass / fail table

| # | Item | Result |
| --- | --- | --- |
| 1 | Scope, commit trailer | **pass** (one note) |
| 2 | `pwsh scripts/ci-local.ps1` green, props at 20 000 | **pass** |
| 3 | Semantics vs reference and consumer, probe by probe | **pass** (F4 wording) |
| 4 | Rotation, reflow, the region-scroll gap | **pass** (F1 defect, F2/F3 declarations) |
| 5 | Render exposure | **pass** |
| 6 | 5 shape deviations, 4 corrections, US-0076 table | **pass** (F6) |
| 7 | Code quality, performance sanity | **pass** (F5) |
| 8 | Packet completeness, DB row `US-0078` | **pass** |

---

## 2. Scope and trailer — pass

```
$ git diff 02962f4...HEAD --stat
 crates/vt/src/lib.rs                               |   4 +
 crates/vt/src/render/render_tests.rs               |  34 +-
 crates/vt/src/render/state.rs                      |  19 +-
 crates/vt/src/selection/expand.rs                  | 219 ++++++
 crates/vt/src/selection/mod.rs                     | 427 +++++++++++
 crates/vt/src/selection/selection_props.rs         | 288 +++++++
 crates/vt/src/selection/selection_tests.rs         | 850 +++++++++++++++++++++
 crates/vt/src/selection/text.rs                    | 166 ++++
 .../IN-0029-vt-engine/US-0078-selection.md         | 416 ++++++++++
 9 files changed, 2421 insertions(+), 2 deletions(-)
```

`selection/**` (new), `lib.rs` (`pub mod` + 5 re-exports), `render/state.rs` and
`render/render_tests.rs` (additive only — see § 6), the packet. **No** `grid/`,
`terminal/`, `parser/`, `reflow/` edit. **No** LLD edit. As declared.

`9ef7d9c` trailer is exact:

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018tAaHVGPnSzrEHLVcg3sx1
```

**Note (not a blocker):** merge commit `815e03f` (`Merge commit '02962f4' into
worktree-agent-a3a509ddbf185b930`) carries no trailer. The WIP checkpoint
`6b09bd4` does.

## 3. Gate — pass

```
$ pwsh scripts/ci-local.ps1
...
ci-local: all checks passed.
[exited with code 0]
```

fmt · clippy `-D warnings` · `cargo test --workspace` · verify-dependency-graph
(21 packages) · check-doc-paths (123 paths / 10 docs) · test_check_english ·
check-english (724 files) · completion-catalog validate · third-party-notices
`--check`. All green.

Raw totals summed over the **56** `test result:` sections:
**1455 passed / 0 failed / 8 ignored**. Matches the packet's claim exactly.

```
$ VT_PROPTEST_CASES=20000 cargo test -p oneterm-vt selection::props
test result: ok. 4 passed; 0 failed; 0 ignored; ... finished in 3.24s
```

(`selection_props.rs:30` reads `VT_PROPTEST_CASES`, default 256, so 20 000 was
honoured.)

## 4. Semantics — probe by probe

14 verifier probes, all passing except where noted. The two strongest are
**exhaustive oracles**: `Selection::is_empty` + `range_simple` / `range_block`
transcribed literally out of `vendor/alacritty_terminal/src/selection.rs:190-380`
and compared against `Selection::to_range` over *every* (start row, start col,
start side, end row, end col, end side) on a 3×4 grid — 576 cases per kind.

| Probe | Result |
| --- | --- |
| `probe_simple_matches_the_reference_oracle_exhaustively` | **pass, 0/576 divergences.** Side rules, wrap-to-previous-row on `Left` at col 0, wrap-forward on `Right` at the last column, adjacency-empty, single-cell, reversed anchors — all byte-identical to the reference |
| `probe_block_matches_the_reference_oracle_exhaustively` | **24/576 divergences, all of them declared correction 1.** Every one is a same-column / opposite-side pair on different rows, where the reference emits an *inverted* rectangle (e.g. `(0,0,Right)→(1,0,Left)` → ref `start.col 1, end.col 0`) and two of them are also **out of bounds** (`(0,3,Right)→(1,3,Left)` → ref `start.col 4` on a 4-column grid). OneTerm returns `None`. The correction is real and correctly scoped: those drags cover zero columns |
| `probe_ref_single_cell_both_directions` | pass — L→R and R→L on one cell both give `start == end == pos` |
| `probe_ref_across_adjacent_lines_upward_final_cell_exclusive` | pass — reference's own case, `(0,1,R)→(1,1,R)` = `(0,2)..=(1,1)` |
| `probe_ref_selection_bigger_then_smaller` | pass — repeated `update` re-resolves from the anchors, not from a cached range |
| `probe_selection_ending_on_a_wide_chars_spacer` | pass — ending on the `WideSpacer` and ending on the `Wide` both yield `"ab中"`; starting on the spacer pulls the glyph in (`"中c"`); `contains_cell` on the spacer alone is **false** and on the `Wide` partner of a selected spacer is **true** — exactly `vendor/.../selection.rs:80-84`. See **F4** |
| `probe_block_reversed_corners_and_wide_edges` | pass — bottom-right→top-left with sides reversed normalises to `(0,1)..=(1,3)`, `is_block`, text `"中文\ny中"`; wide glyph at the left edge (spacer) and clipped at the right edge both kept once |
| `probe_semantic_word_continues_across_a_wrapped_boundary_with_escapes_and_wides` | pass — a word that runs over a `WRAPPED` row boundary and contains a wide glyph expands to `"中bcdefgh"`; an escape char on the previous row still stops it |
| `probe_lines_over_a_wrapped_line_takes_the_whole_logical_line` | pass — `Lines` from the middle row of a 3-row wrapped line gives the whole logical line + `"\n"` and does not leak into the unwrapped neighbours |
| `probe_selection_in_history_and_spanning_history_and_screen` | pass — wholly in history (`"row0\nrow1"`) and history→screen (`"row1\nrow2"`) both resolve; `screen_of(start.row)` picks the right screen run |
| `probe_text_line_endings_are_lf_only_and_trailing_blanks_trimmed` | pass — `"hi\nthere\nyou"`, **no `\r` anywhere**. Matches the reference `bounds_to_string` and matches what OneTerm's clipboard path does today: `crates/terminal-view/src/input/edit.rs:29-33` writes `selection_text()` to the clipboard verbatim, and `TermModel::selection_text` → `selection_to_string` is LF-only. No Windows CRLF regression; CRLF-on-Windows remains a product question nobody has opened |
| `probe_empty_and_none_cases` | pass — coincident anchors → `None` range **and** `None` text; a killed anchor → `None`, never a stale range |
| `probe_hit_test_sides_and_clamping` | pass — `.4`→`Left`, `.6`→`Right`, negatives clamp to `(top, 0)`, overshoot clamps to `(top+rows-1, cols-1)`; the fraction is taken from the *unclamped* column, same as `crates/terminal/src/model.rs:425-437` |

`line_length` (text.rs:158) was checked against the reference's
`LineLength::line_length` (`vendor/.../term/cell.rs:296-314`) including its
`WRAPLINE ⇒ full width` short-circuit: equivalent under deviation G1.
`RowRef::cell` reads out of range as blank (`grid/row.rs:302`), so no
caller-supplied `Pos` can panic these paths.

## 5. Rotation, reflow, and the region-scroll gap

| Probe | Result |
| --- | --- |
| Scroll into history, `IL`/`DL`, trim, viewport scroll | pass (packet's own tests, re-run green; the anchor mechanism carries all of them with no selection-specific code) |
| Reflow narrow→wide with the selection inside a wrapped line | pass. 10→6→10 columns: selection `None` afterwards, `Mark` anchors survive the same `Anchors::remap`. **Consistent with the LLD**: `selection.md` says a selection "survives a resize when the column count is unchanged, and is cleared when it changes — matching trap 28", `reflow-and-resize.md` records `kill_selection` as kind-scoped, and DEC-0015 clause 1 is why marks and selection ends can diverge. The packet's "None after reflow while marks survive" is exactly what the three documents ask for. No conflict |
| `probe_partial_region_scroll_splits_the_selection` | reproduced — see F2 |
| `probe_endpoint_scrolled_out_of_a_region_top` | divergence — see F3 |
| `probe_erase_above_over_clears_a_history_only_selection` | **FAIL** — see F1 |

## 6. Render exposure — pass

`EngineView::selection: Option<SelectionRange>` → `RenderState.selection` →
`pub fn selection(&self)`. Three checks:

* **A drag over static content reports a frame.** `state.rs:140` computes
  `selection_changed` and `state.rs:194` adds `&& !selection_changed` to the
  `Unchanged` arm; `render_tests.rs::selection_change_only_returns_partial_and_refreshes_the_range`
  asserts `Partial { scrolled: 0 }` with `changed()` empty. Re-run green.
  The Mode-2026 suppression arm at `state.rs:148` folds it into `meta_dirty`
  instead of losing it — correct, and easy to have missed.
* **No interned id crosses.** `SelectionRange` is `{ Pos, Pos, bool }` and `Pos`
  is `{ RowId(u64), u16 }`. No `GraphemeId` / `StyleId` / `ExtrasId` / interner
  reference on the render boundary.
* **Tri-state.** With neither content nor selection changed the second
  `update` returns `Unchanged` (asserted in the same test).

## 7. Shape deviations and corrections — judgment

| | Claim | Judgment |
| --- | --- | --- |
| D1 | No `Terminal`; module written against `TerminalGrid`, wrappers deferred to US-0076 | **Justified.** `crates/vt/src/terminal/` does not exist on this branch. `release(self)` consuming the value is the right way to make "release exactly once" a type-level fact |
| D2 | No `Config`; escape set is a `&str` parameter, default published as `SEMANTIC_ESCAPE_CHARS` | **Justified.** The constant is byte-for-byte the reference's `",│\`\|:\"' ()[]{}<>\t"`, matched by `char`, so the multi-byte `│` works |
| D3 | `WRAPPED` is a row flag, not a cell flag | **Justified** — deviation G1 is already shipped in `grid-and-scrollback.md`; verified equivalent including the reference's `line_length` short-circuit |
| D4 | The invalidation matrix is a predicate, not call sites | **Justified.** Keeps the packet out of `screen.rs` while a concurrent packet reworks it, and the row-moving half genuinely needs no code. But see **F1** and **F6** |
| D5 | `contains_cell` takes `Option<Pos>` instead of `CursorShape` | **Justified.** `CursorShape` is US-0076's. The quirk is reproduced |
| C1 | `range_block` never returns an inverted rectangle | **Verified as a genuine reference bug** by the exhaustive oracle — 24 cases, 2 of them also out of bounds |
| C2 | Same guard on `range_simple`, unreachable | **Verified unreachable** — 0/576 oracle divergences for `Simple`. Keeping it is cheap and makes the property hold by construction |
| C3 | The `LeadingWideSpacer` tail rule reads the row **below** | **Justified.** `vendor/.../term/mod.rs:641` reads `self.grid[line - 1i32][Column(0)]`; a leading spacer at the end of a row belongs to the glyph on the row below. Implementing the comment rather than the code is right |
| C4 | Block passes `include_wrapped_wide` only on the last row | **Justified.** The reference passes `start.column != 0` on the non-last rows (`term/mod.rs:554`) with no stated meaning, and `true` on the last (`:560`) — OneTerm matches it where it is defined |

**US-0076 wrapper mapping table: complete** — all 7 `impl Terminal` methods in
`selection.md` § Interfaces (`selection_start`, `selection_update`,
`selection_range`, `selection_text`, `selection_clear`, `select_all`,
`hit_test`) have a row. See **F6** for the one obligation it leaves in prose.

## 8. Code quality and performance — pass

* No `unsafe`, no `unwrap()`, no `expect(`, no `panic!`/`todo!`/`unimplemented!`
  anywhere in `selection/{mod,expand,text}.rs` (grep, zero hits). Consistent with
  `docs/agents/error-policy.md`: nothing here returns an error, everything
  degrades to `None` or a blank cell.
* Every `pub` item carries rustdoc; every deviation carries a comment naming the
  reference line it departs from. Module docs point at the LLD and DEC-0015.
  Meets `docs/agents/code-style.md`.
* Pub surface is 6 types/constants + 11 methods. Minimal except **F5**.
* **Performance.** 100 000-line scrollback, selection over 99 000 rows:
  `to_range` = **1 µs** (anchor lookup + two comparisons, no row walk);
  `text` = **97 ms** for **2 564 975 bytes** (~26 MB/s, one `String` grown in
  place). Linear in the covered rows, three orders of magnitude apart as the
  `O(1)` / `O(n)` split requires. PERF-14 holds.

## 9. Packet completeness and the DB row — pass

Packet has every required section, `Implemented` checked, unit proof + verify
command checked, integration/E2E/platform correctly left unchecked with a reason.
The Evidence table, the "what the tests pin" table and the Gaps list are unusually
honest — every one of the eight gaps is real and I could reproduce each.

`harness.db` row `US-0078` (main checkout, read-only):

```
id=US-0078  title=Selection  risk_lane=high_risk  intake_id=34  status=implemented
contract_doc=docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md
packet_doc=docs/spec-intakes/IN-0029-vt-engine/US-0078-selection.md
unit_proof=1 integration_proof=0 e2e_proof=0 platform_proof=0
verify_command=pwsh scripts/ci-local.ps1
last_verified_at=2026-09-13T12:00:00  last_verified_result=pass
evidence: "...56 test-result sections: 1455 passed / 0 failed / 8 ignored..."
```

Consistent with the packet and with my own measured CI totals.

---

## 10. Findings, ranked

### F1 — `ED 1` clears a selection that is wholly in history *(medium, correctness, fix in this packet)*

`crates/vt/src/selection/mod.rs:234`

```rust
Invalidation::EraseAbove => top <= cursor,
```

`ED 1` erases `screen_top..=cursor`. The predicate has no lower bound, so a
selection that lies entirely in the scrollback — which `ED 1` never touches —
reports as invalidated. The reference bounds the range at the screen top
(`vendor/alacritty_terminal/src/term/mod.rs:1791`, `let range = Line(0)..=cursor.line`),
and `selection.md` says "cleared when the range intersects **the cleared rows**".

```
$ cargo test -p oneterm-vt selection::verify_probes::probe_erase_above -- --nocapture
PROBE history-only selection: EraseAbove=true EraseBelow=false EraseLine=false (reference: false/false/false)
thread '...' panicked: ED 1 must not clear a selection that is wholly in history
```

Not user-reachable today (nothing calls `invalidated_by` until US-0076), which is
why this is not a reject. Fix:

```rust
Invalidation::EraseAbove => top <= cursor && bottom >= screen.screen_top(),
```

and add `(EraseAbove, history-only selection, false)` to `invalidation_matrix`,
which currently exercises `EraseAbove` only with both anchors on screen
(`selection_tests.rs`, cases at cursor rows 0 and 3).

`EraseBelow` has the mirror-image missing bound (`bottom >= cursor`, unbounded
above) but nothing lives below the screen bottom, so it is unreachable — same
status as correction C2, and worth the same one-line symmetry.

### F2 — the region-scroll gap misdescribes the reference *(medium, packet fix)*

`docs/spec-intakes/IN-0029-vt-engine/US-0078-selection.md`, § Gaps, first bullet:

> The reference deletes it (`Selection::rotate` returns `None` when the start
> rotates out of the region while the end has not).

Measured, it does not. `Selection::rotate`
(`vendor/alacritty_terminal/src/selection.rs:137-189`) moves an endpoint only
when `start.line >= range_top && start.line < range_bottom`, so an endpoint
outside the region is left exactly where OneTerm's `Anchors::shift_region` leaves
it. It returns `None` in only two sub-cases: the start rotates to
`>= range_bottom` while the end is still `< range_bottom` (a *scroll down*), or
the end overtakes the start.

```
$ cargo test ... probe_partial_region_scroll_splits_the_selection -- --nocapture
PROBE partial SU: range=Some(SelectionRange { start: Pos { row: RowId(1), col: 0 },
  end: Pos { row: RowId(4), col: 4 }, is_block: false })
  text=Some("r2ccc\n\nr3ddd\nr4eee")
```

Hand-running `rotate(dims, Line(1)..Line(4), 1)` on the same selection gives
lines 1..4 — the same split, the same interposed blank row. **OneTerm is at
parity here.** The gap as written reads like a regression against the reference
and it is not; it should say "neither engine keeps a partially-contained
selection coherent", and it belongs to the design owner as a *new* behaviour,
not to US-0075. Restate it in this packet.

### F3 — an endpoint scrolled out of a region top is killed, where the reference clamps *(medium, undeclared divergence, design owner)*

This is the divergence the region-scroll gap *should* have named.
`Anchors::shift_region` (`crates/vt/src/grid/anchor.rs:142`) kills any anchor
whose row falls in the scroll's `kill` range, so the whole selection resolves to
`None`. The reference clamps that endpoint to the region top and rewrites it to
`(range_top, Column(0), Side::Left)` for non-`Block` kinds
(`vendor/alacritty_terminal/src/selection.rs:160-166`), keeping the selection alive.
Its own test `rotate_in_region_up` (`selection.rs:605-616`) pins the clamped result.

```
$ cargo test ... probe_endpoint_scrolled_out_of_a_region_top -- --nocapture
PROBE region-top scroll-out: range=None text=None
  reference would give start=(line1,col0) end=(line3,col3)
```

I judge OneTerm's behaviour **defensible and arguably better** — the anchored
content was genuinely discarded by the region scroll, so clamping would leave the
selection pointing at text the user never selected, which is precisely what
`selection.md` says not to do ("cleared rather than pointing at unrelated text").
But it is a user-visible change from the reference that neither `selection.md`
nor the packet's five deviations declare. Declare it as a fifth correction (or a
sixth deviation) with the probe above as its test, and let the design owner
confirm kill-over-clamp.

### F4 — `contains_cell`'s wide-char rule is documented backwards *(low, wording)*

`crates/vt/src/selection/mod.rs:264-266` (doc comment), `selection.md`
§ `to_range` semantics, and the test name
`selection_tests.rs::contains_cell_extends_a_wide_char_to_its_spacer` all say a
`Wide` cell's membership *extends to* its `WideSpacer`. The implemented rule — and
the reference's, `vendor/.../selection.rs:82-84`, comment "Check if a wide char's
trailing spacer is selected" — is the inverse: a selected **spacer** pulls in its
`Wide` partner; selecting only the `Wide` cell leaves the spacer unselected.
Probe confirms both directions. **The code is right.** The test body is also right
(it selects the spacer and asserts the glyph paints) — only its name and the prose
are inverted. Reword the doc comment and rename the test; the LLD sentence is the
design owner's to correct.

### F5 — `Selection::kind()` is dead public surface *(low)*

`crates/vt/src/selection/mod.rs:167`. No caller in `crates/vt`, and no row in the
US-0076 wrapper table needs it. Drop it, or add it to the table if `Terminal` will
expose the kind.

### F6 — the US-0076 mapping table omits the `invalidated_by` obligation *(low, packet)*

The table maps all 7 `impl Terminal` methods, but `Selection::invalidated_by` — the
whole of deviation D4 — appears only as prose under Gaps and Handoff. Since the
table is what US-0076's owner will read, add a row:
`Terminal::{erase_line, erase_display, reset, swap_alt} dispatch` →
`Selection::invalidated_by(grid, Invalidation::…) evaluated **before** the operation`.

### F7 — `bracket_search` is unbounded *(info, parity)*

`crates/vt/src/selection/expand.rs:138-167` walks every cell to the oldest/newest
row on an unmatched bracket — 8 M cells on a 100 k × 80 scrollback. The reference's
`iter_from` does the same, so this is parity, not a regression, and a double-click
on a lone `(` is rare. Worth a row budget when `Terminal` lands.

### F8 — the tail wide glyph is dropped when it is a grapheme cluster *(info)*

`crates/vt/src/selection/text.rs:148` matches only `CellContent::Scalar` when
adopting the glyph a `LeadingWideSpacer` pushed onto the next row; a clustered
glyph (e.g. an emoji with a variation selector) is silently omitted. The reference
pushes `.c` unconditionally. One `match` arm; negligible.

### Also noted, no action

`clamp` (`mod.rs:315`) clamps row and column independently, where the reference's
`Point::grid_clamp(Boundary::Grid)` snaps an out-of-range point to `(topmost, 0)` /
`(bottommost, last_col)`. Unreachable: anchors are only ever set from live
positions, and `hit_test` already clamps. Recording it because it is the kind of
thing a later `Terminal::selection_update` from a stale viewport could reach.

---

## 11. Verdict

**Merge after fixes.**

Required before merge:
* **F1** — the `EraseAbove` bound, plus the matrix test case.
* **F2** — restate the region-scroll gap; the current text claims a regression
  against the reference that does not exist.
* **F3** — declare the kill-vs-clamp divergence as a correction with a test, and
  route the kill-over-clamp choice to the design owner.

Nice to have, not blocking: F4 (wording + test name), F5 (`kind()`), F6 (one
table row), F8 (one match arm).

The engine semantics are sound. `Simple` is byte-identical to the reference over
an exhaustive sweep; `Block` diverges only where the reference is provably wrong;
extraction, semantic/line expansion, the anchor-based rotation and the render hook
all hold up. F1 is the only behavioural defect and it is one line.
