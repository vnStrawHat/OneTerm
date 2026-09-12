# Independent verification: US-0077 (reflow and resize policies)

Branch `worktree-agent-aeba210aa0ace1fb9` @ `efaa186`, base `83933b9`.
Specs read from the main checkout at `feat/vt-engine` @ `1ef1414`.
Verifier: independent agent, 2026-09-12. **Not committed. No implementation file changed.**

Disk check before work: `Get-PSDrive D` → 55.9 GB free (threshold 15 GB) — proceeded.
All builds went into the worktree's own `target/` (`CARGO_TARGET_DIR` unset, worktree `target/` = 7.0 GB).

---

## Verdict

**Merge after fixes.** Two minor code fixes and three documentation actions; nothing found that
changes the algorithm or the shipped `keep_viewport_top` behaviour. The packet's central claim —
that all thirteen `keep_viewport_top_*` scenarios plus both `default_*` companions are reproduced
with the same inputs and the same expected grids — **holds under line-by-line comparison**, and
`measure_rows` genuinely replaces the scratch-grid `conhost_cursor_row` probe. Fifteen independent
probe tests written by this verifier all pass on the first corrected run; a differential run
against the old engine on a new scenario found the new engine **strictly better** (the old one
loses content) — but that correction is undeclared.

---

## 1. Scope

```
$ git diff 83933b9...HEAD --stat
 crates/vt/src/grid/anchor.rs                       |  16 +
 crates/vt/src/grid/grid_tests.rs                   |  35 +-
 crates/vt/src/grid/row.rs                          |  30 +
 crates/vt/src/grid/screen.rs                       | 262 ++++--
 crates/vt/src/grid/terminal_grid.rs                |  19 +-
 crates/vt/src/lib.rs                               |   2 +
 crates/vt/src/reflow/columns.rs                    | 460 ++++++++++
 crates/vt/src/reflow/mod.rs                        | 186 +++++
 crates/vt/src/reflow/reflow_props.rs               | 448 ++++++++++
 crates/vt/src/reflow/reflow_tests.rs               | 930 +++++++++++++++++++++
 .../IN-0029-vt-engine/US-0077-reflow-and-resize.md | 422 ++++++++++
 11 files changed, 2746 insertions(+), 64 deletions(-)
```

| Check | Result |
| --- | --- |
| `reflow/**` is new, self-contained | **PASS** — 4 files, 2 of them tests (1378 of 2024 reflow lines are tests). |
| Grid edits minimal | **PASS**, see list below. |
| `lib.rs` | **PASS** — `pub mod reflow;` + `pub use reflow::{ResizeOutcome, ResizePolicy};`, nothing else. |
| Packet present | **PASS**. |
| No LLD / intake doc edited | **PASS** — the only `docs/` file in the diff is the packet itself. |
| No parser / render file touched | **PASS** — `crates/vt/src/parser/` and `crates/vt/src/render/` absent from the diff. |
| No new dependency | **PASS** — `git diff 83933b9...HEAD -- crates/vt/Cargo.toml Cargo.toml Cargo.lock` is **empty**. |

Grid edits, each justified and each read in full:

* `anchor.rs` — `Anchors::kill_selection` only (+16).
* `row.rs` — `Row::from_cells` (+25) and one `repair_wide_pairs` arm releasing an inland
  `LeadingWideSpacer` (+5). The second is a genuine bug fix found by the property suite.
* `terminal_grid.rs` — `resize(size, policy) -> ResizeOutcome` delegating to `reflow::resize`.
* `grid_tests.rs` — **mechanical only**: six call sites gained the policy argument. Diffed by eye:
  no assertion text or expected value changed, and `tab_stops_survive_and_regrow_across_a_resize`
  (trap 27) still runs, now through the new `install_rows` / `truncate_columns` path.
