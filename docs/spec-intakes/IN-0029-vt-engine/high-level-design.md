# High-Level Design: VT engine rewrite

Intake: IN-0029
Lane: high_risk
Date: 2026-09-12

## Idea

Replace the vendored `alacritty_terminal` + `vte` pair with two Apache-2.0 workspace crates
OneTerm owns: `oneterm-pty` (pseudo-console transport) and `oneterm-vt` (parser, grid,
dispatch, reflow, damage, render state, graphics). The engine is designed around OneTerm's
consumers rather than around alacritty's API shape: rows are positional ids with engine-owned
anchors, events are values returned from `feed()`, damage is a per-row sequence number read
through a watermark, the renderer gets an incremental render state instead of a viewport copy, and
the ConPTY resize policy lives inside the engine instead of being corrected on top of it. It is a
new build, not a port: the section below lists the problems of the engine it replaces and where
each is solved.

The argument is extension cost, not throughput. The measured headroom at realistic rates is
two to three orders of magnitude ([`research/perf-baseline.md`](research/perf-baseline.md) § 4);
what the rewrite removes is 823 lines of vendored patches, a `refresh.sh --check` CI job, and
the rule that every new VT capability starts as a patch against someone else's tree
([`DEC-0014`](../../decisions/DEC-0014-oneterm-owns-its-vt-engine.md)).

Design is taken from the converged prior art rather than invented: dual-form rows and
sequence-number damage from wezterm, packed 8-byte cells with interned styles and extras from
Rio and Ghostty, GC-by-remap for the grapheme arena from kitty, lazily allocated power-of-two
ring rows from foot, tracking-point reflow from kitty and foot with avt's iterator, and the
two-phase render state from Ghostty ([`research/prior-art.md`](research/prior-art.md) § 2.2, § 9).

## Problems of the current engine and how the new engine solves them

Owner directive, 2026-09-12: *"Think as a new build, not as a copy of the old one. The new engine
must solve the problems that exist in the old one; extensibility, optimality and performance come
first."* This table is the design's answer, and it is the list a reviewer should check the finished
engine against. Every row names where the problem lives today, the section that resolves it, the
packet, and the test that proves it.

