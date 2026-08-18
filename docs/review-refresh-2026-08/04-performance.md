# 04 — Performance

> Part of the [2026-08 refresh review](README.md). Checklist format.

## Assessment

The rendering core is well engineered (device-pixel-snapped grid, per-row layout cache with damage +
scroll rotation, primitive box drawing, coalesced bg/box runs, cached shaped lines). The problems are
that several per-frame steps **bypass** those caches — JSON re-parse of the highlight theme, gutter
re-shaping, three viewport scans under the `Term` lock — and that a few backend paths do avoidable
per-byte or per-request work (SFTP is RTT-bound at 32 KiB / one in flight). Widgets poll on fixed
timers regardless of visibility. Nothing here has been profiled by this review; items are ordered by
how obviously they sit on the hot path.

---

## A. Render hot path (terminal-view)

- [x] **[High] PERF-01 — `build_terminal_theme` re-parses JSON every frame.**
  `crates/terminal-view/src/render/view_render.rs:52` → `theme/terminal_theme.rs:79` →
  `highlight/bridge.rs:121-124` `serde_json::from_str` of the embedded default asset on every render of
  every terminal. *Fix:* `static DEFAULT_STYLES: LazyLock<ClassStyles>`; cache the whole `TerminalTheme`
  keyed by theme id / palette.

- [x] **[High] PERF-02 — Gutter re-shaped every frame.** `element/paint.rs:254-305` calls `shape_line` for
  every visible row each frame, allocating a `Vec<TextRun>` with 1–2 `font.clone()` per row (`:267-295`);
  `element/gutter.rs:334` also `format!`s every row's text every frame. Only the *width* is cached.
  *Fix:* cache `ShapedLine` per (line_number, time_str, font, size) alongside `RowLayout`.

- [x] **[High] PERF-03 — `terminal_info()` scans the whole viewport under the `Term` lock and is called 3× per
  frame.** `crates/terminal/src/model.rs:147-161` runs `last_content_line` (rows×cols) per call; the view
  calls it at `render/view_render.rs:103`, `:116`, `element/prepaint.rs:67`, and once per Output event
  (`local_view.rs:202`). *Fix:* call once per frame in `render`, thread it into `TerminalElement`; make
  `last_content_line` lazy or maintained incrementally by the listener.

- [ ] **[High] PERF-04 — Full-scrollback regex search on every Output event while searching**
  (`search.rs:150` via `local_view.rs:207`). With a 100k-line scrollback and a streaming build this is
  O(scrollback) per PTY read. *Fix:* `search_dirty = true` on output, refresh in `render` (once per frame).
  Also `crates/terminal/src/search.rs:97-116` is a naive O(lines×cols×needle) scan under the `Term` lock per
  keystroke — snapshot rows once, release the lock, then search (`memchr` for ASCII).

- [x] **[Medium] PERF-05 — Per-frame clones/allocs in `render`:** `settings.color_overrides.clone()`
  (`view_render.rs:92`), `cx.theme().clone()` (`:126`, `search.rs:247`), `settings.completion.clone()`
  (`view/completion.rs:200`), `self.font()` rebuilding `Arc<Vec<(String,u32)>>` (`view/font.rs:414-425`),
  `SemanticOverlay` clone (`:173`), `TerminalTheme` clone into the element (`:184`, includes
  `[Option<Rgb>;256]` + `ClassStyles`), `RenderStyleKey` with `font.clone()` + palette copy compared each
  frame (`prepaint.rs:157-164`, `cache.rs:69,195`). *Fix:* pass `&` where possible; cache `Font` on the view
  and rebuild only on settings change; compare a palette hash.

- [x] **[Medium] PERF-06 — Completion re-clones the whole grid on cursor move.**
  `view/completion.rs:250` `snapshot_query()` (O(rows×cols) under lock) on every frame where the cursor moved,
  plus `extract_cursor_command` walks all cells (`:115-128`); `completion_capture_current` (`:459`) again on
  Enter. *Fix:* `query_line_range_cells(cursor_line, 1)`.