* `screen.rs` (+214 −48) — `Screen::resize` replaced by `resize_rows`, `truncate_columns`,
  `install_rows`, `append_blank_rows`, `drop_trailing_rows`, `trim_history`, `restate_viewport`,
  `set_scroll_offset`, `set_cursor`, `set_saved_cursor_pos`, plus `push_rows_with` and
  `take_row` made `pub(crate)`. Nine new `pub(crate)` methods on `Screen` is the largest single
  thing this packet does to an existing file; each one exists because it touches the ring, which
  the packet correctly refuses to let `reflow/` reach into. **Accepted** — see finding **m3**.

---

## 2. Gates

### `pwsh scripts/ci-local.ps1` — **PASS** (exit 0)

```
==> python scripts/verify-dependency-graph.py     (21 packages)
==> python scripts/check-doc-paths.py             Doc path check passed for 122 current paths in 10 documents.
==> python -m unittest scripts/test_check_english.py    Ran 2 tests ... OK
==> python scripts/check-english.py               English contributor-text check passed for 687 files.
==> python scripts/completion-catalog.py validate [completion-catalog] all catalogs valid
==> python scripts/third-party-notices.py --check  THIRD-PARTY-NOTICES.md is up to date.
ci-local: all checks passed.
```

Raw totals summed over the 54 `test result: ok.` sections in the log:
**1315 passed / 0 failed / 6 ignored** — exactly the figure the packet and the `US-0077` DB row
claim. `fmt --check` and `clippy --workspace --all-targets -- -D warnings` are inside the script
and both passed.

### `cargo test -p oneterm-vt` budget (R-28) — **PASS**

Default (256 proptest cases): `reflow::props` alone is `finished in 0.12s`; the whole crate suite
is well inside the 60 s debug budget.

### Proptests at raised case counts, fresh seeds — **PASS**

`proptest` seeds its RNG from OS entropy on each run (no regression file is checked in), so each
invocation below is an independent seed.

```
VT_PROPTEST_CASES unset   test result: ok. 10 passed; 0 failed; ... finished in 0.12s
VT_PROPTEST_CASES=20000   test result: ok. 10 passed; 0 failed; ... finished in 4.61s   (run 1)
VT_PROPTEST_CASES=20000   test result: ok. 10 passed; 0 failed; ... finished in 5.74s   (run 2)
VT_PROPTEST_CASES=40000   test result: ok. 10 passed; 0 failed; ... finished in 9.79s
```

The three timings scale linearly with the case count, which is the evidence that the env var is
actually reaching `ProptestConfig::with_cases` rather than being silently ignored. **3 × 20 000
and 1 × 40 000 cases, three distinct seeds, zero failures.**

---

## 3. `keep_viewport_top` parity and the conhost probe

### 3.1 Scenario-by-scenario fixture comparison — **PASS**

`crates/terminal/src/model.rs:617-910` against `crates/vt/src/reflow/reflow_tests.rs:402-700`,
read side by side. Thirteen `keep_viewport_top_*` plus both `default_*` companions, **fifteen**
scenarios, all present. Inputs (`fed_term` → `fed`, `term_with` → `term_with`, same line counts,
same strings, same prompts, same resize targets) and every expected value match. Four renames,
all documented in the packet.

No relaxed expectation was found. Where the port differs it is **stronger**, not weaker:

* `..._leaves_shrink_and_history_less_grow_to_bottom_anchor` and
  `..._narrow_with_the_cursor_at_the_bottom_matches_bottom_anchor` compare `all_rows()`
  (oldest → newest) where `model.rs` compared `row_text` over `-(history)..rows` — a superset.
* `..._corrects_the_primary_screen_while_alt_is_active` asserts the primary screen **directly**
  (via `primary_row_text`) *and then* after `swap_alt`, where `model.rs` could only observe it
  after leaving the alt screen.
* `..._restores_the_scroll_offset` uses `scroll_viewport(-3)` in place of
  `scroll_display(Scroll::Delta(3))` — same offset, same assertions.

