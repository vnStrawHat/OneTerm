# US-0085 — measurements

Date: 2026-09-13
Branch: `worktree-agent-ad731760c1d949455` off `feat/vt-engine` @ `d3c537b`
Machine: the owner's, session 1 **disconnected** throughout (see
[`US-0085-verify.md`](US-0085-verify.md) § GUI walks).

Everything below is recorded, never gated: `IN-0029.md` forbids a performance
number as a packet's exit criterion.

## 1. The flood table — the legacy-cell rebuild is gone

`cargo test -p oneterm-terminal --test us0081_parity flood_bench -- --ignored
--nocapture`: 4 MiB of coloured text through 4 KiB chunks, grid 120x30,
scrollback 10 000, one snapshot after every chunk (1025 frames). **Before** is
the same bench run from a `git checkout d3c537b` of this worktree, on this
machine, in the same session — not the number `migration.md` recorded on the
owner's box. Median of three runs each.

### `fast-dev` (`debug_assertions = true`)

| | before (`US-0082`) | after (`US-0085`) | delta | old engine |
| --- | ---: | ---: | ---: | ---: |
| `feed` | 102 ms | 103 ms | — | 57 ms |
| **snapshot** | **61 ms** | **36 ms** | **−25 ms** | 26 ms |
| total | 164.4 ms | 140.9 ms | −23.5 ms | 85.0 ms |
| ratio to the old engine | 1.93x | **1.62x** | | — |
| per frame, p95 | 64 us | **39 us** | | 29 us |

### `release` (`debug_assertions = false`)

| | before (`US-0082`) | after (`US-0085`) | delta | old engine |
| --- | ---: | ---: | ---: | ---: |
| `feed` | 58 ms | 58 ms | — | 37 ms |
| **snapshot** | **38 ms** | **12 ms** | **−26 ms** | 9 ms |
| total | 97.5 ms | 72.8 ms | −24.7 ms | 47.5 ms |
| ratio to the old engine | 2.05x | **1.51x** | | — |
| per frame, p95 | 40 us | **14 us** | | 9 us |

`migration.md`'s cost table gave this packet "**the legacy `Cell` rebuild that
remains** — about 9 ms — the only line item a later packet in this intake can
still delete". The measured deletion is larger, **25-26 ms**, because the shim's
`refresh_cells` did more than build `Cell`s: it also wrote every
`IndexedCell::point`, allocated a legacy `Hyperlink` per decorated cell, and
carried a dense 3 600-entry vector per frame. All of it is gone; what is left in
the snapshot column is `render_update` itself — 12 us per frame in release.

`feed` is unchanged, as it must be: this packet touched no engine path.

## 2. Frame time under sustained output

The app's own diagnostics log could not be read: it needs a presented window and
the session is disconnected. The stand-in is
`element_tests::frame_time_under_output`, a **real GPUI draw** in a headless
window over the same `FrameStats` counters the log line prints:

`cargo test -p oneterm-terminal-view --profile fast-dev frame_time_under_output
-- --ignored --nocapture`

1 MiB of scrolling coloured output in 4 KiB chunks, one drawn frame per chunk,
then sixty idle frames. Window grid 69 rows x 249 cols (the headless default),
`fast-dev`:

| | n | avg | p50 | p95 | max |
| --- | ---: | ---: | ---: | ---: | ---: |
| flood, prepaint + paint | 257 | 1 919 us | 1 884 us | 2 103 us | 4 272 us |
| idle, prepaint + paint | 60 | 241 us | 238 us | 296 us | 331 us |

- Under the flood the grid scrolls more than a screen per chunk, so most rows
  really do change: **13 868 of 17 733** viewport rows were planned. The cache is
  not hiding work here; it is reporting it.
- Every one of the sixty idle frames was `RenderUpdate::Unchanged`:
  `rows_candidate == 0`, `rows_planned == 0`, `url_scans == 0`, and
  `frames_unchanged` counted all sixty. The 238 us is paint alone — the tri-state
  skipping layout is what the test asserts, and the counters are the proof the
  packet's acceptance asks for.

**Gap:** no old-binary comparison for this number. It needs a presented window on
both binaries, which the disconnected session forbids; the engine-side half of
the same question is § 1, measured before and after on this machine.

## 3. The plan cache reuses unchanged rows

`plan_cache::tests`, all green:

| Test | What it pins |
| --- | --- |
| `plan_cache_first_frame_plans_every_row_and_idle_plans_none` | 10 rows planned on the first frame; an idle frame plans none, considers none, scans no URL and is counted `frames_unchanged` |
| `only_the_changed_row_replans` | one row rewritten: `rows_candidate == 1`, `rows_planned == 1` |
| `scroll_shifts_plans_and_replans_only_scrolled_in_rows` | a two-row scroll replans two rows of five and leaves the other three keys intact; a scroll of a whole screen rebuilds all five |
| `selection_change_does_not_replan` | a real selection over static rows: 0 candidates, 0 planned, 0 URL scans |
| `device_cell_size_change_replans_all`, `style_key_change_replans_all`, `weight_change_replans_all`, `resize_replans_all_and_resizes_session` | the four invalidations that are not row content |
| `url_mask_delta_replans_continuation_row` | a wrapped URL still replans its continuation row |
