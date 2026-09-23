# Low-Level Design: Testing and benchmarking

Intake: IN-0029
HLD: ../high-level-design.md
Topic: testing-and-bench
Date: 2026-09-12

> One concern per file. The test strategy for `oneterm-vt`, the parity corpus, the differential
> runners, the fuzz targets and the benchmark harness, plus the master map from the 48-item trap
> list to the test that owns each item.

## Concern

How a rewrite of this size is kept honest. Five layers, each catching a different failure class:

| Layer | Catches |
| --- | --- |
| Byte-feed unit tests | "this sequence does the wrong thing" |
| The 45-recording parity gate | "we lost behaviour nobody can name" |
| Differential runners (raw `vte` state machine for the parser; old engine versus new for the whole) | "it is different and we did not notice" |
| Property tests and debug integrity assertions | "it is wrong on inputs nobody wrote a test for" |
| Fuzzing with a memory limit | "untrusted input can hurt us" |

Plus a benchmark that is **recorded, never gated**, because the engine is not a bottleneck at
realistic rates and a gate on it would be a flaky test measuring the machine.

## Design

### 1. Byte-feed unit tests as the spine

Every VT feature gets at least one test of the shape `crates/terminal/src/sixel_tests.rs` already
uses: feed real bytes through the real `Terminal`, then assert on cells, the cursor, modes or the
event batch.

```rust
let mut t = testing::terminal_from_text("");
let batch = testing::feed(&mut t, b"\x1b[3;5Hhello");
assert_eq!(testing::row_text(&t, t.viewport().top + 2), "    hello");
```

**Integrity-check budget (R-28).** `assert_integrity()` walks the screens and every interned id.
Called at the end of every mutating method over a 1200-row recording, 45 replays and 10 000 proptest
cases, an unbounded walk would turn `cargo test --workspace` — the CI gate, which runs in debug —
into a multi-minute job. Ghostty's equivalent is per *page*, not per grid. The rule, as shipped:

| Where | Check |
| --- | --- |
| Every mutating public method | an O(1) check: counters ordered, cursor inside the active screen, viewport offset within history |
| End of `feed`, `resize`, `snapshot_update`, in debug builds | the full invariant set, **bounded to `integrity_lo() = min(batch_lo, visible_top)`** clamped into the live range |
| Under `--features vt-paranoid` | `integrity_lo()` returns `oldest`, so every call is the whole-history, two-screen walk |
| Release builds | both tiers compile out; the `cfg!(debug_assertions)` guards are unchanged |

**Why that bound is sound.** `batch_lo` is the screen top as it was when the current batch opened,
written by `Screen::set_seq`, which `begin_batch` calls on both screens at the top of every `feed`.
Every row a batch can write, scroll or blank is at or above it: the print, erase, insert/delete and
scroll paths all address rows between the screen top and `newest`, and a row pushed into history
inside the batch was a screen row when it was written. `clear_history` and the trim only *drop*
rows, they never rewrite one. `visible_top` is in the bound so a scrolled-back viewport is still
checked. **No invariant is weakened — only the row range they are checked over.** The cost becomes
O(touched rows) and is flat in the scrollback depth.

**Measured** (debug, 160x45, a full 100 000-row history, ten calls each):

| | `feed` (one line) | `snapshot_update` |
| --- | --- | --- |
| whole-history walk (`--features vt-paranoid`) | 258 615 us | 252 772 us |
| bounded (default) | 143 us | 150 us |

The residual ~150 us is the floor the bound describes — two 45-row screens, each cell visited by
both walkers.

**CI runs both tiers.** `scripts/ci-local.ps1`, `scripts/ci-local.sh` and the `vt-package` job in
`.github/workflows/ci.yml` carry an extra step,
`cargo test -p oneterm-vt --features vt-paranoid`, so the unbounded invariants still gate every
change. **M12 is closed**: the manifest entry landed with `US-0076` and the `cfg!` gate and CI step
with the `US-0075` / `US-0079` rework. The step used to run in both quality jobs; `US-0109`
moved it to the one crate-centric job, because the walk is grid logic with no
platform-conditional site and the engine's only such module, `src/pty/`, is already compiled
and run per platform by `cargo test --workspace`.

