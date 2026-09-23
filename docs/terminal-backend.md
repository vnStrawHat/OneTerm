# Terminal Backend Design — OneTerm

> **Status (2026-08):** design record kept current with the implementation. Rust
> blocks are design sketches (types/fields are abridged); the "Current
> implementation" notes and §5.3/§6.5/§7 describe the code as it is. Authoritative
> signatures live in `crates/terminal/src/session.rs`, `crates/terminal/src/backend/`,
> `crates/local-shell/src/` and `crates/ssh/src/`.
>
> Design document for the terminal part: **local shell** + **SSH session**, sharing one VT
> engine. Windows-first priority. Local shell can be `cmd` / `powershell` / `pwsh` / custom.
>
> **The engine is `oneterm-vt` (`crates/vt`), OneTerm's own** (`IN-0029`, `DEC-0014`). The
> shape below was derived from Zed's use of a patched `alacritty` fork — tty + event loop
> + `FairMutex` + snapshot — which OneTerm vendored and shipped until `US-0087` deleted it.
> The architecture survived the swap; the dependency did not, and §4 below records what
> replaced it.
>
> Zed source files referenced (paths in **Zed's** tree, same rev lock `1d217ee39…`,
> not this repository's):
> - `zed/crates/terminal/src/terminal.rs` — model + EventLoop + PTY.
> - `zed/crates/terminal_view/src/terminal_element.rs` — custom `Element` rendering the grid.
> - `zed/crates/terminal_view/src/terminal_view.rs` — View + IME (`ImeState`).
>
> **Core decisions** (see brainstorm history):
> 1. **Local and SSH do not know about each other** — each keeps only its transport
>    (`PtyTransport`: write / resize / close) and its own read loop; parsing, OSC routing,
>    event delivery and the state cache come from the shared pump layer in
>    `oneterm-terminal::backend` (§5.3).
> 2. **Both sessions share one engine and one custom GPUI `Element`.**
> 3. **Local uses `oneterm_vt::pty` + OneTerm's own poll loop** (not `portable-pty`, and not
>    an engine-supplied event loop). `oneterm_vt::pty` — `oneterm-vt`'s default-on `pty`
>    feature (`US-0104`) — owns the ConPTY / `openpty` transport and
>    nothing else; see `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md`.
> 4. **The VT engine is first-party** (`crates/vt`): no forked dependency, no `[patch]`, and a
>    new capability is added under ordinary review (`DEC-0014`).
> 5. **Concurrency model**: `Arc<FairMutex<Terminal>>` + snapshot. The engine owns no lock;
>    the adapter in `crates/terminal` does.
> 6. **The pure kit** (`core`) does not depend on GPUI.
>
> **Forward pointer (2026-09, records only):** `IN-0038` proposes making `oneterm-vt` an
> embeddable, published terminal core. It moves OSC parsing, key/mouse encoding and scrollback
> search out of `crates/terminal` and into the engine, replaces `OscClaims` with a route/override
> table, and renames the engine's `render` module to `snapshot`. When those packets land, section
> 5.3 (`OscRouter`), section 4 (dependencies) and the adapter file layout below change; the target
> shape is in
> [`spec-intakes/IN-0038-embeddable-vt-core/high-level-design.md`](spec-intakes/IN-0038-embeddable-vt-core/high-level-design.md).
> The search half has landed (`US-0100`): `oneterm_vt::search` is where the matcher lives, and
> `crates/terminal` re-exports `SearchMatch` and `SearchOptions` from it. The rest of this document
> is not stale yet.
>
> **Forward pointer (2026-09, records only):** `IN-0039` follows `IN-0038` and closes the four gaps
> an outside evaluation found in the engine -- four unnameable public return types, a kitty
> keyboard protocol that is answered but not encoded, `DECRQCRA` / `DECRQSS` / `XTGETTCAP`, and
> published performance evidence. Nothing in it changes this document's adapter, pump, lock or
> transport story; the one packet with a user-observable effect (`US-0105`) changes the bytes the
> **key encoder** returns for a program that negotiated the kitty protocol. The engine is still
> consumed as a git dependency and is not published to crates.io. Target shape in
> [`spec-intakes/IN-0039-vt-gaps-and-publish/high-level-design.md`](spec-intakes/IN-0039-vt-gaps-and-publish/high-level-design.md).
>
> **Forward pointer (2026-09, records only):** `IN-0040` is the application half of that encoder.
> `US-0105` shipped the engine and recorded that `crates/terminal-view` registers `on_key_down`
> alone, so a key **release** is never observed and a held-key **repeat** is indistinguishable from
> a first press -- which means the kitty `REPORT_EVENT_TYPES` flag OneTerm negotiates and answers
> is not honoured inside OneTerm itself. Section 10 below, "Input: keystroke -> byte + IME",
> describes path 1 as it stands today and is the document that packet updates. Nothing else here
> moves: no pump, lock, transport or session change, and no file under `crates/vt`. Target shape in
> [`spec-intakes/IN-0040-view-key-release-repeat/high-level-design.md`](spec-intakes/IN-0040-view-key-release-repeat/high-level-design.md).

---

## 1. Principles

| # | Principle | Consequence |
|---|---|---|
| 1 | Clear layer separation | UI contains no protocol logic; protocol knows nothing about UI. |
| 2 | Local & SSH independent | The two backends do not depend on each other; both sit on the shared pump layer (`OscRouter<T: PtyTransport>` + `TerminalPump`) and add only transport + read loop. |
| 3 | Shared rendering | A single `TerminalElement` paints the grid for both local and ssh — only needs `&TerminalContent`. |
| 4 | Snapshot, no lock-while-paint | The pump updates the snapshot; render reads the snapshot, does not hold `FairMutex` while painting. |
| 5 | Windows-first | Local prefers ConPTY; `cmd`/`pwsh`/`powershell` shells are configurable. |
| 6 | Strict version lock | `gpui` + `gpui_platform` move together as one release family. The VT engine is first-party, so it has no rev to lock. |

---

## 2. Architecture diagram

```
┌─────────────────── ui crate (GPUI + gpui-component) ───────────────────┐
│  TerminalView (impl Render; hosts local AND ssh sessions)          │
│   ├─ chrome: Button, Tabs, Dock… (gpui-component)                        │
│   └─ child: TerminalElement  (custom gpui::Element, shared)            │
│          • reads TerminalContent snapshot → paint_quad / shape_line      │
│          • EntityInputHandler (IME) + mouse + wheel                   │
└───────▲─────────────────────────────────────────▲──────────────────────┘
        │ TerminalSession trait (terminal)       │
   ┌────┴────────────────┐               ┌────────┴───────────────┐
   │  local-shell crate  │               │  ssh crate             │  ← INDEPENDENT
   │  PseudoConsole +    │               │  russh + shared tokio   │     don't know each other
   │  poll loop (ConPTY) │               │  channel + pty-req      │
   │  LocalTransport     │               │  SshTransport (Cmd)     │
   │  Term<OscRouter<    │               │  Term<OscRouter<        │
   │   LocalTransport>>  │               │   SshTransport>>        │
   └────┬────────────────┘               └────────┬───────────────┘
        └──────────────┬──────────────────────────┘
                ┌──────▼──────────┐
                │ terminal crate  │  backend pump layer: SharedState, SessionEventSink,
                │ (no GPUI)       │  OscRouter, TerminalPump, PtyTransport;
                │                 │  TerminalHandle (lock + render demand);
                │                 │  TerminalSession, TerminalContent,
                │                 │  key/mouse encode, osc, url
                └──────┬──────────┘
                ┌──────▼───────┐
                │  core crate  │  SshConfig, ShellKind/LocalShellConfig, SftpBackend,
                │  (leaf)      │  AppError
                └──────────────┘
```

**Data flow**:
- Input: `Keystroke` (GPUI) → `terminal-view/src/input/keys.rs` maps it to a `KeyEvent` (`Press` / `Repeat` / `Release`) → `oneterm_vt::input::encode_key_event` → `Vec<u8>` → `session.write(bytes)` → PTY/channel.
- Output: PTY/channel → pump (`ShellEventLoop` local / `ssh_main_task` tokio ssh) → `TerminalPump::advance` feeds the per-session printable-output logger and the engine under the terminal lock, collecting the batch's events → `finish_batch` releases the lock, sends those events and then one `SessionEvent::Output` → View `cx.notify()` → `TerminalElement` prepaint calls `session.snapshot_into(&mut cache.snapshot)` (short lock, one `snapshot_update` into the `SnapshotState` the reusable `TerminalContent` in `RenderCache` owns — zero steady-state allocation — and advances *that buffer's* damage watermark) and paints from that buffer; `session.snapshot()` remains as the allocating convenience for tests and one-off reads. Logging behavior and file lifecycle are owned by [`terminal-logging.md`](terminal-logging.md).

---

## 3. Responsibilities per crate

| Crate | Terminal role |
|---|---|
| `core` | `ShellKind` + `LocalShellConfig` + `SshConfig` (config), `SftpBackend`, `AppError` (leaf, no GPUI). |
| `terminal` | `TerminalSession` trait + `SessionEvent`, the `TerminalContent` frame, `TerminalPalette`, printable-output logging controller/parser, `osc`/`url` (key and mouse encoding moved to `oneterm_vt::input` at `US-0099`; this crate re-exports the names), the shared terminal handle (`TerminalHandle`: the `parking_lot::FairMutex` around `oneterm_vt::Terminal` plus the render-demand flag), and the **backend pump layer** (`backend` module: `SharedState`, `SessionEventSink`, `OscRouter`, `TerminalPump`, `PtyTransport`) shared by both backends. |
| `local-shell` | `LocalSession` implementing `PtyOwner`; `spawn` returns the `PtySession` the UI drives (`US-0091`). Spawns a shell via `oneterm_vt::pty::PseudoConsole::spawn` and pumps it with a custom poll loop (`ShellEventLoop<P: EventedPty>`) feeding `TerminalPump`. ConPTY on Windows. `LocalTransport: PtyTransport` (notifier queue). Only `LocalSession` is public. |
| `ssh` | `SshSession` implementing `PtyOwner`; `connect` returns the `PtySession` the UI drives (`US-0091`). russh client on the shared tokio runtime; `ssh_main_task` feeds `TerminalPump`. pty-req + shell + `window_change` + exit-status. `SshTransport: PtyTransport` (bounded `Cmd` channel). SFTP task lifetime tied to the connection. Only `SshSession` + `connect` are public. |
| `terminal-view` | `TerminalElement` (custom `gpui::Element`), `TerminalView` (`Render`; one view type hosts any `TerminalSession`, local or SSH), `TerminalPanel`/`PanelSpec` (dock tab), IME (`EntityInputHandler`), mouse/wheel, font measure, theme → `TerminalPalette`. |
| `app` | Installs the `SessionFactory` (`AppSessionFactory`) + `WorkspaceCommands` through `AppServices`; only crate that links `ssh`/`local-shell`. |

