# 02 — Correctness & concurrency

> Part of the [2026-08 refresh review](README.md). Checklist format.

## Assessment

Most runtime paths avoid `unwrap`, the split-pane tree and completion controller are pure and
tested, and persistence uses locked atomic writes. The defects below cluster in three places:
(1) the **PTY pump ↔ UI thread** locking discipline, (2) **lifecycle/teardown** of sessions and
panels, and (3) **hand-copied state** (settings mirrors, coalescing drain, cached listings) that
drifted from its source of truth.

---

## A. Deadlocks, races, lifecycle

- [x] **[Critical] CORR-01 — Reliable events are `send_blocking`'d while the `Term` lock is held; the UI
  thread blocks on the same lock.**
  `crates/ssh/src/listener.rs:270-278` and `crates/local-shell/src/listener.rs:180-190` are invoked from
  `Term` callbacks (`Bell`, `Title`, `Cwd`, `Progress`, `ShellIntegration`, `AgentStatus`) during
  `processor.advance` with the lock held (`ssh/src/task.rs:73-74`, `local-shell/src/event_loop.rs:400`).
  The UI drains the 4096-slot channel on the main thread and, for `Output`, calls `terminal_info()`
  (Term lock) — `crates/terminal-view/src/view/local_view.rs:196-206`; render `snapshot()` also locks.
  Sequence: batch N queues `Output`; batch N+1 emits > 4096 `Bell`s → pump blocks under lock → main
  thread pops `Output` → blocks on Term → app hangs. Reproducer: `printf '\a%.0s' {1..5000}` locally or
  from a remote host (remote DoS). On SSH it also parks one of the two runtime workers, stalling every
  other SSH session.
  *Fix:* never block under the Term lock. Buffer reliable events in a listener-side
  `Mutex<VecDeque<SessionEvent>>` and flush after the parse batch outside the lock (event loop:
  `send_blocking` there is fine; tokio task: `send().await`), and/or make `Bell`/`Title`/`Progress`
  latest-value coalescible. Add a regression test that holds the Term lock, saturates the queue, and
  asserts the pump does not block. (Best done together with ARCH-01.)

- [x] **[High] CORR-02 — `Exited`/`Closed` swallowed when they arrive behind an `Output` batch.**
  `crates/terminal-view/src/view/local_view.rs:468-528` `drain_coalesced_events` handles Clipboard / Bell /
  Notification / Title / Progress / AgentStatus, but `Exited`/`Closed` fall into the generic arm, so
  `mark_agent_ended` (main loop `:255-266`) is skipped whenever a process prints and exits in the same
  batch — the normal case. *Fix:* one shared `handle_event` (ARCH-22) or add the two arms + regression test.

- [x] **[High] CORR-03 — Tab drag-drop can destroy the user's terminal.**
  `crates/terminal-view/src/panel/ops.rs:420-437` `handle_tab_drop` *takes* the terminal out of the source
  tree, then on `fill_empty` `Err` calls `view.shutdown(cx)` — killing the live session because the target
  Space stopped being empty. *Fix:* on `Err(view)` put it back (`src.update(|sp| sp.tree.fill_empty(orig_id, view))`)
  or check `self.tree.leaf_terminal(target).is_none()` before taking.

- [x] **[High] CORR-04 — On-close docks.json save races process exit.**
  `crates/workspace/src/layout/workspace/mod.rs:57-61` spawns a *detached* background task, then
  `crates/app/src/window.rs:66-69` calls `cx.quit()`. gpui's shutdown only awaits `on_app_quit` observers
  (200 ms); detached tasks are not awaited. The workspace's own `on_app_quit` (`mod.rs:224-242`) is
  entity-bound and a no-op once the window entity is dropped, so the last layout can be lost on normal close.
  *Fix:* write synchronously on the close path, or register an App-level `cx.on_app_quit` in `window.rs`
  returning a future that awaits the write.

