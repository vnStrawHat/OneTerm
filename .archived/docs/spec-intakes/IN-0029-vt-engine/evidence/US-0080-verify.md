# Independent verification: US-0080 (Graphics in the engine, Sixel)

Verifier: independent agent. Branch `worktree-agent-a96e68fbfe615d078` @ `ac628dc`,
base `feat/vt-engine` @ `458aa78`. Worktree
`D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a96e68fbfe615d078`
(own `target/`, 7.8 GB; `D:` had 34.97 GB free at start). Nothing was fixed and
nothing was committed. The scratch harnesses used below
(`crates/tools/tests/zz_verify.rs`, `zz_probes.rs`, `zz_heap.rs`) were deleted
after the run; every number here is raw tool output.

**Verdict: merge.** No blocking defect found. Four documentation-accuracy
findings and one confirmed behavioural divergence from the engine being
replaced, all low severity and all on paths no real Sixel producer takes.

---

## 1. Scope and commit hygiene — PASS

`git diff 458aa78...HEAD --stat` (22 files, +2096 / -15):

```
crates/tools/src/bin/vt-corpus.rs                  |  14 +-
crates/tools/src/corpus.rs                         |  10 +
crates/tools/tests/corpus_check.rs                 |  34 +-
crates/vt/src/graphics/graphics_tests.rs           | 662 +++++++++++++++++++++
crates/vt/src/graphics/mod.rs                      | 145 +++++
crates/vt/src/graphics/placement.rs                | 226 +++++++
crates/vt/src/graphics/sixel.rs                    | 330 ++++++++++
crates/vt/src/lib.rs                               |   6 +-
crates/vt/src/render/mod.rs                        |   2 +-
crates/vt/src/render/render_tests.rs               |   3 +
crates/vt/src/render/state.rs                      |  76 ++-
crates/vt/src/terminal/dispatch.rs                 |  35 +-
crates/vt/src/terminal/mod.rs                      |  36 ++
crates/vt/tests/corpus/NOTICE                      |  10 +
crates/vt/tests/corpus/oneterm/sixel_basic/*       | 130 +
docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md         |  12 +
docs/spec-intakes/IN-0028-sixel-graphics/low-level-design/vendor-graphics.md |   9 +
docs/spec-intakes/IN-0029-vt-engine/US-0080-graphics.md              | 371 +
```

* `crates/vt/src/grid/` — **not touched**. Confirmed.
* `crates/terminal/` — **not touched**, so `US-0081` is unblocked. Confirmed.
* No IN-0029 LLD edited. Confirmed.
* Corpus additions are all under `crates/vt/tests/corpus/oneterm/`; `alacritty-ref/`
  is untouched. Confirmed.
* `Cargo.lock`, `crates/vt/Cargo.toml`, `crates/tools/Cargo.toml`: empty diff —
  **no new dependency**.
* IN-0028 edits: both are additive dated "superseded in place" notes that keep
  IN-0028 as the record of *why* the 10x20 cell and the `bands*6/20` rule exist
  and point forward at `graphics.md`. They are notes, not rewrites — **correct**.