> Dependency rules: `app → {terminal-view, ssh, local-shell, terminal, core, …}`, `ssh → {terminal, core}`,
> `local-shell → {terminal, core}`. No UI crate imports `ssh`/`local-shell` — sessions are created via
> `oneterm_terminal::SessionFactory` and driven via `TerminalSession` (see `docs/agents/crate-dependency-rules.md` R3).

---

## 4. Dependencies

```toml
# root Cargo.toml [workspace.dependencies] (authoritative list: docs/agents/dependencies.md §1/§3)
oneterm-vt = { path = "crates/vt" }   # the VT engine: parser, grid, reflow, selection, damage, graphics
oneterm-vt = { path = "crates/vt" }   # the engine, and its default-on `pty` transport
async-channel = "2"      # event sub (no tokio leaked out)
russh = { version = "0.61", default-features = false, features = ["ring", "flate2", "rsa"] }  # keys API is russh::keys (russh-keys was merged in)
russh-sftp = "2.3"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "sync", "io-util", "net", "macros", "fs"] }
```

> **There is no third-party terminal engine and no `[patch]` section.** OneTerm shipped a
> vendored, patched `alacritty_terminal` / `vte` fork until `IN-0029` replaced it with
> `crates/vt`; `US-0087` deleted the fork, its five patches, its refresh/check CI job and
> both `[patch]` blocks. A capability the engine lacks is added to `crates/vt` under ordinary
> review (`DEC-0014`), not to a fork.
>
> `portable-pty` is **no longer used** for local (brainstorm decision), and `deny.toml` bans
> it. `ssh` needs no local PTY — only the grid, which it gets through `crates/terminal`.

---

## 5. Concurrency model: `Arc<TerminalHandle>` + snapshot

### 5.1. Why

- The **pump** (local `EventLoop` thread / ssh tokio task) feeds bytes to the engine on
  another thread.
- **Render** (`TerminalElement::paint`) runs on the GPUI main thread.
- Both need access to the same engine ⇒ use a fair mutex (fair = the main thread doesn't
  starve for the lock while the pump is busy).

Since IN-0029 `US-0081` the engine is `oneterm_vt::Terminal`, which holds **no lock, no
atomic and no interior mutability** and takes `&mut self`: the synchronisation is the
embedder's choice, and `crates/terminal` makes it `parking_lot::FairMutex`. The shared
handle is `oneterm_terminal::SharedTerminal` = `Arc<TerminalHandle>`
(`crates/terminal/src/handle.rs`), which owns that mutex — directly around
`oneterm_vt::Terminal`, with no wrapper between — and one `Demand`. `Demand` is the
adapter's own primitive and lives beside the policy that uses it, in the same file
(`US-0090` moved it out of `crates/vt`, which now holds no interior mutability at all).

**The demand/yield handshake** the design adds on top of fairness: a fair mutex hands
the lock over on unlock, but a pump that unlocks and immediately relocks still beats a
sleeping waiter, so the render path raises a one-bit flag and the read loop asks for it
at a chunk boundary. `US-0082` wired the adapter's half of it:

- `TerminalHandle::lock_for_render()` raises the demand, then locks. Only the snapshot
  path calls it — `query_state()` and `terminal_info()` run on the same thread and are
  O(1) under the lock, so making them raise it would ask the pump to yield several times
  per frame for reads that never wait.
- `TerminalHandle::render_demand_raised()` is the pump's half: "is a frame waiting for
  me?". The asking takes nothing away — the waiter clears its own demand once it holds
  the lock (`US-0082` rework), so a standing `true` means a frame is still outside.
  A read loop calls it at a chunk boundary — **after** that batch's reply bytes have
  left (R-37, § 5.3) — and drops its guard when it answers `true`. Both pumps do:
  `ssh_main_task` since `US-0084` (it locks per chunk, so its answer to a raised flag is
  to yield the tokio task before the next chunk relocks; a waiting frame gets the engine
  in about 400 us under a flood) and the local loop since `US-0083`
  (`crates/local-shell/src/event_loop.rs`, the shape the 354 ms starvation was measured
  on). (It was called `take_render_demand()` until `US-0090`; the verb was residue from
  the one-shot flag the rework replaced, and the twin `render_demand_raised()` that read
  the same value went with the rename.)

  How long a frame waits is `bytes-per-lock-hold / parse rate`, so the read is capped at
  `MAX_LOCKED_READ` (64 KiB), the bytes handed to one `advance`. ConPTY delivers 82 bytes
  at the median and never notices; a socket delivers ~600 KB per read, where the cap is
  worth 10x on the worst-case wait (`US-0083`, measured both ways). A yield also **ends
  the batch** (`finish_batch`), because on a transport that never runs dry that is the
  only place a batch ever ends, and with it the repaint hint and the batch's events.

  The yield gives the engine up **without leaving the read loop**, which is a
  platform constraint rather than a preference: the conout ring re-arms its wake-up
  only when a read finds it empty (`crates/vt/src/pty/windows/pipe.rs`,
  `PipeReader::read` arming `caller_waiting`), so a loop that stops reading while
  bytes are still buffered parks in `poll.wait` and the session freezes. The loop
  therefore drops the guard, keeps draining the pipe into its buffer, and re-locks
  once the frame is done.

### 5.2. Snapshot vs live borrow (IMPORTANT)

| | Live borrow (WRONG) | Snapshot (CORRECT — what Zed does) |
|---|---|---|
| Paint | `let g = term.lock();` then paint **holding the guard** | `let snap = { let g = term.lock(); build content }; drop(g);` then paint |
| Problem | slow paint (thousands of GPU calls) → pump `term.lock().advance()` **blocks** → jitter under output bursts (`yes`, `cat large file`) | Lock only for µs to copy, pump runs in parallel with paint |
| Cost | 0 | 1 copy ~thousand cells/frame (far cheaper than paint) |

