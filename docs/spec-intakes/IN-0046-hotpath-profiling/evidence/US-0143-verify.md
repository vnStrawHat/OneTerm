# Verification: US-0143, the trimmed history row is reused for the next new row

Packet: [`US-0143-row-reuse.md`](../US-0143-row-reuse.md). Subject: `3529dc0d` on
`perf/vt-row-reuse` (one commit on main `7d8de84b`). Verified 2026-09-25 on Windows 11,
toolchain 1.96.0, the verifier worktree's own `target/`, `CARGO_BUILD_JOBS=2`.

## Verdict: PASS (documentation findings F1-F2, low; F3-F5 informational)

Every code claim holds. `Row::new_in` rewrites every field a `Row` has, the spare cannot leak
content across a width change, a screen switch, `ED 3` or a scrollback edit, the allocation
test fails on main with exactly 1,000 and passes on the change with 0, and the public surface
and package are unchanged. The findings are two stale sentences.

## 1. Correctness of recycling

`Row` has exactly two fields: `header: RowHeader { seq, id, flags, occ }` and
`cells: Vec<Cell>` (`crates/vt/src/grid/row.rs:57-79`). Per-cell data (style, hyperlink and
graphic through `ExtrasId`, grapheme ids) lives in the cells; there is no per-row timestamp,
gutter stamp or OSC 133 mark on `Row` (marks and placements are anchors keyed by `RowId`,
which the trim already releases through `Anchors::trim`).

| Field | `Row::new` | `Row::new_in(Some(spare))` |
| --- | --- | --- |
| `cells` | `vec![template; cols]` | `clear()` then `resize(cols, template)`: length exactly `cols`, every cell the template, whatever the spare's old width |
| `header.seq` | `seq` | `seq` |
| `header.id` | `id` | `id` |
| `header.flags` | `DIRTY \| flags_for(template)` | same expression (drops `WRAPPED`, `HAS_GRAPHIC`, `HAS_EXTRAS`, `STYLED`, `HAS_GRAPHEME`) |
| `header.occ` | `0` | `0` |

The only difference is the vector's capacity, which is at least `cols` either way.

- **Width change.** `clear` + `resize` handles a narrower spare (grows, one reallocation) and
  a wider one (keeps the larger capacity, `len == cols`); no cell beyond `cols` is reachable.
  Proven by mutation: deleting `row.cells.clear()` fails both the packet's recycle test and the
  adversarial test below.
- **Where the spare comes from and goes.** Only `push_rows_with` fills it
  (`screen.rs:572`); only `row_mut` (`:489`) and `blank_row` (`:536`) take it, both through
  `new_in`. `trim_history`, `clear_history`, `set_scrollback_limit` and the reflow never touch
  it, so a spare from an older width or from before `ED 3` can survive, but it is never read as
  content, so staleness is harmless (F3). Each `Screen` has its own spare, so the primary's
  can never enter the alternate screen or the reverse.
- **Graphics, hyperlinks, graphemes.** Nothing was released by the drop of a trimmed row:
  `graphics::placement::sweep` decides liveness from the placement's anchor plus
  `HAS_GRAPHIC` on rows reached through `Screen::row(id)`, and the trim already kills the
  anchor; interned extras and graphemes are not reference-counted by rows. The spare is
  unreachable from `row(id)`, `allocated_rows`, the integrity walk and the sweeps, so its old
  ids keep nothing alive and resurrect nothing.
- **`Screen` has no `Clone`/`PartialEq`/`Debug` derive**, so the spare cannot change an
  equality or a snapshot.
- `is_allocated`, `history_len`, `RowRef`: an empty-template new bottom row still goes
  through `blank_slot` to `None` (`screen.rs:594-596`), confirmed by the packet's test
  (`!is_allocated()` before the print).

## 2. Integrity

- `cargo test -p oneterm-vt --features vt-paranoid`: 564 lib tests pass (2 ignored),
  `scroll_allocations` passes, every integration test green.
- Adversarial test (local, not committed because it passes): a 4x20 screen with a 5-row
  history filled with styled, hyperlinked, graphic-covered, `WRAPPED` + `HAS_GRAPHIC` rows;
  resize to 7 columns, scroll, print; refill; resize to 5x33, scroll, print; enter the
  alternate screen, dirty and scroll it, print; leave; dirty more rows; `ED 3`; scroll, print;
  `set_scrollback_limit(2)`; resize to 3x11 under `KeepViewportTop`, scroll, print. After every
  step `assert_integrity(Some(&interner))` passes, and every printed line lands in a row whose
  length equals the current `cols`, whose other cells are `Cell::EMPTY` and whose flags are
  exactly `DIRTY`. With `row.cells.clear()` deleted from `new_in` it fails at the first print.

## 3. Allocation proof