- [ ] **[Medium] PERF-07 — Blink task notifies every 500 ms per terminal regardless of focus/setting.**
  `local_view.rs:270-292` toggles + `notify()` even when `cursor_blink == Off`, when unfocused, or when the
  tab is inactive; each notify triggers a full `render` → `snapshot()` grid clone. *Fix:* start/stop on
  focus change; skip when Off; snapshot only when damage non-empty or cursor visibility changed.

- [ ] **[Medium] PERF-08 — Row cache misses on every scrolled line.** On a normal linefeed alacritty marks full
  damage, so all rows re-layout + re-shape each output frame; `line_hash` (`cell/hash.rs`) is used only for
  the cursor row (`cache.rs:155-161`). *Fix:* on `Full` damage, hash each row and reuse the previous
  `RowLayout`/`ShapedLine`s from a hash→layout map (rows mostly shift by N).

- [x] **[Medium] PERF-09 — Shaping allocates `SharedString::from(run.text.clone())` per newly-laid-out run**
  (`prepaint.rs:198-199`); `BatchedTextRun.text` is a `String`. *Fix:* `SharedString`/`Arc<str>` in the run.

- [ ] **[Medium] PERF-10 — `url_masks_wrapped` allocates 2 `Vec` per line whenever any row is dirty**
  (`cache.rs:138-140`, `url/mask.rs:31-46`); `update_row_cache` allocates a `Vec<&IndexedCell>` per line
  (`cache.rs:148`) plus a `HashSet<usize>` for dirty rows (`:101-117`) each frame. *Fix:* scratch buffers in
  `RowLayoutCache`; bitset for dirtiness; mask only dirty rows.

- [x] **[Medium] PERF-11 — Output always yanks the viewport to the bottom.** `local_view.rs:199`
  `scroll_to_bottom()` on every Output event: users cannot read scrollback while a command streams
  (mainstream terminals keep the offset and re-snap on keyboard input, which `keyboard.rs` already does).
  *Fix:* remove it here; keep the key-press snap. (UX as much as perf.)

- [x] **[Low] PERF-12 — `line_times: VecDeque<String>` stores an 8-char heap `String` per scrollback line**
  (`local_view.rs:90`, `update_line_times:591-593`) — ~40 B/line, 4 MB at 100k lines per terminal.
  *Fix:* `u32` seconds-of-day, format at gutter build.

- [x] **[Low] PERF-13 — `cell_style` clones `Font` per non-blank cell; `can_append` deep-compares `Font` per
  cell** (`cell/style.rs:297-301`, `cell/batch.rs:220-226`). *Fix:* compare `(weight, style)`; build `Font`
  only when starting a run.

- [ ] **[Low] PERF-14 — `has_selection` for the context menu materialises the whole selection string**
  (`handlers/menu.rs:48-52`). *Fix:* `TerminalSession::has_selection()`.

- [x] **[Low] PERF-15 — URL hover query on every mouse move** (`handlers/mouse.rs:248` → `url.rs:37-44`, 11-line
  grid clone per move). *Fix:* skip when `row/col` unchanged.

- [x] **[Low] PERF-16 — `cursor_bounds()` clones the whole grid to read the cursor** (`crates/terminal/src/model.rs:374`,
  IME path). *Fix:* `renderable_content()` cursor + `display_offset()` under one lock.

- [x] **[Low] PERF-17 — Wheel takes three separate locks** (`model.rs:339-343`). *Fix:* one lock scope.

## B. Backends

- [x] **[Medium] PERF-18 — SFTP transfers are RTT-bound.** 32 KiB chunk, one request in flight
  (`crates/ssh/src/sftp_task/transfer/download.rs:87-99`, `upload.rs:202-221`); russh-sftp allows 256 KiB
  packets and concurrent requests. ≈ 0.6 MB/s at 50 ms RTT. *Fix:* pipeline N outstanding reads via
  `RawSftpSession::read` with offsets (or at least 256 KiB chunks); extract one `copy_with_progress`.

- [ ] **[Medium] PERF-19 — Absolute-line heuristic scans every PTY byte for `\n`** (`ssh/src/task.rs:170`,
  `local-shell/src/event_loop.rs:411-412`) — extra O(n) pass and inaccurate (counts `\n` in escape payloads /
  alt-screen). *Fix:* expose a scrolled-out-lines counter from the vendored fork's grid (already patched for
  OSC/clear).

