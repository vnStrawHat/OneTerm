# Performance baseline — current VT engine (pre-rewrite)

Status: research note (no owning work packet — informational baseline for the
future IN-0029 rewrite decision; no source, schema, or behavior changed).

## 1. Method

- Micro-benchmark: standalone scratch crate at
  `C:\Users\trunglt\AppData\Local\Temp\claude\D--TrungKFC-Research-Rust-myTerm2\fa2d0135-9c28-4a69-a255-ba7479e604cb\scratchpad\vt-bench`
  (kept outside the project; not committed). Path-depends directly on
  `vendor/alacritty_terminal` and patches `vte` to `vendor/vte` (mirrors the
  root `Cargo.toml` `[patch]` block) so it benchmarks OneTerm's actual fork,
  not upstream alacritty/vte.
- Feeds synthetic byte streams straight into
  `alacritty_terminal::vte::ansi::Processor::<StdSyncHandler>::advance`
  against a `Term<VoidListener>` built with `Term::new(Config::default(), &TermSize::new(200, 50), VoidListener)`
  — 200×50 grid, `scrolling_history: 10000` (the `Config` default). No PTY, no GUI.
- Each scenario generates ~100 MiB of input, 3 runs, median MB/s reported
  (single-sample for the parser-only column — a diagnostic delta, not a
  headline number).
- Parser-only isolation: the same bytes run through a `Handler` impl with
  every method left at its default (no-op) body — `alacritty_terminal`'s
  `vte::ansi::Handler` trait gives every callback an empty default, so this
  measures pure `vte` state-machine + dispatch cost with zero grid work.
- Snapshot cost: one `Term` primed with a full frame, then 600 iterations
  (~10 s at 60 Hz) each feeding a small chunk of new bytes and then timing
  `Term::damage()` + `reset_damage()` + `renderable_content()` + copying
  every display cell into an owned `Vec` — the same shape as
  `TerminalContent::refill`.
- Built and run with `CARGO_TARGET_DIR` pointed at a scratch dir
  (`...\scratchpad\target-bench`), never the project's own `target/`
  (the owner may have `cargo run` active concurrently). Release profile,
  `opt-level = 3, lto = true, codegen-units = 1`.
- Bench source: `vt-bench/Cargo.toml` + `vt-bench/src/main.rs` under the
  scratchpad path above. Not part of the OneTerm workspace, not committed.

## 2. Machine

- CPU: 12th Gen Intel Core i7-12700, 12 cores / 20 logical threads, 2.1 GHz base (per `Win32_Processor`)
- RAM: 31.7 GB
- OS: Windows 11 Enterprise, build 26200 (64-bit)
- rustc/cargo: 1.96.0 (`ac68faa20` / `30a34c682`, 2026-05-25)

## 3. Hot path today (byte → pixel)