**Convention (as implemented)**: there is **no cached `last_content`**. The pump only
sends the `Output` hint; `TerminalSession::snapshot()` (`TerminalModel::snapshot`,
`crates/terminal/src/model.rs`) takes the `FairMutex` for the microseconds needed to
copy `TerminalContent` (and advance this consumer's damage watermark), releases it, and the element paints
from that owned copy — the lock is never held **while painting**. Non-render reads use
`query_state()` (O(1), no cells) or `query_line_range_cells()` (damage-free,
O(window×cols)); there is deliberately no damage-free full-grid snapshot — an
O(rows×cols) clone per event is a footgun. Every one of them is a short lock too, so the pump and the
UI contend only briefly (see the "never block inside a `Term` callback" rule in §5.3).

**Damage is a per-row sequence number, not a reset pass.** The engine stamps every row
it mutates with the batch's `SeqNo`; a consumer keeps a **watermark** and "changed for me"
is `row.seq > watermark`. Nobody clears anybody else's damage, so a second consumer needs
no engine change.

**The frame source is the snapshot state, and `TerminalContent` owns it** (`US-0082`). The
watermark belongs to the buffer the consumer keeps — the one `RenderCache` reuses — so
`snapshot_into` is one `Terminal::snapshot_update` into it, and a freshly built
`TerminalContent` reports `Full` by construction instead of consuming somebody else's
damage. Native reads go through `TerminalContent::{update, rows, changed, size,
render_cursor, modes, selection_range, placements, row_id, display_row}`.

**There is nothing else on it** since `US-0085`. The dense `Vec<IndexedCell>`, the forked
engine's value types around it and the per-frame rebuild that produced them are gone; the
view reads `TerminalContent::rows` and resolves the engine's own `SnapshotRow` / `SnapshotCell`
itself, so a frame that changed nothing copies nothing.

The frame also carries `graphics`: the Sixel images decoded since the previous one (each
handed out once, drained by the adapter through `Terminal::take_graphics`). Cells
reference them through `Cell::graphic()` (`GraphicCell { id, col, row }`), whose offset
inside the image is derived from the engine's placement table (R-21), so an image
scrolls, is erased and is resized with its cells; the view keeps the pixels in a bounded
store (IN-0028, DEC-0012).

```rust
// Pump (ShellEventLoop / ssh_main_task) — per read chunk:
pump.advance(&mut *term.lock(), bytes);       // feed + drain under the engine lock
pump.finish_batch_blocking(true);             // lock released: send the batch's events, then Output

// Render (TerminalElement prepaint):
session.snapshot_into(&mut cache.snapshot);   // short engine lock (raises the render demand)
// paint from the buffer: content.cells / content.cursor / content.mode ...
```

> Do NOT hold the lock across layout/paint work; copy, drop the guard, then paint.
> `snapshot_into()` is called exactly once per frame from the render path.

### 5.3. Shared pump layer (`oneterm_terminal::backend`)

Both backends use the same event drain and the same batch driver; they only
provide a transport and a read loop.

| Type | Role |
|---|---|
| `PtyTransport` (trait) | The backend half: `pty_write` / `pty_resize` / `pty_close`. Non-blocking, `Clone` (Arc handles). `LocalTransport` wraps the owner-thread notifier queue; `SshTransport` wraps the bounded `Cmd` channel (byte budget, coalesced resize, closing flag). |
| `SharedState` (`Arc<SharedSessionState>`) | Title / cwd / clipboard / exit code / OSC 133 counters / theme default colours / agent-status seq watermarks (OSC 20308) behind one mutex; `alive`, rx/tx bytes, absolute line count and clear epoch as atomics so a parse batch never takes the mutex. Handed to the SFTP browser as `TerminalCapabilities::cwd_source` so it can read the live cwd. |
| `SessionEventSink` | Delivery policy: `post_repaint()` is coalescible (the hint is dropped and counted when the 4096-slot queue is full), `send_blocking()` / `send()` are reliable and apply backpressure. Counters (`EventQueueDiagnostics`) for tests/diagnostics. The deferred FIFO, `flush_reliable[_blocking]` and `forward_lifecycle*` were deleted at `US-0082`: events are values now, so the drain no longer runs inside a callback and the sink may simply block. |
| `OscRouter<T: PtyTransport>` | The drain over the `EventBatch` `Terminal::feed` fills — **not** a callback installed in the engine, so nothing runs inside it while the caller holds the lock. `drain(&batch, &mut out)` does only what must happen under the lock and cannot wait, in **one pass in byte order**: a `VtEvent::Reply` goes to the transport in the position the input asked for, whichever side produced it, so a reply never overtakes another reply (conhost blocks up to a second for the DA1 answer at session start, which is a latency requirement, not an ordering one) — `Repaint` dropped (the pump owns the hint); `Title`/`TitleReset` → state cache; OSC 52 store/load gated by `TerminalSecurityPolicy` + `ClipboardOrigin` (remote default off — the same code for both backends, so the policy cannot drift; the policy is the user's, derived from `TerminalSettings` by `terminal-view` and passed through `SessionFactory::{spawn_local, connect_ssh}` into `OscRouter::with_security`); `Cwd`/`Notification`/`Progress`/`ShellMark` (the engine's typed OSC 7, 9, `9;4` and 133 events — parsed once, in the engine, and met here by **policy** only: `sanitize_cwd`, the 8 KiB notification cap and its ten-per-second limiter, the prompt counter and the last exit code) → state; `Osc` (only what `Config::osc_routes` routes out of the engine: OSC 20308, the agent channel, and — for one release — the deprecated `9;7` alias, which arrives as a `BuiltinAndForward` wrap of OSC 9 because routing is per number and sub-codes are payload; `OscRoutes::large` on both spellings so an 8 KiB payload is not cut at the parser's 2 KiB inline bound) → seq dedup + the support reply; `ScreenCleared` → clear epoch; `ColorQuery { key, .. }` → the pending colour-query queue keyed by `ColorKey`, answered by `TerminalPump::color_replies` after the batch from the live engine colours with the theme defaults as fallback. The matching `SessionEvent`s are **appended to `out`**, the pump's pending vector, and sent once the lock is released. `RowsScrolled` / `RowsTrimmed` / `GraphicReleased` are dropped until a `RowId`-keyed consumer exists (`US-0085`), and so are `IconName`, `Pointer` and `CursorStyleChanged`, which nothing above the seam speaks yet. |
| `TerminalPump<T>` | Owns one reusable `EventBatch`, the pending `SessionEvent` vector, the gutter's line count and a router clone. Per chunk: `advance(term, bytes)` under the engine lock — `Terminal::feed` into the batch, then `OscRouter::drain` — (or `process_chunk(&handle, bytes)` which also answers colour queries and writes the replies), then `finish_batch[_blocking](repaint)` once the lock is released: publish the line count → send the batch's events (backpressure) → `Output`. Lifecycle: `publish_exit*` / `publish_closed*`, each sending everything queued before it first. The gutter's absolute line number is `Terminal::lines_produced()` — output lines, never implicit wraps, never reset by a clear — floored at the rows the grid holds, because the gutter labels display row `i` with `absolute - offset - rows + i`. `LineAccounting` and its heuristic over `total_lines` were deleted at `US-0082`. |

Local (`ShellEventLoop<P>`) uses the blocking variants on the PTY owner thread;
SSH (`ssh_main_task`) uses the async ones on the tokio runtime. Neither backend
resizes the grid from its loop — the UI thread does that in
`TerminalSession::resize`: `PtyTransport::pty_resize` first, so the process learns
the new size before any output for it arrives, then `TerminalModel::resize_grid`.

**The cell size is the adapter's second duty to the engine** (`BUG-0061`). The grid is
measured in cells and the engine has no way to learn how many pixels one of them is, so
`Terminal::set_cell_pixels` — what `CSI 14 t` multiplies by the grid and reports — stays
`(0, 0)` until an embedder says otherwise, and a program that gates image support on a
non-zero answer (OpenTUI, chafa) draws block mosaics instead of Sixels. `TerminalElement`
pushes it through `TerminalSession::set_cell_pixels` → `TerminalModel::set_cell_pixels`
from `prepaint`, from the measured `CellMetrics::device`, **before** the grid check that
calls `resize`, so the first reply after spawn already carries real pixels. It is a
separate method and not a wider `resize` because a DPI-scale change moves the device cell
while rows and columns stand still, which `resize` would discard. The same `CellMetrics`
the painter uses is the only source.

Since `BUG-0062` that number is **not only a report**: the engine divides an image's pixels
by it to get the image's footprint in cells (`ceil(pixels / cell)`), and by the same number
for the `bands * 6 / cell_height` cursor walk, so a program that sizes a Sixel from the
`CSI 14 t` reply covers the cells it meant. `VIRTUAL_CELL` (10x20) survives only as the
fallback for an embedder that never calls `set_cell_pixels` — which keeps VT340 sizing
byte for byte, and is why the `crates/tools` corpus is unaffected. The painter no longer
rescales to a virtual cell either: it draws the image at its own pixel size, clipped to
the placement's footprint and the grid (`CellMetrics::image_quad`). A font-size or DPI
change after an image is placed keeps the footprint in cells and crops the picture; the
engine does not re-place.

**Resize policy (`ResizePolicy`, DEC-0008).** Both policies are the engine's own since
`US-0077`, and since `US-0082` the adapter does nothing but pick one: `TerminalModel::new`
takes anything convertible into `oneterm_vt::ResizePolicy` and `resize_grid` passes it
straight to `Terminal::resize`.

`ResizePolicy::BottomAnchor` anchors the bottom row: on a row grow it pulls
`min(history, lines_added)` rows out of scrollback into the top of the viewport and moves
the cursor down by that amount; on a column change it joins
rows flagged wrapped and lets history fill the rows that vanished, or splits rows and
pushes the top ones into history. That matches a Unix PTY or a remote shell, which reflow
on their side and repaint, so SSH keeps it. conhost behind ConPTY does neither
(measured with raw PTY dumps in BUG-0051): after `ResizePseudoConsole` it repaints
nothing, re-wraps the rows of the old viewport at the new width as if the top row started
a line, keeps that content at the top, leaves the rows below blank and addresses later
output with absolute cursor positions (`CUP`) in those coordinates. Long lines reach the
parser as continuous text (an implicit wrap, so the engine flags the row wrapped), and
after a maximize from 33x43 to 52x158 with `ls -lath` output conhost's next `CUP` named
row 12 while the grid cursor sat on row 33 (21 joined rows); a pure widen to 132 columns
gave row 14 (19 joined); a grow to 49x34 gave row 38 (5 split rows). With the default
policy typed input therefore lands inside the listing and an exiting alt-screen TUI leaves
stale rows (IN-0019). Local sessions on Windows select `ResizePolicy::KeepViewportTop`
(`local_resize_policy()` in `crates/local-shell/src/session_terminal.rs`); SSH passes
`SSH_RESIZE_POLICY` (`crates/ssh/src/session.rs`). Since `US-0091` both name the engine's
own `oneterm_vt::ResizePolicy` and hand it to `PtySession::new`, so the policy stays with
the backend that owns the PTY. The adapter enum `oneterm_terminal::model::ResizePolicy`
that used to stand between them is deleted.

`KeepViewportTop` is implemented **inside the engine** (`crates/vt/src/reflow/`): it
measures conhost's cursor row by reflowing the viewport-top-to-cursor range through the
same reflow iterator the resize uses, then shifts the viewport by the difference. The
61-line scratch-grid probe the adapter used to run on top of a bottom-anchored resize —
`resize_keeping_viewport_top` and `conhost_cursor_row` — is **deleted** (`US-0082`).
Invariants after the correction, unchanged:

- the cursor row equals the row conhost's next `CUP` names; the saved cursor moves with
  it (clamped to the screen);
- every row below the top row that starts at column 0 has the same text as in conhost;
  when the top row continued a wrapped line from history the grid shows that line joined
  whole while conhost shows it torn at the old top row, so the top rows may differ;
- the joined or pulled rows return to scrollback, and blank rows fill the bottom;
- the pre-resize scroll offset is kept (clamped to history), so a scrolled-back viewport
  keeps its top row and extends downward;
- the correction also applies while the alt screen is active (the primary screen is the
  inactive one; the alt screen has no history and is bottom-anchored), so the shell's
  prompt lands on its row after the TUI exits;
- a selection whose rows moved is dropped, and a resize invalidates every row, so the
  next frame repaints the viewport;
- a row shrink and a column shrink with the cursor on the bottom row produce the
  bottom-anchored result (the measured shift is zero).

The visible tradeoff is blank rows below the prompt after a maximize instead of recovered
scrollback; the rows are still in history. Tests:
`crates/vt/src/reflow/reflow_tests.rs` (`keep_viewport_top_*`, `default_policy_grow_*`),
`crates/terminal/src/model_tests.rs::resize_grid_applies_the_backend_policy`,
`crates/local-shell/src/session_tests.rs::local_session_grow_policy_matches_conpty`,
`crates/ssh/src/session.rs::ssh_session_keeps_the_default_grow_policy`.

**Nothing waits on the UI under the engine lock.** The rule used to be "never block
inside a `Term` callback", because `send_event` ran during `Processor::advance` with the
lock held while the UI thread needed that same lock to drain the event queue (CORR-01).
There is no callback any more: `OscRouter::drain` collects `SessionEvent`s into the
pump's vector, and `finish_batch[_blocking]` sends them **after** the guard is dropped
(event loop: blocking send; tokio task: `send().await`). Ordering seen by the UI is
unchanged — the events a batch produced, then that batch's single `Output` hint — and
lifecycle events (`Exited`/`Closed`) send everything queued before them first.

The layer is testable without a PTY or a network: `test_support::FakePtyTransport`
records writes, and `crates/terminal/src/backend/backend_tests.rs` drives the pump
end to end (title/bell ordering, colour replies, backpressure, lifecycle).

---

## 6. Local backend (`local` crate, Windows-first)

### 6.1. Configurable shell

`core` defines:

```rust
/// Local shell kind.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ShellKind {
    /// Windows cmd.exe (COMSPEC).
    Cmd,
    /// Windows PowerShell 5.1 (powershell.exe).
    PowerShell,
    /// PowerShell 7+ (pwsh.exe).
    Pwsh,
    /// Unix shells.
    Bash,
    Zsh,
    Sh,
    /// Custom command.
    Custom,
}

/// Local shell spawn configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalShellConfig {
    pub kind: ShellKind,
    /// Executable path (None → auto-detect by kind + platform).
    pub program: Option<PathBuf>,
    /// Extra command-line args.
    pub args: Vec<String>,
    /// Env overrides (TERM, COLORTERM, LANG…). TERM=xterm-256color is set by default.
    pub env: HashMap<String, String>,
    /// Working directory (None → current app cwd).
    pub cwd: Option<PathBuf>,
    /// Force UTF-8 codepage (Windows cmd). Default true.
    pub utf8: bool,
}
```

Resolving `ShellKind` → executable + args + env (Windows-first):

| Kind | Default program | Default args | UTF-8 |
|---|---|---|---|
| `Cmd` | `%COMSPEC%` (cmd.exe) | `/K chcp 65001 >nul` (if `utf8`) | `chcp 65001` |
| `PowerShell` | `powershell.exe` (found in PATH / `where`) | `-NoLogo` | env `LANG=en_US.UTF-8`; `[Console]::OutputEncoding=UTF8` via `-NoExit -Command` arg |
| `Pwsh` | `pwsh.exe` | `-NoLogo` | like PowerShell |
| `Bash`/`Zsh`/`Sh` | `$SHELL` / `/bin/bash`… | `-l` (login) per config | env `LANG`/`LC_ALL` |
| `Custom` | `program` (required) | `args` | per `env`/`utf8` |

> The Terminal page of the Settings window (`crates/settings-ui/src/terminal/`) lets the user pick `kind`, type a custom
> `program`, add `args`, set `cwd`, toggle `utf8`, and configure local/SSH output logging. Persisted in `terminal.json` via `oneterm_settings::TerminalConfig`; see [`terminal-logging.md`](terminal-logging.md).

#### 6.1.1 Elevated windows resolve their shell from a trusted table

An **elevated** OneTerm process does not use the table above. `resolve_shell` asks
`oneterm_core::config::elevation` first, and that module builds the spawn configuration from
scratch: **no field of `terminal.json`'s shell block survives except `utf8`** (the console
codepage). The kind resolves to an absolute path OneTerm trusts:

| Kind | Resolved to |
|---|---|
| `Cmd` | `%SystemRoot%\System32\cmd.exe` — **not** `%COMSPEC%`, which is a plain environment variable any parent process can set |
| `PowerShell` | `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe` |
| `Pwsh` | the highest numeric `<N>` for which `%ProgramFiles%\PowerShell\<N>\pwsh.exe` exists |
| `Bash` / `Zsh` / `Sh` / `Custom` | refused — an elevated window runs only the three Windows shells |

`PATH` is not consulted and neither is `terminal.json`. The rule behind it (`DEC-0019`
rule 4, mitigation M3) is that **nothing the unelevated process writes may direct what the
elevated process executes**, and `terminal.json` is a file the unelevated process can write.

`program` and `args` are not the only fields of that file which direct what executes, so
they are not the only ones dropped:

| Field | Elevated | Why |
|---|---|---|
| `program`, `args` | dropped | names the executable and its command line |
| `env` | dropped | the ConPTY environment block writes custom entries **ahead** of the inherited ones, so a `PATH` or `PSModulePath` here decides what the shell runs on the first unqualified command |
| `cwd` | dropped (the home directory is used) | `cmd.exe` searches the working directory before `PATH` |
| `logging.*` | ignored; logging is **off** | a log file is a file created at a config-chosen path under an administrator token, and `write_mode: overwrite` makes it a create-or-truncate primitive |
| `utf8` | honoured | presentation only |

`ResolvedShell` carries the working directory for the same reason the others are dropped:
the spawn takes **everything** from `resolve_shell`'s result, so the single guard at the top
of that function cannot be walked around by a spawner that reads one more field off the
original configuration.

One user-visible consequence, because `cwd` is dropped unconditionally: in an elevated
window **Duplicate tab** and **New Terminal Here** open at the home directory rather than
inheriting the tab's live OSC 7 working directory. An elevated shell takes its working
directory from nothing outside itself — `cmd.exe` searches it before `PATH`, so a cwd is a
code-execution input and not a convenience. The three directories above are writable only by
`TrustedInstaller` and the Administrators group, so after resolution the path is checked
for existence and nothing else. The resolution happens **inside** the elevated process:
resolving first and passing the path as an argument would make the unelevated process the
thing that names the executable, which is the same hole with extra steps. The cost is
stated rather than hidden: a pwsh installed outside `%ProgramFiles%\PowerShell` cannot be
elevated from the menu — the consent prompt is answered and the process then exits with
code 3 and one message naming the path it looked for. The one accepted argument is
`--elevated-shell cmd|powershell|pwsh`; every other command line is one message box and
exit code 2, with no window. See
[`spec-intakes/IN-0043-run-shell-as-administrator/low-level-design/elevated-instance.md`](spec-intakes/IN-0043-run-shell-as-administrator/low-level-design/elevated-instance.md).

### 6.1.2 What each shell integration emits (OSC 7 and OSC 133)

OneTerm injects its own prompt integration so a tab can report its working directory
(OSC 7) and the boundaries of each command (OSC 133 `A` prompt start, `B` prompt end /
input start, `C` output start, `D;<code>` command done). The engine parses all of them
(`crates/vt/src/terminal/dispatch.rs`); the marks are what
[`terminal-semantic-highlighting.md` §4.2](terminal-semantic-highlighting.md) calls the
fast path, and `D`'s code is the only source of the exit-code tint.

**The route decides the ceiling.** A local shell is reached through the *spawn environment*
only — no file written, no profile edited. An SSH shell is reached by typing one line into
the shell after it starts, so there OneTerm can define functions.

| Shell | A | B | C | D | Mechanism |
| --- | :-: | :-: | :-: | :-: | --- |
| `cmd.exe` | ✓ | ✓ | — | — | `PROMPT`: `$E]7;$P$E\` + `A` + `$P$G` + `B` |
| PowerShell / pwsh | ✓ | ✓ | ✓ | ✓ | `-Command` startup: the global `prompt` function is wrapped (writes `D;<code>`, OSC 7 and `A`, returns the original prompt with `B` appended); `C` from a PSReadLine `Enter` handler |
| bash (local) | ✓ | ✓ | ✓ | ✓ | `PROMPT_COMMAND` (`D;<code>`, OSC 7, `A`, and the one-time `B` append to `PS1`) + `PS0` (`C`) |
| zsh (local) | ✓ | ✓ | — | ✓ | `PS1`, with zsh's own `%?` supplying `D`'s code |
| bash (SSH) | ✓ | ✓ | ✓ | ✓ | typed bootstrap: `PROMPT_COMMAND` + `PS0` + `PS1` append |
| zsh (SSH) | ✓ | ✓ | ✓ | ✓ | typed bootstrap: `precmd_functions` + `preexec_functions` + `PS1` append |
| `sh` / `dash` (SSH) | ✓ | — | — | — | typed bootstrap: one OSC 7 + `A` at connect, nothing per prompt |
| `fish` / `csh` / `tcsh` (SSH) | — | — | — | — | the bootstrap is POSIX and they cannot parse it; see below |
| Custom shell, or a user prompt override | — | — | — | — | the shell must emit the sequences itself |

The gaps in that table are the route's, not oversights:

- **`cmd.exe` has one hook.** `PROMPT` is expanded once before the prompt and its `$` codes
  contain nothing for the error level, so there is no place to put `C` or `D`.
- **Local zsh cannot reach `C`.** `preexec` is a *function* and no environment variable
  carries zsh code; installing one needs a sourced file, which the env-only route excludes.
  Remote zsh does emit `C`, because the bootstrap is typed into a running shell.
- **PowerShell's `C` needs PSReadLine, and an `Enter` OneTerm may chain to.** The handler is
  installed only when `Set-PSReadLineKeyHandler` resolves, and only when the name the
  current `Enter` binding reports is a real public static of `PSConsoleReadLine` —
  `[…PSConsoleReadLine].GetMethod($f)`, because the question is *can I still call this
  afterwards?* A user who bound `Enter` to a script block of their own keeps it, and that
  tab reports `A`/`B` only. Do not shorten this to a comparison against the string
  `CustomAction`: `Get-PSReadLineKeyHandler` reports that name only for a block bound
  **without** a `-BriefDescription`, and with one it reports the description, so the
  comparison passes and every `Enter` then calls a method that does not exist and submits
  nothing at all.
- **`fish`, `csh` and `tcsh` get nothing at all.** The bootstrap is one POSIX line; those
  shells cannot parse it and answer with a burst of syntax errors instead of reaching the
  `sh`/`dash` row. The session stays usable and OneTerm does not notice — the write
  succeeded, which is all `send_shell_integration_bootstrap` can observe.

**What OneTerm does with a variable the user already set.** Three rules, because "yield to
the user" and "do not break the user" want different things:

- A user-supplied `PROMPT`, `PS1` or `PS0` **wins outright** — OneTerm sets its own only
  when the variable is absent.
- A user-supplied `PROMPT_COMMAND` that **already emits OSC 133** (its value contains
  `133;`) is left exactly as it is, and `PS0` is then left alone too. That is the
  integration's **opt-out** for a local bash: say "I run my own shell integration" by
  emitting the marks, and OneTerm adds nothing. Two sets of marks per prompt is worse than
  none of OneTerm's.
- Any **other** user-supplied `PROMPT_COMMAND` is *appended to*, with OneTerm's part first
  and `$?` restored by a trailing subshell so the user's hook still sees the real exit
  status. There is no separate on/off switch for a local shell; `shell_integration` is a
  field of `SshSessionConfig` and covers the SSH bootstrap only.

`B` is appended to `PS1` from inside `PROMPT_COMMAND` (local bash) or by the bootstrap
(SSH) rather than injected as a `PS1` variable, because an rc file sets `PS1` after the
environment is read and would drop the marker. In bash it goes in as **four raw bytes and
no backslash at all** — `\001 ESC ]133;B BEL \002`, where `\001`/`\002` are what bash's
`\[`/`\]` expand to. A mark written with backslashes merges with its neighbours at
whichever end the backslash sits: `ESC \` beside the `\` of a closing `\]` becomes the
escape `\\` and prints a stray `]` that never closes the region, and a leading `\[` is
swallowed by a user `PS1` ending in a lone `\`. Raw bytes have no end to get wrong. zsh is
unaffected, because it expands no backslashes in a prompt.

**No integration emits `D` before the first command of the session**, except local zsh,
whose `PS1`-only route has nowhere to hold the flag. bash and the SSH bootstrap skip the
first `D` behind a seen-flag, and the bootstrap clears that flag again after the one call
that paints the first prompt.

Two ceilings of the env-only route worth knowing: a `.bashrc` that assigns an **array**
`PROMPT_COMMAND=(…)` (bash ≥ 5.1) discards OneTerm's scalar outright and the integration
stops; and a `.zshrc` that sets `PROMPT` replaces the generated `PS1` and takes the marks
with it. Neither is detectable from here.

The engine parses OSC 7 itself and reports the host and the path unresolved; the router
sanitises the path into `SessionEvent::Cwd` and updates `TerminalSession::cwd()`. The OSC
133 marks reach the cells as `SnapshotCell::semantic` and the router as
`SessionEvent::ShellIntegration`, whose `D` updates `SharedState::last_exit_code`.

### 6.2. Spawn via `oneterm_vt::pty`

> **Why an elevated shell needs a second process.** The shell is attached to its
> pseudo-console through exactly one channel:
> `UpdateProcThreadAttribute(.., PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, ..)` on the
> `STARTUPINFOEXW` handed to `CreateProcessW`. `CreateProcessW` creates the child with a
> copy of the caller's token and has no flag meaning "elevate", and a process cannot raise
> its own token; the supported route is `ShellExecuteEx` with the `runas` verb, which takes
> no attribute list, no `STARTUPINFOEX` and no environment block. So an elevated child can
> join neither this process's pseudo-console nor the environment that carries OneTerm's
> whole shell integration (`PROMPT`, `PROMPT_COMMAND`, `PS1`, `TERM`). OneTerm therefore
> relaunches **itself** elevated and lets the new process create its own pseudo-console:
> `resolve_shell` and `CreatePseudoConsole` both run under the elevated token, so OSC 7 cwd
> reporting and OSC 133 prompt marks survive intact. That is the whole reason the elevated
> shell opens in a second window (`DEC-0019`; `docs/gui-layout.md` §Panel registration).

> Original design sketch (the forked engine's `EventLoop` + an `ArcSwap` cache). The shipped code
> described below the sketch differs: a custom `ShellEventLoop`, no `last_content`
> cache, `LocalTransport`/`OscRouter` from §5.3, and — since `US-0091` — no `TerminalSession`
> on `LocalSession` at all: `spawn` returns `PtySession<LocalSession>` and the struct keeps
> only the listener and the owner-thread join handle (the shell config is not retained).

```rust
use the_forked_engine::{event_loop::EventLoop, sync::FairMutex, term::{Config, Term}, tty::{self, Options, Shell, WindowSize}};  // historical sketch; the fork is gone

pub struct LocalSession {
    term: Arc<FairMutex<Term<LocalListener>>>,
    notifier: Notifier,                          // EventLoop channel (Msg::Input/Resize/Shutdown)
    last_content: Arc<ArcSwap<TerminalContent>>, // snapshot
    event_tx: Sender<SessionEvent>,
    config: LocalShellConfig,
    // child exit, alive flag…
}

impl LocalSession {
    pub fn spawn(cfg: LocalShellConfig, initial: PtySize) -> core::Result<Self> {
        let (program, args, env) = resolve_shell(&cfg)?;     // §6.1 table
        let opts = Options {
            shell: Some(Shell { program: program.into(), args: args.into_iter().map(Into::into).collect() }),
            working_directory: cfg.cwd.clone(),
            env: env.into_iter().collect(),
            ..Default::default()
        };
        let winsize = WindowSize { rows: initial.rows, cols: initial.cols, ..Default::default() };
        let pty = tty::new(&opts, winsize, 0).map_err(|e| AppError::msg(e.to_string()))?;

        let term = Arc::new(FairMutex::new(Term::new(
            Config { scrolling_history: 10_000, ..Default::default() },
            &TermSize::from(initial),
            LocalListener { /* event_tx clone */ },
        )));
        let mut event_loop = EventLoop::new(term.clone(), LocalListener::default(), pty, false, false)
            .map_err(|e| AppError::msg(e.to_string()))?;
        let notifier = event_loop.channel();           // for write/resize
        event_loop.run().detach();                      // spawn pump thread
        // … child exit watcher (ChildExitWatcher) → SessionEvent::Exited
        Ok(Self { term, notifier, last_content: Arc::new(ArcSwap::from_pointee(default())), event_tx, config: cfg })
    }

    pub fn write(&self, bytes: &[u8]) { self.notifier.tty_notify(bytes.to_vec().into()); }   // Msg::Input
    pub fn resize(&self, r: u16, c: u16) { self.notifier.notify_resize(WindowSize { rows: r, cols: c, ..Default::default() }); }
    pub fn shutdown(&self) { self.notifier.shutdown(); }
}
```

> The `Notifier` sketch above described the vendored fork's event loop, which OneTerm never
> shipped and which no longer exists. The implementation below is the one to read.

**Current implementation** (`crates/local-shell/src/event_loop.rs`): the loop is a
custom `ShellEventLoop<P: EventedPty + OnResize>` on a dedicated "PTY owner"
thread — the PTY is created, polled and dropped there. It reads with a
heap-allocated 1 MiB buffer into `TerminalPump::advance` under a
`TerminalHandle::try_lock()` guard (blocking on `lock()` only when the buffer is
full), answers colour queries with the same guard, then calls
`finish_batch_blocking`. Each read takes at most `MAX_LOCKED_READ` (64 KiB), so
one lock hold is bounded in bytes. At each chunk boundary it asks
`render_demand_raised()` and, when a frame is waiting, answers that batch's colour
queries, drops the guard and finishes the batch there (§ 5.1) instead of holding
it until the pipe runs dry. The poller waits **without a timeout**, so an idle
tab does not wake up: every `ShellNotifier::send` calls `poller.notify()`, and the
child watcher posts a **keyed completion packet** on `PTY_CHILD_EVENT_TOKEN`.
That difference is load-bearing and not an implementation detail — a bare
`notify()` uses the poller's reserved `NOTIFY_KEY`, which `Events::iter()`
filters out, so it never reaches the loop's iteration at all: the exit has to
come in on its own key or it is not read (§6.3, BUG-0072). The token is compared
**before** the loop's hang-up guard (`classify_event`), because on Unix the exit
and the hang-up of the socket that announced it arrive as one event: the reaper
thread drops its half of the pair immediately after posting, and a closed peer
puts `EPOLLHUP` on the loop's half. `is_interrupt()` means "do not do I/O on a
dead PTY", so it is asked only of the PTY token — asking it first discarded the
exit, permanently, because the source is level-triggered (BUG-0074).
Being generic over the PTY, the loop is unit-tested with a
loopback-socket PTY (`event_loop_tests.rs`) — no shell is spawned to cover
output parsing, input FIFO, resize, colour replies, child exit, shutdown, and the
hand-over itself (`a_flooding_loop_hands_the_engine_to_a_waiting_frame` floods the
loop from one thread while another takes frames). The conout re-arm above is *not*
reachable through the loopback socket, whose readiness is level-triggered; the
real-shell tests in `session_tests.rs` are what cover it.

**Closing a local session** is guaranteed to leave no process behind **while OneTerm is
running** — a **Windows** guarantee, because it rests on the escalation in §6.3. The Unix
transport has no escalation: its `PseudoConsole` has no `Drop` at all, and closing the
master hangs up the slave so the child, as session leader, gets `SIGHUP`.
The grace period below is served on the detached PTY owner thread, so a session
still inside it when the application process exits (or is killed) never gets the escalation —
that hole is recorded under Gaps in `BUG-0055` and is not closed by this design.
`LocalSession::drop`
(and `close()`) sends `ShellMsg::Shutdown`, the owner loop deregisters and returns, and the
`PseudoConsole` is dropped on that thread — `ClosePseudoConsole` first, then the bounded
wait and, if it is needed, the escalation described in §6.3. The owner thread itself is
joined by a detached reaper, never by the caller (`CORR-10`). What survives a discarded
session is measured by `session_orphan_tests.rs`, whose ignored `orphan_liveness_table`
reproduces the full table on demand.

### 6.3. Windows-specific

- **ConPTY**: `oneterm_vt::pty` resolves the bundled `conpty.dll` next to the executable first and
  falls back to `kernel32!CreatePseudoConsole` (Win10 1809+) only when it is missing — DEC-0013.
  The resolved host is logged once at `info` (`conpty: bundled` / `conpty: system`).
- **UTF-8**: `Cmd` → `chcp 65001` (via `/K` args). `pwsh`/`powershell` → set env
  `LANG`/`LC_ALL` + (optionally) an init arg `[Console]::OutputEncoding`.
- **TERM**: always `xterm-256color`, `COLORTERM=truecolor`.
- **Resize**: `Notifier::notify_resize` → ConPTY handles it (no SIGWINCH on Windows).
  The grid is realigned to conhost after every resize (`ResizePolicy::KeepViewportTop`,
  §5.3, DEC-0008): conhost keeps its viewport top, re-wraps the viewport rows in place
  and never repaints, so scrollback is not pulled in and joined or split wrapped rows
  move the cursor row exactly as they do in conhost.
- **Ctrl-C**: byte `0x03` → shell handles it. OK.
- **Child exit**: `oneterm_vt::pty` watches the child handle (race-free) and reports
  `ChildEvent::Exited` on `PTY_CHILD_EVENT_TOKEN` → `SessionEvent::Exited(code)`.
  "Race-free" rests on two orderings inside `ChildExitWatcher`, and the loop above is why
  they are load-bearing: it waits without a timeout and reads the child event **only** when
  the poll names that token, so a wake that is never posted is an exit that is never reported.
  (1) The wait callback queues the exit *before* it posts the completion packet, so a poll the
  packet wakes always finds the event. (2) `RegisterWaitForSingleObject` fires immediately for
  a child that exited before the embedder registered — between `PseudoConsole::spawn` and
  `register` the owner thread still opens the session log file — so whichever of the callback
  and `register` runs **second** posts the wake: the callback records the exit under the same
  lock that holds the poll interest, and a registration that finds it already recorded posts
  the packet itself (BUG-0072). The wake is owed **once per registration**: on Windows
  `register` *is* `reregister` (`crates/vt/src/pty/windows.rs`), so an embedder's loop that
  re-registers each pass to toggle write interest would otherwise spin at 100 % CPU after the
  child exited. Re-registering an interest that has already been woken posts nothing;
  `deregister` clears that mark, so a poller registered after one is woken again — the
  transport cannot know whether the event was ever read, and a missed wake costs more than a
  spare one. A registration after the event was read therefore yields an eventless wake, so
  every consumer must tolerate a wake whose `next_child_event()` is `None`.
- **Close**: `ClosePseudoConsole` only *asks* the host to end the session, and a client
  that had not finished starting when the console went away never processes that request —
  measured, such a `cmd.exe` was still alive 15 s later (and its console host with it),
  while a started one exits within 20 ms with `STATUS_CONTROL_C_EXIT`. So
  `ChildExitWatcher::drop` waits `CHILD_EXIT_GRACE` (2 s) on the child handle after the
  pseudo-console has closed and terminates the child if it is still running, logging the
  pid at `warn` first — DEC-0016, BUG-0055. Only this process's own child is touched, and
  only through its handle: never matching by name is DEC-0005's rule, and reaching no
  further than our own child is DEC-0016's, stricter. The wait runs on the "PTY owner"
  thread, which is reaped detached, so no UI thread ever waits for it — and, for the same
  reason, a session dropped as the application exits is not covered (§6.2).
- **Busy shells are not terminated**: the host's close request reaches every client on the
  console, so `cmd.exe` and a foreground grandchild (`ping -t`, `timeout`) both exit with
  `STATUS_CONTROL_C_EXIT` within ~20 ms, well inside the grace period. A grandchild that
  detached from the console (`start /b`, a GUI child) survives, as it did before.

### 6.4. Re-render perf (per Zed)

- The pump doesn't `notify` per byte — a read chunk is parsed as one batch and
  `finish_batch` sends a **single** coalescible `SessionEvent::Output` (§5.3/§6.5).
- The View `cx.notify()` only when `display_offset`/`mode`/`cursor`/cells actually change
  (compare old vs new snapshot). Avoids continuous redraw under `yes`.
- Log `layout took {:?}` for tuning (copy Zed's `log::debug!`).

### 6.5. Transport backpressure contract

SSH and LocalShell use the same observable overload semantics even though their
owner loops use different channel implementations:

- Command queues are bounded to 256 messages and a 4 MiB aggregate write-payload
  budget. The source-of-truth constants are `SSH_COMMAND_QUEUE_CAPACITY`,
  `SSH_COMMAND_BYTE_BUDGET`, `LOCAL_COMMAND_QUEUE_CAPACITY`, and
  `LOCAL_COMMAND_BYTE_BUDGET`.
- The byte budget itself is **one** mechanism, `ByteBudget<LIMIT>` in
  `crates/terminal/src/backend/byte_budget.rs` (`US-0093`): an atomic total that
  reserves at enqueue and releases on delivery, refusing a reservation that
  would cross `LIMIT` **or** overflow `usize`. The two constants above are the
  `LIMIT` each backend instantiates it with and stay with their backends — the
  value is each backend's policy, identical by coincidence rather than by
  contract, so either may change without touching the other.
- Writes preserve FIFO order and are atomic at enqueue time: the complete write is
  accepted, or `TerminalError::QueueFull`/`TerminalError::Closed` is returned.
  Paste uses the same write path and therefore cannot bypass the byte budget.
- Resize is latest-value delivery. Bursts overwrite one pending size instead of
  consuming one queue slot per intermediate geometry.
- Close/shutdown is out-of-band and has priority over queued input. A queue at
  capacity cannot prevent the owner loop from observing close.
- `SessionEvent::Output` is the only coalescible event. Clipboard, notification,
  progress, agent, title, working-directory, bell, and lifecycle events use
  reliable bounded-channel delivery: they are never dropped, and a slow consumer
  applies backpressure to the pump — but only *between* parse batches, never
  while the engine lock is held (§5.3). A closed consumer is logged and counted by
  diagnostic builds.
- Local child exit always ends the session: `alive = false`, then
  `SessionEvent::Exited(code)` (code may be `None` when the platform watcher could
  not read it) followed by `SessionEvent::Closed`.

Backends do not retry rejected writes because retrying after returning an error
could duplicate input. Callers must report failure or explicitly retry the same
payload. Saturation tests use the production policies and constants.

---

## 7. SSH backend (`ssh` crate)

The Tokio runtime is **hidden**: one process-wide `new_multi_thread` runtime with
`SSH_RUNTIME_WORKERS = 2` worker threads (`crates/ssh/src/session.rs`,
`shared_runtime()`), shared by every SSH session so the thread count does not grow
per tab. The exposed API is sync: `connect()` runs the handshake, auth, `pty-req`,
`shell` and the SFTP channel open inside `runtime.block_on`, then spawns
`ssh_main_task` + `sftp_task` and returns a `Box<dyn TerminalSession>`.

```rust
// abridged — see crates/ssh/src/{session,transport,session_terminal}.rs
pub struct SshSession {
    model: TerminalModel<SshListener>,          // Arc<FairMutex<Term<OscRouter<SshTransport>>>>
    transport: SshTransport,                    // bounded async_channel<Cmd> + closing flag
    state: SharedState,                         // title / cwd / counters (§5.3)
    events: Mutex<Option<async_channel::Receiver<SessionEvent>>>, // handed out once
    sftp: Option<Arc<SftpSession>>,             // same TCP connection, own task
    // …
}

enum Cmd { Write(Vec<u8>), Resize { rows, cols }, Close }

pub fn connect(cfg: SshConfig, initial: PtySize, scrollback: usize)
    -> core::Result<Box<dyn TerminalSession>>
{
    let runtime = shared_runtime()?;                       // 2-worker multi-thread runtime
    runtime.block_on(async {
        // russh::client::connect (host-key policy in handler.rs, known_hosts)
        // → authenticate (none / password / private key, keyboard-interactive fallback)
        // → channel_open_session + request_pty("xterm-256color") + request_shell
        // → open the SFTP channel
    })?;
    runtime.spawn(ssh_main_task(/* channel, term, pump, transport, … */));
    runtime.spawn(sftp_task(/* … */));
    Ok(Box::new(session))
}
```

- `SshListener = OscRouter<SshTransport>` — `PtyWrite(text)` → `SshTransport::pty_write`
  → `Cmd::Write` (256-message queue, 4 MiB byte budget, `Cmd::Resize` coalesced).
- `kind() == SessionKind::Ssh` (for OSC 7 cwd semantics: ssh can be `file://host/…`).
- The Terminal Settings `ssh` group owns transport keepalive for newly opened sessions: enabled by default, 30-second default interval validated to `5..=3600` seconds, and an unanswered-request limit defaulting to three and validated to `1..=20`. Disabled maps to russh `keepalive_interval = None`; enabled maps to the captured interval; the captured limit maps to russh `keepalive_max`. Existing sessions are not reconfigured.
- Exit: `ChannelMsg::ExitStatus { exit_status }` → `SessionEvent::Exited(Some(code))`;
  `Eof`/`Close`/`Cmd::Close`/closing flag → `SessionEvent::Closed`. For a remote SSH close, terminal-view retains prior output for scroll/search/select/copy, blocks outbound writes through the shared alive gate, shows one error toast, and keeps a bottom banner visible: `SSH connection closed. Input is disabled.` Local-shell close presentation is unchanged.
- Shutdown signal: the transport's closing flag (set by `pty_close`) — the task holds
  `cmd_tx` clones itself, so the command channel never closes on its own.
  `SshSession::close()` and `Drop` request close for the shell **and** SFTP; the
  two are idempotent.
- `ssh_main_task` ends in a single teardown block: `channel.close()`,
  `publish_closed()` (sends everything the batch queued, then `Closed`), then it
  cancels the SFTP `CancellationToken` so `sftp_task` exits and
  `SftpBackend::alive()` turns false with the connection (the same token,
  `session_shutdown`, stops the port-forward listeners and relays of
  `crates/ssh/src/tunnel.rs`), and finally drops the target handle and then the
  jump-host handles (`route::JumpHandles`, innermost first) so no hop closes
  under a connection that still rides on it.
- `ssh_main_task` has a third `select!` arm: it serves `HandleRequest`s from the
  forward listeners (one direct-tcpip open per accepted local connection),
  because the connection handle lives only in the task.
- Per data chunk (`US-0084`): `TerminalPump::process_chunk` feeds the engine and drains
  that batch under the lock — replies out first (R-37) — then, the lock released,
  `finish_batch(true).await` sends the batch's events before the `Output` hint (§5.3), and
  the loop asks `SharedTerminal::render_demand_raised()` and yields the task when a frame is
  waiting (§5.1). No `crates/ssh` **source file** names the engine — the shared pump is the
  whole of the terminal side, and the fork's last manifest line left with `US-0085`.
- RSA keys authenticate with `rsa-sha2-*` chosen from the server's `server-sig-algs`
  (fallback SHA-512); legacy SHA-1 `ssh-rsa` is never used.
- Auth: `SshAuthMethod::{None, Password, PrivateKey}` (`crates/core/src/ssh_config.rs`)
  with keyboard-interactive as the password fallback; host keys are checked against
  `known_hosts` (`crates/ssh/src/handler.rs`) — see [`ssh-client-connect.md`](ssh-client-connect.md).
  No ssh-agent support yet.

> Sync→async bridge: `async_channel` in both directions — the UI thread sends `Cmd`
> through `SshTransport`, `ssh_main_task` receives it inside the runtime; outgoing
> `SessionEvent`s go over the bounded `async_channel` the view drains on the GPUI
> executor. No `std::sync::mpsc` and no nested `block_on` after connect.

---

## 8. Rendering (`terminal-view` crate) — `TerminalElement`

Custom `gpui::Element`. Paints from the **snapshot**.

> **Superseded:** the view-layer rendering below is the pre-IN-0018 sketch. The element now lives in
> `crates/terminal-view/src/render/element.rs` and draws from a `Frame` + cached row plans; its
> current owning design is
> [`docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md`](spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md).
> Everything about the backend (sections 1-7) is unaffected.

### 8.1. Structure

```rust
pub struct TerminalElement {
    session: Entity<dyn TerminalSession>,   // or generic
    bounds: TerminalBounds,                  // cell_width, line_height, rows, cols
    theme: TerminalTheme,                    // bg/fg/16 ANSI/cursor → gpui::Hsla
    focus: FocusHandle,
    focused: bool,
    cursor_visible: bool,
    interactivity: Interactivity,
}

impl InteractiveElement for TerminalElement { /*…*/ }
impl StatefulInteractiveElement for TerminalElement {}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;       // hitbox, bg_rects, text_runs, cursor, ime_bounds
    fn request_layout(&mut self, …) -> (LayoutId, ()) { /* size_full or size by rows×cols */ }
    fn paint(&mut self, …, layout: &mut LayoutState, window, cx) {
        let content = self.session.read(cx).snapshot();      // no FairMutex lock
        window.with_content_mask(Some(ContentMask { bounds }), |w| {
            w.paint_quad(fill(bounds, self.theme.bg));
            for rect in &layout.bg_rects { rect.paint(origin, &self.bounds, w); }   // batch background
            for run in &layout.text_runs { run.paint(origin, &self.bounds, w, cx); } // ShapedLine.paint
            // cursor + selection + ime marked text
        });
        window.handle_input(&ElementInputHandler::new(self.session.downgrade(), self.session.clone()));  // IME
    }
}
```

### 8.2. `layout_grid` (batched — copy Zed)

Iterate `content.display_iter` (`IndexedCell`):
- **Background**: group consecutive cells with the same background color (skip default bg) → `Vec<LayoutRect>`
  + `merge_background_regions` (merge horizontally/vertically) to reduce `paint_quad`.
- **Text**: group consecutive cells with the same `TextRun` (fg + bold/italic/underline + font) →
  `Vec<BatchedTextRun>`. Each run: `window.text_system().shape_line(text, font_size,
  &[run], Some(cell_width)).paint(pos, line_height, Left, None, window, cx)`.
- **Wide char spacers** + **zero-width chars** (emoji variation sequences): handle correctly
  (copy Zed's `is_wide_char_spacer` / `append_zero_width_chars` logic).
- **Contrast**: `ensure_minimum_contrast(fg, bg, min)` — skip if
  `is_app_chosen_exact_color` (truecolor/256≥16) or `is_decorative_character`
  (box-drawing/powerline). These functions live in `core` (pure).

### 8.3. Font measure (terminal-specific font)

`TerminalSettings` (own font, independent of gpui-component theme):
```rust
pub struct TerminalSettings {
    pub font_family: String,         // e.g. "Cascadia Mono", "JetBrains Mono"
    pub font_size: f32,               // px
    pub font_weight: u32,
    pub line_height: f32,             // multiplier (1.0 = default)
    pub font_features: FontFeatures,
    pub scrollback: usize,
    pub minimum_contrast: f32,
    pub shell: LocalShellConfig,     // §6.1
}
```
Measure (cached, re-measure on font/size change):
```rust
let probe = window.text_system().shape_line("M".repeat(cols).into(), font_size, &[base_run], Some(target_cell_w));
let cell_width = probe.width() / cols as f32;
let line_height = font_size * settings.line_height;     // or ascent+descent+leading
```

### 8.4. Colors (`terminal` + `terminal-view`)

- `oneterm_terminal::TerminalPalette` (`crates/terminal/src/palette.rs`, pure `Rgb`).
- `terminal-view` builds `TerminalTheme`/`TerminalPalette` from `cx.theme()` (`crates/terminal-view/src/theme/`).
- `oneterm_terminal::palette::resolve_color(&Color, &TerminalPalette) -> Rgb` (named/indexed/truecolor).
- `ensure_minimum_contrast(fg: Hsla, bg: Hsla, min: f32) -> Hsla` (`crates/terminal-view/src/theme/contrast.rs`, cached).

---

## 9. `TerminalSession` trait (`terminal` crate)

`TerminalSession` (`crates/terminal/src/session.rs`) is a composed façade over four
focused traits (ARCH-02). The UI holds `Box<dyn TerminalSession>`; a backend
implements all four plus an empty `impl TerminalSession for X {}` (overriding
`capabilities()` when it has optional services). Every trait method is **required** —
a backend that cannot answer returns the documented empty value explicitly
(`Vec::new()`, `None`, `DynamicColors::default()`); there are no silent no-op defaults.

```rust
/// Grid reads — snapshots, damage-free queries, search, selection, colour table.
pub trait TerminalRender: Send + Sync {
    fn snapshot(&self) -> TerminalContent;            // consumes damage; render path only
    fn snapshot_into(&self, out: &mut TerminalContent); // defaulted; reuses out's buffers
    fn query_state(&self) -> TerminalQueryState;      // O(1): mode, cursor, viewport size
    fn query_line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells;
    fn terminal_info(&self) -> TerminalInfo;
    fn is_alt_screen(&self) -> bool;
    fn is_mouse_mode(&self) -> bool;                  // any MOUSE_MODE bit: clicks belong to the program
    fn dynamic_colors(&self) -> DynamicColors;        // OSC 10/11/12 + OSC 4
    fn set_default_colors(&self, fg: Rgb, bg: Rgb, cursor: Rgb, ansi: [Rgb; 16]);
    fn search(&self, query: &str, options: SearchOptions) -> Vec<SearchMatch>;
    fn selection_text(&self) -> Option<String>;
    fn has_selection(&self) -> bool;                  // O(1), no text materialised (PERF-14)
}

/// Bytes into the PTY/channel plus viewport, mouse and selection manipulation.
pub trait TerminalInput: Send + Sync {
    fn write(&self, bytes: &[u8]) -> Result<(), TerminalError>;
    fn flush_pty(&self);
    fn send_ctrl_c(&self);
    fn resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError>;
    fn scroll(&self, delta: i32); fn scroll_to_bottom(&self); fn scroll_to_top(&self);
    fn mouse_down(&self, row: f32, col: f32, button: TerminalMouseButton, sel: SelectionType, mods: MouseModifiers);
    fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers);
    fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers);
    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton, mods: MouseModifiers);
    fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: MouseModifiers);
    fn clear_selection(&self); fn select_all(&self); fn clear(&self);
}

/// IME composition state and commit.
pub trait TerminalIme: Send + Sync {
    fn set_marked_text(&self, text: String);
    fn clear_marked_text(&self);
    fn commit_text(&self, text: &str);
    fn marked_text(&self) -> Option<String>;
}

/// Events, liveness, close, identity.
pub trait TerminalLifecycle: Send + Sync {
    fn take_events(&self) -> Option<Receiver<SessionEvent>>; // once-only; None after the first call
    fn alive(&self) -> bool;
    fn close(&self) -> Result<(), TerminalError>;
    fn kind(&self) -> SessionKind;                            // Local | Ssh
    fn title(&self) -> Option<String>;                        // OSC 0/2
    fn cwd(&self) -> Option<PathBuf>;                         // OSC 7
}

pub trait TerminalSession: TerminalRender + TerminalInput + TerminalIme + TerminalLifecycle + 'static {
    fn capabilities(&self) -> TerminalCapabilities { TerminalCapabilities::default() }
    // Provided helpers built only on the traits above:
    fn send_text(&self, text: &str);
    fn is_bracketed_paste(&self) -> bool;
    fn paste(&self, text: &str) -> Result<(), PasteError>;   // TooLarge | Write(TerminalError)
}
```

> This is only a **render/input/lifecycle interface** — it does not force a shared pump/transport.
> `LocalSession` and `SshSession` differ only in `capabilities()`, `kind()`, their grow-resize
> `ResizePolicy` and how the channel is torn down, so that is all each one still implements —
> the `PtyOwner` trait (`session.rs`). The four trait impls live once, on the concrete
> `PtySession<O: PtyOwner>` in the same file, which owns the engine handle, the state cache, the
> IME compose buffer and the single event receiver; `LocalSession::spawn` and `ssh::connect`
> return one. Until `US-0091` this was a 286-line `impl_pty_terminal_session!` macro expanded
> into both backend crates. The two backends cannot drift, and neither knows the other.

Presentation is not part of the trait: the status-bar breadcrumb is formatted by
`terminal-view` from `cwd()`. Pixel cell metrics (`set_cell_size`/`cursor_bounds`) were
removed — the IME caret position is computed by the element from its own layout.

`TerminalSession::capabilities()` returns optional backend services as one scoped
`TerminalCapabilities` value. SSH supplies network counters, SFTP, and its live CWD
source; local sessions use the default empty value. This keeps optional features out
of the required implementation surface for test fakes and future backends. The app
installs the session factory and workspace callbacks together through `AppServices`;
feature crates read those handles from their GPUI application context.

`paste` returns `Result<(), PasteError>`: a payload over the paste policy limit or an
undeliverable write is returned to the view, which shows a warning notification
(ERR-04) instead of dropping the paste silently.

`search` copies the grid text under the engine lock
(`oneterm_vt::search::GridText::from_terminal`, keyed by `RowId`) and matches after
releasing it (`search_grid_text`), so a long scrollback search never stalls the pump
(PERF-04). The signed grid line a `SearchMatch` publishes is derived from the copy's
`RowId`s once, where the match is produced. The matcher itself is the engine's since
`US-0100`; the adapter passes `SearchPattern::Literal` and `crates/terminal` does not
enable the engine's optional `regex` feature, so OneTerm's search stays literal.

`SessionEvent`: `Output | Title | Cwd | Clipboard | ClipboardRead | ShellIntegration |
Notification | Progress | AgentStatus | Exited(Option<i32>) | Closed |
Bell` — `Output` is the only coalescible event (§6.5).

### 9.1 Session duplication metadata and cwd

Each terminal view retains a non-secret launch descriptor so the terminal context menu can duplicate the terminal in the right-clicked Space without importing a backend crate:

- Local descriptors contain the complete `LocalShellConfig` used to spawn the source session.
- SSH descriptors contain host, port, username, authentication preference, optional private-key path, and shell-integration preference. They never contain a password or private-key passphrase.

At invocation time, duplication reads `TerminalSession::cwd()`. Local duplication clones its descriptor, sets `LocalShellConfig::cwd` to the live value, and spawns a new sibling tab through `SessionFactory`; when the live value is absent, it clears `cwd` so the shell/backend chooses its normal default directory. SSH duplication crosses the app-composed workspace command boundary to `session-ui`, opens a prefilled authentication dialog with empty secret fields and one-shot initial focus on password/passphrase, reconnects through `SessionFactory`, and requests the selected cwd in the new remote login shell only when known. It does not use OneTerm's process cwd as a fallback. The source process/session and terminal contents are not cloned or modified.

See [`decisions/0002-ssh-duplicate-auth.md`](decisions/0002-ssh-duplicate-auth.md) for the accepted credential-lifetime decision and [`terminal-split/04-context-menu.md`](terminal-split/04-context-menu.md) for menu behavior.

---

## 10. Input: keystroke → byte + IME

Four input paths:

1. **Raw keystroke** (`on_key_down` / `on_key_up` on the wrapper div): `map_key` builds an
   `oneterm_vt::input::KeyEvent` → `encode_key_event` → `session.write(bytes)`. Mapping:
   Ctrl+char → `& 0x1f`, F-key / arrow → ANSI escape, Enter → `\r`, Backspace → `0x7f`,
   Tab → `\t` / `\x1b[Z`…
   The event's `kind` is `Repeat` when GPUI reports the key held (`KeyDownEvent::is_held` —
   the OS's own auto-repeat; nothing in the view times, counts or synthesises one), `Press`
   otherwise, and `Release` on the key-up path. A release produces bytes only once the program
   pushed the kitty `REPORT_EVENT_TYPES` flag; otherwise the encoder answers `None` and nothing
   is written, which is why the byte stream is unchanged for a program that negotiated nothing.
   `TerminalView::held_keys` records the keys whose press actually reached the PTY: a release is
   sent only for a member, so a chord the view swallowed (zoom, copy, the completion overlay, a
   printable key the IME owns) never produces a release the program saw no press for, and a
   key-up with no key-down writes nothing. Entries are the **unshifted** `KeySpec`, not the
   platform's key name: Windows renames a digit or an OEM punctuation key to its shifted glyph
   while Shift is down, so `Shift+1` presses as `!` and, if Shift is lifted first, releases as
   `1`, and only the unshifted form pairs the two. The set is drained on blur — one release per
   key — so a window the user left cannot strand a held key; **that drain is proved only by the
   manual Windows walk**, because a GPUI test window is never active and its `on_blur`
   subscription therefore cannot fire. The key-up path does not run `classify_key` (the table is
   full of view-side shortcuts) and does not stop propagation, and a release is never repeated
   onto a broadcast channel's peers.
   `Ctrl+C` follows the same fan-out rule: once the program negotiated a kitty flag that puts a
   ctrl chord on the `CSI u` rung (`DISAMBIGUATE_ESC_CODES` or `REPORT_ALL_KEYS_AS_ESC`) this
   pane's terminal receives the encoded key rather than `SIGINT` — the specification promises it
   bytes — but the channel's peers always receive an interrupt, which is the one form a peer that
   negotiated nothing still understands.
   `KeyEvent::shifted` and `base_layout` stay `None`: a GPUI `Keystroke` carries neither, so
   `REPORT_ALTERNATE_KEYS` is inert for this embedder (`US-0108`).
2. **GPUI action** (Ctrl-Shift-C/V copy/paste, Ctrl-Tab…): map → `try_keystroke` or
   clipboard.
3. **IME**: keystroke not mapped → yield to GPUI IME → `EntityInputHandler` calls back
   `replace_text_in_range(text)` → `session.commit_text(text)`. Pre-edit:
   `replace_and_mark_text_in_range` → `session.set_marked_text` → paint marked text at
   the cursor with an underline.
4. **Paste**: `session.commit_text(text)` (bracketed paste if `TermMode::BRACKETED_PASTE`).

IME impl (`terminal-view`):
- `TerminalView` (`crates/terminal-view/src/terminal_view/ime.rs`) impl `gpui::EntityInputHandler`:
  `selected_text_range`, `marked_text_range`, `replace_text_in_range`,
  `replace_and_mark_text_in_range`, `unmark_text`, `bounds_for_range`,
  `text_for_range`, `character_index_for_point`.
- `ImeState { marked_text: String }` kept on the View.
- In `paint`: `window.handle_input(&ElementInputHandler::new(view_handle))`.
- Paint marked text: shape separately, paint at `ime_cursor_bounds` + underline.

---

## 11. File layout (current)

```
crates/
├── core/src/
│   ├── ssh_config.rs         # SshConfig + SshAuthMethod
│   ├── session_duplicate.rs  # SessionDuplicateConfig (non-secret launch descriptor, §9.1)
│   └── config/shell.rs       # ShellKind, LocalShellConfig, resolve_shell
│
├── terminal/src/             # the engine adapter (no GPUI)
│   ├── session.rs            # the four traits + PtyOwner + PtySession + TerminalSession façade, SessionEvent, TerminalCapabilities
│   ├── handle.rs             # TerminalHandle: the FairMutex + the render-demand flag
│   ├── model.rs              # TerminalModel: snapshot / snapshot_into / query_state / input
│   ├── content.rs            # TerminalContent: the SnapshotState it owns, in the engine's own vocabulary
│   ├── palette.rs / color_classification.rs / osc_color.rs
│   ├── paste.rs                  # (key + mouse encoding live in oneterm_vt::input)
│   ├── osc.rs / osc_agent/ / url_policy.rs / security_policy.rs
│   ├── factory.rs            # PtySize + SessionFactory
│   └── backend/              # shared pump layer (§5.3)
│       ├── transport.rs      # PtyTransport trait
│       ├── byte_budget.rs    # ByteBudget<LIMIT>: the shared write reservation (§6.5)
│       ├── state.rs          # SharedState (title/cwd/clipboard/counters)
│       ├── event_sink.rs     # SessionEventSink (coalescible hint / reliable send)
│       ├── osc_router.rs     # OscRouter<T>: the EventBatch drain
│       ├── pump.rs           # TerminalPump<T>
│       └── backend_tests.rs  # in-memory transport tests
│
├── local-shell/src/
│   ├── lib.rs                # pub: LocalSession
│   ├── session.rs            # LocalSession: tty + ShellEventLoop; spawn() -> PtySession
│   ├── session_terminal.rs   # impl PtyOwner (teardown + capabilities)
│   ├── event_loop.rs         # ShellEventLoop<P>, ShellNotifier (+ event_loop_tests.rs)
│   └── transport.rs          # LocalTransport: PtyTransport; LocalListener alias
│
├── ssh/src/
│   ├── lib.rs                # pub: SshSession, connect
│   ├── session.rs            # connect(): russh + shared tokio runtime -> PtySession
│   ├── session_terminal.rs   # impl PtyOwner (teardown + capabilities)
│   ├── task.rs               # ssh_main_task: channel ↔ TerminalPump
│   ├── transport.rs          # SshTransport: PtyTransport; SshListener alias
│   ├── handler.rs            # host-key policy (known_hosts)
│   ├── counting_stream.rs    # rx/tx byte counters
│   └── sftp.rs / sftp_task.rs / sftp_task/   # SftpSession + tokio task + transfers
│
└── terminal-view/src/        # feature crate (GPUI)
    ├── panel/                # TerminalPanel + PanelSpec (dock tab, Space tree)
    ├── space/                # SpaceTree (split/close/fill) + its rendering
    ├── terminal_view/        # TerminalView (Render, IME, keys, search, scrollbar, completion)
    ├── render/               # frame, metrics, glyphs, row plans, shapes, TerminalElement
    ├── input/                # key/mouse/menu/edit decisions
    └── theme/ · url/ · highlight/ · completion/
```

---

## 12. Implementation order (roadmap)

> **Status:** steps 1–7 are complete; step 8 is partial (known_hosts done, agent
> auth and reconnect not implemented); step 9 is ongoing.

1. ✅ **`core`**: `TerminalSession` trait, `SessionEvent`, `TerminalContent`, `TerminalPalette`,
   `key_encode`, `mouse_encode`, `osc`/`url`, `ShellKind`/`LocalShellConfig` + `resolve_shell`.
2. ✅ **`local`** (Windows-first): `LocalSession` spawns `cmd` (ConPTY, `chcp 65001`),
   `LocalListener`, snapshot + event. E2E test: `echo oneterm_e2e` → snapshot contains the string.
3. ✅ **`ui`**: `TerminalElement` paints grid + cursor + font measure + resize-on-layout.
   `TerminalView` (`Render`) wired into DockArea. Settings shell picker (Settings window ▸ Terminal).
4. ✅ **`ui`**: mouse (down/move/up/wheel), selection (Simple/Semantic/Lines/Block),
   scrollback, hyperlink OSC 8 (Ctrl+click), copy/paste (select-to-copy, middle-click,
   Ctrl+Shift+C/V, OSC 52 clipboard), minimum-contrast.
5. ✅ **`ui`**: IME (`EntityInputHandler` + marked text, `handle_input` in paint,
   alt-screen → disable IME, `bounds_for_range` = cursor bounds).
6. ✅ **`local`**: `powershell`/`pwsh`/`bash`/`zsh`/`sh`/`custom`, child exit detection,
   resize, 10k-line scrollback.
7. ✅ **`ssh`**: `SshSession` password + key, pty-req + shell + window_change + exit.
8. 🟡 **`ssh`**: known_hosts ✅, agent ⬜, reconnect ⬜.
9. 🟡 Perf tuning (batch, snapshot diff, debounce notify).

---

## 13. Risks

| Risk | Mitigation |
|---|---|
| Holding `FairMutex` in paint → jitter | Snapshot pattern (§5.2): short lock to copy, paint from the copy. |
| Tokio (ssh) vs smol (gpui) runtime conflict | Hidden shared tokio runtime inside `ssh` (2 workers), sync API, bridge via `async_channel`. |
| Windows cmd codepage not UTF-8 | `chcp 65001` (cmd), env `LANG` (pwsh). Document requires Win10 1903+ for good ConPTY. |
| `yes` spam → continuous redraw | Snapshot diff + debounce notify (§6.4). |
| Channel backpressure | 256 command messages plus a 4 MiB write budget; latest-value resize, priority close, coalescible repaint hints, and reliable stateful events (§6.5). |
| IME differs on Windows/Linux | Use GPUI's `EntityInputHandler` (ready abstraction), test both platforms. |
| SSH host key not verified | Require known_hosts + accept prompt, don't disable by default. |

---

## 14. Quick reference

| Need | Read |
|---|---|
| Render engine + input (current design) | [`docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md`](spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md) |
| `Element`/`paint_quad`/`shape_line` | `reference/gpui-kit` (tag `v0.6.0`) |
| `EntityInputHandler` | `gpui::EntityInputHandler` trait (docs.rs matching rev) |
| VT engine API | `crates/vt/src/` (`terminal/`, `grid/`, `render/`) + the IN-0029 low-level designs |