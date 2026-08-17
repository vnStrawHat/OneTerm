# 09 — Remediation plan

> Part of the [2026-08 refresh review](README.md). A prioritised, phased checklist that references the
> item IDs in files 01–08. Phases are ordered by risk reduction per unit of work; within a phase, items
> are independent unless noted. Each phase should end with the full CI gate green
> (`fmt`, `clippy -D warnings`, `test --workspace`, the four Python checks).

## Phase 0 — Stop the bleeding (days)

Small, surgical fixes with regression tests. No refactoring.

- [x] CORR-01 — buffer reliable events outside the `Term` lock (minimal version: per-listener
  `VecDeque` flushed after `advance`) — **both** backends. Test TEST-06.
- [x] SEC-01 — strip all `\x1b`/`\x03` in bracketed paste. Test TEST-08.
- [x] SEC-02 — RSA `best_supported_rsa_hash()`.
- [x] CORR-07 — three `get(..)` slices in completion. Test TEST-07.
- [x] CORR-02 — add `Exited`/`Closed` arms to the drain (full dedupe in Phase 2). Test TEST-09.
- [x] CORR-03 — put the view back on `fill_empty` `Err`. Test TEST-09.
- [x] CORR-05 — forward `Exited(None)` and emit `Closed` on local exit. Test TEST-10.
- [x] CORR-06 — `impl Drop for SshSession`. Test TEST-10.
- [x] CORR-04 — synchronous (or awaited) docks.json write on close.
- [x] CORR-12 — persist `base_font_size`; add roundtrip test TEST-12.
- [x] CORR-08 — Linux installer touches only package files.
- [x] CORR-23 — ASCII check in `parse_hex_color`.
- [x] ARCH-12 stop-gap — sanitise `Rename`/`Mkdir`/`Remove` paths in `sftp_task.rs`; Windows test TEST-11.
- [x] SEC-11 — `symlink_metadata()` at the four sites; un-gate the tests from `cfg(unix)` (TEST-11).
- [x] BUILD-01 — run clippy + full tests on `windows-latest`.
- [x] BUILD-23 — add `.github/**` to CI path filters.

## Phase 1 — Contracts and CI identity (1–2 weeks)

- [ ] BUILD-02 / BUILD-13 — workspace `version` + `CARGO_PKG_VERSION`; delete three `VERSION` build scripts.
- [ ] BUILD-03 — `rust-toolchain.toml` + `rust-version`.
- [ ] BUILD-04 / BUILD-05 / BUILD-06 — prune unused deps; centralise `windows-sys`; fix `dependencies.md` §3.
- [ ] BUILD-12 — canonical updater repo constant; drop `git remote` in build.rs.
- [ ] BUILD-18 / BUILD-19 — prune vendor test corpus; `vendor/refresh.sh --check` in CI.
- [ ] BUILD-25 — `cargo-deny` job (licences + bans).
- [ ] ARCH-12 proper — `RemotePath` newtype in `core`; `SftpBackend` takes it.
- [ ] ARCH-05 — `TransferHandle` / `TransferEvent` (kills the negative-progress sentinel; also fixes CORR-31).
- [ ] ARCH-04 — rename `rmdir` → `remove_dir_all`; fix dialog wording.
- [ ] ARCH-03 — `take_events()` once-only contract.
- [ ] ARCH-08 / ERR-03 — panel-name constants module; error log on unregistered names; move
  `RightDockMode::panel_name` out of `core` (ARCH-07).
- [ ] SEC-05 — changed-algorithm host key → `ChangedHostKey`.
- [ ] SEC-06 — quote ConPTY program path / `escape_args`.
- [ ] SEC-13 / SEC-14 — keyboard-interactive fallback; keepalive.
- [ ] CORR-09 — SFTP `load_generation`.
- [ ] CORR-14 / CORR-15 / ARCH-37 — single-writer persistence for sessions.json and update_config.json.
- [ ] CORR-16 — quarantine + continue on runtime docks.json corruption; ERR-05 — remove sftp-ui quarantine.
- [ ] CORR-25 / CORR-26 / CORR-27 / CORR-28 / CORR-29 — key/mouse/PS1/shell/paste encoding fixes + tests
  (TEST-22, TEST-23).
