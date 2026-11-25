# US-0085 — independent verification

Date: 2026-09-13
Verifier: independent agent (not the implementer)
Branch under test: `worktree-agent-ad731760c1d949455` @ `30269c9`, five commits off
`feat/vt-engine` @ `d3c537b`
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-ad731760c1d949455`
Environment: `Get-PSDrive D` free 25.9 GB at start, 16.5 GB at end (the worktree's
own `target/` is 19 GB + 5.5 GB `fast-dev`); `quser` session 1 **`Disc`**, idle
12:18, for the whole run. `Get-Process oneterm` recorded first: **none running**;
no OneTerm process was started, signalled or enumerated by this verification.

**Verdict: merge after fixes** — no blocker, no correctness defect found; five
minor items below, and the packet's own declared GUI gap stands.

---

## 1. Pass/fail table

| # | Check | Result |
| --- | --- | --- |
| 1a | Scope vs may-touch | **pass (with a note)** |
| 1b | Trailers exact on all five commits | **pass** |
| 1c | `grep -rn alacritty_terminal crates/terminal crates/terminal-view` | **pass** |
| 1d | Trial merge with `US-0083`'s branch | **pass — no conflicts, no shared file** |
| 2a | `pwsh scripts/ci-local.ps1` | **pass (exit 0)** |
| 2b | `us0081_parity` corpus and allow-list unwidened | **pass — byte-identical allow-list** |
| 3 | Headless old-path vs new-path render comparison | **pass — 2 822 154 cells, 0 differences** |
| 4 | `plan_cache` semantics and key aliasing | **pass — 0 collisions, all invalidations correct** |
| 5a | `mouse_encode` on `ModeSnapshot` | **pass — provably identical for every reachable mode** |
| 5b | `SearchMatch` on `RowId` across scroll | **pass — 0 mismatches over 30 offsets** |
| 6 | Measurements reproduced | **pass — flood table reproduced within noise** |
| 7 | GUI walks | **not run — session `Disc`; item 3 stands in** |
| 8 | Code quality, packet completeness, DB row | **pass (with minors)** |

---

## 2. Item 1 — scope, trailers, deletion gate, merge

### Trailers

```text
$ git log d3c537b..HEAD --format='--- %h%n%(trailers:only=true)'
--- 30269c9  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
             Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9
--- 0a8b7f3  (identical)
--- 52079f9  (identical)
--- 399750f  (identical)
--- 946c4e5  (identical)
```

All five carry both lines, identical, matching the model that did the work.

### Deletion gate

```text
$ grep -rn "alacritty_terminal" crates/terminal crates/terminal-view
crates/terminal/Cargo.toml:34            alacritty_terminal.workspace = true   [dev-dependencies]
crates/terminal/src/backend/osc_router.rs:305   comment (fork line numbers)
crates/terminal/src/content.rs:13               comment (what US-0085 deleted)
crates/terminal/src/handle.rs:9                 comment (parking_lot vs FairMutex)
crates/terminal/src/lib.rs:9                    comment (what is left, and where)
crates/terminal/tests/us0081_parity.rs          the dev-oracle (34 hits)
```

Exactly as the acceptance states: four comments, one `[dev-dependencies]` line,
one test. No product code in either crate names the fork. `engine_shim.rs` does
not exist. The `impl_pty_terminal_session!` macro expands `$crate::` paths only —
grepped the whole macro body, the single `oneterm_vt` occurrence is in a doc
comment — so the `US-0084` macro wall is genuinely cleared.

### Scope

Every changed file is on the may-touch list except two, both benign:

- `crates/terminal/src/content_tests.rs`, `crates/terminal/src/model_tests.rs` —
  the test modules of `content.rs` / `model.rs`, which are listed. See minor **M4**.

### Trial merge with `US-0083`

```text
$ git merge-tree --write-tree HEAD worktree-agent-a20da8012abddb9c7
fccbf38855d95a1f13625e3b972c12761a8ba728        (exit 0 — clean)

