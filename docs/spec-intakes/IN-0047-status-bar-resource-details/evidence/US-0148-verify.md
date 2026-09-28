# US-0148 adversarial verification

Date: 2026-09-28
Verifier: independent agent (worktree `agent-a7daf0c9a640c3380`), branch
`verify/us-0148` on top of `1099e298` (branch `feat/status-bar-resource-tooltip`,
main @`416903e3`).

## Status: FINAL — superseded by the 2026-09-28 rework re-verification below

Sections 1-7 below verify `1099e298` as it originally shipped (cost,
correctness of the figures, theme/contrast, tests, the independent product
walk, and the full gate). While this verification was in progress, the owner
rejected one accepted claim of `US-0148` from that commit — that the
tooltip's content is frozen for the duration of the hover — and an acceptance
rework was committed as `88846c0e` (same branch,
`feat/status-bar-resource-tooltip`) to make the tooltip refresh on the
sampler's 2 s cadence while it is open, and to move the sampler's work off
the UI thread (this review's own F1 finding). Per `docs/HARNESS.md`'s routing
table, this is acceptance rework of the owning `US-0148`, not a new `BUG`.

**The rework has now been independently re-verified — see
["2026-09-28 acceptance rework re-verification (88846c0e)"](#2026-09-28-acceptance-rework-re-verification-88846c0e)
at the end of this file for the final verdict, gate result, and updated
harness rows.** Sections 1-7 remain as first written (evidence of what
`1099e298` shipped) and are not edited in place; §3's "content updates while
open" sub-claim in particular describes `1099e298` only and is superseded by
the rework, not by an error in this review.

## Verdict (for 1099e298, as originally shipped): PASS WITH FINDINGS

No correctness defect blocked `1099e298`. One Major, non-blocking performance
finding (F1) and two Minor/informational notes were recorded below. The
frozen-tooltip design itself (§3, confirmed as-implemented) was not
re-litigated here since the owner overturned that design choice independently
of this review — see the rework section for the final, current-state verdict.

## 1. Cost

**Claim under test:** `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD)` enumerates
every thread on the system; is paying that (or an equivalent) cost every 2 s
on the UI thread acceptable, and does a cheaper source exist?

**What the code actually does.** `thread_count()`
(`crates/workspace/src/widgets/resource.rs:271-310`) uses
`TH32CS_SNAPPROCESS`, **not** `TH32CS_SNAPTHREAD`: it walks one snapshot of
every *process* and reads `cntThreads` straight off the matching
`PROCESSENTRY32W` entry. This is the cheaper of the two ToolHelp32 snapshot
kinds — confirmed by direct measurement below (a `TH32CS_SNAPTHREAD` walk,
which the code does not use, costs 4-5x more on this machine).

**Where it runs.** `StatusText::new_entity` (`status_text.rs:299-320`) drives
the sampler from `cx.spawn_in(window, ...)`. gpui-pre 0.3.3's own doc comment
on `Context::spawn_in` (`app/context.rs:674`) states: "The returned future
will be polled on the main thread." `ForegroundExecutor::spawn`
(`executor.rs:357`) is documented "Enqueues the given Task to run on the main
thread." So the timer await is non-blocking, but when it resolves, `tick()`
(status_text.rs:337) runs the sampler closure — the `sysinfo` refresh, the
`GetProcessMemoryInfo` call, and `thread_count()`'s snapshot walk — **on the
same thread that pumps window messages and renders**, not on a background
executor. This was true before US-0148 too (the sampler's `sysinfo` refresh
already ran there); US-0148 adds `thread_count()` to that same synchronous
path.

**Measured (this machine, Windows 11 Enterprise 10.0.26200, ~311-312 system
processes, release-profile throwaway probe outside the repo, `n=100`,
`windows-sys 0.59` / `sysinfo 0.37.2`, matching the workspace's pinned
versions):**

| Call | min | median | mean | p95 | max |
| --- | --- | --- | --- | --- | --- |
| `thread_count()`'s actual walk: `TH32CS_SNAPPROCESS` + `Process32First/NextW`, reading `cntThreads` (run 1) | 4.52 ms | 5.02 ms | 5.30 ms | 6.70 ms | 10.09 ms |
| same, run 2 (system under more variance) | 4.76 ms | 7.48 ms | 8.05 ms | 12.53 ms | 16.75 ms |
| `sysinfo::System::refresh_processes_specifics(Some(&[own_pid]))` — what the sampler **already** calls every 2 s, pre-`US-0148` | 5.40 ms | 6.18 ms | 6.56 ms | 8.75 ms | 12.70 ms |
| For comparison only, **not** what the code does: `TH32CS_SNAPTHREAD` (every thread system-wide) | 20.59 ms | 24.21 ms | 24.88 ms | 29.89 ms | 39.07 ms |
| `GetProcessHandleCount` (own process; wrong metric, not a substitute) | 0.40 µs | 0.40 µs | 0.45 µs | 0.50 µs | 2.8 µs |
| `GetProcessMemoryInfo` EX2 (already read every sample, pre-`US-0148`) | 0.40 µs | 0.50 µs | 0.53 µs | 0.60 µs | 2.7 µs |

Inspecting `sysinfo` 0.37.2's Windows backend
(`windows/system.rs:234-243`, `refresh_processes_specifics`) confirms *why*
the sysinfo refresh and `thread_count()` cost the same: **both** take a full
`CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` and walk every entry, even when
`ProcessesToUpdate::Some(&[pid])` filters to one pid — the filter is applied
inside the walk, not before it. So US-0148 adds a second, near-identical walk
on top of a pre-existing one: **combined UI-thread cost per 2 s sample is
roughly 10-16 ms** on this machine (was ~5-8 ms before this commit). That
scales with total *system* process count, not OneTerm's own, so a busier
machine (600-1000+ processes is not unusual on a managed Windows install)
would plausibly push this past 20-30 ms — more than a 60 Hz frame budget
(16.7 ms) and multiples of a 120 Hz one (8.3 ms), which reads as a periodic
stutter risk once every 2 s, worse on higher-refresh displays or busier
hosts.

**Finding F1 (Major, non-blocking — see remediation).** Documented below.

**On the disallowed/ruled-out alternatives, checked:**

- `NtQuerySystemInformation` — out of scope per this verification's own
  constraint (undocumented NT API); not proposed.
- `GetProcessHandleCount` — measured at ~0.4 µs (line above), but it is a
  **handle** count, not a thread count; not a substitute, confirmed by
  measurement and by the task's own framing.
- `sysinfo` 0.37.2 thread count on Windows — checked the source
  (`common/system.rs:2069-2080`, `Process::tasks()`): its doc comment reads
  "This method always returns `None` on other platforms than Linux." `sysinfo`
  genuinely has no cheaper Windows thread count; the `resource.rs` doc
  comments' claim to that effect is correct.
- `TH32CS_SNAPTHREAD` (every thread system-wide) — measured 4-5x worse (table
  above); the implementation correctly avoided it in favour of
  `TH32CS_SNAPPROCESS` + `cntThreads`.

Given all of the above, **`TH32CS_SNAPPROCESS` + `cntThreads` is already the
cheapest available *sanctioned* Win32 path** for one process's thread count
without a new dependency. There is no cheaper syscall to swap in. The real
lever is *where* it runs, not *which* API: move the sampler closure's work
(the `sysinfo` refresh, `os_memory_counters()`, and `thread_count()`) onto
`cx.background_executor()` and post only the resulting `Sample`/`Label` back
through `update_in`, keeping the existing 2 s cadence but taking the ~10-16 ms
walk off the thread that renders. This is a small, scoped change.

Also worth noting: the code's own `ponytail:` comment on `thread_count()`
(`resource.rs:269-270`) names `NtQueryInformationProcess` as the upgrade path
"if that ever shows up in a profile." That escape hatch does not actually
work — `NtQueryInformationProcess(ProcessBasicInformation)` does not return a
thread count field, and the only cheaper *NT*-level source for one process's
thread count is `NtQuerySystemInformation`, which this verification's own
constraints rule out. The comment's premise ("profile it later, swap the
syscall") does not hold; the fix is architectural (off the UI thread), not a
different syscall. Recommend correcting or removing that part of the comment
as a small follow-up.

**Severity rationale:** Major, not Critical/blocking. This is not a new class
of problem — the sampler's pre-existing `sysinfo` refresh already ran
synchronously on the UI thread every 2 s before this commit (apparently
accepted in `US-0137`) — US-0148 roughly **doubles** an already-accepted cost
rather than introducing a new one. The cost is disclosed, if incompletely, via
the existing `ponytail:` comment. Recommend a fast-follow `BUG` to move the
sampler off the UI thread rather than blocking this `US`.

## 2. Correctness of figures

Verified against `sysinfo` 0.37.2 source and `docs/spec-intakes/IN-0045-memory-usage-review/research/memory-attribution.md`:

- **Private working set** → Windows `PROCESS_MEMORY_COUNTERS_EX2::PrivateWorkingSetSize`, the same field Task Manager's "Memory" column reads. Matches.
- **Working set** (Windows) / **Resident (RSS)** (else) → `sysinfo` `Process::memory()`; on Windows this is `WorkingSetSize`. Matches the module doc comment.
- **Commit (private bytes)** (Windows) / **Virtual size** (else) → `sysinfo` `Process::virtual_memory()`; on Windows this is `PrivateUsage`. Matches; correctly excluded from the item's own `MEM` figure (module doc: "Commit... is not the item's figure: it counts pages that were never touched").
- **Peak working set** (Windows only) → `PROCESS_MEMORY_COUNTERS_EX2::PeakWorkingSetSize`, read from the same `GetProcessMemoryInfo` call already made for the private working set (one call, not two) — confirmed in `os_memory_counters()`.
- **CPU denominator.** `sample.cpu_percent = process.cpu_usage() / cores`; `cores = sys.cpus().len().max(1)`. `sysinfo`'s `cpu_usage()` is per-core (100% = 1 core); dividing by `cores` gives the Task Manager convention (100% = all cores), matching the module's existing, already-reviewed doc comment. The row text is `"{cpu}% of {cores} logical cores"` — unambiguous: it reads as "X.X% of total system capacity, out of N logical cores," matching the item's own `CPU 1.2%` figure (same total-system percentage). Minor clarity nit only (not a defect): a reader unfamiliar with the convention could misparse "of N logical cores" as "per core," but this is how Task Manager and Activity Monitor phrase it too.
- **CPU time (user + kernel)** → `Duration::from_millis(process.accumulated_cpu_time())`; `sysinfo`'s `accumulated_cpu_time()` is documented as the process's total CPU time (user + kernel) in ms. Matches; formatting (`format_duration`) scales sensibly (`12.4 s` / `4m 05s` / `3h 07m`), verified by `durations_scale_to_their_size`.
- **Threads.** `process.tasks().map(|t| t.len()).or_else(thread_count)`. `sysinfo::Process::tasks()` "always returns `None` on other platforms than Linux" (source, `common/system.rs:2051`) — so Windows and macOS fall through to `thread_count()`, which is `Some(n)` on Windows (ToolHelp32) and `None` on macOS/other. Matches the acceptance criterion ("Threads [n/a on macOS]") exactly.
  - Note (Minor, F2): `refresh_kind()` (`ProcessRefreshKind::nothing().with_cpu().with_memory()`) relies on `ProcessRefreshKind::nothing()`'s non-obvious default of `tasks: true` — confirmed in `sysinfo` source (`common/system.rs:2380-2394`, the *one* field `Default` does not zero, "Process by default includes all tasks") — to keep Linux's `tasks()` populated. Confirmed correct (the Linux backend at `unix/linux/process.rs:862` only populates tasks `if refresh_kind.tasks()`), but `refresh_kind()`'s own doc comment doesn't mention this reliance. A later edit that adds `.without_tasks()` (reasonable-looking cleanup, since nothing else in `refresh_kind()` mentions tasks) would silently regress the Linux thread count to "n/a" with no test on this Windows-only CI runner to catch it. Suggest a one-line comment.
- **Uptime** → `Duration::from_secs(process.run_time())`; `sysinfo::run_time()` is documented "for how much time the process has been running," i.e., from process start, not from indicator-creation or app-init time. Since the sampled process is OneTerm's own, process start and app start are the same instant anyway — no divergence is possible in practice, but the code path is correct regardless (it reads the OS-reported process start time, not an internal timer).

No correctness defects found in §2.

## 3. UI

- **Theme tokens only.** `git diff 416903e3 1099e298 -- crates/workspace/src/widgets/{resource,status_text,git_status}.rs` searched for `hsla(`/`rgb(`/`rgba(`/hex-literal colours: zero matches. `details_table` (`status_text.rs`) takes its `muted` colour as a parameter (`cx.theme().muted_foreground`) from the caller; the caller reads `cx.theme()` directly. No hardcoded colours.
- **Contrast gate.** `python scripts/check-theme-contrast.py` → `check-theme-contrast: 1482 foreground/surface pairings across 390 token/variant rows, all >= 4.5:1; primary text out-reads muted.foreground on all 585 shared-surface comparisons` (pass). Confirmed the `SURFACES` table already lists `popover.foreground` on `popover.background` (`check-theme-contrast.py:119`, citing the kit's `tooltip.rs:115`) and `muted.foreground` on `popover.background` (`check-theme-contrast.py:153`, citing "tooltips and notification toasts (kit `tooltip.rs`...)") **before** this diff — the pair this tooltip draws was already covered; no surface was added, matching the packet's claim, and `scripts/check-theme-contrast.py` is untouched in the diff.
- **`git_status.rs` change.** Confirmed mechanical only: `Label(segments)` (tuple struct) → `Label::new(segments)` / `.0` → `.segments`, no behaviour change (`git diff 416903e3 1099e298 -- crates/workspace/src/widgets/git_status.rs`).
- **Item text/format unchanged.** `Presentation { icon: Some(Icon::new(IconName::Cpu)), copyable: false, shorten: Shorten::Never }` is untouched by the diff (confirmed via the file diff); the `MEM`/`CPU` label format string is unchanged apart from now being built from `sample.*` fields instead of ad hoc locals.
- **Tooltip built once per hover, frozen while open — confirmed as-implemented in `1099e298`.** Read the kit's `Tooltip::element` (`reference/gpui-kit/crates/component/src/tooltip.rs:53-66`, `92-144`): the builder closure is invoked by `Render::render`, but `status_text.rs`'s `.tooltip(move |window, cx| { let details = details.clone(); Tooltip::element(move |_, cx| details_table(details.clone(), ...)).build(...) })` captures `details: Vec<Section>` **once**, when the outer `build_tooltip` callback runs (on `on_hover(true)` → `request_show`, `crates/gpui-base/src/tooltip.rs` / kit `managed_tooltip_with_placement`). Repeated `Tooltip` view re-renders while the hover persists reuse that same already-captured `Vec<Section>` — no re-sampling, matching "Nothing is read when the tooltip opens or renders." **This is exactly the behaviour the owner has since rejected**; it is accurately described here as what `1099e298` ships, not as a defect in `1099e298` — see the INTERIM notice above.
- **Does not stick after the cursor leaves — confirmed, with a testing caveat (F3, informational).** Independent GUI walk (own pid, private `USERPROFILE`, `PrintWindow(hwnd, dc, 2)`): moving the cursor away in one large `SetCursorPos` jump left the tooltip visibly open through a 500 ms and even a 2 s wait; a graduated cursor path (several intermediate `SetCursorPos` steps across the window before leaving, matching how a physical mouse moves) dismissed it correctly, within the kit's 300 ms `GRACE_PERIOD` (`gpui-base/src/tooltip.rs:13`). This reads as a synthetic-input artifact — a single large `SetCursorPos` warp does not generate the intermediate `WM_MOUSEMOVE` crossing GPUI's hit-testing needs to observe "the pointer left the trigger," which a real mouse move always produces — not a product defect. It does mean the packet's own GUI walk (`evidence/US-0148-gui-walk.md`) never actually exercised "hover, then leave": it only checked "before any hover" (no tooltip) and "while hovering" (tooltip shown), not "after leaving." Recorded as informational; not a blocking finding since the graduated-move retest passed.
- **Focus stealing.** Unchanged from the kit's existing `on_hover`/`request_show`/`request_hide` mechanism (`managed_tooltip_with_placement`); this diff does not touch that machinery, only what content is shown once the mechanism already decides to show something.

## 4. Tests

`cargo test -p oneterm-workspace --lib`: **41 passed, 0 failed, 3 ignored** (the ignored tests are the unrelated `IN-0043` elevation tests). Matches the packet's own claim exactly.

New/changed tests relevant to `US-0148`: `widgets::resource::tests::the_table_lists_every_field_in_a_fixed_order`, `..._figures_the_os_does_not_give_are_left_out_or_marked`, `..._durations_scale_to_their_size`, plus the pre-existing `displayed_memory_*`/`format_memory_*`.

**Discrimination check (adversarial):** locally swapped the order of the first two Memory rows in `the_table_lists_every_field_in_a_fixed_order`'s expected `vec!` (Private working set ↔ Working set) and reran just that test:

```
thread '...the_table_lists_every_field_in_a_fixed_order' panicked at crates\workspace\src\widgets\resource.rs:370:9:
assertion `left == right` failed
...
test result: FAILED. 0 passed; 1 failed; 0 ignored
```

Confirms the test genuinely pins row order, not just row presence. Change reverted immediately after (`git diff --stat` confirmed clean before continuing).

No test exists for the render-time branching in `status_text.rs` (`details.is_empty()` vs `Some(tooltip)` vs `None`) or for `details_table`'s layout — consistent with the file's existing pattern (the prior single-tooltip branch also had no render-path test) and reasonable given GPUI render-path testing requires a window; not flagged as a gap.

## 5. Product (independent GUI walk)

Built `cargo build -p oneterm-app --profile fast-dev` in this worktree's own `target/` (`CARGO_BUILD_JOBS=6`; no `CARGO_TARGET_DIR` override, so it never touched a shared target). Launched with a private `USERPROFILE`/`HOME` scratch directory, driven with `PrintWindow(hwnd, dc, 2)` + `SetCursorPos`, targeting only the pid this walk started; nothing else enumerated, signalled, or closed (closed via `taskkill /PID <own pid> /F` at the end).

Captured the hover table independently:

```
Memory
Private working set              52.1 MB
Working set                     165.9 MB
Commit (private bytes)          145.1 MB
Peak working set                166.6 MB
CPU
Usage               0.0% of 20 logical cores
CPU time (user + kernel)           0.6 s
Threads                               19
Uptime                              3.0 s
```

Cross-checked against `Get-Process -Id <pid>` (a source independent of the
app's own `GetProcessMemoryInfo`/`sysinfo` reads) taken moments after the
screenshot:

| Figure | Tooltip | `Get-Process` | Agreement |
| --- | --- | --- | --- |
| Working set | 165.9 MB | 167.1 MB (`WorkingSet64`) | within 1.2 MB |
| Commit (private bytes) | 145.1 MB | 145.9 MB (`PrivateMemorySize64`) | within 0.8 MB |
| Threads | 19 | 19 (`Threads.Count`) | **exact** |
| CPU time (user + kernel) | 0.6 s | 0.625 s (`TotalProcessorTime`) | within 0.03 s |
| Uptime | 3.0 s | 5.9 s (`Now - StartTime`) | off by ~2.9 s, explained below |

The uptime gap is larger than the packet's own walk saw (which used a tighter
script loop); this walk's driver script does more polling/sleeping between
process launch and the `Get-Process` read than between launch and the
tooltip's own frozen sample, so the two reads are simply taken further apart
in wall-clock time — not a computation error (`process.run_time()` is the OS's
own process-uptime figure, not an app-level timer; see §2).

Private working set (52.1 MB) and peak working set (166.6 MB) have no
`Get-Process`/.NET source independent of `GetProcessMemoryInfo` itself for
private WS (the .NET `Process` class does not expose `PrivateWorkingSetSize`);
a second, separate launch confirmed `Process.PeakWorkingSet64` is readable and
in the same ballpark (160.6 MB at a similar ~3 s uptime with 19 threads),
corroborating the peak-working-set figure is plausible, though not from a
fully independent field for private WS specifically. This matches the scope
the intake itself expected (`IN-0045` research notes `PrivateWorkingSetSize`
has no external tool exposing it besides Task Manager's own "Memory" column
and `GetProcessMemoryInfo`).

Baseline (cursor away, before any hover) showed no tooltip, confirming it is
hover-driven and not a stuck/permanent overlay at rest — corroborating the
packet's own note about the earlier uncontrolled-hover artifact.

**Independent result: consistent with the packet's own GUI walk and within
its claimed tolerances** (memory figures within ~1 MB or better, threads
exact, CPU time/uptime within the sampler's own lag).

## 6. Records

- `docs/spec-intakes/IN-0047-status-bar-resource-details/{IN-0047.md, high-level-design.md, US-0148-resource-tooltip.md}` — all three follow `docs/templates/{spec-intake,design,work}.md`; no unresolved `{{...}}` placeholders (checked). `US-0148-resource-tooltip.md`'s Documentation section names `docs/gui-layout.md` § Status bar and the `resource.rs` module docs as required updates; both are updated in the diff and match the shipped behaviour (verified in §2/§3).
- `docs/gui-layout.md` § Status bar: diff adds one sentence describing the hover table, field order, and colours (`git diff 416903e3 1099e298 -- docs/gui-layout.md`) — accurate against the code (§2/§3).
- `harness.db` (read-only; not modified): current state has `intake` at max `rowid=51`/`document_number=46` and `story` at `intake_id` up to 51 (`US-0144`/`BUG-0081` most recent). `IN-0047`/`US-0148` are not yet recorded there, consistent with the packet's own note ("Records written by hand; `harness.db` not touched"). **Proposed rows** (not inserted — read-only per instructions):

  `intake` (12 cols):

  | id | created_at | input_type | summary | risk_lane | risk_flags | affected_docs | story_id | doc_path | notes | document_number | design_doc |
  | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
  | 52 | 2026-09-28 | new_spec | "status bar resource details: hovering the CPU/MEM item shows Memory (private/working set/commit/peak) and CPU (usage/time/threads/uptime) tables (owner request 2026-09-28)" | normal | NULL | docs/gui-layout.md | US-0148 | docs/spec-intakes/IN-0047-status-bar-resource-details/IN-0047.md | "Acceptance rework in progress for US-0148 (frozen-tooltip content rejected by owner 2026-09-28)" | 47 | docs/spec-intakes/IN-0047-status-bar-resource-details/high-level-design.md |

  `story` (17 cols):

  | id | title | created_at | risk_lane | contract_doc | packet_doc | status | unit_proof | integration_proof | e2e_proof | platform_proof | evidence | verify_command | last_verified_at | last_verified_result | notes | intake_id |
  | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
  | US-0148 | CPU/MEM item shows a resource table on hover | 2026-09-28 | normal | docs/gui-layout.md | docs/spec-intakes/IN-0047-status-bar-resource-details/US-0148-resource-tooltip.md | **reopened** | 1 | 0 | 1 | 1 | docs/spec-intakes/IN-0047-status-bar-resource-details/evidence/{US-0148-gui-walk.md,US-0148-verify.md} | pwsh scripts/ci-local.ps1 | 2026-09-28 | NULL (interim — do not set pass/fail until the rework is re-verified) | "Owner rejected frozen-tooltip-content behaviour 2026-09-28; rework in progress on feat/status-bar-resource-tooltip to refresh on the 2 s cadence while open. Re-verify UI/tests/gate on the rework SHA, then set last_verified_result." | 52 |

  `status: reopened` reflects `docs/HARNESS.md`'s routing rule for acceptance
  feedback on work built but not yet accepted (`harness story reopen`), given
  the owner's rejection — not the packet's own self-reported `Implemented`.

## 7. Gate

Deleted any `target/release` before running (none was present in this
worktree). Ran the full gate with `CARGO_BUILD_JOBS=6` in this worktree's own
`target/` (no shared `CARGO_TARGET_DIR`):

```
pwsh scripts/ci-local.ps1
```

Final line: `ci-local: all checks passed.`

(Individually reran ahead of the full gate: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`python scripts/check-theme-contrast.py` — all passed, see §3.)

## Findings summary

| ID | Severity | Area | Summary |
| --- | --- | --- | --- |
| F1 | Major (non-blocking) | Performance | `thread_count()`'s added `TH32CS_SNAPPROCESS` walk roughly doubles a pre-existing, already-accepted synchronous UI-thread cost in the sampler (~5-8 ms → ~10-16 ms per 2 s sample on this machine); no cheaper sanctioned API exists, recommend moving the sampler off the UI thread as a fast follow. |
| F2 | Minor | Maintainability | `refresh_kind()`'s reliance on `ProcessRefreshKind::nothing()`'s implicit `tasks: true` default (for the Linux thread count) is undocumented at the call site; a future `.without_tasks()` cleanup would silently regress it with no CI coverage on this repo's platform. |
| F3 | Informational | Test evidence | The packet's own GUI walk never exercised "hover, then leave" (only "before hover" and "while hovering"); independently confirmed the tooltip does dismiss correctly with a graduated (non-warped) cursor move, so not a defect, but the walk's own coverage claim is narrower than the acceptance criterion it supports. |

## Gaps (as of the original 1099e298 review)

- §3's "content updates while open, no flicker, still dismisses on leave" was
  re-verified against the rework commit — see the section below. No longer a
  gap.
- Linux/macOS label paths (`RESIDENT_NAME`/`VIRTUAL_NAME`, threads `n/a`) are
  covered only by the unit test, as scoped in the intake's Validation Shape;
  not independently re-verified on those platforms here (no such runner
  available in this session). Still a gap after the rework — nothing in
  `88846c0e` touches those paths.
- Private working set has no fully independent (non-`GetProcessMemoryInfo`)
  OS source to cross-check against on Windows; noted in §5. Unchanged by the
  rework.

## 2026-09-28 acceptance rework re-verification (88846c0e)

Verifier: same agent/worktree, branch `verify/us-0148` moved from `1099e298`
onto `88846c0e` (`fix(us-0148): live-refresh the resource tooltip and move
its sample off the UI thread`, on `feat/status-bar-resource-tooltip`,
on top of `1099e298`).

Owner ruling that triggered this rework: "the information inside the tooltip
does not update with the interval." This is acceptance rework of the owning
`US-0148` (defect found before the story was accepted), not a new `BUG`, per
`docs/HARNESS.md`'s routing table — and the packet/intake docs record it that
way (`US-0148-resource-tooltip.md`'s new "Acceptance rework — 2026-09-28"
section, `IN-0047.md`'s follow-up-candidate reconciliation).

### (a) Live refresh

Read `crates/workspace/src/widgets/status_text.rs`: a new `DetailsTooltip`
struct is nested as the tooltip's content. Its constructor
(`DetailsTooltip::new`) calls `cx.observe(&source, |_, _, cx| cx.notify())`
on the `Entity<StatusText>` passed in, and its `Render` reads
`self.source.read(cx).details()` fresh on every render — so it re-renders
whenever `StatusText` notifies, which `StatusText::tick()` still does exactly
when the sampled `Label` changes (unchanged `if label != self.label { ...;
cx.notify() }` gate). Since `Uptime` is derived from `process.run_time()`
(whole seconds since process start) and the sampler ticks every 2 s, the
`Label` — and therefore `details()` — differs on essentially every tick in
practice, so the observer fires every ~2 s while the tooltip is open.

Confirmed this is not a leak: `Context::observe` (`gpui-pre` 0.3.3,
`app/context.rs:63-81`) registers a closure on the **watched** entity
(`StatusText`) that captures only a **weak** handle to the **observing**
entity (`DetailsTooltip`); the closure returns `false` when
`this.upgrade()` fails (observing entity dropped), which `observe_internal`
uses to prune the dead entry. So when the tooltip closes and `DetailsTooltip`
is dropped, at most one stale entry lingers on `StatusText`'s observer list
until the next `cx.notify()` (≤2 s later), then self-prunes — no unbounded
growth across repeated hovers. This mirrors an existing codebase pattern
confirmed by reading `crates/settings-ui/src/about.rs`
(`AboutUpdateControls::new`: `cx.observe(&updates::UpdateUiState::global(cx),
|_, _, cx| cx.notify()).detach()`), which the HLD diff also cites — not a
novel mechanism.

**Independent GUI walk** (own pid, private `USERPROFILE`, `fast-dev` build of
`88846c0e`, `PrintWindow(hwnd, dc, 2)`): moved the pointer onto the CPU/MEM
item once and held it motionless, capturing at t=0, t≈3 s (actual gaps
~3.2 s/3.1 s per the script's own timestamps), t≈6 s:

| Capture | Wall clock | Private WS | Working set | Commit | Usage | CPU time | Threads | Uptime |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| t0 | 11:31:21.992 | 51.9 MB | 112.9 MB | 145.2 MB | 0.0% | 0.5 s | 18 | 3.0 s |
| t3 | 11:31:25.169 | 52.8 MB | 114.0 MB | 145.9 MB | 0.2% | 0.5 s | 18 | 5.0 s |
| t6 | 11:31:28.309 | 53.2 MB | 114.4 MB | 118.4 MB | 0.2% | 0.8 s | 18 | 9.0 s |

Uptime and CPU time both advance monotonically across all three captures
while the pointer never moved off the item — the content is live, not frozen.
No flicker or visible jank observed between captures (screenshots:
`live-t0.png`, `live-t3.png`, `live-t6.png`, this walk's own scratch output;
the packet's own equivalent captures are committed at
`evidence/US-0148-live-t{0,3,6}-dark.png` and show the same pattern with
different absolute numbers). This independently corroborates the packet's own
rework evidence (`US-0148-gui-walk.md` § "Acceptance rework: the tooltip
stays live while open", pid 22316, Uptime 29.0s→33.0s→35.0s).

**Dismissal on leave**: a graduated cursor move (several intermediate
`SetCursorPos` steps across the window, per this review's earlier finding
that a single large warp does not reliably generate a leave event) correctly
dismissed the resource tooltip. A *different* status-bar item's own
"Click to copy" tooltip briefly appeared in the same capture because the
graduated leave path crossed over that other item on the way out — an
artifact of this walk's chosen exit path, not a resource-tooltip defect; the
resource table itself was gone as expected.

### (b) F1 — sampler moved off the UI thread

Read `crates/workspace/src/widgets/resource.rs`: `sample_label()` (the
`sysinfo` refresh, `os_memory_counters()`, and `thread_count()`) now runs
inside `cx.background_executor().spawn(async move { ... })`. The tick closure
itself only does:

```rust
if !in_flight.swap(true, Ordering::AcqRel) {
    // ...spawn the background sample, which stores into `slot` and
    // resets `in_flight` when done...
}
lock(&slot).clone()
```

— the same `AtomicBool` + `Mutex<Option<Label>>` stale-while-revalidate shape
`git_status.rs` already uses for its own background `git` calls (confirmed by
reading both files side by side).

- **No double-sampling when a sample takes longer than 2 s**: the `in_flight`
  flag is only cleared by the spawned task itself after it finishes; a tick
  that fires while a sample is still running sees `swap` return `true` and
  skips spawning a second one. Only one sample is ever in flight.
- **No lost wake-up**: `StatusText`'s own 2 s timer loop
  (`cx.spawn_in` → `window.background_executor().timer(interval).await` →
  `tick()`) is unconditional and does not wait on the background sample to
  complete — it just reads whatever is currently in `slot` every 2 s
  regardless. A slow sample means one or more ticks read a stale (but never
  wrong or torn) `Label`; the next tick after the sample completes picks up
  the fresh one. No signal to lose.
- **Foreground cost claim (implementer: 0.2-41.6 µs), checked with my own
  throwaway timer**: reproduced the exact shape (`AtomicBool::swap` +
  `Mutex<Option<Label>>` lock + clone) with a `Label` sized like the real
  indicator's (1 segment, 2 sections, 8 rows), 100,000 iterations, release
  build, outside the repo:

  ```
  n=100000 min=200ns median=300ns mean=314ns p95=500ns max=80µs
  ```

  The minimum (200 ns = 0.2 µs) matches the implementer's own lower bound
  exactly; the median/mean/p95 are all sub-microsecond, comfortably inside
  their claimed 0.2-41.6 µs range (their one 41.6 µs sample and my one 80 µs
  outlier both read as ordinary scheduler/cache-warmup jitter at this scale,
  not a systemic cost). **Confirmed: the foreground tick cost holds**, and is
  4-5 orders of magnitude below the ~10-16 ms this review's F1 measured for
  the pre-fix synchronous path.

F1 is resolved. (I did not re-measure the pre-fix cost at `88846c0e` since
that code path no longer exists there; the implementer's own before/after
table in `US-0148-gui-walk.md` — before 10.0-14.1 ms median ~11.2 ms, after
0.2-41.6 µs — reproduces this review's original finding.)

### (c) F2 — test and corrected comment

- `crates/workspace/src/widgets/resource.rs` tests now include
  `refresh_kind_keeps_tasks_on_for_the_linux_thread_count`, asserting
  `refresh_kind().tasks()`. Ran it (part of the full suite below): passes.
  `refresh_kind()`'s own doc comment now states the reliance explicitly.
- The `thread_count()` ponytail comment no longer names
  `NtQueryInformationProcess` as a viable escape hatch; it now reads "...
  revisit with a per-process (not whole-system) thread-count query if the
  background cost ever shows up in a profile — `NtQueryInformationProcess`
  does not give one, so it is not that call," which is technically accurate
  (confirmed: that call returns `PROCESS_BASIC_INFORMATION`, no thread
  count) and matches this review's F1 write-up exactly.

F2 is resolved.

### (d) Tests, contrast, and the full gate at 88846c0e

- `cargo test -p oneterm-workspace --lib`: **42 passed, 0 failed, 3 ignored**
  (the same unrelated elevation tests; one more pass than the 1099e298 run,
  the new F2 test).
- `python scripts/check-theme-contrast.py`: passed — `1482 foreground/surface
  pairings across 390 token/variant rows, all >= 4.5:1; primary text
  out-reads muted.foreground on all 585 shared-surface comparisons`
  (unchanged from 1099e298; no new tokens or surfaces).
- Grepped `resource.rs`/`status_text.rs` for hardcoded colours
  (`hsla(`/`rgb(`/`rgba(`): none.
- Full gate, `CARGO_BUILD_JOBS=6`, this worktree's own `target/`, no
  `target/release` present beforehand:

  ```
  pwsh scripts/ci-local.ps1
  ```

  Final line: `ci-local: all checks passed.`

### (e) Updated harness rows

Supersedes the interim proposal in §6 above (still read-only; `harness.db`
was not modified). `intake` row is unchanged from §6 except `notes`. `story`
row: `status` → `implemented` (the rework is independently re-verified as
PASS, so this is no longer acceptance-in-progress), `last_verified_result` →
`pass`.

`intake` (12 cols):

| id | created_at | input_type | summary | risk_lane | risk_flags | affected_docs | story_id | doc_path | notes | document_number | design_doc |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 52 | 2026-09-28 | new_spec | "status bar resource details: hovering the CPU/MEM item shows Memory (private/working set/commit/peak) and CPU (usage/time/threads/uptime) tables (owner request 2026-09-28)" | normal | NULL | docs/gui-layout.md | US-0148 | docs/spec-intakes/IN-0047-status-bar-resource-details/IN-0047.md | "Acceptance rework (frozen-tooltip content rejected by owner 2026-09-28, fixed and re-verified at 88846c0e)" | 47 | docs/spec-intakes/IN-0047-status-bar-resource-details/high-level-design.md |

`story` (17 cols):

| id | title | created_at | risk_lane | contract_doc | packet_doc | status | unit_proof | integration_proof | e2e_proof | platform_proof | evidence | verify_command | last_verified_at | last_verified_result | notes | intake_id |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| US-0148 | CPU/MEM item shows a resource table on hover | 2026-09-28 | normal | docs/gui-layout.md | docs/spec-intakes/IN-0047-status-bar-resource-details/US-0148-resource-tooltip.md | **implemented** | 1 | 0 | 1 | 1 | docs/spec-intakes/IN-0047-status-bar-resource-details/evidence/{US-0148-gui-walk.md,US-0148-verify.md} | pwsh scripts/ci-local.ps1 | 2026-09-28 | **pass** | "Acceptance rework (owner rejected frozen-tooltip-content 2026-09-28) independently re-verified at 88846c0e: live refresh via DetailsTooltip/cx.observe confirmed, no leaked observer; F1 sampler moved to cx.background_executor() confirmed, foreground cost 0.2-41.6 µs matches implementer's claim; F2 test + corrected ponytail comment confirmed; 42/0/3 unit tests, contrast gate, and full ci-local.ps1 all pass." | 52 |

### Verdict: PASS

All items (a)-(e) confirmed. No outstanding correctness or acceptance defect.
Remaining gaps are unchanged from the original review (Linux/macOS platform
coverage, private-WS cross-check) and are informational, not blocking — see
"Gaps" above.
