# US-0092 — independent verification

Packet: `US-0092` (`IN-0032`) — change-scoped per-frame work
Verifier: independent agent (not the implementer)
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-aac27e9cee6173dde`
Under review: `git diff 4e83f31..f963a94` (three commits: `9112782`, `3fcc8fb`, `f963a94`)
Date: 2026-09-14

## Verdict

**FAIL** — two behaviour regressions, both in the class the packet's own Acceptance forbids
("`last_content_row` returns the same value for every input it does today" and "URL underlining
must be pixel-identical for the same input"). Each is reproduced by a test in this worktree that
passes against `4e83f31` and fails against `f963a94`.

Everything else in the packet holds: the counted-work proofs are real guards (they fail against
the old code), the wrap-run union logic is sound for every in-viewport case tested, no new
cross-crate dependency was added, and `ci-local.ps1` passes with exactly the totals recorded.

| # | Severity | Site | Summary |
| --- | --- | --- | --- |
| 1 | **Major** | `crates/terminal/src/content.rs:74-77` | `occ == 0` skips a row erased with a non-default background (BCE). `last_content_row` now under-reports; the gutter loses timestamps on those lines — H2's own stated failure mode. |
| 2 | **Major (STALE-RENDER)** | `crates/terminal-view/src/render/plan_cache.rs:224-253` (`mark_scan_runs`) | A wrapped URL whose head scrolls above the viewport top leaves a stale underline on the continuation row. The wrap-run closure cannot see that the viewport boundary moved. |
| 3 | Minor (note) | `crates/terminal/src/content.rs:68-73` | The code comment's premise ("a row the engine says was never written") is not what `occ` means after a `reset`; the LLD it cites says so two lines further on. |

---

## 1. Defect 1 — `occ == 0` skips a background-erased row

**Severity:** Major (behaviour regression, visible in the gutter)
**File:** `crates/terminal/src/content.rs:74-77`

```rust
let occ = usize::from(row.occ());
if !row.is_allocated() || occ == 0 {
    continue;
}
```

### Why it is wrong

`occ` is **not** "this row has no content". `Row::reset` (`crates/vt/src/grid/row.rs:161-171`)
fills the row with the *erase template* and then sets `occ = 0`:

```rust
pub fn reset(&mut self, template: Cell, seq: SeqNo) {
    ...
    self.cells[..occ].fill(template);
    self.header.occ = 0;
    ...
}
```

`Screen::reset_row` passes `self.cursor.erase` as that template
(`crates/vt/src/grid/screen.rs:503-505`), so after `CSI 44 m` + `ED`/scroll the row's cells carry
`bg = Blue` while `occ == 0`. `is_blank_cell` (`content.rs:35-48`) rejects such a cell
(`style.bg != Color::Named(NamedColor::Background)`), so the **old** code called the row content
and the **new** code skips it.

The LLD the packet quotes states this outright two lines below the sentence that was quoted
(`IN-0029/low-level-design/grid-and-scrollback.md:247-252`): *"when the template is the default
style that path is a `memset` of `Cell::EMPTY`, and when it is not it is a fill with the template
cell."* So `occ` is a *touched-since-reset* hint, not a *blank* hint, and a background erase is
precisely where the two diverge. The packet's `..occ` narrowing is also affected by the same
fact, not just the `occ == 0` skip.

### Reachable paths to `Row::reset` with a non-default template

| sequence | path | result |
| --- | --- | --- |
| `CSI 44 m` then `CSI J` (ED Below) | `erase_in_display` → `reset_rows` → `reset_row` → `blank_row(cursor.erase)` | rows below the cursor painted blue, `occ == 0` |
| `CSI 44 m` then `CSI 2 J` on the **alternate** screen | same | whole alt screen painted, `occ == 0` |
| `CSI 41 m` then a scroll (`\n` at the bottom, `SU`, `SD`, `IL`, `DL`) | `screen.rs:817, :849, :884` → `reset_rows` | each row scrolled in painted red, `occ == 0` |

### Repro (tests added by this verification)

`crates/terminal/src/content_tests.rs` — three new tests:

- `last_content_row_sees_a_background_erased_row`
- `last_content_row_sees_a_background_erased_scroll_in`
- `last_content_row_default_erase_stays_blank` (the control — must stay passing)

Against `f963a94`:

```
test content::tests::last_content_row_sees_a_background_erased_row ... FAILED
  assertion `left == right` failed: a blue-erased screen is content down to the last row
    left: 0   right: 4
