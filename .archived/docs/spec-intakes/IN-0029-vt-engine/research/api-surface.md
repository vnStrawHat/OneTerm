# IN-0029 — VT engine rewrite: the API surface OneTerm consumes today

Research note. Inventories every `alacritty_terminal::*` / `vte::*` item OneTerm's own
crates touch, the semantics each call site relies on, the OneTerm-owned deltas already
carried in `vendor/patches/`, the test helpers that must keep working, and the engine
behaviour the architecture docs promise.

**Status**: research only — no design decision, no packet. Input for the High-Level
Design of a from-scratch engine.

**Scope note from the owner**: the replacement does *not* have to be a drop-in for
alacritty's *API*. Call sites may change. The final section therefore states each need
as a **capability + required semantics**, not as a type name, and flags the alacritty
idioms that are awkward for OneTerm today. The concrete file:line inventory below stays,
because it is the ground truth of what behaviour must survive.

---

## 1. How the dependency is wired

| Fact | Where |
|---|---|
| Single workspace dep `alacritty_terminal` (git rev `fcf32fe…`, the Zed fork) | `Cargo.toml:76` |
| Redirected to the vendored tree by `[patch]` | `Cargo.toml:240-241` |
| `vte` redirected too — **but no OneTerm crate depends on `vte` directly** | `Cargo.toml:243-244` |
| Every `vte` item reaches OneTerm through the re-export `pub use vte;` | `vendor/alacritty_terminal/src/lib.rs:20` |
| Consumers | `crates/terminal/Cargo.toml:19`, `crates/local-shell/Cargo.toml:20`, `crates/ssh/Cargo.toml:20`, `crates/terminal-view/Cargo.toml:32`, `crates/tools/Cargo.toml:35` |
| `opt-level = 3` override in both dev profiles | `Cargo.toml:172`, `Cargo.toml:200` |

Consequence for the rewrite: **`vte` is not a separate public dependency of OneTerm**.
A new engine may fold parser and terminal into one crate and re-export whatever
low-level types it wants; nothing outside the engine names `vte` as a crate.

---

## 2. Inventory by crate

193 textual references in 40 files (`crates/`, `*.rs` + `*.toml`). Grouped by the crate
that owns the call site.

### 2.1 `crates/terminal` — the engine adapter (the bulk of the surface)

| File | Lines | Imported items |
|---|---|---|
| `src/content.rs` | 17-23 | `event::EventListener`, `grid::Dimensions`, `index::{Column, Line, Point}`, `selection::SelectionRange`, `term::cell::{Cell, Flags}`, `term::graphics::GraphicData`, `term::{RenderableCursor, Term, TermDamage, TermMode}`; also `grid::GridIterator` (:237), `term::RenderableContent` (:245), `vte::ansi::CursorShape` (:136, :229), `term::test::mock_term` (:267) |
| `src/model.rs` | 13-19 | `grid::{Dimensions, Grid, Scroll}`, `index::{Column, Line, Point, Side}`, `selection::{Selection, SelectionType}`, `sync::FairMutex`, `term::TermMode`, `term::cell::Cell`, `{Term, event::EventListener}`; tests 545-549 add `event::VoidListener`, `term::Config`, `term::cell::Flags`, `term::test::mock_term`, `vte::ansi::{Processor, StdSyncHandler}` |
| `src/search.rs` | 19-23 | `event::EventListener`, `grid::Dimensions`, `index::{Column, Line}`, `term::Term`, `term::cell::{Cell, Flags}`; tests :224 `term::test::mock_term` |
| `src/session.rs` | 15-17, 112, 512-515, 607 | `selection::SelectionType`, `term::TermMode`, `vte::ansi::{Rgb, CursorShape}` |
| `src/lib.rs` | 28-30 | **re-exports** `term::graphics::{GraphicCell, GraphicData, GraphicId, VIRTUAL_CELL as SIXEL_VIRTUAL_CELL}` |
| `src/palette.rs` | 6 | `vte::ansi::{Color, NamedColor, Rgb}` |
| `src/color_classification.rs` | 5, 52 | `vte::ansi::{Color, NamedColor, Rgb}` |
| `src/osc_color.rs` | 20 | `vte::ansi::Rgb` |
| `src/mouse_encode.rs` | 7 | `term::TermMode` |
| `src/logging.rs` | 8 | `vte::{Parser, Perform}` |
| `src/backend/pump.rs` | 23-26 | `grid::Dimensions`, `sync::FairMutex`, `term::Term`, `vte::ansi::{Processor, StdSyncHandler}` |
| `src/backend/osc_router.rs` | 15 | `event::{Event, EventListener}` |
| `src/backend/line_accounting.rs` | 16 | `grid::Dimensions` |
| `src/backend/state.rs` | 13 | `vte::ansi::Rgb` |
| `src/test_support.rs` | 7-12 | `index::{Column, Line, Point}`, `selection::SelectionType`, `term::cell::Cell`, `term::graphics::{GraphicCell, GraphicData}`, `term::{RenderableCursor, TermMode}`, `vte::ansi::{CursorShape, Rgb}` |
| `src/sixel_tests.rs` | 7-13 | `event::{Event, EventListener, VoidListener}`, `grid::Dimensions`, `index::{Column, Line, Point}`, `term::graphics::{GraphicCell, GraphicData, MAX_DIMENSION}`, `term::test::TermSize`, `term::{Config, Term}`, `vte::ansi::{Processor, StdSyncHandler}` |
| `src/backend/backend_tests.rs` | 8-11, 426 | `event::{Event, EventListener}`, `sync::FairMutex`, `term::{ClipboardType, Config, Term}`, `vte::ansi::Rgb`, `grid::Dimensions` |

### 2.2 `crates/local-shell` — PTY ownership

| File | Lines | Items |
|---|---|---|
| `src/session.rs` | 11-14 | `event::WindowSize`, `sync::FairMutex`, `term::{Config, Term}`, `tty::{Options, Shell}` |
| `src/event_loop.rs` | 22-25, 64, 169-179 | `event::{OnResize, WindowSize}`, `sync::FairMutex`, `term::Term`, `tty::{self, EventedPty, Options}`, `tty::Pty`, `tty::ChildEvent`; `PTY_CHILD_EVENT_TOKEN` **re-declared locally as `1`** because upstream's is `pub(crate)` on Unix |
| `src/transport.rs` | 11, 64-69 | `event::WindowSize` |
| `src/event_loop_tests.rs` | 9-11, 348-350 | `grid::Dimensions`, `term::Config`, `tty::{ChildEvent, EventedReadWrite}`, `index::{Point, Line, Column}` |
| `src/session_tests.rs` | 5 | `selection::SelectionType` |

### 2.3 `crates/ssh` — grid only, no PTY

| File | Lines | Items |
|---|---|---|
| `src/session.rs` | 25-26, 56, 244-252 | `sync::FairMutex`, `term::{Config, Term}` |
| `src/task.rs` | 8-9, 40 | `sync::FairMutex`, `term::Term` |

SSH never touches `tty`. Confirms the engine's PTY half must be separable
(`docs/terminal-backend.md:127`).

### 2.4 `crates/terminal-view` — rendering + input

| File | Lines | Items |
|---|---|---|
| `src/render/frame.rs` | 11-16, 588-592 | `selection::SelectionRange`, `term::TermMode`, `term::cell::{Flags, Cell, Hyperlink}`, `term::RenderableCursor`, `index::{Column, Line, Point}`, `vte::ansi::{Color, CursorShape, NamedColor, Rgb}` |
| `src/theme/palette.rs` | 3 | `vte::ansi::Rgb` |
| `src/input/mouse.rs` | 16 | `selection::SelectionType` |
| `src/input/mouse_tests.rs` | 218, 242 | `term::TermMode` |

`frame.rs:3-7` states the design intent explicitly: **it is the only file under
`render/` that names an `alacritty_terminal` type**; everything above it consumes
view-owned `Cell` / `Color` / `CellFlags` / `CursorShape` / `Selection` / `Damage`.
That boundary is already the shape of the future engine contract.

### 2.5 `crates/tools`

`src/bin/pty-throughput.rs:21-22` — `event::WindowSize`, `tty::{self, EventedReadWrite, Options, Shell}`. Diagnostic binary, no `Term`.

### 2.6 Mentions only (no code dependency)

`crates/core/src/lib.rs:3` (doc: leaf crate does **not** depend on the engine),
`crates/app/build.rs:10` (doc: the engine loads `conpty.dll` itself via `LoadLibraryW`).

---

## 3. API surface by concern

### 3.1 Parsing

| Item | Representative use | Semantics OneTerm relies on |
|---|---|---|
| `vte::ansi::Processor<StdSyncHandler>` | `crates/terminal/src/backend/pump.rs:57`, constructed `:67` | One parser instance per session, owned by the pump; state persists across chunk boundaries (a sequence may split across PTY reads). `StdSyncHandler` = the synchronized-update (DCS 2026) timeout source. |
| `Processor::advance(&mut Term, &[u8])` | `pump.rs:89`; tests `model.rs:571`, `sixel_tests.rs:20` | Push parser: caller owns the bytes, the `Term` is the `Handler`. All listener events fire **synchronously inside this call, with the `Term` lock held** (`docs/terminal-backend.md:248-254`). |
| `vte::Parser` + `vte::Perform` | `crates/terminal/src/logging.rs:8`, `:57`, `:212`, `:218`; `impl Perform for LineCollector` `:67-83` | A **second, independent** parser used only to strip escape sequences from the session log. Uses `print`, `execute` only. Must stay available (or be replaced by a small OneTerm-owned stripper). |
| `vte::Params` | (indirect, via the DCS hook the fork added) | Numeric parameter list for CSI/DCS. |
| `vte::ansi::Handler::report_osc` | fork addition, consumed via `Event::Osc` | See §5. |
| `vte::ansi::Handler::dcs_hook / dcs_put / dcs_unhook` | fork addition, consumed by the Sixel decoder | See §5. |