| # | Problem | Where it lives today | How the new design resolves it | Packet | Test |
| --- | --- | --- | --- | --- | --- |
| P1 | **OSC payload is an unbounded `Vec<u8>`** under `std`; an unterminated `ESC ]` from any SSH session grows memory without bound | `vendor/vte/src/lib.rs` `osc_raw`; `research/engine-semantics.md` § 1.4 | Two-tier bound with truncation, never an error: 2 KiB inline, 8 MiB only for numbers the embedder claimed large — [`parser.md`](low-level-design/parser.md) § "OSC: streamed with explicit caps" | `US-0073` | `parser::tests::osc_truncates_at_inline_cap_and_still_dispatches`, `parser::tests::osc_spills_only_for_numbers_claimed_large`, fuzz `parser` with an RSS limit |
| P2 | **Per-cell zero-width list is an unbounded `Vec<char>`** | `vendor/alacritty_terminal/src/term/cell.rs` `CellExtra::zerowidth`; `research/prior-art.md` § 10 risk 6 | Interned grapheme arena, `GRAPHEME_MAX_LEN = 16`, truncate and count, GC by remap — [`cell-and-style.md`](low-level-design/cell-and-style.md) § "Graphemes" | `US-0074` | `intern::tests::cluster_longer_than_cap_is_truncated_and_counted`, `intern::tests::grapheme_gc_preserves_every_live_cell` |
| P3 | **`Row::new(0)` writes through a dangling pointer** in release builds | `vendor/alacritty_terminal/src/grid/row.rs`; `research/prior-art.md` § 0.5 | No `unsafe` in the grid at all; rows are `Vec<Cell>` behind `RowRef`/`RowMut` — [`grid-and-scrollback.md`](low-level-design/grid-and-scrollback.md) § "Row" | `US-0075` | the `grid::` suite under `vt-paranoid`, plus the `terminal` fuzz target |
| P4 | **24 B per cell plus a heap allocation and an atomic refcount per decorated cell**, copy-on-write on every later write | `term/cell.rs` `Option<Arc<CellExtra>>`; `research/prior-art.md` § 1.2 | 8 B packed cell, interned styles and extras, no per-cell allocation — [`cell-and-style.md`](low-level-design/cell-and-style.md) § "`Cell`" | `US-0074` | `cell::tests::cell_is_eight_bytes`; `vt-bench rss` tier 5 |
| P5 | **Fully materialised scrollback**: every row is a full-width `Vec<Cell>` even when empty | `grid/storage.rs`; `research/prior-art.md` § 1.2 | Lazily allocated slots, `None` until written; the ring mask is a session constant — [`grid-and-scrollback.md`](low-level-design/grid-and-scrollback.md) § "Storage" | `US-0075` | `grid::tests::unwritten_slots_read_as_blanks`; `vt-bench rss` |
| P6 | **Events fire under the `Term` lock**, forcing a two-tier deferred/reliable sink, a blocking flush and a deadlock test | `crates/terminal/src/backend/pump.rs:163-178`, `osc_router.rs:214-268` | `feed()` returns a batch of values; nothing runs inside the engine while the caller holds a lock — [`events-and-api.md`](low-level-design/events-and-api.md) § "`feed` and drain" | `US-0079` | `event::tests::events_are_in_byte_order`, `event::tests::feed_clears_the_batch_and_returns_stats` |
| P7 | **`Event::Osc` deep-copies its parameters** into `Vec<Vec<u8>>` on the hot path, then the consumer re-borrows them | vendor patch `alacritty_terminal/0002`; `crates/terminal/src/backend/osc_router.rs:249-257` | One reusable arena per batch; OSC parameters are spans — [`events-and-api.md`](low-level-design/events-and-api.md) | `US-0079` | `event::tests::osc_params_are_spans_not_vectors` |
| P8 | **Full-viewport clone per painted frame**, and the reason a general damage-free snapshot cannot exist | `crates/terminal/src/content.rs:173-222`; `docs/terminal-backend.md:151-155` | Incremental render state: changed rows only, under the lock, as resolved style runs; tri-state result — [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md) | `US-0079` | `render::tests::single_row_change_lists_one_changed_index`, `render::tests::steady_state_makes_no_allocation`; `vt-bench render` tier 3 |
| P9 | **Damage is single-consumer and escalates to `Full` on any scroll**; no scroll event exists | `term/mod.rs` `TermDamage`; `research/prior-art.md` § 1.4 | Per-row sequence numbers plus a watermark; scroll reported as a delta; `VtEvent::RowsScrolled` for in-region motion — [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md) | `US-0079` | `render::tests::pure_scroll_reports_a_delta_with_an_empty_changed_list`, `event::tests::rows_scrolled_is_emitted_for_in_region_motion` |
| P10 | **`INSERT` mode forces full damage every frame**, silently disabling partial redraw for the session | `term/mod.rs:455-459` (trap 34) | Insert-mode writes stamp the rows they touch, like every other mutation — [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md) | `US-0079` | `render::tests::insert_mode_does_not_force_full_damage` |
| P11 | **Renderer starvation under sustained output**: fairness alone does not stop a relocking producer | `research/prior-art.md` § 2.3; `crates/local-shell/src/event_loop.rs:410-415` | Explicit demand/yield handshake at chunk boundaries, a 64 KiB cap as the backstop, replies drained before any yield — [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md) § "Fairness and reply latency" | `US-0081` | `backend::tests::pump_yields_to_the_render_demand_within_one_chunk`, `backend::tests::da1_is_answered_within_the_startup_budget` |
| P12 | **Synchronized output buffers up to 2 MiB of unapplied bytes**, needs host polling, and matches only an exact eight-byte sequence | `vendor/vte/src/ansi.rs`; trap 41 | Nothing is buffered: the renderer skips frames, deadlines use an injected `now`, `CSI ? 1 ; 2026 h` works — [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md) § "Synchronized output" | `US-0079` | `sync::tests::mode_2026_watchdog_forces_the_mode_off`, `parser::tests::sync_mode_with_leading_param_is_recognised` |
| P13 | **ConPTY resize correction is grid surgery in the embedder**: park the alt grid, install a placeholder, `swap_alt` twice, reflow a scratch grid to guess the cursor row | `crates/terminal/src/model.rs:481-541` | `ResizePolicy` is native; the measurement reuses the reflow iterator over a row range — [`reflow-and-resize.md`](low-level-design/reflow-and-resize.md) § "`KeepViewportTop`" | `US-0077` | `reflow::tests::keep_viewport_top_*`, `reflow::tests::measure_rows_matches_the_recorded_conhost_rows` |
| P14 | **Resize costs about 227 us at 80x24** and reflow has six open upstream bugs | `research/prior-art.md` § 3.3, § 1.3 | avt-derived iterator, anchors remapped through one list, property-tested; cost model stated and measured at three scrollback depths as a ratio — [`reflow-and-resize.md`](low-level-design/reflow-and-resize.md) | `US-0077` | `reflow::props::*`; `vt-bench resize` tier 4 |
| P15 | **No absolute output-line counter**: `total_lines()` saturates, so the embedder rescans every chunk for newlines | `crates/terminal/src/backend/line_accounting.rs:16-48` (PERF-19) | `lines_produced()` on the engine, incremented on line feeds only — [`grid-and-scrollback.md`](low-level-design/grid-and-scrollback.md) § "Line counting" | `US-0075` | `grid::tests::lines_produced_counts_output_lines_not_wraps` |
| P16 | **Negative `Line` indices and a moving origin**: every consumer adds the display offset, with two fallbacks in the painter | `crates/terminal-view/src/render/frame.rs:514-532`; `crates/terminal/src/search.rs:59-61` | Positional `RowId` plus a sticky-bottom offset; engine-owned anchors — `DEC-0015`, [`grid-and-scrollback.md`](low-level-design/grid-and-scrollback.md) § "Viewport anchoring" | `US-0075` | `grid::tests::scrolled_back_view_holds_still_while_rows_are_pushed`, `anchor::tests::*` |
| P17 | **Nothing tells the embedder an image died**, so the texture store guesses with an LRU | `research/api-surface.md` § 8 capability I6 | Placements are tracked anchors; liveness derived per row; `VtEvent::GraphicReleased` — [`graphics.md`](low-level-design/graphics.md) § "Liveness and the release signal" | `US-0080` | `graphics::tests::release_event_fires_on_clear_screen` and its four siblings |
| P18 | **ZWJ and emoji width is visibly wrong** (base plus a zero-width tail, the next emoji starting a new cell); no grapheme storage for mode 2027 | `research/prior-art.md` § 6.3, § 10 risk 3 | The storage decision is made now — interned clusters, capped, `cluster_width()` implemented and tested — so mode 2027 becomes a print-path packet, not a redesign — [`cell-and-style.md`](low-level-design/cell-and-style.md) § "Width" | `US-0074` (storage); a later intake for the mode | `width::tests::cluster_width_is_correct_for_emoji_flags_and_skin_tones` |
| P19 | **Hardcoded DA1 with no way to extend device attributes** | vendor patch `alacritty_terminal/0003`; `term/mod.rs:1272-1279` | A table-driven answer set: DA1 / DA2 / DSR / DECRQM / XTVERSION — [`dispatch-and-modes.md`](low-level-design/dispatch-and-modes.md) § "Answers" | `US-0076` | `dispatch::tests::da1_da2_dsr_xtversion_answers`, `dispatch::tests::decrqm_answers_match_the_mode_table` |
| P20 | **No OSC registration**: every OneTerm OSC needed a fork patch (`report_osc`), and the agent channel sits on a colliding sub-code | vendor patch `vte/0001`; `research/prior-art.md` § 6.4 | `OscClaims` registration table; the agent channel becomes a claim, and the collision is fixed in its own packet — [`dispatch-and-modes.md`](low-level-design/dispatch-and-modes.md) § "OSC registration" | `US-0076`, `US-0088` | `dispatch::tests::osc_9_7_reaches_the_embedder_through_a_claim` |
| P21 | **Unknown DCS is invisible** and the Sixel decoder had no payload byte cap | vendor patch `vte/0002` | DCS and APC stream to sinks with a 16 MiB parser-enforced cap, counted in `FeedStats` — [`parser.md`](low-level-design/parser.md) § "DCS and APC" | `US-0073`, `US-0080` | `parser::tests::dcs_aborts_past_byte_cap`, `graphics::tests::payload_byte_cap_aborts_an_endless_sixel` |
| P22 | **Malformed input is undiagnosable**: no counters, so a truncated OSC or a dropped sequence cannot be seen from a bug report | across `vendor/vte` and `Term` | `FeedStats` counts every malformed case and the embedder logs at `debug` — [`events-and-api.md`](low-level-design/events-and-api.md) § "Error policy" | `US-0073` | `event::tests::malformed_input_moves_the_right_counter` |
| P23 | **`ED 1` does not clear row 0** when the cursor is on row 1 | `term/mod.rs:1770` (trap 11) | Spec-correct; deviation C2 | `US-0075` | `grid::tests::ed1_clears_row_zero` |
| P24 | **`DCH` clamps `end` to `cols - 1`**, so a large count is not a plain shift left | `term/mod.rs:1546-1572` (trap 19) | Spec-correct; deviation C1 | `US-0075` | `grid::tests::delete_chars_shifts_left_by_n` |
| P25 | **A region scroll shorter than its count blanks without rotating**, losing content | `grid/mod.rs:252-307` (trap 17) | Spec-correct; deviation C3 | `US-0075` | `grid::tests::small_region_scroll_rotates_then_blanks` |
| P26 | **Insert mode over a wide character leaves orphaned spacers** | `term/mod.rs:1104-1113` (trap 7) | Wide pairs repaired on the insert path; deviation C4 | `US-0075` | `cell::tests::insert_mode_over_wide_char_repairs_the_pair` |
| P27 | **`CPR` ignores origin mode**, and **`RIS` keeps OSC colour overrides** | `term/mod.rs:1356-1360`, `:1841-1875` (traps 38, 39) | Both spec-correct; deviations C5 and C6 | `US-0076` | `dispatch::tests::cpr_honours_origin_mode`, `dispatch::tests::ris_resets_the_palette` |
| P28 | **`OSC 4` rejects an even parameter count wholesale** | `vendor/vte/src/ansi.rs:1372-1408` (trap 26) | Complete pairs are applied, a trailing odd parameter ignored; deviation C7 | `US-0076` | `dispatch::tests::osc_4_applies_complete_pairs` |
| P29 | **`? 47` / `? 1047` / `? 1048` are silently ignored** and **`DECSTR` is unimplemented** | `ansi.rs:895-916` (trap 13); `research/prior-art.md` § 6.3 | Both implemented; deviations C8 and C9 | `US-0076` | `grid::tests::alt_screen_47_and_1047_and_1048`, `dispatch::tests::decstr_soft_reset_scope` |
| P30 | **Kitty keyboard stack overflow pops the title stack** | `term/mod.rs:1305-1311` (trap 42) | Fixed-size flag stack that wraps; `pop(n >= len)` resets | `US-0076` | `dispatch::tests::kitty_pop_beyond_len_resets_the_stack` |
| P31 | **Every capability costs a fork patch**: 823 patch lines across five patches, a `refresh.sh --check` CI job, and a rebase on every upstream move | `vendor/patches/`, `.github/workflows/ci.yml:65-68` | A first-party crate under normal review; extension points are API — `DEC-0014`, [`migration.md`](low-level-design/migration.md) § "Deletion list" | `US-0087` | `python scripts/third-party-notices.py --check`; `test -d vendor` fails |
| P32 | **The engine and the PTY ship in one crate**, so `crates/ssh` links a PTY it never uses and `crates/tools` links a grid it never uses | `crates/ssh/Cargo.toml:20`, `crates/tools/Cargo.toml:35` | Two leaf crates split by consumer — [`pty.md`](low-level-design/pty.md) | `US-0071` | `cargo test -p oneterm-pty`; `grep -rn "alacritty_terminal::tty" crates/` empty |
| P33 | **Dead configuration and unused machinery carried forever**: vi mode, regex search, the engine's own event loop, `setup_env`, three `Config` knobs OneTerm never sets | `research/api-surface.md` § 4.1, § 9.9 | Not built; `Config` has five fields and each has a consumer — [`events-and-api.md`](low-level-design/events-and-api.md) § "The `Config`" | `US-0073` | review against the single public-surface block in `events-and-api.md` |