- [x] **[High] CORR-05 — Local exit with `Exited(None)` leaves the session "alive" forever.**
  `crates/local-shell/src/event_loop.rs:330-344` only clears `alive` and forwards `Exited` when
  `status.is_some()`; alacritty's Windows watcher yields `Exited(None)` when `GetExitCodeProcess` fails or the
  watcher disconnects (`vendor/alacritty_terminal/src/tty/windows/child.rs:43-44`). The thread returns; the
  tab shows a live-but-dead PTY. The local backend also never emits `SessionEvent::Closed`.
  *Fix:* set `alive=false` and forward `Exited(code)` (code may be `None`) unconditionally, then `Closed`.

- [x] **[High] CORR-06 — `SshSession` has no `Drop`; the task owns senders of its own command channel.**
  `crates/ssh/src/task.rs:110-116` receives `term` (whose `SshListener` owns a `cmd_tx`) and `listener`
  (another `cmd_tx`), so `cmd_rx.recv()` at `task.rs:291-294` can never return `Err` while the task runs;
  `crates/ssh/src/session.rs:62-75` has no `Drop`. A real drop-without-close path exists:
  `crates/session-ui/src/common.rs:232-235` discards an `Ok(session)` when cancellation raced with success →
  leaked connection, Term (scrollback) and task.
  *Fix:* `impl Drop for SshSession { fn drop(&mut self) { let _ = self.listener.pty_close(); /* + sftp */ } }`,
  and treat the closing flag as the sole shutdown signal.