* Trailer: one commit, `ac628dc`, ending

  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_018tAaHVGPnSzrEHLVcg3sx1
  ```

  identical in form and session to every other commit on the branch
  (`0f83a95`…`1a8f6f1`). PASS.

## 2. Quality gate — PASS

`pwsh scripts/ci-local.ps1` → **`ci-local: all checks passed.`** (exit 0)

Raw totals across the log's 57 `test result:` sections:
**1549 passed / 0 failed / 8 ignored** — exactly the packet's claim.
All five Python policy checks green (dependency graph 21 packages, doc paths 123,
check-english 741 files, completion catalogs, THIRD-PARTY-NOTICES).

Parity gates:

```
45 recordings, 45 passed, 0 failed (New engine)        # vt-corpus check --engine new
45 recordings, 45 passed, 0 failed (Old engine)        # vt-corpus check --engine old
1 recordings, 1 passed, 0 failed (New engine)          # --dir crates/vt/tests/corpus/oneterm  (sixel_basic)
45 recordings, 45 identical, 0 differing                # vt-diff
10 recordings, 10 identical, 0 differing                # vt-diff --fixtures   (| `sixel` | 262178 | 10045 | identical |)
1 recordings, 1 identical, 0 differing                  # vt-diff --dir .../oneterm  (| `sixel_basic` | 160 | 108 | identical |)
```

The new recording is a real exercise of the grammar, not a token one — its 160
bytes carry RGB and HLS register definitions, raster attributes, `!` repeat, `$`,
`-`, an image clipped on the right at column 18 of 20, and a third image at the
bottom that scrolls into history; `state.expect` pins `cursor row=7 col=0`, the
conhost `bands*6/20` rule. `config.json` `{"history_size":100}`,
`size.json` `{"columns":20,"screen_lines":8}`, `grid.expect` 108 rows.

## 3. Decoder fidelity: old engine vs new engine — PASS (one documented divergence)

Scratch differential harness: the same bytes into `alacritty_terminal::Term`
(via `vte::ansi::Processor<StdSyncHandler>`) and into `oneterm_vt::Terminal`,
comparing **every RGBA byte**, every placement cell `(row, col, id, offset_col,
offset_row)` — the new engine's offsets read through the public
`RenderState::graphic_offset`, the old engine's read out of `GraphicCell` — and
the final cursor `(row, col)`.

| Case | Result |
| --- | --- |
| `snake.six` (libsixel, 600x450, 256 KiB) | **identical**: 1 080 000 RGBA bytes, 0 differing; 1380 placement cells identical; cursor `(22, 0)` identical |
| IN-0028 test card (`make_sixel.py`, 96x48) | **identical**: 18 432 bytes, 25 cells, cursor `(4, 0)` |
| chunked feed, `snake.six` at 1 / 7 / 32768 bytes | identical to the whole-buffer result in all three |
| chunked feed, card at 1 / 7 / 32768 bytes | identical in all three |
| raster crop (`"1;1;4;3` over 6 columns of data) | identical |
| raster larger than the data (`"1;1;40;30`) | identical |
| HLS (`#1;1;120;50;100`), HLS hue 0 | identical |
| repeat (`!12~`), `$`, `-`, multi-band | identical |
| 4096 clamp (`!5000~`, `"1;1;9000;9000` + `!9000~`) | identical |
| corrupt band (`\x00`, `\x7f`, `\xff` inside data) | identical, no panic |
| empty `DCS q` | identical: 0 images, 0 cells |
| non-Sixel DCS (`\x1bP0$r1;2m\x1b\\`) | identical: **places nothing**, 0 images |
| `P2 = 1` and `P2 = 2` (`\x1bP0;1;0q`, `\x1bP0;2;0q`) | identical — both engines ignore P2, untouched pixels stay `[0,0,0,0]` |
| out-of-range register (`#300`) | identical |
| tall image (20 bands) on a 12-row and on a 4-row grid | identical, cursor `(6,0)` / `(3,0)` — the conhost `bands*6/20` rule |
| clipped right (`CUP 3;35` then a 3000 px image) | identical, cursor `(2, 34)` |
| scroll into history | identical |
| `CSI 2 J` on primary and on the alternate screen | identical placement cells |
| `IL`, `DL`, `SU`, `SD`, in-region scroll, overwrite | identical placement cells in all six |
| two images overlapping the same cells | identical |

`cargo test -p oneterm-tools --test zz_verify` → **11 passed, 0 failed**.

**The one divergence (deviation 7), confirmed real:**
`\x1bPq#5;7;1;2;3~~~~\x1b\\` — a five-parameter `#` whose `Pu` is neither 1 nor 2.

