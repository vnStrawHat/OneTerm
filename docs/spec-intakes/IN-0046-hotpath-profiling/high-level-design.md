# High-Level Design: hotpath profiling

Intake: IN-0046
Lane: normal
Date: 2026-09-25

## Idea

Function-level time and allocation profiling of OneTerm's hot paths with the `hotpath`
crate (0.26.1, MIT). A developer builds the app with one cargo feature, runs a load, and
gets a table of the instrumented functions: calls, average, percentiles and share of the
run's wall time, or, in the allocation build, bytes and allocation count per call.

The feature is **off by default and costs nothing when off**: every site is written as
`#[cfg_attr(feature = "hotpath-profiling", hotpath::measure)]`, so without the feature the
attribute does not exist, `hotpath` is not in the build graph, and the binary is the same
as before (checked by comparing release binaries, US-0142).

## Diagram

```text
oneterm-app  features:
  hotpath-profiling        = dep:hotpath + hotpath/hotpath (timing)
                             + the leaf features below
  hotpath-profiling-alloc  = hotpath-profiling + hotpath/hotpath-alloc (bytes / count)
     |
     |  run() carries #[hotpath::main(allocator = oom::OomResilientAlloc, ...)]
     |  -> the guard lives for run(); report on drop, or after HOTPATH_SHUTDOWN_MS
     |  -> alloc build: hotpath's CountingAllocator wraps OomResilientAlloc and the
     |     plain #[global_allocator] static is compiled out (one allocator, not two)
     |
     +--> oneterm-terminal-view/hotpath-profiling  (dep:hotpath)
     |       PlanCache::update, class_rows_into, build_row_plan, GlyphCache::shape,
     |       TerminalElement::prepaint / paint, GridPainter::paint
     |       +--> oneterm-highlight/hotpath-profiling: scan_line_into
     |       +--> oneterm-terminal/hotpath-profiling
     +--> oneterm-local-shell/hotpath-profiling: the PTY read (pty_read)
     |       +--> oneterm-terminal/hotpath-profiling
     |               Pump::advance, TerminalContent::refill
     |               +--> oneterm-vt/hotpath-profiling
     |                       Terminal::feed, Parser::advance,
     |                       Screen::scroll_up / scroll_down, Terminal::snapshot_update
     +--> oneterm-ssh/hotpath-profiling: copy_sequential (SFTP transfer body)
     +--> oneterm-workspace/hotpath-profiling (US-0145): OneTermWorkspace::render,
     |       build_status_bar, AppTitleBar::render, StatusText::render
     +--> oneterm-session-ui/hotpath-profiling (US-0145): SessionPanel::render
     +--> oneterm-sftp-ui/hotpath-profiling (US-0145): SftpPanel::render, LocalPane::render
     (US-0145 also: TerminalView::render / blink_tick, TerminalPanel::render / title,
      SpaceTree::render, TerminalElement::request_layout, SshClientPanel::render in the app)
```

A leaf crate's feature only adds the optional `hotpath` dependency (`dep:hotpath`). Only
`oneterm-app` turns hotpath's own `hotpath` / `hotpath-alloc` features on, so a leaf
built alone with `--all-features` (the `oneterm-vt` CI steps) compiles hotpath's no-op
half and measures nothing.

## UI Wireframe

N/A — no UI surface. The report is text on stdout, or in the file named by
`HOTPATH_OUTPUT_PATH` (a release build has no console, so set it).

## Data Flow

1. `cargo build -p oneterm-app --release --features hotpath-profiling` (timing) or
   `--features hotpath-profiling-alloc` (allocations).
2. At start `run()` builds the hotpath guard: a background thread receives one message
   per measured call; hotpath's live metrics server listens on port 6770 unless
   `HOTPATH_METRICS_SERVER_OFF=1`.
3. Each instrumented call takes a timestamp (or the thread-local allocation counters) on
   entry and sends the delta on exit.
4. The report is written when the guard drops: when `run()` returns, or, with
   `HOTPATH_SHUTDOWN_MS=<ms>`, after that time, followed by `process::exit(0)`. The
   measurement protocol uses the latter; a killed process writes nothing.
5. Report controls (hotpath's env vars): `HOTPATH_OUTPUT_PATH`, `HOTPATH_OUTPUT_FORMAT`
   (`table` / `json` / `json-pretty`), `HOTPATH_ALLOC_METRIC` (`bytes` / `count`),
   `HOTPATH_LIMIT`, `HOTPATH_FOCUS`.

6. The measurement protocol is `research/hotpath-measure.ps1` (private home, own pid,
   closes its window so the report is written). hotpath's thread sampler costs about 10 % of
   a core on its own thread in every run; subtract it from process-level CPU.
7. Outside the app: `frame_time_under_output` starts a guard under
   `--features hotpath-profiling,hotpath/hotpath` on `oneterm-terminal-view`, which is how
   the per-site overhead was measured.

Adding a site: put the `cfg_attr` line on a function in one of the crates above (a crate
without the feature gets `hotpath-profiling = ["dep:hotpath"]`, the optional dependency,
and a line in `oneterm-app`'s fan-out). Keep sites at batch granularity (per chunk, per frame, per row, per shaped run); a site in a
per-byte or per-cell function costs more than the work it measures.

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane, developer-only feature that is off by default; the wiring above is
  the whole design.
