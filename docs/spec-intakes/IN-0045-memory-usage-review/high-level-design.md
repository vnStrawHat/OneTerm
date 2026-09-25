# High-Level Design: Memory usage review

Intake: IN-0045
Lane: normal
Date: 2026-09-25

## Idea

This is a memory budget for the Windows release build: what each part may hold, per process
and per tab or pane, and in which counter. Packets under this intake are judged against it
with `research/measure.ps1`.

**Private working set** is the number users see in Task Manager. **Commit** is the number
the status bar shows. Both are budgeted, because untouched commit still counts against the
system commit limit and it is what OneTerm itself displays.

## Diagram

```text
oneterm.exe (one process)
+-------------------------------------------------------------------+
| per process                                                       |
|   gpui + gpui-component floor ..... 42 privws /  84 commit        |
|   OneTerm globals ................. <= 8 privws / <= 12 commit    |
|     (themes, highlight rules, settings, sysinfo, logging)         |
|   OOM ballast ..................... 0 privws / 16 commit (DEC-0020)|
+-------------------------------------------------------------------+
| per tab or split pane (x N)                                       |
|   idle: view + render caches + adapter + pump                     |
|        budget <= 2.5 privws / <= 3 commit                         |
|        today     2.0 privws / 26.6 commit  <-- glyph table 23.6   |
|   history: ring share + 8 B x cols per row (0.79 KB at 89 cols);  |
|        US-0138 deferred with a trigger, US-0139 not fixed         |
|   render caches: bounded by entries, never by history, and no     |
|        up-front table larger than 1 MB                            |
|   images: per-terminal byte budget (US-0140)                      |
+-------------------------------------------------------------------+
children per tab: OpenConsole.exe + shell (about 3.5 privws, not OneTerm's code)
```

## UI Wireframe

Only `US-0137` touches a surface: the resource item at the right end of the status bar.
Wording is an owner decision.

```text
+--------------------------------------------------------------------------+
| (clock) | (cwd breadcrumb)          | (git) |  CPU 0.1%  MEM 49.3 MB  [ ] |
+--------------------------------------------------------------------------+
                                       tooltip: "Private working set (as in
                                       Task Manager). Commit: 182.2 MB"
```

## Data Flow

1. `measure.ps1` launches a release build with a private `%USERPROFILE%`, and forces
   1280x800.
2. It runs S1 (one idle tab), S2 (three tabs), S3 (fill one tab's 10,000-line history),
   S4 (back to one tab) and S5 (two more idle minutes).
3. At each step it records `WorkingSetSize`, `PrivateWorkingSetSize` and `PrivateUsage` of
   the app pid, and the same summed over its child processes.
4. A packet passes the budget when:
   - S1 and S2 are inside the per-process and per-tab lines above;
   - S3 − S1 is inside history + render-cache;
   - S4 is within 2 MB of S1;
   - S5 is within 1 MB of S4.
5. `vmregions.ps1` attributes any overshoot by region (committed vs resident). A throwaway
   allocator log, as in research § 4.1, names the call site.

Budget table (MB; the "today" figures are measured on main `19237c8d`, 2026-09-25):

| Part | Scope | Counter | Today | Budget |
| --- | --- | --- | --- | --- |
| gpui + gpui-component | process | privws / commit | 41.7 / 84.1 | not ours; tracked only |
| OneTerm globals | process | privws / commit | about 7.7 / about 10 | 8 / 12 |
| OOM ballast | process | commit | 64 (16 since US-0141) | 16 (`DEC-0020`, accepted) |
| Idle tab or pane (view, adapter, pump, 1 MiB read buffer) | per tab | privws / commit | 2.0 / 26.6 | 2.5 / 3 |
| Glyph cache | per view | commit (idle), privws (busy) | 23.6 / up to 23.6 | table ≤ 1 MB up front; entries ≤ 4096 |
| History | per tab | privws | 0.75 KB/row + ring | ring share + 8 B × columns per row; trimming deferred (US-0138), ring stays preallocated (US-0139), see research/history-storage-assessment.md |
| Sixel | per terminal | privws | unbounded (64 MiB/image) | byte budget (US-0140) |

## Detail Design

Detail design is **required for the high-risk lane** and optional otherwise. When present, add one file per concern under `low-level-design/` so each stays reviewable.

- [x] Detail design: not needed
- Reason: this intake is measurement only. `US-0138` (engine storage) should add a
  `low-level-design/history-storage.md` before it starts, because it touches the external
  `oneterm-vt` contract.