$ comm -12 <(this branch's changed files) <(US-0083's changed files)
(empty)
```

`worktree-agent-a20da8012abddb9c7` is at `5f9cc1e` and its three commits touch
**no file this branch touches** — `crates/local-shell/src/session_tests.rs`
included. The packet's Gap 3 ("the edits are listed line by line so they can be
re-applied if that packet's branch wins the merge") is a non-issue as the two
branches actually stand. See minor **M5**.

---

## 3. Item 2 — CI and the parity oracle

```text
$ pwsh scripts/ci-local.ps1
==> cargo fmt --all -- --check                         ok
==> cargo clippy --workspace --all-targets -- -D warnings   ok
==> cargo test --workspace                             ok
    oneterm_terminal          245 passed; 0 failed; 0 ignored
    us0081_parity               5 passed; 0 failed; 2 ignored
    oneterm_terminal_view     288 passed; 0 failed; 3 ignored
==> cargo test -p oneterm-vt --features vt-paranoid    359 passed; 0 failed; 2 ignored
==> python scripts/verify-dependency-graph.py          21 packages, passed
==> python scripts/check-doc-paths.py                  124 paths, passed
==> python -m unittest scripts/test_check_english.py   OK
==> python scripts/check-english.py                    755 files, passed
==> python scripts/completion-catalog.py validate      all catalogs valid
==> python scripts/third-party-notices.py --check      up to date
ci-local: all checks passed.                           [exited with code 0]
```

### The oracle was not weakened

Normalised diff of `crates/terminal/tests/us0081_parity.rs` against `f9af66c`
(the `US-0081` merge), 18 hunks:

- The **allow-list is byte-identical** — the same five entries
  (`cell.hyperlink`, `damage`, `mode`, `mode.dropped_line_wrap_urgency`,
  `total_lines`) with the same comments. Nothing widened.
- `corpus_cases()`, `hand_cases()`, `diff_cells`, `diff_content`,
  `check_damage_sound` and the chunking are unchanged — still the same 81
  streams, compared field by field.
- The only substantive change: the new side's legacy shape now comes from
  `legacy_from_native(&TerminalContent, …)` instead of `TerminalContent::refill`.
  The comparison target is the same; the producer moved into the test.

**Observation that drove item 3.** `legacy_from_native` is a *second,
independent* implementation of the conversion the view now performs in
`render/frame.rs`. A green parity run therefore proves the engine and the native
accessors, but it does **not** cover `FrameRow::cell`. That hole is what the
headless comparison below closes.

---

## 4. Item 3 — headless old-path vs new-path render comparison

This replaces the sampler pixel comparison. `row_plan.rs` was not changed by this
packet other than losing its `hash` field, and `element.rs` changed only where it
derives the graphic anchor, so **identical `Cell` streams + identical
cursor/selection/graphic anchors imply identical `RowPlan`s and therefore
identical painter inputs** (text runs, style runs, wide/spacer handling, cursor
rect and shape, selection quads, hyperlink underline spans, cell anchors).

### Method

A scratch test module in the worktree's `crates/terminal-view` (removed again;
copies in the session scratchpad as `indep-harness/us0085_indep.rs` and
`indep-harness/plan_cache-indep-tests.diff`) ran, per stream and per chunk:

- **OLD path**, copied verbatim from `git show d3c537b:` — `engine_shim::write_row`
  (run iteration, vt → alacritty: `legacy_style` / `legacy_color` /
  `legacy_flags` / `width_flag`) composed with `frame::Cell::from_indexed`
  (alacritty → view: `Color::from_vte`, `CellFlags::from_vte`, including the
  `NamedColor::DimBlack` discriminant arithmetic). Built against the real
  `alacritty_terminal` types via a temporary dev-dependency, so nothing was
  re-derived by hand.
- **NEW path** — `FrameRow::cell(col)` plus `element.rs`'s
  `frame.graphic_offset(id, row_id, col)`.

Compared per cell: `ch`, zero-width followers, `fg`, `bg`, the full 12-bit
`CellFlags` (inverse/bold/italic/dim/hidden/underline/undercurl/strikeout/
wide/wide-spacer/leading-wide-spacer/wrapline), hyperlink presence, graphic id
and derived `(across, down)` anchor. Plus, per frame: `FrameRow::wraps()`,
`Frame::size()`, cursor row and hidden-ness, selection rows, and the invariant
that each `RenderCell::run` index lands inside that run's own `cols` range (the
new path reads the index, the old one read the range).

### Result

```text
running 3 tests
test render::us0085_indep::the_style_maps_are_pointwise_equal ... ok
hand streams: 15; cells compared: 368400; diffs: 0
test render::us0085_indep::old_and_new_view_conversions_agree_on_hand_streams ... ok
corpus streams: 46; cells compared: 2453754; diffs: 0
test render::us0085_indep::old_and_new_view_conversions_agree_on_the_corpus ... ok