```
deviation7: 72 differing bytes; old first px [0, 0, 0, 255] new first px [51, 204, 204, 255]
```

The old engine returns before selecting (paints with register 0); the new engine
selects register 5, which the LLD's grammar table specifies. Cursor and cells
identical. No real producer emits `Pu ∉ {1,2}`.

### DCS byte cap / heap bound — PASS

Scratch test with a tracking `#[global_allocator]`, 32 MiB of unterminated
`DCS q` payload (`!4096~-` repeated) fed in chunks:

```
heap: sent=32 MiB  peak live=96.2 MiB  after feed=0.2 MiB  base=0.2 MiB
heap: after ST live=0.2 MiB
heap: placements=0 images=0
```

Bounded, and the decoder is dropped on the abort: live heap returns to the
baseline. Nothing is placed and no image is produced — matching
`aborted_dcs_stamps_no_cells`.

A *terminated* maximal image is also bounded, but is worth recording:

```
huge: peak=160.0 MiB images=1 size=Some((4096, 4096, 67108864))
```

i.e. one legal image can be 64 MiB of RGBA with a ~160 MiB transient during
decode. This is `MAX_DIMENSION`'s budget by construction and is the same in the
engine being replaced, so it is not a regression — but with `MAX_PLACEMENTS = 256`
there is no cap on the *total* pixel bytes a terminal holds. See finding F5.

## 4. Placement / anchor behaviour — PASS

Scratch probes, `cargo test -p oneterm-tools --test zz_probes` → **12 passed**.

| Probe | Raw output | Verdict |
| --- | --- | --- |
| P1 — image at the bottom, then 5 line feeds (R-13) | `placements=1 released=[] history=5`; placement `row=RowId(7)` | **alive in history**, no spurious release |
| P3 — overwrite every covered cell with text | `before=[Some(1),Some(1),Some(1),Some(1),None,None] after=[None,None,None,None,None,None] placements=1 released=[]` | cell refs drop; **no release** — see F1 |
| P3b — `EL 2` over the covered row | `graphic(2,0)=None placements=1 released=[]` | same shape as P3 |
| P4 — `CSI 2 J`, new/primary | `placements=1 released=[] viewport graphic cells per row=[0,0,0,0,0,0,0,0] history=1` | image **gone from the viewport**, kept in scrollback |
| P4 — `CSI 2 J`, new/alt | `placements=0 released=[1] viewport=[0,…,0]` | released exactly once |
| P4 — `CSI 2 J`, old/primary | `viewport=[0,…,0] history graphic cells=0` | old engine destroys it entirely |
| P5 — history trim | `placements=0 released=[1]` | released exactly once |
| P6 — reflow that kills the anchor | `reflowed=true trimmed=3 placements=0 events in batch=0 released=[]` then `after feed(&[]) released=[1]` | queued by `resize`, **delivered by `feed(&[])`** |
| P7 — 300 images, `MAX_PLACEMENTS` | `placements=256 released=44 first=Some(1) last=Some(44)` | bound holds, **oldest first** |
| P8 — two images on the same cells | new: `placements=[1,2] cells row2=[Some(2),Some(2),Some(2),Some(2),None,None]`; old: `cells row2=[Some(2),Some(2),Some(2),Some(2),None,None]` | the view sees **image 2 only, in both engines** |
| P9 — drain discipline | `first=1 second=0` after two `render_update`s | `take_graphics` is the only drain (R-16) |
| P11 — id reuse | `first=GraphicId(1) second=GraphicId(2) first resolves=false` | ids never reused, stale id resolves to nothing |
| P12 — image top row in history | `history=3 placement=RenderPlacement { row: RowId(0), cols: 4, rows: 6, pixel_size: (40,108) } col0 refs=[(0,1,Some((0,3))),(1,1,Some((0,4))),(2,1,Some((0,5))))]` | partly-off-screen image still fully computable |