test content::tests::last_content_row_sees_a_background_erased_scroll_in ... FAILED
  assertion `left == right` failed: the red row scrolled in is content
    left: 2   right: 3
test content::tests::last_content_row_default_erase_stays_blank ... ok
```

Against the old loop (tamper: the `occ` skip removed, counter kept) — i.e. the branch-point
behaviour:

```
test content::tests::last_content_row_sees_a_background_erased_row ... ok
test content::tests::last_content_row_sees_a_background_erased_scroll_in ... ok
```

So this is a regression, not a pre-existing wart.

### User-visible symptom

`terminal_info().last_content_row` feeds
`crates/terminal-view/src/terminal_view/gutter_timestamps.rs:91`:

```rust
let content_row = info.cursor_row.max(info.last_content_row);
```

Rows painted by a background erase **below** the cursor no longer raise the stamping high-water
mark, so they render `[--:--:--]` in the gutter — the exact failure mode the packet named for H2
("otherwise lines below the cursor (TUI, progress bars using cursor-up…) show `[--:--:--]`").
`cursor_row.max(...)` masks it whenever the cursor is at or below the painted region, which is
why a casual smoke check does not show it.

### The `occ` semantics table this verification established

| row state | `occ` | visually blank? | old result | new result | agree? |
| --- | ---: | --- | --- | --- | --- |
| never written (unallocated slot) | 0 | yes | blank | blank (skipped) | yes |
| written, then `EL`/`ED` **with default bg** | 0 | yes | blank | blank (skipped) | yes |
| written, then `CSI 2 K` (`RowMut::fill`) | cols | yes | blank | blank (scanned) | yes |
| **erased with a non-default bg** (`reset`) | **0** | **no** | **content** | **blank (skipped)** | **NO — defect 1** |
| **scrolled in under a non-default bg** | **0** | **no** | **content** | **blank (skipped)** | **NO — defect 1** |
| styled spaces written by `set()` (e.g. underlined blanks) | > 0 | no (underline counts) | content | content | yes |
| wide-char glyph + `WideSpacer` | > 0 | no | content | content | yes |
| zero-width grapheme on a cell | > 0 | no | content | content | yes |
| `OSC 8` hyperlink on a space | > 0 | no | content | content | yes |
| graphic (Sixel) placement only | n/a | no | blank | blank | yes — pre-existing, `is_blank_cell` never looked at graphics; **not** a regression of this packet |

### Suggested minimal fix (not applied)

The engine already carries the missing bit: `reset` sets `flags = DIRTY | flags_for(template)`,
so a coloured erase leaves `RowFlags::STYLED` (and `HAS_EXTRAS` when the erase cell carries one)
set on the row. `RowRef::flags()` is already `pub`. Skipping only when
`occ == 0 && !flags.intersects(STYLED | HAS_EXTRAS | HAS_GRAPHEME)` keeps the whole win on the
idle screen (a fresh shell's blank rows carry no hints) and closes the hole. Hints
over-approximate in the safe direction, which is what a skip needs.

---

## 2. Defect 2 — stale URL underline when a wrapped URL's head scrolls off the top

**Severity:** Major, STALE-RENDER
**File:** `crates/terminal-view/src/render/plan_cache.rs:224-253` (`mark_scan_runs`) and
`:153-189` (phase 2)

### Why it is wrong

`mark_scan_runs` closes the dirty rows under wrap runs, and the induction behind it is: a row's
mask can only change if a row **in its wrap run** changed. That induction has one hole — the mask
of a row also depends on **where the viewport boundary cuts its run**, and the viewport boundary
moves without any row's `(RowId, SeqNo)` changing.

Concretely: a URL wraps from display row *r* onto *r+1*. Row *r+1*'s mask is `true` only because
pass 2 of `url_masks_rows_into` extended into it from row *r*. One more line of output scrolls
row *r* into history; row *r+1* becomes display row 0. Its content, its `RowId` and its `SeqNo`
are unchanged, so it is not dirty; no row in its (now truncated) run is dirty either. It keeps the
extended mask, and a full rescan of the same frame would give it an empty mask, because the prefix
that made it a URL is no longer on screen.

`wraps_prev` does not help: the run walk is `while start > 0 && connected(start - 1)`, and there is
no `connected(-1)`. The information that row 0 *used to have* a wrap-connected predecessor is not
in either flag array.

### Repro (tests added by this verification)

`crates/terminal-view/src/render/plan_cache.rs` — new tests, all comparing every row against a
from-scratch `url_masks_into` of the same frame (`assert_masks_match_a_full_rescan`):

```
url_v2_scrolling_the_viewport_keeps_the_masks_exact          FAILED
url_v2_streaming_output_keeps_the_masks_exact                FAILED
```

Raw failure (streaming, a 6x24 viewport, one wrapped URL among plain lines):

```
after line 4: row 0 mask differs from a full rescan
 got [true x21, false x3]