P1, P2 and P3 are live defects reachable from any SSH session in the engine shipping today. The
owner ruled on 2026-09-12 that **no further patch is applied to the fork**: they are fixed by the
new parser and the new cell storage landing, and the exposure window is the migration itself.

P23-P30 are the behaviours the 45 recordings pin. They are implemented **spec-correct from the
start**, not reproduced — see "Correctness first" in
[`grid-and-scrollback.md`](low-level-design/grid-and-scrollback.md) and
[`dispatch-and-modes.md`](low-level-design/dispatch-and-modes.md), and the per-recording expected
differences the parity harness carries
([`testing-and-bench.md`](low-level-design/testing-and-bench.md) § 2).

## Crate layout

```
L0  core ── (unchanged, depends on neither engine crate: R6 still holds)
L0  vt   (oneterm-vt)   leaf: parser + dispatch + grid + reflow + damage + graphics
L0  pty  (oneterm-pty)  leaf: ConPTY / openpty transport, no grid
L0  terminal (oneterm-terminal) ── core, vt        (adapter: TerminalSession, pump, OSC routing)
L3  local-shell ── core, terminal, pty
L3  ssh         ── core, terminal, vt
L3  terminal-view ── … terminal
    tools (outside the layering) ── pty, vt   (benchmarks, differential runner, diagnostics)
```

| Crate | Dir | Contains |
| --- | --- | --- |
| `oneterm-vt` | `crates/vt` | parser, dispatch, cell/style/grapheme, grid/scrollback, tracked anchors, selection, reflow, damage/render-state, events, graphics |
| `oneterm-pty` | `crates/pty` | `PseudoConsole`, `EventedReadWrite`, `EventedPty`, `OnResize`, child-exit watcher, poll tokens, the `ConptyApi` resolution order (bundled `conpty.dll` first, `kernel32` as the fallback) |

Neither depends on any OneTerm crate, and neither depends on gpui.

**Every new direct dependency, in one table (R-23, R-48).** All are already in `Cargo.lock`, so
the graph does not grow; `deny.toml:88` sets `multiple-versions = "warn"`, and `bitflags` and
`rustc-hash` each already resolve to two versions, so each row names the line to pin.

| Crate | Pin | Declared by | For |
| --- | --- | --- | --- |
| `memchr` | 2.x | `oneterm-vt` | the ground-state control-byte scan |
| `unicode-width` | 0.2.x | `oneterm-vt` | scalar width |
| `unicode-segmentation` | 1.x | `oneterm-vt` | `cluster_width()` (mode 2027 is deferred) |
| `smallvec` | 1.x | `oneterm-vt` | small parameter and run vectors |
| `bitflags` | **2.x** | `oneterm-vt` | `Attrs`, `RowFlags` |
| `rustc-hash` | **2.x** | `oneterm-vt` | `FxHashMap` for the interners |
| `polling` | 3.x | `oneterm-pty` | **public** in `EventedReadWrite` (R-33) |
| `windows-sys` | 0.59 (the workspace pin) | `oneterm-pty` | ConPTY FFI |
| `libc` | 0.2 | `oneterm-pty` | `openpty` |
| `parking_lot` | 0.12 | `crates/terminal` (the **adapter**, not the engine) | `FairMutex` |
| `proptest` | 1.x, dev-only | `oneterm-vt` | reflow and grid properties |
| `vte` | 0.15, dev-only | `oneterm-vt` | the differential oracle, retired at `US-0087` |

`cargo-fuzz` (`libfuzzer-sys`, `arbitrary`, a nightly toolchain) is **not** a workspace
dependency: it lives in a nested `crates/vt/fuzz/` crate, runs on Linux only, and is never a
packet gate (R-47).

**Module layout (R-49).** `docs/agents/code-style.md` forbids a folder holding a single file, so
the engine is flat files — `cell.rs`, `intern.rs`, `grid.rs`, `anchor.rs`, `selection.rs`,
`reflow.rs`, `damage.rs`, `render.rs`, `dispatch.rs`, `event.rs`, `strip.rs`, `testing.rs` — with
two folders that genuinely split: `parser/` (`state.rs`, `params.rs`, `osc.rs`, `utf8.rs`) and
`graphics/` (`mod.rs`, `sixel.rs`) (N-14).

### Why one engine crate and not three

The owner asked whether the parser should be `oneterm-vt-parser`. It is a **module**
(`crates/vt/src/parser/`), not a crate, and the same answer applies to a separate `-grid` crate.
Checked against the rules that govern this:

| Rule | Reading |
| --- | --- |
| `docs/agents/code-style.md`, "Crate organization" | "Prefer extending an existing crate before creating a new one"; "Extract reusable functionality into a dedicated crate only after **multiple crates** need it." Only `oneterm-vt` will ever consume the parser. |
| `docs/agents/structure.md` § 4 | "Open an issue / TODO before adding a crate beyond those in §3." Two crates are already being added; a third needs a consumer, and there is none. |
| `docs/agents/code-style.md`, "Public APIs" | "Prefer `pub(crate)` over `pub`." A parser crate forces `pub` on `Params`, `ParamSep`, `OscAccumulator`, the state enum and the dispatch trait — an API surface with one caller. |
| R10 (crate-dependency-rules) | New shared types go in the lowest crate that needs them. The parser's types are needed by exactly one crate, which is already the lowest. |
| R11 | Package name = `oneterm-<dir>`, and every new crate is a member row plus a manifest, a lints block, a `verify-dependency-graph.py` entry and two doc rows. That cost buys nothing here. |

The requirement the owner actually stated — "design the dispatch layer so the state machine is
a replaceable module" — is a code-structure requirement, not a crate requirement, and is met by
a module boundary: `parser` knows nothing about the grid and emits only `Action` values through
a narrow internal `Dispatch` trait, so it can be replaced (or wrapped around a different state
machine) without touching `dispatch/`. `vte` stays available as a **dev-dependency oracle** of
`oneterm-vt` for the differential tests, never as a runtime dependency.

`oneterm-pty` **is** a separate crate, because it has three consumers with different needs:
`crates/local-shell` (PTY plus grid), `crates/tools` (PTY, no grid), `crates/ssh` (grid, no
PTY). That is exactly the "multiple crates need it" trigger the style guide names, and it is
also why it is extracted first, before any engine work.