### 3.2 Grid / screen model

| Item | Representative use | Semantics |
|---|---|---|
| `Term::<EP>::new(Config, &D: Dimensions, EP)` | `local-shell/src/session.rs:97-101`, `ssh/src/session.rs:248-252`, `sixel_tests.rs:16` | Term is generic over the listener; size comes from any `Dimensions`. |
| `Term::renderable_content() -> RenderableContent<'_>` | `content.rs:196`, `model.rs:111`, `model.rs:137` | `&self` only. Fields destructured at `content.rs:245-253`: `display_iter`, `cursor`, `mode`, `display_offset`, `selection`, `colors` (`colors` is *discarded*). |
| `RenderableContent::display_iter: GridIterator<'a, Cell>` | `content.rs:206-209`, `model.rs:140-148` | Yields `Indexed<Cell> { point, cell }` in **display order, top-left to bottom-right, display_offset already applied**, dense (`rows × cols` items). `model.rs:138-148` relies on `skip(start_line*cols).take(count*cols)` addressing a line range — i.e. **strict row-major density**. `frame.rs:518` has a fallback for the non-dense case (resize race) that binary-searches by `point.line`. |
| `Term::grid() -> &Grid<Cell>` | `content.rs:49`, `model.rs:176`, `search.rs:83`, `sixel_tests.rs:39`, 47 call sites total | Direct grid access for line-range reads and tests. |
| `Term::grid_mut() -> &mut Grid<Cell>` | `model.rs:485`, `:493`, `:514` | Needed only by the ConPTY resize correction. |
| `Index<Line> for Grid` → `&Row<Cell>`, `Index<Column> for Row` → `&Cell` | `content.rs:51-52`, `search.rs:90-92`, `model.rs:535`, `model.rs:594`, `model.rs:605` | **`Line` may be negative** — `Line(-1)` is the newest history row. `model.rs:628` asserts `row_text(&term, -1) == "line15"`. |
| `Index<Point> for Grid` → `&Cell` | `local-shell/src/event_loop_tests.rs:352` | Point-addressed read. |
| `Grid<Cell>: Clone` rows | `model.rs:535` (`probe[Line(..)] = grid[Line(line)].clone()`) | A `Row` must be cloneable into another grid. |
| `Grid::new(lines, columns, max_scroll_limit)` | `model.rs:483`, `model.rs:533` | Construct a standalone/scratch grid, optionally history-less (`0`). |
| `Grid::resize(reflow: bool, lines, columns)` | `model.rs:502-503`, `model.rs:512`, `model.rs:539` | Reflow on/off switch. `reflow=false` is used for the parked alt grid (no wrap join/split). |
| `Grid::scroll_up(&Range<Line>, positions)` | `model.rs:497` | Rotate the region up; exposed bottom rows reset; rows leaving the top go to history (growing the scroll limit when below the cap). |
| `Grid::display_offset() -> usize` | `model.rs:180`, `:394`, `:489`, `:506`; `content.rs` via `RenderableContent` | **Counts lines scrolled into history**; 0 = at the bottom. Display row = `grid line + display_offset` (`search.rs:59-61`, `frame.rs:525`, `frame.rs:541`). |
| `Grid::scroll_display(Scroll)` / `Term::scroll_display(Scroll)` | `model.rs:227`, `:234`, `:244`, `:398`, `:418`, `:507`, `:679` | `Scroll::Delta(i32)` and `Scroll::Bottom` are the only variants used. `PageUp`/`PageDown`/`Top` are unused. |
| `Grid::cursor: Cursor<Cell>` (`point`, `input_needs_wrap`) | `model.rs:176`, `:494`, `:498`, `:532`, `:537-538`; `sixel_tests.rs:107`, `:147` | Public, mutable. `input_needs_wrap` is copied into the scratch probe so reflow splits the same way. |
| `Grid::saved_cursor` | `model.rs:499` | DECSC cursor moves with the viewport correction, clamped to `>= 0`. |
| `Grid::history_size()` (via `Dimensions`) | `model.rs:501`, `:540`; `model.rs:629` etc. — 19 call sites | `total_lines - screen_lines`. |
| `Dimensions` trait (`total_lines`, `screen_lines`, `columns`, + provided `last_column`, `topmost_line`, `bottommost_line`, `history_size`) | implemented **by OneTerm** at `model.rs:38-48` (`TerminalSize`), `backend/pump.rs:42-52` (`GridSize`), `backend_tests.rs:426-436` (`Dims`); consumed at `content.rs:47-48`, `model.rs:118-120`, `search.rs:84-86`, `line_accounting.rs:32-43` | Generic size trait; `LineAccounting::observe<D: Dimensions>` is generic so it can be unit-tested without a `Term`. |
| `Term::resize<S: Dimensions>(S)` | `model.rs:219`, `:491` | **Anchors the bottom row**: grow pulls `min(history, added)` rows from scrollback into the top and moves the cursor down; column grow joins `WRAPLINE` rows; column shrink splits and pushes top rows into history. Leaves the whole terminal damaged and **rotates the selection**. Pinned by 10 tests in `model.rs:616-910`. |
| `Term::swap_alt()` | `model.rs:486`, `:513` | Swaps primary/alt grids, restoring keyboard-mode stacks and mode flags. There is **no `inactive_grid()` accessor**, which is why `model.rs:481-516` parks the alt grid in a local and swaps twice. |
| `Term::total_lines()` | `content.rs:214`, `model.rs:120`, `:172`, `:243`; `sixel_tests.rs:188` | Scrollback + viewport; stops growing at the scrollback cap (which is why `LineAccounting` exists). |
| `Term::damage() -> TermDamage<'_>` / `Term::reset_damage()` | `content.rs:186-193` | `TermDamage::Full` or `Partial(TermDamageIterator)` yielding `LineDamageBounds { line, .. }` where **`line` is already the display row** (offset applied). OneTerm reads only `.line` and filters `< screen_lines`. `damage()` needs `&mut self`, which is why `TerminalContent::refill` takes `&mut Term` (`content.rs:171-172`). |
| `Term::exit()` | `local-shell/src/event_loop.rs:372` | Marks the terminal dead on child exit. |
| `Term::mode() -> &TermMode` | `model.rs:187`, `:226`, `:393`, `:482` | 19 call sites. |
| `Cell` fields `c`, `fg`, `bg`, `flags`, `extra: Option<Arc<CellExtra>>` | `content.rs:31-36`, `frame.rs:300-318`, `frame.rs:365-384`, `search.rs:92-100` | `c` is a single `char`; combining marks live in `zerowidth()`. |
| `Cell::default()` | `test_support.rs:305`, `frame.rs:612` | Space, `Named(Background)` bg, `Named(Foreground)` fg, empty flags. |
| `Cell::zerowidth() -> Option<&[char]>` / `push_zerowidth(char)` | `frame.rs:305`, `:371`; `frame.rs:680` | Combining marks / ZWJ tail attached to the cell's base char. |
| `Cell::hyperlink() -> Option<Hyperlink>` / `set_hyperlink(Option<Hyperlink>)` | `content.rs:33`, `frame.rs:306-312`, `frame.rs:324`, `frame.rs:379-383`, `frame.rs:687`, `frame.rs:702` | `Hyperlink::id() -> &str`, `uri() -> &str`, `Hyperlink::new(Option<T: ToString>, String)`, `Clone`. OneTerm hashes `id + \0 + uri` to an `u64` identity (`frame.rs:306-312`). |
| `Cell::graphic() -> Option<GraphicCell>` / `set_graphic(Option<GraphicCell>)` | `frame.rs:313-317`, `sixel_tests.rs:39`, `test_support.rs:312` | Fork addition. See §3.7. |
| `Cell::underline_color()` / `set_underline_color()` | **never used** | SGR 58/59 colored underline is parsed by the engine but not rendered. |
| `Flags` bits actually read | `INVERSE, BOLD, ITALIC, DIM, HIDDEN, UNDERLINE, DOUBLE_UNDERLINE, UNDERCURL, DOTTED_UNDERLINE+DASHED_UNDERLINE (only via `ALL_UNDERLINES`), STRIKEOUT, WIDE_CHAR, WIDE_CHAR_SPACER, LEADING_WIDE_CHAR_SPACER, WRAPLINE` | `frame.rs:208-233` maps them to the view's own `CellFlags`; `ALL_UNDERLINES` collapses every underline kind into one bit plus a separate `UNDERCURL` bit. `content.rs:34-36` and `search.rs:96` use `WIDE_CHAR_SPACER`; `model.rs:596` uses `WRAPLINE`. |
| `Flags::bits() -> u16` | `frame.rs:370` | Row hashing depends on a stable numeric representation. |
| `Flags::BOLD_ITALIC`, `DIM_BOLD` (composites) | **never used** | |
| `GridIterator::point()` | **never used** | |
| `Indexed<Cell>` (`point`, `cell`) | `content.rs:206-209`, `model.rs:143-147` | Copied into OneTerm's own `IndexedCell` (`content.rs:61-64`). |