- [x] **[Low] PERF-20 — Per-chunk `SharedState` locking 3–4× on the SSH hot path** (`task.rs:152,162,178,189`)
  and rx/tx counters under a `Mutex` in `poll_read` (`counting_stream.rs:49`). *Fix:* atomics; read defaults once.

- [x] **[Low] PERF-21 — 1 MiB read buffer on the owner-thread stack** (`event_loop.rs:218`; default 2 MiB stack).
  *Fix:* heap-allocate or raise the builder stack size.

- [x] **[Low] PERF-22 — Idle wake-ups.** Event loop 50 ms poll timeout (CORR-18); agent-ui 120 ms tick with no
  cards (`agent-ui/src/view.rs:132-164`); sftp follow-cwd 500 ms poll (`sftp-ui/src/panel.rs:153-175`).
  *Fix:* adaptive/paused timers.

## C. Engine crates

- [x] **[Medium] PERF-23 — Highlight scanner rebuilds the same string three times per line.**
  `crates/highlight/src/scanner/mod.rs:31` (`chars` Vec), `scanner/output.rs:284` (`String` + byte map),
  `scanner/structural.rs:15-18` (again); `try_ipv4` collects a `String` per octet (`structural.rs:90-94`).
  *Fix:* build `chars` + `byte_to_char` once in `scan_line`, pass `line: &str` through, offer
  `scan_line_into(&mut Vec<u8>)`.

- [x] **[Low] PERF-24 — `command_names` is O(n²)** (`crates/completion/src/catalog.rs:197-218`, per keystroke).
  *Fix:* precompute per-family deduped name list in `Catalog::from_raw`.

- [x] **[Low] PERF-25 — `sort_entries` allocates two lowercase strings per comparison** (`sftp-ui/src/types.rs:188`).
  *Fix:* `sort_by_cached_key`.

## D. Shell & widgets

- [x] **[Medium] PERF-26 — `System::new_all()` at every startup on the main thread.**
  `crates/app/src/crash_report.rs:137` enumerates all processes/CPUs/memory before the UI opens, even when
  there is no foreign `.native.tmp`. *Fix:* build lazily only when a foreign staging file exists, with
  `RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing())`.

- [x] **[Medium] PERF-27 — Persistence work on the UI thread at startup.**
  `crates/workspace/src/layout/workspace/persistence.rs:22,30` `quarantine_file` on the UI thread;
  `mod.rs:159,161,163` read/parse docks.json three times; `UiConfig::load`/`TerminalConfig::load`
  `atomic_write` defaults on the UI thread; `sftp-ui/src/table_delegate.rs:114-118 persist()` does a locked
  read-modify-write of docks.json on the UI thread and `:57` reads it synchronously in the constructor;
  `settings-ui/src/updates/config.rs:23` blocking read + default write. `docs/agents/persistence.md` forbids
  `atomic_write`/quarantine on the UI thread. *Fix:* read once, pass the document; schedule writes/quarantine
  on the background executor.

- [x] **[Low] PERF-28 — `ResourceIndicator` refresh on the UI thread every 2 s with a heavier-than-needed kind**
  (`workspace/src/widgets/resource.rs:386-387` `refresh_processes` default kind → Toolhelp32 snapshot of all
  processes on Windows). *Fix:* `refresh_processes_specifics(Some(&[pid]), true, ProcessRefreshKind::nothing().with_cpu().with_memory())`
  or sample in the background.

- [x] **[Low] PERF-29 — Polling widgets notify even when unchanged.** `net_speed.rs:209` `cx.notify()`
  unconditionally every second; `breadcrumb.rs:496-508` walks the dock tree every 500 ms. *Fix:*
  compare-before-notify; make breadcrumb event-driven.

- [x] **[Low] PERF-30 — Two `ui_config.json` writes per theme switch; menu tree built twice per Theme change**
  (`theme/src/theme.rs:94-95`, `workspace/src/layout/app_menus.rs:73-74`). *Fix:* build once, debounce persist.

- [x] **[Low] PERF-31 — `save_state_for_key` clones the whole SFTP entry vec on every dirty snapshot**
  (`sftp-ui/src/panel.rs:328-332`). *Fix:* `Arc<[FileEntry]>` in the delegate.