The reference's own five cases (`shrink_reflow`, `shrink_reflow_twice`,
`shrink_reflow_empty_cell_inside_line`, `grow_reflow`, `grow_reflow_multiline`) were compared
against `vendor/alacritty_terminal/src/grid/tests.rs:164-290` and are faithful, including the
1-column expansion `["1", "", "3", "4"]`.

### 3.2 `conhost_cursor_row` → `measure_rows` — **PASS**

The scratch-grid probe (`model.rs:528-541`: a `Grid::new(rows, old_cols, rows*old_cols)` clone
with `pad = old_cols / cols + 1` blank rows, reflowed, then `history_size + line - pad`) is
**deleted, not ported**. `measure_rows` (`columns.rs:215-260`) walks the screen rows from
`screen_top()` down to the cursor row through the *same* `Line`/`emit_line` code, with a
`CountSink` that allocates no cells, and returns the produced-row index the cursor landed on.

Agreement is established structurally rather than by a side-by-side numeric run: the
`KeepViewportTop` result is `shift = cursor_row_index − conhost_row` applied on top of a
`BottomAnchor` resize, and the two `matches_bottom_anchor` tests pin the `BottomAnchor` half
independently. All fifteen scenarios landing on the recorded final cursor row therefore forces
`measure_rows` to return the same number the old probe did on every one of them. This is strong
indirect evidence, not a direct comparison — recorded as such.

The BUG-0051 fixtures are checked: `measure_rows_matches_the_recorded_conhost_rows` asserts
**11, 13, 37**, which is what `ESC[12;18H` / `ESC[14;18H` / `ESC[38;18H` mean 0-based. The LLD's
Verification bullet says "row 12 … row 14 … row 38" — the LLD is reading those 1-based. See
finding **d2**.

`measure_rows_never_looks_above_the_screen` correctly pins the measured conhost rule that the top
row starts a logical line even when it continues one from history.

### 3.3 The verifier's own scenario, new engine vs old engine — **DIVERGENCE FOUND (new is correct)**

Scenario (`5×10`, scrollback 100): six `lineN` lines, then the 36-character line
`abcdefghijklmnopqrstuvwxyz0123456789` — which wraps over four rows **spanning the history
boundary** (rows −? … 4, its head sitting at screen index 1 with five rows already in history) —
then the **cursor parked mid-line** at screen row 2, column 3. Narrow 10 → 6, then widen 6 → 14,
`BottomAnchor` on both sides so only the reflow is compared. Old engine driven through
`alacritty_terminal` directly (a scratch test under `crates/tools/tests/`), new engine through a
scratch probe inside `crates/vt`.

**Narrow 10 → 6 — identical**, cell for cell, including wrap flags, cursor and history:

```
OLD/NEW after narrow to 6: 5x6 history=7 cursor=(1,1) wrap=false
  [ -1] w=1 |abcdef|     [  1] w=1 |mnopqr|     [  3] w=1 |yz0123|
  [  0] w=1 |ghijkl|     [  2] w=1 |stuvwx|     [  4] w=0 |456789|
```

**Widen 6 → 14 — they differ:**

```
OLD after widen to 14: 5x14 history=3 cursor=(3,13) wrap=false
  [  2] w=0 |line5|
  [  3] w=1 |abcdefghijklmn|
  [  4] w=1 |opqrstuvwxyz01|      <-- dangling WRAPLINE on the bottom row
                                  <-- "23456789" IS GONE

NEW after widen to 14: 5x14 history=4 cursor=(2,13) wrap=false
  [  1] w=0 |line5|
  [  2] w=1 |abcdefghijklmn|
  [  3] w=1 |opqrstuvwxyz01|
  [  4] w=0 |23456789|            <-- preserved, and no dangling wrap
```

Control run (same scenario, cursor left where the text put it): the two engines are **identical**
(`history=4`, same eight rows, tail present). So the divergence is caused purely by the cursor
sitting above the tail of a wrapped line, which is the old engine's `grow_columns` driving row
placement off `cursor_line_delta`. **The new engine is right and the old engine loses content.**