### 3.3 Cursor

| Item | Use | Semantics |
|---|---|---|
| `RenderableCursor { shape: CursorShape, point: Point }` | `content.rs:95`, `:135-138`, `:211`; `test_support.rs:338-341`; `frame.rs:534-545`, `:617-620`, `:713-723` | `point.line` is a **grid** line (may be negative); display row = `line + display_offset` (`frame.rs:537-541`). |
| `vte::ansi::CursorShape` variants `Block, Beam, Underline, HollowBlock, Hidden` | `frame.rs:431-439`, `content.rs:136`, `:229`; `session.rs:112`; `test_support.rs:339` | `Hidden` is how "cursor invisible" is expressed — there is no separate visibility flag (`content.rs:225-231`). All five variants are matched exhaustively. |
| `Grid::cursor.point` (raw) | `model.rs:176`, `:494`, `sixel_tests.rs:147-151` | Used for `TerminalInfo::cursor_line` and for the resize correction. |
| `CursorStyle`, `Term::cursor_style()` | **never used** | OneTerm reads the shape off `RenderableCursor` only. |

### 3.4 Selection

| Item | Use | Semantics |
|---|---|---|
| `Term::selection: Option<Selection>` (public field) | `model.rs:254`, `:261`, `:274-277`, `:282`, `:292`, `:508` | Selection state lives **inside** `Term`, mutated directly. |
| `Selection::new(SelectionType, Point, Side)` | `model.rs:254`, `:290` | Anchor + side. |
| `Selection::update(Point, Side)` | `model.rs:262`, `:291` | Drag end point. |
| `Selection::to_range(&Term) -> Option<SelectionRange>` | `model.rs:276` | Used by `has_selection()` to avoid materialising the text (PERF-14). |
| `Term::selection_to_string() -> Option<String>` | `model.rs:268` | Copy path. |
| `SelectionRange { start: Point, end: Point, is_block: bool }` | `content.rs:103`, `frame.rs:549-559`, `frame.rs:728-733` | Both ends **inclusive**; rows converted to display rows with `+ display_offset`. |
| `SelectionType::{Simple, Semantic, Lines, Block}` | `terminal-view/src/input/mouse.rs:300-308` (click count → type), `model.rs:290`, `session.rs:347`, `test_support.rs:46` | All four variants used. Semantic word boundaries come from `Config::semantic_escape_chars`. |
| `Side::{Left, Right}` | `model.rs:290-291`, `:431-435` | Picked from the fractional part of the mouse column. |
| Selection rotation on resize | `model.rs:508` drops the selection after a corrected resize; `docs/terminal-backend.md:237-238` | The engine rotates selections on resize; OneTerm clears rather than trusting it after its own correction. |

### 3.5 Modes & events

| Item | Use | Semantics |
|---|---|---|
| `TermMode` bits used | `ALT_SCREEN` (13×), `MOUSE_MODE` (7×, composite of `MOUSE_REPORT_CLICK|MOUSE_MOTION|MOUSE_DRAG`), `UTF8_MOUSE` (5×), `MOUSE_REPORT_CLICK` (4×), `SHOW_CURSOR` (3×), `BRACKETED_PASTE` (3×), `APP_CURSOR` (3×), `SGR_MOUSE` (2×), `MOUSE_MOTION`, `MOUSE_DRAG` | `model.rs:192`, `:197`, `:226`, `:325`, `:339`, `:397`, `:404-405`; `mouse_encode.rs:83`, `:97`, `:191`, `:195`; `session.rs:425`; `frame.rs:564`; `test_support.rs:236`, `:419`, `:427` |
| `TermMode::empty()` / `contains` / `intersects` / `set` | `content.rs:139`, `test_support.rs:99-105` | bitflags API. |
| **Unused mode bits** | `APP_KEYPAD, LINE_WRAP, LINE_FEED_NEW_LINE, ORIGIN, INSERT, FOCUS_IN_OUT, ALTERNATE_SCROLL, VI, URGENCY_HINTS, KITTY_KEYBOARD_PROTOCOL` and its four sub-bits | They are still *implemented* (they change behaviour inside the engine) — OneTerm just never queries them. Kitty keyboard is off by default (`Config::kitty_keyboard = false`). |
| `EventListener::send_event(&self, Event)` | `osc_router.rs:214-268` (the single production impl), `sixel_tests.rs:254-260` (test recorder) | **`&self`, not `&mut self`** — the router is cloned into the `Term` and into the pump. Must never block: it runs under the `Term` lock. |
| `VoidListener` | `model.rs:545`, `sixel_tests.rs:7` | Null sink for tests. |
| `Event` variants — **all 15 matched exhaustively** at `osc_router.rs:216-267` | | |
| `Event::Wakeup` | `osc_router.rs:218` | → `SessionEvent::Output` repaint hint. |
| `Event::Title(String)` / `ResetTitle` | `:220-221` | OSC 0/2. |
| `Event::ClipboardStore(ClipboardType, String)` | `:223` | OSC 52 write; text already base64-decoded by the engine. |
| `Event::ClipboardLoad(ClipboardType, Arc<dyn Fn(&str)->String>)` | `:224-230` | OSC 52 read request. **The formatter closure is dropped** — OneTerm builds its own reply (`encode_osc52`). |
| `Event::PtyWrite(String)` | `:232-236` | Engine-originated reply (DA, DSR, …) → transport. |
| `Event::ChildExit(ExitStatus)` | `:239-243` | Only alacritty's own `EventLoop` emits it; OneTerm pumps publish exit themselves. Tested at `backend_tests.rs:166`. |
| `Event::Exit` | `:245` | Ignored. |
| `Event::Bell` | `:247` | |
| `Event::Osc { params: Vec<Vec<u8>>, bell_terminated: bool }` | `:249-257`; tests `backend_tests.rs:198-201`, `:213-220`, `:236-239` | **Fork addition.** Raw, unparsed OSC params including `params[0]` (the OSC number). |
| `Event::ClearScreen` | `:259`; test `backend_tests.rs:191` | **Fork addition.** `CSI 2J` / `CSI 3J` / RIS → bump the clear epoch (gutter timestamps). |
| `Event::ColorRequest(usize, Arc<dyn Fn(Rgb)->String>)` | `:262`; answered in `pump.rs:105-128` | Deferred: queued during the parse batch, answered after the batch when the `Term` colours can be read. The index space is `0..=255` palette, `256` fg, `257` bg, `258` cursor (`osc_color.rs:23-27`). |
| `Event::MouseCursorDirty`, `CursorBlinkingChange`, `TextAreaSizeRequest(_)` | `:264-266` | Ignored. |
| `ClipboardType` | `backend_tests.rs:10`, `:126` | Only `Clipboard` used in tests; the variant is not inspected in production. |

### 3.6 Colors

| Item | Use | Semantics |
|---|---|---|
| `vte::ansi::Rgb { r, g, b }` (u8) | `osc_color.rs:20`, `palette.rs:6`, `backend/state.rs:13`, `session.rs:512-515`, `test_support.rs:12`, `terminal-view/src/theme/palette.rs:3` | The one colour type crossing the engine↔UI boundary. `terminal-view/src/theme/palette.rs:93-114` converts `Rgb ↔ gpui::Rgba/Hsla`. |
| `vte::ansi::Color::{Named(NamedColor), Indexed(u8), Spec(Rgb)}` | `palette.rs:96-102`, `color_classification.rs:35-47`, `frame.rs:100-140`, `frame.rs:569-575` | Matched exhaustively both ways (`Color::from_vte` / `to_vte`), round-trip-tested at `frame.rs:765-787`. |
| `vte::ansi::NamedColor` — **all 21 variants** | `palette.rs:104-141` (exhaustive match), `frame.rs:104-122`, `frame.rs:143-171` | Relies on the **discriminant layout**: `Black = 0 … BrightWhite = 15`, `Foreground`, `Background`, `Cursor`, `DimBlack … DimWhite` contiguous, `BrightForeground`, `DimForeground`. `palette.rs:136` does `nc as usize - NamedColor::DimBlack as usize`; `frame.rs:118` and `frame.rs:121` do the same. **A rewrite must preserve these numeric relationships or change both sites.** |
| `Term::colors() -> &Colors` with `Index<usize> -> Option<Rgb>` | `model.rs:155-164`, `pump.rs:116` | 259-slot table of *OSC-set overrides*; `None` = not overridden, fall back to the theme. Indices per `osc_color.rs:23-27`. |
| `RenderableContent::colors` | destructured and **discarded** at `content.rs:252` | The render snapshot does not carry colours; the UI reads them through `dynamic_colors()`. |

### 3.7 Graphics (Sixel — OneTerm fork)