test result: ok. 3 passed; 0 failed
```

- **46 corpus streams** (the 45 vendored alacritty recordings + OneTerm's
  `sixel_basic`), chunked at 4 096 bytes, every intermediate frame compared.
- **15 hand streams** covering SGR (all five underline kinds, overline, dim,
  hidden, inverse), 256-colour, truecolour, bright, default-restore, CJK,
  Hangul, combining marks, ZWJ emoji, ligature-shaped ASCII, OSC 8 hyperlinks
  and an auto-detected URL, wide-at-edge, wrap, alt-screen swap, RIS,
  scrollback, cursor shapes incl. `?25l`, Sixel, **Sixel then `cls`**, tabs and
  NULs — each run twice, at 4 096 bytes and **one byte at a time**, so every
  intermediate frame is compared.
- **2 822 154 cells compared, 0 differences.**
- `the_style_maps_are_pointwise_equal` additionally checks the maps in isolation:
  all **4 096** subsets of the twelve SGR attribute bits, all four `CellWidth`
  values, all 29 `NamedColor`s, all 256 palette entries and an RGB value — old
  composed map == new direct map, pointwise. This is what retires the deleted
  `NamedColor::DimBlack` discriminant arithmetic safely.

Two divergences I expected from reading the diff and which measured as
**behaviourally identical**:

- `Frame::cursor()` now returns `-1` when the cursor is scrolled out of view,
  where the old path returned a large positive row. `cursor::resolve` rejects
  both (`cursor.rs:46` guards `row < 0 || row >= rows`), and the in-viewport rows
  agree exactly.
- `FrameRow::cell` sets the hyperlink from `cell.hyperlink` directly, where the
  old path required `state.hyperlink(id)` to resolve first. Over 2.8 M cells the
  two never disagreed — every id resolves.

### What this does not cover

Selection geometry was checked analytically and by the view's own tests, not by a
stream: both paths reduce to `row.0 - viewport_top` (old:
`(row - screen_top) + display_offset` with `screen_top = viewport_top + offset`),
which is the same expression. The streams contain no drag. And, as the packet
says: what a frame *contains* is now proven very thoroughly; what a GPU draws
from it is not.

---

## 5. Item 4 — `plan_cache` semantics and key aliasing

Scratch tests added to `plan_cache::tests` (removed again; diff in the scratchpad):

```text
INDEP settled-idle:    update Unchanged             candidates 0  planned 0  unchanged 1
INDEP cursor-move:     update Partial{scrolled:0}   candidates 0  planned 0
INDEP resize:          size 6x30                    candidates 6  planned 6
INDEP RIS:             update Full                  candidates 6  planned 6  aliased []
INDEP alt-enter:       update Full  planned 6  aliased-with-primary []
INDEP alt-leave:       update Full  planned 6  aliased-with-alt []  restored-primary true
INDEP alt-re-enter:    update Full  planned 6  aliased-with-first-visit []
INDEP ring-wrap:       4000 distinct keys, 0 collisions   (2 000 scrolled lines, 6-row viewport)
INDEP search translation: 0 mismatches over 30 offsets
```

- **Idle**: `RenderUpdate::Unchanged` short-circuits before the key scan, the URL
  scan and layout — `rows_candidate == 0`, `rows_planned == 0`,
  `frames_unchanged == 1`. Confirmed at `plan_cache.rs:112-118`, which returns
  before `self.dirty` is even sized.
- **Scroll**: `shift()` rotates `rows`, `keys` and `mask_prev` by the reported
  delta and the key comparison then finds only the scrolled-in rows — the
  packet's own `scroll_shifts_plans_and_replans_only_scrolled_in_rows` shows
  2-of-5 and 1-of-5; I reproduced it and found no reshaping of the kept rows.
- **Cursor move**: a bare `CUP` replans **zero** rows, not two. That is correct
  and better than the task's expectation: the cursor is its own paint layer
  (`cursor::resolve` reads `frame.row(...)` fresh every frame), so it was never
  baked into a `RowPlan`. The old code made the cursor row a candidate on every
  frame; this packet's `changed()` list is exact and does not.
- **Resize / RIS / alt-swap**: all three reach `rows_planned == rows`. Resize goes
  through the `restyled` branch (grid size changed); RIS and both alt directions
  report `RenderUpdate::Full` *and* carry fresh `(RowId, SeqNo)` keys, so the
  rebuild does not depend on `Full` alone.
- **Key aliasing — the question the task singles out.** `RowId` is an absolute,
  monotonically increasing `u64` whose top bit is the alternate-screen lane
  (`crates/vt/src/grid/mod.rs:62`, `ALT_ORIGIN = 1 << 63`), so a scrollback ring
  wrap recycles *slots*, never ids. Empirically: 2 000 scrolled lines produced
  4 000 distinct keys and **zero** cases of one key covering two different row
  texts. Across an alt/primary swap the two lanes never intersect, leaving alt
  restores the primary keys verbatim, and a **second** visit to the alt screen
  produced keys disjoint from the first visit's. No aliasing path found.

---

## 6. Item 5 — mouse and search

### `mouse_encode` on `ModeSnapshot`

Verified by reading rather than a table test, because the table test would be
vacuous: at `d3c537b` the mode reaching `mouse_encode` was **already** derived
from `ModeSnapshot` by `engine_shim::legacy_mode`, which mapped the engine's
single `MouseEncoding` value onto exactly one of `SGR_MOUSE` / `UTF8_MOUSE`
(`engine_shim.rs:565-568`):

```rust
match mouse.encoding {
    MouseEncoding::Default => {}
    MouseEncoding::Utf8 => mode |= TermMode::UTF8_MOUSE,
    MouseEncoding::Sgr  => mode |= TermMode::SGR_MOUSE,
}
```

The new `encode()` checks `encoding == Sgr` then `== Utf8` in the same order. For
**every mode the old path could produce**, the two encode identically; the
"both bits set" case the old bitset could in principle express was already
unreachable before this packet. `? 9 / 1000 / 1002 / 1003` gating is the caller's
(`model.rs:196` / `mouse_reporting()`), untouched. Focus events (`? 1004`) were
never read by the view at `d3c537b` either — the only readers of
`Mode::FocusInOut` at that commit are `crates/tools` and the engine itself — so
nothing regressed. Bracketed paste moved from `mode.contains(BRACKETED_PASTE)` to
`modes.bracketed_paste`; application cursor from `mode.contains(APP_CURSOR)` to
`modes.app_cursor`; both one-for-one.

### `SearchMatch` on `RowId`

`display_row(screen_top, offset) = (row - screen_top) + offset`, against the old
`line + offset` where `line` was already `row - screen_top`. Same expression, and
strictly more robust: the match now carries a stable `RowId` instead of a signed
line that went stale as output pushed history.

The whole translation rests on `screen_top == viewport_top + scroll_offset`.
Checked at 30 consecutive scroll positions over a 40-line history, together with
the round trip `content.row_id(d) → display_row → d`:

```text
INDEP search translation: 0 mismatches over 30 offsets
```

---

## 7. Item 6 — measurements

### Flood table, `fast-dev`, this machine, median of three

`cargo test -p oneterm-terminal --profile fast-dev --test us0081_parity flood_bench
-- --ignored --nocapture`, 4 MiB / 4 KiB chunks / 120x30 / 10 000 scrollback /
1 025 frames. "Before" measured by `git checkout d3c537b -- crates/terminal` in
this same worktree and restored afterwards.

| | before (`d3c537b`) | packet claims | after (`HEAD`) | packet claims |
| --- | ---: | ---: | ---: | ---: |
| total | **163.6 ms** | 164.4 ms | **136.2 ms** | 140.9 ms |
| `feed` | 101 ms | 102 ms | 99 ms | 103 ms |
| **snapshot** | **60 ms** | 61 ms | **35 ms** | 36 ms |
| per frame, p95 | 67 us | 64 us | 37 us | 39 us |

**Reproduced.** The snapshot column really does drop ~25 ms, not the ~9 ms
`migration.md` attributed to the `Cell` rebuild alone, and `feed` is unchanged as
it must be. (Release was not re-measured: the worktree's `target/` had already
reached 19 GB and a third profile would have crossed the disk floor.)

### Headless frame time, `fast-dev`

`cargo test -p oneterm-terminal-view --profile fast-dev frame_time_under_output
-- --ignored --nocapture`:

```text
flood (prepaint+paint): n=257 total=645 ms avg=2513 us p50=2432 us p95=3161 us max=4269 us
idle  (prepaint+paint): n=60  total=18 ms  avg=307 us  p50=294 us  p95=426 us  max=455 us
rows planned over 257 flood frames: 13868 of 17733 viewport rows
```

The packet records p50 1 884 us / 238 us; mine are ~25 % higher across **both**
columns, which is machine load (a build was running), not a discrepancy — the
ratio is identical and the deterministic number, **13 868 of 17 733 rows
planned, matches exactly**. The idle-counter claim is confirmed independently by
§ 5 above: every idle frame is `Unchanged` with 0 candidates and 0 plans.

---

## 8. Item 7 — GUI walks

```text
$ quser
USERNAME   SESSIONNAME  ID  STATE  IDLE TIME  LOGON TIME
>trunglt                 1  Disc      12:18   9/11/2026 4:35 PM