**The guarding test** is `snapshot::bench::integrity_walk_cost_per_feed_and_snapshot_update`
(`crates/vt/src/snapshot/snapshot_bench.rs`). It runs one probe loop **twice** in the same process
— once over a full 100 000-row history and once over a near-empty one, same geometry, same
scrollback limit, same screen content — reports the per-`feed` and per-`snapshot_update` cost of
each, and when the feature is off **asserts the ratio between them, `< 20x`**, plus a loose
absolute ceiling of 20 ms as a backstop. Each number is the *cheapest* of ten calls, not their
mean. Debug only — the walk does not exist in a release build.

The ratio, not a wall clock, is what carries R-28: the property is "flat in the scrollback depth",
the two probes differ in nothing but that depth, and the runner's speed and load cancel out of
their quotient. The bounded walk puts the ratio at about 1 and an O(history) walk puts it at about
1700, so the bound sits more than an order of magnitude from either population. **`BUG-0075`**
(`../BUG-0075-integrity-walk-bench-measures-the-runner.md`) replaced the earlier form — a 1 ms
ceiling on the *mean* of ten calls — which failed on a loaded CI runner at 1382.4 us with the walk
perfectly well bounded: a mean carries whatever the scheduler did to the thread, and 1 ms was 7x
above the honest cost, not the three orders of magnitude the margin was described as.

### 2. The parity corpus

**Source.** alacritty's 45 ref tests, Apache-2.0. They are real `tmux` / `vim` / `zsh` / `fish`
captures plus targeted regressions, and replaying them pins behavioural parity with the engine
being replaced — the cheapest safety net this rewrite can have.

**What is vendored.** `crates/tools/corpus/alacritty-ref/<name>/` with `recording` (raw bytes),
`size.json` and `config.json` — about **956 KB** — plus the two expectation files below. The
upstream `grid.json` files (46 MB) are **not** committed; they are used once, as the cross-check
oracle described below.

Attribution: `crates/tools/corpus/NOTICE` carries the Apache-2.0 notice and the source
revision; `THIRD-PARTY-NOTICES.md` gains a corpus row as the two fork rows are removed.

**Two expectation files per recording, both generated by the OLD engine at `US-0072`
(R-57, R-58).**

1. `grid.expect` — **cell-exact, nothing trimmed**. Per row: the wrap flag, then a run-length
   encoding of `(content, width class, fg, bg, attrs, underline colour, hyperlink id presence)`
   over **every** column including trailing blanks. Per grid: the row count and the viewport
   position expressed as distance-from-newest, which both engines can produce. This is upstream's
   `Grid::eq` comparison — `c`, `fg`, `bg`, `flags` including `WRAPLINE`, and `extra`, plus
   `columns`, `lines` and `display_offset` — kept as-is, in a form that survives the fork's
   deletion and compresses the 46 MB of JSON to a few megabytes without losing a cell.
   Run-length encoding is lossless; trimming is what the earlier draft did and is exactly what
   `vim_24bitcolors_bce` exists to catch.
2. `state.expect` — the OneTerm snapshot the upstream harness never checked
   ([`../research/engine-semantics.md`](../research/engine-semantics.md) § 7.3, trap 44): cursor
   position and shape, the pending-wrap flag, every non-default mode, the colour-override table,
   the **current title**, the tab stops, and the scroll region.
   **Gap, as built in `US-0072`:** the engine being replaced exposes no title *stack*, only the
   current title, so `state.expect` cannot carry the stack and `CSI 22 t` / `CSI 23 t` depth is
   unverified by the corpus. It is covered by `dispatch::tests::title_stack_caps_at_sixteen_dropping_the_oldest`
   instead, and a later packet may add the field once the new engine is the blessing engine.