**Judgment on `CSI 2 J` (deviation 5) against IN-0028's GUI walk.** The walk's
row reads "`cls` → The image is gone with its cells". P4 shows the old engine
destroys it (0 graphic cells in viewport *and* 0 in history, because its
`Cell::is_empty` ignores `graphic`, so `clear_viewport` resets the rows) and the
new engine removes it from the **viewport** while keeping it in scrollback
(R-13 / `is_erasable`). What the user observes — the image disappears on `cls` —
is preserved; scrolling back now reveals it, which is the intended R-13
improvement, not a regression. **Acceptable.**

**Judgment on release delivery via `feed(&[])`.** P6 confirms the contract: a
`resize` release reaches the embedder only on the next `feed`, an empty one
included. The shim's pump already calls `feed` every wake, and the packet
documents the requirement in *What the shim does*. **Acceptable for `US-0081`**,
provided `US-0081` actually issues the empty feed after a resize; that is the one
thing the next packet must not forget.

**Judgment on overlap (deviation 4).** P8 shows the visible outcome is
**identical to the old engine**: the cells carry the newer id, so the painter
draws image 2 only. The older placement stays live and its texture is retained
until the row is reset — a memory artefact, not a visual one, and bounded by
`MAX_PLACEMENTS`.

## 5. Render exposure — PASS

`RenderState::placements() / placement(id) / graphic_offset(id, row, col)` give
the painter everything `crates/terminal-view/src/render/{graphics.rs,element.rs
paint_graphics,frame.rs}` needs:

* origin — `RenderPlacement { row, col }` is the image's top-left **cell**, anchor
  already resolved, so the painter takes the first visible cell's origin and
  subtracts `offset * cell_size` (P12 proves this works when the top row is in
  history: offsets `(0,3) (0,4) (0,5)` on a 6-row image with 3 rows scrolled off).
* scale — `pixel_size` plus `VIRTUAL_CELL (10, 20)`; `cell_width / 10`,
  `line_height / 20`, matching the old `GraphicData` + `VIRTUAL_CELL` pair.
* `GraphicData { id, width, height, rgba }` is byte-identical to the vendored
  struct the view consumes today (§3).
* No interned id crosses the boundary: `RenderCell.graphic` carries a
  `GraphicId` only, and `RenderState` holds no pixels (R-16).
* Stale ids are inert: `placement()` returns `None` after a release (P11), and
  `graphic_offset` returns `None` for a cell outside the extent.

The whole differential in §3 read the new engine's per-cell offsets **through
`RenderState::graphic_offset`** and they matched the old engine's per-cell
`GraphicCell { col, row }` in every one of the 1380 cells of `snake.six`. That is
the strongest available evidence that `US-0081` can drop the per-cell offset.

## 6. Deviations from the LLD — judgments

| # | Deviation | Judgment |
| --- | --- | --- |
| 1 | No `DcsSink` trait | **Justified.** One implementor; Kitty arrives on APC. No doc change needed beyond the packet's record. |
| 2 | No reserved Kitty fields on `Placement` | **Justified.** YAGNI; the per-placement record is the part that is hard to retrofit and it exists. |
| 3 | `render_update` does not run the release scan | **Justified, and the LLD is wrong.** `render_update` has no batch, and R-16 keeps `RenderState` out of graphics ownership; a release there is unobservable. **Design owner: correct `damage-and-render-state.md`.** |
| 4 | Overlap debug invariant not implemented | **The LLD's invariant is false, not the implementation incomplete.** Confirmed by P8: `CUP` back and print a second image and two live placements legitimately share a bottom row, on a `HAS_GRAPHIC` false positive the LLD itself declares allowed. The implemented dangling-reference invariant is the right one. **Design owner: drop the overlap invariant from `graphics.md`.** |
| 5 | `release_event_fires_on_clear_screen` written on the alt screen | **Justified** (P4, §4). Primary `ED 2` must *not* release under R-13, and the test now asserts that explicitly. |
| 6 | `MAX_PLACEMENTS = 256`, oldest released | **Justified and needed** (P7). The LLD bounds nothing; an unbounded table plus a linear sweep is a DoS. **Design owner: record the bound in `graphics.md`.** |
| 7 | Unknown colour mode selects the register | **Real divergence from the old engine, confirmed** (§3, 72 differing bytes). Follows the LLD text. Harmless — no real producer reaches it — but it is a parity break that the corpus gate cannot see, so it belongs in the packet as such rather than as a pure "LLD text followed". |
| 8 | `SixelParser::new()` takes no parameters | **Justified.** The decoder never read `P1;P2;P3`; the dispatch layer does the `q` test. Confirmed by the `P2 = 1/2` rows in §3. |
| 9 | `Vec<ScrollReport>` out of `graphics::place` | **Justified.** Keeps `Handler::report` the single reporting path. |