$ Get-Process oneterm
(no processes)
```

Session **`Disc`**, exactly as the packet records. None of the four walks was run
and no `US-0085-verify-*.png` exists. No OneTerm instance was launched,
enumerated or signalled by this verification. Item 3 stands in, and it is a much
stronger stand-in than the packet claims for itself: the packet argued from
`us0081_parity` (which, as § 3 notes, does **not** exercise `render/frame.rs`),
whereas § 4 compares the view's own conversion, old against new, over 2.8 M
cells. What remains genuinely unverified is only the GPU draw — glyph
rasterisation, ligature shaping and the Sixel upload as pixels.

---

## 9. Item 8 — code quality, completeness, DB

- **No `unsafe`** added anywhere. The only `unsafe` in the touched crates is the
  pre-existing counting allocator in `element_tests.rs` (not in this diff).
- **No `unwrap` / `expect` / `panic!` / `todo!` added on any runtime path**
  (`git diff d3c537b...HEAD -- crates/*/src | grep '^+.*\.(unwrap|expect)\('`
  is empty outside tests and fixtures).
- **No dead code**: `RowPlan::hash` and `invalidate()` went with the field;
  `Fnv1a` survives with `write` / `write_u16` / `write_u32`, all three still
  called (`glyphs.rs`, `state.rs`, `theme/palette.rs:164`); `write_u8` and
  `hash_vte_color` are gone. Clippy `-D warnings` over all targets confirms.
- **Module boundary held**: `element_tests::render_has_single_engine_file` now
  guards five engine *cell* needles instead of one fork name, and passes.
- **Packet completeness**: Outcome / Scope / Acceptance / Documentation /
  Reconciliation / Context / Plan / Decisions / Verification Plan / Evidence /
  Gaps / Handoff all present; the one unmet acceptance box is checked `NOT MET`
  with its reason. The `migration.md` obligations I could check are met: the
  `US-0085` may-touch row, the deletion list (all four entries deleted: the
  display-line conversion, the per-frame clone loop, the two display-offset
  fallbacks, `engine_shim.rs`), S1/S2/S3 retired, the IN-0018 HLD reconciliation
  (`0a8b7f3`), and the "two-line follow-up" correctly recorded in the Handoff to
  `US-0083` / `US-0084` rather than done here.
- **Harness DB row** (`harness.db`, table `story`):
  `US-0085 | crates/terminal-view goes native | high_risk | implemented |
  unit 1, integration 1, e2e 0, platform 0 | verify pwsh scripts/ci-local.ps1 |
  last_verified pass | intake 34`. Accurate, including `e2e_proof = 0` for the
  walks.

---

## 10. Findings

### Blocker

None.

### Major

**J1 — the four GUI walks and the sampler pixel comparison are still owed.**
Declared by the packet itself (acceptance box `NOT MET`, Gaps 1-2) and confirmed
independently: session 1 is `Disc`. `IN-0029.md`'s intake acceptance asks for the
walks reproduced, so this is a merge-policy call for the owner, not a defect. My
§ 4 narrows what is actually at risk to the GPU draw alone. *Fix: re-run the four
walks from an Active desktop before the intake closes, not necessarily before
this packet merges.*

### Minor

**M1 — `interner_mut` is public engine API for one test helper.**
`crates/vt/src/terminal/mod.rs:387-397` adds an unconditional
`pub fn interner_mut(&mut self) -> &mut Interner`. Its only caller in the whole
workspace is `crates/terminal/src/test_support.rs:112`, which is itself behind
the `test-support` feature. As written it hands every downstream consumer mutable
access to the engine's intern tables. *Fix: `crates/vt` has no `test-support`
feature today — either add one and gate the accessor, or mark it
`#[doc(hidden)]` and say in the doc comment that it is not a supported entry
point.*

**M2 — stale module doc.** `crates/terminal-view/src/render/mod.rs:3` still reads
"`frame` (the only alacritty-typed file)" after the packet removed every
alacritty type from the crate. The packet renamed the matching test
(`render_has_single_alacritty_file` → `render_has_single_engine_file`) but not
this line. *Fix: "the only engine-typed file".*

**M3 — `evidence/US-0085-verify.md` § 2 records the view suite as "2 ignored";
CI reports 3.** The new `element_tests::frame_time_under_output` is the third.
*Fix: one digit.*

**M4 — two files edited that the Scope section does not list.**
`crates/terminal/src/content_tests.rs` and `crates/terminal/src/model_tests.rs`.
They are the test modules of `content.rs` and `model.rs`, which *are* listed, and
the rewrites are documented in § 2 of the implementer's verify — so this is a
wording gap, not scope creep. *Fix: add "and their `_tests.rs` modules" to the
Scope bullet.*

**M5 — Gap 3 overstates the `US-0083` risk.** `git merge-tree` of the two
branches is clean and they share **no** changed file: `US-0083`'s branch
(`worktree-agent-a20da8012abddb9c7` @ `5f9cc1e`) never touches
`crates/local-shell/src/session_tests.rs`. *Fix: soften Gap 3 to "verified clean
against `US-0083` @ `5f9cc1e`", keeping the line-by-line list in case that branch
moves.*

---

## 11. Verdict

**Merge after fixes.**

The rewrite is correct where it matters most: the conversion the whole screen
depends on produces, cell for cell, exactly what the deleted path produced —
2 822 154 cells across 61 streams with zero differences, and the style, colour
and width maps proved pointwise equal in isolation. The plan cache's new
`(RowId, SeqNo)` key cannot alias across a ring wrap, an alt/primary swap, a
second alt visit or a RIS, and every invalidation the task asked about behaves
as specified — with the cursor case better than specified. The parity oracle was
not weakened, CI is green, and the flood numbers reproduce within noise.

The five minor items are all documentation or API-hygiene, none of them affecting
behaviour; **M1** is the only one worth insisting on before merge. The GUI gap is
real, declared, and belongs to the intake rather than to this packet.

Reproduction artefacts for §§ 4-6 are in the session scratchpad under
`indep-harness/`; the worktree was returned to a clean `git status` and nothing
was committed.
