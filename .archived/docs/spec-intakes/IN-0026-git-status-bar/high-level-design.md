# High-Level Design: Git status in the status bar

Intake: IN-0026
Lane: normal
Date: 2026-09-11

## Idea

The status bar already polls the active terminal through `ActiveTerminalMetricsProvider`
(breadcrumb, network speed). A third extractor, `local_cwd`, returns the OSC 7 cwd of the
active terminal only when its session is a local shell. A new `StatusText` widget samples
that cwd every 500 ms; when the cwd changed or 2 s passed since the last run, it spawns
`git -C <cwd> status --porcelain=v2 --branch` on the background executor, parses the output
into a short label, and shows it after the breadcrumb.

## Diagram

```text
 TerminalPanel::local_cwd ──▶ terminal-view::status::active_local_cwd
                                       │ (registered at init as provider.local_cwd)
 workspace widgets/git_status ◀────────┘
   tick (500 ms, UI thread): cwd = active_terminal::local_cwd(dock, cx)
     cwd changed or last run > 2 s, none in flight
       └─▶ background_executor: git -C cwd status --porcelain=v2 --branch
             └─▶ parse → Arc<Mutex<Option<String>>> label
   sampler returns the stored label (None ⇒ indicator hidden)
```

## UI Wireframe

```text
+------------------------------------------------------------------------------+
| 14:02:11 | D:\TrungKFC-Research\Rust\myTerm2 | main* ↑1        ↓ 0 bps ↑ 0 bps | CPU 1.2% MEM 80 MB | [▭] |
+------------------------------------------------------------------------------+
```

Label forms: `main` (clean), `main* (+100 -41)` (work tree or index changed; lines added
and removed against `HEAD` from `git diff --numstat`, green and red), `main ↑2 ↓1`
(ahead/behind upstream), `a1b2c3d` (detached HEAD, short oid). All status-bar indicator text uses the
theme foreground colour (owner trial, 2026-09-11); only the diffstat counts are coloured. Every indicator carries a leading
icon. Hidden when the active
terminal is SSH, has no cwd, is outside a repository, or `git` is missing.

## Data Flow

1. The shell emits OSC 7; the session stores the cwd (already the case).
2. `TerminalPanel::local_cwd` returns it when `session.kind()` is `Local`.
3. The widget's sampler reads it through `oneterm_state::active_terminal::local_cwd`.
4. On cwd change or every 2 s, one background task runs git with `GIT_OPTIONAL_LOCKS=0`
   and (Windows) `CREATE_NO_WINDOW`; concurrent runs are skipped while one is in flight.
5. `parse_porcelain` turns the output into the label; a non-zero exit or spawn error yields
   `None` and the indicator hides.
6. `StatusText` re-renders only when the label text changed.

## Detail Design

- [x] Detail design: not needed
- Reason: one widget and one additive provider field; the parsing rules are in the widget's
  unit tests.
