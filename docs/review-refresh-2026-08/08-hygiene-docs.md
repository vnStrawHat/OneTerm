# 08 — Hygiene, dead code, documentation

> Part of the [2026-08 refresh review](README.md). Checklist format.

## Assessment

Code hygiene is good at the macro level (no file > 700 lines, `dbg!`/`todo!` denied, 4 TODOs total,
English-only rule respected in code comments). The debt is in the details: dead action handlers and a
dead persistence chain, `pub` surface nobody uses, comments describing code that was refactored away,
hard-coded colours despite the "theme only" rule, and a `docs/` tree (134 files, 1.3 MB) that carries
two parallel review series, a boilerplate `PROJECT.md`, and a completed refactor plan still labelled
authoritative.

---

## A. Dead code

- [ ] **[Medium] HYG-01 — Dead action handlers and a dead persistence chain.** `AddSession`, `AddSftpBrowser`,
  `ToggleDockToggleButton` (`crates/workspace/src/layout/workspace/actions.rs:134-153,227-238`) are handled
  but dispatched nowhere; the entire `toggle_button_visible` chain — `Arc<AtomicBool>` (`mod.rs:121,186-206`),
  `AppState.toggle_button_visible` (`app_state.rs:41`), docks.json field (`dock_persistence.rs:32`),
  `read_toggle_button_visible` (`persistence.rs:128-130`) — exists only for that undispatched action.
  `SelectFont` (`app_menus.rs:51-54`) never dispatched; `SelectLocale`/Language menu
  (`app_menus.rs:24-28,112-120`) is a no-op ("rust-i18n is not wired up") and contains a non-English UI label
  (`:117`); `theme.rs:197` `let _ = cx.theme();`. *Fix:* delete, or wire deliberately.

- [ ] **[Medium] HYG-02 — Dead module + dependency in `terminal`.** `crates/terminal/src/url.rs` (`link_ranges`,
  `url_at`) has no callers outside its own tests; `terminal-view/src/url/detect.rs` reimplements detection.
  *Fix:* remove the module and the `linkify` dependency, or make the UI use it. Also dead:
  `Suggestion::remainder` (`completion/src/engine.rs:70`), `ExternalTargetPolicy::validate_with_display`
  (unless wired — SEC-03); `strip_unsafe_chars`/`truncate_utf8` are `pub` with no external callers.

- [ ] **[Medium] HYG-03 — Dead trait surface / stale state in backends.** `set_cell_size`/`cell_width`/
  `line_height`/`cursor_bounds` never called (`local-shell/src/session.rs:129-133`,
  `ssh/src/session_terminal.rs:26-29,212-216`); `SessionState.foreground_process` never written and
  `SessionEvent::ForegroundProcess` never emitted; `last_exit_code`/`exit_code` written but never read;
  `SftpEvent` + `SftpSession::subscribe` (`ssh/src/sftp.rs:86-93,133-135`) have no consumer;
  `Event::ChildExit` branch (`local-shell/src/listener.rs:354-358`) unreachable; `ShellMsg::Resize/Shutdown`
  channel branches (`event_loop.rs:283-289`) unreachable; `SshSession.cmd_tx` kept only under
  `#[allow(dead_code)]` (`session.rs:67-69`). *Fix:* delete or wire up with a test.

- [ ] **[Low] HYG-04 — terminal-view vestigial code.** `agent_status` field written (`local_view.rs:250,510`)
  but never read; explicit `impl Drop` (`:151-158`) redundant; `ScrollbarHandle` impl (`scroll_handle.rs:88-111`);
  unused params `_focus`/`metrics` (`handlers/keyboard.rs:24-27`), `_line_times`/`_theme`
  (`element/gutter.rs:246,250`), `_theme` (`view/scrollbar_overlay.rs:130`), `_window` (`panel/ops.rs:457`,
  `search.rs:241`), `_cx` (`terminal_panel.rs:408`); `LayoutPoint.line` in cached rows never read by paint;
  `allow_fuzzy_accept` branch (`completion/controller.rs:290-296`) unreachable and latent-buggy (writes the whole
  suggestion without erasing typed text).