**Per-recording expected differences (owner ruling, 2026-09-12).** The engine is correctness-first,
so some recordings will legitimately differ from expectations blessed by the engine being replaced.
That is declared per recording and **per cell**, never as a skipped recording:

```json
// crates/tools/corpus/alacritty-ref/delete_chars_reset/expected-diffs.json
// JSON rather than TOML: `serde_json` is already in the graph, `toml` is on the
// do-not-re-add list in docs/agents/dependencies.md section 3 (US-0072 verify, F1).
{ "diff": [ {
  "deviation": "C1",
  "rows": "4",
  "cols": "70..80",
  "fields": ["content", "attrs"],
  "reason": "DCH is a plain shift left; the reference clamps end to cols - 1"
} ] }
```

Harness rules, and they are what keep the gate a gate:

- A difference **inside** a declared window, in a declared field, passes. Anything else fails —
  a different row, a different column, a different field, or a difference in a recording with no
  `expected-diffs.json`.
- A declared window that produces **no** difference also fails (`stale declared diff`), so a
  correction that is later re-implemented as parity cannot leave a permanent hole.
- Every `deviation` id must resolve to a row in the corrections tables of
  [`grid-and-scrollback.md`](grid-and-scrollback.md) or
  [`dispatch-and-modes.md`](dispatch-and-modes.md); an unknown id fails.
- `state.expect` takes the same mechanism for its own fields (palette after `RIS` for C6, for
  example).
- The files are written by `US-0072`'s scripted grep where it can prove the window, and by the
  packet that implements the correction otherwise; either way the diff and the reason land in the
  commit that changes the behaviour.

**Who blesses (R-58): nothing does, any more.** `US-0072` generated both files **with the old
engine**; they were reviewed once, committed and **frozen**. The new engine never blesses — a gate
whose expectations its own subject may rewrite proves self-consistency and nothing else. While the
fork existed the rule was enforced by `vt-corpus bless` refusing to write without a
`--deviation <row-id>` naming a deviation row. **`US-0087` deleted the subcommand along with the
fork**, and that is the stronger form of the same rule: with no old engine there is no engine that
*may* bless, so keeping a writer would have meant shipping one with nothing behind it. The 46
frozen expectations are now read-only artefacts; a genuine future change to one is a reviewed,
hand-authored diff carrying its reason, never a tool run.

**The cross-check (R-61) ran once and is gone.** `US-0072` ran
`vt-corpus cross-check --grid-json <dir>` against upstream's `grid.json` set — copied out of the
cargo checkout into a scratch directory with its SHA-256 recorded in the packet — converted it to
`grid.expect` form and diffed it against the old engine's output, proving the expectation format
loses nothing. That was its whole purpose; `US-0087` removed the subcommand with the rest of the
old-engine paths.

**Our own recordings** under `crates/tools/corpus/oneterm/`, covering the surfaces with zero
upstream coverage and the most new code (R-59):

| Recording | Covers |
| --- | --- |
| `sixel_basic` | the IN-0028 decoder, placement and release. **Shipped by `US-0080`** at `crates/tools/corpus/oneterm/sixel_basic/`, outside `alacritty-ref/`, blessed by the **old** engine like every other expectation and checked by the same `vt-corpus check` and `vt-diff` runs |
| `sixel_scroll` | an image scrolling into history and being trimmed |
| `osc_9_7_agent` | the agent channel end to end |
| `osc_133_prompt` | shell integration marks |
| `conpty_resize` | the BUG-0051 capture |
| `cjk_emoji_wrap` | wide characters at the wrap boundary |
| `palette_osc` | OSC 4 / 10 / 11 / 12 / 104 / 110-112, set and query |
| `tabs_and_regions` | `HTS`, `TBC`, `DECSTBM`, `IL`/`DL` inside a region (`vttest` menus 1 and 3, captured once) |
| `hyperlinks_shared_id` | OSC 8 with and without `id=`, two links sharing an id |
| `queries_da_decrqm` | DA1, DA2, DSR, DECRQM over the whole mode table |
| `mouse_mode_transitions` | 1000 / 1002 / 1003 / 1005 / 1006 set and unset, including the exclusivity asymmetry |
| `captured_session` | a real OneTerm session |