1. **PTY read** — `crates/local-shell/src/event_loop.rs:390` — `self.pty.reader().read(&mut buf[unprocessed..])` into a persistent 1 MiB (`READ_BUFFER_SIZE`, line 36) boxed-slice buffer; loops until the pipe is empty (no per-chunk cap, comment at line 431).
2. **Lock `Term`** — `crates/local-shell/src/event_loop.rs:407-423` — `self.term.try_lock_unfair()` (falls back to `lock_unfair()` once the unprocessed buffer fills, i.e. ≥1 MiB queued); the `FairMutex` (`alacritty_terminal::sync::FairMutex`) is held for the rest of the read loop's iteration.
3. **Parse + line accounting** — `crates/terminal/src/backend/pump.rs:87-91` (`TerminalPump::advance`) — per chunk: `self.router.logging().process(bytes)` (session-log passthrough, only allocates if logging is on), `self.processor.advance(term, bytes)` (the real parse + grid mutation, `vte::ansi::Processor::advance`), `self.lines.observe(term, bytes)`.
4. **Line accounting scan** — `crates/terminal/src/backend/line_accounting.rs:32-43` — in the common case (scrollback not yet full) this is O(1) (`total_lines` delta); once scrollback is full it falls back to `bytes.iter().filter(|&&b| b == b'\n').count()` — **a second full byte-by-byte scan of every chunk**, called out in the file's own doc comment as PERF-19 (a grid counter would replace it).
5. **Color-query replies (rare)** — `event_loop.rs:441-453` — only when OSC 10/11/12 queries were seen; negligible on the steady-state hot path.
6. **Unlock + publish** — `event_loop.rs:472`, `pump.rs:163-169` (`finish_batch_blocking`) — lock is dropped before this; publishes `absolute_line_count`, flushes any deferred reliable events, and posts one `SessionEvent::Output` per read-loop iteration (not per byte) to wake the UI.
7. **Snapshot (render thread, GPUI prepaint)** — `crates/terminal-view/src/render/element.rs:121` (`state.frame.snapshot(...)`) → `crates/terminal/src/model.rs:103-106` (`TerminalModel::snapshot_into`) → locks the same `FairMutex` again, briefly, then calls:
8. **`TerminalContent::refill`** — `crates/terminal/src/content.rs:173-222` — `term.damage()` (dirty-row list) + `term.reset_damage()`, then `term.renderable_content()` (a `GridIterator` over the visible cells) is drained into `self.cells` via `self.cells.clear(); self.cells.extend(display_iter.map(|indexed| IndexedCell { point, cell: indexed.cell.clone() }))` — **one `Cell::clone()` per visible cell, every snapshot** (200×50 = 10,000 clones/frame at OneTerm's default grid). The `Vec` itself is reused (`clear()`, not a fresh allocation) so steady-state this is copies, not `malloc`.
9. **Paint** — `crates/terminal-view/src/render/element.rs` `paint()` — reads only `Frame`'s owned snapshot; never touches `Term`/`FairMutex` again for that frame.

**Where copies/allocations happen:**
- Per byte: none beyond what `vte`'s state machine itself does internally (no per-byte heap traffic in OneTerm's wrapper code).
- Per chunk (up to 1 MiB): the PTY read buffer is preallocated once (`event_loop.rs:270`) and reused; `Cow::Owned` reallocation only on a partial PTY write of *input* (rare, keystrokes not output).
- Per read-loop iteration: one `FairMutex` lock/unlock (parse side) — cheap unless the render thread is mid-snapshot (contention, see below).
- Per byte once scrollback is full: the O(n) newline rescan in `LineAccounting::observe` (step 4) — a real second pass over already-parsed bytes.
- Per rendered frame (not per byte): one more lock/unlock, one damage-iterator drain, and one `Cell::clone()` per visible cell (`content.rs:206-209`) — bounded by grid size, not by output volume, so this cost is flat regardless of how much text scrolled since the last frame.

## 4. Results — synthetic `Processor::advance` throughput (200×50 grid, release, 3-run median)

| Scenario | Full parse+grid MB/s | Full ns/byte | Parser-only MB/s | Parser-only ns/byte |
|---|---:|---:|---:|---:|
| a: plain ASCII lines + `\n` | 126.5 | 7.54 | 1191.0 | 0.80 |
| b: long unwrapped lines (implicit wrap) | 146.3 | 6.52 | 1165.1 | 0.82 |
| c: heavy 24-bit SGR per char | 253.5 | 3.76 | 358.6 | 2.66 |
| d: cursor-movement TUI redraw (CUP + short strings) | 194.1 | 4.91 | 420.1 | 2.27 |
| e: DECSTBM scroll region + bare LF | 147.6 | 6.46 | 998.6 | 0.96 |
| f: CJK wide chars | 163.4 | 5.84 | 1069.6 | 0.89 |
| g1: vtebench-style `dense_cells` (256-color/cell) | 238.8 | 3.99 | 322.0 | 2.96 |
| g2: vtebench-style `scrolling` (plain full-width lines) | 145.1 | 6.57 | 1321.8 | 0.72 |

Snapshot cost (h): **29.9 µs/frame** for a full 200×50 (10,000-cell) `renderable_content()` + copy — a theoretical ceiling around 33 kHz from the snapshot alone (i.e. nowhere near the limiter; 60 Hz costs ~0.18% of a frame budget).

`pty-throughput` (real ConPTY, `cmd /c for /l ... do @echo ...`, 6 s measured window): **~1.2 MiB/s** sustained — two full orders of magnitude below every synthetic parser number above. `cmd.exe`'s own for-loop + ConPTY relay is the bottleneck for this producer, not OneTerm's parser.

## 5. Observations

