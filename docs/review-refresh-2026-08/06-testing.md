# 06 — Testing

> Part of the [2026-08 refresh review](README.md). Checklist format.

## Assessment

Test density is strongly bimodal. Engine crates are well covered (`terminal` 29 tests/KLOC,
`highlight` 28, `completion` 22) with a good `FakeTerminalSession` fixture, persistence fault-injection
and cross-process tests, and pure-logic tests for `SpaceTree` and `CompletionController`. Feature UI
crates are effectively untested (`sftp-ui` 0.8/KLOC, `workspace` 0.5, `settings-ui` 2.0, `agent-ui`
2.6), and the two backends' tokio/PTY loops are **untestable by construction** because they use
`russh::Channel`/`Handle`/`SftpSession` and real shells concretely. There are no integration tests
(`crates/*/tests/*.rs`), and CI runs the full test suite only on Ubuntu (see BUILD-01).

---

## A. Structural / testability

- [ ] **[High] TEST-01 — `ssh_main_task`, `sftp_task`, and `connect()` have no unit tests and cannot have any.**
  They use `russh::Channel`/`Handle`/`SftpSession` concretely. *Fix:* introduce `ShellTransport`
  (`recv/data/window_change/close`) and `SftpFs` traits; test lifecycle (Close ⇒ Closed, Exited ordering,
  resize coalescing, OSC reply routing) with fakes.

- [ ] **[Medium] TEST-02 — Local session tests spawn real shells** (`crates/local-shell/src/session_tests.rs:33-37,253-277`)
  — slow, OS-dependent, already flaky (commit c7a757b). `ShellEventLoop::run` is untested in isolation.
  *Fix:* make `run` generic over `EventedReadWrite` and drive it with a pipe/fake; keep one real-shell smoke
  test behind `#[ignore]`.

- [x] **[Medium] TEST-03 — `handlers/keyboard.rs` decision tree is inside a closure and untestable.**
  *Fix:* extract `fn classify_key(&Keystroke, search_focused, completion_visible, alt_screen) -> KeyAction`
  and test it; same for the `handle_mouse_down` URL/selection branch.

- [x] **[Medium] TEST-04 — Update flow untestable without network.** *Fix:* mock `GitHubClient` behind a trait
  so `download_and_stage` and `verify_asset_digest` wiring can be tested.

- [ ] **[Medium] TEST-05 — Feature UI crates have almost no tests** (`sftp-ui` 3, `workspace` 1, `settings-ui` 7,
  `agent-ui` 3). gpui `test-support` is already a dev-dep of `state`; add it to `workspace`/`sftp-ui`.
  Extract pure helpers first (below), then add `#[gpui::test]` for the main flows.

## B. Missing regression tests for findings in this review

- [x] **[High] TEST-06 — Pump-under-lock deadlock** (CORR-01): hold the Term lock, saturate the queue, assert the
  pump does not block.
- [x] **[High] TEST-07 — Completion Unicode slices** (CORR-07): fuzzy-then-accept and history-option cases with
  multibyte candidates at `engine.rs:73,83`, `scoring.rs:56`.
- [x] **[High] TEST-08 — Nested bracketed-paste marker** (SEC-01): add the vector to
  `session.rs::bracketed_paste_strips_embedded_markers`.
- [x] **[Medium] TEST-09 — `Exited` after `Output` in one batch** (CORR-02); drop into a non-empty Space (CORR-03).
- [x] **[Medium] TEST-10 — `Exited(None)` on local exit** (CORR-05); `SshSession` drop without close (CORR-06);
  RSA hash selection (SEC-02); changed-algorithm host key (SEC-05).
- [x] **[Medium] TEST-11 — SFTP path-safety tests are `#[cfg(unix)]`** (`ssh/src/sftp_task/sftp_task_tests.rs:1-29,66-88`)
  — download-root/symlink protections never run on the Windows-first CI; and SEC-11 shows they are wrong.
  *Fix:* `std::os::windows::fs::symlink_file` under `cfg(windows)`; Windows regression for ARCH-12
  (backslash in remote path).
- [x] **[Medium] TEST-12 — `terminal_settings` (apply/persist/color/font) has zero tests** — no
  `apply_config(to_config())` roundtrip, no `parse_hex_color`/`hsla_to_hex` inverse, no
  `parse_weight`/`weight_to_string` inverse. Would have caught CORR-12 and CORR-23.