### Rule impact

- **R1 / R2** — both new crates are leaves in L0. No cycle, no upward edge.
- **R3** — unchanged: no UI crate gains a backend edge. `terminal-view` still reaches the
  engine only through `crates/terminal`.
- **R6** — `core` still depends on neither engine crate. The rule's wording
  ("no `alacritty_terminal`") is renamed to "no `oneterm-vt`, no `oneterm-pty`".
- **R7** — "the engines are gpui-free" now covers four crates: `terminal`, `vt`, `pty`,
  `completion`, `highlight`. `oneterm-vt` and `oneterm-pty` additionally depend on no OneTerm
  crate at all, which is stricter than R7 requires.
- **R8** — `local-shell` gains `pty`, `ssh` gains `vt`. Both are lower-layer, protocol-free
  crates; the rule ("depend on only `core` + `terminal` + their protocol crates") is widened to
  name them.
- **`crates/tools`** stays outside the layering and may depend on `pty` and `vt` directly, as
  it does on `alacritty_terminal` today.

## Diagram

```text
                       ┌──────────────────────── crates/pty (oneterm-pty) ────────────────────────┐
  child process ──────▶│ ConPTY (bundled conpty.dll, else kernel32) | openpty   EventedPty/RW   │
                       └──────────────┬───────────────────────────────────────────────────────────┘
                                      │ bytes (64 KiB chunks, caller's poll loop)
  ssh channel ────────────────────────┤
                                      ▼
   ┌──────────────────────────── crates/terminal (adapter) ───────────────────────────────┐
   │  ShellEventLoop / ssh_main_task                                                       │
   │    lock = Arc<FairMutex<Terminal>>       demand: Arc<AtomicBool>  ◀── raised by view   │
   │    ┌──────────────────────────── under the lock ───────────────────────────────────┐  │
   │    │  events = term.feed(chunk, &mut batch)                                         │  │
   │    └───────────────────────────────────────────────────────────────────────────────┘  │
   │    drain batch  ─▶ OscRouter (OSC 7/9/9;4/9;7/133, clipboard policy, PtyWrite)         │
   │                 ─▶ SessionEvent::{Output, Title, Cwd, Bell, AgentStatus, …}            │
   └──────────────────────────────────────────┬────────────────────────────────────────────┘
                                              │
   ┌────────────────────── crates/vt (oneterm-vt) ────────────────────────────────────────┐
   │  parser/  Williams state machine ─▶ Action (print_str | csi | esc | osc | dcs | apc)  │
   │           memchr scan, batched print runs, streamed OSC/DCS with caps                 │
   │  dispatch/ modes, SGR, erase, scroll, DA/DSR/DECRQM/XTVERSION, OSC registry           │
   │  grid/    ring of Option<Row>, RowId(u64) absolute, dual-form rows, scroll regions    │
   │  cell/    Cell(u64) ─▶ StyleSet(u16) · ExtrasTable(u16) · GraphemeArena               │
   │  reflow/  tracking points + old→new remap; BottomAnchor | KeepViewportTop             │
   │  damage/  per-row seqno + dirty bit; scroll delta as a distinct event                 │
   │  graphics/ Sixel decoder, cell-anchored placements, release signal                    │
   └──────────────────────────────────────────┬────────────────────────────────────────────┘
                                              │  render_update(&mut RenderState)  (under lock)
                                              │    Unchanged | Partial{rows, scroll} | Full
                                              ▼
   ┌────────────────── crates/terminal-view (render) ─────────────────────────────────────┐
   │  RenderState::resolve(&Palette)   (outside the lock: style ids ─▶ colours, runs)      │
   │  plan_cache keyed by (RowId, seqno) ─▶ row_plan ─▶ shapes/glyphs/quads ─▶ paint       │
   │  graphics store keyed by GraphicId, evicted on VtEvent::GraphicReleased               │
   └───────────────────────────────────────────────────────────────────────────────────────┘
```

## UI Wireframe

N/A — no user-facing surface changes; the terminal grid must look and behave identically, which
is what the three reproduced GUI walks in the intake's acceptance prove.

## Public API sketch

Abridged. Authoritative signatures live in the twelve
[`low-level-design/`](low-level-design/) files.

```rust
// ─────────────────────────── identity and geometry ───────────────────────────
pub struct RowId(pub u64);            // a POSITION in the output stream (DEC-0015)
pub struct Pos { pub row: RowId, pub col: u16 }
pub struct Size { pub rows: u16, pub cols: u16 }
pub struct Viewport { pub top: RowId, pub rows: u16, pub cols: u16 }

// ─────────────────────────── engine ───────────────────────────
pub struct Terminal { /* Send, !Sync — the caller owns the lock */ }

impl Terminal {
    pub fn new(config: Config, size: Size) -> Self;

    /// Parse `bytes`, mutate the grid, append events to `batch`.
    /// Never blocks, never calls back, never panics on input. `now` is passed in so a
    /// replay is deterministic (R-11).
    pub fn feed(&mut self, bytes: &[u8], batch: &mut EventBatch, now: Instant) -> FeedStats;

    /// Copy changed rows as resolved style runs. Call with the lock held.
    pub fn render_update(&mut self, state: &mut RenderState, now: Instant) -> RenderUpdate;

    /// Resize with an explicit policy. Anchors move themselves; there is no
    /// tracking-point slice and no public remap table (R-31).
    pub fn resize(&mut self, size: Size, policy: ResizePolicy) -> ResizeOutcome;

    // viewport and rows
    pub fn viewport(&self) -> Viewport;
    pub fn scroll_viewport(&mut self, delta: i32);      // negative = towards history
    pub fn scroll_to_bottom(&mut self);
    pub fn history_len(&self) -> u32;
    pub fn lines_produced(&self) -> u64;                // OUTPUT lines, the gutter's counter (R-05)
    pub fn row(&self, id: RowId) -> Option<RowRef<'_>>;
    pub fn row_range(&self) -> Range<RowId>;
    pub fn row_text(&self, id: RowId, out: &mut String);

    // anchors — the one mechanism that survives scrolls and reflow (DEC-0015, R-02)
    pub fn anchor_register(&mut self, kind: AnchorKind, pos: Pos) -> AnchorId;
    pub fn anchor_get(&self, id: AnchorId) -> Option<Pos>;
    pub fn anchor_release(&mut self, id: AnchorId);

    // cursor, modes, colours
    pub fn cursor(&self) -> Cursor;
    pub fn modes(&self) -> ModeSnapshot;                // also carried in RenderState (R-17)
    pub fn color(&self, key: ColorKey) -> Option<Rgb>;
    pub fn set_theme_colors(&mut self, theme: &ThemeColors);
    pub fn set_cell_pixels(&mut self, w: u16, h: u16);  // for CSI 14 t; one owner (R-40)

    // selection — designed in low-level-design/selection.md (R-18)
    pub fn selection_start(&mut self, pos: Pos, side: Side, kind: SelectionKind);
    pub fn selection_update(&mut self, pos: Pos, side: Side);
    pub fn selection_range(&self) -> Option<SelectionRange>;   // O(1), no text
    pub fn selection_text(&self) -> Option<String>;
    pub fn selection_clear(&mut self);
    pub fn select_all(&mut self);
    pub fn hit_test(&self, viewport_row: f32, col: f32) -> (Pos, Side);

    // graphics — ONE drain owner, the adapter (R-16)
    pub fn take_graphics(&mut self) -> Vec<Arc<GraphicData>>;

    // test + debug
    pub fn snapshot_text(&self) -> String;
    #[cfg(debug_assertions)] pub fn assert_integrity(&self);
}

// ─────────────────────────── events ───────────────────────────
pub struct EventBatch { /* one reusable byte arena; cleared per batch */ }
pub enum VtEvent {
    Repaint, Title(StrSpan), TitleReset, Bell,
    ClipboardStore { selection: ClipboardKind, text: StrSpan },
    ClipboardLoad  { selection: ClipboardKind },
    Reply(ByteSpan),                               // drained BEFORE any yield (R-37)
    ColorQuery { key: ColorKey, terminator: StringTerm },
    ScreenCleared,
    Osc { code: u32, params: ParamSpans, terminator: StringTerm, truncated: bool },
    RowsScrolled { top: RowId, bottom: RowId, delta: i32 },   // in-region motion (R-02)
    RowsTrimmed { oldest: RowId },
    GraphicReleased(GraphicId),
}

// ─────────────────────────── render hand-off ───────────────────────────
pub enum RenderUpdate { Unchanged, Partial { scrolled: i32 }, Full }
impl RenderState {
    pub fn rows(&self) -> &[RenderRow];    // ALWAYS the full viewport (R-15)
    pub fn changed(&self) -> &[u16];       // viewport indices to rebuild
    pub fn modes(&self) -> ModeSnapshot;   // (R-17)
    pub fn placements(&self) -> &[Placement];
    pub fn map_colors(&mut self, palette: &Palette);   // outside the lock; styles are already
}                                                      // resolved values, never ids (R-14)
```

