# US-0085 — verification

Date: 2026-09-13
Branch: `worktree-agent-ad731760c1d949455`, off `feat/vt-engine` @ `d3c537b`
(the worktree tool based it on `main` @ `c936ac0`, which has no `crates/vt`; a
`git reset --hard d3c537b` was run before any file was read or written).

Measurements live in [`US-0085-measurements.md`](US-0085-measurements.md).

## 1. The deletion gate

```text
grep -rn "alacritty_terminal" crates/terminal/src crates/terminal/Cargo.toml crates/terminal-view
crates/terminal/src/backend/osc_router.rs:305  (a comment citing the fork's line numbers)
crates/terminal/src/content.rs:13              (a comment: what US-0085 deleted)
crates/terminal/src/handle.rs:9                (a comment: why parking_lot, not the fork's FairMutex)
crates/terminal/src/lib.rs:9                   (a comment: what is left, and where)
crates/terminal/Cargo.toml:34                  [dev-dependencies], for tests/us0081_parity.rs
```

No **code** in either crate names the fork. The manifest line moved to
`[dev-dependencies]` because `tests/us0081_parity.rs` — the old-versus-new
differential — is the one thing that still has to link the engine being
replaced; `migration.md` retires both at `US-0087`.

`crates/terminal/src/engine_shim.rs` does not exist.

## 2. Test results

```text
cargo test --workspace                                  green (exit 0)
cargo test -p oneterm-terminal                          245 passed, 0 failed
cargo test -p oneterm-terminal --test us0081_parity       5 passed, 2 ignored
cargo test -p oneterm-terminal-view                     288 passed, 0 failed, 3 ignored
cargo clippy --workspace --all-targets -- -D warnings    green
cargo fmt --all -- --check                              green
```

`us0081_parity` is the load-bearing one: the same 81 byte streams, the same
field-by-field snapshot diff, the same five-difference allow-list. It now
derives the legacy shape from `TerminalContent`'s native accessors — the
conversion `US-0085` deleted from the product, moved into the test whole — so a
green run says the **native** frame path produces exactly the frame the shim
produced, which is exactly the frame the old engine produced.

### Tests rewritten, and what pins them now

`crates/terminal`:

| Test | Was | Is |
| --- | --- | --- |
| `content_tests::snapshot_has_cells_and_bounds` | dense `cells` + `terminal_bounds` | **deleted**; `the_frame_source_is_the_render_state` asserts `size()`, `rows()` and the row text |
| `content_tests::damage_full_on_first_snapshot` | `damage == Full` | `the_first_update_is_full`: `update() == RenderUpdate::Full` |
| `content_tests::damage_partial_on_unchanged` | "only the cursor line may be dirty" | `an_unchanged_frame_copies_nothing`: `Unchanged`, empty `changed()`, full viewport still in `rows()` |
| `content_tests::only_the_changed_rows_are_rebuilt_when_the_viewport_stood_still` | the compatibility vector's cells and points | `..._are_copied_...`: one index in `changed()`, the other rows' text intact, **and an untouched row keeps its `SeqNo`** — the plan cache's key |
| `content_tests::a_scrollback_move_rebuilds_every_point` | every `point.line` rebuilt | `a_scrollback_move_reports_a_delta_and_keeps_row_identity`: `Partial { scrolled: -1 }` and the row that was on top is now at display row 1 **with the same `RowId`**. The point rebuild it pinned is the work this packet deleted |
| `content_tests::last_content_line_finds_the_last_written_row` | signed grid line | `last_content_row_...`, a display row |
| `content_tests::style_runs_reach_the_right_columns` | legacy `Flags` / `Color` per cell | the row's `StyleRun`s: same columns, same colours, plus "three runs, not nine cells' worth of style" |
| `content_tests::hyperlinks_are_reachable_by_id` | — | **new**: one run shares one `HyperlinkId` and `hyperlink(id)` resolves the target |
| `model_tests::a_sixel_reaches_the_snapshot_once_with_per_cell_offsets` | a `GraphicCell { id, col, row }` on every covered cell | `..._with_its_placement`: the cell names the image, the placement says where its top-left is, and `graphic_offset` derives the per-cell offset (R-21). The pixels are still handed out exactly once |
| `model_tests::search_reports_matches_in_grid_lines` | `matches[0].line == -1` | `search_reports_matches_on_their_rows`: the match carries a `RowId`; `grid_line(screen_top)` is still `-1` |
| `search::tests` (12) | `.line` | `.row` / `.grid_line(screen_top)` / `.display_row(screen_top, offset)` |
| `mouse_encode::tests` (19) | `TermMode` fixtures | `ModeSnapshot` fixtures. `sgr_ignores_utf8_mouse_flag` → `sgr_wins_over_utf8`: the engine's mouse **encoding is one value**, so `? 1006` replaces `? 1005` instead of overlapping it |
| `color_classification::tests` (5 colour tests) | `is_app_chosen_exact_color`, `is_default_background_color` | **deleted with the two predicates**, which had no caller outside their own tests. The view asks the same two questions of its own `render::frame::Color` |
| `palette::tests` (8) | `Color::Spec` / `Color::Indexed` | `Color::Rgb` / `Color::Palette`; four renamed to match |
| `session::tests::bracketed_paste_strips_embedded_markers` | `probe.set_mode(BRACKETED_PASTE)` | `probe.feed(b"\x1b[?2004h")` — the fake has a real engine now |