This state is trivially reachable in the product (any `CUP` / arrow-key move inside a wrapped
command line), so it is a real correction, not a curiosity. It is **not declared anywhere** —
see finding **M1**.

---

## 4. Reflow correctness probes

Fifteen probes written by this verifier (`reflow::verify_probes::p1 … p13`; sources kept at
`…/scratchpad/verify_probes.rs` and `…/scratchpad/verify_us0077_old_engine.rs`, removed from the
worktree). Four failed on the first run; all four were **the verifier's own arithmetic**
(a trailing blank logical line in my helper, a 1-column step in a tour I wrote that is legitimately
destructive on the alt screen, and two miscounted fixtures). After correction:

```
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 138 filtered out; finished in 0.05s
```

| # | Probe | Result |
| --- | --- | --- |
| p1 | Narrow/widen tour `20→3→7→2→40→5→20` preserves every logical line, incl. `Ａ`/`Ｂ`/`Ｃ` wide glyphs, a `👨‍👩‍👦` ZWJ cluster and a combining `é` | **PASS** |
| p1b | Trailing spaces are dropped per logical line | **PASS** |
| p1c | A **coloured** trailing blank survives; a **BOLD** trailing blank also survives | **PASS** — see finding **d1** |
| p2 | Split point between the halves of a wide pair: `LeadingWideSpacer` in the last column, `Wide`+`WideSpacer` on the next row, and no stranded spacer after widening back | **PASS** |
| p3 | Cursor on a **wrapped** row at the last column with pending wrap, across two widens: stays armed at `col 9`, disarms at `col 10`, and the next printed glyph lands correctly | **PASS** |
| p4 | `DECSC` saved cursor carried by its character through `10→4→20` | **PASS** |
| p5 | Two `Mark` anchors with both endpoints on wrapped rows of one logical line ride their characters; both selection anchors die (trap 28) | **PASS** |
| p6 | `Mark` on a row that **merges into another** lands mid-row on its own character (`col 5`) | **PASS** |
| p7 | Alt screen never rejoins on a widen and reports `history_len() == 0` at every size in a `2×4 / 9×30 / 1×1 / 4×10` tour, integrity green each step | **PASS** — the proptest-found defect is genuinely fixed |
| p8 | Identity resize keeps `DECSTBM` (`{top:1,bottom:4}` survives); any other resize resets it to full | **PASS** |
| p9 | `cols == 1` degrades wide pairs to blanks, `rows == 1`, and `1×1 → 1×80`, integrity green | **PASS** |
| p10 | Resize with `display_offset = 7` across `5×6 / 9×30 / 5×10`: the first visible character is still first every time | **PASS** |
| p10b | `KeepViewportTop` restores the numeric offset exactly (`7` before, `7` after an `8×14` resize) | **PASS** |
| p11 | Trim order: with `cap = scrollback_limit + rows = 6`, `rows_trimmed = 5`, survivors are `["2222","3333","3333","4444","4444",""]` — the **oldest** went first, and `"00000000"` is gone | **PASS** |
| p12 | **1 000 random resizes** (`1..=40` rows × `1..=60` cols, policy alternating, xorshift-driven) with `assert_integrity` after every one | **PASS** |
| p13 | Differential dump vs the old engine (§ 3.3) | **DIVERGENCE, new engine correct** |

`ResizeOutcome` is exercised by p8 (`ResizeOutcome::default()` on an identity resize) and p11
(`reflowed == true`, `rows_trimmed == 5`). The `Full` render signal is **correctly not this
packet's** — `damage-and-render-state.md` has `RenderState` derive it from the size it last
observed, and the packet does not fake it.

---

## 5. Timing and profile

Reproduced, release, median of 20, `80×24 ↔ 100×40`:

```
scrollback      0: grow     1.5 us   shrink     2.0 us   rows-only round trip 0.200 us   (live rows 24)
scrollback  10000: grow  3774.7 us   shrink  3516.5 us   rows-only round trip 0.200 us   (live rows 10024)
scrollback 100000: grow 29070.1 us   shrink 29795.5 us   rows-only round trip 0.100 us   (live rows 100024)
```

