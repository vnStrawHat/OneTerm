# 7. RFC — Custom Single-Pass Terminal Engine

> **STATUS: SUPERSEDED PREMISE · KEPT AS ARCHITECTURE REFERENCE · NOT SCHEDULED.**
> This RFC was written as "Option C, path 3" from
> [`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.6: replace
> `alacritty_terminal` with an in-house parser + grid + renderer bridge doing the whole
> PTY→pixels pipeline in a single pass, originally justified as *"the only path that could
> genuinely raise the delivered throughput of the DOOM-fire workload."*
>
> ⚠️ **That justification no longer holds.** [`06`](06-results-and-ceiling.md) §6.7
> proved (four independent ways) that the end-to-end delivered throughput is bound by the
> **producer (DOOM-fire) + Windows ConPTY transport**, *not* by OneTerm's parser / grid /
> render. A faster in-house engine would therefore **not raise delivered throughput** for
> this workload — it would only add parse/CPU **headroom**, which the far cheaper R1 patch
> ([`09`](09-patch-alacritty-fork.md)) already delivered (~+70% parse capacity). And
> because the per-frame cost is **window-size dependent** (§6.4.1), there was never a
> single throughput figure to target in the first place.
>
> **What this document is now:** a design/architecture reference for a from-scratch VT
> engine (storage layout, snapshot bridge, migration seam, correctness plan), useful if a
> rewrite is ever motivated by *other* goals (owning the engine, dropping the rev-pinned
> fork, non-ConPTY transports). It is **not** a route to higher delivered throughput.
> Nothing here is built. Read §7.9 before treating any of it as scheduled work.

---

## 7.1. Goal & non-goals

> **Correction (see the status banner + [`06`](06-results-and-ceiling.md) §6.7):** the
> throughput goal below is **retained only as the original RFC text**. It is disproven —
> delivered throughput is producer/ConPTY-bound and window-size dependent, so this engine
> could not raise it. Read the goal as "the historical motivation"; the durable value of
> the design is **CPU/parse headroom** and **dropping the rev-pinned fork**, not a
> throughput number.

**Goal (as originally written — superseded).** Cut the per-frame pump+render cost by
removing the three structural costs a third-party engine forces on us, and by choosing
data structures tuned for the worst case (full-screen, every-cell-changes, truecolor).

**Non-goals.**
- Not a rewrite of the UI, SSH/SFTP, or session layers — only the parse+grid+render
  core changes.
- Not a general "make everything faster" project — normal TUIs already run fine.
- Not a VT-spec research project — we target parity with what OneTerm uses today,
  not with xterm's full historical surface.

**Success criteria (must all hold to justify the effort):**
1. A Phase-0 spike shows a **large per-frame pump+render CPU reduction** — parse capacity
   clearly above the R1 baseline (~69 MiB/s) plus the render-clone eliminated — enough to
   justify the migration on **headroom** grounds. *(It will not raise delivered
   throughput — §6.7 — so a throughput number is explicitly **not** a criterion.)*
2. Zero visible regressions vs the alacritty path on a differential test corpus
   (§7.7): same bytes in → same rendered grid out.
3. All existing features intact: scrollback, selection, search, URL/OSC 8, OSC
   7/9/52/133, alt-screen, mouse modes, IME, box-drawing, minimum-contrast, bell.
4. The quality gate (`fmt` / `clippy -D warnings` / `build` / `test`) stays green,
   and the gpui rev-lock is untouched.

If (1) cannot be demonstrated in a spike (§7.8, Phase 0), **do not proceed** — accept
the ceiling (06 §6.6 option A).

---

## 7.2. Why a *single pass* is the lever

Today the PTY→pixels path pays three redundant costs (measured in 06 §6.2–6.4):

| # | Redundancy | Where | Cost |
|---|---|---|---|
| R1 | **Double parse** — every byte runs through `alacritty::Processor::advance` *and* a second `vte::Parser` for OSC 7/9/52/133 + `CSI 2J/3J` clear detection | `local/event_loop.rs`, `ssh/task.rs`, `core/osc.rs` | part of the 72%-busy pump (pre-R1) |
| R2 | **Snapshot clone** — `TerminalContent::from()` copies ~5.4k cells out of `Term` under the `FairMutex` every render | `core/content.rs` | ~200 µs lock hold + alloc/copy per frame |
| R3 | **Opaque grid** — alacritty's `Grid<Cell>` is general-purpose (scrollback ring, wide-char spacers, per-cell `Flags`/`Hyperlink`); its per-cell mutation cost is fixed and unmodifiable (rev-pinned) | `alacritty_terminal` | the dominant part of the pump's parse+mutate time |

A single-pass engine collapses all three:

- **One parser** feeds the grid *and* extracts OSC/clear in the same state machine
  (kills R1).
- **A render-owned grid representation** the paint code can read **without cloning**
  (double-buffered or epoch-guarded), killing R2's clone.
- **A storage layout we control** (columnar / attribute-run / SoA) tuned for
  "overwrite the whole screen with truecolor cells every frame", killing R3's
  fixed cost and enabling SIMD/bulk fills.

Removing R1 (the double-parse) + R2 (the clone + lock hold) + a cache-friendly R3 layout
is the combination that would most reduce per-frame pump+render CPU on a single thread.
**Per §6.7 this buys CPU/parse headroom (lower power, room for more concurrent sessions),
not higher delivered throughput** — the latter is producer/ConPTY-bound. The Phase-0
spike (§7.8) therefore measures the CPU/parse-capacity delta, not a delivery figure.

---

## 7.3. The rev-lock constraint (the hard part)

`alacritty_terminal` here is **not upstream** — it is the pinned fork
`zed-industries/alacritty @ fcf32feacb367b75ec84dd40f041e4fd411d3cc1`, already patched
with `TerminalContent` / `display_iter` (see `docs/agents/dependencies.md` §1, §3).
Its types are woven through **~110 sites across ~38 files** in `core`, `local`, `ssh`
and `ui`. The public surface OneTerm depends on includes:

```
term::Term, term::Config, term::TermMode, term::TermDamage,
term::RenderableContent, term::RenderableCursor, term::cell::{Cell, Flags},
grid::{Dimensions, Scroll, GridIterator}, index::{Point, Line, Column, Side},
selection::{Selection, SelectionType, SelectionRange},
vte::{Params, Perform, Parser}, vte::ansi::{Processor, StdSyncHandler,
      Rgb, Color, NamedColor, CursorShape},
event::{Event, EventListener, WindowSize, OnResize}, tty::*, sync::FairMutex
```

Two consequences:

1. **We cannot just "swap the internals".** These are concrete types, not traits, so
   every consumer (search, url, selection layout, cursor, cell style/color, mouse
   encode, palette, IME) is coupled to alacritty's exact structs.
2. **A custom engine must ship an equivalent type layer.** The realistic design is a
   new crate `oneterm-vt` that defines *our own* `Cell`, `Flags`, `Point`, `Grid`,
   `Selection`, `Mode`, `Rgb/Color`, `CursorShape`, etc., plus a thin **adapter** so
   the rest of the codebase migrates module-by-module rather than in one big-bang.

The gpui rev-lock itself (`1d217ee…`) is **unaffected** — gpui does not depend on
`alacritty_terminal`; only OneTerm does. So this change does **not** touch gpui. It
only removes OneTerm's dependency on the alacritty fork. That is the whole point of
calling it "breaks the alacritty rev-lock": we stop tracking that fork.

---

## 7.4. Proposed architecture

A new workspace crate `oneterm-vt` (peer of `oneterm-core`), owning:

```
┌─────────────────────────── oneterm-vt ───────────────────────────┐
│                                                                   │
│  Parser (single state machine)                                    │
│    • CSI / SGR / OSC / DCS / ESC / control bytes                  │
│    • UTF-8 decode + wide-char (east-asian width) handling         │
│    • split-sequence state carried across read boundaries          │
│         │ emits ops ──────────────┐                               │
│         ▼                         ▼                               │
│  Grid (SoA / attribute-run)   SideChannel                         │
│    • rows × cols cells         • OSC 7/9/52/133 → queue           │
│    • alt-screen + scrollback   • clear (2J/3J/RIS) flag           │
│    • damage bitset per row     • title, palette (OSC 4/10/11)     │
│    • cursor, modes, tabs        • mode changes (mouse, bracketed) │
│         │                                                          │
│         ▼                                                          │
│  Snapshot bridge (lock-free)                                       │
│    • double-buffer OR epoch-guarded read view                     │
│    • exposes a RenderFrame the paint code consumes with no clone   │
└───────────────────────────────────────────────────────────────────┘
```

### 7.4.1. Storage layout (the R3 win)

Store cells as **structure-of-arrays** per row, not `Vec<Cell>`:

```
chars:  Box<[char]>        // or u32 codepoints
fg:     Box<[u32]>         // packed RGBA / palette index
bg:     Box<[u32]>
flags:  Box<[u16]>         // bold/italic/underline/inverse/wide/…
```

Benefits for the DOOM-fire worst case:
- Writing a full row of `▀` with new fg/bg is a tight loop over contiguous `u32`
  arrays → auto-vectorizable, cache-friendly, no per-cell `Flags` bitset object.
- The renderer reads `fg[]`/`bg[]` directly to emit quads; no `Cell` struct copy.
- **Attribute-run optimization:** when a whole row shares one SGR state (common in
  many TUIs, though *not* DOOM-fire), store a run instead of per-cell — cheap fast
  path that DOOM-fire simply won't hit but normal apps will.

### 7.4.2. Snapshot bridge (the R2 win)

Replace `TerminalContent::from()` (clone under lock) with one of:
- **Double buffer:** parser writes to back buffer; on frame boundary, atomically swap;
  renderer reads front buffer lock-free. Costs one extra grid's worth of memory
  (~small) and a pointer swap instead of a 5.4k-cell copy.
- **Epoch/seqlock read view:** renderer reads directly; a generation counter detects a
  torn read and retries. Zero extra memory, slightly more complex.

Either eliminates the ~200 µs lock hold + allocation per render frame.

### 7.4.3. Parser (the R1 win)

One `vte`-style state machine. On each dispatch it (a) mutates the grid **and** (b)
routes OSC/DCS/mode/clear to the side channel — so OSC 7/9/52/133 and `CSI 2J/3J`
clear detection come for free, no second pass. (We can keep the `vte` crate itself —
it is a tiny, dependency-light state machine and is *not* the alacritty fork; only the
`Term`/`Grid`/`Processor` layer is being replaced. Indeed R1 in doc 09 already added the
OSC hook to our vendored `vte`.)

### 7.4.4. Threading model

Unchanged in shape: PTY reader thread parses into the grid; UI thread renders from the
snapshot bridge. The `Arc<FairMutex<Term>>` is replaced by the double-buffer/epoch
bridge, so the two threads no longer contend on a coarse lock for the clone.

---

## 7.5. Scope — what must be reimplemented (the honesty section)

Everything below is currently provided by the alacritty fork and is exercised by
OneTerm. A custom engine owns all of it:

**Parsing**: UTF-8; C0/C1 controls; CSI (cursor moves, EL/ED erase, IL/DL insert/delete
lines, SU/SD scroll, DECSTBM margins, SGR incl. 256/truecolor, DECSET/DECRST private
modes); OSC 0/2/4/7/8/9/10/11/12/52/104/133; DCS (at least sixel/passthrough
tolerance); ESC (RIS, charset select, save/restore cursor, index/reverse-index,
alt-screen enter/exit); tab stops; wide-char + combining-char handling; split
sequences across reads.

**Grid/state**: primary + alternate screen; scrollback ring; per-cell fg/bg/flags;
cursor position/shape/visibility; insert/replace mode; origin mode; autowrap;
tab stops; selection model + `SelectionRange`; line/point indexing; resize/reflow.

**Modes** consumed by OneTerm today (`mouse_encode.rs`, handlers): mouse reporting
(1000/1002/1003/1006), bracketed paste (2004), application cursor keys, alt-screen
(1049), focus reporting.

**Consumers to migrate** (each is a coupling point, §7.3): `core/{content,search,url,
palette,osc,osc_color,colors_util,mouse_encode,session}`, `local/{session,
session_terminal,event_loop,listener,state}`, `ssh/{session,session_terminal,task,
listener,state}`, `ui/.../{cell/style,cell/color,url,prepaint,layout/selection,
layout/types}`.

This is why the effort is "large": the parser is a few thousand lines, but the
**migration of ~110 coupling sites** and reaching **behavioural parity** with a
battle-tested emulator is the real cost.

---

## 7.6. Migration strategy (incremental, reversible)

1. **Trait seam first.** Define OneTerm-owned types (`Cell`, `Point`, `Grid` view,
   `Mode`, `Color`, …) and a `TerminalEngine` trait. Adapt the *alacritty* path to
   implement it (wrap existing types). No behaviour change — just decoupling.
2. **Feature flag.** `--features engine-alacritty` (default) vs
   `--features engine-native`. Both compile; CI runs the differential tests against
   both.
3. **Build `oneterm-vt` behind the flag.** Bring up parser + grid + snapshot bridge;
   pass the differential corpus before wiring any UI.
4. **Migrate consumers module-by-module** to the owned types via the adapter.
5. **Flip the default** only after §7.1 success criteria are met on real hardware.
6. **Remove the alacritty dependency** (and its tty — see note) once native is proven.

> **tty caveat.** `alacritty_terminal::tty` (ConPTY on Windows) is a *separate* concern
> from the parser/grid. It can be kept even after replacing the engine, or replaced
> independently. Decouple these two decisions — the engine RFC does **not** require
> touching the PTY backend. (And note: the PTY backend is exactly where the delivered-
> throughput limit lives — §6.7 — so replacing the engine while keeping ConPTY changes
> CPU cost, not delivery.)

---

## 7.7. Testing & correctness

Correctness is the dominant risk (VT is a large, edge-case-ridden spec). Plan:

- **Differential/golden tests:** feed identical byte streams to alacritty and to
  `oneterm-vt`; assert identical grid contents, cursor, modes, damage. Corpus: real
  captures (vim, tmux, htop, `ls --color`, DOOM-fire, `clear`, resize storms,
  split-across-read sequences, malformed/partial escapes).
- **Reuse existing unit tests:** the `osc.rs`, `search.rs`, `url.rs`, box-drawing, and
  `content.rs` tests become parity tests against the new engine.
- **Property tests:** random SGR/CSI streams never panic; parser is total on arbitrary
  bytes; resize never loses/duplicates rows.
- **Fuzzing:** `cargo fuzz` the parser on arbitrary bytes (must never panic/OOM).
- **Perf gate:** the `[TerminalElement]` + `[PTY pump]` instrumentation (already in
  the tree) is the acceptance meter — re-measure per 06 §6.6 (`paint_us`, pump % busy,
  parse capacity).

---

## 7.8. Effort & phasing (rough)

| Phase | Work | Rough size | Gate |
|---|---|---|---|
| **0. Spike** | Minimal parser + SoA grid + double-buffer, DOOM-fire path only, no features | ~1–2 wk | **Does it materially cut per-frame pump+render CPU (parse capacity well above the R1 ~69 MiB/s baseline, clone eliminated)?** If no → stop. |
| 1. Type seam + `TerminalEngine` trait, adapt alacritty | decoupling | ~1 wk | gate stays green, no behaviour change |
| 2. Full parser (CSI/OSC/DCS/ESC/modes) + differential corpus | the bulk | ~3–5 wk | parity on corpus |
| 3. Scrollback, selection, search, resize/reflow parity | | ~2–3 wk | feature parity |
| 4. Migrate all ~110 consumer sites | | ~2 wk | gate green on `engine-native` |
| 5. Soak, fuzz, flip default, remove alacritty dep | | ~1–2 wk | success criteria §7.1 |

Total ≈ **2–3.5 months** of focused work, front-loaded with a **kill-switch spike**.
Ranges are indicative, not commitments.

---

## 7.9. Decision criteria & recommendation

**Pursue only if all are true:**
- The motivation is **not** delivered throughput — a Phase-0 spike would *not* raise it
  (§6.7: producer/ConPTY-bound, size-dependent). Valid motivations are owning the
  engine, dropping the rev-pinned alacritty fork, or enabling a non-ConPTY transport, AND
- Those goals are a **hard product requirement** (not a curiosity), AND
- There is appetite to own a VT emulator's long-tail correctness + maintenance.

**Otherwise: don't.** Per 06 §6.6 option A / §6.7, the delivered throughput at the
benchmark size is the practical ceiling for this pathological workload with *any* engine
(the limiter is outside OneTerm), and normal terminal usage is unaffected. The high-value
work (Tier 1/2/3, debug fix, damage fix, R1 parse headroom) is already banked.

### 7.9.1. Lower-risk alternative — patch the fork instead of replacing it

Because the dependency is **already a fork** (`zed-industries/alacritty`), a much
cheaper middle path exists:

- Fork/patch that repo's `Grid<Cell>` mutation hot path (SoA storage or bulk
  same-attribute fills) **in place**, keeping every public type identical.
- Add a `RenderableContent`-style zero-copy read view to kill R2's clone.
- Keep OneTerm's ~110 call sites **unchanged** (types don't move).

This captures most of R2 + R3 with a fraction of the migration risk, at the cost of
maintaining a heavier fork delta. R1 (the double-parse) is already removed this way —
doc 09 added the OSC/clear hook to the vendored fork. It is the pragmatic first step if
the headroom is worth it but a full rewrite is too big to fund.

---

## 7.10. Alternatives considered (and rejected as the *primary* path)

| Alternative | Verdict |
|---|---|
| Optimize within OneTerm only (done: Tier 1/2/3) | ✅ done; buys per-frame CPU/alloc headroom; delivered throughput is outside OneTerm (§6.7) |
| Remove only the OSC double-parse (R1) | ✅ done (doc 09): ~+70% parse capacity, pump parse-bound → wait-bound; buys pump-CPU headroom, not delivered throughput |
| GPU quad instancing (render side) | improves `paint_us`/smoothness only; **does not change delivered throughput** (06 §6.3) |
| Multi-thread alacritty's grid mutation | not possible without forking; alacritty's `Term` is single-writer |
| **Custom single-pass engine (this RFC)** | the only *pure-Rust* path that removes all three structural costs — but largest cost/risk, and does not raise delivered throughput (§6.7) |
| **Patch the existing fork (§7.9.1)** | pragmatic middle ground; captures R2+R3, keeps types (R1 already done). Full design in [`09-patch-alacritty-fork.md`](09-patch-alacritty-fork.md). |
| **Adopt `libghostty-vt`** (Ghostty's VT lib, via FFI) | inherits a fast, battle-tested engine — no parser to write; but adds Zig+FFI+Windows-MSVC build risk and an FFI read boundary. Full evaluation in [`08-libghostty-vt-evaluation.md`](08-libghostty-vt-evaluation.md). |
| **Adopt `termwiz`** (WezTerm's VT lib, pure Rust) | lower-risk Rust-native baseline to benchmark; cost structure vs alacritty measured favourable (see doc 08 §8.5) |

---

## 7.11. Summary

A custom single-pass engine removes all three structural costs (double-parse, snapshot
clone, opaque fixed-cost grid) and would decouple OneTerm from the pinned alacritty fork.
It was originally pitched as the path to higher delivered throughput — but
[`06`](06-results-and-ceiling.md) §6.7 disproves that: delivery is producer/ConPTY-bound
and window-size dependent, so this engine would add parse/CPU **headroom**, not delivered
throughput. It remains a multi-month, correctness-critical rewrite touching ~110 sites.
**Do not pursue it for throughput.** If the motivation is instead "own the engine / drop
the fork / enable a non-ConPTY transport", treat this as the reference design and still
gate it behind a Phase-0 spike. For pure headroom with far less risk, R1 (shipped) and the
rest of [`09`](09-patch-alacritty-fork.md) are the pragmatic path.