- [x] **[High] CORR-09 — SFTP `load_dir` has no request generation / backend guard.**
  `crates/sftp-ui/src/panel_ops.rs:43-93`: any in-flight `read_dir` result is applied when it arrives, so fast
  navigation, auto-follow + user navigation, or a tab switch during load produces the *earlier* listing (and
  even rewrites `this.cwd` from the stale result's first entry, `:64-71`). Same for `goto_path`
  (`panel.rs:548-576`). *Fix:* `load_generation: u64` + `active_key` captured by the task; discard mismatches.

- [ ] **[Medium] CORR-10 — `close()`/`Drop` join the PTY owner thread on the UI thread.**
  `crates/local-shell/src/session.rs:149-172`, `session_terminal.rs:234-238`. Blocks the UI for the loop's
  current iteration and can deadlock with CORR-01. *Fix:* send `Shutdown` and detach; join on a background
  thread or with a bounded timeout.

- [x] **[Medium] CORR-11 — `Cmd::Close` path exits without `SessionEvent::Closed`.**
  `crates/ssh/src/task.rs:282-290` breaks silently while the closing-flag path (`124-133`) forwards `Closed`;
  which one runs is a race. *Fix:* single teardown block after the loop.

- [ ] **[Medium] CORR-13 — Metadata ops block the SFTP command loop.** `crates/ssh/src/sftp_task.rs:84-129`
  awaits `ReadDir/Stat/Rename/Remove/Mkdir` inline, so a slow `read_dir` (up to the 10 s request timeout)
  delays `Cancel`/`Close`. *Fix:* spawn every command into the `JoinSet` (optionally behind a small semaphore).

- [x] **[Medium] CORR-14 — Concurrent `SshSessionStore::save()` tasks can complete out of order.**
  `crates/session-ui/src/session_state.rs:201-208` spawns a detached background write per mutation
  (last-completed-writer wins). *Fix:* single-flight queue (like `settings-ui/src/updates/config.rs`) or
  `update_json_file`.

- [x] **[Medium] CORR-15 — Update-check completion overwrites concurrent user preference edits.**
  `crates/settings-ui/src/updates/actions.rs:56-59,118-121` do `*config = next_config` with the pre-check
  config the manager mutated on the background thread; edits made during the check are lost in memory and on
  disk. *Fix:* merge only cache fields back into the entity, or persist via `update_json_file`.

- [x] **[Medium] CORR-16 — Runtime docks.json corruption disables all layout saves until restart.**
  `crates/state/src/dock_persistence.rs:117-128` parses the existing file inside the transaction; on
  `InvalidData` every `save_state` (`workspace/persistence.rs:99-103`) fails and only logs; quarantine happens
  only in `load_layout` at startup. *Fix:* quarantine and continue from `DockDocument::default()` with a
  recovery log.

- [ ] **[Low] CORR-17 — Blocking I/O on tokio workers:** `check_known_hosts`/`learn_known_hosts` inside async
  `check_server_key` (`crates/ssh/src/handler.rs:265-270`), `load_secret_key` (`session.rs:240-243`).
  *Fix:* `spawn_blocking`.

- [ ] **[Low] CORR-18 — Polling instead of notification:** `wait_for_cancellation` sleeps 25 ms
  (`ssh/src/session.rs:102-106`); `send_local_upload_entry` spins with `thread::sleep(1ms)`
  (`sftp_task/transfer/upload.rs:263-277`); event loop uses a 50 ms poll timeout despite `poller.notify()`
  (`event_loop.rs:253-255`), waking every idle tab 20×/s. *Fix:* `Notify`/`CancellationToken`;
  `send_blocking` in the walker; `None` timeout in the poller.

- [ ] **[Low] CORR-19 — `closing` flag uses `Relaxed`** (`ssh/src/listener.rs:198,237`). *Fix:* Release/Acquire.

- [ ] **[Low] CORR-20 — `ChannelMsg::ExitSignal` unhandled** (`ssh/src/task.rs:264-266`). *Fix:* map to `Exited(None)`.

- [ ] **[Low] CORR-21 — Double Term resize on SSH.** UI thread resizes (`ssh/src/session_terminal.rs:112-118`)
  and the task resizes again (`task.rs:139-142`). *Fix:* remove the second.

- [ ] **[Low] CORR-22 — `subscribed_tabs` grows unbounded.** `workspace/.../mod.rs:133,344`: EntityIds of
  dropped `TabPanel`s are never removed. *Fix:* retain only ids present in `collect_tab_panels`.

## B. Logic errors & panics

- [x] **[High] CORR-07 — Unicode-boundary panics still reachable in completion (3 sites).**
  `crates/completion/src/engine.rs:73,75` (`&self.text[..typed.len()]` in `remainder`), `:83`
  (`is_prefix_of_typed`), and `crates/completion/src/engine/scoring.rs:56` (`flag[..token.len()]`).
  Only `history::prefix_match` was fixed in 2d63267. Path: Cmd/PowerShell family, history first-token `日x`,
  typed `x` → fuzzy suggestion `日x`; on accept `controller.rs:285` → `is_prefix_of_typed("x")` slices at
  byte 1 inside a 3-byte char → panic. *Fix:* `self.text.get(..typed.len())` / `flag.get(..token.len())`
  returning `false`/`None`; regression tests for all three; delete `remainder` (no non-test callers).

- [x] **[High] CORR-08 — Linux install moves *every* file in `current_exe.parent()` into a backup dir.**
  `crates/update/src/install.rs:194-212` `replace_directory_contents`. If the binary lives in `~/.local/bin`
  or `/usr/local/bin`, all sibling tools are relocated to `.bin.backup-<pid>-<ts>` and never restored on
  success. *Fix:* restrict to files present in the package; remove the backup after successful launch.

- [x] **[Medium] CORR-12 — Zoomed font size persisted as the configured size.**
  `crates/settings/src/terminal_settings/persist.rs:65` writes `size: self.font_size` (live, zoom-modified;
  see `settings.rs:523-528`, `mutators.rs:150-165`, `terminal-view/src/handlers/keyboard.rs:93-114`). Any
  subsequent `persist_global` writes the zoomed size into `terminal.json`, and on next launch it becomes the
  base. *Fix:* `size: self.base_font_size` + roundtrip test (ARCH-16).

- [x] **[Medium] CORR-23 — Panic on non-ASCII 6-byte colour strings.**
  `crates/settings/src/terminal_settings/color.rs:12-17` checks `s.len() != 6` (bytes) then slices
  `&s[0..2]`; reachable at startup from a user-edited `terminal.json` (`apply.rs:34,86-99`).
  *Fix:* `if !s.is_ascii() || s.len() != 6 { return None }` or `s.get(0..2)?`.

- [ ] **[Medium] CORR-24 — Contrast cache key collides.** `crates/terminal-view/src/theme/contrast.rs:214-221`
  packs four 32-bit floats into a `u64` at 16-bit stride — bits overlap, distinct (fg,bg) pairs can collide
  and return the wrong adjusted colour; the thread-local map (`:205-212`) is unbounded.
  *Fix:* key on `([u32;4],[u32;4],u32)`; clear when `len() > 4096`.

- [x] **[Medium] CORR-25 — Legacy X10/X11 mouse encoding emits UTF-8, not raw bytes.**
  `crates/terminal/src/mouse_encode.rs:92-99`: `col_byte as char` into a `String` becomes 2 UTF-8 bytes for
  values ≥ 0x80 (col/row > 95) — that is 1005 encoding, but `TermMode::UTF8_MOUSE` is never checked; test
  `x11_coordinates_above_127` (`:294`) enshrines the wrong wire format. *Fix:* return `Vec<u8>`; raw byte
  unless `UTF8_MOUSE`.

- [x] **[Medium] CORR-26 — Ctrl-key encoding wrong for non-letters and ignores Alt.**
  `crates/terminal/src/key_encode.rs:96`: `byte & 0x1f` maps Ctrl+2..7/0/1/9 to garbage instead of
  NUL/ESC/FS/GS/RS/US; Ctrl+`?` → 0x1f instead of 0x7f; Ctrl+Alt+x drops the ESC prefix; `encode_key`
  never returns `None` despite its `Option` contract (l.90). Related: `terminal-view/src/handlers/keyboard.rs:233-242`
  routes Ctrl+Alt (AltGr on Windows) through the same path, so `@ { [ € ~` on non-US layouts risk being sent
  as control bytes and `stop_propagation` at `:283` prevents the WM_CHAR fallback.
  *Fix:* explicit table for `@[\]^_?` and digits per xterm; prefix `0x1b` when `alt`; if
  `mods.control && mods.alt && key_char.is_some()` treat as plain text. Verify on a DE/FR layout.

- [x] **[Medium] CORR-27 — Zsh PS1 contains stray backslashes.** `crates/core/src/config/shell.rs:294`:
  in a normal Rust string `\\\\` is two literal backslashes; the terminal receives `ESC ] 133;A ESC \ \` —
  the second `\` prints and `%{…%}` width is off. *Fix:* `"%{\x1b]133;A\x1b\\%}…"` + exact-bytes test.

- [x] **[Medium] CORR-28 — Zsh/Bash/Sh kind ignores the requested shell when `$SHELL` differs.**
  `crates/core/src/config/shell.rs:245-249`: `ShellKind::Zsh` with `program: None` resolves to `$SHELL`
  (often bash) but still injects the zsh PS1. *Fix:* `find_in_path(name)` first, then `/bin/{name}`, and only
  use `$SHELL` when it matches.

- [x] **[Medium] CORR-29 — Non-bracketed paste keeps LF.** `crates/terminal/src/paste.rs:65-67`: in
  `PasteMode::Plain` multi-line text is written verbatim; raw-mode apps and cmd/ConPTY expect `\r`
  (alacritty rewrites). *Fix:* rewrite `\r\n`/`\n` → `\r` in Plain mode + test.

- [ ] **[Medium] CORR-30 — Index-based SFTP selection survives re-sort.**
  `crates/sftp-ui/src/table_delegate.rs:328-353` `perform_sort` resorts `entries` but neither remaps nor
  clears `SftpPanel::selected`; toolbar Delete/Rename then operate on whatever now sits at that index.
  *Fix:* clear or remap by path in `perform_sort`.

- [x] **[Medium] CORR-31 — One cancel aborts the whole SFTP batch silently; `Cancelled` can leave items
  `InProgress`.** `crates/sftp-ui/src/transfer.rs:110` returns after a `-1.0` sentinel or `AppError::Cancelled`
  (`:153-155`, `:447-450`) skipping remaining files without marking them. *Fix:* continue with the next file
  (or mark the rest `Cancelled`); set `Cancelled` before returning.

- [ ] **[Medium] CORR-32 — Silent port fallback in three places.** `session-ui/src/common.rs:146`,
  `quick_connect_dialog.rs:224`, `session_dialog.rs:190` all `parse().unwrap_or(22)`; typing `2222x`
  silently connects to port 22. *Fix:* `parse_port(&str) -> Result<u16, String>` + notification.

- [ ] **[Medium] CORR-33 — SSH sessions ignore the user's scrollback setting.** `session-ui/src/common.rs:229`
  passes `10_000` to `SessionFactory::connect_ssh` while local shells use `TerminalSettings.scrollback_history`
  (`terminal-view/src/panel/ops.rs:101`). *Fix:* read the setting (add `oneterm-settings` dep, allowed L1).

- [ ] **[Medium] CORR-34 — Delete session has no confirmation** (`session-ui/src/panel.rs:146-165`), reachable
  from a rebindable key. *Fix:* confirm dialog (sftp delete already has one).

- [ ] **[Medium] CORR-35 — Key-binding row descriptions are applied to the group, not the row.**
  `crates/settings-ui/src/key_bindings/key_bindings_ui.rs:59-64`: `.item(...).description(...)` calls
  `SettingGroup::description`, so each group shows only the last action's "Default: …". *Fix:* move
  `.description(..)` onto the `SettingItem`.

- [ ] **[Medium] CORR-36 — Unbounded `Box::leak` on every render.** `key_bindings_ui.rs:74-82` leaks a `String`
  per action each time `page()` is built; `SettingsPanel::pages` rebuilds every render (`panel.rs:54-62`).
  `SettingItem::description` takes `impl Into<Text>` (vendored `setting/item.rs:142`), so the `&'static str`
  justification is stale. *Fix:* `SharedString`.

- [ ] **[Medium] CORR-37 — Cached update candidate ignores channel.** `crates/update/src/manager.rs:229-242`
  restores `cached_candidate` on `304` without checking `prerelease` vs current `channel`. *Fix:* store
  `prerelease` in `CachedUpdateCandidate` and filter.

- [ ] **[Medium] CORR-38 — Windows updater helper writes the backup to the *parent* of the install dir**
  (`crates/update/src/install.rs:61-64` → `%INSTALL%\..\.oneterm-backup-*`); `is_writable_dir` (`:245`) only
  probes the install dir; if the parent is not writable the script exits 1 (`:320-322`) **after** the app
  already quit (`settings-ui/src/updates/install.rs:134`). Backups accumulate forever. *Fix:* backup inside
  `update_cache_dir()`, verify before quitting, delete after `start` succeeds.

- [ ] **[Medium] CORR-39 — macOS `Restarted` path launches with `open <bundle>` while the old instance runs**
  (`install.rs:109`) → activates the existing instance, then `cx.quit()` closes it. *Fix:* `open -n` or spawn
  the binary directly.

- [ ] **[Medium] CORR-40 — Update download total timeout is 60 s** (`crates/update/src/github.rs`
  `DOWNLOAD_TOTAL_TIMEOUT` via `RequestBuilder::timeout`, which spans the whole body in reqwest-blocking).
  A ~50 MB asset on a slow link fails deterministically. *Fix:* connect/read-idle timeout, not total.

- [ ] **[Medium] CORR-41 — Agent "Done" chip counts cards the filter can never show.**
  `crates/agent-ui/src/view.rs:183-195` `passes_filter` excludes `Lifecycle::Ended`, while `state == done`
  sets `Ended` (`docs/agent-panel-display.md:127`); the Done chip (`:290-296`) is non-zero while the Done
  filter is always empty. *Fix:* show ended cards under Done/All (dimmed) or count only non-ended.

- [ ] **[Medium] CORR-42 — Search bar `refresh_search` runs a full-scrollback search on every Output event**
  (`terminal-view/src/view/local_view.rs:207` → `search.rs:144-155`); `run_search`'s scroll-to-match is
  immediately undone by the unconditional `scroll_to_bottom()` at `:199`. *Fix:* mark dirty, refresh once per
  frame in `render`; do not force-scroll while `display_offset > 0` (see PERF/UX).

- [ ] **[Medium] CORR-43 — Block cursor occludes the glyph under it.** `terminal-view/src/element/paint.rs:334-338`
  paints a solid quad after the text runs; `HollowBlock` (`:335`) is filled; unfocused windows also draw a
  filled block (`:315`). *Fix:* paint the cursor quad before the cell's text run and re-paint that run with
  inverted fg (or `paint_quad` with border for Hollow/unfocused).

- [ ] **[Medium] CORR-44 — Host key of a *different algorithm* is reported as "unknown host" rather than
  "changed".** See SEC-05 (security file) — listed there.

- [ ] **[Low] CORR-45 — IPv6 host without port yields spurious `NonDefaultPort`.**
  `crates/terminal/src/url_policy.rs:196-212`: `https://[::1]/` → brackets stripped → `rfind(':')` → port `1`.
  *Fix:* only look for a port after `]` when the host starts with `[`.

- [ ] **[Low] CORR-46 — OSC 7 cwd parsing has no percent-decoding and mangles `file:///C:/…`.**
  `crates/terminal/src/osc.rs:179-187`. *Fix:* percent-decode; strip leading `/` when the remainder matches
  `[A-Za-z]:`.

- [ ] **[Low] CORR-47 — Quarantine name collides across runs.** `crates/core/src/persistence.rs:325-331`
  `.name.invalid-{seq}` uses a process-local counter and `fs::rename` overwrites. *Fix:* include pid +
  timestamp or use no-replace semantics.

- [ ] **[Low] CORR-48 — Prompt-fallback regexes are far too permissive.** `crates/highlight/src/profile.rs:241,258`,
  `scanner/prompt.rs:103`: `^[^\s]*[\$#%]` classifies `100%`, `#include`, `$HOME=…` as prompt lines (then
  "Command" mode suppresses keyword/structural highlighting); second alternation branch redundant.
  *Fix:* require the sign followed by EOL/space and preceded by a plausible `user@host:path`; negative tests.

- [ ] **[Low] CORR-49 — `-p` in the secret-flag vocabulary over-redacts** (`crates/completion/src/redact/detect.rs:175`):
  `mkdir -p dir`, `docker run -p 8080:80`, `ssh -p 22` lose the next token; attached `-pSECRET` is kept
  (`redact.rs:64`). *Fix:* per-command `-p` handling; attached-form detection.

- [ ] **[Low] CORR-50 — `select_next` underflow hazard.** `terminal-view/src/completion/controller.rs:258`
  `len() - 1` panics if `selected.is_some()` while `suggestions` is empty. *Fix:* `saturating_sub(1)`.

- [ ] **[Low] CORR-51 — `layout_row` skips a space cell after a zero-width cell before the blank check**
  (`terminal-view/src/layout/row.rs:43-46`), losing a non-default bg rect; intent undocumented.
  *Fix:* move after `is_blank`, document.

- [ ] **[Low] CORR-52 — Empty first SFTP listing never resolves the cwd.** `sftp-ui/src/panel_ops.rs:64-71`
  derives the absolute cwd from `entries.first()`; for an empty root the panel keeps `"."`. *Fix:* add
  `realpath` to `SftpBackend` (or `stat(".").path`).

- [ ] **[Low] CORR-53 — Rename/New-Folder accept `/` and `..` in the name** (`sftp-ui/src/actions.rs:96-107,327-339`).
  *Fix:* validate `!name.contains('/') && name != ".." && name != "."`.

- [ ] **[Low] CORR-54 — Quick Connect saves the session before the connection is attempted**
  (`session-ui/src/quick_connect_dialog.rs:245-262`). *Fix:* save on success.

- [ ] **[Low] CORR-55 — Rebinding has no conflict detection** (`settings-ui/src/key_bindings/key_bindings_ui.rs:200-223`).
  *Fix:* warn on duplicate keystroke.

- [ ] **[Low] CORR-56 — Capture mode relies on the settings window having no action handlers** (gpui dispatches
  key bindings before `on_key_down`, vendored `gpui/src/window.rs:4902-4917`). *Fix:* keystroke interceptor
  or document the invariant.

- [ ] **[Low] CORR-57 — Multiple Settings windows can be opened** (`settings-ui/src/window.rs:33-75`). *Fix:* dedupe.

- [ ] **[Low] CORR-58 — `check_interval_hours` overflow** (`update/src/config.rs` ~L120,
  `Duration::hours(... as i64)`). *Fix:* clamp.

- [ ] **[Low] CORR-59 — Windows updater script `timeout /T 1` fails when stdin is not a console**
  (helper spawned with `CREATE_NO_WINDOW`) → busy spin. *Fix:* `ping -n 2 127.0.0.1 >NUL` or `waitfor /t 1`.

- [ ] **[Low] CORR-60 — ANSI palette silently shifts on one bad entry.** `settings/src/terminal_settings/apply.rs:95-99`
  `filter_map(parse_hex_color)` drops invalid entries, so colour 3 becomes colour 2. *Fix:* `Option<Hsla>` per
  slot + log.

- [ ] **[Low] CORR-61 — Non-NotFound read errors select defaults, later overwritten.**
  `settings/src/ui_config.rs:129-132`, `terminal_config/document.rs:169-172`: e.g. `PermissionDenied` →
  defaults; next persist overwrites a possibly-valid file. *Fix:* "load failed" flag → refuse to persist.

- [ ] **[Low] CORR-62 — Whole crash-report load aborts on a single I/O error.**
  `app/src/crash_report.rs:195-197,202` (`delete_report(old_path)?`, `load_and_sanitize_report(&path)?`).
  *Fix:* log and continue per file.

- [ ] **[Low] CORR-63 — `expect` inside async window setup** (`app/src/window.rs:77`). *Fix:* `?` with context.

- [ ] **[Low] CORR-64 — `build_named_panel` fakes `PanelInfo::tabs(0)` for a leaf panel** (`workspace/.../mod.rs:89-93`).
  *Fix:* `PanelInfo::panel(Value::Null)`.

- [ ] **[Low] CORR-65 — `open_search` subscription is `.detach()`ed** (`terminal-view/src/search.rs:68-92`)
  rather than stored; correct only by accident. *Fix:* store the `Subscription`.

- [ ] **[Low] CORR-66 — Two stampers for `line_times`.** `render/view_render.rs:128-133` claims "single source"
  but the Output handler also calls `update_line_times` (`local_view.rs:203`). *Fix:* keep the Output stamp.

- [ ] **[Low] CORR-67 — `SshSessionStore` follow-cwd poll runs `purge_closed()` and ticks every 500 ms even
  when follow is disabled** (`sftp-ui/src/panel.rs:153-175`). *Fix:* stop the timer when not needed.

- [x] **[Low] CORR-68 — `apply_check_result` leaves a stale `candidate` on `Disabled`**
  (`settings-ui/src/updates/actions.rs:152`). *Fix:* clear.

- [ ] **[Low] CORR-69 — `ManualInstall` outcome only visible in About status text**
  (`settings-ui/src/updates/notify.rs:45`). *Fix:* Info notification with the package path.

- [ ] **[Low] CORR-70 — Ended agent cards are never pruned** (`AgentRegistry::clear_ended` at
  `state/src/agent_registry.rs:231` has no UI caller). *Fix:* "Clear ended" affordance or auto-clear.
