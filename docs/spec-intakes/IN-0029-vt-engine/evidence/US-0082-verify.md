# US-0082 — independent verification

Verifier: independent agent. Not the implementer. Nothing fixed, nothing committed.
Branch `worktree-agent-a8c26d0ccf878c84b` @ `5fa622e`, six commits off `feat/vt-engine` @ `f9af66c`.
Worktree `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a8c26d0ccf878c84b`
(`Get-PSDrive D`: 62.9 GB free, above the 15 GB floor). Specs read from the main checkout
(`feat/vt-engine` @ `bd79d97`), never written to.

**Verdict: merge after fixes.** Every finding is documentation or labelling. No behaviour defect
was found, the differential is untouched and green, and the contract `US-0083` / `US-0084` /
`US-0085` need is implemented, reachable and — for the demand handshake — measurably effective.

---

## 1. Pass / fail table

| # | Check | Result | Raw evidence |
| --- | --- | --- | --- |
| 1a | Scope: only `crates/terminal` + the two assigned docs + packet docs | **PASS** | `git diff f9af66c...HEAD --stat`: 28 files — 24 under `crates/terminal/`, `docs/agents/structure.md`, `docs/terminal-backend.md`, the packet and its bench evidence. `git diff f9af66c HEAD --stat -- crates/terminal-view/ crates/local-shell/ crates/ssh/ crates/app/ crates/vt/ crates/terminal/tests/` → **empty**. N-04's must-not column for `US-0082` is "any other crate"; held. |
| 1b | Additive `crates/vt` accessors | **PASS (none needed)** | `crates/vt` byte-identical. `Demand` (`crates/vt/src/render/demand.rs:20`) and `RenderState` already shipped at `US-0079`/`US-0081`. Packet says so; true. |
| 1c | Trailer exact on all six commits | **PASS** | Each of `bf73a0c 45588a7 dd2d2a8 13a3001 abcceba 5fa622e` ends `Refs: IN-0029` / `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` / `Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9` — identical to the convention on `f9af66c` and its parents. |
| 1d | No new dependency | **PASS** | `git diff f9af66c...HEAD -- '*Cargo.toml' Cargo.lock` → empty. `parking_lot` was already there. |
| 2a | `pwsh scripts/ci-local.ps1` green | **PASS** | exit 0, all ten steps. `verify-dependency-graph` 21 packages, `check-doc-paths` 124 paths / 10 documents, `check-english` 752 files, completion catalogs valid, `THIRD-PARTY-NOTICES.md` up to date. |
| 2b | Raw totals | **PASS — reproduce the packet exactly** | Scripted count over the log's `test result: ok.` lines: **62 sections, 1918 passed, 0 failed, 13 ignored**. Identical to the packet's claim and to the harness `evidence` field. |
| 2c | `us0081_parity` unchanged: same 81 streams, same allow-list | **PASS** | Blob hash identical — `git rev-parse HEAD:crates/terminal/tests/us0081_parity.rs` = `git rev-parse f9af66c:...` = `bddae247f8e1e2b8b3f1d69db92f04415452e9b9`. Corpus still 45 vendored + `sixel_basic` + 35 hand streams = **81**. Allow-list still the five S-kinds (`cell.hyperlink`, `damage`, `mode`, `mode.dropped_line_wrap_urgency`, `total_lines`) — **not widened, nothing removed**. CI run: `5 passed; 0 failed; 2 ignored`, `snapshot_parity_over_every_recording_and_hand_stream ... ok`. |
| 3 | Deleted tests all re-covered | **PASS** | Coverage map in § 2. 15 + 11 + the deferred tier + line accounting + colour constants: every one lands on a named test. |
| 4a | `TerminalHandle` FairMutex + Demand contract | **PASS, and proved effective** | § 3. Two-thread scratch test: honouring `take_render_demand()` hands the lock over in **1 batch / 157 µs**; ignoring it (today's local-shell shape) costs **3800 batches / 354 ms**. |
| 4b | `TerminalModel::new(term, impl Into<ResizePolicy>)` | **PASS** | `model.rs:86`. `From<ResizePolicy> for oneterm_vt::ResizePolicy` at `model.rs:60`. `US-0083`/`0084` change one token each, as claimed. |
| 4c | `OscRouter::drain(&batch, &mut Vec<SessionEvent>)` + event-order rule | **PASS** | `osc_router.rs:137` — exact signature. `VtEvent::Repaint => {}` at `osc_router.rs:161` with the LLD's reason in the comment. Order pinned by `backend_tests.rs:535 each_chunk_posts_exactly_one_output_after_its_reliable_events` (three chunks → Title, Bell, Output ×3) and `:564 events_from_several_advances_survive_to_one_finish_batch`. |
| 4d | Every field the view reads is still reachable | **PASS** | All 13 accessors present on `TerminalContent`: `update` `rows` `changed` `size` `render_cursor` `modes` `selection_range` `placements` `hyperlink` `scroll_offset` `row_id` `display_row` (+ `invalidate`, `cursor_visible`) — `content.rs:225-300`. Compat fields `cells/cursor/mode/display_offset/total_lines/selection/terminal_bounds/damage/graphics` unchanged. |
| 4e | Losing `Clone` breaks nothing | **PASS** | Outside `crates/terminal`, `TerminalContent` appears only in `crates/terminal-view/src/render/frame.rs` (`:467` one owned field of `Frame`, `:479` `default()`, `:577` `size_of`, `:593-607` tests). `Frame` does not derive `Clone`; no `.clone()` of a content anywhere; no `snapshot()`/`snapshot_into` caller outside the crate except `frame.rs:487`. Broadcast fans **input** out to sessions, each with its own view and its own `Frame`; the search UI reads `GridText`, not a content. Per-view ownership is what `DEC-0015` asks for and it removes `US-0081`'s latent shared-watermark theft. |
| 5 | Behaviour parity of what the view receives | **PASS — stronger than asked** | No rebuild of the old branch was needed: the unchanged differential feeds identical bytes to `alacritty_terminal::Term` (through the pre-`US-0081` `refill`, copied verbatim) **and** to the native adapter, then diffs every field of `TerminalContent` — per-cell char, followers, colours, flags, hyperlink, graphic ref and `point`; cursor; mode; scroll offset; bounds; selection; damage; images — after every 4 KiB chunk of all 81 streams. Green, with only S1–S5 allow-listed. That compares against the **reference engine**, not against the shim, so it subsumes the requested shim-vs-native diff. Zero differences beyond S1–S5. |
| 6 | Flood measurement reproduced | **PASS** | § 4. Median of three: NEW 148.2 ms (feed 91 / snapshot 55) vs OLD 85.2 ms (56 / 27), ratio 1.74x — the packet records 150.7 (93/56) vs 83.3 (55/27), 1.81x. Within run-to-run noise. |
| 7 | Deferred items assigned | **PASS with one gap** | § 5. `Engine::exit`, the signed lines and the dropped events are assigned in both the packet and the LLD. `mouse_encode.rs` (and two deletion-list rows) are assigned to `US-0082` by `migration.md` and silently slip — disclosed as gap 3 but not listed among the packet's three declared deviations → **F3**. |
| 8 | Code quality | **PASS** | No `unsafe` block in `crates/terminal` (only the identifiers `strip_unsafe_chars` / `is_unsafe_char`). No `.unwrap()` / `.expect()` on any runtime path — the one lock is `pending.lock().unwrap_or_else(PoisonError::into_inner)` (`pump.rs:244`). No dead code (clippy `-D warnings` covers `dead_code`). Exactly one lint suppression, `engine_shim.rs:131` → **F5**. Module and item docs are dense and accurate except **F1**. |
| 9a | Packet completeness | **PASS** | Classification, Outcome, Scope, Acceptance, Documentation (Owning Docs / Action / Reconciliation + three declared deviations), Context, Plan, Decisions, Verification Plan, Evidence, six Gaps, Handoff. Status `Implemented`, proof block honest (`E2E` unchecked). |
| 9b | Harness DB row `US-0082` | **PASS** | `story` row present: `status=implemented`, `risk_lane=high_risk`, `contract_doc=…/low-level-design/migration.md`, `packet_doc=…/US-0082-terminal-native.md`, `unit/integration/platform_proof=1`, **`e2e_proof=0`**, `verify_command=pwsh scripts/ci-local.ps1`, `last_verified_result=pass`, `intake_id=34`. |
| 9c | GUI walk gap honest | **PASS — and still true now** | `quser` at verification time: `trunglt … 1 Disc 10:51 9/11/2026 4:35 PM`. The only session is **disconnected**, so there is no interactive desktop. No app was launched, no `oneterm.exe` was enumerated or stopped. The walk remains owed. |

---

## 2. Coverage map for the deleted tests

### The 15 `legacy_resize` scenarios → `oneterm_vt::reflow::tests`

| Deleted (`crates/terminal/src/legacy_resize.rs` @ `f9af66c`) | Covers it now (`crates/vt/src/reflow/reflow_tests.rs`) |
| --- | --- |
| `keep_viewport_top_grow_keeps_rows_cursor_and_history` | `:411` same name |
| `default_grow_pulls_history_and_moves_the_cursor_down` | `:428 default_policy_grow_anchors_the_bottom_row` + `:244 rows_only_grow_moves_the_cursor_down_by_the_pulled_rows` |
| `keep_viewport_top_grow_larger_than_history` | `:440` same name |
| `keep_viewport_top_repeated_grows_and_column_change` | `:456` same name |
| `keep_viewport_top_restores_a_scrolled_back_viewport` | `:471 keep_viewport_top_restores_the_scroll_offset` |
| `keep_viewport_top_leaves_shrink_and_history_less_grow_to_alacritty` | `:483 …_to_bottom_anchor` |
| `keep_viewport_top_grow_during_alt_screen_corrects_the_primary_grid` | `:501 keep_viewport_top_corrects_the_primary_screen_while_alt_is_active` |
| `default_grow_during_alt_screen_pulls_the_primary_history` | same name |
| `keep_viewport_top_widen_joins_wrapped_rows_and_keeps_the_top_row` | same name |
| `keep_viewport_top_widen_without_row_change_moves_the_cursor_up` | same name |
| `keep_viewport_top_top_row_continuing_a_history_line_keeps_the_cursor_row` | same name |
| `keep_viewport_top_widen_joins_the_cursor_row` | same name |
| `keep_viewport_top_narrow_with_a_mid_screen_cursor_pulls_split_rows_back` | same name |
| `keep_viewport_top_narrow_with_the_cursor_at_the_bottom_matches_alacritty` | `…_matches_bottom_anchor` |
| `keep_viewport_top_widen_during_alt_screen_joins_the_primary_rows` | same name |

Plus `keep_viewport_top_moves_the_saved_cursor` with no old counterpart (a superset, as claimed).
The adapter's own share — that the backend's policy actually reaches the engine — is
`model_tests::resize_grid_applies_the_backend_policy`. **15/15 covered, nothing uncovered.**

### The 11 `sixel_tests` → `oneterm_vt::graphics::tests` (26 tests)

| Deleted | Covers it now (`crates/vt/src/graphics/graphics_tests.rs`) |
| --- | --- |
| `decodes_colors_columns_and_carriage_return` | `:154 decodes_a_minimal_sixel`, `:169 rgb_and_hls_colour_registers`, `:188 repeat_and_band_control_characters` |
| `repeat_hls_and_partial_columns` | `:188`, `:169`, `:205 untouched_pixels_stay_transparent` |
| `raster_attributes_fix_the_size_and_a_new_band_moves_down` | `:216 raster_attributes_declare_the_size` + `:284 cursor_descends_bands_times_six_over_twenty` |
| `width_is_clamped` | `:234 dimensions_clamp_at_4096` |
| `empty_and_non_sixel_dcs_place_nothing` | `:405` same name |
| `places_cells_and_leaves_the_cursor_on_the_last_band_row` | `:245 placed_at_the_cursor_column_and_clipped_right` + `:284` |
| `image_wider_than_the_grid_is_clipped_on_the_right` | `:245` |
| `image_at_the_bottom_scrolls_into_history` | `:307 scrolls_into_history_with_its_cells` |
| `erase_and_overwrite_drop_the_reference` | `:341 overwriting_or_erasing_a_cell_drops_the_reference` |
| `band_count_decides_the_cursor_row` | `:284` |
| `primary_device_attributes_advertise_sixel` | `:391 da1_advertises_sixel` |

**11/11 pinned one-to-one or better**; the adapter's addition (per-cell offsets, handed out once) is
`model_tests::a_sixel_reaches_the_snapshot_once_with_per_cell_offsets`.

### Deferred tier, line accounting, colour constants

| Deleted | Covers it now | Verified present |
| --- | --- | --- |
| `reliable_events_do_not_block_while_the_engine_lock_is_held` (CORR-01) | `backend_tests.rs:215 the_drain_never_waits_on_a_full_queue` | yes |
| `deferred_events_flush_in_order_once_the_queue_drains`, `deferred_events_keep_fifo_order`, `async_flush_delivers_deferred_events` | `backend_tests.rs:399 pending_events_apply_backpressure_outside_the_lock`, `pump_async_variants_publish_lifecycle_in_order`, `:535`, `:564` | yes |
| `line_accounting_tracks_growth_saturation_and_reset` | `backend_tests.rs:465 the_gutter_line_count_is_the_engines_output_line_count` (floor at the viewport, growth past the cap, a wrap is not a line, a clear does not reset) | yes |
| `color_key_indices_match_the_adapter_constants` | `backend_tests.rs:344 color_queries_are_queued_with_their_typed_key` + `osc_color.rs:172 every_color_key_is_answered_or_skipped` | yes |
| `content::tests` (5), `search::tests` (11), `osc_color::tests` (5), `model::tests` (5) | rebuilt / kept as the packet describes; all named tests exist (`content_tests.rs:31,48,107,139,168`, `search.rs:386`) | yes |

Deletion greps are clean as claimed: `resize_keeping_viewport_top`, `conhost_cursor_row` and
`LineAccounting` have **no definition and no call site** in `crates/` — only prose that records the
deletion (`crates/vt/src/reflow/mod.rs:8-9`, `backend/mod.rs:15`, `pump.rs:99`,
`backend_tests.rs:461`). That satisfies `migration.md:373`.

---

## 3. Contract review for US-0083 / US-0084 / US-0085

**`TerminalHandle` (`crates/terminal/src/handle.rs`).** `FairMutex<Engine>` + one `Demand`.
`lock_for_render()` (`:106`) raises then locks; `take_render_demand()` (`:115`) swaps and clears;
`render_demand_raised()` reads without clearing. `lock_for_render` is called only from
`model.rs:106` (`snapshot`) and `:113` (`snapshot_into`) — the two frame paths, exactly as
documented, and not from `query_state` / `terminal_info`, which would ask the pump to yield for
O(1) reads. `take_render_demand` has **no non-test caller** (the packet's gap 6, honest).

*The two-thread proof.* I wrote a scratch integration test (run, then deleted; worktree verified
clean with `git status --porcelain`) modelling the real starvation case — a pump that takes the
lock once and keeps feeding while bytes arrive, which is literally
`crates/local-shell/src/event_loop.rs:391-420`:

```
honoured: 1 batches, 156.8µs
ignored : 3800 batches, 354.4733ms
test honouring_the_demand_bounds_the_renderers_wait ... ok
```

A pump that checks `take_render_demand()` at the chunk boundary and breaks hands the lock over
within **one** batch; the same pump ignoring it makes the renderer wait **3 800 batches / 354 ms** —
about a 2 260x latency difference, and today's behaviour until `US-0083` lands. The adapter half of
R-37 is therefore not just present but demonstrably sufficient; the engine-level counterpart
(`crates/vt/src/render/render_tests.rs:785`) uses an unfair `std::sync::Mutex` and does not cover
the `FairMutex` path, so this is new information.

**`TerminalModel::new(term, impl Into<oneterm_vt::ResizePolicy>)`** — `model.rs:86`, with
`From<crate::model::ResizePolicy>` at `:60`. Both backends switch by changing one token in
`impl_pty_terminal_session!`. Confirmed.

**`OscRouter::drain(&self, batch: &EventBatch, out: &mut Vec<SessionEvent>)`** — `osc_router.rs:137`.
Replies go to the transport first (`:138-142`), then every non-`Reply` event is routed into `out`.
`Repaint` is dropped (`:161`) so the pump stays the single owner of the hint; `RowsScrolled` /
`RowsTrimmed` (`:200`) and `GraphicReleased` (`:202`) are dropped with their reasons. The pump
sends `out` after the guard drops (`pump.rs:231-241`), and the order rule is tested.

**`TerminalContent` owns the `RenderState`.** `content.rs:112-151`: `state`, `update`,
`last_cursor_row`, `built_offset` are `pub(crate)`; the compat fields stay public. Every accessor
`api-surface` § 9 and `frame.rs` need is reachable (table row 4d). `row_id` / `display_row`
(`:279`, `:285`) are the two-way translation `plan_cache` will key on.

**The incremental compat rebuild (`engine_shim.rs:132-174`) is sound.** `laid_out` requires both
`cells.len() == rows*cols` **and** the same `display_offset`, so a resize or a scroll-offset change
falls back to a full re-lay-out; `incremental` additionally requires `Partial { scrolled: 0 }`.
Positions are rewritten only when `!laid_out`, which is correct because `point.line` is a pure
function of `(dense index, display_offset)` — content scrolling does not move a dense slot. The
differential diffs `IndexedCell::point` per cell after every chunk over all 81 streams, so this
optimisation is covered by the strongest test in the tree.

---

## 4. Perf attribution

Reproduced on this machine, `--profile fast-dev`, 4 MiB / 4 KiB chunks, 120x30, scrollback 10 000,
median of three:

| | this run (after) | packet (after) | this run (old engine) | packet (old) |
| --- | ---: | ---: | ---: | ---: |
| feed | 91 ms | 93 ms | 56 ms | 55 ms |
| snapshot | 55 ms | 56 ms | 27 ms | 27 ms |
| total | 148.2 ms | 150.7 ms | 85.2 ms | 83.3 ms |
| ratio | **1.74x** | 1.81x | — | — |

**The table reproduces.** The snapshot half is down from `f9af66c`'s 75 ms as claimed.

**Where the remaining feed cost is.** The bench calls `Terminal::feed` directly, so the lock and the
demand handshake contribute **zero** to that column — it is not a synchronisation cost. Re-running
the identical bench with `CARGO_PROFILE_FAST_DEV_DEBUG_ASSERTIONS=false`:

```
flood: 4194344 bytes, grid 120x30, scrollback 10000, debug_assertions=false
NEW total 103.0446ms   new feed 66 ms   new render_update+snapshot 34 ms
OLD total  79.6204ms   old advance 53 ms   old damage+snapshot 25 ms
```

So of the 35 ms feed gap, **~25 ms is debug-only**, and of the snapshot's 28 ms gap, **~21 ms is**.
`cargo test -p oneterm-vt integrity_walk_cost` confirms the source directly — `feed(one line)
19.5 µs`, `render_update() 20.2 µs` over a 100 000-row history — which across 1 025 chunks is
≈ 20 ms on each side, i.e. essentially the whole delta. Attribution:

| Cost | Size | Owner |
| --- | ---: | --- |
| Bounded integrity walk + debug asserts (feed) | ~25 ms | not shippable cost; `testing-and-bench.md`'s tier, already reduced three orders of magnitude at `US-0075`/`US-0079`. Nobody removes it; it is off in release. |
| Engine parse + dispatch above the old engine (feed) | ~13 ms (66 vs 53) | engine cost for `IN-0029`; not `US-0083`/`0085`'s to remove |
| Integrity walk inside `render_update` (snapshot) | ~21 ms | same |
| The legacy `Cell` rebuild that remains (snapshot) | ~9 ms (34 vs 25) | **`US-0085`** — the only line item a later packet in this intake can still delete |
| Lock / demand handshake | 0 in this bench | measured separately in § 3 |

In a release-shaped build the new engine is **1.29x** the old on this flood, not 1.81x — worth
saying, because `migration.md`'s shim-era note records release at 90 vs 43 ms (2.1x), and this
packet has therefore roughly halved the release-build ratio as well. The packet's flood evidence
attributes the whole feed column to "the engine" without separating the debug tier → **F4**.

---

## 5. Judgement on the deliberately-deferred items

| Item | Judgement | Assigned in the packet | Assigned in `migration.md` |
| --- | --- | --- | --- |
| `Engine` as a `Deref` newtype whose only member is the no-op `exit()` (`handle.rs:41-70`) | **Accept.** Deleting it means editing `crates/local-shell/src/event_loop.rs:357`, which N-04 forbids this packet. The alternative — keeping `engine.rs` and its render state — is strictly worse. It carries a `ponytail:` comment naming the ceiling and the upgrade path, `Deref`/`DerefMut` make it free at every other call site, and it costs one type and one empty fn. | yes — Handoff, "deleting `Engine`: one no-op blocks it" | implied by the `US-0083` row ("`crates/terminal`" is must-not-touch for that packet, so the deletion has to be paired) |
| `mouse_encode` still takes `TermMode` | **Accept the code, flag the record.** The reasoning is right: `TerminalQueryState` publishes `TermMode` anyway, so converting now leaves two representations and rewrites the encoder's fixtures twice. But `migration.md:149` lists `mouse_encode.rs` among the files that "move onto the native API" at `US-0082`. | yes — gap 3 | **conflicts**: `:149` assigns it here, `:167` moves only the *view's* `input/mouse.rs` at `US-0085` → **F3** |
| `SearchMatch::line`, `TerminalInfo::{cursor_line,last_content_line}`, `TerminalQueryState::cursor_line` keep signed lines | **Accept.** Verified the blocker is real: `crates/terminal-view` constructs `SearchMatch { line, .. }` in its own tests and calls `display_row`, both of which N-04 puts in `US-0085`. The implementation did move to `RowId` internally (`search.rs:151` converts once, where a match is published). | yes — declared deviation 2 | `:290` assigns the change to `US-0082`; the deviation is declared against it, which is the right way to disagree with a spec |
| `VtEvent::RowsScrolled` / `RowsTrimmed` / `GraphicReleased` dropped; `Partial { scrolled }` surfaced as `Full` | **Accept.** Each needs a `RowId`-keyed cache above the seam that does not exist yet; forwarding them now would be an event with no consumer. Both drop sites carry the reason. | yes — gap 5 | yes — the view-needs table and the `US-0085` row (`plan_cache` on `(RowId, SeqNo)`) |
| `TermDamageInfo`'s display-line conversion and the per-frame cell loop, `migration.md:252-253` | **Accept the code** (both are compat surface `frame.rs` reads), but they are deletion-list rows assigned to `US-0082` that are only *implicitly* covered by declared deviation 1 → **F3** |

---

## 6. Findings, ranked

**F1 — `crates/terminal/src/model.rs:74` — stale doc + broken intra-doc link (medium).**
```rust
/// Every render-path state that must survive between frames
/// (the damage watermark, the copied rows) lives inside the lock, on
/// [`crate::engine::Engine`].
```
`crate::engine` is the module **this packet deleted**; the watermark and the copied rows now live
in `TerminalContent`, which is the packet's headline change. The sentence is false and the link
does not resolve. CI does not catch it — `ci-local.ps1` has no `cargo doc` step, and
`broken_intra_doc_links` is a rustdoc lint, not a clippy one.
*Fix:* replace the two lines with "the damage watermark and the copied rows live in the caller's
[`TerminalContent`]", or delete the sentence.

**F2 — packet, Documentation → Reconciliation, deviation 1 — "exactly five files" is wrong (medium, doc accuracy).**
The packet asserts the crate names an `alacritty_terminal` item "in exactly five files —
`engine_shim.rs`, `content.rs`, `session.rs`, `palette.rs` and `osc_color.rs`. Nothing else."
Actual, by `grep`: **13 non-test source files** — those five plus `color_classification.rs`,
`handle.rs` (a doc comment), `lib.rs`, `model.rs`, `mouse_encode.rs`, `backend/state.rs` and
`backend/osc_router.rs`. The packet's own "What survives of `alacritty_terminal`" table lists most
of them, so the packet contradicts itself; only `backend/osc_router.rs` is missing from that table
as well. The substantive claim ("nothing runs, nothing on the engine-facing path") holds — I checked
each site.
*Fix:* delete the "exactly five files" sentence and point at the table; add the `osc_router.rs` row.

**F3 — packet vs `migration.md` — three `US-0082` rows slip without being declared (low-medium).**
`migration.md:149` assigns `mouse_encode.rs` to this packet's native swap; `:252` assigns
"`TermDamageInfo`'s display-line conversion" and `:253` "the per-frame cell clone loop in `refill`"
to this packet's deletion list. None is done. The first is disclosed as *gap 3* (a gap, not a
deviation); the other two only implicitly, under deviation 1. Each has a sound reason, and they all
reduce to "the compat surface survives until `US-0085`".
*Fix:* one sentence added to declared deviation 1 naming all three rows, so the packet and the LLD
agree on paper as well as in practice.

**F4 — `evidence/US-0082-flood-bench.md` / packet Measurement — the feed column is attributed to the engine without separating the debug tier (low).**
"Feed is the engine's and is untouched here" is true about *this packet's* changes but reads as
"this is the engine's real cost". Measured here, ~25 of the 35 ms feed gap and ~21 of the snapshot
gap are the bounded integrity walk, which is off in release; the release-shaped ratio is 1.29x, not
1.81x.
*Fix:* two lines in the bench evidence with the `debug_assertions=false` numbers above, so
`US-0085` inherits the right target (≈9 ms of legacy-cell rebuild, not 28 ms).

**F5 — `crates/terminal/src/engine_shim.rs:131` — the crate's only lint suppression (low).**
`#[allow(clippy::too_many_arguments)]` on `refresh_cells(cells, built_offset, state, update, rows,
cols, display_offset)`. All seven are reachable from `&mut TerminalContent` plus the `RenderUpdate`;
passing those two removes the suppression and three of the call-site arguments.
*Fix:* optional; take it or leave the `allow`.

**F6 — packet Outcome § 3 / `pump.rs:68` — "caller-owned vector" is not what was built (nit).**
The vector is `TerminalPump::pending: Mutex<Vec<SessionEvent>>`, pump-owned and locked per
`advance`, because `finish_batch*` takes `&self`. The behaviour — collect under the engine lock,
send outside it — is exactly what the design asks for; only the wording is off.
*Fix:* say "a pump-owned pending vector".

---

## 7. What is still owed after this packet

1. **The GUI walk** (`quser` → `Disc`). The differential plus "no consumer changed" is a strong
   stand-in, and the two **declared gutter behaviour changes** (line numbers count output lines, not
   display rows; the count no longer resets on `cls`) are exactly the kind of thing only a human at
   the window will judge. The packet routes that to the owner, correctly, with the one-line revert
   priced at `US-0086`.
2. **The pump half of the handshake** — measured above as a 354 ms renderer stall until `US-0083` /
   `US-0084` call `take_render_demand()`. Nothing in this packet regresses it; it is today's
   behaviour, and the fix is now one `if` in each read loop.

---

*Method note: the scratch demand test was created in the worktree, run, and deleted; `git status
--porcelain` is clean apart from this file. The main checkout was read only. No application
process was launched, enumerated or stopped.*
