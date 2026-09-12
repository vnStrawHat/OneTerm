# IN-0029 — VT engine prior art, build-vs-adopt research

> **Status:** research record only. No source, schema, or configuration is changed by this
> document, so it carries no work packet (`docs/HARNESS.md` §"Routing"). It is the input to
> the IN-0029 Spec Intake and its High-Level Design.
>
> **Date:** 2026-09-12 · **Scope:** replace the vendored `alacritty_terminal` + `vte` pair with
> an engine OneTerm controls — either written from scratch, adopted, or forked.
> **Platforms in scope:** Windows/ConPTY first, then Unix PTY and SSH byte streams.
>
> **Confidence marking.** Claims sourced from the tree in this repository are marked
> **[local]** and were read directly from `vendor/` or `crates/`. Claims from upstream
> repositories, blogs, and crates.io are cited inline with URLs. Vendor-published benchmark
> numbers are labelled as such and are **not** independently reproduced.
>
> **Companion notes in this folder** — this document covers *external* prior art only. Read it
> alongside [`perf-baseline.md`](perf-baseline.md) (measured throughput of the current engine on
> the owner's machine — the numbers this document defers to wherever they conflict with inference),
> [`api-surface.md`](api-surface.md) (the exact `alacritty_terminal` / `vte` API OneTerm consumes),
> and [`engine-semantics.md`](engine-semantics.md) (the behavioural specification a replacement must
> satisfy).

---

## 0. Executive summary

> **The headline, before anything else: the case for this intake is extension cost, not speed.**
> The sibling [`perf-baseline.md`](perf-baseline.md) measured the current engine at
> **126–253 MB/s** full parse + grid on the owner's machine, against a real ConPTY producer rate of
> **~1.2 MiB/s** (`cmd.exe` loop) to ~30 MiB/s (DOOM-fire class) — two to three orders of magnitude
> of headroom at realistic shell and SSH rates. The per-frame viewport snapshot that looks expensive
> costs **29.9 µs, 0.18 % of a 60 Hz budget**. Where headroom *is* thin, the lever is the
> **grid-mutation side, not the parser**: `vte` alone runs at 322–1 321 MB/s, so `Term`'s `Handler`
> implementation costs 3–8× what the parser does. A rewrite argued on throughput would be
> unfalsifiable in normal use. A rewrite argued on **Sixel/Kitty graphics, kitty keyboard, OSC
> extensions, and the compounding cost of an 823-line vendored patch series** is defensible today.

1. **The engine is ~12–13k LOC of grid/parse code, not a weekend.** `alacritty_terminal`'s
   core (`term/`, `grid/`, `selection`, `vi_mode`, `search`) is 8 700 lines plus 4 000 lines of
   `vte` **[local]**, and that is *before* Sixel, Kitty graphics, kitty keyboard, search UX and
   snapshot/restore. Anything we write from scratch must clear that bar to reach today's
   behaviour, and clear it again to beat it.

2. **The blast radius of swapping engines is small and already surveyed.** 31 files,
   124 references to `alacritty_terminal`, concentrated in `crates/terminal` (79),
   `crates/local-shell` (22) and `crates/terminal-view` (15) **[local]**. The UI only sees
   `Cell`, `Flags`, `Rgb`, `TermMode`, `Point`, `SelectionRange`, `RenderableCursor` and
   `GraphicData`. OneTerm already funnels everything through a **snapshot** boundary
   (`TerminalContent::from(&mut Term)`, `docs/terminal-backend.md` §5.2), so the engine can be
   replaced behind that seam.

3. **Two credible off-the-shelf engines now exist that did not exist when OneTerm picked
   alacritty**: `rio-vt` (pure Rust, MIT, published 2026-07-26, Sixel + Kitty + iTerm2 +
   mode 2027 + kitty keyboard built in) and `libghostty-vt` (Zig, MIT, C ABI with mature Rust
   bindings, paged memory + idle scrollback compression, Windows-proven in production by
   Paneflow). Both already exceed the feature set a from-scratch engine would take a year to
   reach.

4. **Upstream `alacritty_terminal` is in maintenance mode and hostile to graphics by policy.**
   0.26.0 (2026-04-06) shipped one Windows arg-escaping option and one breaking exit-status
   change; the Sixel PR ([#4763](https://github.com/alacritty/alacritty/pull/4763)) has been
   open since 2021 and the maintainer has said it has "little chance of ever getting
   upstreamed". The `zed-industries` fork we pin is **5 commits, Unix-only**, and buys a
   Windows-first project nothing.

5. **Our pinned fork carries two upstream defects that upstream has since fixed.**
   `CellExtra.zerowidth` is an **unbounded `Vec<char>`** **[local]** — upstream's unreleased
   0.26.1-dev changelog lists "Unbounded per-cell memory usage for zero-width cells" as fixed —
   and `Row::new(0)` writes through a dangling pointer in release builds **[local]**, also
   listed as fixed upstream. Independently, `vte` 0.15's OSC buffer is an **unbounded
   `Vec<u8>`** in `std` builds **[local]**: a stream that opens `ESC ]` and never terminates it
   grows memory without bound.

6. **An unrelated but urgent finding: OSC 9;7 is not a free sub-code.** ConEmu defines
   `ESC ] 9 ; 7 ; "cmd" ST` as **"Run some process with arguments"**
   ([ConEmu documentation](https://conemu.github.io/en/AnsiEscapeCodes.html), verified directly).
   `docs/osc-agent-status.md` §2 states that sub-codes 0–4 are taken and "7 is free" **[local]** —
   that rationale is wrong, and the protocol we ask third-party agents to emit collides with a
   command-execution sub-code on a Windows terminal. See §6.4; this needs its own packet, separate
   from the engine work.

7. **A large, licence-clean parity harness already exists.** alacritty's 45 ref-test recordings are
   Apache-2.0 and ship inside the crates.io package; libvterm's 43 `.test` files and Ghostty's
   ~4 000 AFL++ fuzz seeds are MIT. Replaying alacritty's recordings against a new engine pins
   behavioural parity with the engine being replaced — the cheapest safety net a rewrite can have.
   `vendor/refresh.sh` currently prunes that directory **[local]**.

8. **Recommendation (detail in §9): do not start from a blank file.** Build the OneTerm engine
   as an *owned Rust crate* whose grid/cell/damage design is taken from the converged prior
   art (dual-form lines, interned styles, sequence-number damage, page-or-ring storage), and
   settle `rio-vt` vs from-scratch with a two-week measured spike behind the existing
   `TerminalSession` seam. Rank `libghostty-vt` third for OneTerm specifically: it is the best
   engine and the worst fit (Zig in the Windows build, `!Send + !Sync` handles, no Sixel, no
   ability to patch internals for OSC 9;7).

---

## 1. Baseline — what OneTerm runs today

Everything in this section was read from this repository **[local]**.

### 1.1 Sizes

| Component | Path | Lines |
|---|---|---:|
| `alacritty_terminal` `term/mod.rs` | `vendor/alacritty_terminal/src/term/mod.rs` | 3 394 |
| `term/search.rs` | | 1 251 |
| `vi_mode.rs` | | 893 |
| `grid/{mod,storage,resize,row}.rs` | | 2 107 |
| `term/{cell,color,graphics}.rs` | | 750 |
| `selection.rs`, `index.rs`, `event_loop.rs` | | 1 277 |
| `tty/` (unix + windows ConPTY) | | 1 663 |
| `vte` `lib.rs` (parser) + `ansi.rs` (VT semantics) + `params.rs` | `vendor/vte/src/` | 4 169 |
| **Total vendored** | | **16 642** |
| OneTerm glue (`crates/terminal`) | | 9 825 |
| OneTerm renderer (`crates/terminal-view`) | | 21 322 |

Fork burden: **823 patch lines** across 5 patches (`vendor/patches/`), of which the Sixel patch
is 577 lines.

### 1.2 Cell, row, grid

```rust
// vendor/alacritty_terminal/src/term/cell.rs
pub struct Cell {
    pub c: char,                       // 4 B
    pub fg: Color,                     // 4 B  (enum Named|Spec(Rgb{u8,u8,u8})|Indexed(u8))
    pub bg: Color,                     // 4 B
    pub flags: Flags,                  // 2 B  (bitflags u16, 17 flags defined)
    pub extra: Option<Arc<CellExtra>>, // 8 B
}
// tests: const EXPECTED_CELL_SIZE: usize = 24; assert!(size_of::<Cell>() <= 24);
```

`CellExtra` (heap, refcounted, copy-on-write through `Arc::make_mut`) holds
`zerowidth: Vec<char>`, `underline_color: Option<Color>`, `hyperlink: Option<Hyperlink>` and —
OneTerm's addition — `graphic: Option<GraphicCell>`.

Consequences measured from the source:

- **24 B/cell, always full width.** `Row<T> { inner: Vec<T>, occ: usize }` is
  32 B of header plus a *fully materialised* `Vec` of `columns` cells; `occ` is only a
  "modified since reset" hint, never a storage saving. A 200-column × 100 000-line scrollback
  is ≈ 480 MB of live allocation with no compression and no reclaim.
- **One heap allocation + refcount per decorated cell.** A single combining mark, an OSC 8
  hyperlink or a coloured underline promotes the cell to `Some(Arc<CellExtra>)`; every further
  write goes through `Arc::make_mut` (clone-on-write).
- **`zerowidth` is unbounded in our pinned tree** — no `MAX_ZEROWIDTH_CHARS` cap. Upstream
  fixed this after our pin.
- Storage is a **ring**: `Storage<T> { inner: Vec<Row<T>>, zero, visible_lines, len }` with
  `rotate` as modular arithmetic on `zero`, and a `MAX_CACHE_SIZE = 1_000` row free-list.
  `Grid::scroll_up` over the whole screen is O(1) index rotation; over a scroll region it is
  O(region) row swaps.

### 1.3 Reflow

`grid/resize.rs`: `grow_columns`/`shrink_columns` join and split rows using the
`Flags::WRAPLINE` marker on the last cell of a row; `grow_lines` pulls
`min(history_size, lines_added)` rows out of scrollback. Known upstream bugs that are still
open — cursor not reflowing with content
([#3584](https://github.com/alacritty/alacritty/issues/3584)), jumbled content on resize
([#4419](https://github.com/alacritty/alacritty/issues/4419)), unreadable horizontal shrink
([#3815](https://github.com/alacritty/alacritty/issues/3815)), whitespace inserted when
resizing during output ([#3968](https://github.com/alacritty/alacritty/issues/3968)),
duplicated prompt line ([#2408](https://github.com/alacritty/alacritty/issues/2408)), hard
breaks in copied long lines ([#8010](https://github.com/alacritty/alacritty/issues/8010)).

### 1.4 Damage

```rust
pub struct LineDamageBounds { pub line: usize, pub left: usize, pub right: usize }
pub enum TermDamage<'a> { Full, Partial(TermDamageIterator<'a>) }
```

Per **viewport** line, with column bounds, held in `Vec<LineDamageBounds>` sized to the visible
rows. Scrollback is not tracked. It is a **single-consumer** model: `Term::damage()` yields the
set and `Term::reset_damage()` clears it, so exactly one reader can exist. Entering insert mode,
any resize, or any reflow escalates to `Full`.

### 1.5 Threading and renderer hand-off

`docs/terminal-backend.md` §5:

- `Arc<FairMutex<Term<EP>>>`; the pump thread (local `EventLoop` or the SSH tokio task) parses
  under the lock, the GPUI main thread paints.
- The renderer never holds the lock while painting. `TerminalContent::from(&mut Term)` takes the
  lock, **copies the whole viewport into owned `IndexedCell { point, cell }` values** plus
  cursor/selection/mode/display-offset and the pending Sixel images, drops the lock, and the
  element paints from that copy.
- alacritty's own read loop releases the lock every `MAX_LOCKED_READ = 65 535` bytes and uses a
  `READ_BUFFER_SIZE = 0x10_0000` (1 MiB) buffer **[local, `event_loop.rs`]**.
- `SessionEvent::Output` is the only coalescible event; everything else is reliable and bounded.

**The cost this design pays — and what it actually measures.** The snapshot is a full viewport copy
per frame: one `Cell::clone()` per visible cell. The intuition is that this is expensive; the
measurement says otherwise. [`perf-baseline.md`](perf-baseline.md) §4 records **29.9 µs per frame**
for a 200×50 (10 000-cell) `renderable_content()` + copy on the owner's machine — about **0.18 % of
a 60 Hz frame budget**. It is real overhead that a shared-immutable or double-buffered design does
not pay, and it scales with *viewport area* rather than output volume (so a much larger window costs
proportionally more), but **it is not a bottleneck today and must not be used as the headline
justification for a rewrite.** See §7.4 and §9.6.

### 1.6 Parser (vte 0.15) — what it actually is

vte 0.15 is **not** table-driven any more. `Parser::advance` dispatches on `State` into
hand-written `advance_csi_param`, `advance_osc_string`, … methods **[local]**:

```rust
while i != bytes.len() {
    match self.state {
        State::Ground => i += self.advance_ground(performer, &bytes[i..]),
        _ => { let byte = bytes[i]; self.change_state(performer, byte); i += 1; },
    }
}
```

The ground fast path is the interesting part:

```rust
let plain_chars = memchr::memchr(0x1B, bytes).unwrap_or(num_bytes);
...
match str::from_utf8(&bytes[..plain_chars]) {
    Ok(parsed) => { Self::ground_dispatch(performer, parsed); ... }
```

`memchr` (SIMD, `memchr` 2.8.2 is already in our lock file) finds the next `ESC`; the run is
validated as UTF-8 in one pass; then `ground_dispatch` walks `text.chars()` and calls
`performer.print(c)` **per character**. So the *scan* is vectorised but the *dispatch* is not
batched: there is no `print_str(&str)` on `Perform`. Note also that `memchr` looks only for
`0x1B` — `\n`, `\r`, `\t` are found only in the per-char loop.

Limits and deviations from Paul Williams' diagram **[local]**:

| Item | vte 0.15 |
|---|---|
| `MAX_INTERMEDIATES` | 2 |
| `MAX_PARAMS` (incl. subparams) | 32 (Williams: 16) |
| `MAX_OSC_PARAMS` | 16 |
| OSC payload size | **unbounded `Vec<u8>` under `std`**; `1024` only in `no_std` |
| DCS payload | not buffered; `Perform::put` per byte, handler owns the bound |
| OSC terminators | `BEL (0x07)`, `ESC` (so `ESC \` works), `CAN`, `SUB` |
| 8-bit C1 introducers (`0x9B` CSI, `0x90` DCS, `0x9D` OSC) | **not supported** — executed as C1 controls, never as introducers |
| C1 in ground | `'\u{80}'..='\u{9f}' => performer.execute(c as u8)` |
| Colon subparameters | supported (`Params` tracks `subparams` per param) |
| UTF-8 | validated per run, `partial_utf8: [u8; 4]` carries across `advance` calls |
| Synchronized output (DEC 2026) | handled one layer up in `ansi::Processor`: buffers up to `SYNC_BUFFER_SIZE = 0x20_0000` (2 MiB) with a timeout |

### 1.7 VT coverage of the engine we ship today

Read directly from `vendor/vte/src/ansi.rs` **[local]**.

- **CSI finals handled:** `@ A B C D E F G I J K L M P S T X Z a b c d e f g h l m n p q r s t u`
  plus `?`-prefixed `h l m p u W`, `>`-prefixed `c m u`, `<`-prefixed `u`, `=`-prefixed `u`,
  `$`-prefixed `p`, `SP`-prefixed `q` (DECSCUSR), `SP k` (SCP).
- **Private modes recognised:** 1, 3, 6, 7, 12, 25, 1000, 1002, 1003, 1004, 1005, 1006, 1007,
  1042, 1049, 2004, 2026. ANSI modes: 4 (IRM), 20 (LNM).
- **OSC handled natively:** 0, 2, 4, 8, 10, 11, 12, 22, 50, 52, 104, 110, 111, 112. Everything
  else falls through — which is exactly what OneTerm's `report_osc` patch captures for
  OSC 7 / 9 / 9;4 / 9;7 / 133.
- **Kitty keyboard protocol:** the *mode stack* is implemented (`CSI ? u`, `CSI = u`,
  `CSI > u`, `CSI < u` → `report/set/push/pop_keyboard_mode`) and `TermMode::KITTY_KEYBOARD_PROTOCOL`
  is tracked. **OneTerm never reads it**: `grep -rin kitty crates/` returns nothing **[local]**,
  and `crates/terminal/src/key_encode.rs` emits legacy encodings only.
- **DECRQM/DECRPM:** present (`CSI $ p`, `CSI ? $ p` → `report_mode` / `report_private_mode`).
- **modifyOtherKeys:** present (`CSI > 4 ; n m`, `CSI ? 4 m`).
- **Not present:** DECSLRM / left-right margins (mode ?69), DECCARA/DECFRA/DECERA rectangular
  ops, DECRQCRA checksum, XTVERSION (`CSI > 0 q`), XTGETTCAP, XTPUSHSGR/XTPOPSGR,
  XTSMGRAPHICS, mode 2027 grapheme clustering, mode 2048 in-band resize, mode 1016 pixel mouse,
  DECDWL/DECDHL double-width/height lines (`ESC # 8` DECALN is the only `ESC #` handled).

### 1.8 Windows/ConPTY constraints already measured here

`docs/terminal-backend.md` §5.3 and DEC-0008 record measurements from raw PTY dumps (BUG-0051,
IN-0019) that any replacement engine must honour:

- conhost behind ConPTY **does not repaint after `ResizePseudoConsole`**. It re-wraps the rows
  of the old viewport at the new width as if the top row started a line, keeps that content at
  the top, leaves the rows below blank, and addresses later output with absolute `CUP`.
- Measured drift: maximising 33×43 → 52×158 with `ls -lath` output, conhost's next `CUP` named
  row 12 while a bottom-anchored grid cursor sat on row 33 (21 joined rows); widening to 132
  columns gave row 14; growing to 49×34 gave row 38.
- OneTerm therefore runs a **`ResizePolicy::KeepViewportTop`** correction on Windows local
  sessions: reflow a scratch copy of the viewport to measure conhost's cursor row, then shift
  the real grid by the difference. SSH keeps `ResizePolicy::Default`.
- OneTerm ships Windows Terminal's `conpty.dll` + `OpenConsole.exe` (1.23.2512.16003, MIT)
  next to `oneterm.exe` so ConPTY uses that host rather than the system `conhost.exe`
  (`THIRD-PARTY-NOTICES.md` §1).
- `crates/tools/src/bin/pty-throughput.rs` already isolates transport throughput from parsing:
  its docstring records a **~30 MiB/s ConPTY plateau** observed with the DOOM-fire workload.

**A replacement engine must expose enough of the grid (reflow a scratch grid, read the cursor
row, shift the viewport) to reimplement `KeepViewportTop`.** This is a hard requirement that
rules out any engine whose grid is opaque behind a narrow API.

---
## 2. Engine architecture survey

### 2.1 Comparison matrix

| Engine | Lang / licence | Cell representation | Grid + scrollback storage | Reflow | Damage model | Renderer hand-off | Parser |
|---|---|---|---|---|---|---|---|
| **alacritty_terminal** 0.26 | Rust, Apache-2.0 | 24 B struct: `char`, fg, bg, `Flags(u16)`, `Option<Arc<CellExtra>>`; extras heap-boxed + CoW | Ring `Storage<Row<Cell>>`, `zero` rotation, 1 000-row free list; `Row = Vec<Cell>` always full width | Yes, join/split on `WRAPLINE`; 6 long-standing open bugs; ~227 µs at 80×24→100×40 (third-party bench) | `Vec<LineDamageBounds>` per **viewport** line + col bounds; single consumer, `reset_damage()` required; `Full` on any scroll/resize | Consumer copies (OneTerm: full viewport clone per frame) | `vte` 0.15 — hand-written match state machine, `memchr` ESC scan, **per-char `print`** |
| **wezterm-term** + termwiz | Rust, MIT (crates unpublished) | `Cell` = 24 B: `TeenyString` (u64 NaN-box, inline <7 bytes) + `CellAttributes` 16 B (u32 attrs + 2 `SmallColor` + `Option<Box<FatAttributes>>`); **no interning** | `Screen { lines: VecDeque<Line> }` — viewport + scrollback in one deque; `stable_row_index_offset` gives durable row IDs | Yes — `rewrap_lines` drains, joins on `last_cell_was_wrapped`, re-splits; alt screen never reflows; documented column-zero cursor edge case | **Sequence numbers.** Every mutation stamps `Line.seqno`; `changed_since(seqno)`; multi-consumer, no reset pass | `Mutex<Terminal>`; GUI holds the lock and walks lines through a visitor (`with_lines_mut`, zero copy); per-line `appdata` caches shaping | `vtparse` — checked-in static `[[u16;256];15]` table, typed `CsiParam` (colon subparams), explicit `apc_dispatch`; **no print-run batching** |
| **Rio `rio-vt`** 0.5.26 | Rust, MIT (alacritty-derived) | `Square(u64)` — `#[repr(transparent)]` packed: codepoint bits 0..21, `CellFlags` 23..30, `style_id` u16, `extras_id` u16. Styles and extras live in **per-grid interned, GC'd side tables** | alacritty ring retained (`Storage` + `zero` + row free list), plus `total_lines_scrolled: u64` stable row space and `ReflowRemap` old→new row mapping | Yes, with remap so graphics re-anchor; vendor bench 5.0 µs at 80×24→100×40 | `TermDamage::{Full, Partial}` + `LineDamage`; per-`Row` dirty bit; single consumer, explicit reset | `parking_lot` mutex, renderer iterates damaged lines | alacritty-derived, plus `simd_utf8.rs`, `simd_base64.rs`, `codepoint_width.rs`, `grapheme_lut.rs` |
| **Ghostty** (`libghostty-vt`) | Zig, MIT | `packed struct(u64)`: `content_tag:u2`, 24-bit content union (codepoint / palette / rgb), `style_id:u16`, `wide:u2`, `protected`, `hyperlink`, `semantic_content:u2`, **16 bits spare**. Styles interned in a per-page `RefCountedSet` (Robin Hood, load factor 0.8125, id 0 reserved) | **Page list.** Intrusive doubly-linked list of `Page`s, each a single `mmap`/`VirtualAlloc` page-aligned block (~400 KiB std) using `Offset(T)=u32` instead of pointers, with in-page arenas for graphemes/strings/hyperlinks/styles; `UntouchedPool` so idle pooled pages cost virtual memory only; per-node `serial:u64` generation counter; LZ4 **idle scrollback compression** (PR #13264, 70–90 % physical memory) | Yes, `PageList.resizeCols` rewrites the whole list through a `ReflowCursor` with a `StyleCache` memo; cursor tracked as a pin; `Capacity.adjust` re-geometries a page at constant size | `Row.dirty` bit **inside the packed u64 row header** + `Page.dirty` + `Screen.Dirty` + `Terminal.Dirty`; false-positives allowed, never false-negatives; consumed per **page chunk** | Two-phase: `beginUpdate` under a **demand-signalling** mutex (unfair mutexes starve the renderer under sustained output; 1 ms handoff timeout), then `endUpdate` outside the lock; `RenderState` keeps per-row arenas + `StyleRun`s; mode 2026 skips the frame entirely | Table-driven (`parse_table.zig`, comptime-generated), colon subparams; **SIMD bulk path**: `utf8DecodeUntilControlSeq` on Google Highway + simdutf, 4 096-codepoint buffer, emits one `print_slice` action for a whole run |
| **Contour** | C++, Apache-2.0 | **No `Cell` type any more** — `LineSoA` structure-of-arrays tiered hot (`codepoints`, `widths`) / warm (`sgr`) / cold (`hyperlinks`, `textScaleExtras`) + a cluster pool | `crispy::Ring<Line>`; `_stableBase` / `_stableFloor` / `_generation` for incremental mirroring; two history limits with **block-atomic eviction** at command-block boundaries | Yes, centralised through `resizeBuffers()`/`rotateBuffers*()` so stable IDs never desync | `_screenDirty` + refresh-rate arbitration in `ensureFreshRenderBuffer()` | **Double-buffered `RenderBuffer`.** Terminal thread builds the back buffer (`RenderLine` for trivial lines, `RenderCell` otherwise) and swaps; GUI takes an RAII reader lock. GUI never touches the grid | `constexpr ParserTable` built at compile time from readable builder calls + libunicode SIMD `scan_text`; bulk DCS/APC passthrough; handler API is `print(string_view, size_t)` |
| **Windows Terminal** `TextBuffer` | C++, MIT | **No cell array.** `ROW` = flat UTF-16 `_chars` + `_charOffsets: span<uint16_t>` (column→char index, MSB = trailing half of a wide glyph) + `til::small_rle<TextAttribute, uint16_t, 2>` attributes | Circular buffer of `ROW` with `_firstRow` rotation over **one `VirtualAlloc` arena with lazy commit** (`_commitReadAheadRowCount = 128`) | `TextBuffer::Reflow` allocates a whole new buffer and copies; truncates trailing whitespace; double-height rows truncated not reflowed; documented cursor-loss and `REFLOW_JANK_CURSOR_WRAP` hacks | (renderer-specific; `til::rect` invalidation in the render engines) | N/A (separate render engines) | Hand-written Williams derivative with extra `CsiSubParam`, `OscTermination`, `Ss3*`, `Vt52Param` states; `MAX_PARAMETER_COUNT=32`, `MAX_SUBPARAMETER_COUNT=6`; **every action returns `bool`; `false` ⇒ `FlushToTerminal()` re-emits the original byte run** |

Sources for the rows above: **alacritty** — this repository's `vendor/` tree **[local]** plus
[crates.io/alacritty_terminal](https://crates.io/crates/alacritty_terminal);
**wezterm** — `term/src/screen.rs`, `wezterm-surface/src/line/{line,storage,clusterline}.rs`,
`wezterm-cell/src/lib.rs`, `vtparse/src/{lib,transitions}.rs`, `mux/src/localpane.rs` in
[wezterm/wezterm](https://github.com/wezterm/wezterm);
**Rio** — `rio-vt/src/crosswords/{mod,square}.rs`, `rio-vt/src/crosswords/grid/{mod,storage}.rs` in
[raphamorim/rio](https://github.com/raphamorim/rio) and
[the rio-vt/librio announcement, 2026-07-27](https://rioterm.com/blog/2026/07/27/rio-vt-and-librio);
**Ghostty** — `src/terminal/{page.zig,PageList.zig,style.zig,ref_counted_set.zig,Parser.zig,stream.zig,render.zig,osc.zig}`,
`src/renderer/{State.zig,generic.zig}`, `src/termio/Exec.zig` in
[ghostty-org/ghostty](https://github.com/ghostty-org/ghostty) @ `main` (1.3.2-dev), plus
[PR #13264](https://github.com/ghostty-org/ghostty/pull/13264) and
[devlog 006](https://mitchellh.com/writing/ghostty-devlog-006);
**Contour** — `src/vtparser/{Parser.hpp,Parser-impl.hpp}`, `src/vtbackend/grid/{Line.hpp,LineSoA.hpp,Grid.hpp}`,
`src/vtbackend/render/RenderBuffer.hpp` in [contour-terminal/contour](https://github.com/contour-terminal/contour);
**Windows Terminal** — `src/buffer/out/{Row.hpp,textBuffer.hpp,textBuffer.cpp}`,
`src/terminal/parser/stateMachine.cpp`, `src/host/{VtIo.cpp,_stream.cpp,outputStream.cpp}` in
[microsoft/terminal](https://github.com/microsoft/terminal).

### 2.2 The three ideas every engine converged on independently

Read across the matrix, three designs appear in projects that did not copy each other. That
convergence is the strongest evidence in this report.

1. **A line has two forms: a cheap uniform form and an expensive general form.**
   wezterm: `CellStorage::C(ClusteredLine)` (one `String` + run-length `(cell_width, attrs)`
   clusters + a lazily-allocated wide-char bitset, 64 B header) versus `CellStorage::V(Vec<Cell>)`;
   lines are *built* clustered, coerced to `Vec` on the first random-access mutation, and
   `compress_for_scrollback()` re-clusters them on the way out of the viewport.
   Contour: `TrivialLineBuffer` (one text fragment + one `GraphicsAttributes`, `isTrivialBuffer()`
   is an O(1) cached flag, rendered as a single batched `RenderLine`) versus `LineSoA`.
   Windows Terminal arrives at the same place from the other direction: one flat string plus an
   RLE attribute vector *is* the cheap form, and it has no expensive form at all.
   Order-of-magnitude effect: an 80-column uniform ASCII line is ~170 B clustered versus
   80 × 24 = 1 920 B as a `Vec<Cell>`.

2. **Styles are interned, not inlined — once you can afford an id table.**
   Ghostty: `style_id: u16` into a per-page `RefCountedSet` with resurrection of zero-ref entries
   and a Robin Hood probe-length DoS guard; overflow escalates through *double the page's style
   capacity → re-clone the page → split the page → fall back to default style*, and the
   contract is that setting the **default** style can never fail.
   Rio: `style_id: u16` + `extras_id: u16` into content-hash-interned `StyleSet`/`ExtrasTable`
   with mark-and-sweep GC.
   wezterm and vt100 deliberately did **not** intern, and pay 16–24 B/cell for it.
   Mitchell Hashimoto's stated target is *"low numbers of unique styles"* — beta telemetry showed
   users rarely exceed 16 ([discussion #4837](https://github.com/ghostty-org/ghostty/discussions/4837)),
   which is exactly the regime interning wins in.

3. **Damage is a monotonic stamp or a bit inside a header you already write — never a side table.**
   wezterm's `SequenceNo` is the most expressive: multi-consumer (local GUI, mux clients,
   `wezterm cli` all keep their own watermark), needs no clear pass, and doubles as the
   invalidation key for a shaping cache. Ghostty's `Row.dirty` costs nothing because
   `Row.reset()` is already a single 8-byte store, and it is consumed per *page chunk* so the
   page-level flag hoists work out of the row loop. alacritty's `Vec<LineDamageBounds>` is the
   weakest of the three: viewport-only, single-consumer, and it escalates to `Full` on any scroll.

### 2.3 Renderer hand-off — three positions, and what they cost

| Position | Who | Cost | Failure mode it avoids |
|---|---|---|---|
| **GUI walks the live grid under the lock** | wezterm (`with_lines_mut` visitor) | Zero copies, but frame time is inside the critical section | — |
| **Consumer copies a snapshot under a short lock** | alacritty embedders, **OneTerm today** | One viewport memcpy per frame (~173 KB at 160×45) | Paint never blocks the parser |
| **Engine builds a flattened, pre-shaped buffer and swaps** | Contour (`RenderDoubleBuffer`), Ghostty (`RenderState` + two-phase `beginUpdate`/`endUpdate`) | One build per *changed* frame, done on the terminal thread; the expensive style denormalisation happens outside the lock | Paint never blocks the parser **and** the renderer never re-walks unchanged rows |

Ghostty's history here is instructive and directly relevant to OneTerm, because OneTerm is
currently in position two. From `src/terminal/render.zig`:

> "Previously, our renderer would use `clone` to clone the screen within the viewport to perform
> rendering. This worked well enough that we kept it all the way up through the Ghostty 1.2.x
> series, but the clone time was repeatedly a bottleneck blocking IO."

They also had to solve a second problem that OneTerm's `FairMutex` choice already anticipates but
does not fully solve — lock fairness under sustained output. `src/renderer/State.zig`:

> "Both `std.Thread.Mutex` and os_unfair_lock are unfair: a running thread that unlocks and
> immediately relocks beats a sleeping waiter every time … Under sustained pty output the IO parse
> thread is exactly such a loop, so without this signal the renderer can starve for as long as the
> output lasts."

Their answer is an explicit `demand` atomic plus `yieldToDemand()` called by the IO thread at
batch boundaries, with a 1 ms handoff timeout. alacritty's answer is the coarser
`MAX_LOCKED_READ = 65 535` byte cap on how long the parser may hold the lock **[local]**.

### 2.4 Threading models

| Engine | Model |
|---|---|
| alacritty / OneTerm | PTY reader thread parses under `FairMutex<Term>`; GUI snapshots under the same lock. 1 MiB read buffer, lock released every 64 KiB **[local]** |
| wezterm | `Mutex<Terminal>` one layer up in `mux::LocalPane`; GUI holds it for a frame's line walk |
| Ghostty | Four roles: app/GUI thread, renderer thread (libxev loop), termio **writer** thread, and an `"io-reader"` thread running a two-stage gather/parse pipeline — `buffer_count = 4`, `buffer_capacity = 64 * 1024`, one batch = one lock acquisition; the gather stage blocks when the ring fills, applying kernel backpressure to the child |
| Contour | Terminal thread owns `_stateMutex` (plain non-recursive), builds the render back buffer, swaps; GUI only ever holds the render-buffer reader lock. Callbacks on the parser thread are explicitly forbidden from reading terminal state |
| `libghostty-vt` Rust bindings | **All handle types are `!Send + !Sync` by design**; the documented pattern is "own the terminal on one thread, talk to it over channels" |

That last row is a hard architectural constraint, not a detail: OneTerm's
`Arc<FairMutex<Term>>` shared between the pump and the GPUI thread does not port to
`libghostty-vt` without changing the ownership model to message passing.

---
### 2.5 Second matrix — kitty, foot, libvterm, xterm.js

These four are not adoption candidates (two are C, one is GPL, one is TypeScript) but each owns a
design idea worth taking. Versions read 2026-09-12: kitty 0.48.2+, foot 1.28.0, libvterm 0.3.3
(neovim mirror), xterm.js 6.0.0.

| | **kitty** (C, **GPL-3.0**) | **foot** (C, MIT) | **libvterm** (C, MIT) | **xterm.js** (TS, MIT) |
|---|---|---|---|---|
| Bytes/cell | **32** — split `CPUCell` 12 B + `GPUCell` 20 B in two parallel arrays | **12** — `{ char32_t wc; struct attributes attrs; }` where attrs is exactly 2 × u32: 8 flag bits + 24-bit fg, then 8 state bits + 24-bit bg | **~40+** — `uint32 chars[6]` inline, unconditional | **12** — three `uint32` in one `Uint32Array` per line: `content` (width 2 / combined 1 / codepoint 21), `fg`, `bg` (each 2-bit colour-mode tag + 24-bit value + 6 flag bits) |
| Multi-codepoint | `ch_is_idx` bit → refcounted `TextCache` interning arena, ≤24 cp/cell, **with documented GC by index remap** | `wc > 0x00200000` is a sentinel index into a side table; `CELL_SPACER` marks wide continuation | inline, capped at `VTERM_MAX_CHARS_PER_CELL 6` | `IS_COMBINED_MASK` → per-column `{[index]: string}` map (not interned, must be re-keyed on copy/reflow) |
| Scrollback | Segmented ring: `SEGMENT_SIZE 2048` rows per segment, each segment one `mmap(MAP_PRIVATE|MAP_ANONYMOUS)` *"to avoid fragmentation in libc malloc pool"*; plus a separate byte ring (`PagerHistoryBuf`) holding the scrollback as ANSI text for the pager | **Power-of-two ring of `struct row *`, NULL until first written** — `grid_row_absolute()` is a mask, not a modulo; a 10k-line scrollback costs 10k pointers until used | **None.** libvterm hands rows out via `sb_pushline` and asks for them back with `sb_popline` | `CircularList<IBufferLine>` — a ring of line objects, each its own `Uint32Array` |
| Damage | 1 bit/line (`LineAttrs.has_dirty_text`) gating **shaping only**; the GPU cell upload is a full `rows × cols × 20 B` memcpy whenever anything changed | Three tiers: `attrs.clean` per cell (free — it lives in the attribute word), `row->dirty`, and a scroll-damage list (`DAMAGE_SCROLL{,_REVERSE,_IN_VIEW,…}`); plus Wayland **buffer-age reuse** | Rect callbacks with four runtime merge levels: `VTERM_DAMAGE_{CELL,ROW,SCREEN,SCROLL}` + `flush_damage()`; consecutive identical scrollrects coalesce | Two numbers: `DirtyRowTracker {start, end}` |
| Reflow | `kitty/resize.c` — fresh dest buffers, treats scrollback + screen as one continuous stream, sentinel-terminated `TrackCursor[]` array remaps N coordinates in one pass, `memcpy` fast path when both dimensions are unchanged | `grid_resize_and_reflow(..., tracking_points[])` — same tracking-point idea | in-place in `screen.c`, opt-in via `vterm_screen_enable_reflow` | Two separate algorithms: unwrap (forward, batched index permutation) and wrap (**backwards, in place**, no temp buffer); carries a defensive *"this has been known to fail for an unknown reason"* guard |
| Parser | Hand-written switch machine (`kitty/vt-parser.c`), CSI params accumulate into a `uint64_t` + `int mult`; **SIMD** via SIMDe, compiled three times (128/256/512) with runtime dispatch | Hand-written switch, **byte-at-a-time, no SIMD** — batching lives in the printer instead | Hand-written switch; **OSC/DCS are streamed** as `VTermStringFragment {str, len, initial, final}` so a 10 MB OSC 52 paste never allocates 10 MB inside the parser | Table-driven `Uint16Array`, index `state<<8|code`, value `action<<8|next`; every codepoint ≥ 0xA0 folded onto one pseudo-byte so a byte-indexed table drives a UTF-32 stream |
| Threading | One `io_thread` that only *reads bytes*; parsing and rendering share the main thread. Handoff is a single 1 MiB buffer + one mutex, and **the reader drops the lock while parsing** | Render worker pool, semaphore-gated, each worker owning its own pixman view and its own damage region — no locking in the hot render path | None (pure library) | None (single JS thread; an async handler stack lets a handler return a Promise and resume mid-chunk) |

**The five ideas worth stealing from this tier:**

1. **foot's 12-byte cell packing.** 8 flag bits + 24-bit colour packs into exactly one `u32`, twice.
   The source comment is the whole argument: *"we want the cells to be as small as possible. Larger
   cells means fewer scrollback lines (or performance drops due to cache misses)."*
2. **kitty's `TextCache` interning with GC-by-remap.** The header states the problem and the fix
   verbatim: *"TextCache interns unique cell texts forever, so a stream of unique multi-codepoint
   cells grows it without bound. The GC mirrors the hyperlink pool design: steal the current
   entries, then have the owner (Screen) remap every live cell index via `tc_gc_map_index()`, which
   re-interns only referenced entries into the fresh cache."* This is the same problem Rio solves
   with mark-and-sweep and Ghostty with per-page `RefCountedSet` resurrection — three independent
   solutions to one problem we will also have.
3. **foot's specialised print function.** One `u8` of "is anything unusual enabled?" selects
   between `ascii_printer_fast` (~25 lines: no insert mode, no charset translation, no
   sixel-overwrite check, no OSC 8, no grapheme merging) and `ascii_printer_generic`, via a
   function pointer swapped when any bit changes. kitty's 0.49 notes describe converging on the
   same idea.
4. **xterm.js's marker fixup.** A `Marker` is an absolute line number that subscribes to
   `onTrim` / `onInsert` / `onDelete` and adjusts or disposes itself. That is how shell-integration
   decorations (OSC 133 prompt marks — OneTerm already renders these) survive scroll and trim.
5. **libvterm's streamed string fragments.** OSC and DCS payloads arrive as
   `{str, len, initial, final}` chunks rather than an accumulated buffer, so there is no length
   limit to choose and no unbounded buffer to exploit. This is the direct fix for the unbounded
   `osc_raw: Vec<u8>` in vte 0.15 **[local]**.

**Two numbers from this tier.** kitty publishes a throughput table
(`docs/performance.rst`, MB/s, AMD Ryzen 7 PRO 5850U, Linux/X11, rendering suppressed):

| Terminal | ASCII | Unicode | CSI | Images | Average |
|---|---:|---:|---:|---:|---:|
| kitty 0.33 | 121.8 | 105.0 | 59.8 | 251.6 | 134.55 |
| gnome-terminal 3.50.1 | 33.4 | 55.0 | 16.1 | 142.8 | 61.83 |
| **alacritty 0.13.1** | **43.1** | **46.5** | **32.5** | **94.1** | **54.05** |
| wezterm 20230712 | 16.4 | 26.0 | 11.1 | 140.5 | 48.5 |
| xterm 389 | 47.7 | 18.3 | 0.6 | 56.3 | 30.72 |
| konsole 23.08.04 | 25.2 | 37.7 | 23.6 | 23.4 | 27.48 |

and foot publishes a vtebench table (`doc/benchmark.md`, 2022-05-12, foot 1.12.1, i9-9900, ms,
lower is better) in which foot beats alacritty on cursor motion (10.40 vs 24.97), dense cells
(29.58 vs 97.45), light cells (4.34 vs 12.84) and unicode (11.56 vs 15.94) — but **loses on every
scrolling benchmark**. The two tables are not comparable to each other (foot has no X11 backend,
so kitty's harness excludes it), which is itself the lesson: published cross-terminal numbers
measure different things on different hardware, and none of them isolate the VT engine from the
renderer.

Finally, an xterm.js data point that is directly actionable because it isolates *one* change.
[PR #1796](https://github.com/xtermjs/xterm.js/pull/1796) introduced the `Uint32Array` parse path
plus batched print-run dispatch and fast paths for EXECUTE and CSI:

| Case | before | after | speedup |
|---|---:|---:|---:|
| PRINT throughput | 52.04 MB/s | 340.81–447.50 MB/s | **~8×** |
| CSI with params | 41.37 MB/s | 66.76–112.88 MB/s | ~2–3× |
| EXECUTE | 39.98 MB/s | 70.81–84.29 MB/s | ~2× |

Even discounting heavily for a decade of JS-engine churn, the *shape* is the finding:
**batching printable runs is the single largest parser win, and it is available before any SIMD
work.** vte 0.15 does not do it — it `memchr`s to the next ESC and then calls `print(char)` per
character **[local]**. Ghostty (`print_slice`), Contour (`print(string_view, size_t)`) and
xterm.js (`_printHandler(data, start, end)`) all do.

---
## 3. Reusable Rust crates — adopt, fork, borrow, or ignore

This section answers the owner's question directly: *is there anything we should adopt outright
instead of writing an engine?* Every candidate found on crates.io and GitHub is listed, including
the ones that turn out to be dead ends, so the search is auditable.

### 3.1 Candidate matrix

| Crate | Latest (date) | Licence | Maint. | LOC (core) | Reflow | Graphemes / 2027 | Scrollback | Damage API | Sixel | Kitty gfx | Kitty kbd | OSC 8 / 52 / 133 | Sync 2026 | DA / DECRQM | Windows | Verdict |
|---|---|---|---|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|---|
| **`rio-vt`** | 0.5.26 (2026-08-23) | MIT | active (app train) | ~15–20k | ✅ | ✅ `grapheme_lut.rs` | ✅ | ✅ public `TermDamage`/`LineDamage` | ✅ | ✅ | ✅ default-on | 8 ✅ / 52 ✅ / **133 ?** | ✅ (alacritty lineage) | ✅ | ✅ `windows-sys` target dep | **Adopt (after spike) / fork** |
| **`alacritty_terminal`** | 0.26.0 (2026-04-06) | Apache-2.0 | maintenance | ~10k (+2.5k vte) | ✅ (buggy) | ❌ | ✅ | ⚠️ viewport-only | ❌ (PR open since 2021) | ❌ | ✅ mode stack | 8 ✅ / 52 ✅ / 133 ❌ | ✅ | ✅ | ✅ incl. ConPTY `tty` | **Status quo / fork** |
| **`libghostty-vt`** (+`-sys`) | 0.2.1 (2026-07-18) | MIT / MIT-OR-Apache bindings | active, **untagged upstream** | n/a (FFI) | ✅ | ✅ | ✅ + LZ4 compression | ✅ tri-state `RenderState` | ❌ | ✅ | ✅ | 8 ✅ / 52 ✅ / 133 ✅ | ✅ | ✅ | ⚠️ builds, needs Zig 0.16 | **Borrow design; adopt only with a vendored `.lib`** |
| **`wezterm-term`** + `termwiz` | **not published** (termwiz 0.23.3, 2025-03-20) | MIT (repo `NOASSERTION`) | monorepo active | ~15k | ✅ | ✅ | ✅ | ✅ `SeqNo` (best-in-class) | ✅ | ✅ | ✅ | ✅ all three | ✅ | ✅ | ✅ (`winapi 0.3`) | **Borrow design only** |
| **`vte`** | 0.15.0 (2025-02-02) | Apache-2.0 OR MIT | slow, healthy | ~2k | — | — | — | — | — | — | ✅ mode stack in `ansi` | — | via `advance_until_terminated` | ✅ | ✅ pure Rust | **Adopt as parser layer** |
| **`vtparse`** | 0.7.0 (2025-04-19) | MIT | monorepo | ~1.6k | — | — | — | — | — | — | — | — | — | — | ✅ | **Adopt as parser layer (alt.)** |
| **`vt100`** | 0.16.2 (2025-07-12) | MIT | stale 14 mo, 21 issues, 4 forks | ~4k | ❌ | ⚠️ 22-byte cell holds combiners, no mode | ✅ | ❌ diff-whole-screen | ❌ (no DCS at all) | ❌ | ❌ | ❌ / ✅ / ❌ | ❌ | ❌ | ✅ | **Borrow the 32 B cell only** |
| **`avt`** | 0.18.0 (2026-05-05) | Apache-2.0 | healthy | ~5k | ✅ **excellent** | ❌ one `char`/cell | ✅ | ✅ `Changes { lines, scrollback }` | ❌ | ❌ | ❌ | ❌ none | ❌ | ❌ | ✅ (2 deps) | **Borrow reflow + damage API** |
| **`zellij-server::panes`** | 0.45.1 (2026-08-28) | MIT | very active | ~14k | ✅ | ? | ✅ 10 000 | ⚠️ "which lines to re-emit as ANSI" | ✅ `sixel.rs` | ✅ 0.45 | ✅ input side | 8 ✅ / 52 ✅ / 133 ? | ? | ? | ⚠️ partial | **Borrow `sixel.rs` + `kitty_graphics/`** |
| **`anstyle-parse`** | 1.0.0 (2026-02-11) | MIT OR Apache-2.0 | very active | ~1.5k | — | — | — | — | — | — | — | — | — | — | ✅ | **Ignore** — the semantic half was deliberately deleted |
| **`vterm-sys`** (libvterm) | 0.1.0 (2016-04-23) | NOASSERTION | **dead since 2016** | — | ❌ | ❌ | host-managed | rect callbacks | ❌ | ❌ | ❌ | ❌ | ❌ | partial | ❌ C on MSVC | **Ignore** |
| **`par-term-emu-core-rust`** | 0.48.0 (2026-08-30) | MIT | 63 versions in 9 mo | — | ? | ? | ✅ | ? | ✅ | ✅ | ? | ✅ | ? | ? | ✅ | **Ignore** — 1.7k recent downloads, single author, velocity/scope ratio reads generated |
| **Warp** | open-sourced 2026 | **AGPL-3.0** (engine) | active | — | — | — | — | — | — | — | — | — | — | — | — | **Ignore** — licence-incompatible with Apache-2.0 OneTerm |
| `termina`, `terminput`, `crossterm`, `termion` | — | MIT / MIT-OR-Apache | active | — | — | — | — | — | — | — | ✅ (encoders) | — | — | — | ✅ | **Ignore as engines**; `terminput` is a real candidate for the *key→bytes* layer |
| `tui-term`, `portable-pty`, `pty-process`, `shpool-vterm`, `panoptes-vt100`, `os-terminal`, `beamterm`, `ansi-parser` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | **Ignore** — wrappers, PTY transport, or parser-only |

Legend: ✅ present · ❌ absent · ⚠️ present but unsuitable · ? unverified.

### 3.2 Verdicts with reasons

**`rio-vt` — adopt as-is, after a spike; degrade to fork if the spike fails.**
It is the only crate that is simultaneously *published*, *MIT*, *Windows-capable*, *damage-tracking*,
and already shipping Sixel + Kitty graphics + iTerm2 images + kitty keyboard + mode 2027. Every
other route to that feature set means either a git pin on an unpublished monorepo or writing the
grid ourselves. It is also a legible `alacritty_terminal` descendant (`Term`→`Crosswords`,
`Cell`→`Square`), so the port is a rename-and-adapt exercise rather than a rethink, and forking it
later is tractable.

Four things the spike must settle, none of which could be answered from documentation:
1. Does `rio-vt` with `default-features = false` (dropping `corcovado` + `teletypewriter`) build
   clean on `x86_64-pc-windows-msvc`?
2. Is `Crosswords` drivable purely by feeding bytes, with no `corcovado` event loop?
3. What is the real attribution obligation? **rio-vt declares MIT, but `crosswords/`, `grid/` and
   `ansi/` are visibly derived from Apache-2.0 `alacritty_terminal`** — the `crosswords/mod.rs`
   header itself says so. This must be resolved before shipping (see §7).
4. How invasive is adding OSC 9;7 to its `performer`?

Dependency friction against our policy (`docs/agents/dependencies.md` §3) **[local]**: rio-vt's
non-optional deps are `tracing, base64, bitflags, rustc-hash, regex, regex-automata, memchr,
parking_lot, serde, smallvec, flate2, cursor-icon, simdutf, rio-unicode, rio-grapheme-width,
rio-graphics`. Of these, only **`simdutf`** is new to our lock file — and it is a C++ FFI crate,
so it adds a C++ toolchain requirement. `tracing` is already present transitively but is on our
"do not re-add" list; `windows-sys 0.61.2` is already in the graph alongside our pinned 0.59.

**`libghostty-vt` — the best engine, the worst fit for OneTerm.**
`include/ghostty/vt.h` is an umbrella over 31 headers: full terminal state, scrollback, reflow,
incremental render state for custom renderers, selection *and selection gestures*, scrollback
search, snapshot/restore, HTML/text/VT formatters, Kitty graphics, kitty keyboard encoding,
mouse/focus encoding, paste safety, idle scrollback compression, pluggable allocator. Ghostty
1.3.0's release notes state libghostty "already supports Windows" even though the GUI does not,
and Paneflow 0.8.1 (2026-07-21) shipped it as the **default backend on Windows x64 MSVC**,
replacing `alacritty_terminal`. Rust bindings are real and used — `libghostty-vt` 0.2.1 has 103k
downloads in 90 days.

Five costs, each of which alone is survivable and which together are not:
1. **Zig 0.16 on PATH and a network fetch in the build**, including Windows CI, unless we copy
   Paneflow's pattern of vendoring a prebuilt `ghostty-vt-static.lib` with recorded toolchain
   version, headers, symbols and checksums. That is a second build system in a repo that already
   maintains `vendor/refresh.sh`.
2. **No tagged release, no ABI stability.** The header banner says "not yet stable and is
   definitely going to change"; the `-sys` README says the bindings "do not guarantee
   compatibility with arbitrary installed C API revisions".
3. **`!Send + !Sync` handles** force OneTerm off `Arc<FairMutex<Term>>` onto message passing.
4. **No Sixel.** Ghostty implements Kitty graphics only. Adopting it regresses IN-0028.
5. **No ability to patch internals.** OSC 9;7 and anything else OneTerm-specific would have to be
   upstreamed into Zig or intercepted before the bytes reach the terminal.

**`wezterm-term` — borrow design only; it is not a dependency we can take.**
`GET /api/v1/crates/wezterm-term` returns 404; so do `wezterm-cell`, `wezterm-surface` and
`wezterm-escape-parser`. The only published copies are a third party's
`tattoy-wezterm-term 0.1.0-fork.5`. Depending on it means git-pinning a revision of a monorepo
with ~1 850 open issues and a `NOASSERTION` top-level licence, and dragging in `image`,
`miniz_oxide`, a full BiDi implementation, `terminfo`, `csscolorparser` and `humansize` through
workspace-inherited versions. Its *ideas* — `TeenyString`, dual-form `CellStorage`,
`SequenceNo` damage — are worth more to us than its code.

**`vte` 0.15 — keep it as the parser layer if we build our own grid.**
It is already vendored here, dual-licensed, `no_std`-capable, Windows-clean, and it has the two
APIs that matter: `advance(&[u8])` (batched since 0.14.0) and `advance_until_terminated`, which is
the correct hook for DEC 2026 synchronized output. `vtparse` 0.7 is the alternative and is
*better* on DCS and colon sub-parameter fidelity (typed `CsiParam`, explicit `apc_dispatch`), at
the cost of ecosystem gravity and no print-run batching.

**`vt100` and `avt` — mine them, don't adopt them.**
`vt100`'s `Cell` is the cleanest packed layout in the survey: exactly 32 bytes,
`{ contents: [u8; 22], len: u8, attrs: Attrs }` with wide/wide-continuation packed into the high
bits of `len`, so combining sequences accumulate naturally per cell with no side allocation. It is
disqualified as an engine three times over: no DCS at all (so Sixel and Kitty graphics are
impossible without writing that layer), no reflow (`set_size` *clears* wrap flags), and a
"clone the screen and diff it" damage model. Four independent live forks is what an
unmaintained-but-load-bearing crate looks like.

`avt`'s `Reflow` iterator and its `feed_str(&mut self) -> Changes { lines, scrollback }` damage
API are the two best free implementations of those two problems in Rust, in a few hundred lines of
Apache-2.0 code, with proptest coverage and a fuzz script. `avt` is a *playback* terminal — zero
OSC, zero DCS, zero input protocols, one `char` per cell — so it cannot be the engine, but if we
build on `vte` we should port its reflow rather than invent one.

**`zellij` — two MIT files worth reading.** `zellij-server/src/panes/sixel.rs` (18.9 KB) and
`panes/kitty_graphics/` are among the few readable, working, MIT-licensed Rust implementations of
those protocols outside wezterm; `hyperlink_tracker.rs` is a good OSC 8 reference. Depending on
`zellij-server` would link a WASM plugin host (`wasmi`), `tokio`, `prost` and a multiplexer server
into a GPUI app to reach one struct whose cell type is `pub(crate)`.

**`libvterm` bindings — dead.** `vterm-sys` 0.1.0 was published 2016-04-23 and the repo has not
moved since 2016-04-25. `libvterm-sys`, `rust-vterm` and `libvterm` do not exist on crates.io.
Even alive, it would mean a C dependency built under MSVC by every contributor, for a feature set
behind everything else here.

### 3.3 Does anything beat `alacritty_terminal` on throughput or memory today?

**Yes — with caveats on both axes.**

*Throughput.* The only published Rust-vs-Rust numbers are Rio's, from
[`raphamorim/rio-vt-benchmark`](https://github.com/raphamorim/rio-vt-benchmark) (Criterion medians,
Apple Silicon). Two incompatible number sets circulate and could not be reconciled — the
announcement blog and the benchmark repo's README disagree by ~2.8× on `ascii_plain` for the same
crate, presumably different machines and versions. Reported parse throughput, MiB/s, blog table:

| Workload | rio-vt | vt100 | alacritty_terminal |
|---|---:|---:|---:|
| `ascii_plain` | **835** | 196 | 279 |
| `alt_screen_redraw` | **588** | 231 | 282 |
| `mixed` | **302** | 221 | 254 |
| `scroll_storm` | **274** | 101 | 266 |
| `sgr_churn` | 235 | **349** | 332 |
| `unicode_wide` | 248 | 203 | **337** |

**It is not a uniform win, and the two workloads alacritty and vt100 win are the two that dominate
real traffic**: SGR churn (any TUI) and wide Unicode (CJK, emoji). Treat the parse magnitudes as
indicative only.

Two figures are consistent across both of Rio's number sets and large enough to survive the noise:

- **Resize 80×24 → 100×40: rio-vt 5.0 µs, vt100 7.5 µs, libghostty-vt 71 µs,
  alacritty_terminal 227–237 µs.** ~45× against alacritty, and rio-vt still reflows while vt100
  clips. For a GUI that resizes on every drag frame this is the single most relevant number in the
  report.
- **Screen serialization: rio-vt 4.3–4.5 µs vs vt100 18.6–19.4 µs.**

*Memory.* Structurally, both `rio-vt` and Ghostty beat alacritty by construction:

- alacritty: 24 B/cell **[local]**, always full-width rows, `Vec<Row<Cell>>`, plus one heap
  allocation and refcount per decorated cell. 200 cols × 100 000 lines ≈ 480 MB with no
  compression and no reclaim.
- `rio-vt`: 8 B/cell with interned, GC'd style and extras tables — a 3× structural reduction,
  paid for with a mark-and-sweep collector that must be correct.
- Ghostty: 8 B/cell, page-aligned arenas, interned styles, *plus* LZ4 idle scrollback compression
  ([PR #13264](https://github.com/ghostty-org/ghostty/pull/13264), merged 2026-07-09) reporting
  121 pages → 3.03 MB (6.11 % of raw), 46.53 MB saved (93.89 %), ~101 µs/page compress,
  ~26 µs/page restore, no measurable throughput cost. Nothing in Rust does this.

Mitchell Hashimoto published a direct libghostty-vs-`alacritty_terminal` memory comparison, but it
is on X behind a login wall and **the numbers could not be retrieved — flagged unverified**.

### 3.4 Does the FFI route beat a Rust rewrite?

**On engine quality, yes. For OneTerm specifically, no.** The decisive items are not performance:

- OneTerm ships Sixel today (IN-0028). `libghostty-vt` has Kitty graphics and not Sixel; adopting
  it is a feature regression on a capability we already shipped.
- OneTerm's differentiator is the OSC 9;7 agent channel (`docs/osc-agent-status.md`) **[local]**.
  Under FFI, extending OSC dispatch means upstreaming Zig or pre-filtering the byte stream before
  it reaches the terminal — the opposite of "an engine OneTerm owns for easy extension", which is
  the stated reason for this intake.
- `!Send + !Sync` handles invalidate the current concurrency design and every test helper built on
  `Arc<FairMutex<Term>>`.
- Zig 0.16 in Windows CI, or a vendored prebuilt `.lib` with a drift-detection job, is a strictly
  larger maintenance surface than the 823-line patch series we maintain today **[local]**.

The honest summary: `libghostty-vt` is what OneTerm's engine should *look like* in five years, and
is the best single source of design ideas in this report. It is not what OneTerm should *link*
in 2026.

---
## 4. Parser design

### 4.1 What Paul Williams' state diagram actually covers

[vt100.net/emu/dec_ansi_parser](https://vt100.net/emu/dec_ansi_parser) is the common ancestor of
every parser in this report. Its scope, quoted:

- **States:** ground, escape, escape intermediate, csi entry, csi param, csi intermediate,
  csi ignore, dcs entry, dcs param, dcs intermediate, dcs passthrough, dcs ignore, osc string,
  sos/pm/apc string.
- **Actions:** ignore, print, execute, clear, collect, param, esc_dispatch, csi_dispatch, hook,
  put, unhook, osc_start, osc_put, osc_end.
- **C1 (0x80–0x9F):** *"All C1 controls cancel any escape sequence, control sequence or control
  string in progress and are executed."* DCS/SOS/CSI/OSC/PM/APC transition to their states; all
  others return to ground.
- **Parameters:** *"There is no limit to the number of characters in the parameter string, but a
  maximum of 16 parameters will be processed. All parameters beyond the 16th will be silently
  ignored."* On the value ceiling: *"the supported maximum is not critical, but it must be at
  least 16383."*
- **DCS passthrough:** *"establish a channel to a handler for the appropriate control function, and
  then pass all subsequent characters through to this alternate handler, until the data string is
  terminated"*, with an "end of data" signal for cleanup.
- **Fidelity claim:** *"it is claimed that this parser will exhibit the same visible behaviour as
  any one of DEC's 8-bit ANSI-compatible terminals, from VT220 to VT525."*
- **Its own disclaimer:** *"Completeness does not mean that this state diagram contains all the
  information you need to write a terminal emulator! There are no details here of the mapping of
  character sets or of cursor movement behaviour."*

**What it does not cover, at all:**

- **UTF-8.** The document contains no mention of it; it treats A0–FF exactly like GL 20–7F.
- **BEL as an OSC terminator.** Only ST (0x9C) terminates OSC in the diagram.
- Colon sub-parameters, grapheme clustering, synchronized output, or any post-1990s extension.

### 4.2 Deviations every modern terminal adopts

| Deviation | Who does it | Notes |
|---|---|---|
| UTF-8 input | everyone | vte validates a whole run with `str::from_utf8` and carries `partial_utf8: [u8;4]` across calls **[local]**; xterm.js folds every codepoint ≥ 0xA0 onto one pseudo-byte so a 256-wide table drives a UTF-32 stream; libvterm keeps UTF-8 *out* of the parser entirely and decodes in the state layer honouring GL/GR charset designation |
| OSC terminated by BEL (0x07) | everyone | plus, in practice, by `ESC` (which then re-enters Escape, so `ESC \` = ST falls out for free), and by CAN/SUB **[local]** |
| Colon sub-parameters (`SGR 4:3`, `38:2::r:g:b`) | vte (`Params.subparams`), vtparse (`CsiParam` enum), Ghostty (`Action.CSI.SepList` + `Sep = {semicolon, colon}`), Windows Terminal (a whole extra `CsiSubParam` state + `_subParameterRanges`) | Williams has no notion of it. Ghostty documents it as its *only* deviation from the table |
| 8-bit C1 introducers (0x9B CSI, 0x90 DCS, 0x9D OSC) | **divided** | vte 0.15: **not supported** — C1 is `execute`d in ground but never introduces a sequence **[local]**. Windows Terminal: gated — `_parserMode.set(Mode::AcceptC1, _isEngineForInput)`, i.e. **the output parser rejects 8-bit C1 by default**, the input parser accepts it, and `S7C1T`/`S8C1T` were added in 1.23 |
| Raised parameter limits | all | vte: `MAX_PARAMS = 32` incl. subparams, `MAX_INTERMEDIATES = 2` **[local]**; vtparse: `MAX_PARAMS = 256`; Ghostty: `MAX_PARAMS = 24`, `MAX_INTERMEDIATE = 4` ("4 because we also use the intermediates array for UTF8 decoding"); Windows Terminal: `MAX_PARAMETER_COUNT = 32`, `MAX_SUBPARAMETER_COUNT = 6`, `MAX_PARAMETER_VALUE = 65535`; libvterm: `CSI_ARGS_MAX = 16` (the Williams number) |
| Bulk DCS/APC passthrough | Contour, Ghostty | Contour's rationale in-source: *"every byte of a DCS payload… walks the state machine individually just to be handed on unchanged, which costs more than decoding it"* |
| Synchronized output (DEC 2026) | vte (`ansi::Processor` buffers up to 2 MiB with a timeout **[local]**), Ghostty (the *renderer* skips the frame; a 1 s watchdog in termio guards a program that never ends the block) | Two legitimately different places to implement it. Ghostty's is better: buffering 2 MiB of parsed-but-unapplied bytes is a memory amplifier a malicious stream can aim at you |
| "Not handled ⇒ re-emit the original bytes" | Windows Terminal | `_SafeExecute` runs the action; `false` triggers `FlushToTerminal()`, which flushes `_cachedSequence` (a partial sequence spanning read-chunk boundaries) then the current run. **This one mechanism is the entire basis of ConPTY passthrough in both directions** and is ~20 lines |

### 4.3 Table-driven versus hand-written — the evidence

The received wisdom that table-driven is faster does not survive contact with the data.

| Engine | Approach | Notes |
|---|---|---|
| vte 0.15 | **Hand-written** `match` per state. The table macro was removed after 0.12 | `change_state` dispatches on `State` to `advance_csi_param`, `advance_osc_string`, … **[local]** |
| foot | Hand-written switch, byte-at-a-time, no SIMD | Yet foot wins several vtebench benchmarks; its batching is in the *printer*, not the parser |
| kitty | Hand-written switch + SIMD scanners | CSI params accumulate into a `uint64_t accumulator` with an `int mult` rather than per-digit `*10+d` |
| Windows Terminal | Hand-written, Williams-derived, with extra states | |
| vtparse | Checked-in static `[[u16;256];15]` table, `get_unchecked` indexed | **No print-run batching** — `parse()` is `for b in bytes { self.parse_byte(*b, actor) }` |
| Ghostty | `comptime`-generated table (`parse_table.zig`) | But the table is *bypassed* for the hot paths |
| Contour | **`constexpr ParserTable::get()`** built at compile time from readable builder calls | Best of both: no checked-in blob, no codegen step, zero runtime construction |
| xterm.js | `Uint16Array` table + three hand-written fast paths | |

The pattern: **the table is not the performance story; the fast paths around it are.** Ghostty,
Contour and xterm.js all have tables *and* bypass them for printable runs. vtparse has the
cleanest table and the worst throughput characteristics because it lacks the bypass.

### 4.4 The performance techniques, ranked by payoff per line of code

1. **Batch printable runs into one handler call.** xterm.js measured ~8× on PRINT from this plus
   the CSI fast path ([PR #1796](https://github.com/xtermjs/xterm.js/pull/1796)). Ghostty emits a
   single `print_slice` action carrying `cps: []const u32`; Contour's handler signature is
   `print(std::string_view, size_t) -> size_t`, documented as *"Optimization that passes in ASCII
   chars between [0x20 .. 0x7F]"*. **vte 0.15 does not do this** — `ground_dispatch` calls
   `performer.print(c)` per char **[local]**. This is the cheapest large win available.
2. **Scan for the next control byte with SIMD.** `memchr` 2.8.2 is already in our lock file
   **[local]** and gives runtime-dispatched SSE2/AVX2 `memchr`/`memchr2`/`memchr3`. vte already
   uses `memchr(0x1B, …)`; note it scans **only** for ESC, so C0 bytes are found by the per-char
   loop. `memchr3(0x1B, 0x0A, 0x0D, …)` would cover the common cases.
3. **Specialise the print path on a "nothing unusual is enabled" flag.** foot's
   `bits_affecting_ascii_printer` is a single `u8` and a function pointer; `ascii_printer_fast` is
   ~25 lines with no insert-mode, charset, sixel-overwrite, OSC 8 or grapheme checks.
4. **Fuse UTF-8 decoding with the control-byte scan.** kitty's `utf8_decode_to_esc` processes two
   vectors at a time and tests "is any byte ESC or non-ASCII" with a single
   `or_si(or_si(cmpeq(v1,esc), cmpeq(v2,esc)), or_si(v1,v2))`. Announced in 0.33.0 as *"a 2x faster
   escape code parser that uses SIMD CPU vector instruction"*; 0.49 added AVX-512 for a further
   *"~75% faster for multi-byte text"*. Ghostty's equivalent is
   `utf8DecodeUntilControlSeq` on Google Highway + simdutf, feeding a 4 096-codepoint buffer, and
   its measured gains (devlog 006, M3 Max) were *"7.32 ± 0.11 times faster than scalar"* for ASCII
   and *"16.60 ± 0.61 times faster than scalar"* for UTF-8 decode. **This is a large project and
   should come last** — after batching and the fast print path, it is the remaining ceiling.
5. **Carry grapheme-join state as one integer across the run.** xterm.js keeps
   `precedingJoinState: number` on the parser, resets it at every non-print action, and
   `charProperties(code, precedingJoinState)` returns width *and* join state in one packed int —
   one lookup instead of separate `wcwidth` + grapheme-break calls.
6. **Stream OSC/DCS payloads instead of accumulating them.** libvterm's
   `VTermStringFragment {str, len, initial, final}` removes the "what limit?" question entirely.
   If you do accumulate, cap it: Ghostty uses `MAX_BUF = 2048` inline with
   `MAX_ALLOCATING_BUF = 8 MiB` for OSC 52/99 only, and **truncates** rather than erroring, with
   the comment *"OSC input is untrusted, so these captures must have a finite bound"*; its DCS cap
   is 1 MiB. vte 0.15 in `std` mode and Windows Terminal's `_ActionOscPut` both have **no cap at
   all** — do not inherit that.
7. **Batch at the lock, not per byte.** alacritty releases the `Term` lock every 64 KiB
   (`MAX_LOCKED_READ`) with a 1 MiB read buffer **[local]**; Ghostty's IO reader is a two-stage
   gather/parse pipeline with `buffer_count = 4`, `buffer_capacity = 64 * 1024`, one batch = one
   lock acquisition, and the gather stage blocks when the ring fills so backpressure reaches the
   child; kitty takes its input-buffer lock *once per buffer* rather than once per escape code,
   worth *"~30%"* on escape-heavy input, and drops the lock entirely while parsing.

---
## 5. The Windows/ConPTY contract

OneTerm is Windows-first, so this is a constraint section, not a survey section. Everything here is
a requirement on whatever engine we end up with.

### 5.1 There are two ConPTYs and they behave differently

- **ConPTY v1** (`VtEngine`): conhost maintained its own screen buffer and a *renderer* diffed
  frames and re-emitted VT to the host terminal. Only sequences conhost recognised survived; the
  rest were dropped or **reordered** relative to interleaved text
  ([#8698](https://github.com/microsoft/terminal/issues/8698), open since 2021-01-03). This is what
  ships in-box on Windows 10 and older Windows 11 builds.
- **ConPTY v2** (Windows Terminal 1.22, [PR #17510](https://github.com/microsoft/terminal/pull/17510),
  merged 2024-08-01; design doc
  [`#13000 - In-process ConPTY.md`](https://github.com/microsoft/terminal/blob/main/doc/specs/%2313000%20-%20In-process%20ConPTY.md)):
  `VtEngine` deleted. Console API calls translate directly to VT, and **the application's own VT
  output is forwarded byte-for-byte**. Claimed *"2x the I/O speed for VT heavy workloads (SGR), up
  to 16x … for plaintext"*.

The forwarding mechanism is `WriteCharsVT()` in `src/host/_stream.cpp`: conhost parses the stream
to keep its own buffer coherent for Console-API readers, **and independently writes the identical
bytes to the output pipe**. Everything the application emits — known, unknown, DCS, OSC, Sixel —
reaches the host terminal unmodified.

**This is why OneTerm already ships Windows Terminal's `conpty.dll` + `OpenConsole.exe`
(1.23.2512.16003, MIT) next to `oneterm.exe`** (`THIRD-PARTY-NOTICES.md` §1) **[local]**. That
decision, originally made for correct Ctrl+C delivery, is also what buys us DCS/Sixel passthrough
on Windows 10. Microsoft's own position in
[#17313](https://github.com/microsoft/terminal/issues/17313) is that terminals wanting the new
behaviour should ship their own OpenConsole; WezTerm and Contour do the same. **Keep doing it.**

### 5.2 Two modifications conhost makes to the stream

1. **LF → CRLF** unless `DISABLE_NEWLINE_AUTO_RETURN` is set. A bare `\n` from the application
   arrives as `\r\n`.
2. **Injections.** After sequences that would disturb conhost's assumptions it splices its own
   modes back in (`stateMachine.hpp`, `InjectionType` / `GetInjections()`):
   ```
   "\x1b[?1004h\x1b[?9001h"   // after RIS: focus events + win32-input-mode
   "\x1b[?1004h"              // after DECRST of focus mode
   "\x1b[?9001h"              // after DECRST of win32-input-mode
   ```
   **You cannot turn off `?1004` or `?9001` from inside a ConPTY session.**

### 5.3 Device Attributes, DSR/CPR, DECRQM — the host answers, not conhost

`src/host/outputStream.cpp`:

```cpp
void ConhostInternalGetSet::ReturnResponse(const std::wstring_view response)
{
    // ConPTY should not respond to requests. That's the job of the terminal.
    if (gci.IsInVtIoMode()) { return; }
    ...
}
```

`AdaptDispatch` still *computes* the replies and discards them; the application's request was
already forwarded to us verbatim. Our reply goes back on the ConPTY **input** pipe, where
`InputStateMachineEngine`'s CSI switch ends in `default: return false` → `FlushToTerminal()` →
`WriteStringRaw` → straight into the client's input stream. That second half was only fixed in
[PR #17741](https://github.com/microsoft/terminal/pull/17741) (2024-08-22, v1.22); before it,
conhost *swallowed* terminal responses.

For calibration, standalone conhost's own answers (`adaptDispatch.cpp`) — what a Windows user's
applications expect a terminal to look like:

- DA1: `CSI ?61;4;6;7;14;21;22;23;24;28;32;42c` (+`;52` with clipboard write enabled). **`4` = Sixel.**
- DA2: `CSI >0;10;1c`
- DA3: `DCS !|00000000 ST`

OneTerm's Sixel patch already answers DA1 as `CSI ? 62 ; 4 c` **[local]**.

### 5.4 The startup handshake we must implement

`src/host/VtIo.cpp` (`StartIfNeeded`) sends, the moment the first client connects:

```
(only with PSEUDOCONSOLE_INHERIT_CURSOR)  "\x1b[6n"
"\x1b[c"          DA1
"\x1b[?1004h"     focus events
"\x1b[?9001h"     win32-input-mode
```

and then **blocks for up to 1 000 ms** waiting for our DA1 reply
(`_pVtInputThread->WaitUntilDA1(1000)`) when inheriting the cursor. On shutdown it sends
`"\x1b[?1004l\x1b[?9001l"`.

**Minimum viable host:** answer `ESC [ c` with DA1, answer `ESC [ 6 n` with `CSI row;col R`,
accept `?1004` and `?9001`. Answer DA1 promptly or eat a one-second stall on every
`INHERIT_CURSOR` session.

### 5.5 CPR versus F3 — a documented heuristic, not a protocol

`CSI 1;<m>R` collides with F3 in win32 terminals. Conhost issues `ESC [ 6 n` itself
(`VtIo::Writer::WriteDSRCPR`) and disambiguates our reply:

```cpp
case CsiActionCodes::CSI_F3:
    // The F3 case is special - it shares a code with the DeviceStatusResponse.
    if (_captureNextCursorPositionReport.exchange(false, ...)) { ...; return true; }
    // Heuristic: If the hosting terminal used the win32 input mode, chances are high
    // that this is a CPR requested by the terminal application as opposed to a F3 key.
    if (_encounteredWin32InputModeSequence) { return false; }
    ...
```

Consequence: **once we have sent conhost a single win32-input-mode sequence, every `CSI…R` we send
is treated as CPR. F3 must then always be sent in win32 form.** Half-adopting win32-input-mode
corrupts F3.

### 5.6 Resize, reflow drift, and why OneTerm's `KeepViewportTop` exists

ConPTY's buffer is viewport-sized. **Scrollback is the host's, and ConPTY never reflows it.**
If our reflow differs from `TextBuffer::Reflow` in any detail we drift — which is exactly what
OneTerm measured in BUG-0051 and encoded as `ResizePolicy::KeepViewportTop`
(`docs/terminal-backend.md` §5.3, DEC-0008) **[local]**.

`TextBuffer::Reflow` (`src/buffer/out/textBuffer.cpp:2703-2957`) has quirks we must match, and
they are documented in its own comments:

- **Trailing whitespace is truncated** — *"Rows don't store any information for what column the
  last written character is in. We simply truncate all trailing whitespace in this
  implementation."*
- **Double-height rows are truncated, not reflowed** — *"A pair of double height rows should
  optimally wrap as a union… But for this initial implementation I chose the alternative approach:
  Just truncate them."*
- **`REFLOW_JANK_CURSOR_WRAP`** — the cursor row is treated as if whitespace fills up to the
  cursor, *"Pretending as if there's always at least whitespace in front of the cursor has the
  benefit that the cursor retains its distance from any preceding text."* Two defensive clamp lines
  exist because without them the main loop can **deadlock**.
- **The cursor can be lost** if more text follows it than fits.

Upstream's own fix for the drift, [#18725](https://github.com/microsoft/terminal/issues/18725)
(closed, milestone v1.24, PRs #19089/#19535), is to have **conhost re-issue `ESC [ 6 n` after every
resize** (debounced) and adapt to the host's answer. So on Windows Terminal 1.24+ ConPTY:
**expect a CPR request after every resize, and answer it honestly.** That is strictly better than
our current guess-and-correct approach and the new engine should be built to support it.

Related open issues worth knowing: content loss on grow
([Discussion #16879](https://github.com/microsoft/terminal/discussions/16879)), resize races
([#4200](https://github.com/microsoft/terminal/issues/4200)), the out-of-sync megathread
([#15976](https://github.com/microsoft/terminal/issues/15976)), and secondary-screen-buffer content
polluting the main scrollback ([#17874](https://github.com/microsoft/terminal/issues/17874)).

### 5.7 `CreatePseudoConsole` flags — correct current values

From `src/winconpty/winconpty.h` on `main`:

```c
#define PSEUDOCONSOLE_INHERIT_CURSOR        (0x1)
#define PSEUDOCONSOLE_GLYPH_WIDTH__MASK      0x18
#define PSEUDOCONSOLE_GLYPH_WIDTH_GRAPHEMES  0x08
#define PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH   0x10
#define PSEUDOCONSOLE_GLYPH_WIDTH_CONSOLE    0x18
#define PSEUDOCONSOLE_AMBIGUOUS_IS_WIDE      0x20
```

**Correct the record: there is no live `PSEUDOCONSOLE_PASSTHROUGH_MODE`.** It existed up to ~1.18
(`v1.18.3181.0` also had `RESIZE_QUIRK 0x2` and `WIN32_INPUT_MODE 0x4`), was experimental, and
**0x8 has since been reused for `GLYPH_WIDTH_GRAPHEMES`**. Any advice to pass 0x8 for "passthrough"
now selects grapheme width mode instead. The v2 rewrite made "everything passes through except what
the Console API generates" the default. The glyph-width flags are a real opportunity, though: they
let us tell conhost which width rules to use, which is directly relevant to CJK/emoji column drift.

### 5.8 Pipes, chunk sizes, deadlocks

Microsoft's documentation
([creating-a-pseudoconsole-session](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session))
says only:

> "These channels are processed by the pseudoconsole system using ReadFile and WriteFile with
> **synchronous I/O** … as long as an OVERLAPPED structure is not required."

> "To prevent race conditions and deadlocks, we highly recommend that each of the communication
> channels is serviced on a separate thread … Servicing all of the pseudoconsole activities on the
> same thread may result in a deadlock."

**There is no documented maximum chunk size.** What Windows Terminal itself does
(`src/cascadia/TerminalConnection/ConptyConnection.cpp`) is the best available guidance: **one
duplex named pipe with a 128 KiB buffer, 128 KiB reads, overlapped I/O on the host side, double
buffered** — process the previous chunk while the next read is in flight. The *ConPTY* end must not
be overlapped; the host end may be. Overlapped ConPTY is still an open request
([#262](https://github.com/microsoft/terminal/issues/262)).

For comparison, OneTerm inherits alacritty's `READ_BUFFER_SIZE = 0x10_0000` (1 MiB) with
`PIPE_CAPACITY` set to the same value **[local]**. Measured transport rates on the owner's machine:
`crates/tools/src/bin/pty-throughput.rs` records **~1.2 MiB/s** against a `cmd.exe` for-loop
(producer-bound) and its own docstring records a **~30 MiB/s** plateau for a DOOM-fire-class
producer ([`perf-baseline.md`](perf-baseline.md) §4) **[local]**. Both are far below the parser's
126–253 MB/s full-pipeline rate, so on Windows the transport or the producer — not the VT engine —
is very often the ceiling.

Signal-pipe packets, if we ever drive ConPTY directly (`src/host/PtySignalInputThread.hpp`):

```cpp
enum class PtySignal : unsigned short {
    ShowHideWindow = 1,   // { ushort show }
    ClearBuffer    = 2,   // { ushort keepCursorRow }
    SetParent      = 3,   // { uint64 hwnd }
    ResizeWindow   = 8,   // { ushort sx; ushort sy }
};
```

### 5.9 What conhost emits, and therefore what we must implement

Grepped exhaustively from `src/host/VtIo.cpp`:

| Emitted | When |
|---|---|
| `\x1b[c`, `\x1b[?1004h`, `\x1b[?9001h` | session start |
| `\x1b[?1004l`, `\x1b[?9001l` | shutdown |
| `\x1b[6n` | `INHERIT_CURSOR`, and after every resize on 1.24+ |
| `\x1b[{row};{col}H` — **absolute CUP, one per line** in `WriteInfos`/`WriteScreenInfo` | Console-API output translation |
| `\x1b[0;…m` SGR | attribute runs |
| `\x1b[?25h/l` DECTCEM | cursor visibility |
| `\x1b[?7h/l` DECAWM | mirrors `ENABLE_WRAP_AT_EOL_OUTPUT` |
| `\x1b[?1049h/l` | `SetConsoleActiveScreenBuffer` |
| `\x1b[?1003;1006h/l` | mouse mode |
| `\x1b[1t` / `\x1b[2t` | window show/hide |
| `\x1b]0;<title>\x1b\\` | `SetConsoleTitle` (ST-terminated, not BEL) |
| `\x1b7` / `\x1b8` DECSC/DECRC | around out-of-band writes |
| `\x1b[H\x1b[2J\x1b[3J` | the `ClearBuffer` signal — **so `ED 3` (clear scrollback) is mandatory** |
| `\r\n` | LF translation and delayed-EOL-wrap fixups |

So: Console-API TUIs still produce **CUP-per-line spam**; VT-native applications produce whatever
they wrote, verbatim. `DECAWM` must be implemented with correct deferred-EOL-wrap semantics or
`WriteCharsLegacy`'s delayed-wrap compensation misplaces text.

### 5.10 Keyboard: win32-input-mode, and kitty keyboard's status

**win32-input-mode** (`ESC [ ? 9001 h`, spec
[`#4999 - Improved keyboard handling in Conpty.md`](https://github.com/microsoft/terminal/blob/main/doc/specs/%234999%20-%20Improved%20keyboard%20handling%20in%20Conpty.md))
encodes a full `KEY_EVENT_RECORD`:

```
ESC [ Vk ; Sc ; Uc ; Kd ; Cs ; Rc _
```

(virtual key, scan code, Unicode char, key-down flag, control-key-state, repeat count). It is the
only lossless Windows key path — VT cannot represent Ctrl+Space, Shift+Enter, key *release*, or the
raw VK/SC that Win32 console applications read through `ReadConsoleInput`. Conhost asks for it
unprompted and re-injects the DECSET if anything disables it.

**OneTerm does not implement it today** — `crates/terminal/src/key_encode.rs` emits legacy
encodings only **[local]**. This is a real gap independent of the engine question.

**Kitty keyboard protocol over ConPTY is the one item that needs an empirical test, not more
reading.** Two facts:

1. Windows Terminal itself supports KKP as of Preview 1.25.
2. Conhost **deliberately refuses to participate** when it is ConPTY —
   `adaptDispatch.cpp:2077-2128`, all four entry points bail:
   ```cpp
   void AdaptDispatch::SetKittyKeyboardProtocol(const VTParameter flags, const VTParameter mode) noexcept
   {
       // Avoid setting KKP flags in `_terminalInput` when we're ConPTY. Otherwise, we'd be translating
       // W32IM to KKP, even when KKP is not supported by the hosting terminal (possibly intentionally).
       if (_api.IsConPTY()) { return; }
       ...
   }
   ```

Mechanically it *should* work end to end — the application's `CSI > flags u` is forwarded to us
verbatim, conhost ignores it, and our kitty-encoded `CSI … u` reply hits `default: return false`
and is passed raw to the application. But conhost has *already* enabled `?9001` and re-injects it
after any DECRST, so a terminal honouring both is being asked for two encodings at once, and the
precedence rule is undocumented. **Flagged as the single highest-value experiment in this report.**

---
## 6. Conformance corpora and the spec surface

### 6.1 What we may legally reuse

| Corpus | Licence | Vendorable into Apache-2.0 OneTerm? | Size / shape |
|---|---|---|---|
| **alacritty `alacritty_terminal/tests/ref/`** | **Apache-2.0** | **Yes — best fit, same licence** | **45 tests, 46 MB** (the `grid.json` dumps dominate; the raw recordings total only 956 KB). Ships *inside the crates.io package*, so vendoring is a directory copy |
| **libvterm `t/*.test`** | **MIT** (© 2008 Paul Evans) | **Yes** | **43 files** + `harness.c` + `run-test.pl`, driven by a tiny line DSL |
| **ghostty `test/fuzz-libghostty/`** | **MIT** | **Yes** | AFL++ harnesses (`fuzz-osc`, `fuzz-parser`, `fuzz-stream`) + **~4 000 corpus seeds** covering OSC 52/66/133/3008/1337/5522, CSI intermediates, DA2, truncated ESC |
| ghostty `src/terminal/res/{glitch,rgb}.txt`, `kitty/testdata/`, `snapshot/testdata/*.hex` | MIT | Yes | small raw VT streams + image payloads + 22 snapshot fixtures |
| **wezterm `term/src/test/`** | **MIT** | **Yes (adapt, don't copy)** | ≈52 live tests; `TestTerm` derefs to wezterm's `Terminal`, so the *case bodies* port but the harness does not |
| **xterm.js unit tests** | **MIT** | Yes (as an edge-case catalogue) | `InputHandler.test.ts` 194 cases / 2 684 lines; `EscapeSequenceParser.test.ts` 161 cases / 2 666 lines. No fixture files — all inline |
| **Windows Terminal VT tests** | **MIT** | **Yes, with attribution** | `ut_parser/OutputEngineTest.cpp` 64 `TEST_METHOD`; `InputEngineTest.cpp` 25; `ut_adapter/adapterTest.cpp` 53; `ut_host/ScreenBufferTests.cpp` 116. **The authority on what ConPTY expects.** Framework is WEX/TAEF, so port the cases, not the harness |
| `vte` crate tests | Apache-2.0 OR MIT | Yes | 34 `#[test]` in `lib.rs`, 23 in `ansi.rs`, one corpus file `tests/demo.vte` |
| `vt100` crate tests | MIT | Yes | ≈42 tests + a 344-line helper module |
| **termstandard/colors** | **Unlicense (public domain)** | **Yes, unconditionally** | truecolor probe + cross-terminal matrix |
| **vttest** | MIT/X11 **+ no-advertising clause** (© 1996-2025 Thomas E. Dickey) | Yes, but see below | RELEASE 2 PATCHLEVEL 7 (2025-12-05). Menu-driven; several tests need human eyes, keyboard or mouse |
| **ctlseqs.ms** (XTerm Control Sequences) | MIT/X11 + no-advertising (© Dickey; © X Consortium) | Yes with both copyright lines *and* the clause | xterm patch #411, 2026-08-23. **The reference document**, not tests |
| **terminal-wg specifications** | CC0-1.0 | Yes (no obligation) | **Effectively dead**: last commit 2019-01-27, **zero accepted specs**, 5 stale MRs |
| **esctest / esctest2** | **GPL-2.0** | 🚫 **NO** | And it cannot run on Windows: `tty.setraw` (POSIX termios), `select.select` on a tty fd, and `xwininfo` (X11) are all hard blockers |
| **kitty `kitty_tests/`** | **GPL-3.0** | 🚫 **NO** | Read for ideas only. `parser.py` 19, `screen.py` 47, `graphics.py` 27, `keys.py` 9 |
| Wikipedia comparison tables | CC BY-SA 4.0 | 🚫 No (copyleft content) | link only |
| **arewesixelyet.com** | **No LICENSE file → all rights reserved** | 🚫 No | link only; still the best Sixel adoption matrix |
| **TerminalGuide** (terminalguide.namepad.de) | MIT (© 2018-2019 Martin Hostettler) | link and attribute | **The closest thing to a real per-sequence cross-terminal matrix** (urxvt/xterm/vte/konsole/linuxvc) |
| ghostty.org/docs/vt | MIT | yes | Ghostty's *own* reference, not a cross-terminal matrix, and visibly incomplete (no SGR or DECSET page) |

**On vttest:** do not run it in CI. It is menu-driven and needs human input; its `-c commands` replay is not a substitute. **Harvest it from where it has already been frozen into byte streams** — alacritty's `vttest_*` ref recordings (`vttest_cursor_movement_1`, `vttest_insert`, `vttest_origin_mode_1/2`, `vttest_scroll`, `vttest_tab_clear_set`) and libvterm's `t/90vttest_*.test`.

**On the alacritty corpus specifically:** the recordings are real captures —
`tmux_htop`, `tmux_git_log`, `fish_cc`, `zsh_tab_completion`, `vim_24bitcolors_bce`,
`vim_large_window_scroll`, `vim_simple_edit` — plus targeted regressions (`csi_rep`,
`decaln_reset`, `deccolm_reset`, `selective_erasure`, `hyperlinks`, `zerowidth`, `origin_goto`,
`scroll_in_region_up_preserves_history`, `saved_cursor_alt`, `wrapline_alt_toggle`, `issue_855`).
Only `grid.json` is alacritty-shaped. **Take the `.recording` files, regenerate expectations with
our engine, and keep `grid.json` as a cross-check oracle while the new engine is young.**
Note our `vendor/refresh.sh` currently prunes this whole directory **[local]**.

### 6.2 The spec surface, with authoritative links

Roots: [ctlseqs](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)
(source: [`ctlseqs.ms`](https://raw.githubusercontent.com/ThomasDickey/xterm-snapshots/master/ctlseqs.ms)) ·
[xterm changelog](https://invisible-island.net/xterm/xterm.log.html) ·
[ECMA-48 5th ed.](https://ecma-international.org/wp-content/uploads/ECMA-48_5th_edition_june_1991.pdf) ·
[VT100/220/320/420 docs](https://vt100.net/docs/) ·
[VT330/340 Programmer Reference](https://vt100.net/docs/vt3xx-gp/) (sixel = ch. 14) ·
[DEC STD 070](https://archive.org/details/bitsavers_decstandar0VideoSystemsReferenceManualDec91_74264381) ·
[Williams parser](https://vt100.net/emu/dec_ansi_parser) ·
[bash/dec-modes](https://github.com/bash/dec-modes) (second opinion on contested modes).

**Kitty keyboard protocol** — [spec](https://sw.kovidgoyal.net/kitty/keyboard-protocol/).
Push `CSI > flags u`, pop `CSI < n u`, set `CSI = flags ; mode u` (mode 1 all / 2 set-only /
3 reset-only), query `CSI ? u` → `CSI ? flags u` (**silence means unsupported — pair the query with
DA1 so you don't hang**). Flags: 1 disambiguate, 2 event types, 4 alternate keys, 8 all keys as
escapes, 16 associated text. Encoding
`CSI key[:shifted[:base]] ; modifiers[:event] ; text u`; modifiers are **1 + bitmask** (shift 1,
alt 2, ctrl 4, super 8, hyper 16, meta 32, caps 64, num 128); events 1 press / 2 repeat / 3 release.
Implemented by kitty, Alacritty, foot, Ghostty, iTerm2, **Windows Terminal (Preview 1.25)**, Rio,
Warp, WezTerm, xterm.js. **xterm itself does not** — it uses modifyOtherKeys, so support both.

**Kitty graphics protocol** — [spec](https://sw.kovidgoyal.net/kitty/graphics-protocol/).
`ESC _ G <key=value,…> ; <base64 payload> ESC \`. `a=` t/T/p/d/f/a/c/q; `f=` 24/32/100(PNG);
`t=` d(direct)/f(file)/t(temp)/s(POSIX shm — **named shared memory on Windows**); `m=1` chunking at
4096 base64 bytes; `i=`/`I=` ids; `c=`/`r=` cell size; `z=` z-index (negative = under text);
`C=1` don't move the cursor. Deletion `a=d,d=` with lowercase = placement only, **uppercase also
frees the data**. Unicode placeholders: transmit `U=1`, then draw **U+10EEEE** cells whose
foreground colour is the image id, underline colour the placement id, with combining diacritics
encoding row then column — this is how tmux survives redraws. Adopted by kitty, Ghostty, WezTerm,
Konsole, wayst, Warp, st (patched), xterm.js, iTerm2 (partial). **Windows Terminal: no.**

**Sixel** — `DCS P1;P2;P3 q <data> ST`. `P2=1` means *leave 0-bits unchanged* (transparent).
Data bytes `?`…`~`, value = byte − 0x3F, six vertical pixels, LSB at top. Commands: `"Pan;Pad;Ph;Pv`
raster attributes (emit first so the terminal can pre-allocate), `#Pc` select / `#Pc;Pu;Px;Py;Pz`
define (**`Pu=2` is RGB in percent 0-100, not 0-255**), `!Pn` repeat, `$` graphics CR, `-` graphics NL.
**XTSMGRAPHICS** `CSI ? Pi ; Pa ; Pv S` → `CSI ? Pi ; Ps ; Pv S`, `Pi` 1 colour registers /
2 sixel geometry / 3 ReGIS, `Pa` 1 read / 2 reset / 3 set / 4 read-max.

> **DECSDM (mode ?80) is inverted in most terminals.** The hardware-correct semantics, confirmed on
> real VT340/VT382 hardware: **set = sixel *display* mode = scrolling disabled** (image anchored at
> the top-left of the graphics page, cursor does not move). The VT330/340 manual documents it
> backwards; that error propagated into xterm and from xterm into foot, mintty, contour, mlterm and
> jexer. xterm fixed it in **patch #369**; foot in
> [issue #631 / PR #632](https://codeberg.org/dnkl/foot/issues/631) (2021-07-16);
> mintty in [#1127](https://github.com/mintty/mintty/issues/1127);
> contour in [#287](https://github.com/contour-terminal/contour/issues/287).
> **Recommendation: implement the hardware-correct semantics and put the legacy inversion behind a
> config flag** — content in the wild was authored against both. Also support `?8452` (cursor ends
> to the *right* of the graphic rather than on the line below).

**iTerm2 inline images** — `ESC ] 1337 ; File=<k>=<v>;… : <base64> BEL`. Note the **`:` before the
payload**. Args: `inline`, `name`, `size`, `width`/`height` (`N` cells, `Npx`, `N%`, `auto`),
`preserveAspectRatio`, `doNotMoveCursor` (a WezTerm extension since adopted), `type`. Sibling OSC
1337 sub-commands worth parsing: `SetUserVar`, `CurrentDir`, `RemoteHost`,
`ShellIntegrationVersion`.

**OSC catalogue** (the ones that matter for OneTerm, beyond what §1.7 shows we already handle):

| OSC | Form | Note |
|---|---|---|
| 4 / 10-19 / 104 / 110-112 | set + `?` query; `OSC 104 ST` with no param resets the whole palette | replies use 16-bit-per-channel `rgb:RRRR/GGGG/BBBB` |
| **7** | `OSC 7 ; file://<host>/<pct-encoded-abs-path> ST` | **No formal spec.** Apple Terminal origin, popularised by VTE. kitty uses a `kitty-shell-cwd://` variant |
| **8** | `OSC 8 ; params ; URI ST` … `OSC 8 ; ; ST` | de-facto spec is [egmontkob's gist](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda); only `id=` is defined, URI ≤ 2083 bytes, `id` ≤ 250 in VTE |
| **52** | `OSC 52 ; Pc ; Pd ST`, `Pd = ?` reads back | **read exfiltrates the user's clipboard; write enables paste injection.** Most terminals disable read by default; alacritty disabled paste by default in 0.13.0; OneTerm gates both in `security_policy.rs` **[local]** |
| **133** | `A` prompt start · `B` input start · `C` output start · `D[;exit]` done · `P;k=v` | FinalTerm origin; closest thing to a spec is the [freedesktop semantic-prompts proposal](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md) — which `crates/terminal/src/osc.rs` already cites **[local]** |
| 633 | VS Code's superset of 133, incl. `E ; <commandline> [; nonce]` | worth parsing |
| **9** (iTerm2) | `OSC 9 ; <message> ST` — a single field, no sub-code | what most agent CLIs emit today |
| **9;4** (ConEmu) | `ESC ] 9 ; 4 ; st ; pr ST`, st 0 remove / 1 set 0-100 / 2 error / 3 indeterminate / 4 paused | widely adopted: Windows Terminal, Ghostty 1.2, ConEmu, WezTerm, mintty, Konsole |
| **9;9** (ConEmu) | `ESC ] 9 ; 9 ; "cwd" ST` — a plain path, no `file://` | **the Windows-native OSC 7**; Windows Terminal adopted it |
| 777 | `ESC ] 777 ; notify ; <title> ; <body> ST` | urxvt dispatcher family |
| 99 (kitty) | `ESC ] 99 ; <metadata> ; <payload> ESC \` | structured notifications; ≤ 2048 raw / 4096 encoded per chunk |
| 3008 | uapi-group hierarchical context signalling | see §6.4 |

**Private modes worth implementing**, beyond the 17 alacritty already recognises **[local]**:
**69** DECLRMM (required before DECSLRM does anything), **80** DECSDM, **1016** SGR-pixel mouse
(xterm patch #359), **2027** grapheme clustering
([terminal-unicode-core](https://github.com/contour-terminal/terminal-unicode-core)),
**2048** in-band resize, **2031** colour-scheme change notification, **8452** sixel cursor
placement, 47/1047/1048 (alacritty only maps 1049), 66 DECNKM, 1015, 45 reverse-wraparound,
1034/1036/1039 meta/alt-sends-escape, 1007 alternate scroll.

**Mode 2026 (synchronized output)** —
[iTerm2 spec](https://gitlab.com/gnachman/iterm2/-/wikis/synchronized-updates-spec) (which also
defines the legacy `DCS = 1 s ST` / `DCS = 2 s ST` form) and
[contour's doc](https://github.com/contour-terminal/contour/blob/master/docs/vt-extensions/synchronized-output.md).
Detect with `CSI ? 2026 $ p`. **Must-have for a GPUI renderer.**

**Mode 2048 (in-band resize)** — canonical spec is
[Tim Culverhouse's gist](https://gist.github.com/rockorager/e695fb2924d36b2bcf1fff4a3704bd83).
`CSI ? 2048 h` enables; the terminal sends one report immediately and one per resize:
**`CSI 48 ; rows ; cols ; height_px ; width_px t`** (rows before cols, height before width; pixels
are the text area excluding padding, 0 if unknown; colon sub-params may appear and must be ignored).
Implemented by foot, Ghostty, iTerm2, kitty. **This is strictly better than ConPTY's CPR-after-
resize dance** for applications that support it, and OneTerm should emit it.

**Mode 2031 (colour-scheme notification)** — `CSI ? 2031 h` enables unsolicited DSR on theme change;
one-shot query `CSI ? 996 n`; reply `CSI ? 997 ; 1 n` = dark, `CSI ? 997 ; 2 n` = light. Directly
relevant to a Windows app that follows the OS theme.

**Other sequences:** modifyOtherKeys (`CSI > 4 ; Pv m`, keys arrive as `CSI 27 ; mod ; key ~`) ·
XTGETTCAP (`DCS + q <hex names> ST` → `DCS 1 + r <hex>=<hex> ST`, or `DCS 0 + r ST`) ·
XTVERSION (`CSI > 0 q` → `DCS > | <name and version> ST`) ·
XTPUSHSGR/XTPOPSGR (`CSI # {` / `CSI # }`) and XTPUSHCOLORS/XTPOPCOLORS (`CSI # P` / `# Q` / `# R`) ·
DECRQM/DECRPM (reply `Ps`: **0 not recognised · 1 set · 2 reset · 3 permanently set · 4 permanently
reset** — 0 and 4 both mean "don't use it") · DECRQSS (`DCS $ q Pt ST`) ·
DECSCUSR · DECSTBM (**homes the cursor**) · DECSLRM · DECCARA/DECRARA/DECFRA/DECERA/DECSERA +
DECSACE · DECRQCRA.

> **DECRQCRA checksums are not portable.** xterm computes a 16-bit *negated* sum over a per-cell
> value derived from the character **plus attribute bits**, and **XTCHECKSUM (`CSI Ps # y`)** toggles
> five independent behaviours (negate or not; include attributes or not; include the character or
> not; blanks as 0x20 or 0x00; mask to 8 bits). Terminals picked different defaults — which is why
> esctest ships `--xterm-checksum` for the pre/post-patch-279 polarity. Usable only as a
> same-terminal self-check.

### 6.3 What alacritty does **not** implement — the concrete gap list

Read from `alacritty_terminal` 0.26.0 + `vte` 0.15 sources and cross-checked against alacritty's own
support table [`extra/man/alacritty-escapes.7.scd`](https://raw.githubusercontent.com/alacritty/alacritty/master/extra/man/alacritty-escapes.7.scd),
which uses the statuses *Implemented / Partial / Rejected*.

| Feature | alacritty | Note |
|---|:-:|---|
| **All DCS** (Sixel, DECRQSS, XTGETTCAP, DECRQCRA replies, ReGIS) | **NO** | `vte`'s `Perform::hook/put/unhook` are `debug!` no-ops. **This is the single biggest gap and it lives in the parser, not the terminal** — which is exactly why `vendor/patches/vte/0002` exists here **[local]** |
| Kitty graphics, iTerm2 OSC 1337 images | **NO** | no APC handling at all |
| **Kitty keyboard protocol** | **YES** (all 5 flags) | but **off by default** (`Config::kitty_keyboard`), and OneTerm never reads it **[local]** |
| modifyOtherKeys | **parsed then dropped** | `vte` produces `set_modify_other_keys`; `Term` implements neither method |
| **Synchronized output 2026** | **YES** | with two quirks: detection is an **exact 8-byte memcmp** of `\x1b[?2026h`, so `CSI ? 1 ; 2026 h` won't batch; and `report_private_mode(SyncUpdate)` hardcodes `Reset`, so it never reports "set" mid-frame |
| In-band resize 2048 | **NO** | not in `PrivateMode::new` |
| **Grapheme clustering 2027** | **NO** | width is per-codepoint `unicode_width` + a `Vec<char>` of zero-width chars. A ZWJ family emoji lands as base (width 2) + ZWJ in `zerowidth` + **the next emoji starting a new cell** — visibly broken |
| Colour-scheme notification 2031 / `?996` | **NO** | `device_status` handles only 5 and 6 |
| DECSLRM / DECLRMM `?69` | **NO** | `('s', [])` is unconditionally save-cursor; [issue #160](https://github.com/alacritty/alacritty/issues/160) open since 2017 |
| Rectangular ops, DECSACE, DECRQCRA | **NO** | no `$ r/t/x/z/{`, `* x`, `* y` arms |
| XTVERSION, XTGETTCAP, XTPUSHSGR/POPSGR, XTPUSHCOLORS, XTSMGRAPHICS | **NO** | `('q',[b' '])` is DECSCUSR only; no `#` intermediates |
| **DECRQM / DECRPM** | **YES** | quirk: `?3` DECCOLM reports `NotSupported` although `set_private_mode` does run `deccolm()` |
| OSC 8 hyperlinks, OSC 52, OSC 4/10/11/12 + queries, 104/110/111/112 | **YES** | OSC 52 paste disabled by default since 0.13.0 |
| **OSC 7, 9, 9;4, 9;9, 133, 777, 99, 1337** | **NO** | vte dispatches 13 OSC numbers; everything modern falls through — which is why `vendor/patches/vte/0001` adds `report_osc` **[local]** |
| **Blinking text (SGR 5/6/25)** | **NO** | `vte` produces the attrs; `Term::terminal_attribute` has no arm, and `cell::Flags` has no `BLINK` bit |
| **Overline (SGR 53/55)** | **NO — explicitly rejected** | manpage marks SGR 11-19 and 51-55 Rejected |
| Underline styles `4:0-5` + SGR 58/59 | **YES** | all five styles plus underline colour |
| **DECDWL / DECDHL / DECSWL** (`ESC # 3/4/5/6`) | **NO** | `ESC # 8` DECALN is the only `ESC #` handled **[local]** |
| Charsets | **Partial** | only `B` (ASCII) and `0` (DEC line drawing); no UK, no DEC supplemental |
| SS2/SS3, LS2/LS3, S7C1T/S8C1T | **NO** | |
| Alt screen 47 / 1047 / 1048 | **NO** (only 1049) | |
| Mouse 1000/1002/1003/1004/1005/1006 | **YES** | |
| **Mouse 1015 / 1016 (SGR-pixel)** | **NO** | no pixel-coordinate mouse path anywhere |
| DSR | **Partial** | only `CSI 5 n` and `CSI 6 n`; **no DECXCPR (`CSI ? 6 n`)**, no `?996` |
| **DA1** | **minimal** | `identify_terminal(None)` → **`CSI ? 6 c`** = plain VT102: no `4` (sixel), no `22` (ANSI colour), no `28` (rectangular editing). OneTerm's Sixel patch already overrides this to `CSI ? 62 ; 4 c` **[local]**; a new engine should advertise at least `CSI ? 62 ; 4 ; 22 c` |

### 6.4 OSC 9;7 — a correction OneTerm needs to make regardless of the engine decision

**There is no public specification for OSC 9;7 as an agent-status channel.** An exhaustive search
found no proposal, RFC, gist, issue or vendor document. `docs/osc-agent-status.md` is OneTerm's own
invention and contains no external references **[local]**.

**But the sub-code is not free.** Verified directly against
[ConEmu's ANSI escape codes documentation](https://conemu.github.io/en/AnsiEscapeCodes.html):

| Sub-code | Syntax | ConEmu meaning |
|---|---|---|
| 9;1 | `ESC ] 9 ; 1 ; ms ST` | Sleep |
| 9;2 | `ESC ] 9 ; 2 ; "txt" ST` | Show GUI MessageBox |
| 9;3 | `ESC ] 9 ; 3 ; "txt" ST` | Change tab title |
| 9;4 | `ESC ] 9 ; 4 ; st ; pr ST` | Taskbar progress |
| 9;5 | `ESC ] 9 ; 5 ST` | Wait for keypress |
| 9;6 | `ESC ] 9 ; 6 ; "txt" ST` | Execute a GuiMacro |
| **9;7** | **`ESC ] 9 ; 7 ; "cmd" ST`** | **"Run some process with arguments."** |
| 9;8 | `ESC ] 9 ; 8 ; "env" ST` | Output an environment variable |
| 9;9 | `ESC ] 9 ; 9 ; "cwd" ST` | Report CWD |
| 9;10 | `ESC ] 9 ; 10 [; n] ST` | xterm emulation mode |
| 9;11 | `ESC ] 9 ; 11 ; "txt" ST` | Comment (ignored) |
| 9;12 | `ESC ] 9 ; 12 ST` | Treat cursor position as prompt start |

`docs/osc-agent-status.md` §2 states *"Sub-codes `0..4` are taken (ConEmu misc + progress); `7` is
free and reads naturally as 'agent event'"* **[local]**. **That rationale is factually wrong:
ConEmu defines 9;1 through 9;12, and 9;7 means "execute this command line."**

The consequence is not hypothetical: an agent emitting OneTerm's `ESC ] 9 ; 7 ; <base64-json> ST`
while running under ConEmu or cmder is asking ConEmu to spawn a process with that payload as its
command line. OneTerm itself is unaffected — it only *reads* the sequence — but the **protocol we
are asking third-party agents to emit collides with a command-execution sub-code on a Windows
terminal, on a Windows-first product.**

**This is an IN-0029-adjacent finding that should be raised as its own BUG/US against the OSC 9;7
spec, not folded into the engine work.** Options, in order of preference:

1. **Move to an unused sub-code** (9;13+ or a private high number) and record the collision and the
   migration in `docs/osc-agent-status.md`.
2. Keep 9;7 and document the collision explicitly, with a note that agents must not emit it when
   `ConEmuANSI`/`ConEmuPID` is set in the environment.
3. Align the wire format with the one real written spec in this space:
   [uapi-group OSC 3008 "hierarchical context signalling"](https://github.com/uapi-group/specifications/blob/main/specs/osc_context.md)
   — `OSC "3008;start=" CTXID *(";" FIELD) ST` / `"3008;end="`, with fields
   `type,user,hostname,machineid,bootid,pid,pidfdid,comm,cwd,cmdline,vm,container,targetuser,targethost,sessionid`
   plus `exit,status,signal`. Driven by systemd; Ghostty already parses OSC 3008.

Worth noting for scope: OSC 133 `C` + `D;<exit>` already conveys "command started / finished /
exit code" — the bulk of "working versus idle" — with zero invention, and OSC 9;4 already conveys
0-100 progress plus error/indeterminate/paused with real multi-terminal adoption. The 2025-26
practice among agent CLIs is converging on **OSC 9 / 777 / 99 notifications**, not on a bespoke
status protocol.

---
## 7. Performance references and a benchmark method that works on Windows

### 7.1 What the published numbers are worth

| Source | What it measures | Caveat |
|---|---|---|
| [alacritty/vtebench](https://github.com/alacritty/vtebench) 0.3.1 (MIT/Apache-2.0) | *"how quickly terminal emulators read from PTY output"* | Its own README: *"This benchmark is not sufficient to get a general understanding of the performance of a terminal emulator. It lacks support for critical factors like frame rate or latency. The only factor this benchmark stresses is the speed at which a terminal reads from the PTY."* |
| kitty `docs/performance.rst` | MB/s per category with rendering suppressed | Linux/X11 only, so foot/iTerm2/Terminal.app are excluded; not comparable to foot's table |
| foot `doc/benchmark.md` (2022-05-12) | vtebench wall-clock ms | Old; foot is a CPU renderer, so it wins partial updates and loses scrolling |
| Ghostty `ghostty-bench` | Isolated engine actions, fed from a **file, not a PTY** | The most honest harness of the lot; see below |
| Rio's [rio-vt-benchmark](https://github.com/raphamorim/rio-vt-benchmark) | Criterion medians, library-vs-library | Vendor-published, unreproduced, two contradictory number sets |
| xterm.js [PR #1796](https://github.com/xtermjs/xterm.js/pull/1796) | Before/after on one change | A decade old; useful for *shape*, not magnitude |

Mitchell Hashimoto's framing in
[ghostty discussion #4837](https://github.com/ghostty-org/ghostty/discussions/4837) is the right
prior for all of it:

> "Ghostty tends to do rather poor on synthetic benchmarks."
> "Every cell changing rapidly, large numbers of unique styles, etc. are all pathologically bad
> scenarios that Ghostty is not optimized for."
> vtebench-style tests are "fairly poor measures of real world behaviors" — he matched Alacritty on
> that benchmark while Ghostty was "unusably slow in real applications like Neovim".

And the gap nobody has closed: *"input latency is the one metric we have never once reliably
measured or optimized."*

### 7.2 Why vtebench cannot be our harness on Windows

The benchmarks are **POSIX shell scripts**. `benchmarks/dense_cells/benchmark` opens with:

```sh
#!/bin/sh
tty="/dev/$(ps -o tty= -p $$)"
columns=$(tput cols < $tty)
lines=$(tput lines < $tty)
```

`ps -o tty=`, `/dev/<tty>`, `tput`, `seq` — none of that works against ConPTY. Even ported, running
through a real PTY measures **the ConPTY transport**, which our own `pty-throughput` probe already
shows plateaus near ~30 MiB/s **[local]** — well below any engine's parse rate, so the engine would
never be the bottleneck under measurement.

### 7.3 The method to use instead

**Most of this already exists.** [`perf-baseline.md`](perf-baseline.md) built exactly the right
harness — a scratch crate path-depending on `vendor/`, feeding synthetic byte streams straight into
`Processor::advance` against a `Term<VoidListener>`, with a parser-only column obtained by running
the same bytes through a default (no-op) `Handler`. The work below is to **promote that harness into
`crates/tools`, add the two dimensions it does not cover (resize latency and RSS), and widen the
fixture set** — not to start over.

Split the measurement exactly as Ghostty does — generation, transport, and engine are three
different experiments.

**1. Fixture generation, once, offline.** Port vtebench's twelve generators (`cursor_motion`,
`dense_cells`, `light_cells`, `medium_cells`, `scrolling`, `scrolling_{top,bottom}{,_small}_region`,
`scrolling_fullscreen`, `sync_medium_cells`, `unicode`) to a small Rust generator binary in
`crates/tools`, writing **fixed-size byte files** for a fixed geometry (e.g. 160×45 to match
`pty-throughput`). Fixed geometry removes `tput` and makes runs comparable across machines. Add
OneTerm-specific fixtures: a Sixel-heavy stream, an OSC 9;7 agent-status stream, a CJK/emoji
stream, an SGR-churn stream (the workload alacritty *wins* in Rio's table), and a captured real
session — Ghostty's stated direction is *"native session recording to extract real workload data"*,
and alacritty's own event loop already has the recording hook (`writer.write_all(&buf[..])`
**[local]**), which is how `tests/ref` recordings are made.

**2. Engine benchmark: feed bytes directly, no PTY.** This is what Ghostty's `TerminalStream.zig`
does, and its comment is the whole design:

```zig
// This buffer size matches the read buffer size used by the
// real IO thread (see termio Exec.zig buffer_capacity) so that
// the benchmark exercises the stream with realistic chunk sizes.
var buf: [64 * 1024]u8 = undefined;
```

with an unbuffered reader *"avoiding a per-chunk memcpy through an intermediate reader buffer that
would pollute the measurement."* In Rust: a `criterion` or `divan` bench in the engine crate that
`mmap`s or reads the fixture, chunks it at 64 KiB, and calls `advance(&mut term, chunk)` in a loop.
Report MiB/s. Measure three tiers separately, as Ghostty does with `terminal-parser` versus
`terminal-stream`:

- parser only (a no-op handler),
- parser + grid writes (the real terminal),
- parser + grid + one snapshot/render-state build per simulated frame.

That third tier is the one that would have caught OneTerm's per-frame viewport copy.

**3. Resize benchmark, separately.** `Term::resize(lines, cols)` on a filled 80×24 grid, 100 k
scrollback, timed with Criterion. This is where the published gap is largest (alacritty 227 µs vs
rio-vt 5.0 µs) and where a GUI feels it on every drag frame. Ghostty has a dedicated
`ghostty-bench +terminal-resize` action for exactly this reason.

**4. Memory benchmark.** RSS after filling N scrollback lines with plain / unicode / heavily-styled
/ mixed content, matching the categories Ghostty used for its libghostty-vs-alacritty comparison.

**5. Transport benchmark stays where it is.** `crates/tools/src/bin/pty-throughput.rs` already
isolates ConPTY **[local]**; keep it as the ceiling reference so engine numbers are read against it.

**6. Harness hygiene**, from `src/benchmark/AGENTS.md`: do not pipe the generator into the
benchmark (*"That mixes generation cost into the measurement"*), compare two renamed binaries with
`hyperfine`, and never run benchmarks in parallel.

### 7.4 What actually dominates

**Start from the local measurement, not from the literature.**
[`perf-baseline.md`](perf-baseline.md) §4 fed ~100 MiB of synthetic input straight into
`Processor::advance` against a 200×50 `Term` on the owner's machine (i7-12700, release, 3-run
median). The decisive column pair:

| Scenario | Full parse + grid | Parser only |
|---|---:|---:|
| plain ASCII lines | 126.5 MB/s | 1 191.0 MB/s |
| long unwrapped lines | 146.3 MB/s | 1 165.1 MB/s |
| heavy 24-bit SGR per char | 253.5 MB/s | 358.6 MB/s |
| cursor-movement TUI redraw | 194.1 MB/s | 420.1 MB/s |
| CJK wide chars | 163.4 MB/s | 1 069.6 MB/s |
| vtebench-style `dense_cells` | 238.8 MB/s | 322.0 MB/s |
| vtebench-style `scrolling` | 145.1 MB/s | 1 321.8 MB/s |

**The grid-mutation half of `advance` costs 3–8× the parsing half.** That single fact reorders every
recommendation in this report: `vte` is not the problem, `Term`'s `Handler` implementation is. It
also explains the counter-intuitive ordering — SGR-dense content is the *fastest* full-pipeline
scenario (253 MB/s) because each escape sets one cheap attribute, while plain text is the *slowest*
(126 MB/s) because it pays cell-write and wrap bookkeeping on every byte.

With that anchor, the cost centres in priority order:

| Rank | Cost centre | Evidence |
|---|---|---|
| **1** | **Grid writes, wrap bookkeeping, erase** | Local: 3–8× the parser cost **[local]**. Externally: Ghostty's false-positive-only `Row.styled` flag is worth *"around 4x"* on erase; foot's `ascii_printer_fast`; kitty's *"~35%"* from prefetching scrollback memory before writing |
| **2** | **Reflow** | 227 µs versus 5 µs at 80×24 is a 45× spread on an operation a GUI performs on every drag frame. Not covered by the local baseline — **measure it** |
| **3** | **Per-byte dispatch in the parser** | Real but second-order here: xterm.js's ~8× from batching print runs, Contour's bulk DCS passthrough, and the fact that Ghostty, Contour and xterm.js all bypass their own tables for printable runs. Locally the parser already runs at 322–1 321 MB/s **[local]** |
| **4** | **A second pass over already-parsed bytes** | `LineAccounting::observe` falls back to `bytes.iter().filter(|&&b| b == b'\n').count()` once scrollback is full — a full extra byte scan, active in most of the numbers above (`perf-baseline.md` §5.6, tracked as PERF-19) |
| **5** | **Lock contention under sustained output** | `try_lock_unfair()` normally lets the renderer win, but the `unprocessed >= READ_BUFFER_SIZE` fallback to `lock_unfair()` can make the render thread wait for a full 1 MiB batch (`perf-baseline.md` §5.8). Externally: Ghostty's demand-signalling mutex; kitty's *"~30%"* from locking once per buffer instead of once per escape code |
| **6** | **UTF-8 decode + width lookup** | Ghostty devlog 006: SIMD UTF-8 *"16.60 ± 0.61 times faster than scalar"*, width LUT ~2.8×/~5×, grapheme-break table *"almost 8x"*. Locally, CJK costs 163 vs 126 MB/s **[local]** — heavier per byte, still not a bottleneck |
| **7** | **Snapshot / hand-off to the renderer** | Ghostty abandoned the viewport clone because it *"was repeatedly a bottleneck blocking IO"* — but **at OneTerm's grid sizes it measures 29.9 µs/frame, 0.18 % of a 60 Hz budget** **[local]**. An architectural improvement, not a hot spot |
| **8** | **On Windows: the transport, at realistic rates** | `pty-throughput` against a `cmd.exe` for-loop measures **~1.2 MiB/s** — producer-bound, two orders of magnitude below the parser (`perf-baseline.md` §4). The tool's own docstring records a ~30 MiB/s plateau for a DOOM-fire-class producer **[local]**. Either way, at ordinary shell and SSH rates the VT engine has **2–3 orders of magnitude of headroom** |

**The honest conclusion, and it is the most important sentence in this document:**
*at realistic workloads the current engine is not a measurable bottleneck.* The rewrite must be
argued from **feature gaps, extension cost, and fork maintenance** (§9.0), not from throughput —
unless the target is deliberately pathological full-screen TUI animation at tens of MB/s, in which
case the lever is the **grid-mutation side**, not the parser.

---

## 8. Licensing

### 8.1 What OneTerm is bound by today

OneTerm is **Apache-2.0** (root `LICENSE`, `[workspace.package] license`), with a `NOTICE` file, and
enforces its dependency licence policy mechanically through `deny.toml` +
`cargo deny check licenses bans advisories` in CI **[local]**. The allow-list is: Apache-2.0
(+ LLVM-exception), MIT, MIT-0, BSD-2/3-Clause, ISC, Zlib, 0BSD, CC0-1.0, Unicode-3.0, Unlicense,
BSL-1.0, MPL-2.0, bzip2-1.0.6 — with crate-scoped GPL exceptions for `zlog`/`ztracing`/
`ztracing_macro` and `oneterm-tools` only.

`THIRD-PARTY-NOTICES.md` is **generated** by `scripts/third-party-notices.py` and verified in CI
with `--check`. Its §2 table is hand-written in the script's `HEADER` and currently reads
**[local]**:

| Crate | Upstream | Base revision | Licence | OneTerm delta |
|---|---|---|---|---|
| `vte` 0.15.0 | crates.io | 0.15.0 | Apache-2.0 OR MIT | `vendor/patches/vte/` |
| `alacritty_terminal` 0.26.1-dev | zed-industries/alacritty | `fcf32fea…` | Apache-2.0 | `vendor/patches/alacritty_terminal/` |

with the statement *"Each fork is pristine upstream plus the listed patch set; the upstream
`LICENSE-*` files are kept inside each vendored tree."* `NOTICE` repeats the same claim in prose.

**Any engine decision changes all four of these:** the `deny.toml` allow-list (probably not), the
`HEADER` §2 table in `scripts/third-party-notices.py`, the `NOTICE` prose, and
`docs/agents/dependencies.md` §1's locked-family table.

### 8.2 What we may copy, and under what obligation

| Source | Licence | May we copy code? | Obligation if we do |
|---|---|---|---|
| `alacritty_terminal`, `vte` | Apache-2.0 (vte also MIT) | **Yes** | Retain copyright/patent/attribution notices; **state changes made** (Apache §4(b)); carry the `NOTICE` content. Already handled by our patch-series discipline |
| alacritty `tests/ref` recordings | Apache-2.0 | **Yes** | Same. Note our `vendor/refresh.sh` currently **prunes** them from the vendored tree **[local]** — re-adding them is a deliberate step |
| Ghostty / `libghostty-vt` | MIT | **Yes** | Retain the MIT notice (`Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors`). Source files carry no per-file headers, only doc comments; third-party attributions appear inline where code is derived. Note `pkg/highway` and `pkg/simdutf` are separately licensed if we port the SIMD decoder rather than the scalar path |
| wezterm / termwiz / vtparse | MIT (repo top-level is `NOASSERTION`) | Yes for the MIT-declared crates | Retain MIT notice. **Resolve the top-level `NOASSERTION` before copying anything not covered by a crate-level declaration** |
| `rio-vt` | declares **MIT** | ⚠️ **Open question** | `rio-vt/src/crosswords/mod.rs` carries the header *"originally taken from … alacritty_terminal … which is licensed under Apache 2.0 license."* MIT-declared-over-Apache-derived. Apache-2.0 permits relicensing derivatives under MIT-compatible terms only if the Apache notice/attribution obligations are still met. **Read rio-vt's per-file headers and satisfy both before shipping** |
| foot | MIT | **Yes** | Retain notice |
| libvterm | MIT (`Copyright (c) 2008 Paul Evans`) | **Yes** | Retain notice |
| xterm.js | MIT | **Yes** | Retain notice |
| zellij (`sixel.rs`, `kitty_graphics/`) | MIT | **Yes** | Retain notice; attribute the file origin in our source header |
| `vt100` | MIT | **Yes** | Retain notice |
| `avt` | **Apache-2.0** (single, not dual) | **Yes** | Apache §4(b) "state changes" applies |
| **kitty** | **GPL-3.0** | **NO** | Read for design only. Ideas are not copyrightable; code and close transcription are. Our `deny.toml` would reject it and our binary is Apache-2.0 |
| **Warp** | **AGPL-3.0** (engine; only `warpui`/`warpui_core` are MIT) | **NO** | Incompatible |
| **vttest** | Thomas Dickey's terms | see §6 | Runnable as a tool regardless; vendoring is the question |
| Windows Terminal source | MIT | **Yes** | We already redistribute its binaries under MIT (`THIRD-PARTY-NOTICES.md` §1) |

### 8.3 Practical rules for this project

1. **Design ideas carry no obligation.** Bit layouts, "intern the styles", "dual-form lines",
   "sequence-number damage" are concepts. Reading GPL code and implementing the same idea
   independently is normal practice; copying GPL code is not, and our `deny.toml` enforces that at
   the dependency level. For kitty specifically, keep the separation explicit in commit messages.
2. **A from-scratch engine is the licensing *simplification*.** It removes the `vendor/` fork
   burden (823 patch lines), the §2 notices table, and the `refresh.sh --check` CI job — replaced
   by a normal Apache-2.0 workspace crate whose notices are whatever it *chooses* to borrow.
3. **Adopting `rio-vt` is the licensing *complication*.** It adds an MIT dependency whose provenance
   chain runs through Apache-2.0 code with an unresolved relicensing question. That must be settled
   in writing (a note in `docs/license-analysis.md`) before it ships, not after.
4. **Adopting `libghostty-vt` adds a statically linked MIT C library** plus the transitive licences
   of `pkg/highway` and `pkg/simdutf`, and a prebuilt-binary provenance record of the kind
   `THIRD-PARTY-NOTICES.md` §1 already keeps for `conpty.dll`/`OpenConsole.exe` (source commit,
   toolchain version, SHA-256). That is a known, already-practised pattern here — the licensing is
   the *easy* part of that option.

---
## 9. Recommendations for OneTerm

### 9.0 The decision that comes before all the others

The intake's premise is "write an engine from scratch for performance control and easy extension".
The research supports the *goal* and qualifies the *method*:

- **Performance control — the weakest of the two arguments, and the local measurements say so.**
  [`perf-baseline.md`](perf-baseline.md) §4 records the current engine at **126–253 MB/s**
  full parse + grid on this machine, against a measured ConPTY producer rate of **~1.2 MiB/s** for a
  `cmd.exe` loop and ~30 MiB/s for a DOOM-fire-class producer. That is **2–3 orders of magnitude of
  headroom at realistic shell and SSH rates.** The snapshot copy that looks expensive costs
  29.9 µs/frame — 0.18 % of a 60 Hz budget. Where headroom *is* thin, the lever is the
  **grid-mutation side, not the parser**: the parser alone runs at 322–1 321 MB/s, so `Term`'s
  `Handler` implementation costs 3–8× what `vte` does. Separately, `rio-vt` already beats alacritty
  by ~45× on resize and ~4× on screen serialization at zero engineering cost, while alacritty *wins*
  Rio's `sgr_churn` and `unicode_wide` workloads. **Do not build the case for this intake on
  throughput.**
- **Easy extension:** this is the argument that actually holds. OneTerm's differentiators are
  Sixel (shipped), Kitty graphics (wanted), kitty keyboard (wanted), and the OSC 9;7 agent channel
  (our own spec). Today each costs a patch in a vendored fork that `refresh.sh --check` must keep
  rebasable — 823 patch lines and growing **[local]**. That cost is real and compounding, and it is
  the strongest case in the file for owning the engine.

**Recommended shape: own the engine, but do not start from an empty file.** Concretely:

> Create `crates/vt` (name TBD) as an Apache-2.0 OneTerm crate implementing the grid, cell,
> scrollback, reflow, damage and semantic layer, built on a parser we do not write
> (`vte` 0.15 initially, since it is already vendored and understood), with the designs in
> §9.1–§9.8 taken from the converged prior art. Run a **two-week measured spike against `rio-vt`
> first**, behind OneTerm's existing `TerminalSession` seam, and only proceed with the rewrite if
> the spike shows `rio-vt` cannot be extended for OSC 9;7 cheaply or cannot build clean on
> `x86_64-pc-windows-msvc`.

The spike is cheap because the seam already exists: `TerminalSession` is a trait, the UI consumes an
owned `TerminalContent` snapshot, and the alacritty blast radius is 124 references in 31 files,
79 of them inside `crates/terminal` **[local]**. The owner has said API changes are acceptable, so
the snapshot type can change shape freely.

Rank order if the spike is not run: **(1)** own a Rust engine on `vte`; **(2)** adopt `rio-vt`;
**(3)** stay on the vendored alacritty fork; **(4)** `libghostty-vt` — best engine, wrong fit
(Zig in the Windows build, `!Send + !Sync`, no Sixel, cannot patch internals).

### 9.1 Cell layout

**Recommendation: 8 bytes, packed, with interned styles — `rio-vt`'s and Ghostty's shape, not
alacritty's.**

```rust
/// 8 bytes. Zero must be a valid empty cell.
#[repr(transparent)]
pub struct Cell(u64);
// bits  0..21   content: Unicode scalar, or an index into the text arena
//       21      content_is_index  (multi-codepoint grapheme)
//       22..24  width: narrow | wide | spacer_tail | spacer_head
//       24..26  reserved (semantic content: output | input | prompt — OSC 133)
//       26..27  protected
//       27..28  has_extras (hyperlink / graphic / underline colour)
//       28..32  free
//       32..48  style_id: u16   → per-grid interned StyleSet
//       48..64  extras_id: u16  → per-grid interned ExtrasTable
```

Why this and not the alternatives:

- **Against alacritty's 24 B + `Option<Arc<CellExtra>>`**: 3× the memory, and one heap allocation
  plus a refcount per decorated cell, with `Arc::make_mut` clone-on-write on every subsequent
  write. Our pinned copy additionally has an **unbounded `Vec<char>` of zero-width characters per
  cell** **[local]**.
- **Against wezterm's inline 24 B `TeenyString` + `CellAttributes`**: elegant and allocation-free
  for the common case, but it declines to intern, so a screen of uniformly-styled text stores the
  same 16-byte attribute set 7 200 times.
- **For interning**: Ghostty's telemetry-driven target is *"low numbers of unique styles"* — under
  16 in practice. A `u16` id covers 65 535 distinct styles per grid, which is far past any real
  workload.
- **Row flags for fast paths, false-positives allowed:** copy Ghostty's `Row.styled` /
  `Row.hyperlink` / `Row.grapheme` bits, stored *inside* the row header so setting them is part of
  a store we already perform. Their measurement: ~4× on erase when a row was never styled.

**The two hard parts, both of which have three independent prior solutions — pick one deliberately:**

1. **Multi-codepoint grapheme storage.** kitty interns into a refcounted `TextCache` with
   GC-by-remap; Ghostty keys an out-of-band per-page `grapheme_map` by cell offset with 4-codepoint
   chunks (*"most skin-tone emoji are ≤ 4 codepoints"*), capped at `grapheme_max_len = 64`; foot
   uses a sentinel codepoint range into a side table; xterm.js uses a per-column map (and pays to
   re-key it on every copy and reflow — do not do that). **Recommendation: intern, like kitty and
   Ghostty**, because identical emoji sequences are common and deduplication is free once you have
   the table. Cap codepoints per cell (kitty: 24) and **plan the GC before you need it** — kitty's
   header documents the exact failure (*"interns unique cell texts forever, so a stream of unique
   multi-codepoint cells grows it without bound"*).
2. **Style-table exhaustion.** Ghostty's escalation ladder is the most thought-through: double the
   page's style capacity → re-clone the page → split the page → log and fall back to default, with
   the invariant that *setting the default style can never fail*. Rio's is mark-and-sweep GC on a
   cadence. Whatever we choose, **write the overflow path first**, not last.

### 9.2 Storage model

**Recommendation: start with a ring of dual-form rows; design the row type so pages are a later
change, not a rewrite.**

- **Ring, not pages, for v1.** Ghostty's page list is the better design at scale (memcpy-able,
  serializable, poolable, compressible) but it is also the largest single piece of engineering in
  this report — `PageList.zig` alone carries 290 inline tests. alacritty's and Rio's ring with
  modular `zero` rotation is O(1) for full-screen scroll and is well understood here.
- **Power-of-two ring size** so `grid_row_absolute` is a mask, not a modulo — foot's trick, free.
- **Lazily allocated rows.** foot's `struct row *` array is NULL until first written, so a
  100 000-line scrollback costs 100 000 pointers, not 100 000 × cols × 8 B. This alone is a larger
  memory win than the cell packing for the common case of a mostly-empty scrollback.
- **Dual-form rows.** The strongest signal in the survey is that wezterm and Contour converged on
  this independently, and Windows Terminal arrived at the cheap form as its *only* form:
  - *uniform form* — one text buffer + run-length `(width, style_id)` runs, built by the printer,
    ~170 B for an 80-column ASCII line;
  - *general form* — a flat `Vec<Cell>`, materialised on the first random-access mutation;
  - compress back to uniform on the way out of the viewport (wezterm's
    `compress_for_scrollback()`).
  This is also what makes the renderer's batched fast path possible (Contour's trivial line renders
  as one `RenderLine`).
- **Stable row identity from day one.** wezterm's `stable_row_index_offset`, Rio's
  `total_lines_scrolled: u64`, Contour's `_stableBase`/`_stableFloor`/`_generation`, Ghostty's
  per-node `serial: u64`. OneTerm needs it for OSC 133 prompt marks, search results, Sixel
  placements, selection anchors, and the agent panel. Retrofitting it is painful; a monotonically
  increasing `u64` costs nothing.
- **Markers as first-class objects.** xterm.js's `Marker` subscribing to `onTrim`/`onInsert`/
  `onDelete` is the right model, and OneTerm already renders OSC 133 prompt marks.
- **Defer**: page-aligned arenas, `madvise`/`VirtualAlloc` reclaim, and LZ4 idle compression.
  Ghostty reports 70–90 % physical-memory savings from compression, but it is a v3 feature and
  depends on the page design.

### 9.3 Reflow

**Recommendation: port `avt`'s `Reflow` iterator; carry a tracking-point array; match conhost's
quirks for the ConPTY path.**

- `avt` (Apache-2.0, proptest-covered, a few hundred lines) has the cleanest free reflow in Rust:
  a `Reflow` iterator driven by each line's `wrapped` flag, rejoining logical lines and
  redistributing them, returning the new cursor position.
- **Tracking points, not just the cursor.** kitty's sentinel-terminated `TrackCursor[] {x, y,
  dest_x, dest_y}` array and foot's `tracking_points[]` both remap N coordinates in one pass.
  OneTerm needs the cursor, the saved cursor, both selection anchors, every visible OSC 133 mark,
  and every Sixel anchor remapped. **Design the reflow API to take a slice of tracking points.**
- **Emit an old→new row remap** (Rio's `ReflowRemap`) so graphics and markers can re-anchor without
  the engine knowing about them.
- **Fast path**: if neither dimension changed, memcpy — kitty's first branch.
- **Alt screen never reflows** (wezterm gates on `allow_scrollback`).
- **The ConPTY quirk set is non-negotiable** (§5.6): truncate trailing whitespace, honour the
  wrap-forced flag, apply the cursor-row whitespace assumption. And build for the 1.24+ world where
  **conhost re-queries CPR after every resize** — answering that truthfully is strictly better than
  OneTerm's current `KeepViewportTop` correction, which reflows a scratch grid to *guess* what
  conhost did **[local]**. Keep the correction as the fallback for older conhosts.
- Learn from xterm.js's scar tissue: shrink **backwards, in place** (no temp buffer), and treat the
  defensive `if (wrappedLines[destLineIndex] === undefined) break;` with its
  *"has been known to fail for an unknown reason"* comment as a warning that this code needs
  property tests, not more asserts.
- **Budget: the target is Rio's 5 µs at 80×24, not alacritty's 227 µs.** Measure it (§7.3).

### 9.4 Damage

**Recommendation: a monotonic sequence number, plus a dirty bit inside the row header.**

```rust
pub type SeqNo = u64;                 // bumped once per advance() batch
struct Row { seqno: SeqNo, flags: RowFlags /* dirty, styled, grapheme, hyperlink, wrapped */, … }
```

- **Sequence numbers over dirty bits** (wezterm's model) because OneTerm has, or will have,
  *several* consumers of change: the terminal element, the search index, the semantic-highlight
  pass, the agent panel, the gutter timestamps, and the logging path. A watermark per consumer
  needs no reset pass and cannot be clobbered by another reader. alacritty's
  `damage()` + `reset_damage()` is single-consumer and is already a constraint OneTerm works around
  (`query_line_range_cells` exists precisely because there is *"deliberately no damage-free
  full-grid snapshot"* **[local]**).
- **Also keep a dirty bit in the row header** (Ghostty, foot) so the render pass can skip a row
  with one load, and a per-*chunk* or per-*region* flag so the page/region loop hoists work out of
  the row loop.
- **Column bounds are worth keeping** (alacritty already has `left`/`right`), and foot's per-cell
  `clean` bit is free if we have a spare bit — but only pay for it if measurements say the renderer
  is cell-bound rather than row-bound.
- **Scroll damage as a distinct event**, not as N dirty rows: foot's
  `DAMAGE_SCROLL{,_REVERSE,_IN_VIEW}` list and libvterm's scrollrect coalescing both exist because
  a fast `cat` should hand the renderer one "move N lines" instruction, not N row invalidations.
  alacritty escalates to `Full` here, which is the worst option.
- Take libvterm's **merge-level concept** (`CELL`/`ROW`/`SCREEN`/`SCROLL` as a runtime knob plus an
  explicit `flush_damage()`) only if we expose a callback API; a monolithic engine does not need it.

### 9.5 Parser

**Recommendation: keep `vte` 0.15 initially, add a batched print path, and treat SIMD as phase 3.**

1. **Phase 1 — do not write a parser.** `vte` is vendored, understood, dual-licensed, Windows-clean,
   has batched `advance(&[u8])` and `advance_until_terminated` (the correct DEC 2026 hook), and
   carries the `ansi::Handler` semantic layer with ~90 methods OneTerm already implements against
   **[local]**. Writing a state machine is the least valuable part of this project.
2. **Phase 2 — the cheap wins, in this order.**
   - **Batch printable runs.** `Perform::print_str(&str)` (or an equivalent) instead of
     `print(char)` per character. It is the single biggest measured *parser* win in the literature
     (~8× on PRINT in xterm.js) and is a small patch to `ground_dispatch` **[local]**. But note what
     the local baseline says: the parser is already the *cheap* half (322–1 321 MB/s versus
     126–253 MB/s for parse + grid) **[local]**, so the real value of batching here is not the
     parser saving — it is that it **unlocks the grid-side wins**: run-length writes into a uniform
     row, one width/grapheme pass per run, one damage stamp per run. Sequence it for that reason.
   - **Cap OSC and DCS payloads.** vte's `osc_raw` is an unbounded `Vec<u8>` under `std`
     **[local]**; this is a remote memory-exhaustion vector reachable from any SSH session. Use
     Ghostty's shape: a small inline buffer (2 KiB), a large allocating bound (8 MiB) for OSC 52/99
     only, and **truncate rather than error**. Better still, adopt libvterm's streamed
     `{str, len, initial, final}` fragments so there is no buffer to bound.
   - **`memchr3(0x1B, 0x0A, 0x0D)`** instead of `memchr(0x1B)` so newlines leave the per-char loop.
   - **foot's specialised print function**: one `u8` of "is anything unusual enabled?"
     (insert mode, charset translation, active Sixel overlap, OSC 8 open, grapheme mode) selecting
     between a ~25-line fast printer and the general one.
3. **Phase 3 — only if measurement demands it.** A fused SIMD UTF-8-decode-and-scan-for-control
   (kitty's `utf8_decode_to_esc`, Ghostty's `utf8DecodeUntilControlSeq`). In Rust this is
   `std::simd` or a `memchr`-style runtime-dispatch crate. Ghostty's measured ceiling is
   ~7× (ASCII) and ~16× (UTF-8 decode) over scalar — but only *after* the scalar path is already
   tight, and on Windows it is measured against a 30 MiB/s transport ceiling.
4. **Own the semantic layer, not the state machine.** If `vte`'s `Handler` trait becomes the
   constraint — and it will, for XTVERSION, XTGETTCAP, mode 2048, DECSLRM, rectangular ops — fork
   *that* file into our crate rather than rewriting the state machine underneath it. `vtparse`'s
   typed `CsiParam` (which preserves colon sub-parameter structure) is the better model for the new
   dispatch layer.
5. **Copy Windows Terminal's "not handled ⇒ re-emit the original bytes" mechanism.** Every action
   returns a bool; `false` flushes the cached partial sequence plus the current run back out. It is
   ~20 lines, it is the entire basis of ConPTY passthrough, and OneTerm will want it for SSH→ConPTY
   bridging and for forwarding unknown DCS to a future graphics backend.

### 9.6 Renderer hand-off

**Recommendation: design for an incremental render state — Ghostty's two-phase model, not Contour's
full double buffer — but schedule it on architectural merit, not on a performance claim.**

Today: `TerminalContent::refill` clones the whole viewport into owned `IndexedCell`s every frame
**[local]**. Ghostty ran the same design through 1.2.x and abandoned it because the clone
*"was repeatedly a bottleneck blocking IO."* **On OneTerm's machine and grid sizes it is not one:
29.9 µs/frame, 0.18 % of a 60 Hz budget** ([`perf-baseline.md`](perf-baseline.md) §4). It scales with
viewport area rather than output volume, so a very large window moves the number, and it is the cost
that forces `query_line_range_cells` to exist instead of a general snapshot — but it is not why
anything is slow today. Build the incremental model because it is the better shape, and prove the
claim with the §7.3 tier-3 benchmark rather than asserting it.

The target design:

1. **`begin_update` under the lock**: walk only rows whose `seqno` exceeds the renderer's
   watermark, copy their cells into per-row arenas in a persistent `RenderState`, record style runs,
   clear the dirty bits. Bounded by *changed* rows, not viewport size.
2. **`end_update` outside the lock**: expand style ids to concrete colours, apply selection and
   search highlights, build shaping input. Ghostty's note is that style runs let this skip
   *"the (comparatively large) per-cell style fill when a rebuilt row produced identical runs,
   which is the common case: text changes far more often than styling."*
3. **Tri-state result** (`none` / `partial` / `full`) so the element can skip layout entirely on an
   unchanged frame — OneTerm's `plan_cache` and `row_plan` already want exactly this
   (`crates/terminal-view/src/render/` **[local]**).
4. **Fair handoff under sustained output.** `FairMutex` helps but does not solve it. Add Ghostty's
   explicit demand signal: the renderer raises a flag, the pump checks it at batch boundaries and
   yields, with a short timeout. alacritty's `MAX_LOCKED_READ = 65 535` **[local]** is the blunt
   version of the same idea and should stay as a backstop.
5. **Synchronized output should skip the frame, not buffer the bytes.** vte's `Processor` buffers
   up to 2 MiB of unapplied input **[local]**; Ghostty instead applies everything and has the
   *renderer* skip frames while mode 2026 is set, with a 1 s watchdog for a program that never ends
   the block. The latter cannot be aimed at us as a memory amplifier.
6. Do **not** adopt Contour's full `RenderDoubleBuffer`. It decouples perfectly but flattens every
   changed frame into a fresh buffer; the incremental model does strictly less work for the same
   guarantee, and GPUI's element model already owns the paint side.

### 9.7 Extension points

These are the reason to own the engine, so design them as API, not as patches.

**Graphics (Sixel today, Kitty and iTerm2 next).**
- Keep OneTerm's current anchoring model — it matches what foot and Rio do. `GraphicCell { id, col,
  row }` in the cell's extras, images owned by the engine, handed to the embedder once
  (`take_graphics`) **[local]**.
- Anchor placements in **stable absolute row space**, not ring indices (Rio's
  `total_lines_scrolled`, foot's absolute ring row + `verify_no_wraparound_crossover` invariant).
- Re-anchor through the reflow remap (§9.3).
- Steal foot's two debug invariants — no two images with overlapping column ranges on the same end
  row; no image straddling the ring wrap point — as `debug_assert`s.
- Text printed over an image should **split** the image (foot's `sixel_overwrite_by_rectangle`),
  not delete it.
- Decoders: `zellij`'s `sixel.rs` and `kitty_graphics/` are MIT and readable; `icy_sixel` is a
  maintained pure-Rust encoder/decoder. `image`, `png` and `base64` are already in our graph
  **[local]**. Keep `MAX_DIMENSION` and add a **byte cap** on the DCS/APC payload itself, which the
  current code lacks.
- Unicode placeholders (kitty's U+10EEEE) need a row flag — Ghostty's
  `Row.kitty_virtual_placeholder` and kitty's `LineAttrs.has_image_placeholders` are the same idea.

**OSC.**
- The current single-pass design is right and should be kept: one parser, unknown OSCs routed to the
  embedder as `Event::Osc` **[local]**, with `crates/terminal/src/osc.rs` owning OSC 7 / 9 / 9;4 /
  9;7 / 133 and the security policy applied centrally in `osc_router.rs`.
- In an owned engine this becomes a **registered dispatch table** instead of a fallthrough: the
  embedder claims OSC numbers, the engine handles the rest. OSC 9;7 (`docs/osc-agent-status.md`)
  stops being a fork patch and becomes a registration.
- Ghostty's OSC list is the target coverage: 0/1/2, 4/5/10–19/104/105/110–119, 7, 8, 9 + 777,
  9;1…9;11 (the ConEmu family, which is where our 9;7 lives), 21, 22, 52, 66, 72, 99, 133, 1337.
  Note their parser needs bridge states for invalid prefixes — *"to support OSC 777 we need to have
  a state '77' even though there is no OSC 77."*
- Apply the security policy (`crates/terminal/src/security_policy.rs` **[local]**) at the engine
  boundary for length, control characters and rate, not only at the consumer.

**Keyboard.**
- The engine owns the *state* (kitty keyboard flag stack, modifyOtherKeys level, DECCKM, DECNKM,
  bracketed paste); the app owns the *encoding*. Expose the flags as a queryable value —
  `vte`/alacritty already track them and **OneTerm reads none of them** **[local]**.
- Ghostty's `FlagStack { flags: [Flags; 8], idx: u3 }` is the right implementation: fixed size, no
  heap, push wraps and evicts, and `pop(n >= len)` resets the whole stack — explicitly *"avoids a
  DoS vector where a malicious client could send a huge number of pop commands to waste cpu."*
- On Windows, **win32-input-mode (`?9001`) is the priority, not kitty keyboard** (§5.10): conhost
  asks for it unprompted, re-injects it after any DECRST, and F3/CPR disambiguation depends on
  using it consistently. `crates/terminal/src/key_encode.rs` needs it regardless of which engine
  wins.
- `terminput` (MIT OR Apache-2.0, active) is a plausible dependency for the key→bytes layer
  including kitty encoding, if we would rather not own it.

### 9.8 Test strategy

See §6 for what each corpus contains and what licence it carries. The strategy:

1. **Byte-feed unit tests as the spine.** `crates/terminal/src/sixel_tests.rs` already has exactly
   the right shape — `feed(&mut term, bytes)` through the real `Processor`, then assert on cells
   **[local]**. Every VT feature gets one.
2. **Re-adopt alacritty's ref tests, and extend the format.** Each directory is
   `alacritty.recording` (raw bytes) + `size.json` + `config.json` + `grid.json`, replayed with
   `parser.advance(&mut terminal, &recording)` and compared cell by cell. They are Apache-2.0, so
   we may vendor them with attribution. **Our `vendor/refresh.sh` currently prunes them**
   **[local]** — re-adding them under our own crate is a deliberate step and gives us a large
   free regression corpus that also *pins behavioural parity with the engine we are replacing*.
   Add our own recordings for Sixel, OSC 9;7 and ConPTY resize sequences; alacritty's event loop
   already shows how recordings are captured (`writer.write_all(&buf[..unprocessed])` **[local]**).
3. **Property tests where the scars are.** Reflow, specifically: `avt` uses proptest, xterm.js's
   reflow carries a *"known to fail for an unknown reason"* guard, and alacritty has six open reflow
   bugs. Properties worth asserting: text content is preserved across a resize round-trip; tracked
   points stay on their character; no row exceeds the column count; wrap flags are consistent.
4. **Invariant checking in debug builds.** Ghostty's `verifyIntegrity` with a named
   `IntegrityError` set (`ZeroRowCount`, `UnmarkedGraphemeRow`, `MissingGraphemeData`, …), invoked
   through `defer self.assertIntegrity()`, is effectively a fuzzer that runs in every debug test.
   The Rust equivalent is a `#[cfg(debug_assertions)] fn assert_integrity(&self)` called at the end
   of every mutating public method.
5. **Fuzz the parser.** `cargo-fuzz` on `advance(&mut term, data)` with a corpus seeded from the ref
   recordings. The two bugs already known to exist in our tree — unbounded `osc_raw`, unbounded
   per-cell `zerowidth` **[local]** — are exactly what a fuzzer with a memory limit finds.
6. **Conformance as a reporting tool, not a gate.** `esctest` and `vttest` drive a real terminal
   through a PTY; on Windows that means ConPTY in the loop, which conflates the engine with the
   transport. Run them on Linux CI against a headless harness, publish a pass/fail matrix like
   Ghostty's VT support table, and gate on our own tests.
7. **Differential testing during migration.** While both engines exist, feed the same bytes to the
   old and new engine and diff the resulting grids. This is the cheapest possible safety net for a
   rewrite and it works for the `rio-vt` spike too.
8. **CI additions**: the workspace gate (`scripts/ci-local.sh` **[local]**) gains the new crate's
   tests automatically. Add a bench job that runs the §7.3 fixtures and **records** numbers without
   failing on them — regression *detection*, not a flaky gate.

### 9.9 Suggested sequencing

| Phase | Outcome | Verifiable by |
|---|---|---|
| **0** | Spike: `rio-vt` behind `TerminalSession` on Windows. Answer the four open questions in §3.2 | Builds on MSVC; a terminal session runs; the four questions answered in the packet |
| **0b** | Promote [`perf-baseline.md`](perf-baseline.md)'s scratch harness into `crates/tools` and extend it per §7.3 — it already covers tiers 1-2 and the snapshot; it does **not** cover resize or memory | Recorded baseline numbers for parse, parse+grid, snapshot, **resize**, and **RSS**, all reproducible from the repo |
| **1** | Safety fixes to the current fork regardless of direction: cap `osc_raw`, cap per-cell zerowidth, fix `Row::new(0)` | Fuzz target survives; `refresh.sh --check` green |
| **2** | New crate: cell + row (dual form) + ring + damage seqno, driven by `vte`. No reflow, no graphics | Alacritty ref-test corpus replays green |
| **3** | Reflow with tracking points + remap; ConPTY quirk parity | Property tests; the BUG-0051 resize scenarios |
| **4** | Render-state hand-off replacing the viewport snapshot | Benchmark tier 3 improves; frame time under `yes` |
| **5** | Graphics moved into the engine (Sixel first, then Kitty) | IN-0028 evidence walks reproduced |
| **6** | OSC registration table + OSC 9;7 as a registration, not a patch | Agent panel unchanged; `vendor/` deleted |
| **7** | win32-input-mode + kitty keyboard encoding | The §5.10 experiment settled empirically |
| **8** | Optional: print-run batching, SIMD scan, page storage, scrollback compression | Benchmarks, in that order |

`vendor/`, `vendor/refresh.sh`, its CI job, and the §2 notices table all disappear at phase 6.

---
## 10. Ranked risks

Ordered by expected cost × likelihood. Each carries the cheapest available mitigation.

| # | Risk | Why it is likely | Mitigation |
|---|---|---|---|
| **1** | **We rewrite 12–13k LOC and land behind where we started.** Both `rio-vt` and `libghostty-vt` already clear alacritty's feature bar; a fresh engine needs a year to reach parity and then has to *beat* it. alacritty is also not uniformly slow — it wins SGR-churn and wide-Unicode in Rio's own benchmark | This is the default failure mode of every from-scratch engine project | **Run the §9.0 spike before committing.** Build the §7.3 benchmark harness *first* and record a baseline against the current engine, so "better" is a number, not an intuition. Use differential testing (§9.8.7) against the old engine throughout |
| **2** | **Reflow correctness.** alacritty has six open reflow bugs; xterm.js ships a *"known to fail for an unknown reason"* guard; Windows Terminal's `Reflow` documents a path that can **deadlock** without two defensive clamp lines. And OneTerm has a *second* reflow contract to satisfy — conhost's quirks (§5.6) | Reflow touches cursor, saved cursor, selection anchors, OSC 133 marks, search results and Sixel anchors simultaneously | Port `avt`'s proptest-covered `Reflow`; design the API around a **tracking-point slice** + an **old→new row remap** from day one (§9.3); property-test the round-trip; keep `KeepViewportTop` as the fallback and move to answering conhost's post-resize CPR on 1.24+ |
| **3** | **Grapheme + width model is a rewrite-scale decision made early and cheaply regretted.** alacritty's per-codepoint `unicode_width` + `Vec<char>` zero-width model visibly breaks ZWJ family emoji today **[local]**; mode 2027 requires UAX #29 segmentation and cluster-width measurement throughout the print path | CJK/emoji column drift is the most user-visible class of terminal bug, and ConPTY adds its own `PSEUDOCONSOLE_GLYPH_WIDTH_*` axis | Decide the storage (intern, like kitty/Ghostty) and the width source (`unicode-segmentation` 1.13.3 is already in our graph **[local]**) in the High-Level Design, not during implementation. Cap codepoints per cell and **write the interning GC before it is needed** |
| **4** | **`rio-vt`'s licence chain is unresolved.** It declares MIT while `crosswords/`, `grid/` and `ansi/` are visibly derived from Apache-2.0 `alacritty_terminal` — its own file header says so | We enforce licences mechanically (`cargo deny`) and generate `THIRD-PARTY-NOTICES.md`; a wrong answer ships in every release | Settle it in writing in `docs/license-analysis.md` **before** any adoption, not after. If it cannot be settled, treat `rio-vt` as design prior art only |
| **5** | **OSC 9;7 collides with ConEmu's "run a process" sub-code** (§6.4), and our own spec's rationale for choosing it is factually wrong | Verified first-hand against ConEmu's documentation. OneTerm is Windows-first and the protocol asks *third-party agents* to emit the sequence | Raise as a **separate BUG/US against `docs/osc-agent-status.md`**, independent of the engine work. Prefer moving to an unused sub-code; at minimum document the collision and the ConEmu environment guard |
| **6** | **Unbounded buffers in the engine we ship today.** `vte`'s `osc_raw` is an unbounded `Vec<u8>` under `std`; `CellExtra.zerowidth` is an unbounded `Vec<char>` per cell in our pinned revision; `Row::new(0)` writes through a dangling pointer in release builds. All three reachable from any SSH session **[local]** | Upstream's unreleased 0.26.1-dev changelog lists the latter two as fixed — our pin predates that | **Fix these in the current fork now (§9.9 phase 1), regardless of the engine decision.** Add a `cargo-fuzz` target with a memory limit. Adopt Ghostty's bounded-with-truncation policy or libvterm's streamed fragments |
| **7** | **`libghostty-vt` looks like the obvious answer and is a trap for this product.** Best engine in the survey; `!Send + !Sync` handles invalidate our concurrency model, Zig 0.16 enters the Windows build, there is **no tagged release or ABI stability**, and it has **no Sixel** — adopting it regresses IN-0028 | Its Rust bindings have 103k downloads in 90 days and a production Windows precedent (Paneflow), so it will keep looking attractive | Record the decision and its reasons as a DEC so it is not relitigated. Revisit only if Ghostty tags a stable C ABI *and* lands Sixel |
| **8** | **We optimise the parser because it is the legible part, while the measurement says the grid is 3–8× more expensive and neither is a bottleneck.** [`perf-baseline.md`](perf-baseline.md) §4: parser alone 322–1 321 MB/s, parse + grid 126–253 MB/s, snapshot 29.9 µs/frame, real ConPTY producer ~1.2 MiB/s | Parser work is satisfying and has the best literature; grid and hand-off work is neither | Make §7.3's **tier-3 benchmark** (parse + grid + one render-state build per frame) the primary metric from day one, and add the **resize** benchmark, which the local baseline does not cover and where the published spread is 45× |
| **9** | **Lock starvation under sustained output.** `FairMutex` mitigates but does not solve it; Ghostty needed an explicit demand signal with a 1 ms timeout because *"a running thread that unlocks and immediately relocks beats a sleeping waiter every time"* | Our pump is exactly such a loop. `MAX_LOCKED_READ = 65 535` **[local]** is the only current guard | Implement the demand/yield handshake with the new hand-off (§9.6.4); keep the byte cap as a backstop; add a "frame time under `yes`" measurement |
| **10** | **Scope creep through the extension points.** Kitty graphics is ~250 KB of source in Ghostty, whose own header admits *"The performance of this particular subsystem of Ghostty is not great… I tried to avoid pessimization but my aim to ship a v1 came at some cost"* | Every protocol in §6.2 is individually justifiable | Sequence strictly (§9.9). Graphics moves into the engine only at phase 5, **after** ref tests, reflow and hand-off are green. Kitty graphics is a separate intake |
| **11** | **Windows keyboard is a gap independent of the engine, and kitty keyboard over ConPTY is genuinely undefined.** OneTerm implements neither win32-input-mode nor any kitty encoding **[local]**; conhost refuses to participate in KKP when it is ConPTY, while simultaneously forcing `?9001` on | Half-adopting win32-input-mode **corrupts F3** through the CPR disambiguation heuristic (§5.5) | Implement `?9001` fully and send *all* keys through it. Settle KKP-over-ConPTY **empirically** — it is the single highest-value experiment in this report and needs no engine decision first |
| **12** | **Dependency-policy friction if we adopt `rio-vt`.** It pulls `simdutf` (a C++ FFI crate, new to our graph) and `tracing` (on our do-not-re-add list), and `windows-sys 0.61.2` alongside our pinned 0.59 **[local]** | `docs/agents/dependencies.md` §3 requires a design decision for new dependency categories | Resolve in the spike: check whether `default-features = false` drops `simdutf`, and whether a DEC is needed for the C++ toolchain requirement on Windows CI |
| **13** | **The PTY layer disappears with the engine.** `alacritty_terminal::tty` is our ConPTY spawn path, including the `LoadLibraryW("conpty.dll")` trick that makes us use the bundled OpenConsole **[local]**, and `docs/agents/dependencies.md` explicitly says *"do not use `portable-pty`"* | Easy to forget: the engine and the transport ship in the same crate today | Extract the `tty` module into its own OneTerm crate **first**, as a standalone step, so it survives any engine decision. It is ~1 660 lines and has no dependency on the grid |
| **14** | **Losing behaviour we cannot name.** 3 394 lines of `term/mod.rs` encode years of edge cases; 21 322 lines of `terminal-view` consume them **[local]** | Nobody can enumerate what a terminal does | **Take alacritty's 45 ref recordings as a parity harness** (§6.1). They are Apache-2.0, they are real tmux/vim/zsh captures, and replaying them pins behavioural parity with the engine being replaced — the single cheapest safety net available |
| **15** | **On Windows the engine is demonstrably not the bottleneck at realistic rates** — measured ~1.2 MiB/s for a `cmd.exe` producer and ~30 MiB/s for a DOOM-fire-class one, against 126–253 MB/s parse + grid **[local]**. A rewrite justified on speed would therefore be unfalsifiable in normal use | The performance argument is the easiest one to make and the hardest to disprove | Always report engine numbers *and* the transport ceiling together. Justify the intake on **feature gaps, extension cost and fork maintenance**. For perceived speed on Windows, frame pacing, synchronized output and reflow latency matter more than parse throughput |

## 11. Open questions this research could not close

1. **Does `rio-vt` with `default-features = false` build clean on `x86_64-pc-windows-msvc`, and can
   `Crosswords` be driven by bytes alone with no `corcovado` event loop?** Not verified by
   compilation. Decides adopt-versus-fork.
2. **The rio-vt licence chain** (MIT declared over Apache-2.0-derived source). Needs a written
   determination.
3. **Kitty keyboard protocol end-to-end through ConPTY**, and the precedence rule between
   win32-input-mode and KKP when conhost forces `?9001` on. Reasoned from source, not observed.
4. **Ghostty's published libghostty-vs-alacritty memory numbers** — behind a login wall; the
   structural argument stands, the magnitudes do not.
5. **Rio's benchmark magnitudes** — two contradictory published number sets for the same crate.
   Only the resize (5 µs vs 227 µs) and serialize (4.3 µs vs 18.6 µs) gaps are large enough to
   trust unreproduced.
6. **Whether `libghostty-vt` has landed Sixel** since the docs.rs snapshot, and whether its Windows
   build works without vendoring a prebuilt `.lib`.
7. **ConPTY v1 (in-box Windows 10 conhost) OSC forwarding whitelist** — not enumerated. Matters
   only if we support users who cannot run our bundled OpenConsole.
8. **Current DECSDM (mode ?80) polarity in WezTerm and Ghostty** — unconfirmed; affects which
   default we pick.
9. **Whether Zed intends to move off `alacritty_terminal`.** Only a stale 2024 issue
   ([zed#10791](https://github.com/zed-industries/zed/issues/10791)) was found; "follow Zed" is not
   a supportable plan.

---

## Appendix — primary sources

**This repository [local]:** `vendor/alacritty_terminal/src/{term/{mod,cell,graphics}.rs,
grid/{mod,row,storage,resize}.rs, event_loop.rs, tty/windows/conpty.rs}` ·
`vendor/vte/src/{lib,ansi,params}.rs` · `vendor/patches/` · `vendor/README.md` ·
`crates/terminal/src/{content,backend/{pump,state,osc_router},osc,key_encode,mouse_encode,
security_policy,sixel_tests}.rs` · `crates/terminal-view/src/render/` ·
`crates/tools/src/bin/pty-throughput.rs` · `docs/terminal-backend.md` ·
`docs/osc-agent-status.md` · `docs/osc-sequences-checklist.md` · `docs/license-analysis.md` ·
`docs/agents/dependencies.md` · `THIRD-PARTY-NOTICES.md` · `NOTICE` · `deny.toml` ·
`scripts/third-party-notices.py`.

**Engines:** [alacritty/alacritty](https://github.com/alacritty/alacritty) ·
[crates.io/alacritty_terminal](https://crates.io/crates/alacritty_terminal) ·
[alacritty/vte](https://github.com/alacritty/vte) ·
[wezterm/wezterm](https://github.com/wezterm/wezterm) ·
[raphamorim/rio](https://github.com/raphamorim/rio) ·
[rio-vt + librio, 2026-07-27](https://rioterm.com/blog/2026/07/27/rio-vt-and-librio) ·
[crates.io/rio-vt](https://crates.io/crates/rio-vt) ·
[raphamorim/rio-vt-benchmark](https://github.com/raphamorim/rio-vt-benchmark) ·
[ghostty-org/ghostty](https://github.com/ghostty-org/ghostty) ·
[Ghostty 1.3.0 release notes](https://ghostty.org/docs/install/release-notes/1-3-0) ·
[ghostty PR #13264 (scrollback compression)](https://github.com/ghostty-org/ghostty/pull/13264) ·
[ghostty discussion #4837 (performance)](https://github.com/ghostty-org/ghostty/discussions/4837) ·
[mitchellh: Ghostty devlog 006](https://mitchellh.com/writing/ghostty-devlog-006) ·
[mitchellh: Libghostty Is Coming](https://mitchellh.com/writing/libghostty-is-coming) ·
[uzaaft/libghostty-rs](https://github.com/uzaaft/libghostty-rs) ·
[Paneflow: libghostty-vt on Windows](https://paneflow.dev/blog/libghostty-windows) ·
[contour-terminal/contour](https://github.com/contour-terminal/contour) ·
[Contour internals](https://contour-terminal.org/internals/) ·
[kovidgoyal/kitty](https://github.com/kovidgoyal/kitty) ·
[kitty performance.rst](https://github.com/kovidgoyal/kitty/blob/master/docs/performance.rst) ·
[dnkl/foot](https://codeberg.org/dnkl/foot) ·
[foot doc/benchmark.md](https://codeberg.org/dnkl/foot/src/branch/master/doc/benchmark.md) ·
[neovim/libvterm](https://github.com/neovim/libvterm) ·
[xtermjs/xterm.js](https://github.com/xtermjs/xterm.js) ·
[xterm.js PR #1796](https://github.com/xtermjs/xterm.js/pull/1796) ·
[microsoft/terminal](https://github.com/microsoft/terminal) ·
[In-process ConPTY spec](https://github.com/microsoft/terminal/blob/main/doc/specs/%2313000%20-%20In-process%20ConPTY.md) ·
[Improved keyboard handling in ConPTY](https://github.com/microsoft/terminal/blob/main/doc/specs/%234999%20-%20Improved%20keyboard%20handling%20in%20Conpty.md) ·
[WT Preview 1.22](https://devblogs.microsoft.com/commandline/windows-terminal-preview-1-22-release/) ·
[Creating a Pseudoconsole session](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session).

**Crates surveyed:** [vt100](https://crates.io/crates/vt100) ·
[avt](https://crates.io/crates/avt) · [vtparse](https://crates.io/crates/vtparse) ·
[anstyle-parse](https://crates.io/crates/anstyle-parse) ·
[termwiz](https://crates.io/crates/termwiz) · [termina](https://crates.io/crates/termina) ·
[terminput](https://crates.io/crates/terminput) · [tui-term](https://crates.io/crates/tui-term) ·
[portable-pty](https://crates.io/crates/portable-pty) ·
[vterm-sys](https://crates.io/crates/vterm-sys) ·
[zellij-server](https://crates.io/crates/zellij-server) ·
[libghostty-vt](https://crates.io/crates/libghostty-vt) ·
[warpdotdev/Warp](https://github.com/warpdotdev/Warp).

**Specs:** [Williams DEC ANSI parser](https://vt100.net/emu/dec_ansi_parser) ·
[XTerm Control Sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) ·
[xterm changelog](https://invisible-island.net/xterm/xterm.log.html) ·
[ECMA-48 5th ed.](https://ecma-international.org/wp-content/uploads/ECMA-48_5th_edition_june_1991.pdf) ·
[VT330/340 Programmer Reference](https://vt100.net/docs/vt3xx-gp/) ·
[DEC STD 070](https://archive.org/details/bitsavers_decstandar0VideoSystemsReferenceManualDec91_74264381) ·
[kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) ·
[kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) ·
[kitty desktop notifications (OSC 99)](https://sw.kovidgoyal.net/kitty/desktop-notifications/) ·
[iTerm2 inline images](https://iterm2.com/documentation-images.html) ·
[OSC 8 hyperlinks](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) ·
[OSC 133 semantic prompts](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md) ·
[ConEmu ANSI escape codes](https://conemu.github.io/en/AnsiEscapeCodes.html) ·
[synchronized output (contour)](https://github.com/contour-terminal/contour/blob/master/docs/vt-extensions/synchronized-output.md) ·
[synchronized updates (iTerm2)](https://gitlab.com/gnachman/iterm2/-/wikis/synchronized-updates-spec) ·
[terminal-unicode-core (mode 2027)](https://github.com/contour-terminal/terminal-unicode-core) ·
[in-band resize (mode 2048)](https://gist.github.com/rockorager/e695fb2924d36b2bcf1fff4a3704bd83) ·
[colour-scheme notifications (mode 2031)](https://raw.githubusercontent.com/contour-terminal/contour/master/docs/vt-extensions/color-palette-update-notifications.md) ·
[bash/dec-modes](https://github.com/bash/dec-modes) ·
[uapi-group OSC 3008](https://github.com/uapi-group/specifications/blob/main/specs/osc_context.md) ·
[TerminalGuide](https://terminalguide.namepad.de/) ·
[Are We Sixel Yet?](https://www.arewesixelyet.com/) ·
[alacritty-escapes(7)](https://raw.githubusercontent.com/alacritty/alacritty/master/extra/man/alacritty-escapes.7.scd).

**Benchmarks:** [alacritty/vtebench](https://github.com/alacritty/vtebench) ·
[contour-terminal/termbench-pro](https://github.com/contour-terminal/termbench-pro).