- [ ] **[Low] HYG-05 — agent-ui dead branch:** `view.rs:460-463 group_badges` handles `Ended` but `visible()`
  already filters them out.

- [ ] **[Low] HYG-06 — Redundant/inconsistent code in engines:** `core/src/config/shell.rs:209-210,226-227`
  (second `find_in_path("….exe")` redundant); `highlight/src/scanner/command.rs:12-14` pass-through wrapper;
  `highlight/src/scanner/prompt.rs:103` `.unwrap()` while sibling regexes use `.expect(...)`;
  `terminal/src/palette.rs:104-119` matches on magic discriminants `256/257/258/259..=266` instead of
  `NamedColor` variants and duplicates `FOREGROUND_INDEX` etc. from `osc_color.rs`; `terminal/src/content.rs:131-237`
  `from`/`from_query` duplicate the snapshot body; `local-shell/src/session_terminal.rs:100-112` `send_ctrl_c`
  duplicated `cfg(windows)`/`not(windows)` with identical bodies (and its doc claims
  `GenerateConsoleCtrlEvent`, not implemented); `SftpSession.alive: Arc<Mutex<bool>>` (`sftp.rs:111`) →
  `AtomicBool`; `zoom.rs` is a 17-line re-export shim.

- [ ] **[Low] HYG-07 — Commented-out code** `sftp-ui/src/render_transfer.rs:68`.

## B. Stale comments & docs-in-code

- [ ] **[Medium] HYG-08 — "Split to stay under ~N lines" rationales contradict code-style** (split by
  responsibility, not size): `terminal-view/src/panel/ops.rs:4-5` (file is 509 lines), `layout/mod.rs:3`,
  `cell/mod.rs:3`, `handlers/mod.rs:3` ("was split from …"), `sftp-ui/src/lib.rs:3-4`, `types.rs:38`,
  `panel_ops.rs:4`, `table_delegate_menu.rs:2`, `transfer.rs:3`, `render_transfer.rs:3`
  (`file_browser.rs`/`render_list.rs` no longer exist). *Fix:* delete the historical prose.