want [false x24]
```

and, scrolling a 5x24 viewport forward one row at a time:

```
scrolled forward 2: row 0 mask differs from a full rescan
 got [true x19, false x5]
want [false x24]
```

Both pass against the old whole-viewport shape (tamper: `mark_scan_runs` forced to
`scan.fill(true)`), so this is a regression introduced by this packet.

Direction of the error: always an **extra** underline (cells the full rescan calls not-a-URL), on
the continuation row of a URL whose head is above the viewport top. It persists as long as that
row stays non-dirty, which in scrollback is indefinitely.

Reproduced in the GUI as well — see leg (c′) below and
`US-0092-verify-d-stale-underline.png`: three rows underlined with no `https://` on screen.
Whether keeping that underline is *nicer* than dropping it is a product question this packet did
not open; what it is, is a divergence from the behaviour the packet promised to preserve, and an
inconsistent one — the same visible text underlines or not depending on which direction the user
scrolled to get there, and a later resize (which drops every key) silently erases it.

The underline and the click also disagree afterwards: Ctrl+click goes through
`crates/terminal-view/src/url/detect.rs` on a fresh `query_line_range_cells` window, which is an
independent computation, so the user can see an underline the click will not open.

### Root cause confirmed by probe

Adding two lines to `update()` — mark display row 0 dirty whenever
`RenderUpdate::Partial { scrolled } if scrolled != 0` — makes both failing tests pass and every
other `plan_cache` test except `scroll_shifts_plans_and_replans_only_scrolled_in_rows`, which
then sees `rows_planned == 2` instead of `1`. That last failure says the *correct* repair is to
put row 0 into `self.scan` (the URL rescan set) rather than into `self.dirty` (the replan set):
the existing mask-delta compare then only dirties row 0 if the mask really changed, so a scrolled
frame costs one extra rescanned row and no extra plan. The probe was reverted; no fix is applied
in this worktree.

### Invalidation matrix actually exercised