- [x] **[Medium] TEST-13 — docks.json has no concurrent-update, quarantine, or backup-preservation test at the
  owner** (`state/src/dock_persistence.rs:170-230` covers isolated update + v0 migration only), contrary to
  `persistence.md` "Fixture convention".
- [x] **[Medium] TEST-14 — `AgentRegistry` fold/lifecycle is essentially untested**
  (`state/src/agent_registry.rs:156-274`: `apply`, `set_lifecycle`, `remove_terminal`, `clear_ended`,
  `refresh_stale`, `summary`).
- [x] **[Medium] TEST-15 — Updater: no tests for real archives** (zip-slip entry, tar with `..`/symlink, nested
  `dist/` layout — only `reject_unsafe_path` unit tests), `should_auto_check` interval math,
  `select_candidate` channel/prerelease/`skipped_version` filtering, cached-candidate-vs-channel.

## C. Pure code that is untested (cheap wins)

- [x] **[Medium] TEST-16 — terminal-view pure functions with 0 tests:** `update_line_times`
  (`view/local_view.rs:541-620`, the most intricate arithmetic in the crate), `compute_gutter_entries`
  (`element/gutter.rs:293-347`), `layout_selection` (`layout/selection.rs`), `layout_row` (`layout/row.rs`:
  run batching, box-run coalescing, zero-width/space skip), `line_hash`, `pixel_to_grid`,
  `visible_search_highlights` + `scroll_to_active_match` centring math (`search.rs:181-235`), scroll-only
  cache rotation with `Partial` damage (`cache.rs:79-117` — tests only use `cells: &[]`), scrollbar
  thumb→offset math (duplicated in `mouse.rs:185-206` and `scrollbar_overlay.rs:201-223`).
- [x] **[Medium] TEST-17 — sftp-ui pure helpers:** `format_size`, `format_permissions` (setuid/sticky),
  `format_owner`, `sort_entries` (folder-first + desc), `SftpTableDelegate::{apply_persisted_state,
  apply_widths, toggle_visibility, to_persisted_state}`, transfer state transitions (after ARCH-32).
- [x] **[Medium] TEST-18 — settings-ui:** `keystroke_to_string`/`is_modifier_only`, `save_key_bindings` diffing,
  `apply_check_result` for `Available`/`Disabled`/`Err`, `UpdateUiState::{shows_install_button, can_install_update}`,
  `percent_encode`; the persist-queue test only exercises the mutex struct, not `drain_update_config_persist_queue`.
- [x] **[Low] TEST-19 — session-ui:** `parse_user_host_port` edge cases (IPv6, empty user, invalid port),
  `build_tree_items` grouping/sorting/filter, `SshSessionStore::rename_group`, quick-connect field precedence.
- [x] **[Low] TEST-20 — workspace:** `center_has_no_visible_panel`, `switch_right_dock_mode`, `format_speed`,
  `format_memory`, load→reset_center_only→save flow.
- [x] **[Low] TEST-21 — theme:** all `EMBEDDED_THEME_FILES` parse and "Zed One Dark"/"Zed One Light" exist
  (`theme.rs:28-61,116-117`).
- [x] **[Low] TEST-22 — engine edge cases:** `url_policy` IPv6 host without port; `osc.rs parse_cwd_url` with
  `%20` and `file:///C:/`; `shell.rs` exact zsh PS1 bytes and Zsh kind vs `$SHELL`; highlight negative prompt
  cases (`100%`, `#include`).

## D. Weak tests

- [x] **[Medium] TEST-23 — Test enshrines the wrong wire format:** `crates/terminal/src/mouse_encode.rs:294-313`
  expects `char::from_u32(233)` (2 UTF-8 bytes) for X11 col 200 (CORR-25). Rewrite against `Vec<u8>` and add
  a `UTF8_MOUSE` case.
- [x] **[Low] TEST-24 — Test asserts nothing:** `crates/terminal/src/content.rs:321-335`
  `damage_partial_on_unchanged` accepts either enum variant. Assert `Partial(v)` with `v ⊆ {cursor line}` or
  delete.
- [x] **[Low] TEST-25 — The one gpui test in sftp-ui** (`transfer.rs:501-537`) only checks that a stale download
  does not panic.