Every one of these has both expectation files, generated the same way — except the four that
exercise behaviour the old engine does not have (`osc_9_7_agent` is fine, it only forwards;
`queries_da_decrqm` differs by the DA1 answer), which are marked in the packet with the field the
old engine cannot produce.

### 3. Differential runners

**Parser versus the raw `vte` state machine** (`crates/vt/tests/differential.rs`). Full rationale
and the patch-scope verification are in [`parser.md`](parser.md) § "The differential oracle": the
two vendored `vte` patches touch only `src/ansi.rs`, so `vte::Parser` + `vte::Perform` are
pristine even under `[patch]`, and the oracle needs no second copy of the crate.

**Old engine versus new** (`crates/tools/src/bin/vt-diff.rs`, alive from `US-0072` to `US-0087`,
where it was deleted with the engine it compared against).
Feeds the same bytes to the vendored `Term` and to `oneterm-vt` and compares in the **`grid.expect`
form**, not in a trimmed text form (R-43). That matters: the trimmed form could not see BCE
backgrounds on erased trailing cells, per-cell attributes, wrap flags, or the viewport position —
which is most of what a reimplementation gets wrong. It reports the first differing row and column
with 40 bytes of input context.

### 4. Property tests

`proptest` (already in `Cargo.lock`). Concentrated where the scars are: reflow (seven properties
in [`reflow-and-resize.md`](reflow-and-resize.md)), plus
`grid::props::scroll_and_erase_preserve_integrity` and
`parser::props::arbitrary_bytes_never_panic_and_chunking_is_invariant`.

### 5. Fuzzing — Linux only, never a packet gate (R-47, R-60)

`cargo-fuzz` brings `libfuzzer-sys` and `arbitrary` in a nested `fuzz/` crate, requires a nightly
toolchain against a pinned `rust-toolchain.toml`, and **libFuzzer is not usable on
`x86_64-pc-windows-msvc`** — the project's primary and only QA-tested platform.

| Decision | Value |
| --- | --- |
| Host | Linux CI (or WSL locally); never Windows |
| Toolchain | a recorded nightly exception, documented in `docs/agents/dependencies.md` § 3 alongside the new declarations |
| Schedule | a scheduled job, not a pull-request gate and **not** a packet exit criterion |
| Targets | `parser`, `terminal`, `sixel`, each with `-rss_limit_mb=512` |
| Corpora | the 45 recordings plus Ghostty's MIT-licensed AFL++ seeds |
| Findings | each becomes a unit test with the minimised input under `crates/vt/tests/regressions/` |

The memory limit is the point: the defects being designed out are memory-exhaustion defects.

### 6. Conformance as a report, not a gate

`esctest` is GPL-2.0 and cannot run on Windows; `vttest` is menu-driven. Neither is vendored. A
Linux CI job runs them against a headless harness and publishes a pass/fail matrix as an artifact;
it never fails the build. The parts of `vttest` that matter are frozen into the corpus already.

### 7. Benchmarks

Promoted from the scratch harness described in
[`../research/perf-baseline.md`](../research/perf-baseline.md) § 1 into
`crates/tools/src/bin/vt-bench.rs`.

**Fixtures** are generated once, offline, by `crates/tools/src/bin/vt-fixtures.rs` at a fixed
geometry (160x45, matching `pty-throughput`), ported from vtebench's generators plus OneTerm's own
(Sixel, OSC 9;7, CJK and emoji, SGR churn, a captured session). vtebench itself is unusable: its
benchmarks are POSIX shell scripts needing `ps -o tty=`, `/dev/<tty>` and `tput`.

**Tiers:**

