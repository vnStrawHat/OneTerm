# 05 — Error handling (against `docs/agents/error-policy.md`)

> Part of the [2026-08 refresh review](README.md). Checklist format.

## Assessment

The policy itself is good and the persistence/backend boundaries mostly return typed `Result`s. Two
rules are systematically under-enforced: (1) **`let _ =` without a justification comment** — 209
occurrences in non-test code (terminal-view 60, ssh 51, local-shell 14, update 12, settings-ui 8,
core 8); (2) **user actions whose failure is log-only** (paste too large, spawn failure, drag-drop
without connection, file-picker errors, unregistered panel). Poisoned `std::sync::Mutex` on the pump
paths is the one place where a panic can cascade.

---

- [ ] **[Medium] ERR-01 — 61 unjustified `let _ =` in non-test backend code.**
  Examples: `crates/ssh/src/task.rs:126,284`; `sftp_task.rs:64,87,92,104,112,119,128,255`;
  `transfer/download.rs:93,113,129,133`; `transfer/staging.rs:64,70,80,82,102,108,115,122,124`;
  `crates/local-shell/src/event_loop.rs:196,200,207,211,269,293,342`; `local-shell/src/session.rs:168,170`.
  *Fix:* a `report_best_effort(op, result)` helper (like `report_generated_input`) that logs the operation
  name at `debug`/`warn`, per policy.

- [ ] **[Medium] ERR-02 — `std::sync::Mutex::lock().unwrap()` on every state/counter access in network/PTY paths.**
  `crates/ssh/src/state.rs:80`, `counting_stream.rs:49,67`, `task.rs:128,152,162,…`, both listeners. A panic
  inside a Term callback poisons the mutex and cascades into UI-thread panics.
  *Fix:* `parking_lot::Mutex` (already in the dependency graph via gpui) or
  `unwrap_or_else(PoisonError::into_inner)`; make rx/tx `AtomicU64`.

- [x] **[Medium] ERR-03 — Unregistered panel name yields a silent `InvalidPanel`.**
  `crates/workspace/src/layout/workspace/mod.rs:82-102`: a layout containing a stale name or a feature that
  failed to `init` renders "not registered" text with no log. *Fix:* `log::error!` with the name (see ARCH-08).

- [ ] **[Medium] ERR-04 — Paste rejections and spawn failures are log-only.**
  `TerminalSession::paste` `TooLarge` → `log::warn` (`crates/terminal/src/session.rs:409-412`);
  `spawn_local_view` failure → `log::warn` + empty tree (`terminal-view/src/panel/terminal_panel.rs:95-102`,
  `ops.rs:339-345`). Policy: user actions must produce a corrective notification. *Fix:* return `Result` to
  the view and `push_notification`.

- [x] **[Medium] ERR-05 — Persistence quarantine by a non-owner.** `crates/sftp-ui/src/persistence.rs:20-23`
  quarantines the whole shared `docks.json` on `InvalidData` — a shell/state-owned document
  ("a crate may mutate only fields it owns"). *Fix:* leave quarantine to `oneterm_state::dock_persistence`.

- [ ] **[Low] ERR-06 — `let _ =` / `_ =` without justification in shell/state/UI code:**
  `workspace/.../mod.rs:316`, `persistence.rs:49`, `layout.rs:39,101`, `widgets/breadcrumb.rs:55`,
  `datetime_clock.rs:40`, `net_speed.rs:57`, `resource.rs:99`; `settings-ui/src/updates/actions.rs:56,62,118,124`,
  `updates/install.rs:74,82,118`; `sftp-ui/src/panel.rs:160,550,583`; `session-ui/src/common.rs:110`;
  `update/src/manager.rs:282`, `update/src/install.rs:89,185,187,249`; `core/src/persistence.rs:201`;
  terminal-view: `render/theme_apply.rs:345` (`let _ = self`), `view/completion.rs:375` (`let _ = cx`),
  `:133` (`let _ = strip_cols`), `cell/batch.rs:211` (`let _ = &mut style`). *Fix:* comment or handle;
  delete the no-op ones.

- [ ] **[Low] ERR-07 — Drag-and-drop with no connection and file-picker errors are log-only**
  (`sftp-ui/src/render.rs:384-386`, `transfer.rs:253-260,342-349`). *Fix:* notification.

- [ ] **[Low] ERR-08 — `render_file_list` hides the table behind a full-panel `Error:` label** after a failed
  listing (`sftp-ui/src/render.rs:351-361`) — the user loses the previous list and toolbar. *Fix:* banner
  over the stale table.

- [ ] **[Low] ERR-09 — `add_ssh_terminal_to_dock` swallows the dock update error with `.ok()`**
  (`session-ui/src/common.rs:178-182`) — the terminal silently never appears. *Fix:* notify.

- [ ] **[Low] ERR-10 — `SwitchTheme` with unknown name is ignored silently** (`theme/src/theme.rs:93-97`).
  *Fix:* `log::warn!` (or notify) with the name.

- [ ] **[Low] ERR-11 — Info-level log noise on hot paths:** `workspace/.../mod.rs:341` (every `LayoutChanged`),
  `:358,:378`, `persistence.rs:93` (every debounce save). *Fix:* `debug!`.

- [x] **[Low] ERR-12 — `manager.rs:281-283` swallows `remove_dir_all` failure** on staging cleanup. *Fix:*
  `warn` with path.

- [ ] **[Low] ERR-13 — `Events::with_capacity(1024.try_into().unwrap())`** (`local-shell/src/event_loop.rs:230`).
  *Fix:* `NonZeroUsize::new(1024)` const or `Events::new()`.

- [ ] **[Low] ERR-14 — Non-NotFound read errors select defaults, later overwritten** — see CORR-61.

- [ ] **[Low] ERR-15 — Temp-file tests clean up with `let _ = remove_file` after assertions**
  (`ssh/src/handler_tests.rs:41,60,80`) — leak on failure. *Fix:* RAII guard.