| Grid sources | `scroll_allocations` |
| --- | --- |
| main `7d8de84b` (`row.rs`, `screen.rs` restored, test kept) | FAILED: "1000 scrolled lines over a full history made 1000 allocations" |
| `3529dc0d` | ok (0), default features and `vt-paranoid` |
| `3529dc0d`, lines prefixed with `ESC [44m` (the `blank_row` path, local edit) | ok (0) |

The counting `#[global_allocator]` is only in `crates/vt/tests/scroll_allocations.rs`, an
integration-test binary; `crates/vt/src` has no `GlobalAlloc` (grep). The file ships in the
package like every other test file and is never compiled into the library.

## 4. Measurements

The release hotpath flood was not re-run (it needs two release builds of `oneterm-app` and a
presented window on a machine that ran out of memory earlier today). The raw reports
support the table; spot checks from `research/raw/us0143-*.json`:

| Claim | JSON |
| --- | --- |
| `Parser::advance` avg 865 / 872 -> 660 / 630 ns | `time-before-1` 865 ns, `time-before-2` 872 ns, `time-after-1` 660 ns, `time-after-2` 630 ns |
| allocations 300,079 -> 10,115 | `count-before` total 300079 (p50 1), `count-after` total 10115 (p50 0) |
| bytes 203.8 MB (712 B p50) -> 6.9 MB (0 B p50) | `bytes-before` 203.8 MB, p50 712 B; `bytes-after` 6.9 MB, p50 0 B |

No report contains a `Screen::scroll_up` entry, which confirms the F5 site was removed for the
measurement. The average of the two after runs is 645 ns, -25.7 % against 868.5 ns.

`frame_time_under_output` (fast-dev, one run each, same worktree):

| Grid sources | flood avg / p50 / p95 | rows planned |
| --- | --- | --- |
| main `7d8de84b` | 1764 / 1700 / 2412 us | 13868 of 17733 |
| `3529dc0d` | 1599 / 1563 / 1853 us | 13868 of 17733 |

The test times `prepaint` + `paint` only, so the 9 % difference is noise in the change's
favour; the identical planned-row count shows the renderer saw the same damage.

## 5. Public contract

- `python scripts/vt-public-api.py --check --no-doc`: public API surface unchanged
  (`public-api.windows.txt`). `--diff-platforms`: the delta is 6 lines, all inside
  `oneterm_vt::pty`.
- `cargo build -p oneterm-vt --no-default-features --examples`: ok.
  `cargo test -p oneterm-vt --no-default-features`: 537 lib tests pass.
- `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features`: ok; the
  rustdoc citation grep (inside the gate) passes, the new comments cite nothing.
- `cargo package -p oneterm-vt --allow-dirty --list | python scripts/verify-dependency-graph.py --package-list -`:
  the package reaches nothing outside `crates/vt`.
- CHANGELOG: not owed. The crate's promise names "allocation behaviour, and performance" and
  "grid internals reachable through `grid::Screen`" as not promised, and an entry belongs there
  only when the compiled API or reply bytes change. An embedder that reads `heap_bytes` sees at
  most one extra row per screen (F1 covers the doc).

## Findings

### F1 (low, docs): `Screen::heap_bytes` rustdoc does not mention the spare

`crates/vt/src/grid/screen.rs:1767-1768` still says "the ring's slots, the rows that were
actually written, and the tab bitmap", while the body now adds the spare. It is a public
method's rustdoc; one clause ("and the one spare row a trim kept") fixes it.

### F2 (low, docs): two stale sentences in the grid LLD

`docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` line 213
("writing one allocates the row's `Vec<Cell>`") is now true only without a spare, and the
design table row G6 (line 501) still reads "no `zero` rotation and no free list". The packet
updated the Storage bullet at line 220 but not these two.

### F3 (info): the spare outlives the history it came from

It is kept across `ED 3`, `RIS`, a scrollback-limit change, a resize and leaving the
alternate screen: at most one row per screen (up to `MAX_COLS` x 8 B = 16 KiB), counted in
`heap_bytes`. After a narrowing, one buffer with the wider capacity keeps circulating. Not a
correctness issue; dropping it in `clear_history` would be a one-line tidy if anyone cares.

### F4 (info, tests): the recycle unit test covers `row_mut` only

The packet's unit test reaches `new_in` through the first print (`row_mut`). The
`blank_row` path (a coloured erase template) is covered here by the local `ESC [44m` variant
of the allocation test (0 allocations) and by the adversarial test's structure, not by a
committed test.

### F5 (info, records): packet environment line

The packet records `CARGO_BUILD_JOBS=3` and two other agents building; this verification
used 2 jobs. No effect on the figures checked.

## Gate

`pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=2`, after deleting the worktree's
`target/release`: exit 0, final line `ci-local: all checks passed.`

## Gaps

- The release hotpath flood was not re-run; the table is checked against its raw JSON only.
- The TUI load was not measured (as the packet says).
- No GUI walk: the change is inside the engine and the unit, integration and frame-time tests
  cover it.