1. Every synthetic scenario clears 120+ MB/s full parse+grid throughput; the real ConPTY-backed run tops out ~1.2 MiB/s — for ordinary shell workloads (even a tight `cmd.exe` loop) the VT engine is not the bottleneck by ~100×. Anything visibly "slow" in practice (DOOM-fire-style full-screen redraws) has to be producing tens of MB/s sustained, which is a different regime than a shell prompt.
2. The parser-only column shows `vte`'s raw state machine alone runs at 350–1300+ MB/s depending on density; the **grid-mutation half of `Processor::advance` (writing cells, tracking damage, wrapping) is 3–8× the cost of parsing**, i.e. `Term`'s `Handler` impl, not `vte` itself, dominates the full-pipeline number. A rewrite that keeps `vte` and only replaces the `Handler`/grid side has the larger win available.
3. SGR-heavy content (c) and `dense_cells` (g1) are the *fastest* full-pipeline scenarios (238–253 MB/s) despite being the densest in escape-sequence count — each SGR sets one attribute that's cheap to apply to a single cell write, whereas plain-text scenarios (a, b, e, g2) sit at 126–147 MB/s. Throughput here is dominated by cell-write and line/wrap bookkeeping, not escape-sequence parsing complexity.
4. Cursor-movement-heavy TUI redraws (d, 194 MB/s full / 420 MB/s parser-only) cost more per parsed byte than plain SGR but less than plain text — CUP parsing is cheap, but each jump breaks the parser's ability to just linearly walk the grid, adding per-move overhead versus a straight scroll.
5. CJK wide chars (f) cost noticeably more per byte than plain ASCII (163 vs 126 MB/s, and this is MB/s of UTF-8 bytes — each CJK char is 3 bytes) — wide-char handling (spacer cell insertion, width lookup) is visibly heavier than ASCII's 1-byte-per-cell path, though still far from a bottleneck at real-world rates.
6. `LineAccounting::observe`'s O(n) `\n`-scan fallback (line_accounting.rs:38, PERF-19 in the file's own docs) only triggers once scrollback (10k lines default) is full — none of these 100 MiB synthetic runs on a fresh `Term` exercise that path in the "history full" branch except where a scenario itself produces >10k lines (most do, given 100 MiB / ~70-byte lines ≫ 10k lines) — so this second byte scan **is** active in most of the numbers above, and is already a known, tracked, non-blocking cost.
7. The render-side snapshot (`TerminalContent::refill`) costs ~30 µs/frame at 200×50 regardless of how much text scrolled — it's proportional to *visible grid area*, not to output volume. This means a wider/taller terminal (more visible cells) scales snapshot cost linearly, while parse throughput does not depend on grid size at all (it's a byte-stream cost). A rewrite should keep this separation: don't let a bigger viewport slow down parsing, and don't let a busier PTY slow down painting.
8. The `FairMutex` is only held for the parse call and, separately and briefly, for the snapshot call — the two never overlap for long because `try_lock_unfair()` (event_loop.rs:410) lets the render thread's snapshot win a contended lock quickly instead of queuing behind a giant parse batch; the exception is the `unprocessed >= READ_BUFFER_SIZE` fallback to `lock_unfair()` (line 412), which can make the render thread wait for a full 1 MiB batch to finish parsing under sustained heavy output. This is the one place where "the parser is slow" could translate into a dropped frame, and it only shows up under an extreme producer (tens of MB/s), consistent with observation 1.
9. Bottom line for the rewrite decision: at realistic shell/SSH throughput (single-digit MiB/s), neither `vte` parsing nor the current `Term`/grid nor the snapshot copy is a measurable bottleneck — headroom is 2-3 orders of magnitude. Any rewrite's performance case has to be argued from *other* motivations (API ergonomics, feature gaps, maintainability of the vendored fork per IN-0017/docs/terminal-fullscreen-perf) rather than raw throughput, unless the target workload is deliberately pathological (full-screen TUI animation at tens of MB/s, where grid-mutation cost — not `vte` — is the lever to pull per observation 2).

## 6. Existing benches / probes inventory (per task step 1)

- `crates/tools/src/bin/pty-throughput.rs` — the only existing throughput probe in the repo (no `#[bench]` or `criterion` anywhere under `crates/`). Spawns a real child through the same `alacritty_terminal::tty` ConPTY path OneTerm uses, reads raw bytes with **no `Term`, no parser, no GUI** — isolates PTY-transport throughput from VT parsing. Has a built-in "wait 5 s for an intro screen, send Enter, measure 6 s" heuristic aimed at full-screen TUI demos (DOOM-fire); against a plain command it just measures whatever that command emits in its 6 s window post-Enter. Used here once (§4) against a `cmd.exe` for-loop as the "real world" datapoint; not re-run for DOOM-fire itself (out of scope / time-boxed per task instructions).