Four shapes deliberately absent, each because the survey or the review showed a cost with no
OneTerm consumer: vi mode and vi motions; regex scrollback search; the engine's own PTY event
loop, notifier and message enum; and `VtEvent::Passthrough` with its echo buffer and `Handled`
return (R-35).

## Threading and locking

| Actor | Holds | Rule |
| --- | --- | --- |
| Pump thread (local poll loop / ssh tokio task) | `FairMutex<Terminal>` for the duration of one `feed()` call | Chunks are capped at 64 KiB. Between chunks it checks the demand flag and releases. |
| GPUI main thread (render) | the same lock, for `render_update` only | `render_update` copies only changed rows; `resolve` runs after the guard is dropped. |
| Any other consumer (search, gutter, agent panel) | the same lock, briefly | Each holds its own watermark; nobody clears damage for anyone else. |

- `oneterm-vt` itself contains no lock, no atomic and no interior mutability. `Terminal: Send`,
  `Terminal: !Sync`. The adapter chooses the synchronisation, which is why the engine can be
  unit-tested and fuzzed with no runtime.
- The lock is `parking_lot::FairMutex` (already in `Cargo.lock`). `lease()` — the one alacritty
  primitive `parking_lot` lacks — has no call site in OneTerm today
  ([`research/api-surface.md`](research/api-surface.md) § 3.9).
- Fairness alone is not enough under sustained output, because a thread that unlocks and
  immediately relocks beats a sleeping waiter. The render path raises an `Arc<AtomicBool>`
  demand flag; the pump tests it at every chunk boundary and yields
  ([`research/prior-art.md`](research/prior-art.md) § 2.3). The 64 KiB chunk cap stays as a
  backstop, the same role `MAX_LOCKED_READ` plays today.
- **Reply bytes leave before any yield (R-37).** The pump drains the batch in order — `Reply`
  first, then everything else — and only then tests the demand flag. Conhost blocks for up to one
  second waiting for the DA1 answer at session start, which is exactly when a burst is arriving.
- **No callback ever runs while the engine holds anything.** `feed()` returns events; there is
  no `EventListener`. This deletes the deferred/reliable two-tier machinery in
  `crates/terminal/src/backend/pump.rs:163-178` and the deadlock class it guards against.

## Data ownership

| Owner | Owns | Lifetime |
| --- | --- | --- |
| `Terminal` | cells, rows, interned styles / extras / graphemes, tab stops, modes, colour overrides, title stack, keyboard flag stack, decoded graphics pixels not yet drained, graphics placements | the session |
| `EventBatch` (caller-owned, reused) | one byte arena backing every `StrSpan` / `ByteSpan` in the batch | cleared at the start of each `feed()` |
| `RenderState` (caller-owned, reused) | copied rows, style-run cache, its own watermark | until the consumer drops it; invalidated to `Full` on resize, alt swap and reflow |
| Embedder (`crates/terminal`) | the mutex, the demand flag, OSC interpretation, the clipboard policy, session logging, key and mouse encoding | the session |
| `crates/terminal-view` | `RenderImage` GPU tiles keyed by `GraphicId`, the row-plan cache keyed by `(RowId, seqno)` | evicted on `GraphicReleased` / on watermark advance |

Row identity is assigned by the engine. A `RowId` keeps naming the same content across
scrollback pushes, `RIS`, `ED 2`, `ED 3` and viewport scrolling — **not** across `IL`, `DL`,
`SU`, `SD`, an in-region scroll or reflow, which copy content between fixed ids. Anchoring is
therefore an engine service: one tracked-anchor list holds the saved cursor, both selection
anchors, every graphics placement, every mark and the viewport top, and every row-moving
primitive plus reflow updates it. A consumer registers an anchor and reads it back; it never
stores a `RowId` and assumes the content stayed (`DEC-0015`).

## Memory model and caps

Every limit below is a named constant with a test, and every overflow **truncates or degrades
rather than erroring**, because all of this input is untrusted.

| Structure | Size | Cap | Overflow behaviour |
| --- | --- | --- | --- |
| `Cell` | 8 B packed | — | `Cell(0)` is a valid empty cell (space, default style, no extras) |
| Row | `RowHeader` (24 B) + `Vec<Cell>` (`8 * cols`) — **one representation** (R-51) | — | dual-form rows deferred to a later packet, gated on the tier-5 RSS numbers |
| Row slot | `Option<Row>`, `None` until first written — about **48 B per slot**, and the whole ring is allocated in `Terminal::new` (N-10): under 1 MB at the default 10 000-row scrollback, about 50 MB at `SCROLLBACK_MAX` | ring length = `next_power_of_two(scrollback_limit + MAX_ROWS)`, **fixed for the session** (R-30) | oldest row dropped, `RowsTrimmed` emitted |
| Viewport | — | `MAX_ROWS = 1024`, `MAX_COLS = 2048` (N-11) | a larger resize is clamped, so the ring mask stays valid |
| Scrollback | default 10 000 rows (unchanged, user-settable) | `SCROLLBACK_MAX = 1_000_000` | clamped at config load; changing it at run time rehomes the ring once, off the resize path |
| `StyleSet` | `u16` id per **terminal** (R-20) | 65 535 entries | fall back to the default style (id 0), `warn` once. **No sweep** (R-52), so an id never moves |
| `ExtrasTable` | `u16` id per terminal | 65 535 entries | one entry per image plus one per hyperlink, not one per cell (R-21); same fallback |
| `GraphemeArena` | `(offset, len)` into one `Vec<char>` | `GRAPHEME_MAX_LEN = 16` codepoints; sweep at 65 536 entries or 1 MiB of chars (R-27) | extra codepoints dropped; GC by remap over rows flagged `HAS_GRAPHEME` |
| Tracked anchors | one `Vec` entry each | bounded by the live anchors (cursor, saved cursor, 2 selection, per image, per visible mark) | an anchor in a blanked range dies |
| OSC payload | inline `[u8; OSC_INLINE = 2048]` | `OSC_LARGE = 8 MiB`, only for numbers the embedder marked large | truncate, set `truncated`, still dispatch |
| DCS / APC payload | never buffered — streamed to the sink | `DCS_MAX_BYTES = 16 MiB` per sequence | abort, count in `FeedStats`. Binding constraint for Sixel: 16 MiB of payload cannot produce the 64 MiB pixel clamp (R-55) |
| Sixel image | RGBA8 | `MAX_DIMENSION = 4096` per axis | clamp |
| Title stack | `Vec<Option<String>>` | `TITLE_STACK_MAX = 16` | drop the oldest |
| Kitty keyboard stack | fixed `[Flags; 8]` | 8 | push wraps; `pop(n >= len)` resets |
| `EventBatch` arena | one `Vec<u8>` | `EVENT_ARENA_SOFT = 1 MiB`, shrunk after a larger batch | further payloads truncate |
| `RenderState` | full viewport of `RenderRow` + a `changed` list | bounded by the viewport | reused; resolved style runs cost ~20 B per run for changed rows only |

