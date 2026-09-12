# High-Level Design: VT engine rewrite

Intake: IN-0029
Lane: high_risk
Date: 2026-09-12

## Idea

Replace the vendored `alacritty_terminal` + `vte` pair with two Apache-2.0 workspace crates
OneTerm owns: `oneterm-pty` (pseudo-console transport) and `oneterm-vt` (parser, grid,
dispatch, reflow, damage, render state, graphics). The engine is designed around OneTerm's
consumers rather than around alacritty's API shape: rows have absolute ids, events are values
returned from `feed()`, damage is a per-row sequence number read through per-consumer
watermarks, the renderer gets an incremental render state instead of a viewport copy, and the
ConPTY resize policy lives inside the engine instead of being corrected on top of it.

The argument is extension cost, not throughput. The measured headroom at realistic rates is
two to three orders of magnitude ([`research/perf-baseline.md`](research/perf-baseline.md) § 4);
what the rewrite removes is 823 lines of vendored patches, a `refresh.sh --check` CI job, and
the rule that every new VT capability starts as a patch against someone else's tree
([`DEC-0014`](../../decisions/DEC-0014-oneterm-owns-its-vt-engine.md)).

Design is taken from the converged prior art rather than invented: dual-form rows and
sequence-number damage from wezterm, packed 8-byte cells with interned styles and extras from
Rio and Ghostty, GC-by-remap for the grapheme arena from kitty, lazily allocated power-of-two
ring rows from foot, tracking-point reflow from kitty and foot with avt's iterator, the
two-phase render state from Ghostty, and "unhandled implies re-emit the original bytes" from
Windows Terminal ([`research/prior-art.md`](research/prior-art.md) § 2.2, § 9).

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
| 3 | `US-0075` | Grid, scrollback, anchors | The viewport-offset table proven row by row (R-01); all four `scroll_up` cases including the bottom-bounded region (R-03); the two screens' id ranges disjoint (R-04); `lines_produced` matches today's gutter (R-05); anchors move with content through every primitive (R-02); the debug-suite runtime budget measured (R-28) |
| 4 | `US-0076` | Dispatch and modes | **The 45-recording parity gate is green** against the frozen expectations (`cargo test -p oneterm-vt --test ref_corpus`); every sequence marked supported in `docs/osc-sequences-checklist.md` has a byte-feed test; `? 9001` accepted silently (R-36); `AppKeypad` reported (R-64); mode 2027 recognised and inert (R-56) |
| 5 | `US-0077` | Reflow and resize | The ten `keep_viewport_top_*` behaviours reproduced as engine tests **while the old suite still runs against the old engine** (R-44); seven proptest properties green over 10 000 cases; `measure_rows` fixtures carry their host version (R-39); resize recorded at three scrollback depths as a ratio, not a target (R-29) |
| 6 | `US-0078` | Selection | The invalidation matrix proven row by row; anchors follow a region scroll; `selection_range()` allocation-free; block extraction and semantic expansion tested (R-18) |
| 7 | `US-0079` | Damage, render state, events | `rows()` always full plus a `changed` list (R-15); style runs carry resolved values and survive a grapheme sweep (R-14); `ModeSnapshot` refreshed on every update (R-17); `RenderState` never drains graphics (R-16); mode 2026 deterministic with an injected clock (R-11) |
| 8 | `US-0080` | Graphics in the engine | The ten `sixel_tests.rs` behaviours reproduced; one extras entry per image; a placement moves with an in-region scroll; `GraphicReleased` fires on `CSI 2 J`, on a row reset and on a trim (R-22); `vt-diff` green over the Sixel recordings. **Engine-level only — IN-0028's evidence walk moves to `US-0081`, because the new engine is not behind the application until the shim (N-02)** |
| 9 | `US-0081` | **Engine behind the seam (shim)** | `cargo test --workspace` green with `LegacySnapshot` producing today's `TerminalContent`; `vt-diff` zero divergence over 45 recordings plus a captured session; the IN-0018, IN-0027 **and IN-0028** GUI walks reproduced (N-02); scope bounded by the table in `migration.md` — all of `crates/terminal`, and in the two backends only the shared-terminal type, its construction and the manifest line, with no backend test changed (N-04) |
| 10 | `US-0082` | `crates/terminal` goes native | `RowId`, batch drain, `RenderState`, `ColorKey`; `model.rs:481-541`, `line_accounting.rs` and the deferred event tier deleted; the old and new resize suites both green in the same commit, then the old one deleted (R-44); `GridText` kept adapter-side (R-19) |
| 11 | `US-0083` | `crates/local-shell` goes native | the read loop drains the `EventBatch`, `ResizePolicy` is selected through the engine API, the `oneterm-pty` tokens replace the last local constants, the `alacritty_terminal` manifest line is deleted (N-04); `cargo test -p oneterm-local-shell` green; `local_session_grow_policy_matches_conpty` unchanged |
| 12 | `US-0084` | `crates/ssh` goes native | the same three for the tokio task plus `BottomAnchor` selection and the manifest line (N-04); `cargo test -p oneterm-ssh` green; `ssh_session_keeps_the_default_grow_policy` unchanged; no `alacritty_terminal` dependency left |
| 13 | `US-0085` | `crates/terminal-view` goes native | `frame.rs`, `plan_cache`, `input/mouse.rs`, `theme/palette.rs` on `RenderRow` / `SelectionKind` / `ModeSnapshot`; both display-offset fallbacks and `engine_shim.rs` deleted; GUI walks re-run |
| 14 | `US-0086` | Deferred deviations and extension-point hardening | Every `US-0086` row of the two deviation tables implemented **after** the gate — including D11, moved out of the parity packet so storing blink and overline cannot turn `grid.expect` red (N-03) — each with the recording risk measured in `US-0072`; OSC 9;7 proven to work as a registration with no engine change |
| 15 | `US-0087` | Decommission | `vendor/` absent; no `refresh.sh` CI job; no `[patch]`; the `vte` dev-oracle and `vt-diff` deleted; `python scripts/third-party-notices.py --check`, `check-doc-paths.py` and `pwsh scripts/ci-local.ps1` green; every owning doc reconciled |

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
