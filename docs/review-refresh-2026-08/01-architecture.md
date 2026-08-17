# 01 — Architecture & code structure

> Part of the [2026-08 refresh review](README.md). Checklist format; tick items as they land.
> Severity: Critical / High / Medium / Low.

## Assessment

The layered workspace is genuinely enforced (`cargo tree` confirms R1–R12; the dependency-graph
policy script runs in CI). `core` and `terminal` are gpui-free, backends are behind `TerminalSession`
/ `SftpBackend` / `SessionFactory`, the shell builds panels by name and drives features through a
fn-pointer registry. That is the good news.

The recurring architectural weakness is **duplication and leakage at the seams**: two backends that
copy the same pump/listener code, a UI crate whose central struct absorbed every concern, contracts
(`TerminalSession`, `SftpBackend`, panel names, `AppError`) that are either too wide or too primitive,
and shared crates (`state`, `settings`) that grew by accretion. None of it blocks the current release;
all of it raises the cost of the next feature.

---

## A. Layering & contracts

- [ ] **[High] ARCH-01 — Backend pump/listener/state logic duplicated between `ssh` and `local-shell`.**
  `crates/ssh/src/listener.rs:254-471` vs `crates/local-shell/src/listener.rs:164-398`
  (forward / forward_lifecycle / colour queries / set_title / set_clipboard / handle_osc_payload /
  send_event are near-identical); `crates/ssh/src/state.rs:14-54` vs `crates/local-shell/src/state.rs:32-71`;
  OSC 10/11/12 colour-reply block `crates/ssh/src/task.rs:183-222` vs `crates/local-shell/src/event_loop.rs:440-482`;
  absolute-line accounting `task.rs:158-181` vs `event_loop.rs:357-418`. Drift is already visible:
  local forwards `ClipboardRead` unconditionally (`local-shell/src/listener.rs:344-346`) while ssh
  consults `allow_clipboard_read` (`ssh/src/listener.rs:420-427`).
  *Fix:* lift `SessionState`, a `SessionEventSink` (delivery policy + counters), an `OscRouter`
  (alacritty `Event` → `SessionEvent` + state), `ColorQueryReplier` and `LineAccounting` into
  `crates/terminal`; each backend keeps only transport (`pty_write` / `pty_resize` / `pty_close`)
  behind a small trait. This also fixes CORR-01 in one place.