| Item | Use | Semantics |
|---|---|---|
| `term::graphics::GraphicId(pub u64)` | re-exported `terminal/src/lib.rs:28-30`; `frame.rs:314` reads `g.id.0` | Per-`Term` id, starts at 1, never reset by RIS. |
| `term::graphics::GraphicCell { id, col: u16, row: u16 }` | `frame.rs:313-317`, `sixel_tests.rs:127-131`, `test_support.rs:171` | `Copy`. `(col,row)` is the cell's offset **inside the image's cell grid**, not a screen coordinate. |
| `term::graphics::GraphicData { id, width, height, rgba }` | `content.rs:111`, `content.rs:217`, `frame.rs:500`, `sixel_tests.rs:33-36` | RGBA8 row-major, stride `width*4`, **straight (non-premultiplied) alpha**. |
| `term::graphics::MAX_DIMENSION = 4096` | `sixel_tests.rs:10`, `:95-96` | Hard clamp on both axes. |
| `term::graphics::VIRTUAL_CELL = (10, 20)` | re-exported as `SIXEL_VIRTUAL_CELL` (`terminal/src/lib.rs:29`) | Virtual cell the image geometry is computed in (VT340/conhost); the renderer rescales to the real font cell. |
| `Term::take_graphics() -> Vec<Arc<GraphicData>>` | `content.rs:217`, `sixel_tests.rs:28`, `:57` | **Drain, oldest first, each image handed out exactly once.** The grid keeps only `GraphicCell` references; the embedder owns cache lifetime. |
| Placement / erase semantics | pinned by `sixel_tests.rs:99-249` | Anchored at the current cursor **column** (not column 0), clipped right at `columns()`, cursor descends `bands*6/20` rows through normal `linefeed` (scroll region + scrollback apply) keeping its column; overwriting or erasing a cell drops the reference; RIS drops pending images, `CSI 2J` does not. |
| DA1 answer `\x1b[?62;4c` | `sixel_tests.rs:262-271` | Feature detection for `lsix`/`chafa`/`timg`/tmux. |

### 3.8 PTY (local shell only)

| Item | Use | Semantics |
|---|---|---|
| `tty::new(&Options, WindowSize, u64) -> io::Result<Pty>` | `local-shell/src/event_loop.rs:195`, `tools/src/bin/pty-throughput.rs:49` | Spawns the child; ConPTY on Win10 1809+ selected automatically. |
| `tty::Options { shell, working_directory, drain_on_exit, env, child_signal_mask (unix), escape_args (windows) }` | `local-shell/src/session.rs:53-70`, `pty-throughput.rs:35-41` | All six fields set explicitly, including the two cfg-gated ones. |
| `tty::Shell::new(String, Vec<String>)` | `local-shell/src/session.rs:54-57` | Program + args. |
| `event::WindowSize { num_lines, num_cols, cell_width, cell_height }` | `local-shell/src/session.rs:71-76`, `transport.rs:64-69`, `event_loop.rs:22`, `pty-throughput.rs:43-48` | OneTerm always passes `cell_width = cell_height = 0`; only rows/cols matter. |
| `tty::EventedReadWrite` (`register` unsafe / `reregister` / `deregister` / `reader` / `writer`) | consumed `event_loop.rs:276`, `:340`, `:390`, `:534`; **implemented by OneTerm** for a loopback test PTY at `event_loop_tests.rs:270-318` | Built on the `polling` crate's `Poller`, `Event`, `PollMode`. |
| `tty::EventedPty::next_child_event() -> Option<ChildEvent>` | `event_loop.rs:371`; implemented at `event_loop_tests.rs:320-326` | |
| `tty::ChildEvent::Exited(Option<ExitStatus>)` | `event_loop.rs:371`, `event_loop_tests.rs:241` | `None` = the platform watcher could not read a code; the session still ends. |
| `event::OnResize::on_resize(WindowSize)` | `event_loop.rs:329`; implemented at `event_loop_tests.rs:328-332` | |
| `tty::Pty::child()` (unix) → `.id()`; `Pty::child_watcher().pid()` (windows) | `event_loop.rs:169-179` | Child pid, used for the log filename identity. Platform-asymmetric API. |
| `PTY_CHILD_EVENT_TOKEN` | **not importable** — re-declared as `const … = 1` at `event_loop.rs:64` | `pub(crate)` on Unix, `pub` on Windows. A rewrite should just export it. |
| `tty::setup_env()` | **never called** | OneTerm sets `TERM`/`COLORTERM` itself via `Options::env` (`crates/core/src/config/shell.rs`). |

### 3.9 Sync

| Item | Use | Semantics |
|---|---|---|
| `sync::FairMutex<Term<EP>>` behind `Arc` | `model.rs:75`, `local-shell/src/session.rs:97`, `ssh/src/session.rs:56`, `pump.rs:141`, `event_loop.rs:161` | Fairness exists so the GPUI main thread does not starve behind a busy pump (`docs/terminal-backend.md:137-138`). |
| `lock()` | everywhere in `model.rs` | Fair acquire. |
| `try_lock_unfair()` / `lock_unfair()` | `event_loop.rs:410-415`, `:443` | The pump grabs the grid unfairly while draining a read, escalating to a blocking unfair lock only once the 1 MiB buffer is full. |

### 3.10 Test support

| Item | Use |
|---|---|
| `term::test::mock_term(&str) -> Term<VoidListener>` | `content.rs:267, 271, 281, 291, 301, 310`; `model.rs:548, 555`; `search.rs:224` + 9 tests |
| `term::test::TermSize::new(cols, lines)` | `sixel_tests.rs:11, 16, 265` |
| `event::VoidListener` | `model.rs:545`, `sixel_tests.rs:7` |

`pub mod test` is **not** `#[cfg(test)]`-gated in the vendored crate
(`vendor/alacritty_terminal/src/term/mod.rs:2508`), so OneTerm's `#[cfg(test)]` modules
can use it without a feature flag.

---

## 4. What OneTerm already owns (and must NOT be re-added to the engine)

| OneTerm implements | Where | Engine feature it replaces / never uses |
|---|---|---|
| Its own PTY event loop | `crates/local-shell/src/event_loop.rs` (whole file; header `:1-12` says so) | `alacritty_terminal::event_loop::EventLoop`, `Notifier`, `Msg`, `thread::spawn_named`. **Zero references** in `crates/`. |
| Bounded command queue, byte budget, latest-value resize, out-of-band shutdown | `event_loop.rs:43-45`, `:101-154` | alacritty's unbounded `EventLoopSender`. |
| Search over a copied `GridText` snapshot | `crates/terminal/src/search.rs:70-148` | `alacritty_terminal::term::search` (`RegexSearch`, `Match`, `regex_search_left/right`). **Zero references.** |
| Key encoding | `crates/terminal/src/key_encode.rs` (572 lines) | The engine's keyboard-protocol reporting. |
| Mouse encoding (X10/SGR/UTF8) | `crates/terminal/src/mouse_encode.rs` (453 lines), reads only `TermMode` | — |
| OSC 7 / 9 / 9;4 / 9;7 / 133 interpretation | `crates/terminal/src/osc.rs`, `osc_agent.rs`, consumed by `osc_router.rs:146-211` | The engine only *forwards* raw params (`Event::Osc`). |
| OSC 52 base64 encode + clipboard policy | `crates/terminal/src/osc.rs:encode_osc52`, `security_policy.rs`, applied `osc_router.rs:131-142`, `:224-230` | The engine's `ClipboardLoad` formatter closure is discarded. |
| Bracketed-paste wrapping + marker stripping | `crates/terminal/src/paste.rs` (316 lines), `session.rs:425` | — |
| Absolute line counting past the scrollback cap | `crates/terminal/src/backend/line_accounting.rs` | Engine `total_lines()` saturates. |
| Session log with escape stripping | `crates/terminal/src/logging.rs` — uses a **second** `vte::Parser` | — |
| Colour resolution (`Color` → `Rgb`), dim mixing, 6×6×6 cube + greyscale ramp | `crates/terminal/src/palette.rs:51-147` | The engine's `Colors` table is only the OSC-override layer. |
| URL detection, hover, target policy | `crates/terminal-view/src/url/*`, `crates/terminal/src/url_policy.rs` | `Term::bounds_to_string`, `expand_wide`, semantic selection helpers — **unused**. |
| Damage → row-plan cache, shaping, contrast | `crates/terminal-view/src/render/*` | — |
| ConPTY-shaped resize policy | `crates/terminal/src/model.rs:440-541` | Written *on top of* the engine's resize, not instead of it. |

### 4.1 Explicit confirmations requested

| Question | Answer |
|---|---|
| Is `vi_mode` referenced? | **No.** Zero matches for `vi_mode`, `ViMode`, `toggle_vi_mode`, `vi_motion`, `vi_goto_point`, `ViMotion` in `crates/`. `TermMode::VI` is never queried. `Config::vi_mode_cursor_style` is left at `Default`. |
| Is `term/search.rs` referenced? | **No.** Zero matches for `RegexSearch` / `term::search`. OneTerm's own `search.rs` is unrelated. |
| Is `event_loop.rs` referenced? | **No**, only in prose (`local-shell/src/event_loop.rs:1`, `:12` cite it as the design reference). |
| Is `tty/unix.rs` referenced? | **Indirectly.** No path import, but `tty::new`, `tty::Pty`, `Pty::child()`, `Options::child_signal_mask` resolve to it on Unix (`local-shell/src/event_loop.rs:169-171`, `session.rs:61-62`). The Windows half is used the same way with a different shape (`child_watcher().pid()`, `escape_args`). |
| `setup_env`? | **Never called.** |
| `PTY_CHILD_EVENT_TOKEN`? | Not importable; mirrored as a local `const = 1` (`event_loop.rs:58-64`). |
| `Term::set_options`, `bounds_to_string`, `expand_wide`, `semantic_escape_chars`, `cursor_style`, `scroll_to_point`? | **Zero call sites each.** |
| `Cell::underline_color`, `CellExtra` directly, `Cell::extra`? | **Zero call sites.** `CellExtra` is only reached through `zerowidth()`/`hyperlink()`/`graphic()`. |