- [ ] **[Low] HYG-09 — Stale comments (engines):** `terminal/src/url_policy.rs:155` ("keep core
  dependency-free" — this is the terminal crate); `highlight/src/lib.rs:12-14`, `color.rs:285-289`
  ("trivial cast" — bridge divides `h` by 360); `terminal/src/osc_agent/dedup.rs:296` (cites `osc_agent/tests.rs`,
  file is `receiver_tests.rs`); `terminal/src/key_encode.rs:8,75` ("Returns `None` when unrecognized" — never
  does); `paste.rs:3`, `security_policy.rs:4`, `url_policy.rs:3` ("Before Phase 1…" changelog prose in module docs).

- [ ] **[Low] HYG-10 — Stale comments (backends):** `ssh/src/listener.rs:8`, `state.rs:4` cite `local/src/...`
  (crate is `local-shell`); `local-shell/src/session.rs:4-6` cites "#11/#12 … freya handle.rs";
  `session_terminal.rs:4` "ARCH-05" ticket ids in module docs; `local-shell/src/session_terminal.rs:256-259`
  `scroll_to_prompt` TODO with `let _ = n;`.

- [ ] **[Low] HYG-11 — Stale comments (shell/state/settings):** `theme/src/theme.rs:149-170` describes radius
  0.001px/0px + `Scrolling` while code sets 4px/6px + `Always`; `workspace/.../mod.rs:300` "Debounce 5s" vs 2 s;
  `app/src/init.rs:47-49` "Agent panel (placeholder for now)"; `actions.rs:243`, `mod.rs:417` reference
  `views::settings`; `settings/src/ui_config.rs:165`, `terminal_settings/settings.rs:633` "called from
  `ui::init`"; `actions/src/lib.rs:42-45` "Add a new SessionPanel/SftpPanel"; `state/Cargo.toml:3`;
  `mod.rs:310,316` `story` variable name (copy-paste from reference `dock.rs`); `mod.rs:76,105` re-exports
  carrying long docs; `app/src/crash_report.rs:148` treats any live process with that PID as an owner (docs
  say "another live OneTerm PID"); debug `crashes_dir()` inherits a relative `config_dir()` (= `target/`).

- [ ] **[Low] HYG-12 — Stale comments (terminal-view / feature UIs):** `terminal-view/src/view/completion.rs:47-50`
  doc for `CursorCommand` sits on `CompletionKeyAction`; `render/view_render.rs:128` "single source" (two
  stampers); `settings-ui/src/panel.rs:50` "four setting pages" (five); `terminal/completion.rs:6` says
  `ClearCompletionHistory` is bindable but `BINDABLE_ACTIONS` has no such entry; `window.rs:69` `let _ = cx;`.

## C. Hard-coded colours & magic numbers (project rule: read from theme)

- [ ] **[Medium] HYG-13 — Hard-coded colours.** `terminal-view/src/theme/terminal_theme.rs:65-79`,
  `theme/palette.rs:88` (selection, search, gutter dim, Tango ANSI-16); `view/scrollbar_overlay.rs:75`
  (`hsla(0,0,0.5,…)`); `space/placeholder.rs:47` (`rgb(0x58c4dc)`); `completion/overlay.rs:189-191` (`white()`);
  `workspace/src/layout/title_bar.rs:66` (`rgb(0x58c4dc)`); `settings-ui/src/about.rs:138` (`rgb(0x58c4dc)`);
  `session-ui/src/tree_render.rs:46,48,92` (`0x58c4dc`, `0x7c8a15`, `#56B6C2`), `session_dialog.rs:136`
  (`#56B6C2`); `agent-ui/src/card.rs:596-600` (`usage_color` HSL gradient outside the theme).
  *Fix:* add `terminal.{selection,search_match,search_active,scrollbar_thumb,ansi[16]}` and an accent token
  to the theme JSON / `TerminalTheme` builder; derive `usage_color` from `theme.success/warning/danger`.

- [ ] **[Low] HYG-14 — Magic numbers.** `PtySize{24,80}` three times (`terminal-view/src/panel/terminal_panel.rs:267`,
  `ops.rs:104`, `session-ui/src/common.rs:229`); scrollbar 24/12/8/2 px and 2 s/3 s fade
  (`scrollbar_overlay.rs:149,162,176-179,197,232-233`); 150 ms search debounce (`search.rs:77`);
  `URL_WINDOW = 5` duplicated (`mouse.rs:35`, `url.rs:34`); `8.0` fallback advance (`measure.rs:162`);
  `+ px(8.0)` gutter pad (`gutter.rs:271`) vs `px(4.0)` paint offset (`paint.rs:300`); `px(480.)` repeated
  (`workspace/.../layout.rs:50,104`, `actions.rs:221`); `px(70.)`/`px(50.)` (`title_bar.rs:121,127,133`);
  `1600×1000`/`0.85`/`640×480` (`app/src/window.rs:23-27,40-43`). *Fix:* named `const`s in one place.

- [ ] **[Low] HYG-15 — Duplication hot spots worth a helper:** 6× "scroll + set `last_scroll_time` + notify"
  and 3× zoom blocks in `terminal-view/src/handlers/keyboard.rs:87-192`; 5× `MouseModifiers { shift, alt, ctrl }`
  construction in `mouse.rs`/`scroll.rs` (add `From<gpui::Modifiers>`); scrollbar drag math (2×).

## D. Repository & documentation

- [ ] **[Medium] HYG-16 — Doc sprawl and stale "source of truth" pointers.** `docs/` = 134 tracked files, 1.29 MB:
  `docs/review/` (14 files, 2026-07-22) and `docs/repository-review/` (14 files, 2026-07-23) are two parallel
  one-day-apart reviews with the same 14 topics; `docs/refactor/ui-crate-restructure.md` is a completed plan
  still called "authoritative" in `structure.md`; `docs/PROJECT.md` is an unfilled template; `docs/spec-intakes/`
  (33 files, 159 KB); `docs/terminal-code-review-remediation.md` (40 KB); AGENTS.md's Harness block mandates a
  `harness` tool that is not in the repo (`harness.db*` are git-ignored local files).
  *Fix:* move `docs/review`, `docs/repository-review`, `docs/refactor`, `terminal-code-review-remediation.md`
  under `docs/archive/` with a one-line status header; fill or delete `PROJECT.md`; add a `docs/README.md`
  index (current vs historical); make this review the current one and archive it likewise when superseded.

- [x] **[Medium] HYG-17 — AGENTS.md quality gate is narrower than CI.** §4 lists fmt + clippy + build; CI also
  requires `cargo test --workspace`, `verify-dependency-graph.py`, `check-ui-fork.py`, `check-doc-paths.py`,
  `check-english.py`, `benchmark-scale.py --list`. Agents following AGENTS.md will push red CI. *Fix:* list the
  full set (or add `scripts/ci-local.{sh,ps1}` and reference it).

- [ ] **[Low] HYG-18 — Design docs drifted from implementation.** `docs/terminal-backend.md` §7 describes a
  per-session current-thread runtime, `last_content: ArcSwap` cache and `std::sync::mpsc`; §5.2 says paint
  never locks the FairMutex — the implementation uses a shared 2-worker runtime and `snapshot()` locks Term.
  `docs/ssh-client-connect.md` §9.3 still says "MVP: accept any host key". `docs/terminal-split.md` TL;DR says
  4 px borders, code paints 1 px (`space/render.rs:292-294`). `docs/gui-layout.md` (v_split right dock,
  version 1, `std::fs::write`). `docs/agents/structure.md:84-92` lists neither `agent_*`, `services.rs`,
  `completion_history.rs` nor `dock_persistence.rs` and mentions a removed `SFTP_TABLE_STATE_FIELD`;
  `docs/agents/dependencies.md` §2 tells every gpui crate to depend on `gpui_platform` (only `app` does).
  *Fix:* update or label historical.

- [ ] **[Low] HYG-19 — README gaps.** Feature list omits auto-update and crash reporting (both shipped);
  "Release build" documents `dist/oneterm-<triple>/` while CI produces `oneterm-<version>-<triple>`; points to
  `docs/review/performance-benchmark.md` (a historical dir). *Fix:* update.

- [ ] **[Low] HYG-20 — Stray files at repo root** (all git-ignored, but the ignore file grew one-off entries
  instead of the junk being deleted): `NUL` (54 B), `opentui-examples.exe` (142 MB), `harness.db*`, `dist/`
  (10 `.oneterm-backup-*` dirs + 9.9 MB zip), `.pi/` (35 MB). *Fix:* delete `NUL` (`del \\.\NUL`) and the
  exe; replace the two ignore lines with generic patterns.

- [ ] **[Low] HYG-21 — No `NOTICE` / `THIRD-PARTY-NOTICES.md`** (Apache-2.0 §4(d); bundled Windows Terminal
  binaries are MIT). *Fix:* generate with `cargo about`.

- [ ] **[Low] HYG-22 — No `.editorconfig`** despite Python/PowerShell/Bash/JSON/MD in the repo. *Fix:* minimal
  file (`end_of_line = lf`, `indent_style = space`, `[*.rs] indent_size = 4`).

- [ ] **[Low] HYG-23 — `local-shell/examples/{doom_fire,pty_throughput}.rs` are diagnostics living in the crate.**
  *Fix:* `crates/tools` or feature-gate.