The packet's table (1.6 / 2.1, 4395 / 4874, 32947 / 33701) is **honest** — this machine runs
slightly faster, same order everywhere. The rows-only figure confirms the packet's claim that the
whole cost is the column half.

**Where the 29–33 ms goes.** Measured directly (`p14_cost_breakdown`, release, per 100 000 rows of
96 cells):

```
alloc+fill 23 118 us | +Row::from_cells flag scan 12 271 us | one extend_from_slice pass 15 332 us | drop 17 966 us
```

The 100 000-row case reflows ~100 k source rows into ~50 k produced rows, so the reflow's budget is
roughly: **one fresh `Vec<Cell>` allocation + fill per produced row (~11.5 ms) and the drop of
100 k old rows (~18 ms) — together over half the total; the `flags_for` rescan inside
`Row::from_cells` (~6 ms); and two cell-copy passes (`Line::push_row`'s `extend_from_slice`, then
`emit_line` pushing cell by cell).** It is allocator traffic first, per-cell rescanning second.

**Is "lazy history reflow" the right follow-up?** Yes for drag-resize latency — it is the only
change that alters the *shape* (viewport-sized work per frame instead of history-sized), and the
measurement above is a good argument for it. But the packet's line "the algorithm is
O(live rows × cols) and **no amount of care changes that**" understates what is left on the table.
Three cheap constant-factor wins, none implemented, **proposed not applied**:

1. **Recycle row allocations.** `take_row` hands back `Option<Row>` whose `Vec<Cell>` is dropped;
   `emit_line` immediately `Vec::with_capacity`s a new one. A small free-list would remove most of
   the ~29 ms of alloc + drop measured above. Largest single win, contained inside `columns.rs`.
2. **Skip untouched, unwrapped rows.** The reference already does this: a row is a reflow target
   only when it is short and carries `WRAPLINE`; everything else is merely `grow`n in place. A
   typical 100 000-row scrollback is mostly short unwrapped lines, all of which this
   implementation still tears down and rebuilds cell by cell. **This is the biggest missed win
   for real content** and it is also the reference's behaviour, so it is not a novel risk.
3. **Fold the flag computation into `emit_line`.** It already visits every cell and knows the
   widths; `Row::from_cells` then walks all of them again (~12 ms per 100 k rows).

Recommend recording 1–3 in the LLD's cost section alongside the deferred lazy reflow, so the
"nothing can be done eagerly" reading does not harden into a design fact.

---

## 6. Deviation judgments (the eight readings)