Structural effect versus today: 24 B per cell with an `Arc<CellExtra>` heap allocation and
refcount per decorated cell becomes 8 B per cell with two `u16` ids and no per-cell allocation,
and a 100 000-row scrollback that is mostly empty costs 100 000 `Option<Row>` slots instead of
100 000 fully materialised rows ([`research/prior-art.md`](research/prior-art.md) § 9.1, § 9.2).
This is a design property, not a claim; the RSS tier of the benchmark measures it.

## Data flow, byte to pixel

1. **Read.** `oneterm-pty` (or the ssh channel) fills a 64 KiB chunk of a reusable buffer. The
   poll loop is the caller's, as it is today.
2. **Lock.** The pump takes `FairMutex<Terminal>`. If the demand flag is set it yields first.
3. **Parse and apply.** `Terminal::feed(chunk, &mut batch)`. The parser scans with
   `memchr3(0x1B, 0x0A, 0x0D)`, validates each printable run as UTF-8 once, and hands whole
   runs to `dispatch::print_str`. The print path takes a run-length fast path when nothing
   unusual is enabled (no insert mode, no charset translation, no open hyperlink, no image on
   the row) and the general per-character path otherwise. Each mutated row gets the batch
   sequence number and its dirty bit. Control sequences go through the dispatch tables.
   Unregistered OSC numbers and unhandled sequences are dropped and counted in `FeedStats`, as
   the engine being replaced drops them.
4. **Unlock and drain.** The pump drops the guard, then walks `batch`: OSC 7 / 9 / 9;4 / 9;7 /
   133 through the existing `OscRouter`, clipboard through the existing security policy,
   `Reply` bytes into the transport, `Repaint` into one coalescible `SessionEvent::Output`.
   Exactly as today, except that nothing runs under the lock.
5. **Render, phase 1 (locked).** GPUI prepaint takes the lock and calls `render_update`. Rows
   whose sequence number exceeds the render state's watermark are copied into the render
   state's arena; a pure scroll reports a delta instead of N changed rows; an unchanged frame
   returns `Unchanged` and the element skips layout and paint entirely. While mode 2026 is open
   the call returns `Unchanged` until the closing sequence or the 150 ms timeout, with a 1 s
   watchdog.
6. **Render, phase 2 (unlocked).** `RenderState::resolve(&palette)` expands interned style ids
   into concrete colours and style runs. A rebuilt row that produced identical runs skips the
   per-cell style fill, which is the common case because text changes far more often than
   styling.
7. **Paint.** `plan_cache` keys on `(RowId, seqno)` instead of on a hash of a copied row, so a
   scroll shifts the cache rather than invalidating it. Graphics are painted from the store,
   keyed by `GraphicId`, evicted on `GraphicReleased`.

## How today's consumers map onto the new API

| Today | File:line | Becomes | Deleted? |
| --- | --- | --- | --- |
| `TerminalContent::refill` clones every visible cell | `crates/terminal/src/content.rs:173-222` | `Terminal::render_update` into a reusable `RenderState`; `TerminalContent` becomes a thin view over it | the clone loop, yes |
| `Term::damage()` + `reset_damage()` | `content.rs:171-193` | per-row seqno + a watermark inside `RenderState` | yes |
| `TerminalContent.mode: TermMode`, read at paint time | `content.rs:97`, `crates/terminal-view/src/render/frame.rs:564` | `ModeSnapshot` in the render state, refreshed every update (R-17) | the lock-at-paint hazard, yes |
| `TermDamageInfo` display-line conversion | `content.rs:66-106` | rows carry `RowId`; no conversion | yes |
| `resize_keeping_viewport_top` + `conhost_cursor_row` (parked alt grid, placeholder `Grid`, double `swap_alt`, scratch probe) | `crates/terminal/src/model.rs:481-541` | `Terminal::resize(size, ResizePolicy::KeepViewportTop)` — anchors move themselves, so there is no tracking-point argument (R-31) | **yes, 61 lines** |
| `LineAccounting::observe` and its newline rescan (PERF-19) | `crates/terminal/src/backend/line_accounting.rs:1-49` | `Terminal::lines_produced()` — output lines, the same meaning it has today (R-05) | **yes, the whole file** |
| `OscRouter` as an `EventListener` firing under the lock; `SessionEventSink` deferred/reliable tiers | `crates/terminal/src/backend/osc_router.rs:214-268`, `pump.rs:163-178` | `OscRouter` becomes a plain function over the drained `EventBatch`; the deferred tier disappears | the two-tier machinery, yes |
| `Event::Osc { params: Vec<Vec<u8>> }` deep copy per forwarded OSC | vendor patch 0002; consumed `osc_router.rs:249-257` | `VtEvent::Osc` with spans into the batch arena | yes |
| `Event::ColorRequest` queued during the batch, answered after it | `pump.rs:105-128` | `VtEvent::ColorQuery { key: ColorKey, .. }` — typed key, still answered after the batch | the 256/257/258 magic indices, yes |
| `osc_color.rs` index space 0..=255 / 256 / 257 / 258 | `crates/terminal/src/osc_color.rs:23-27` | `ColorKey::{Palette(u8), Foreground, Background, Cursor, BrightForeground, DimForeground, Dim(u8)}` | the constants, yes |
| `NamedColor` discriminant arithmetic in two crates | `crates/terminal/src/palette.rs:136`, `crates/terminal-view/src/render/frame.rs:118`, `:121` | `NamedColor::dim_index()` / explicit mapping | yes |
| Search over a copied `GridText` snapshot | `crates/terminal/src/search.rs:70-148` | **`GridText` stays**, rebuilt adapter-side from `row_text` over `row_range()` under one lock, then matched unlocked exactly as today (R-19); matches carry `RowId`, so `display_row(display_offset)` disappears | the offset conversion, yes |
| URL detection and the URL policy, reading the same snapshot | `crates/terminal/src/url.rs`, `url_policy.rs` | the same `GridText` (R-19) | nothing |
| `input/mouse.rs`, `input/mouse_tests.rs`, `theme/palette.rs` importing engine types | `crates/terminal-view/src/…` | `SelectionKind`, `ModeSnapshot` and the engine's `Rgb` (R-26: these are the other three files above the seam, not just `frame.rs`) | the imports, yes |
| `search.rs` topmost/bottommost `Line` span | `search.rs:41-52`, `:82-92` | `Terminal::row_range()` | yes |
| `frame.rs` display-offset fallbacks (dense + binary-search) | `crates/terminal-view/src/render/frame.rs:514-532` | rows arrive with `RowId`; there is no non-dense case | **yes, both** |
| `frame.rs` engine-type conversions (`Cell`, `Flags`, `Color`, `CursorShape`, `Hyperlink`) | `frame.rs:11-16`, `:208-233`, `:298-319` | `RenderRow` already carries the view's shapes; the conversion layer shrinks to colour resolution | mostly |
| `logging.rs` second `vte::Parser` just to strip escapes | `crates/terminal/src/logging.rs:8`, `:57-83` | `oneterm_vt::strip::EscapeStripper` (about 60 lines, shares the parser module) | the second parser, yes |
| `local-shell` PTY imports, `PTY_CHILD_EVENT_TOKEN` hard-coded as `1`, two cfg'd child-pid functions | `crates/local-shell/src/event_loop.rs:58-64`, `:168-179` | `oneterm_pty::{PTY_CHILD_EVENT_TOKEN, PTY_READ_WRITE_TOKEN}`, `PseudoConsole::child_pid()` | the workarounds, yes |
| `ssh` uses `alacritty_terminal` only for `Term` + `FairMutex` | `crates/ssh/src/session.rs:25-26`, `:56`, `task.rs:8-9` | `oneterm_vt::Terminal` + `parking_lot::FairMutex` | the dependency, yes |
| `tools/src/bin/pty-throughput.rs` | `crates/tools/src/bin/pty-throughput.rs:21-22` | `oneterm-pty` | the engine dependency, yes |
| `mock_term`, `TermSize`, `VoidListener` test helpers | `content.rs`, `model.rs`, `search.rs`, `sixel_tests.rs` | `oneterm_vt::testing::{terminal_from_text, feed}` — not `cfg(test)`-gated, so downstream crates use it without a feature flag | replaced |
| `test_support.rs` fake session (fabricates `TerminalContent` directly) | `crates/terminal/src/test_support.rs:1-662` | same role, rebuilt on `RenderRow` | rewritten, not deleted |
| `vendor/`, `vendor/patches/`, `vendor/refresh.sh`, its CI job, `[patch]` block, notices rows | `vendor/**`, `.github/workflows/ci.yml:65-68`, `Cargo.toml:240-244`, `scripts/third-party-notices.py:85-86` | nothing | **yes, all of it** |