`crates/terminal-view`:

| Test | Was | Is |
| --- | --- | --- |
| `frame::tests::frame_row_hash_changes_with_content_and_style` | the per-row FNV hash | **`row_keys_follow_content_not_frames`**: `(RowId, SeqNo)` is stable across a frame with no output and moves for the row that changed — and only that row |
| `frame::tests::frame_damage_and_wrap_flags_pass_through` | `Damage::Rows` | `frame_wrap_flag_reaches_the_last_cell`; `Damage` is deleted, the tri-state replaces it |
| `frame::tests::frame_selection_converts_to_display_rows` | a fabricated `SelectionRange` at a display offset | `frame_selection_and_cursor_are_display_rows`, through a real engine selection |
| `frame::tests::frame_styles_come_from_the_rows_runs`, `frame_hyperlinks_are_one_id_per_run` | — | **new**: the two things the per-cell record used to carry |
| `plan_cache::tests::cursor_row_replans_on_undamaged_change` | the cursor row was always a candidate, to catch an undamaged echo | `only_the_changed_row_replans`. The engine's `changed` list is exact, so the cursor row is no longer a candidate and an idle frame considers **zero** rows (it used to consider one) |
| `plan_cache::tests::scroll_rotates_plans_and_replans_only_scrolled_in_rows` | rotation driven by the `display_offset` delta, verified by hashes | `scroll_shifts_...`: driven by `Partial { scrolled }` and verified by the key |
| `plan_cache::tests` (the other 7) | `FrameBuilder` fabrication | the same assertions, over frames a real engine produced |
| `element_tests::render_has_single_alacritty_file` | one needle, `alacritty_terminal` | `render_has_single_engine_file`: five needles for the engine's **cell** vocabulary. `RenderUpdate` is deliberately not one — the tri-state is the contract `plan_cache` is written against |
| `element_tests::sixel_image_paints_once_per_frame` | a fabricated `GraphicData` plus four `GraphicCell`s | a real Sixel DCS through the fake, and `cls` to release it |
| `element_tests::frame_time_under_output` | — | **new**, `#[ignore]`d measurement (§ 2 of the measurements file) |
| `row_plan::tests` | one `assert_ne!(plan.hash, 0)` | **deleted** with the field |
| `url::tests` (17 `detect_url_at` calls) | `Vec<IndexedCell>` + a `num_cols` argument | `LineRangeCells`, which carries its own stride |

`crates/local-shell/src/session_tests.rs` — the one file this packet touched in a
crate `US-0083` / `US-0084` own. Seven one-line reads of deleted fields, nothing
else:

| Line | Was | Is |
| --- | --- | --- |
| the import | `alacritty_terminal::selection::SelectionType` | `oneterm_terminal::SelectionKind as SelectionType` (the three call sites are untouched: the variant names match) |
| `snapshot_contains` | `.cells.iter().map(\|c\| c.cell.c)` | `.text()` |
| the cwd assertion | the same map | `.text()` |
| `trait_snapshot_bounds` | `.terminal_bounds.num_cols` / `num_lines` | `.size().cols` / `.rows` |
| `trait_write_resize_no_panic` | `.terminal_bounds.num_cols` | `.size().cols` |
| `mouse_drag_updates_selection_not_mouse_move` | `.selection` and `sel.start.column.0` | `.selection_range()` and `sel.start.col` |

`crates/ssh` is untouched: the only thing that broke there was the macro
expanding `::oneterm_vt` paths into a crate that does not depend on it, which is
fixed inside `crates/terminal` (every path in `impl_pty_terminal_session!` is
now `$crate::`).

## 3. GUI walks — **not reproducible; the gap stands**

`quser` reports session 1 in state **`Disc`** (idle 12:03) for the whole packet.
That is the same environment the `US-0081` independent verifier recorded: with
the session disconnected, `Graphics.CopyFromScreen` fails with "The handle is
invalid" and `PrintWindow` returns an all-black bitmap. The owner's own
`oneterm.exe` (pid 27376) was recorded first and never touched.