| # | Reading | Judgment |
| --- | --- | --- |
| 1 | `AnchorKind` screen discriminant (M6) **not added**; lane check instead | **Justified → LLD wording.** The LLD itself offers "or an equivalent lane check". Verified equivalent: `ScreenKind::origin()` gives `PRIMARY_ORIGIN` / `ALT_ORIGIN`, `Screen::row_range()` is exactly `oldest..newest+1`, and `reflow_columns` collects points from that same range and walks that same range — so lane membership *is* screen membership, and an out-of-lane position is returned unchanged. Probes p5/p6 confirm marks survive with their character and p7 confirms the alt lane is untouched by a primary reflow. **One residual, worth a design-owner line:** `Anchors::kill_selection` matches on *kind*, not lane, so it kills a selection registered on the alt lane too. Harmless today (there is one selection, and a resize does invalidate an alt-screen selection anyway) but it is the single place where the missing discriminant is observable. |
| 2 | A resize `sync_anchors` **before** reading the list | **Justified → LLD sentence.** Verified: `sync_anchors` writes only the three screen-owned entries, so it cannot clobber marks, graphics or selections; and `Screen::scroll_viewport` / `scroll_to_bottom` really do take no `&mut Anchors`, so the `ViewportTop` entry really can be stale. The ordering in `reflow::resize` is correct — sync (74-75) → measure (80) → reflow (89) → `read_back` (91) → `resize_rows`, whose own `sync_anchors` runs *after* the fields were restated. The LLD states the rule but not who performs the handover; it should gain that sentence. Design owner. |
| 3 | `rows_trimmed` counts both trims | **Justified → LLD sentence.** Reporting only the reflow cap would make the number mean less than its name. p11 confirms the count. |
| 4 | Trim uses `Cell::is_blank`, not `is_erasable` | **Justified, but the packet records only half of it.** Checked against the reference: `alacritty::Cell::is_empty` accepts `' '` **or `'\t'`**, and requires default `fg`/`bg` and no `INVERSE`/underline/`STRIKEOUT`. So (a) a **styled blank is kept by both** engines — the packet's worry is unfounded and p1c proves the coloured case; (b) tabs diverge as recorded; (c) **a third, unrecorded divergence:** the reference's `is_empty` ignores `BOLD`/`DIM`/`ITALIC`/`HIDDEN` (trap 37) while `is_blank` requires `style_id() == StyleId::DEFAULT`, so a trailing **bold space** is trimmed by the reference and kept here (p1c asserts this). All three differences are in the "keep more" direction, which is the safe one. Packet's Reading 4 should name the bold case too. |
| 5 | Resize fills with `Cell::EMPTY`, never the erase cell | **Justified — this is exactly the reference.** `vendor/alacritty_terminal/src/grid/resize.rs:14-36` does `let template = mem::take(&mut self.cursor.template);` for the duration of the resize. Keeping `push_rows` (erase cell) on the scroll paths and `push_rows_with(Cell::EMPTY)` on the resize paths reproduces it faithfully. No action. |
| 6 | A wide glyph cannot be reflowed into a 1-column screen; degrades to two blanks | **Justified → LLD sentence.** `emit_line`'s `cols >= 2` guards are what make the split loop provably terminate, which is precisely the Windows Terminal hang the research names. p9 confirms no `Wide`/`WideSpacer` survives at `cols == 1`. |
| 7 | Identity resize keeps `DECSTBM` | **Justified — this is also the reference.** `Term::resize` early-returns when both dimensions are unchanged, *before* `scroll_region = Line(0)..Line(screen_lines)`. p8 confirms the new engine behaves the same. Trap 28's unconditional wording and the LLD's step order want reconciling in one sentence, as the packet asks. |
| 8 | `measure_rows` is `u16` and clamps `min(rows − 1)` at the boundary | **Justified.** Same as the deleted call site. No action. |

---

## 7. Attribution

* `crates/vt/src/reflow/columns.rs:8-13` credits `avt` by name, licence (Apache-2.0) and URL, and
  states explicitly that **no `avt` source is copied** — the iterator was written from the LLD's
  six-step description against OneTerm's own cell/row/anchor types. Reading the code, that claim is
  credible: the structure (`RowSink` trait, `Line`, `Point`, `RingSink`/`CountSink`) is OneTerm's.
* `docs/license-analysis.md` § 3 requires a `NOTICE` / `THIRD-PARTY-NOTICES.md` entry for **reused
  source**, and says plainly that ideas are not copyrightable. With no source copied, the file
  header is arguably sufficient; the LLD nevertheless asks for the two lines.