## Risks

From [`research/prior-art.md`](research/prior-art.md) § 10, restated with this design's
mitigation and where the mitigation is verified.

| # | Risk | Mitigation in this design | Verified by |
| --- | --- | --- | --- |
| 1 | Rewrite 12-13k LOC and land behind where we started | Phase plan whose every exit criterion is a green test; the 45-recording parity gate; the differential old-versus-new runner running through the whole migration; the benchmark baseline recorded in `US-0072` **before** any engine code exists | `US-0072` exit, `US-0076` exit, `US-0079` exit |
| 2 | Reflow correctness (six open alacritty bugs; a "known to fail for an unknown reason" guard in xterm.js; a documented deadlock path in Windows Terminal) — and OneTerm has a second contract in conhost's quirks | Port avt's Apache-2.0 iterator with attribution; tracking-point slice and old-to-new remap in the API from day one; proptest round-trip properties; `KeepViewportTop` native with the existing `keep_viewport_top_*` tests as its contract | `reflow-and-resize.md` |
| 3 | Grapheme and width model is decided early and regretted | Decided here, not during implementation: intern with GC-by-remap, cap at 16 codepoints, `unicode-width` + `unicode-segmentation` (both already in the lock), mode 2027 designed in, and the ConPTY `PSEUDOCONSOLE_GLYPH_WIDTH_*` axis set from the same mode | `cell-and-style.md` |
| 4 | `rio-vt` licence chain | Not adopted (`DEC-0014`); design prior art only | `DEC-0014` |
| 5 | OSC 9;7 collides with ConEmu's "run some process" sub-code | Out of scope by owner decision; the OSC registration table makes either resolution a one-line change. Raised as its own packet against `docs/osc-agent-status.md` | `IN-0029.md` open decisions |
| 6 | Unbounded buffers reachable from any SSH session (OSC payload, per-cell zero-width list, `Row::new(0)`) | Designed out: bounded OSC with truncation, `GRAPHEME_MAX_LEN = 16`, no unsafe row allocation. Fuzz target with an RSS limit. The **existing** fork keeps the defect until `US-0079`; that exposure is an explicit owner decision | `parser.md`, `cell-and-style.md`, `testing-and-bench.md` |
| 7 | `libghostty-vt` keeps looking like the answer | Rejected with reasons in `DEC-0014` so it is not relitigated | `DEC-0014` |
| 8 | Optimising the parser because it is the legible part, while the grid costs 3-8x and neither is a bottleneck | Tier 3 (parse + grid + one render-state build per frame) is the primary metric from day one; the resize tier, which the local baseline never covered, is mandatory; every report prints the ConPTY transport ceiling next to the engine number | `testing-and-bench.md` |
| 9 | Lock starvation under sustained output | Explicit demand/yield handshake plus the 64 KiB chunk cap; a "frame time under `yes`" measurement in the bench tiers | `damage-and-render-state.md` |
| 10 | Scope creep through the extension points | Strict packet sequencing: graphics only at `US-0080`, extension points only at `US-0081`, Kitty graphics / kitty keyboard / win32-input-mode / mode 2048 / mode 2031 explicitly out of scope | `IN-0029.md` packet list |
| 11 | Windows keyboard gaps (win32-input-mode, kitty keyboard over ConPTY) | Out of scope; the engine owns the flag **state** and exposes it, the app owns the encoding, so the later intake does not need engine changes | `dispatch-and-modes.md` |
| 12 | Dependency-policy friction | Six new direct declarations, all already in `Cargo.lock`; no new crate enters the graph; `docs/agents/dependencies.md` § 3 updated | `US-0082` |
| 13 | The PTY layer disappears with the engine | Extracted **first**, as `US-0071`, with no dependency on the engine work | `US-0071` exit |
| 14 | Losing behaviour nobody can name | The 45 recordings are real tmux / vim / zsh / fish captures replayed cell by cell, plus the differential runner over a captured OneTerm session | `US-0076`, `US-0079` |
| 15 | On Windows the engine is demonstrably not the bottleneck, so a speed-justified rewrite is unfalsifiable | The intake forbids a throughput outcome; the case is extension cost, stated in `DEC-0014` and in this HLD's first section | `IN-0029.md` acceptance |

Three risks this design adds that the research did not list, each already mitigated in the text:

| # | Risk | Mitigation |
| --- | --- | --- |
| 16 | The grapheme GC is a new failure class: a missed live reference corrupts text silently | GC by remap walks both screens, skipping rows without `HAS_GRAPHEME`; `assert_integrity` checks every live id resolves. The **style** sweep that carried the same risk is deleted (R-52), so style ids never move and a render copy can never be invalidated by table maintenance |
| 17 | `RenderState` is stateful and can go stale (resize, alt swap, reflow, palette change) | every one of those returns `Full` and bumps a generation the state compares; a debug assertion checks the watermark never moves backwards |
| 18 | The tracked-anchor list must be updated by **every** row-moving primitive; one that forgets produces a silently misplaced mark, selection or image | it is the single mechanism (reflow uses it too, so the reflow property tests exercise it), every primitive's table row names its `shift_region` call, and a debug assertion checks every live anchor is inside the live row range |

## Phase plan

One phase per packet, in dependency order. Exit criteria are commands and green tests, not
judgements, and **no exit criterion is a performance number** (R-29).