| Tier | What | Geometry | Why |
| --- | --- | --- | --- |
| 1 | parser only (a no-op `Dispatch`) | 160x45 | isolates the state machine |
| 2 | parse + grid (`Terminal::feed`) | 160x45 | the real cost centre: the grid half costs 3-8x the parser half |
| 3 | parse + grid + one `snapshot_update` + `map_colors` per simulated frame | 160x45 | **the primary metric**; the tier that would have caught the per-frame viewport copy |
| 4 | resize latency at 0 / 10 000 / 100 000 rows of scrollback, 80x24 to 100x40 | its own geometry, deliberately (R-63) | this tier measures an operation, not a stream; it is never compared against tiers 1-3, only against the other engine at the same depth |
| 5 | Live heap after filling N scrollback rows with plain / unicode / heavily styled / mixed content | 160x45 | the memory claim is a design property until this measures it |

**Tier 5 is a `vt-bench rss` report, not a `#[test]` (R-60):** a memory assertion inside a unit
test is unreliable under a harness running other tests in parallel. The unit test
`grid::tests::unwritten_slots_read_as_blanks` asserts the *logical* property (unwritten slots read
as blanks and allocate no row); the *physical* property is the bench's job.

**What `US-0072` built, and the gap:** the tier measures **live heap** through a counting
allocator, not process RSS. Live heap is the number the design's claims are about (bytes per cell,
lazily allocated slots) and it is portable and deterministic, where RSS on Windows moves with the
allocator's retained arenas and with anything else in the process. Process RSS is therefore **not**
measured today; if the retained-memory question ever matters, a `vt-bench rss --process` mode is
the follow-up. The tier keeps the name `rss` for continuity with the packet that created it.

**Reporting rule.** Every engine number is printed next to the ConPTY transport ceiling from
`pty-throughput` (about 1.2 MiB/s for a `cmd.exe` producer, about 30 MiB/s for a DOOM-fire-class
one), so no number is read in isolation. **No numeric target appears in any packet's exit
criteria** (R-29): the phase plan records ratios against the old engine, never an absolute
microsecond figure taken from a vendor's blog.

**Hygiene:** never pipe the generator into the benchmark, compare two renamed binaries with
`hyperfine`, never run benchmarks in parallel.

**CI wiring (R-46, R-60).** `cargo test --workspace` picks up the new crates automatically. The
Windows bench job does not exist in `.github/workflows/ci.yml` today; **`US-0072` owns creating
it**, together with the `verify-dependency-graph.py` allow-list entry and the
`docs/agents/dependencies.md` § 3 rows for the new declarations — every doc and CI edit lands in
the packet that adds the code it describes, not nine packets later. The job records and never
gates.

### 8. Deviation grep (R-53)

`US-0072` runs one scripted pass over the 45 recordings for every sequence a correction or a
deferred feature touches — `?47`, `?1047`, `?1048`, `?5W`, `!p`, `?6n`, `>0q`, `?45`, `>4;m`,
SGR 5 / 6 / 53, `CSI P` with a large count, `CSI 1 J`, `OSC 4` with an even count, `RIS` after a
palette change — and fills in the "Affected recordings" column of the corrections tables in
[`dispatch-and-modes.md`](dispatch-and-modes.md) and
[`grid-and-scrollback.md`](grid-and-scrollback.md) with a measured answer. Until then those cells
read "measure in `US-0072`", never "none".

## Trap list — master map

All 48 items from [`../research/engine-semantics.md`](../research/engine-semantics.md) § 8.