- [ ] **[High] ARCH-12 — `SftpBackend` models remote POSIX paths as host `PathBuf`.**
  `crates/core/src/sftp.rs:95-130`; consumers `crates/sftp-ui/src/actions.rs:106-107` (`parent.join(&new_name)`)
  and `:339` (`cwd.join(&name)`); the ssh backend sanitises `\`→`/` only for read_dir/stat/upload/download/rmdir
  (`ssh/src/sftp_task/metadata.rs:145,209`, `transfer/upload.rs:60`), **not** for `Rename`/`Mkdir`/`Remove`
  (`ssh/src/sftp_task.rs:101,109,125`). On Windows, renaming `/home/u/a.txt` → `b.txt` sends `/home/u\b.txt`.
  *Fix:* introduce a `RemotePath` newtype in `core` (always `/`-separated, with `join`/`parent`/`file_name`)
  and change the trait to take it. Stop-gap: sanitise in `sftp_task.rs` for all three commands + Windows regression test.

- [ ] **[Medium] ARCH-02 — `TerminalSession` is a ~45-method god trait with silent no-op defaults.**
  `crates/terminal/src/session.rs:196-448` mixes render snapshots, PTY I/O, mouse, selection, search, IME,
  lifecycle, automation (`send_keystroke`), presentation (`breadcrumb_text` "text shown in the toolbar") and
  capabilities. Defaults such as `search()` → `Vec::new()` (l.342) and `scroll_to_prompt()` no-op (l.408)
  hide unimplemented behaviour behind success. Dead surface: `set_cell_size`/`cell_width`/`line_height`/
  `cursor_bounds` have no UI callers; `SessionEvent::ForegroundProcess` is never emitted.
  *Fix:* split into focused traits composed by the session (`TerminalRender`, `TerminalInput`, `TerminalIme`,
  `TerminalLifecycle`) or move optional features into `TerminalCapabilities`; remove presentation methods;
  delete dead members.

- [ ] **[Medium] ARCH-03 — Single-subscriber `subscribe()` contract is undocumented and duplicated 3×.**
  `crates/terminal/src/session.rs:355`, `test_support.rs:359-365`, `ssh/src/session_terminal.rs:219`,
  `local-shell/src/session_terminal.rs:221`: a second call silently returns a dead `bounded(1)` receiver.
  *Fix:* `fn take_events(&self) -> Option<Receiver<SessionEvent>>` (or `Result<_, AlreadySubscribed>`) and
  document once-only semantics on the trait.

- [ ] **[Medium] ARCH-04 — `SftpBackend::rmdir` contract mismatch.** `crates/core/src/sftp.rs:96-97` says
  "Remove an empty directory"; the implementation is a bounded recursive delete
  (`crates/ssh/src/sftp_task.rs:114-121`, `sftp_task/recursive_delete.rs`); the confirm dialog
  (`crates/sftp-ui/src/actions.rs:236`) says only "delete folder".
  *Fix:* rename to `remove_dir_all` with an explicit doc; word the dialog accordingly.

- [ ] **[Medium] ARCH-05 — Cancellation encoded as a magic negative progress value.**
  `crates/sftp-ui/src/transfer.rs:93-94,388-389` treat `progress < 0.0` as "cancelled"; the trait doc
  (`core/src/sftp.rs:114`) says 0.0–1.0. The trait also mixes async `SftpFuture` methods with sync
  channel-returning `upload/download` and returns tuples `(Receiver<f64>, Receiver<Result<()>>)`.
  *Fix:* a `TransferHandle { progress: Receiver<TransferEvent>, cancel }` where
  `enum TransferEvent { Progress(f64), Cancelled }`.

- [ ] **[Medium] ARCH-06 — Stringly-typed errors dominate.** `crates/core/src/error.rs:43-51`:
  `AppError::Other(String)` / `AppError::msg` has ~120 call sites vs ~18 typed uses; `crates/ssh/src/session.rs`
  uses `anyhow` internally and flattens at 394-401 (losing `Cancelled`); `map_sftp_err` (`sftp_task.rs:262-264`)
  flattens SFTP status codes to strings so the UI cannot distinguish permission-denied from not-found.
  *Fix:* typed variants for the recurring classes (shell resolution, SFTP status, connect phase, config load);
  reserve `Other` for opaque messages.

- [ ] **[Medium] ARCH-07 — UI concepts leak into the "pure domain" `core`.**
  `crates/core/src/config/dock_mode.rs:37-43` (`RightDockMode::panel_name()` returns gpui-component
  `PanelRegistry` names), `crates/core/src/sftp.rs:57-64` (`SftpTableState` = column widths/visibility),
  `crates/terminal/src/osc_agent/types.rs` (`AgentState::badge()` returns emoji).
  *Fix:* keep the enums, move panel-name mapping to the panel-name module (ARCH-08), `SftpTableState` to
  `settings`/`state`, badges to `agent-ui`/theme.

- [ ] **[Low] ARCH-09 — Duplicate identical structs.** `crates/core/src/sftp.rs:23-38` (`FileEntry`) and
  `:42-55` (`FileStat`) have the same 12 fields. *Fix:* keep one.

- [ ] **[Low] ARCH-10 — Compat shim violates "no compatibility layers".** `crates/ssh/src/config.rs:1-7`
  re-exports core's `SshConfig` "to keep … working". *Fix:* delete; import from core.

- [ ] **[Low] ARCH-11 — Layer docs omit `completion`.** `docs/agents/crate-dependency-rules.md` L0 lists
  `core · terminal · highlight`; `state` and `terminal-view` depend on `oneterm-completion`. *Fix:* add it.

## B. Shell inversion & global state

- [ ] **[High] ARCH-08 — Panel names are string literals duplicated across five places, including `core`.**
  `crates/workspace/src/layout/workspace/layout.rs:27,36,87,97`, `.../workspace/actions.rs:33`,
  `crates/core/src/config/dock_mode.rs:37-41`, `crates/app/src/ssh_client_panel.rs:44`,
  `crates/app/src/agent_panel.rs:32`, `crates/terminal-view/src/lib.rs:44`. R4 ("shell does not know which
  features exist") holds only nominally; a typo yields a silent `InvalidPanel` (see ERR-03).
  *Fix:* one `pub mod panel_names { pub const TERMINAL: &str = …; }` in the lowest shared crate that already
  talks to gpui-component (`oneterm-state`), used by both registration and builders; `log::error!` in
  `build_named_panel` when the name is not registered.

- [ ] **[Medium] ARCH-13 — Three ad-hoc injection registries instead of the documented single bundle.**
  `crates/state/src/services.rs:145-191` (`AppServices`, dup-checked, validated),
  `crates/state/src/active_terminal.rs:315-327` (`set_provider` → unconditional `set_global`),
  `crates/state/src/agent_focus.rs:142-153` (same); plus `terminal-view` keeps its own mutable global
  `AgentNavIndex` (`terminal-view/src/agent.rs:131-148`) re-inserted on every OSC 9;7 event.
  *Fix:* fold `ActiveTerminalMetricsProvider` and `AgentFocuser` into `AppServices` (builder that features
  contribute to during `init()`, then `validate()`); keep the nav index inside `AgentRegistry`.

- [ ] **[Medium] ARCH-14 — `AppState` carries workspace-private mirrors and self-described legacy fields.**
  `crates/state/src/app_state.rs:28-41`: `dock_area` ("legacy commands"), `primary_workspace_id`
  ("legacy constructors"), `zoomed_panel: Option<Arc<Mutex<Option<String>>>>`,
  `toggle_button_visible: Option<Arc<AtomicBool>>` — `Arc<Mutex>`/atomics inside a single-threaded gpui
  entity that exist only so `crates/app/src/window.rs:66-69` can call `save_dock_state_on_close`.
  *Fix:* register the on-quit save from the shell with an App-level `cx.on_app_quit` capturing a
  `WeakEntity<DockArea>` + `Rc<Cell<_>>` mirrors; delete both fields (the toggle chain is dead — HYG-01).

- [ ] **[Medium] ARCH-15 — `state` is becoming a dumping ground.** `notif_ext.rs` is UI styling;
  `agent_model.rs`/`agent_registry.rs` (~760 lines) is a feature domain model; `completion_history.rs` is a
  one-entity wrapper; `dock_persistence.rs` is a schema owner. `crates/state/Cargo.toml:3` still says
  "AppState + notification helpers"; `docs/agents/structure.md:84-92` lists none of the new modules.
  *Fix:* keep `state` = "cross-feature runtime state + injection"; move `notif_ext` to `theme`; consider a
  gpui-free `crates/agent` model crate; update `Cargo.toml` description and `structure.md`.

- [ ] **[Medium] ARCH-16 — `settings` config↔live split is a hand-maintained bidirectional copy with
  duplicated defaults.** `crates/settings/src/terminal_settings/apply.rs:15-101` (config→live),
  `persist.rs:59-134` (live→config), `settings.rs:434-455` ("Mirrors CompletionConfig::default()") and
  `:592-619` re-state 1.2 / 10_000 / true …. Every new field touches four places; the drift already produced
  CORR-12 (zoomed font size persisted).
  *Fix:* `TerminalSettings::default() = apply_config(&TerminalConfig::default())` and a
  `apply_config(to_config()) == self` roundtrip test; longer term, keep `TerminalConfig` inside
  `TerminalSettings` and derive only parsed fields (`Hsla`, `FontWeight`).

- [ ] **[Low] ARCH-17 — `AppServices::commands(cx)` returns `Option`, so shell handlers silently no-op**
  (`workspace/.../actions.rs:86-88,124-126,252-254,270-274,285-288`, `mod.rs:418-422`) although
  `app/src/init.rs:69-70` `expect`s presence. *Fix:* treat as a startup invariant (`AppServices::global(cx)`
  panics with a precise message per error-policy) and delete the fallbacks (the About fallback at
  `actions.rs:290-299` is the only reason `crates/workspace/build.rs` exists).

- [ ] **[Low] ARCH-18 — `theme` owns `UiConfig` persistence.** `crates/theme/src/theme.rs:184-195` installs
  the observer that writes `ui_config.json` on every `Theme` mutation; each switch persists twice
  (`apply_config` at :94 then `apply_list_style_override` at :95). *Fix:* move the observer to `settings`
  (`UiConfig::observe_theme(cx)`), coalesce.

- [ ] **[Low] ARCH-19 — Inconsistent init contracts.** `AppState::init` is idempotent and called twice
  (`app/src/init.rs:19`, `workspace/.../mod.rs:140`); `UiConfig::init` (`ui_config.rs:167-171`) and
  `TerminalSettings::init` (`settings.rs:634-642`) overwrite unconditionally. *Fix:* pick one contract.

- [ ] **[Low] ARCH-20 — Global service-locator coupling in terminal-view.** The view/panel reach into 8
  process-globals (`TerminalSettings::global`, `AppState::global`, `AppServices::session_factory`,
  `commands::commands`, `AgentRegistry::try_global`, `GlobalCompletionHistory::try_global`,
  `active_terminal::set_provider`, `agent_focus::set_focuser`). *Fix:* pass `Entity<TerminalSettings>` /
  a small `TerminalDeps` struct into `LocalTerminalView::new`.

## C. terminal-view structure

- [ ] **[High] ARCH-21 — `LocalTerminalView` god struct with `impl` spread across 12 files.**
  `crates/terminal-view/src/view/local_view.rs:43-149`: 39 fields spanning session handle, focus, event loop
  + blink tasks, notifications, progress, agent status, scrollbar drag, URL hover, gutter timestamps,
  semantic overlay, palette push, 3 render caches, search (7 fields), split ctx, lifecycle, completion (3).
  `impl LocalTerminalView` blocks live in `view/{local_view,completion,cursor,font,grid,key,scrollbar_overlay}.rs`,
  `render/{view_render,overlays,theme_apply}.rs`, `search.rs`, `ime.rs`.
  *Fix:* extract cohesive sub-state structs owned by the view — `SearchState`, `GutterTimestamps`
  (`line_times`, `line_time_base`, `last_clear_epoch`, `update_line_times`), `UrlHover`, `ScrollbarState`,
  `CompletionState` — and fold `render/` back into `view/`.

- [ ] **[Medium] ARCH-22 — Event handling duplicated between the main loop and the coalescing drain.**
  `view/local_view.rs:172-268` and `:468-528` are ~50 lines of the same `match SessionEvent` (and the drain
  is missing arms — CORR-02). *Fix:* one `fn handle_event(&mut self, ev, cx)` called from both places; the
  drain only decides whether to coalesce `Output`.

- [ ] **[Medium] ARCH-23 — Module split by implementation type, not concept.** `cell/` is five 14–65-line
  files + `mod.rs`; `theme/resolve.rs` is 11 lines; `render/mod.rs` declares 3 files that are all
  `impl LocalTerminalView`; 14 `mod.rs` files; `layout/mod.rs`, `cell/mod.rs`, `handlers/mod.rs` still say
  "was split from …". Contradicts `docs/agents/code-style.md` (no `mod.rs` unless needed, no single-file
  folders). *Fix:* merge `cell/*` into `layout/row.rs`, `theme/resolve.rs` into `theme/palette.rs`,
  `render/*` into `view/render.rs`; drop the historical comments.

- [ ] **[Medium] ARCH-24 — Panel/ops/space duplication.** "Install a view into a Space" sequence appears 3×
  (`panel/ops.rs:249-264`, `:346-361`, `:433-439`); "duplicate destination is no longer available" 3×
  (`ops.rs:165-172`, `:252-260`, `:276-283`); Copy/Paste/SelectAll/Clear bodies twice
  (`handlers/menu.rs:129-190`, `panel/actions.rs:61-118`); split menu items in both `handlers/menu.rs:270-292`
  and `space/placeholder.rs:13-24`; `TerminalPanel::title()` (`terminal_panel.rs:460-469`) re-implements
  `tab_label()` (`:363-374`). *Fix:* `fn place_view(&mut self, target, view, window, cx) -> Result<(), ()>`,
  one `fn close_unplaced(...)`, `title()` calls `tab_label()`.

- [ ] **[Medium] ARCH-25 — Constructor sprawl / public API width.** `TerminalPanel` has 10 constructors
  (`terminal_panel.rs:59-227`); externally only `from_session_entity_with_duplicate_config`, `init`,
  `new_terminal_with_shell_cmd`, `find_in_active_terminal` are used. `lib.rs:24-33` re-exports
  `LocalTerminalView`, `TerminalTheme`, `build_terminal_theme`, `ensure_minimum_contrast`, `resolve_cell_color`,
  `TerminalSettingsPanel`, `pub mod panel`, all unused outside. *Fix:* one `TerminalPanel::open(spec: PanelSpec, …)`
  (enum `DefaultShell{workspace}` / `Shell(kind)` / `Session{session,title,duplicate_config}`); everything
  else `pub(crate)`.

- [ ] **[Low] ARCH-26 — `TerminalRenderCache` is 4 separate `Rc<RefCell<..>>`** with a tuple-typed gutter cache
  `Option<(Pixels, usize, Pixels, SharedString)>` (`layout/types.rs:266-272`, `view/local_view.rs:112-124`).
  *Fix:* one `Rc<RefCell<RenderCache { rows, gutter: GutterCache{…}, grid_size, metrics }>>`.

- [ ] **[Low] ARCH-27 — `TerminalScrollHandle` implements gpui-component `ScrollbarHandle`
  (`scroll_handle.rs:88-111`) but no `Scrollbar` element uses it**; the custom overlay in
  `view/scrollbar_overlay.rs` bypasses it. *Fix:* use `Scrollbar::vertical` (theming/fade for free) or delete
  the impl.

## D. Backends & feature crates

- [ ] **[Medium] ARCH-28 — SFTP task lifetime is not tied to the SSH connection.**
  `SshSession::close()` (`crates/ssh/src/session_terminal.rs:231-235`) never closes SFTP; when
  `ssh_main_task` exits, `sftp_task` keeps running and `alive()` stays true (`sftp_task.rs:251-254`); every op
  then fails after russh-sftp's 10 s timeout. *Fix:* `CancellationToken` cancelled at the end of
  `ssh_main_task`; `SshSession::close` calls `sftp.close()`.

- [ ] **[Medium] ARCH-29 — Backend public API far wider than used.** Only `LocalSession::spawn` and
  `oneterm_ssh::connect` are consumed (`crates/app/src/session_factory.rs:22,32`), yet
  `crates/local-shell/src/lib.rs:7-17` exports `event_loop`, `listener`, `state`, `ShellEventLoop`,
  `ShellNotifier`, `LocalListener`; `crates/ssh/src/lib.rs:9-27` exports `listener`, `Cmd`, `SshListener`,
  `SftpCmd`, `SftpEvent`, `SftpSession`. *Fix:* `pub(crate)` everything except `LocalSession`, `SshSession`,
  `connect`.

- [ ] **[Medium] ARCH-30 — sftp-ui executes actions inside `render()` via `PendingAction`.**
  `crates/sftp-ui/src/render.rs:29-43`, rationale in `types.rs:34-36` ("context-menu on_click only has
  `&mut App`") is stale — `PopupMenuItem::on_click` receives `&mut Window` (vendored `menu/popup_menu.rs:196-198`).
  Side effects in the layout pass are a re-entrancy hazard. *Fix:* call
  `panel.update(cx, |this, cx| this.do_rename(window, cx))` from the click handler; delete `PendingAction`.

- [ ] **[Medium] ARCH-31 — `SftpPanel` is a god struct.** 20 `pub(crate)` fields (`crates/sftp-ui/src/panel.rs:39-99`)
  written from `panel_ops`, `transfer`, `render*`, `table_delegate*`, `actions`. *Fix:* group into
  `BrowserView { cwd, selected, error, path_error }`, `TransferQueueView`, `FollowCwd { enabled, last, cache }`
  with methods; make fields private.

- [ ] **[Medium] ARCH-32 — Upload/download flow duplicated.** `crates/sftp-ui/src/transfer.rs:25-205` vs
  `:283-489`: progress loop, cancel sentinel, result mapping, store update identical; and in the backend
  the four copy loops (`ssh/src/sftp_task/transfer/download.rs:86-131,232-283`, `upload.rs:201-242,428-473`).
  *Fix:* `async fn run_transfer(panel, key, id, progress_rx, result_rx, cx)` in the UI; one
  `copy_with_progress` in the backend.

- [ ] **[Medium] ARCH-33 — Duplicated dialog scaffolding across feature crates.** The
  "`Rc<dyn Fn(&ClickEvent,&mut Window,&mut App)->bool>` save-logic + footer Cancel/OK" pattern is copied in
  `sftp-ui/src/actions.rs:90-200,321-425`, `session-ui/src/connect_dialog.rs:97-204`,
  `quick_connect_dialog.rs:170-300`, `session_dialog.rs:160-300`, `rename_group.rs`; "labelled input field"
  in `session-ui/src/common.rs:63-83` vs inline in sftp actions. *Fix:* a small `FormDialog` /
  `labelled_field` helper in `oneterm-state` (R10 – lowest gpui-aware shared layer) or a new `ui-kit` crate.

- [ ] **[Medium] ARCH-34 — Two independent `user@host:port` parsers.** `session-ui/src/common.rs:138-153`
  and `quick_connect_dialog.rs:193-207` differ (invalid port → default vs. treat as host). *Fix:* one function.

- [ ] **[Medium] ARCH-35 — `SshSessionStore` identifies sessions by `Vec` index.**
  `crates/session-ui/src/session_state.rs:104-141`; tree ids encode the index (`panel.rs:36`); dialogs capture
  `index` for later `update`. Any reorder/removal between opening a dialog and saving targets a different
  session. *Fix:* stable `id` on `SshSession` (schema v2), address by id.

- [ ] **[Medium] ARCH-36 — Updater knobs are dead.** `UpdateConfig::should_auto_check`
  (`crates/update/src/config.rs` ~L112) is never called; `channel` and `skipped_version` have no UI;
  `docs/auto-update.md` still claims a 24 h interval. *Fix:* wire them (settings items + honour interval in
  `start_auto_check`) or delete fields and doc claims.

- [ ] **[Low] ARCH-37 — `UpdateManager` and settings-ui are two whole-document writers of
  `update_config.json`** (`crates/update/src/manager.rs:244-248` vs `settings-ui/src/updates/config.rs`).
  *Fix:* split "cache metadata" (etag/last_checked/cached_candidate) from "preferences", or route all writes
  through `update_json_file`.

- [ ] **[Low] ARCH-38 — Crash-report dialog and "10 clicks on About panics" live in settings-ui**
  (`crates/settings-ui/src/crash_report_dialog.rs`, `about.rs:140-145`). *Fix:* move crash UI to `app`;
  gate the panic trigger behind a debug/env flag.

- [ ] **[Low] ARCH-39 — `SettingsPanel` implements `Panel` (`settings-ui/src/panel.rs:73-89`) but is never
  registered** (shown via `Root::new` in its own window). *Fix:* remove the impl or register it.

- [ ] **[Low] ARCH-40 — `SftpBrowserStore` is a `Mutex` inside a gpui `Global`** (`sftp-ui/src/browser_state.rs:117`),
  lazily created in `global(&mut App)`. *Fix:* `RefCell`, created in `init()`.

- [ ] **[Low] ARCH-41 — `AgentStatusEvent` repeats envelope fields in every variant.**
  `crates/terminal/src/osc_agent/mod.rs:64-168`: `agent/seq/ts` duplicated across 7 variants → three 7-arm
  accessor `match`es. *Fix:* `struct AgentStatusEvent { agent, seq, ts, payload: AgentPayload }`.

- [ ] **[Low] ARCH-42 — Presentation-driven struct-of-arrays in highlight theme.**
  `crates/highlight/src/theme.rs:50-63` (five parallel `Box<[…; 32]>`), `RowRoles.role: Box<[u8]>` (`role.rs:82`).
  *Fix:* `Box<[ClassStyle; COUNT]>` and `Box<[RowRole]>`.

- [ ] **[Low] ARCH-43 — Single-file folders.** `crates/completion/src/catalog/schema.rs`,
  `crates/completion/src/redact/detect.rs`. *Fix:* `catalog_schema.rs` / `redact_detect.rs` siblings.

- [ ] **[Low] ARCH-44 — Tuple returns / bool params in public trait API.** `core/src/sftp.rs:116-130`,
  `terminal/src/session.rs:246` (`(Vec<IndexedCell>, usize)`), `:361` (`is_local() -> bool`).
  *Fix:* `LineRangeCells { cells, num_cols }`, `SessionKind` enum.

## Process-global state inventory (for reference)

| Name | File | Kind | Mutable? | Verdict |
|---|---|---|---|---|
| `AppStateGlobal(Entity<AppState>)` | `state/src/app_state.rs:78-101` | gpui Global → Entity | yes | partly justified; legacy fields + mirrors are not (ARCH-14) |
| `AppServices` | `state/src/services.rs:145-150` | gpui Global (immutable) | set once, dup-checked | justified |
| `ActiveTerminalMetricsProvider` | `state/src/active_terminal.rs:315-322` | gpui Global (fn ptrs) | overwritable | fold into `AppServices` (ARCH-13) |
| `AgentFocuser` | `state/src/agent_focus.rs:142-148` | gpui Global (fn ptr) | overwritable | fold into `AppServices` |
| `AgentRegistryGlobal` | `state/src/agent_registry.rs:68-89` | gpui Global → Entity | yes | justified; crate placement questionable |
| `AgentNavIndex` | `terminal-view/src/agent.rs:131-148` | mutable global map | yes | merge into `AgentRegistry` |
| `GlobalCompletionHistory` | `state/src/completion_history.rs:103-126` | gpui Global → Entity | yes | justified |
| `UiConfigGlobal`, `TerminalSettingsGlobal` | `settings/src/ui_config.rs:155-171`, `terminal_settings/settings.rs:623-642` | gpui Global → Entity | yes | justified; init not idempotent |
| `SftpBrowserStore` | `sftp-ui/src/browser_state.rs:117` | gpui Global (Mutex) | yes | fine; drop the Mutex |
| `zoomed_panel: Arc<Mutex<_>>`, `toggle_button_visible: Arc<AtomicBool>` | `workspace/.../mod.rs:121,130,186,198` + AppState | shared primitives in single-threaded entities | yes | not justified (ARCH-14 / HYG-01) |
| `TEMP_SEQUENCE` | `core/src/persistence.rs:15` | `static AtomicU64` | yes | justified |
| Panic hook, `CrashHandler`, `SetConsoleCtrlHandler`, `env_logger` | `app/src/{crash_report,lib,native_crash}.rs` | process handlers | — | justified |
