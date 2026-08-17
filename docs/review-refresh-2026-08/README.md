# OneTerm — Full project review (refresh, 2026-08-17)

> **Status:** current. This is a from-scratch review of the whole workspace at commit
> `c7a757b` (v0.3.9). It intentionally does **not** build on `docs/review/` or
> `docs/repository-review/` (July 2026); those should be treated as historical.
>
> Every finding was verified by reading the code (file + line given). Findings are
> written as checklists so they can be ticked off as remediation lands. Line numbers
> refer to the tree at the commit above.

## How to read this report

| File | Category | Focus |
|---|---|---|
| [01-architecture.md](01-architecture.md) | Architecture & code structure | layering, god types, duplication, contracts, module layout, global state |
| [02-correctness-concurrency.md](02-correctness-concurrency.md) | Correctness & concurrency | deadlocks, races, lifecycle bugs, logic errors, panics |
| [03-security.md](03-security.md) | Security | SSH/host keys, SFTP paths, OSC/paste, updater trust chain, crash reports |
| [04-performance.md](04-performance.md) | Performance | render hot path, PTY pump, SFTP throughput, polling widgets |
| [05-error-handling.md](05-error-handling.md) | Error handling policy | `let _ =`, swallowed errors, `unwrap` on runtime paths, missing notifications |
| [06-testing.md](06-testing.md) | Testing | coverage gaps, untestable-by-design code, weak tests |
| [07-build-deps-ci.md](07-build-deps-ci.md) | Build, dependencies, vendor forks, CI | Cargo manifests, toolchain, vendored crates, workflows, scripts |
| [08-hygiene-docs.md](08-hygiene-docs.md) | Hygiene & documentation | dead code, stale comments, hard-coded colours, doc sprawl |
| [09-remediation-plan.md](09-remediation-plan.md) | Remediation plan | prioritised, phased checklist across all categories |

Severity scale: **Critical** (hang / data loss / remote-triggerable), **High** (real user-visible
defect or structural debt that blocks evolution), **Medium** (defect with a workaround, or
clear design smell), **Low** (polish, consistency, minor risk).

## Executive summary

**Health check (mechanical, run 2026-08-17):** `cargo clippy --workspace --all-targets -D warnings` clean;
`cargo test --workspace` green (~590 tests); `verify-dependency-graph.py`, `check-doc-paths.py`,
`check-english.py`, `check-ui-fork.py` all pass. The declared crate rules R1–R12 hold in `cargo tree`.

**Overall:** the project is in good shape structurally — the layered workspace (domain → engine →
shared → shell/features/backends → app) is real, not aspirational; the persistence layer, host-key
handling, crash capture, and OSC/paste/URL policies are engineered with care; the engine crates
(`terminal`, `completion`, `highlight`) are well tested. The problems are concentrated in a few
places, and most of them are **structural rather than local**:

1. **A remotely triggerable UI hang** (Critical): both backends `send_blocking` reliable events from
   inside `Term` callbacks while the `Term` lock is held; the UI thread locks `Term` while draining
   the same channel. `printf '\a%.0s' {1..5000}` (locally or from an SSH host) freezes the app.
2. **Backend duplication** (High): `ssh` and `local-shell` copy ~600 lines of listener/state/OSC/colour
   /line-accounting logic that already drifts (clipboard-read policy differs).
3. **The `LocalTerminalView` god struct** (39 fields, `impl` spread over 12 files) and a render hot path
   that re-parses JSON, re-shapes the gutter and scans the viewport under lock several times per frame.
4. **Contract leaks**: remote SFTP paths modelled as host `PathBuf` (creates `u\b.txt` on Windows),
   `TerminalSession` as a 45-method trait with silent no-op defaults, panel names as string literals
   in five places (including the "pure domain" `core`), three parallel fn-pointer registries.
5. **Data-safety edges**: layout save on close is a detached task racing `quit()`; the zoomed font size
   is persisted as the base size; the Linux updater relocates every file in the install directory;
   RSA keys sign with SHA-1 (fails on OpenSSH ≥ 8.8).
6. **CI blind spot**: the primary platform (Windows) never runs clippy or the UI test suites; all
   crates report version 0.0.0; the toolchain floats.

## Top 15 items to fix first