| # | Trap | Owning LLD | Test |
| --- | --- | --- | --- |
| 1 | Pending wrap then `BS`; no reverse wrap at column 0 | grid | `grid::tests::pending_wrap_then_backspace`, `grid::tests::backspace_at_column_zero_is_a_noop` |
| 2 | Pending wrap then `EL 0` erases nothing | grid | `grid::tests::pending_wrap_then_el0_erases_nothing` |
| 3 | Pending wrap then `HT` | grid | `grid::tests::pending_wrap_then_tab_wraps_and_returns` |
| 4 | Pending wrap with `DECAWM` off | grid (G3) | `grid::tests::decawm_off_then_el0_erases`, `grid::tests::decawm_off_then_tab_moves_to_the_next_stop` |
| 5 | Wide char at the last column | grid (print path) | `grid::tests::wide_char_at_last_column_wrap_on_and_off` (`US-0075`) |
| 6 | Overwriting half a wide pair | cell-and-style (same row) + grid (cross row) | `cell::tests::wide_pair_repair_on_overwrite`, `grid::tests::wide_pair_repair_across_rows` (`US-0075`) |
| 7 | Insert mode over a wide char | grid (print path) | `grid::tests::insert_mode_over_wide_char_repairs_the_pair` (`US-0075`, correction C4 — the old name `..._leaves_orphan_spacer` described the behaviour the correction replaced) |
| 8 | Zero-width char at column 0 | grid (print path) | `grid::tests::zero_width_at_column_zero_attaches_to_column_zero` (`US-0075`) |
| 9 | `ED 2` scrolls the occupied viewport into scrollback and **bumps the scroll offset** like any other push, so a scrolled-back view keeps its content | grid | `grid::tests::ed2_scrolls_the_viewport_into_history`, `grid::tests::ed2_bumps_the_scroll_offset` |
| 10 | `ED 3` resets the viewport offset | grid | `grid::tests::ed3_resets_the_viewport_to_the_bottom` |
| 11 | `ED 1` skips row 0 when the cursor is on row 1 | grid | `grid::tests::ed1_with_cursor_on_row_one_keeps_row_zero` |
| 12 | `ScreenCleared` scope and ordering | dispatch | `dispatch::tests::screen_cleared_fires_only_for_ed2_ed3_and_ris`, `dispatch::tests::ed3_with_no_history_still_fires_screen_cleared` |
| 13 | `? 47` / `? 1047` / `? 1048` ignored | grid (correction C8, shipped at `US-0076`) | `grid::tests::alt_screen_47_and_1047_and_1048` |
| 14 | `? 1049 h` clobbers the primary DECSC slot | grid | `grid::tests::entering_alt_screen_overwrites_the_saved_cursor`, `grid::tests::leaving_alt_screen_restores_the_entry_cursor` |
| 15 | Invalid `DECSTBM` is a no-op; valid always homes | grid | `grid::tests::decstbm_invalid_range_is_a_noop_and_valid_homes_the_cursor` |
| 16 | Cursor outside the scroll region | grid | `grid::tests::linefeed_below_the_region_does_not_scroll`, `grid::tests::insert_and_delete_lines_outside_the_region_are_noops` |
| 17 | Scrollback fills only from a region starting at row 0 | grid | `grid::tests::scroll_up_fills_history_only_from_row_zero`, `grid::tests::scroll_up_with_a_bottom_bounded_region_keeps_the_rows_below`, `grid::tests::small_region_scroll_blanks_without_rotating` |
| 18 | `SD` never touches history | grid | `grid::tests::scroll_down_never_pulls_from_history` |
| 19 | `DCH` clamps `end` to `cols - 1` | grid | `grid::tests::delete_chars_clamps_end_to_last_column` |
| 20 | `SGR 38;5` versus `38:5` | dispatch (+ parser) | `dispatch::tests::sgr_semicolon_and_colon_colour_forms`, `parser::tests::params_defaults_and_separators` |
| 21 | `SGR 4:x` styles; `SGR 21` is cancel-bold | dispatch | `dispatch::tests::sgr_21_is_cancel_bold_and_underline_styles_are_exclusive` |
| 22 | A parameter of `0` means default | dispatch | `dispatch::tests::param_zero_means_default` |
| 23 | CSI ignore versus CSI overflow | parser | `parser::tests::private_marker_in_csi_param_dispatches_nothing`, `parser::tests::param_overflow_dispatches_with_ignore` |
| 24 | OSC terminators; unbounded buffer | parser | `parser::tests::osc_terminators_bel_and_st`, `parser::tests::osc_keeps_c1_st_as_payload`, `parser::tests::osc_truncates_at_inline_cap_and_still_dispatches` |
| 25 | OSC 52 selection byte and decode failures | dispatch | `dispatch::tests::osc_52_selection_byte_validation` |
| 26 | OSC 4 odd parameter count; OSC 104 scope | dispatch | `dispatch::tests::osc_4_requires_an_odd_parameter_count`, `dispatch::tests::osc_104_does_not_reset_the_special_colours` |
| 27 | Tab stops after a resize; `CSI ? 5 W` | grid (G5 for the second half) | `grid::tests::tab_stops_survive_and_regrow_across_a_resize`, `grid::tests::csi_5_w_restores_default_tab_stops` |
| 28 | Resize destroys the region and sometimes the selection | reflow | `reflow::tests::resize_resets_the_scroll_region_and_clears_the_selection` |
| 29 | The alternate screen never reflows | reflow | `reflow::tests::alt_screen_grow_and_shrink_do_not_reflow` |
| 30 | Resize with the viewport scrolled back | reflow | `reflow::tests::first_visible_character_survives_a_resize`, `reflow::tests::sticky_bottom_survives_a_resize` |
| 31 | Reflow re-arms the pending wrap | reflow | `reflow::tests::pending_wrap_is_rearmed_only_without_the_wrapped_flag` |
| 32 | Shrinking columns truncates history | reflow | `reflow::tests::shrink_columns_truncates_the_oldest_history` |
| 33 | Reflow inserts and removes leading wide spacers | reflow | `reflow::tests::wide_char_at_the_new_last_column_moves_to_the_next_row`, `reflow::props::wide_pairs_are_never_split` |
| 34 | Insert mode forces full damage | damage (D2) | `render::tests::insert_mode_does_not_force_full_damage` |
| 35 | Damage indices shift with the scroll offset | damage | `render::tests::changes_while_scrolled_back_are_not_copied` |
| 36 | `Row::reset` clears only up to `occ` | grid | `grid::tests::row_reset_respects_occ_and_the_background_template` |
| 37 | `is_empty` ignores several attributes | cell-and-style | `cell::tests::blank_and_erasable_predicates_differ_on_a_bold_space` |
| 38 | CPR ignores origin mode | dispatch | `dispatch::tests::cpr_ignores_origin_mode` |
| 39 | RIS keeps the colour palette | dispatch | `dispatch::tests::ris_keeps_the_palette_and_the_row_ids` |
| 40 | `DECCOLM` does not change the column count | dispatch | `dispatch::tests::deccolm_does_not_change_the_width` |
| 41 | Sync updates: exact eight bytes, host must poll | parser + damage (D4) | `parser::tests::sync_mode_with_leading_param_is_recognised`, `sync::tests::mode_2026_watchdog_forces_the_mode_off` |
| 42 | Kitty: query reads the stack; push overflows the wrong stack | dispatch (D15) | `dispatch::tests::kitty_query_reads_the_stack_top`, `dispatch::tests::kitty_pop_beyond_len_resets_the_stack` |
| 43 | `REP` replays through the print path | dispatch | `dispatch::tests::rep_replays_through_the_print_path` |
| 44 | Grid equality ignores cursor, modes, colours | testing-and-bench | `corpus::tests::state_expect_covers_cursor_modes_palette_title_and_tabs` |
| 45 | `Storage::PartialEq` asserts `zero == 0` | testing-and-bench (G6) | `corpus::tests::expectations_compare_content_not_storage_layout` |
| 46 | `display_iter` advances before yielding | grid | `grid::tests::first_visible_cell_is_viewport_top_column_zero` |
| 47 | `ESC \` in an OSC versus a DCS | parser | `parser::tests::dcs_exit_via_esc_resets_intermediates`, `parser::tests::osc_terminators_bel_and_st` |
| 48 | 8-bit C1 is not an introducer | parser | `parser::tests::c1_is_executed_not_an_introducer` |

## Interfaces

```rust
// crates/tools/src/bin/vt-corpus.rs
//   vt-corpus replay [--engine new|old] [--filter <name>]
//   vt-corpus bless  --engine old [--deviation <row-id>] [--filter <name>]
//   vt-corpus cross-check --grid-json <dir>       // US-0072 only, path given explicitly
//   vt-corpus grep-deviations                     // US-0072, fills the Recording risk columns