| change | in-viewport wrap connectivity | verdict | test |
| --- | --- | --- | --- |
| 3-row wrapped URL, **middle** row rewritten | run closure | pass | `url_pass_rescans_the_whole_wrap_run_of_a_changed_row` (implementer's) |
| 3-row wrapped URL, **first** row rewritten | run closure | pass | `url_v2_first_row_of_a_three_row_url_changes` |
| 3-row wrapped URL, **last** row rewritten | run closure | pass | `url_v2_last_row_of_a_three_row_url_changes` |
| a row **loses** `WRAPLINE`, continuation untouched | `wraps_prev` union | pass | `url_pass_rescans_a_continuation_row_whose_wrap_was_dropped` (implementer's) |
| same, but the wrap is dropped in a frame that is **never rendered** (two feeds, one render) | union is per-update, not per-feed | pass | `url_v2_wrap_dropped_in_a_frame_that_was_never_rendered` |
| `DL` deleting a row inside a wrapped URL | rows rehomed → all dirty | pass | `url_v2_delete_and_insert_line_inside_a_wrapped_url` |
| `IL` pushing a continuation row down | same | pass | same test |
| alt-screen enter / leave (`CSI ? 1049 h/l`) | row identity changes | pass | `url_v2_clear_screen_and_alt_screen_swap` |
| `CSI 2 J` | same | pass | same test |
| resize / reflow at 30 → 18 → 12 → 40 columns | `GridSize` change ⇒ `restyled` ⇒ every key dropped | pass | `url_v2_resize_rewraps_the_url` |
| font / style / device-cell change | `StyleKey` / `cell` change ⇒ `restyled` | pass | existing tests |
| viewport **scrolled back** (head becomes visible) | scrolled-in row is dirty, run walk reaches down | pass | `url_v2_scrolling_the_viewport_keeps_the_masks_exact` (back legs) |
| viewport **scrolled forward** / new output — head leaves the top | **not expressible** | **FAIL** | `url_v2_scrolling_...` (forward legs), `url_v2_streaming_output_keeps_the_masks_exact` |
| scrollback trim | `RowId` is monotonic, never reused | pass | `url_v2_streaming_output_keeps_the_masks_exact` (40 lines through a 6-row viewport) |

The cache is keyed on identity, not index: `keys[r]` holds `(RowId, SeqNo)` and `mask_prev[r]` is
only trusted when that key matched. That part is right — defect 2 is not an index-keying bug, it
is a missing dependency on the viewport boundary.

---

## 3. Note — the code comment's premise

`crates/terminal/src/content.rs:68-73` and
`IN-0029/low-level-design/damage-and-render-state.md:27-33` both describe the skip as
"a row the engine's `RowHeader.occ` hint says was never written". After a background erase the row
*was* written, and `occ` says 0 anyway. The quote is accurate; the paraphrase built on it is not,
and it is the paraphrase a future reader will trust. Worth correcting alongside defect 1.

---

## Counted-work guards — tamper results

Each tamper restored only the change-scoping hunk, keeping the counters, then the tests were
re-run and the tamper reverted.

| test | site | against `f963a94` | against the old shape | catches it? |
| --- | --- | --- | --- | --- |
| `last_content_row_cost_follows_the_content_not_the_viewport` | H2 | pass (2 / 2 cells) | **FAIL** `left: 1800, right: 3600` | yes |
| `last_content_row_pins_the_blank_definition` | H2 | pass | pass | no (correctness pin, not a guard) |
| `url_pass_scans_the_changed_rows_not_the_viewport` | H1 | pass (1 row) | **FAIL** `left: 45, right: 1` | yes |
| `url_pass_rescans_the_whole_wrap_run_of_a_changed_row` | H1 scope | pass (3 rows) | **FAIL** `left: 5, right: 3` | yes |
| `url_pass_rescans_a_continuation_row_whose_wrap_was_dropped` | H1 `wraps_prev` | pass (2 rows) | **FAIL** `left: 3, right: 2` | yes |

Both counted-work claims in the packet are therefore genuine. The `..occ` narrowing and the wrap
closure do what the packet says; the defects are in what they *exclude*, not in the counting.

## Cross-crate surface (`US-0090` conflict check)

`git diff 4e83f31..f963a94 | grep -n 'oneterm_vt::'` returns **no added code line** — the only
hits are one unchanged hunk-header context line (`fn is_blank_cell(cell: oneterm_vt::Cell, …)`)
and one line of packet prose. The new calls are `row.occ()` / `row.is_allocated()` on the
`RowRef` that `screen.row()` already returned; `RowRef` was already exported at the branch point
(`git show 4e83f31:crates/vt/src/lib.rs` → `pub use grid::{Pos, RowId, RowRef, SeqNo, …}`) and all
three methods were already `pub`. **No new crate edge, no new public item, no overlap with
`US-0090`'s narrowing beyond the three methods the packet's Handoff already lists.**

## ci-local.ps1

Run from this worktree against the pristine `f963a94` tree (the verification tests were set aside
for the run), `CARGO_BUILD_JOBS=3`:

```
ci-local: all checks passed.        (exit 0)
sections: 60  passed: 1940  failed: 0  ignored: 14
```

Identical to the implementer's recorded totals (60 / 1940 / 0 / 14).

## vt-bench tier 3 (non-regression guard only)

`cargo run -q -p oneterm-tools --release --bin vt-bench -- all --mib 2`, this worktree,
µs/frame at 7 200 cells. This does not measure either changed site — tier 3 is the engine's render
tier and both changes are above it.

| fixture | implementer "after" | this verification |
| --- | ---: | ---: |
| `plain_ascii` | 4.6 | 4.1 |
| `long_lines` | 2.3 | 2.5 |
| `heavy_sgr` | 1.3 | 1.3 |
| `tui_redraw` | 17.9 | 16.2 |
| `scroll_region` | 21.5 | 23.3 |
| `cjk_wide` | 1.7 | 1.5 |
| `dense_cells` | 1.7 | 1.5 |
| `scrolling` | 2.3 | 2.1 |
| `sixel` | 5.6 | 6.2 |
| `osc_9_7` | 5.6 | 6.8 |

All within the 1.2–22.6 µs/frame band the packet cites as the standing baseline, except
`scroll_region` which sits at 23.3 here and sat at 33.0 at the branch point on the implementer's
machine — i.e. still well below its own "before". Run-to-run spread across the two machines is
larger than any effect this packet could have, which is the point: it is a guard, not evidence.
**No regression.**

## GUI walk

**Process safety.** `Get-Process oneterm` enumerated **before** launching: pids `2504` and
`14804` (the owner's own windows, one of which runs their agent session). Launch was
`Start-Process -PassThru` with `-WorkingDirectory` set to this worktree; my pid was **15440**.
Only that pid was ever acted on — never by name, never by window title. Shutdown was
`CloseMainWindow()` first (it sufficed; the `Stop-Process` fallback did not fire). After the run
both `2504` and `14804` were re-checked and **ALIVE**; `15440` was gone. `target/terminal.json`
and `target/urls.txt` were written for the run and deleted afterwards (`target/` is gitignored).

Build: `cargo build -q -p oneterm-app --profile fast-dev`, exit 0; D: had 50.0 GB free.
Driver: `scratchpad/gui2.ps1` — the pid-targeted posted-message driver extended with
`wheel` (`WM_MOUSEWHEEL`, screen coords via `ClientToScreen`), `ctrlclick` and `move`
(`MoveWindow`). Captures via `PrintWindow(…, PW_RENDERFULLCONTENT)`.

| leg | result | evidence |
| --- | --- | --- |
| (a) 200 URL-bearing lines incl. a 221-char URL wrapping across a row boundary | **pass** — every URL underlined; the wrapped URL's underline is continuous across rows 198→199 and stops exactly at `tail-end`, with `after-the-wrapped-url` on the same row left plain. Gutter shows a real `[20:27:16]` on every visible line including the blanks below the prompt (H2's `[--:--:--]` failure mode absent for *default*-background blanks) | `US-0092-verify-a-url-heavy.png` |
| (b) wheel up 3 rows (`WM_MOUSEWHEEL` +120) and back | **pass** — scrolls, underlines intact through the `shift()` / `mask_prev` + `wraps_prev` rotation | `US-0092-verify-b-scrolled-up.png` |
| (c) window resized narrower (`MoveWindow` 1920×1032 → 1084×892 client) so the URL re-wraps | **pass** — the URL re-wrapped from 2 rows onto **5** (rows 394-398); the underline is continuous over all five and still stops exactly at `tail-end`. Every other URL re-wrapped onto its `ml` continuation row with the underline following. A `GridSize` change drops every key, so the whole viewport is re-derived — correct and confirmed | `US-0092-verify-c-narrow.png` |
| **(c′) defect 2, reproduced in the GUI** | **FAIL** — with the 5-row URL, wheeling **forward** until the viewport top lands mid-run (top = display row 396) leaves rows 396/397/398 fully underlined while **no `https://` prefix is anywhere on screen**: rows 394-395 are above the top edge. A whole-viewport rescan of that same frame underlines nothing there (this is exactly what `url_v2_scrolling_the_viewport_keeps_the_masks_exact` asserts, and what the tampered whole-viewport build produces). Approaching the same position by wheeling **backward** shows the underline correctly, because then the head row scrolls in dirty — so the same visible text underlines differently depending on scroll direction | `US-0092-verify-d-stale-underline.png` |
| (d) drag-select across the wrapped URL (posted `WM_LBUTTONDOWN` / `WM_MOUSEMOVE` × 12 / `WM_LBUTTONUP`) | **partial** — the drag reaches the app: a selection highlight rendered on the first attempt (at the viewport bottom) and all three drags fired copy-on-select, visible in the log as `gpui_windows::clipboard: Failed to open clipboard: Access is denied. (0x80070005)` × 3. The highlight did not render in the scrolled-back captures, so "selection and underline agree" could not be confirmed from a screenshot. The underline was unchanged by the selection in every capture | `US-0092-verify-e-drag-select.png` |
| (e) Ctrl+click the URL | **not deliverable — gap stands** — `WM_KEYDOWN VK_CONTROL` + `WM_LBUTTONDOWN` with `MK_CONTROL` in `wParam` posts cleanly but nothing happens: no browser process appeared (12 before, 12 after, no new pid) and the log shows no URL-open attempt. GPUI reads modifier state from the real keyboard (`GetKeyState`), which a posted message does not set, so a posted-message driver cannot deliver a modified click. Same class of limitation the implementer hit; it is **not** evidence about the change | `US-0092-verify-f-ctrlclick.png` |
| (f) full-screen TUI | not run — out of the time budget for this verification; the gap the implementer recorded still stands | — |

## Tests left in this worktree (uncommitted)

- `crates/terminal/src/content_tests.rs`
  - `last_content_row_sees_a_background_erased_row` (fails — defect 1)
  - `last_content_row_sees_a_background_erased_scroll_in` (fails — defect 1)
  - `last_content_row_default_erase_stays_blank` (passes — control)
- `crates/terminal-view/src/render/plan_cache.rs`
  - `assert_masks_match_a_full_rescan` (helper: every row vs a from-scratch `url_masks_into`)
  - `url_v2_first_row_of_a_three_row_url_changes` (passes)
  - `url_v2_last_row_of_a_three_row_url_changes` (passes)
  - `url_v2_scrolling_the_viewport_keeps_the_masks_exact` (**fails** — defect 2)
  - `url_v2_delete_and_insert_line_inside_a_wrapped_url` (passes)
  - `url_v2_clear_screen_and_alt_screen_swap` (passes)
  - `url_v2_resize_rewraps_the_url` (passes)
  - `url_v2_streaming_output_keeps_the_masks_exact` (**fails** — defect 2)
  - `url_v2_wrap_dropped_in_a_frame_that_was_never_rendered` (passes)

Final run with them present:

```
cargo test -p oneterm-terminal      --lib : 273 passed; 2 failed   (defect 1)
cargo test -p oneterm-terminal-view --lib : 297 passed; 2 failed   (defect 2)
```

Screenshots added under `evidence/`: `US-0092-verify-a-url-heavy.png`,
`-b-scrolled-up.png`, `-c-narrow.png`, `-d-stale-underline.png`, `-e-drag-select.png`,
`-f-ctrlclick.png`.

Nothing was committed and nothing was pushed.

## Commit trailers

All three commits carry both required lines:

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

`9112782`, `3fcc8fb`, `f963a94` — checked with `git log --format='%h%n%B' 4e83f31..f963a94`. OK.