* The gap **is** recorded in the packet and in the `US-0077` DB row, with the real reason
  (`THIRD-PARTY-NOTICES.md` is generated and CI-checked against `Cargo.lock`, so a hand-written
  section needs the generator's owner). **But it is assigned to "the owner or a follow-up packet"
  with no ID.** See finding **m1**. `python scripts/third-party-notices.py --check` passes today.

---

## 8. Code quality

| Check | Result |
| --- | --- |
| `unsafe` | **PASS** — zero in `crates/vt`; the only hit is the words "`GlobalAlloc` is an `unsafe` trait and this crate has none" in a doc comment. |
| `unwrap` / `expect` outside tests | **PASS** — zero in `reflow/mod.rs`, `reflow/columns.rs`, `screen.rs`, `row.rs`, `anchor.rs`, `terminal_grid.rs`. |
| Lossy casts | **PASS** — every `as u16` / `as u32` is bounded (`min(u16::MAX as usize)`, `MAX_ROWS`-bounded shifts, `dropped as u32`) or clamped at the API boundary. |
| Public surface minimal | **PASS** — `ResizePolicy` and `ResizeOutcome` only; `reflow::resize`, `reflow_columns`, `measure_rows` are `pub(crate)` as the LLD's Interfaces section requires. |
| Docs | **PASS** — module-level docs on both new source files with a design pointer; every new `Screen` entry point carries a doc comment naming the trap or rule it serves. |
| Backward, in-place shrink (no temp full copy) | **FAIL against the recommendation, not against the LLD.** See finding **m2**. |

**m2 detail.** The instruction to verify this comes from `research/prior-art.md` § 9.3 —
*"shrink **backwards, in place** (no temp buffer)"*, learned from xterm.js's scar tissue. The LLD's
own step list does **not** restate it, and the implementation does the opposite: `RingSink` builds
the whole output into a `VecDeque<Option<Row>>` capped at `scrollback_limit + rows`, which
`install_rows` then writes into the ring, so the old rows and the new ones are alive simultaneously
(`columns.rs:44-78, 130-165`). The packet **does** record the consequence honestly under Gaps
("peak memory during a reflow is roughly double the live cells … no test measures it"), and the
reference has the same shape (`take_all` + a `new_raw` vector). Judgment: **acceptable as built**
— an in-place backward shrink cannot be done against a ring whose slot for a row may be the
destination of another, and the cap is what keeps the write inside the ring. But the Gap should
cite `prior-art.md` § 9.3 by name so the deviation from a named recommendation is visible, and the
~2× peak should be stated as a number for a 1 000 000-row scrollback.

---

## 9. Packet, DB row, trailer

* **Packet — PASS.** Every template section present and filled. The Documentation Action
  ("no contract change") is defensible: the packet was forbidden to edit the intake docs and
  routed all eight readings to the design owner instead of back-writing them. Acceptance boxes
  are all genuinely satisfied by evidence reproduced above. The Gaps section is unusually candid
  (synthetic `measure_rows` fixtures, missing `OpenConsole.exe` version for R-39, no lazy reflow,
  2× peak memory, no integration/E2E proof, silent `drop_trailing_rows` clamp) and every one of
  those gaps was confirmed real by this verifier.
* **DB row `US-0077` — PASS.** Present in `harness.db` (`story` table): `status=implemented`,
  `risk_lane=high_risk`, `unit_proof=1`, `verify_command='pwsh scripts/ci-local.ps1'`,
  `last_verified_result='pass'`, `contract_doc` and `packet_doc` both correct, `intake_id=34`.
  The `evidence` and `notes` columns carry the full numbers and all eight readings, and every
  figure in them matches what was reproduced here.
* **Trailer on `efaa186` — PASS.** Conventional-commit subject with the packet id, a body that
  explains *why*, `Refs:` pointing at the packet, and both required trailer lines
  (`Co-Authored-By:` + `Claude-Session:`) in the exact required shape.

---

## 10. Findings

No blockers.

### Major

**M1 — The correction over the old engine is undeclared.**
`crates/vt/src/reflow/columns.rs:343-410` (`emit_line`) — where the old engine's `grow_columns`
loses the tail of a wrapped logical line when the cursor sits above that tail (§ 3.3: `"23456789"`
vanishes and a `WRAPLINE` dangles on the bottom row), the new engine preserves it. This is a
**behaviour difference from the engine being replaced, in a reachable state**, and it is recorded
nowhere: not in the packet's readings, not in its Gaps, not as a declared difference for the
`US-0076` parity gate — which will compare the two engines and, for any recording that moves the
cursor inside a wrapped line before a resize, will see a real diff it has no entry for.
*Proposed fix (packet, no code change):* add a ninth reading — "a grow no longer loses the tail of
a wrapped line below the cursor; the reference does (`grow_columns` drives placement off
`cursor_line_delta`)" — with the two dumps from § 3.3, and flag it to `US-0076`'s owner for
`expected-diffs.json`. Design owner: the intake owner, with `US-0076`.

### Minor

**m1 — The attribution gap is recorded but not assigned to anything actionable.**
Packet § Gaps, "Attribution beyond the file header" — "Left for the owner or a follow-up packet"
names neither an owner nor an ID, so nothing will pick it up.
*Proposed fix:* name the packet (`US-0087` retires `alacritty_terminal`; the notices generator is
touched there) or open a maintenance packet id, and record it in the backlog table.

**m2 — The `prior-art.md` § 9.3 in-place-shrink recommendation is deviated from without citing it.**
`crates/vt/src/reflow/columns.rs:44-78` — see § 8.
*Proposed fix:* one sentence in the packet's existing peak-memory Gap citing § 9.3 and stating the
2× figure for a 1 000 000-row scrollback; no code change.

**m3 — `Screen::set_cursor` / `set_saved_cursor_pos` are unguarded back doors.**
`crates/vt/src/grid/screen.rs:1451-1460` — both write the field and clamp, but neither writes the
anchor entry, so a future caller that uses them *outside* the reflow's read-back window will leave
the list stale in exactly the way reading 2 was created to fix.
*Proposed fix:* rename to `restate_cursor_after_reflow` / `restate_saved_cursor_after_reflow`, or
add a `#[doc]` line forbidding use outside `reflow::read_back`. Two-line change.

**m4 — The anchor remap is O(anchors²).**
`crates/vt/src/reflow/columns.rs:181-196` — `anchors.remap` runs a linear `table.iter().find()`
per anchor. `table` is built from `points`, which is already sorted by `(row, col)`
(`columns.rs:457`), so it is sorted. With the `Mark(u32)` anchors a long shell session accumulates
(one or more per prompt) this becomes visible: ~1 ms per resize at 1 000 marks, ~100 ms at 10 000.
*Proposed fix:* `table.binary_search_by(|(key, _)| key.cmp(&pos))` — a two-line change with the
sortedness already guaranteed.

**d1 — Reading 4 omits the `BOLD`-blank divergence.** See § 6 row 4. Packet text only.

**d2 — The LLD's `measure_rows` verification bullet is off by one.**
`low-level-design/reflow-and-resize.md`, Verification → "the three measured BUG-0051 cases
(… giving row 12; … row 14; … row 38)". BUG-0051 § Measurements (b) records `ESC[12;18H` =
**row 11, 0-based**, and the implementation asserts 11 / 13 / 37 correctly. The LLD's bullet reads
the CUP parameters 1-based and will make the next verifier think the test is wrong.
*Proposed fix:* LLD edit to 11 / 13 / 37 (0-based). Design owner.

**d3 — `Anchors::kill_selection` is kind-scoped where everything else is lane-scoped.**
`crates/vt/src/grid/anchor.rs:171-179`. See § 6 row 1. Conservative today; record it as the one
observable consequence of not adding the M6 discriminant, so the choice can be revisited if a
second selection is ever tracked.

---

## Reproduction

```powershell
# gates
pwsh scripts/ci-local.ps1
$env:VT_PROPTEST_CASES="20000"; cargo test -p oneterm-vt reflow::props
cargo test -p oneterm-vt --release resize_latency -- --ignored --nocapture

# the verifier's probes (sources in the session scratchpad; both files were removed
# from the worktree after use, the worktree is clean at efaa186)
#   scratchpad/verify_probes.rs            -> crates/vt/src/reflow/, hooked from reflow/mod.rs
#   scratchpad/verify_us0077_old_engine.rs -> crates/tools/tests/
cargo test -p oneterm-vt verify_probes -- --nocapture
cargo test -p oneterm-vt --release p14_cost -- --ignored --nocapture
cargo test -p oneterm-tools --test verify_us0077_old_engine -- --nocapture
```