// crates/tools/src/bin/vt-diff.rs      // old vs new, compares in grid.expect form
// crates/tools/src/bin/vt-fixtures.rs  // offline fixture generation
// crates/tools/src/bin/vt-bench.rs     // parse|grid|render|resize|rss

// crates/vt/tests/ref_corpus.rs        // the gate: one #[test] per recording, both files
// crates/vt/tests/differential.rs      // parser vs the raw vte state machine
// crates/vt/fuzz/fuzz_targets/{parser,terminal,sixel}.rs   // Linux only
```

## Edge Cases and Failure Modes

- [ ] **A recording that legitimately differs** — it carries an `expected-diffs.json` entry naming
  the correction, the cells and the fields. There is no whole-recording skip and no silent
  `#[ignore]`, and a declared window that stops differing fails the gate. **No recording needs one
  today**; the first that does is a signal to re-read the correction, not to bless.
- [ ] **`bless` used to hide a regression** — refused without `--deviation <row-id>`; the
  expectation diff and the named row are both in the commit.
- [ ] **The differential runner diverging on a captured session** — it prints the first differing
  row, column and the 40 bytes of input that produced it.
- [ ] **Benchmark noise** — three-run median, fixed geometry, offline fixtures, no parallel runs,
  no threshold.
- [ ] **The corpus growing the repository** — recordings (956 KB) plus run-length expectations;
  the 46 MB of `grid.json` is never committed.