| Ph | Packet | Outcome | Exit criteria |
| --- | --- | --- | --- |
| 0 | `US-0071` | `oneterm-pty` extracted | `cargo test -p oneterm-pty` green including the loopback contract from `crates/local-shell/src/event_loop_tests.rs:270-332`; `grep -rn "alacritty_terminal::tty" crates/` empty; the bundled-host resolution order proven by `conpty_api_prefers_the_bundled_host`; a local shell opens, resizes and exits on Windows; `structure.md`, the graph allow-list and `dependencies.md` § 3 updated **in this packet** (R-46) |
| 0b | `US-0072` | Benchmark and parity harness | All 45 recordings vendored with attribution; **both expectation files blessed by the OLD engine** and frozen (R-58); `vt-corpus cross-check` shows no loss against upstream `grid.json` (R-57); `vt-corpus grep-deviations` has filled every "measure in `US-0072`" cell (R-53); five bench tiers recorded for the old engine; `vt-diff` old-against-old clean; the Windows bench job exists in CI and records without gating (R-60) |
| 1 | `US-0073` | Parser core | `cargo test -p oneterm-vt parser::` green; the differential test against the raw `vte` state machine agrees on all 45 recordings and the fuzz seeds, modulo P1-P7; `oracle_precondition_patches_do_not_touch_the_state_machine` green (R-41) |
| 2 | `US-0074` | Cell, style, grapheme | `size_of::<Cell>() == 8`; the style ladder driven to exhaustion with **no id ever moving**; the grapheme GC driven and proven content-preserving; one extras entry per image (R-21) |
| 3 | `US-0075` | Grid, scrollback, anchors | The viewport-offset table proven row by row (R-01); all four `scroll_up` cases including the bottom-bounded region (R-03); the two screens' id ranges disjoint (R-04); `lines_produced` matches today's gutter (R-05); anchors move with content through every primitive (R-02); corrections **C1-C4** implemented spec-correct with their expected differences declared; the debug-suite runtime budget measured (R-28) |
| 4 | `US-0076` | Dispatch and modes | **The 45-recording parity gate is green** against the frozen expectations, with every difference covered by a declared cell-level window naming a correction and no stale window (`cargo test -p oneterm-vt --test ref_corpus`); corrections **C5-C11** implemented; every sequence marked supported in `docs/osc-sequences-checklist.md` has a byte-feed test; `? 9001` accepted silently (R-36); `AppKeypad` reported (R-64); mode 2027 recognised and inert (R-56) |
| 5 | `US-0077` | Reflow and resize | The ten `keep_viewport_top_*` behaviours reproduced as engine tests **while the old suite still runs against the old engine** (R-44); seven proptest properties green over 10 000 cases; `measure_rows` fixtures carry their host version (R-39); resize recorded at three scrollback depths as a ratio, not a target (R-29) |
| 6 | `US-0078` | Selection | The invalidation matrix proven row by row; anchors follow a region scroll; `selection_range()` allocation-free; block extraction and semantic expansion tested (R-18) |
| 7 | `US-0079` | Damage, render state, events | `rows()` always full plus a `changed` list (R-15); style runs carry resolved values and survive a grapheme sweep (R-14); `ModeSnapshot` refreshed on every update (R-17); `RenderState` never drains graphics (R-16); mode 2026 deterministic with an injected clock (R-11) |
| 8 | `US-0080` | Graphics in the engine | The ten `sixel_tests.rs` behaviours reproduced; one extras entry per image; a placement moves with an in-region scroll; `GraphicReleased` fires on `CSI 2 J`, on a row reset and on a trim (R-22); `vt-diff` green over the Sixel recordings. **Engine-level only — IN-0028's evidence walk moves to `US-0081`, because the new engine is not behind the application until the shim (N-02)** |
| 9 | `US-0081` | **Engine behind the seam (shim)** | `cargo test --workspace` green with `LegacySnapshot` producing today's `TerminalContent`; `vt-diff` zero divergence over 45 recordings plus a captured session; the IN-0018, IN-0027 **and IN-0028** GUI walks reproduced (N-02); scope bounded by the table in `migration.md` — all of `crates/terminal`, and in the two backends only the shared-terminal type, its construction and the manifest line, with no backend test changed (N-04) |
| 10 | `US-0082` | `crates/terminal` goes native | `RowId`, batch drain, `RenderState`, `ColorKey`; `model.rs:481-541`, `line_accounting.rs` and the deferred event tier deleted; the old and new resize suites both green in the same commit, then the old one deleted (R-44); `GridText` kept adapter-side (R-19) |
| 11 | `US-0083` | `crates/local-shell` goes native | the read loop drains the `EventBatch`, `ResizePolicy` is selected through the engine API, the `oneterm-pty` tokens replace the last local constants, the `alacritty_terminal` manifest line is deleted (N-04); `cargo test -p oneterm-local-shell` green; `local_session_grow_policy_matches_conpty` unchanged |
| 12 | `US-0084` | `crates/ssh` goes native | the same three for the tokio task plus `BottomAnchor` selection and the manifest line (N-04); `cargo test -p oneterm-ssh` green; `ssh_session_keeps_the_default_grow_policy` unchanged; no `alacritty_terminal` dependency left |
| 13 | `US-0085` | `crates/terminal-view` goes native | `frame.rs`, `plan_cache`, `input/mouse.rs`, `theme/palette.rs` on `RenderRow` / `SelectionKind` / `ModeSnapshot`; both display-offset fallbacks and `engine_shim.rs` deleted; GUI walks re-run |
| 14 | `US-0086` | Additive features | The four capabilities the reference never had and no recording exercises: DECXCPR (D7), XTVERSION (D8), `modifyOtherKeys` reporting (D10) and reverse wrap `? 45` (D12). **Behaviour corrections are no longer deferred here** — the owner's correctness-first ruling put them in `US-0075` and `US-0076` with declared expected differences |
| 15 | `US-0088` | Agent protocol off OSC 9;7 | A free OSC code chosen from a survey of the xterm / iTerm2 / ConEmu / WezTerm / kitty assignments; `9;7` kept as a deprecated alias for one release; `docs/osc-agent-status.md`, the registration in `crates/terminal` and the completion catalogs updated; the agent panel unchanged in behaviour |
| 16 | `US-0087` | Decommission | `vendor/` absent; no `refresh.sh` CI job; no `[patch]`; the `vte` dev-oracle and `vt-diff` deleted; `python scripts/third-party-notices.py --check`, `check-doc-paths.py` and `pwsh scripts/ci-local.ps1` green; every owning doc reconciled |

## Detail Design

- [x] Detail design: required (high-risk) — twelve files under
  [`low-level-design/`](low-level-design/), one per concern:
  [`parser.md`](low-level-design/parser.md),
  [`cell-and-style.md`](low-level-design/cell-and-style.md),
  [`grid-and-scrollback.md`](low-level-design/grid-and-scrollback.md),
  [`selection.md`](low-level-design/selection.md),
  [`reflow-and-resize.md`](low-level-design/reflow-and-resize.md),
  [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md),
  [`dispatch-and-modes.md`](low-level-design/dispatch-and-modes.md),
  [`graphics.md`](low-level-design/graphics.md),
  [`events-and-api.md`](low-level-design/events-and-api.md),
  [`testing-and-bench.md`](low-level-design/testing-and-bench.md),
  [`migration.md`](low-level-design/migration.md),
  and [`pty.md`](low-level-design/pty.md).
- Reason: the lane is high risk and the work spans months and many agents. Each file is written so
  an implementer can code from it without re-deriving the semantics from the research notes, and
  each names the tests that prove it. `testing-and-bench.md` carries the master table mapping all
  48 trap-list items from
  [`research/engine-semantics.md`](research/engine-semantics.md) § 8 to an owning file and a test
  name.

> Template note (R-50): this document's data-flow section is titled "Data flow, byte to pixel"
> where `docs/templates/design.md` says "Data Flow", and the section order is otherwise the
> template's. The deviation is deliberate and recorded so a generator does not silently rewrite it.