---

## 5. OneTerm-owned deltas in `vendor/patches/` (must be native in the new engine)

Five patches. `vendor/patches/alacritty_terminal/0001-*` is build-only (standalone
manifest, `edition = "2024"`, `rust-version = "1.85.0"`) and carries no API or runtime
delta.

### 5.1 `vte/0001` — `Handler::report_osc`

```rust
fn report_osc(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
```

- Called from the **`_` wildcard arm** of `osc_dispatch`, before the existing
  `unhandled(params)` logger. Raw byte slices, `params[0]` included, no UTF-8 guarantee,
  borrowed for the call only.
- `bell_terminated = true` ⇒ `BEL`; `false` ⇒ `ST`.
- **Fall-through set** (what reaches the hook): everything except the natively
  dispatched `0, 2, 4, 10, 11, 12, 50, 52, 104, 110, 111, 112`. Two guarded arms leak:
  OSC 8 with ≤ 2 params and OSC 22 with arity ≠ 2 **do** reach it. A malformed OSC 0/2
  (fewer than 2 params) does **not**.
- Synchronous, byte-ordered, no buffering or dedup.

### 5.2 `vte/0002` — DCS passthrough

```rust
fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
fn dcs_put(&mut self, _byte: u8) {}
fn dcs_unhook(&mut self) {}
```

- `dcs_hook` once per DCS, `action` = final byte (`'q'` = Sixel); `dcs_put` **once per
  data byte** (so the decoder must be a streaming state machine); `dcs_unhook` at `ST`
  or on abort. A new `hook` implicitly ends a previous sequence.
- Behavioural change upstream: `Performer::{hook,put,unhook}` previously only logged and
  dropped; now they forward and no longer log.

### 5.3 `alacritty_terminal/0002` — `Event::Osc` + `Event::ClearScreen`

- New `Event` variants:
  `Osc { params: Vec<Vec<u8>>, bell_terminated: bool }` and `ClearScreen`
  (`vendor/alacritty_terminal/src/event.rs:65-69`).
- `Debug` for `Osc` prints only the param **count** — deliberately, so OSC 52 clipboard
  and OSC 7 paths never leak into logs.
- `Term::report_osc` is a pure forwarder: `params.iter().map(|p| p.to_vec()).collect()`
  — **one allocation per param plus the outer Vec, per forwarded OSC**, on the hot path.
- `clear_screen`: `ClearMode::All` (`CSI 2J`) and `ClearMode::Saved` (`CSI 3J`) emit
  `ClearScreen`; `Below`/`Above` do not. Emitted *after* the clearing, *before*
  `mark_fully_damaged()`.
- `reset_state` (RIS): emits `CursorBlinkingChange` **then** `ClearScreen`, both after
  the state reset. Ordering is relied upon.

### 5.4 `alacritty_terminal/0003` — Sixel graphics

New module `term/graphics.rs` (340 lines) with the public items listed in §3.7, plus:

- `Cell::graphic()` / `set_graphic()`, and a new private `CellExtra::graphic` field; the
  "can I drop `extra`?" predicates in `set_underline_color` / `set_hyperlink` were
  extended with `&& extra.graphic.is_none()`.
- `Term::take_graphics()`; private `Term::graphics: Graphics`.
- `Term`'s `dcs_hook/put/unhook` impls: only `DCS q` is decoded; any other final byte
  **clears** an in-flight parser (so a non-Sixel DCS aborts a prior unterminated Sixel).
- **Decoder subset** (DEC STD 070): `0x3F..=0x7E` sixel byte, `!Pn` repeat, `#Pr[;Pm;…]`
  register select/define (`Pm==2` RGB %, `Pm==1` DEC HLS with hue 0 = blue, other modes
  select only), `"Pan;Pad;Ph;Pv` raster attributes (size wins over measured extents,
  aspect ratio parsed and ignored), `$` CR within band, `-` new band. DCS params
  `P1;P2;P3` are parsed and **unused** — in particular **P2 background-select is ignored
  and untouched pixels stay fully transparent**, which differs from a strict VT340.
  Default palette = 16 VT340 registers; 16..255 opaque black.
- **Placement**: anchored at the current cursor **column**; `cols = min(width.div_ceil(10),
  columns()-start)`, horizontally clipped, never wrapped; `rows = height.div_ceil(20)`;
  the cursor descends `cursor_rows = bands*6/20` rows through normal `linefeed` (scroll
  region, scrollback and damage all behave as for text) and **keeps its column**; rows
  below the final cursor row are placed without further scrolling and clipped at the
  screen bottom. Cells are stamped *before* the pixels are pushed to `pending`.
- **DA1 answer changed**: `\x1b[?6c` → `\x1b[?62;4c`.
- **RIS** clears `pending` and the in-flight parser; the id counter is **not** reset.
  `CSI 2J` does not clear pending graphics.
- **Invalidation is entirely implicit**: nothing removes a `GraphicCell`. An image dies
  when its cells are overwritten, scrolled out or reset. No refcount, no eviction, no
  "id N is unreferenced" signal — the embedder's cache must decide liveness itself.
- Alt-screen: grids swap (so cells and their `GraphicCell`s return) but `pending` /
  `parser` are untouched.
- **`vendor/README.md` §2 is stale**: it lists `Term::set_cell_size()` among the 0003
  additions. No such method exists. Scaling is the renderer's job via `VIRTUAL_CELL`;
  the terminal never learns the real cell size. Do **not** design a setter for it.

---

## 6. Test helpers the new engine must keep working (or that must be ported)

### 6.1 Engine-provided helpers OneTerm's tests depend on

| Helper | Call sites | Behaviour relied on |
|---|---|---|
| `mock_term(&str)` | `content.rs:271, 281, 291, 301, 310`; `model.rs:555`; `search.rs:228, 234, 245, 256, 271, 280, 295, 307, 316, 322, 339` | Grid sized to the content (`num_cols` = widest line by **display width**, `screen_lines` = line count — `search.rs:257-259` asserts `cols == 19`). `\n` breaks *and* sets `WRAPLINE` on the previous row; `\r\n` breaks without wrapping. Wide chars get `WIDE_CHAR` + a `WIDE_CHAR_SPACER` cell (`search.rs:336-344`). Cursor is visible by default (`content.rs:289-295`). Damage is Full until the first `reset_damage` (`content.rs:298-304`). |
| `TermSize::new(cols, lines)` | `sixel_tests.rs:16, 265` | Trivial `Dimensions`. |
| `VoidListener` | `model.rs:553`, `sixel_tests.rs:16` | Drops every event. |
| `Term::new` + `Processor::advance` used directly as a test driver | `model.rs:563-614` (`fed_term`, `term_with`, `feed`), `sixel_tests.rs:19-21` | A fresh `Processor` per `feed` call is fine because each call passes a complete sequence. |

### 6.2 OneTerm-owned test support

`crates/terminal/src/test_support.rs` (662 lines, gated `#[cfg(any(test, feature = "test-support"))]`
at `lib.rs:24-25`) is a **fake `TerminalSession`**, not a fake `Term` — it fabricates
`TerminalContent` directly. Its engine dependency is narrow:

| Line | Engine item | Why |
|---|---|---|
| `:236` | `TermMode::SHOW_CURSOR` | Default mode of the fake. |
| `:99-105` | `TermMode::ALT_SCREEN` via `set` | `set_alt_screen(bool)`. |
| `:305-307` | `Cell::default()`, `cell.c = …` | Builds cells from a text blob. |
| `:312` | `cell.set_graphic(Some(GraphicCell))` | `set_graphic` probe hook. |
| `:187-189` | `Arc<GraphicData>` | `push_graphic`, hand-out-once emulation. |
| `:314-317` | `Point::new(Line(row), Column(col))` | Dense display-order cells. |
| `:338-341` | `RenderableCursor { shape: CursorShape::Block, point }` | |
| `:434-441` | `Rgb` | `set_default_colors` signature. |
| `:46`, `:502` | `SelectionType` | `FakeInputCall::MouseDown`. |

`crates/terminal-view/src/render/frame.rs:586-757` (`FrameBuilder`) is the second test
fixture. It builds a `TerminalContent` out of real `Cell`s and needs:
`Cell::default`, `cell.c/fg/bg/flags` assignment, `push_zerowidth`, `set_hyperlink`,
`Hyperlink::new(None::<&str>, String)` + `Clone`, `RenderableCursor`, `CursorShape` (all
five variants), `SelectionRange { start, end, is_block }`, `Point::new(Line, Column)`,
and `CellFlags::to_vte()` (`frame.rs:236-261`, `#[cfg(test)]`).

### 6.3 Behavioural test suites that pin engine semantics