- [ ] **A test that needs a PTY** — none do; everything in `oneterm-vt` is driven by bytes, so the
  whole engine is testable on Linux CI as well as Windows.
- [ ] **The debug suite getting slow** — the integrity budget above, re-measured at `US-0077`.

## Verification

- [ ] `US-0072`: `vt-corpus replay --engine old` green on all 45 recordings against both
  expectation files; `vt-corpus cross-check` shows no loss against upstream `grid.json`;
  `vt-corpus grep-deviations` has filled every "measure in `US-0072`" cell; `vt-bench` reports all
  five tiers for the old engine; `vt-diff` old-against-old reports no difference (a self-test of
  the comparator); the Windows bench job exists in CI and records without gating.
- [ ] `US-0073`: `cargo test -p oneterm-vt --test differential` green, including
  `oracle_precondition_patches_do_not_touch_the_state_machine`.
- [x] `US-0076`: **met — 45 of 45 green with no `expected-diffs.json` file at all**
  (`evidence/US-0076-parity-gate.md`). Every correction the tables predicted would need a declared
  window is measured free, C9 included. `vt-diff` old-against-new is identical on all 45 recordings
  **and** on all 10 `vt-bench` fixtures. The declared-window mechanism stays for corrections that
  land later, and the three families that would diverge if a stream exercised them are listed in
  [`dispatch-and-modes.md`](dispatch-and-modes.md) § "By-design differential divergences" so a
  future verifier does not re-derive them from a red diff.
- [ ] `US-0077`: `reflow::props` green over 10 000 cases; the debug-suite runtime budget
  re-measured.
- [ ] `US-0081`-`US-0085`: `vt-diff` green over all recordings plus the captured session at the
  shim flip and at each consumer-group swap.
- [x] `US-0087`: **met** (`c8d84ff`) — the fork and the old-engine paths are deleted, and with them
  `vt-diff`, the `vte` dev-dependency and `tests/differential.rs`, `vt-corpus bless` and the
  cross-check. What remains of `vt-corpus` is the read-only `check` that runs the gate.