## 7. Code quality — PASS

* `grep -rn unsafe crates/vt/src/graphics/ crates/vt/src/render/state.rs crates/vt/src/terminal/dispatch.rs` → **no match**.
* `grep -rn '\.unwrap()\|\.expect('` over `graphics/{mod,sixel,placement}.rs`,
  `render/state.rs`, `terminal/dispatch.rs` → **no match** outside tests. The
  port even removed the vendored `args.last_mut().expect(...)` in favour of an
  `if let Some(last)` (`sixel.rs:argument`).
* Public surface is the frozen API in the packet and nothing more:
  `graphics::{VIRTUAL_CELL, MAX_DIMENSION, MAX_PIXEL_BYTES, MAX_PLACEMENTS,
  GraphicData, Placement}`, `Terminal::{take_graphics, placements}`,
  `render::RenderPlacement` + three `RenderState` accessors. `GraphicsState`,
  `SixelParser`, `DecodedSixel`, `place`, `sweep`, `drain_released`,
  `assert_integrity` are all `pub(crate)`.
* Module and item docs are dense and cite the owning LLD sections; the
  deliberate O(n) costs carry `ponytail:` comments.
* No new dependency (§1).
* Attribution: `crates/vt/src/graphics/sixel.rs` states the port source
  (`vendor/patches/alacritty_terminal/0003`, OneTerm's own patch, Apache-2.0), so
  no third-party licence question arises. `crates/vt/tests/corpus/NOTICE` gains an
  `oneterm/` section that correctly says the files are OneTerm's own (Apache-2.0),
  synthetic rather than captured, and blessed by the old engine then frozen — the
  vendored section still covers only `alacritty-ref/`. `third-party-notices.py
  --check` passes.

## 8. Packet completeness — PASS with corrections

**Acceptance bullet 1 verified mechanically.** All **25** test names listed in
`low-level-design/graphics.md` § Verification were grepped for in
`crates/vt/src/graphics/graphics_tests.rs`: each appears exactly once, none
missing, none duplicated. `cargo test -p oneterm-vt --lib graphics::` →
`26 passed; 0 failed` (the 25 named plus `empty_and_non_sixel_dcs_place_nothing`)
in 0.60 s.

`overwriting_or_erasing_a_cell_drops_the_reference`
(`graphics_tests.rs:341-358`) is itself honest — it asserts only that the *cell
reference* drops and explicitly documents the R-13 primary-screen behaviour. It
is the packet's Acceptance sentence, not the test, that overstates (F1).

Outcome / Scope / Acceptance / Documentation / Reconciliation / Context / Plan /
Decisions / Verification Plan / Public API / Deviations / Evidence / Gaps /
Handoff are all present and specific; status `Implemented`; proof boxes
consistent with the gaps (no E2E, no platform proof, both owned by `US-0081`).
The evidence numbers reproduce exactly (§2).

**Harness DB row `US-0080`** (read from a copy of `harness.db`, table `story`):
present and accurate — `risk_lane=high_risk`, `contract_doc=…/low-level-design/graphics.md`,
`packet_doc=…/US-0080-graphics.md`, `status=implemented`, `unit_proof=1`,
`integration_proof=1`, `e2e_proof=0`, `platform_proof=0`,
`verify_command='pwsh scripts/ci-local.ps1'`, `last_verified_result=pass`,
`intake_id=34`, with the full evidence and notes text. Every number in it
reproduces (§2). The same F1/F2 wordings appear in the `evidence` column and
should be corrected there too.

Corrections needed — see F1/F2 below.

---

## Findings

| id | Severity | Where | What | Fix |
| --- | --- | --- | --- | --- |
| F1 | Low (doc accuracy) | `docs/spec-intakes/IN-0029-vt-engine/US-0080-graphics.md:63` (Acceptance bullet 3) | "…and is dropped by an erase, **an overwrite of every covered cell**, a history trim and a reflow that kills its anchor — each firing `GraphicReleased` exactly once." P3/P3b disprove the overwrite and the `EL 2` clauses: the cell references drop, the placement stays live and **no** release fires, because liveness is the `HAS_GRAPHIC` row flag and neither an overwrite nor `EL 2` clears it. The behaviour is the design's documented false positive (Gaps 3, deviation 4) — the acceptance sentence is what is wrong. | Reword to "…dropped by a row reset (`Row::reset`, scroll blanking, an alt-screen wipe), a history trim and a reflow that kills its anchor"; move erase/overwrite to the false-positive note. |
| F2 | Low (doc accuracy) | same file, Deviations item 7 | Records the unknown-colour-mode change as "the LLD text is followed. No test in either suite reaches it" without saying it is a **behavioural divergence from the engine being replaced** (72 of 96 RGBA bytes differ, §3). | Add one sentence: old engine selects nothing, new engine selects; parity break on `Pu ∉ {1,2}` only, invisible to the corpus gate. |
| F3 | Low (design debt, owner) | `low-level-design/damage-and-render-state.md`, `low-level-design/graphics.md` | Deviations 3, 4 and 6 are LLD defects, not implementation shortcuts: the `render_update` release scan is unimplementable, the overlap invariant is false, and the placement bound is missing from the contract. | Design owner reconciles the three LLD rows; no code change. |
| F4 | Info | `crates/vt/src/graphics/placement.rs:170` (`sweep` / `is_live`) | A placement that is invisible (every covered cell overwritten) keeps its anchor, its table slot and — through the view — its GPU texture until the row is reset or `MAX_PLACEMENTS` evicts it. Bounded and visually correct (P3, P8), but it means `US-0081`'s texture store must tolerate up to 256 live textures. | None required; note it in the `US-0081` packet. |
| F5 | Info | `crates/vt/src/graphics/mod.rs:52` (`MAX_PLACEMENTS`) | `MAX_DIMENSION` bounds one image at 64 MiB of RGBA (measured: `huge: peak=160.0 MiB … (4096, 4096, 67108864)`); `MAX_PLACEMENTS` bounds the count but not the total pixel bytes held. Identical to the engine being replaced, so not a regression. | Consider a total-bytes budget in a later packet if a workload ever holds many large images. |

## Verdict

**Merge.** The decoder is a faithful port — pixel-for-pixel identical to the
engine being replaced on a real 600x450 libsixel image and on IN-0028's own test
card, at every chunk boundary, with identical placement cells and an identical
conhost cursor — and the new cell-anchored ownership model is verifiably better
behaved (releases fire, ids never repeat, stale ids are inert, a partly
scrolled-off image is still paintable). The gate is green and reproducible. The
findings are two packet wordings (F1, F2), three LLD rows for the design owner
(F3) and two notes (F4, F5); none of them blocks the merge or `US-0081`.