| Suite | File:lines | What it pins |
|---|---|---|
| Resize / ConPTY policy (10 tests) | `crates/terminal/src/model.rs:616-910` | Bottom-anchored `Term::resize`, `grow_lines` cursor movement, `WRAPLINE` join/split, `Grid::scroll_up`, `saved_cursor` clamping, `display_offset` restoration, alt-screen park/swap, negative `Line` indexing (`:628`, `:654`, `:769`, `:816`) |
| Sixel (10 tests) | `crates/terminal/src/sixel_tests.rs:46-271` | Decoder, placement, cursor advance, clipping, scroll-into-history, erase/overwrite/RIS, DA1 |
| Snapshot / damage (5 tests) | `crates/terminal/src/content.rs:269-324` | `damage()` Full on first read, Partial (cursor row only) when unchanged |
| Search (11 tests) | `crates/terminal/src/search.rs:226-344` | `topmost_line()`/`bottommost_line()` span, wide-char spacer handling |
| Router / pump (≈25 tests) | `crates/terminal/src/backend/backend_tests.rs:98-640` | Every `Event` variant → `SessionEvent`; OSC colour query deferral; ordering (reliable events before the batch's `Output`) |
| Frame conversions (5 tests) | `crates/terminal-view/src/render/frame.rs:764-890` | `Color`/`CellFlags`/`CursorShape` round-trips; row hashing |
| Loopback PTY loop (in-memory) | `crates/local-shell/src/event_loop_tests.rs:196-...` | `EventedReadWrite` / `EventedPty` / `OnResize` are implementable **outside** the engine |

---

## 7. Engine behaviour the docs promise

Condensed from `docs/terminal-backend.md` and `docs/osc-sequences-checklist.md`.
(Full extraction retained in the sub-sections; citations are `file:line`.)

### 7.1 `docs/terminal-backend.md`

**Locking & snapshots**
- `Arc<FairMutex<Term<EP>>>` + snapshot is the whole model; fairness stops the main
  thread starving behind the pump — `:32`, `:131`, `:136-138`.
- Never hold the lock across layout/paint; copy, drop, paint — `:142-146`, `:173-175`.
- No cached `last_content`: `snapshot()` locks, copies, **consumes damage**, unlocks — `:148-156`.
- `snapshot_into(&mut TerminalContent)` refills in place, **zero steady-state
  allocation**, and also consumes damage — `:84`, `:627`.
- `query_state()` is O(1); `query_line_range_cells` is damage-free and O(window×cols);
  there is **deliberately no damage-free full-grid snapshot** — `:151-155`, `:625-626`.
- **Never block inside a `Term` callback** — `send_event` runs during
  `Processor::advance` under the lock; the listener may only `try_send` — `:248-254`.
- Pump uses `try_lock_unfair`, escalating to `lock_unfair` only on a full 1 MiB
  buffer — `:387-390`.

**Grid / viewport**
- `display_offset` counts lines scrolled into scrollback — `:227-228`, `:414`.
- `Config::scrolling_history` (default 10 000, user-settable) — `:361`, `:592`, `:818`.
- Engine gives no absolute line id; OneTerm layers `LineAccounting` — `:188`.
- `display_iter` cells must distinguish wide-char spacers and zero-width chars — `:570-577`.
- Grid text (incl. scrollback) must be copyable under one short lock — `:702-703`.

**Resize (DEC-0008, the most load-bearing contract)** — `:192-246`
- `pty_resize` strictly precedes `resize_grid`; never resize from a read loop.
- `Term::resize` **anchors the bottom row**: row grow pulls `min(history, added)` rows
  from scrollback into the top and moves the cursor down by that amount.
- Column grow joins `WRAPLINE` rows (cursor keeps its index); column shrink splits and
  pushes top rows into history.
- Implicit wrap must set `WRAPLINE` — this is what makes join/split work.
- `Grid::resize` must work on a **history-less scratch grid** (used to *measure* the
  reflowed cursor row with the same code path).
- `Grid::scroll_up` over the whole screen; saved cursor moves with the correction.
- Pre-resize `display_offset` preserved (clamped to history).
- Alt grid has no history; the primary grid is the inactive one and still gets corrected.
- Resize **rotates the selection** and leaves the terminal **fully damaged**.
- Regression suites named as must-pass: `model.rs` `keep_viewport_top_*` /
  `default_grow_*`, `local_session_grow_policy_matches_conpty`,
  `ssh_session_keeps_the_default_grow_policy` — `:244-246`.

**Modes / colours / graphics / events**
- `is_mouse_mode()` = **any** `MOUSE_MODE` bit — `:631`.
- Live `Term` colours readable for `ColorRequest` replies, theme defaults as fallback — `:187`.
- `Term::take_graphics`: images decoded since the previous snapshot, **each handed out
  exactly once** — `:158-160`.
- `Cell::graphic()` anchors an image so it scrolls, erases and resizes with its cells — `:160-162`.
- The complete listener contract enumerated at `:187`: `Wakeup`, `Title`/`ResetTitle`,
  `Osc` (fork), `ClearScreen` (fork), `ColorRequest`, `PtyWrite`, `Bell`,
  `ClipboardStore`/`ClipboardLoad`.

**Performance**
- One read chunk = one parse batch = **one** coalescible `Output` — `:84`, `:412-413`.
- 1 MiB heap read buffer; poller waits with no timeout (idle tab never wakes) — `:387`, `:390-392`.
- `snapshot()` exactly once per painted frame — `:175`.
- Known risk: FairMutex-in-paint jitter; `yes` spam → continuous redraw — `:830`, `:833`.

**PTY ownership**
- PTY is created, polled and dropped on one dedicated owner thread; the shipped loop is
  OneTerm's `ShellEventLoop<P: EventedPty + OnResize>`, not the engine's — `:382-392`.
- ConPTY selected automatically; `notify_resize` drives `ResizePseudoConsole`;
  `ChildExitWatcher` is race-free — `:396`, `:401`, `:407`.
- SSH needs the grid only, not the PTY — `:127`.
- Bounded queues (256 msgs / 4 MiB), FIFO atomic-at-enqueue writes, latest-value resize,
  out-of-band close, `Output` as the only coalescible event — `:421-441`.

### 7.2 `docs/osc-sequences-checklist.md`

| Sequence | Status | Owner today | Doc lines |
|---|---|---|---|
| OSC 0 / 2 (title) | supported | engine → `Event::Title` | `:54`, `:56`, `:219` |
| OSC 1 (icon name) | not supported | — | `:55` |
| OSC 4 set + query, OSC 104 reset | supported | engine parses; query → `ColorRequest`, OneTerm replies after the batch | `:64`, `:66`, `:69-72`, `:222`, `:240`, `:285-286` |
| OSC 10 / 11 / 12 (+ `?` query), OSC 110 / 111 / 112 reset | supported | same | `:80-82`, `:87`, `:91-93`, `:223`, `:239` |
| OSC 5 / 105, 13 / 14 / 113 / 114, 17 / 19 / 117 / 119, 39 | not supported | — | `:65`, `:67`, `:83-89`, `:241`, `:289` |
| OSC 52 write | supported | engine decodes base64 → `ClipboardStore`; OneTerm applies policy | `:101`, `:105-113`, `:224`, `:237` |
| OSC 52 read | opt-in, default **off** | `ClipboardLoad` → `SessionEvent::ClipboardRead`; OneTerm builds the reply | `:105-113`, `:282` |
| OSC 7 (cwd) | supported | **OneTerm**, via the fork's `Event::Osc` | `:138`, `:141-143`, `:220`, `:235`, `:269-270`, `:280` |
| OSC 8 (hyperlink, `id=` grouping) | supported | **engine** stores it on the cell; OneTerm vets the target | `:120`, `:122-130`, `:221`, `:236`, `:282` |
| OSC 9 (notification), 9;4 (progress), 9;7 (agent status) | supported | **OneTerm**, via `Event::Osc` — upstream *drops* these | `:151-153`, `:158-162`, `:226`, `:242`, `:279`, `:287-288` |
| OSC 9;1 / 9;2 / 9;3, OSC 99, OSC 777 | not supported | — | `:154-156`, `:162`, `:243`, `:290` |
| OSC 133 A/B/C/D (+ exit code) | supported | **OneTerm**, via `Event::Osc` | `:170`, `:176-185`, `:225`, `:238`, `:280` |
| OSC 133;P, OSC 633 * | not supported | — | `:171-173`, `:184`, `:228`, `:244`, `:290` |
| OSC 50 / 20 / 21 / 22 / 46 / 66 / 1337 / 3008, Kitty APC graphics | not supported | — | `:193`, `:201-211`, `:245`, `:290` |
| DCS `q` (Sixel) | supported | **vendored `Term`** (IN-0028) | `:209-211`, `:245`, `:248` |
| DA1 reply `CSI ? 62 ; 4 c` | emitted | vendored `Term` | `:211` |
| `BEL` and `ESC \` string terminators | both accepted | parser | `:22-31`, `:266` |
| Colour syntaxes `rgb:RRRR/GGGG/BBBB`, `rgb:RR/GG/BB`, `#RRGGBB`, `?` | accepted | engine colour parser | `:41-46`, `:273` |

**Doc drift to fix while rewriting** (flagged, not fixed here):
1. The checklist (`:143`, `:185`, `:269-270`, `:280`) says OneTerm parses OSC 7/133 "in
   parallel" via a second parser and calls the type `OscSink`. That is **stale**: since
   the fork's single-pass hook there is exactly one parser, the type is `OscRouter<T>`,
   and `local-shell/src/event_loop.rs:5-7` says so explicitly.
2. The checklist (`:280`) promises **FIFO ordering of multiple OSCs in one read batch**.
   The forwarding hook must preserve it (it does today — synchronous, byte-ordered).
3. `vendor/README.md` §2 lists a non-existent `Term::set_cell_size()` (see §5.4).

---

## 8. Alacritty idioms that are awkward for OneTerm today

Flagged so the new design can drop them rather than reproduce them.

| Idiom | Why it hurts | Evidence |
|---|---|---|
| **Full snapshot copy per frame** | Every painted frame clones `rows × cols` `Cell`s (each potentially touching an `Arc<CellExtra>` refcount) out of the grid under the lock, then the view converts each one again into its own `Cell`. `snapshot_into` removes the *allocation* but not the copy or the refcount traffic. A borrow-with-guard or a double-buffered/generation-stamped grid could remove the copy entirely. | `content.rs:205-209`, `frame.rs:298-319`, `docs/terminal-backend.md:146` budgets it as "~1000 cells/frame" |
| **`FairMutex` around the whole `Term`** | One lock covers parse, render-snapshot, query, scroll, selection and resize. The pump already works around it with `try_lock_unfair` + escalation, and the listener must be non-blocking *because* the lock is held during parse. A split (grid snapshot vs. control state) or an SPSC hand-off of finished frames would remove the whole class of problem. | `event_loop.rs:410-415`, `osc_router.rs:9-11`, `docs/terminal-backend.md:248-254`, `:830` |
| **Listener events fire under the lock** | Forces `SessionEventSink`'s two-tier deferred/reliable machinery, `flush_reliable_blocking`, and the CORR-01 deadlock test. An engine that returns events as a batch from `advance` would delete all of it. | `pump.rs:163-178`, `backend_tests.rs:296-319` |
| **Negative `Line` indexing** | `Line(i32)` where `< 0` means history forces every consumer to do `line + display_offset` and to reason about a moving origin. Six conversion sites; two different fallbacks in `frame.rs:514-532`. A stable absolute row id (or an explicit `(history, viewport)` split) would be clearer *and* would subsume `LineAccounting`. | `search.rs:41-52`, `frame.rs:525`, `:541`, `:550-557`, `model.rs:430` |
| **`Arc<CellExtra>` for zerowidth + hyperlink + underline colour + graphic** | Four rarely-set attributes share one COW allocation with hand-written "can I drop it?" predicates that every new attribute must extend (the Sixel patch had to touch two of them). Clone-per-snapshot pays an atomic refcount per decorated cell. Interned side tables keyed by row/column would be cheaper and additive. | `vendor/…/term/cell.rs:124-132`, `:185-241`; patch 0003 §5.4 |
| **`Event::Osc` deep-copies params** | `Vec<Vec<u8>>` per forwarded OSC on the hot path, purely so the event can cross a channel — then `osc_router.rs:250` immediately re-borrows them as `&[&[u8]]`. A callback or a borrowed batch would avoid both. | patch 0002; `osc_router.rs:249-257` |
| **No `inactive_grid()` accessor** | The ConPTY resize correction has to park the alt grid in a local, install a placeholder `Grid`, `swap_alt()` twice and put it back — 35 lines of ceremony for "operate on the primary grid". | `model.rs:481-516` |
| **`PTY_CHILD_EVENT_TOKEN` is `pub(crate)` on Unix** | OneTerm hard-codes `1` and documents the hazard. | `event_loop.rs:58-64` |
| **Platform-asymmetric PTY child API** | `pty.child().id()` (Unix) vs `pty.child_watcher().pid()` (Windows) needs two cfg'd functions for one value. | `event_loop.rs:168-179` |
| **`NamedColor` discriminant arithmetic** | `nc as usize - NamedColor::DimBlack as usize` in two crates couples the colour model to enum ordering. | `palette.rs:136`, `frame.rs:118`, `:121` |
| **`total_lines()` saturates at the scrollback cap** | Forces the newline-counting heuristic in `LineAccounting` (explicitly marked PERF-19 "replace by a grid counter in the fork"). An engine-side monotonic line counter deletes that module. | `line_accounting.rs:1-14` |
| **`damage()` needs `&mut self`** | Only reason `TerminalContent::refill` takes `&mut Term` — an immutable damage read (or damage returned with the frame) would let the render path take a shared lock. | `content.rs:171-172` |
| **Graphics have no invalidation signal** | Images die implicitly; the embedder's cache must guess liveness or leak by id. | §5.4 |
| **`Config` carries dead knobs** | `vi_mode_cursor_style`, `kitty_keyboard`, `osc52` are never set by OneTerm; `semantic_escape_chars` is left at the default. | `local-shell/src/session.rs:93-96`, `ssh/src/session.rs:244-247` |

---

## 9. Minimal engine surface

Stated as capabilities, not type names. Each line is a hard requirement backed by a call
site or a test above; the "may change" column says where OneTerm is willing to adapt.

### 9.1 Parse

| # | Capability | Required semantics | May change |
|---|---|---|---|
| P1 | Feed a byte chunk into a terminal | Push API, parser state survives chunk boundaries, caller owns the buffer. Synchronized updates (DCS 2026) honoured. | Signature; whether it returns events instead of calling back. |
| P2 | Surface OSC payloads the engine does not itself implement | Raw params **including the OSC number**, byte slices (no UTF-8 guarantee), `bell_terminated` flag, **FIFO in byte order**, no dedup. Must cover at minimum OSC 7, 9, 9;4, 9;7, 133, and OSC 8 with ≤ 2 params. | Ownership (borrowed batch strongly preferred over `Vec<Vec<u8>>`); delivery mechanism. |
| P3 | Streaming DCS passthrough | `hook(params, final_byte)` → per-byte `put` → `unhook`; a new hook aborts the previous sequence. Needed only if Sixel stays outside the core. | Could be internal-only if Sixel is built in. |
| P4 | Strip escape sequences from a byte stream without a terminal | Used by session logging (`logging.rs`). | Could be a separate tiny OneTerm helper; does not have to be engine API. |

### 9.2 Screen model

| # | Capability | Required semantics | May change |
|---|---|---|---|
| G1 | Iterate the visible cells with the scroll offset applied | Dense row-major, `rows × cols`, top-left first, each item carrying its grid position. Must support addressing a sub-range of rows cheaply (`skip/take` today). | Whether it copies or borrows; a borrowed frame is preferred. |
| G2 | Read arbitrary rows including scrollback | Random access by (row, column) over `history ∪ viewport`, plus the row extent (`topmost`, `bottommost`). Used by search, URL detection, the gutter and the resize probe. | Negative-index convention is **not** required; an absolute row id is welcome. |
| G3 | Report the scroll offset | Lines scrolled into history; `0` = bottom. Display row = grid row + offset. | Could be replaced by exposing display rows directly. |
| G4 | Report dimensions | columns, visible rows, total rows, history size. Must be implementable by plain OneTerm structs too (`TerminalSize`, `GridSize`, `Dims` all implement it today for resize and tests). | Trait shape. |
| G5 | Monotonic absolute output-line counter | Today `LineAccounting` reconstructs it from saturating `total_lines` + newline counting. An engine-side counter that never saturates deletes that module. | New capability — currently missing. |
| G6 | Per-cell content | base `char`; combining/ZWJ tail; foreground and background as {named, palette index, truecolor}; attribute bits `INVERSE BOLD ITALIC DIM HIDDEN STRIKEOUT WRAPLINE WIDE_CHAR WIDE_CHAR_SPACER LEADING_WIDE_CHAR_SPACER` + underline {none, single, double, curly, dotted, dashed}; OSC 8 hyperlink (id + uri); image-fragment reference. Attributes must be hashable to a stable `u64` for the row-plan cache. | Representation entirely. `underline_color` is **not** needed. `BOLD_ITALIC`/`DIM_BOLD` composites are not needed. |
| G7 | Blank-cell predicate | space + default bg + no hyperlink + none of `INVERSE|underline|STRIKEOUT|WIDE_CHAR_SPACER` (`content.rs:30-37`). | Could be engine-provided. |
| G8 | Scroll the viewport | By signed delta and to the bottom. (`PageUp`/`PageDown`/`Top` are not used.) | |
| G9 | Damage since the last read | `Full` or a set of **display row** indices; reading it clears it. Must report Full after construction and after a resize, and only the cursor row when nothing changed. | Should be readable through a shared reference (see §8). |
| G10 | Resize with reflow | Bottom-anchored: row grow pulls `min(history, added)` rows into the top and moves the cursor down; column grow joins `WRAPLINE` rows keeping the cursor index; column shrink splits and pushes rows into history. Preserves `display_offset` where possible, leaves everything damaged. | |
| G11 | Resize primitives the ConPTY correction needs | (a) reflow a **history-less scratch grid** to measure where reflow puts the cursor; (b) scroll a row range up, pushing rows into history and blanking the bottom; (c) resize with reflow **off**; (d) move the saved (DECSC) cursor with the correction; (e) operate on the **inactive** (primary) grid while the alt screen is active. | Could collapse into one "resize with conhost policy" engine call, which would delete `model.rs:440-541` outright — **the single biggest simplification available**. |
| G12 | Cursor | position in the same coordinate space as cells, plus shape ∈ {Block, Beam, Underline, HollowBlock, Hidden}; `Hidden` carries visibility. | |

### 9.3 Selection

| # | Capability | Required semantics |
|---|---|---|
| S1 | Start / update a selection from a (row, column, left-or-right-half) anchor | Four kinds: character, word/semantic, line, block. |
| S2 | Report the selection as an inclusive range in cell coordinates, or `None` | Cheaply — `has_selection()` must not materialise text. |
| S3 | Materialise the selected text | Wide chars and wrapped lines handled. |
| S4 | Select everything including scrollback | |
| S5 | Selection survives (or is explicitly invalidated by) scrolling and resize | OneTerm currently drops it after a corrected resize; a clear rule either way is enough. |

### 9.4 Modes & outbound events

| # | Capability | Required semantics |
|---|---|---|
| M1 | Query mode bits | At minimum: alt-screen, any-mouse-reporting (as one composite), mouse-motion, mouse-drag, SGR mouse, UTF-8 mouse, bracketed paste, DECCKM (app cursor), cursor-visible. Nothing else is read today. |
| E1 | Repaint hint | One per parse batch, coalescible. |
| E2 | Title set / reset | |
| E3 | Clipboard write request (OSC 52) with the payload already decoded | |
| E4 | Clipboard read request (OSC 52 `?`) | OneTerm formats the reply itself; the engine's formatter closure is unused. |
| E5 | Terminal-originated bytes to send back to the PTY | DA/DSR/colour replies. |
| E6 | Bell | |
| E7 | Screen cleared | `CSI 2J`, `CSI 3J`, RIS — and *not* `CSI 0J`/`1J`. Ordering after RIS state reset is relied on. |
| E8 | Colour query, answerable **after** the parse batch | Index space: `0..=255` palette, plus distinct fg / bg / cursor slots. Reply must be deferrable because the answer needs a lock-free read. |
| E9 | Delivery discipline | Events must **not** require the consumer to block while the engine holds its lock. Returning a batch from the feed call satisfies this trivially; a non-blocking callback also works. |

### 9.5 Colours

| # | Capability | Required semantics |
|---|---|---|
| C1 | A cell colour that distinguishes {named, palette index 0-255, truecolor RGB} | Round-trippable to OneTerm's own `Color` (`frame.rs:765-787`). |
| C2 | Named colours: 16 ANSI + 8 dim + fg / bg / cursor / bright-fg / dim-fg | Must be enumerable **without discriminant arithmetic** (see §8). |
| C3 | OSC-set override table (`None` = not overridden) | Readable for rendering (`dynamic_colors`) and for answering queries. Reset (OSC 104/110/111/112) clears back to `None`. |
| C4 | Theme defaults handed *to* the engine | fg, bg, cursor, 16 ANSI — used as the fallback when answering a query. |

### 9.6 Graphics

| # | Capability | Required semantics |
|---|---|---|
| I1 | Decode `DCS q` Sixel while its bytes stream in | Subset in §5.4; `MAX_DIMENSION = 4096`; untouched pixels transparent. |
| I2 | Anchor the image to grid cells | Per-cell `(image id, col, row)` offset inside the image's own cell grid; scrolls, erases and resizes with the cells. |
| I3 | Hand decoded pixels to the embedder **exactly once** | RGBA8, row-major, straight alpha, oldest first. |
| I4 | Geometry in a virtual 10×20 cell | So cursor advance matches conhost; the renderer rescales. The engine must **not** learn the real cell size. |
| I5 | Advertise Sixel in the DA1 answer | `CSI ? 62 ; 4 c`. |
| I6 | *(wanted, missing today)* Tell the embedder when an image id is no longer referenced | Would let the renderer evict its texture cache instead of guessing. |

### 9.7 PTY (local shell only — SSH must not need it)

| # | Capability | Required semantics |
|---|---|---|
| T1 | Spawn a child in a pseudoterminal | program + args, cwd, env, drain-on-exit, Unix signal mask, Windows arg escaping. ConPTY on Windows, loading `conpty.dll` itself. |
| T2 | Register the PTY with a `polling::Poller` and read/write it | OneTerm drives its own loop; the PTY must be a passive pollable object. Two tokens: I/O and child. **The child token must be a public constant.** |
| T3 | Resize the pseudoconsole | rows/cols; pixel size unused. |
| T4 | Race-free child-exit notification with an optional exit code | |
| T5 | Expose the child pid uniformly across platforms | |
| T6 | Be implementable outside the crate | `event_loop_tests.rs:270-332` implements the whole PTY contract over loopback sockets; keep the traits public. |

### 9.8 Test support

| # | Capability |
|---|---|
| X1 | Build a terminal from a text blob, sized to the content, with `\n` setting `WRAPLINE` and `\r\n` not, wide chars getting a spacer cell, cursor visible, damage Full (today: `mock_term`). |
| X2 | A trivial size type and a null event sink (today: `TermSize`, `VoidListener`). |
| X3 | Construct cells directly for fixtures: default cell, set char / fg / bg / flags, push a combining mark, attach a hyperlink, attach an image fragment. |
| X4 | Available in `#[cfg(test)]` of *downstream* crates without a feature flag (today `pub mod test` is not cfg-gated). |

### 9.9 Not needed — do not build

Vi mode and vi motions · regex scrollback search · the engine's own event loop /
notifier / message enum · `setup_env` · `bounds_to_string` / `expand_wide` /
`semantic_escape_chars` accessor / `cursor_style` accessor / `scroll_to_point` /
`set_options` · per-cell underline colour (SGR 58/59) · `Scroll::{PageUp, PageDown, Top}` ·
mouse-cursor-dirty / cursor-blinking / text-area-size events · Kitty keyboard protocol ·
`Term::set_cell_size` (never existed).

---

## 10. Open questions

1. **Coordinate model.** Keep signed grid rows (history negative) or move to absolute
   row ids? Absolute ids would subsume `LineAccounting` (G5) and simplify search, the
   gutter and `frame.rs:514-532`, but every existing `Line(i32)` assertion in
   `model.rs:616-910` and `search.rs` would need rewriting. Decide before the grid
   design, not after.
2. **Snapshot vs. borrow.** Is the per-frame full-grid copy replaced by a borrowed frame
   (guard held during layout, violating `docs/terminal-backend.md:173-175`), a
   double-buffered grid, or a generation-stamped row cache? This drives the locking
   model and whether `FairMutex` survives at all.
3. **Event delivery.** Batch-return from `feed()` (deletes `SessionEventSink`'s deferred
   machinery, `pump.rs:163-178`) or keep a non-blocking callback (smaller diff at the
   call sites)? Batch-return also fixes the `Event::Osc` allocation.
4. **ConPTY resize policy in the engine?** `model.rs:440-541` plus `conhost_cursor_row`
   is ~100 lines of grid surgery that exists only because `Term::resize` anchors the
   bottom row. Should the engine take a resize *policy* (bottom-anchor vs.
   keep-viewport-top) natively, deleting G11 (a)–(e) from the public surface? This is
   the largest single simplification on the table and needs a DEC.
5. **Scrollback storage.** The current model (ring buffer, `total_lines` saturating at
   `scrolling_history`) is what forces G5. Is unbounded-with-eviction or a
   disk/compressed tail in scope, or is a monotonic counter on top of the same ring
   enough?
6. **Reflow fidelity bar.** How exactly must column-resize reflow match the current
   `WRAPLINE` join/split? The ten `keep_viewport_top_*` tests assert *cell-exact* output
   after widen/narrow with wrapped lines spanning the history boundary
   (`model.rs:797-818`). Are those the contract, or are they "current behaviour" that a
   new reflow may legitimately change?
7. **Sixel fidelity.** Keep the current deliberate deviations (P2 background-select
   ignored → transparent; aspect ratio ignored; no Kitty/iTerm2 protocol), or fix them
   while rewriting? `sixel_tests.rs` pins the current behaviour.
8. **Graphics lifetime signal (I6).** Add reference counting / an eviction event, or
   keep implicit death and an embedder-side LRU? Affects the renderer's texture cache
   design.
9. **`vte::Perform` for session logging.** Does the new engine export a
   sequence-stripping parser, or does `logging.rs` get a ~60-line OneTerm-owned state
   machine? The latter removes the last "second parser" from the codebase.
10. **Colour model shape.** Does the engine keep a 259-slot override table with numeric
    indices (`osc_color.rs:23-27`) or expose a typed `{palette(u8), fg, bg, cursor}`
    key? The latter removes the magic constants 256/257/258 from three files.
11. **PTY: keep or split?** `crates/ssh` needs zero PTY code, and `crates/tools` needs
    PTY with no `Term`. Two crates (`*-vt` + `*-pty`) or one crate with a feature?
12. **MSRV / edition floor.** The vendored manifest pins edition 2024 / rustc 1.85; the
    workspace pins 1.96 (`Cargo.toml:38`). Anything the new engine needs from that gap
    is free — worth confirming nothing plans to publish it standalone.
13. **Doc drift** (§7.2): three stale statements should be corrected as part of the
    intake's documentation action, not silently.

---

*Sources: full read of the 40 files matching `alacritty_terminal|vte` under `crates/`,
the five patches under `vendor/patches/`, the relevant vendored sources, and
`docs/terminal-backend.md` + `docs/osc-sequences-checklist.md`.*