| # | Sev | Item | Where |
|---|---|---|---|
| 1 | Critical | Reliable events `send_blocking`'d under the `Term` lock → UI deadlock | [CORR-01](02-correctness-concurrency.md) |
| 2 | High | Nested bracketed-paste marker bypass | [SEC-01](03-security.md) |
| 3 | High | RSA private keys use `ssh-rsa` (SHA-1) | [SEC-02](03-security.md) |
| 4 | High | Remote SFTP paths are host `PathBuf` (backslash on Windows) | [ARCH-12](01-architecture.md) |
| 5 | High | `Exited`/`Closed` dropped when coalesced behind `Output` | [CORR-02](02-correctness-concurrency.md) |
| 6 | High | Tab drag-drop can `shutdown()` the live session | [CORR-03](02-correctness-concurrency.md) |
| 7 | High | docks.json save on close is a detached task racing `quit()` | [CORR-04](02-correctness-concurrency.md) |
| 8 | High | `LocalSession` stays "alive" on `Exited(None)` | [CORR-05](02-correctness-concurrency.md) |
| 9 | High | `SshSession` has no `Drop`; connection leaks | [CORR-06](02-correctness-concurrency.md) |
| 10 | High | Completion Unicode-boundary panics (3 surviving sites) | [CORR-07](02-correctness-concurrency.md) |
| 11 | High | Linux updater relocates every file in the install dir | [CORR-08](02-correctness-concurrency.md) |
| 12 | High | SFTP `load_dir` has no request generation (stale listing) | [CORR-09](02-correctness-concurrency.md) |
| 13 | High | Backend listener/state/OSC duplication | [ARCH-01](01-architecture.md) |
| 14 | High | Render hot path: JSON re-parse, gutter re-shape, 3× `terminal_info()` per frame | [PERF-01..04](04-performance.md) |
| 15 | High | Windows never runs clippy / UI tests in CI; crates are 0.0.0 | [BUILD-01, BUILD-02](07-build-deps-ci.md) |

## Workspace metrics (2026-08-17)

| Crate | LOC | Tests | Tests/KLOC | `unwrap`/`expect` (non-test) | `let _ =` | `unsafe` |
|---|---|---|---|---|---|---|
| terminal-view | 13 089 | 111 | 8.5 | 10 (`expect`) | 60 | 0 |
| terminal | 6 310 | 183 | 29.0 | 44 | 3 | 0 |
| ssh | 4 426 | 20 | 4.5 | 55 | 51 | 7 |
| sftp-ui | 3 970 | 3 | **0.8** | 3 | 3 | 1 |
| settings-ui | 3 477 | 7 | 2.0 | 3 | 8 | 0 |
| session-ui | 3 319 | 16 | 4.8 | 21 | 3 | 0 |
| local-shell | 2 738 | 32 | 11.7 | 40 | 14 | 8 |
| completion | 2 564 | 57 | 22.2 | 4 | 0 | 0 |
| highlight | 2 366 | 67 | 28.3 | 11 | 1 | 0 |
| workspace | 2 037 | 1 | **0.5** | 5 | 4 | 0 |
| settings | 1 987 | 23 | 11.6 | 19 | 4 | 0 |
| state | 1 827 | 15 | 8.2 | 19 | 2 | 0 |
| update | 1 796 | 20 | 11.1 | 22 | 12 | 2 |
| core | 1 685 | 20 | 11.9 | 35 | 8 | 8 |
| app | 1 583 | 14 | 8.8 | 46 (`expect`, crash path) | 0 | 5 |
| agent-ui | 1 160 | 3 | 2.6 | 0 | 0 | 0 |
| theme | 302 | 0 | 0 | 0 | 1 | 0 |
| actions | 128 | 0 | 0 | 0 | 0 | 0 |
| **total** | **54 764** | **592** | 10.8 | — | 209 | 31 |

`dbg!`/`todo!`/`unimplemented!`: 0 (denied by workspace lints). `TODO/FIXME`: 4. No file exceeds 700 lines.
Files > 500 lines: 16 (largest: `terminal-view/src/view/local_view.rs` 671).

## Method

- Six parallel deep-read passes by layer (engine · backends · terminal-view · shell/state/settings/app ·
  feature UIs + updater · build/deps/CI), each cross-checked against the project's own rules
  (`docs/agents/code-style.md`, `error-policy.md`, `persistence.md`, `crate-dependency-rules.md`) and
  the design docs for that area.
- Mechanical gates re-run locally: fmt, clippy, tests, the four Python checks, `cargo tree -d`.
- The Critical/High findings in the "Top 15" list were independently re-verified by reading the cited lines.