- [ ] HYG-17 — AGENTS.md §4 lists the full CI gate.

## Phase 2 — Structural refactors (2–4 weeks, one PR each)

- [ ] ARCH-01 — lift listener/state/OSC/colour/line-accounting into `crates/terminal`; backends keep transport
  only. Prerequisite for TEST-01/TEST-02 (`ShellTransport`, `EventedReadWrite` generics). Finalises CORR-01,
  SEC-08, CORR-11, CORR-21, PERF-19, PERF-20.
- [ ] ARCH-02 — split `TerminalSession`; delete dead members (HYG-03).
- [ ] ARCH-21 / ARCH-22 / ARCH-23 / ARCH-24 / ARCH-25 — terminal-view: sub-state structs, single
  `handle_event`, module merge, `place_view`, `TerminalPanel::open`, `pub(crate)` sweep. Extract pure
  functions listed in TEST-16 while touching each area.
- [ ] PERF-01 / PERF-02 / PERF-03 / PERF-04 — render hot path: cached theme, cached gutter shapes, one
  `terminal_info()` per frame, dirty-flag search. Then PERF-05..PERF-11 opportunistically.
- [ ] ARCH-13 / ARCH-14 / ARCH-15 / ARCH-16 / ARCH-17 — `AppServices` as the single registry; remove `AppState`
  mirrors + dead toggle chain (HYG-01); move `notif_ext`; settings defaults single-sourced (roundtrip test).
- [ ] ARCH-28 / ARCH-29 — SFTP task tied to connection lifetime; backend `pub(crate)` sweep.
- [ ] ARCH-30 / ARCH-31 / ARCH-32 / ARCH-33 / ARCH-34 / ARCH-35 — sftp-ui/session-ui: drop `PendingAction`,
  group `SftpPanel` state, one `run_transfer`, shared `FormDialog`, one host parser, stable session ids
  (schema v2). Add tests TEST-17 / TEST-19.
- [ ] ARCH-06 — typed `AppError` variants for SFTP status / connect phase / config load.
- [ ] PERF-18 — pipelined SFTP reads (256 KiB, N in flight).
- [ ] BUILD-21 — upstream the two gpui-component patches; retire the fork when merged.

## Phase 3 — Polish (ongoing)

- [ ] SEC-03 / SEC-04 / SEC-07 / SEC-09 / SEC-10 — OSC 8 display check + confirm UI, dedup cap, control-char
  filters, attached-secret redaction, `copy_on_select` setting.
- [ ] SEC-18..SEC-22 — updater: warn on disabled TLS, redact proxy, size caps, https-only, zip symlink guard;
  consider signature verification.
- [ ] SEC-24 / SEC-25 — crash-report permissions and symlink check.
- [ ] CORR-30..CORR-70 — remaining Medium/Low correctness items.
- [ ] ERR-01 / ERR-02 / ERR-04 / ERR-06..ERR-15 — `report_best_effort` helper; `parking_lot`; user
  notifications for log-only failures.
- [ ] PERF-12..PERF-17, PERF-21..PERF-31 — remaining hot-path and widget polling items.
- [ ] TEST-03 / TEST-04 / TEST-05 / TEST-13 / TEST-14 / TEST-15 / TEST-18 / TEST-20 / TEST-21 / TEST-24 /
  TEST-25 — coverage for feature UIs, dock persistence, agent registry, updater, theme.
- [ ] HYG-02..HYG-15 — dead code, stale comments, theme tokens for hard-coded colours, named consts.
- [ ] HYG-16 / HYG-18 / HYG-19 / HYG-20..HYG-23 — archive old reviews, add `docs/README.md`, fix drifted design
  docs and README, delete stray root files, `NOTICE`/third-party notices, `.editorconfig`.
- [ ] BUILD-07..BUILD-11, BUILD-14..BUILD-17, BUILD-20, BUILD-22, BUILD-24, BUILD-26..BUILD-28 — remaining
  manifest, script, vendor-doc and workflow items.

## Suggested tracking

Keep this directory as the live checklist: tick items in files 01–08 as PRs merge, and reference the
item ID (e.g. `CORR-01`) in commit bodies so the history stays searchable. When a phase completes, add
a dated note at the top of this file.