So **none of the four walks was reproduced and no `US-0085-` screenshot exists**:

| Walk | State |
| --- | --- |
| IN-0018 render walk, incl. the sampler pixel comparison against a binary built from `main` | **not run** — needs a presented window on two binaries |
| IN-0027 font fallback / ligature walk | **not run** |
| IN-0028 Sixel walk, incl. `cls` and a prompt below an image | **not run** |
| US-0071 local-shell walk | **not run** |

What stands in for them, and how far it goes:

- **The strongest piece is not mine.**
  [`US-0085-independent-verify.md`](US-0085-independent-verify.md) § 4 ran the
  **old** view conversion — `engine_shim::write_row` composed with
  `frame::Cell::from_indexed`, both copied verbatim from `d3c537b` and built
  against the real `alacritty_terminal` types — against the **new**
  `FrameRow::cell` over 46 corpus streams and 15 hand streams (SGR including every
  underline kind, 256-colour, truecolour, CJK, Hangul, combining marks, ZWJ
  emoji, ligature-shaped ASCII, OSC 8, wide-at-edge, wrap, alt swap, RIS,
  scrollback, cursor shapes, Sixel, **Sixel then `cls`**), each chunked at 4 096
  bytes and again one byte at a time, comparing every cell's character,
  zero-width followers, both colours, all twelve `CellFlags` bits, hyperlink
  presence and the derived graphic anchor:
  **2 822 154 cells, 0 differences** — plus the style, colour and width maps
  proved pointwise equal in isolation over all 4 096 attribute subsets, all four
  `CellWidth`s, all 29 `NamedColor`s and all 256 palette entries. That is what
  retires the deleted `NamedColor::DimBlack` discriminant arithmetic safely, and
  it covers precisely the file `us0081_parity` does **not** exercise.
- **The painted output is unchanged, cell for cell.** `us0081_parity` feeds 81
  streams — the 45 alacritty recordings, OneTerm's `sixel_basic` and 35
  hand-written streams covering SGR, 256/truecolor, CJK, emoji, combining marks,
  wide-at-edge, wrap, alt screen, scrollback and malformed input — to both
  engines and compares every cell's character, zero-width followers, colours,
  flags, hyperlink, graphic reference and position, plus the cursor, the modes,
  the offset, the bounds, the selection, the damage and the decoded images. It
  is green through the **native** path. What a frame contains is therefore
  proven; what a GPU draws from it is not.
- **The paint path runs.** `element_tests` draws real frames in headless GPUI
  windows and counts quads, glyphs, layers, images and uploads:
  `dirty_frame_plans_rows_and_shapes`, `idle_frame_plans_nothing_but_paints`,
  `idle_frame_allocates_nothing`, `sixel_image_paints_once_per_frame` (a real
  Sixel, painted once per frame, gone after `cls` — the IN-0028 walk's two
  central claims, minus the pixels), the gutter and cursor-layer tests, and
  `shapes_tests` (1 359 lines) for the box/block/braille/powerline geometry the
  IN-0018 walk looks at.
- **The ligature and fallback path is untouched.** `CellAnchor`,
  `glyphs.rs`, `row_plan.rs` and `shapes.rs` were not edited by this packet;
  `glyphs_are_anchored_at_their_cell` still pins IN-0027's anchoring rule.
  This is an argument from the diff, not a measurement.

**This gap is real and it is the packet's largest.** The four walks remain owed,
and the sampler comparison against a binary built from `main` is the piece that
would close the IN-0018 half. They need an **Active** desktop session.

## 4. The app builds and starts

```text
cargo build -p oneterm-app --profile fast-dev   Finished in 1m 45s (exit 0)
target/fast-dev/oneterm.exe                     81 644 544 bytes
```

`Get-Process oneterm` before the launch: **27376** — the owner's, recorded first
and never signalled. An instance was started from **this worktree's** binary with
`Start-Process -PassThru`:

| | |
| --- | --- |
| pid | 15516 |
| alive after 8 s | yes |
| main window | `hwnd=2362650`, title `OneTerm`, rect 1616x926 |
| working set | 123 MB, steady across the capture attempt |
| exit | `Stop-Process -Id 15516`; `Get-Process oneterm` afterwards shows only 27376 |

So the render path this packet rewrote starts, opens a window and survives — it
does not panic on the first frame. That is the whole of what can be claimed:

```text
PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT) -> True
sampled 5 300 pixels: 94 non-black (near-black chrome; the saved PNG is black)
```

The capture is an all-black bitmap, which is the documented disconnected-session
result and the same thing the `US-0081` verifier hit. **No pixels, so no walk.**
