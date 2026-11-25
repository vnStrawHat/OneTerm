# US-0137 independent verification

- Date: 2026-09-25
- Commit under test: `a3334841` (branch `feat/status-bar-private-working-set`, one commit on
  main `de845969`)
- Packet: [`US-0137`](../US-0137-status-bar-private-working-set.md)
- Host: Windows 11 Enterprise 10.0.26200, release build, private `USERPROFILE`, own pid only

## Verdict: FAIL (documentation only)

The code is correct. The status bar matches the private working set within 0.5 MB, the
fallback covers both a failed call and an unfilled field, the tests catch every swapped
or dropped branch, and the gate is green. It fails on F1: the Windows version where the
fallback starts is wrong in the owning doc (`docs/gui-layout.md`), in the code comments and
in the packet. The fix is a text change in five places. No code or test changes are needed.

## Findings

### F1 (blocking, docs): the "Windows before 10 21H1" boundary is wrong

Microsoft's reference for `PROCESS_MEMORY_COUNTERS_EX2`
(learn.microsoft.com, `ns-psapi-process_memory_counters_ex2`, Requirements) gives the
minimum client as **"Windows 10 22H2 with September 2023 cumulative update or Windows 11
22H2 with September 2023 cumulative update"**. The Windows SDK agrees:
`Include\10.0.22621.0\um\Psapi.h` declares the struct only under
`#if (NTDDI_VERSION >= NTDDI_WIN10_NI)` (NI = 22H2). Nothing supports "10 21H1". A
Windows 10 21H1/21H2 host, or a 22H2 host without the September 2023 update, takes the
fallback and shows the full working set (about 136 MB idle on this host instead of 51 MB).
The docs say it shows the private working set.

The claim appears in:

- `docs/gui-layout.md:149` ("Windows before 10 21H1"), the owning contract
- `crates/workspace/src/widgets/resource.rs:28` (module doc "Windows 10 21H1+"), `:121`
  (SAFETY comment), `:171` (test comment)
- the packet, lines 38 and 152

Fix: replace it with the Microsoft wording, for example "Windows 10/11 22H2 with the
September 2023 update or later", or name no version ("where Windows does not fill the
field").

### F2 (minor, packet): "`Cargo.lock` unchanged" is not true

The packet's Context says "no new crate, `Cargo.lock` unchanged". The commit adds a line
to `Cargo.lock` (`"windows-sys 0.59.0"` in `oneterm-workspace`'s dependency list). No
new crate is added, so the first half is right. Change it to "no new crate; one new
dependency edge in `Cargo.lock`".

### F3 (minor, comment): the SAFETY comment describes only one of the two fallback paths

The comment at `resource.rs:119-122` says an older Windows "leaves
`PrivateWorkingSetSize` at 0". Depending on the build, an older kernelbase may instead
reject the larger `cb` and **fail** the call (for example with
`ERROR_INSUFFICIENT_BUFFER`). The code already covers that path:
`(ok != 0).then_some(..)` gives `None`, and `displayed_memory(None, resident)` returns
the resident figure (tested). Only the comment is incomplete. Fold it into the F1 edit.

## 1. FFI correctness

- **Struct layout.** windows-sys 0.59.0
  (`src/Windows/Win32/System/ProcessStatus/mod.rs:126-140`) has `cb: u32`,
  `PageFaultCount: u32`, nine `usize` fields (PeakWorkingSetSize .. PrivateUsage),
  `PrivateWorkingSetSize: usize` and `SharedCommitUsage: u64`, all `#[repr(C)]`. This is
  field for field the SDK `Psapi.h` declaration (`DWORD`, `DWORD`, 10 x `SIZE_T`,
  `ULONG64`). On x64 that is 96 bytes.
- **`cb`.** `size_of::<PROCESS_MEMORY_COUNTERS_EX2>()` is written to both `counters.cb`
  and the `cb` argument, so the EX2 size is passed. The pointer is cast to
  `*mut PROCESS_MEMORY_COUNTERS`, which is the binding's parameter type (the documented
  pattern for the EX variants).
- **Older Windows.** Both outcomes lead to the resident figure: a failed call gives
  `None` (`ok != 0` check), and a zero field gives `Some(0)`, which
  `displayed_memory` filters. The struct is zero-initialised, so an unfilled field
  really is 0. See F3 for the comment.
- **Handle.** `GetCurrentProcess()` pseudo-handle (-1). It is not closed, which is
  correct. It has full access to the process itself.
- **Unsafe.** Two `unsafe` blocks, both inside the `#[cfg(windows)]` body:
  `mem::zeroed()` for a plain-integer struct (sound) and the call itself. Both have
  `SAFETY` comments. No other unsafe code was added.
- **Symbol and feature.** `Cargo.lock` pins `windows-sys 0.59.0`. `Win32_System_ProcessStatus`
  gates `pub mod ProcessStatus` (`src/Windows/Win32/System/mod.rs:83-84`). That module
  links `GetProcessMemoryInfo` from `psapi.dll` (line 22; psapi forwards to
  `K32GetProcessMemoryInfo` in kernel32) and defines the struct. `GetCurrentProcess`
  comes from `Win32_System_Threading`, which was already on. The feature is added once, in
  the root `[workspace.dependencies]`, and the crate inherits it
  (`windows-sys.workspace = true`, under `[target.'cfg(windows)'.dependencies]`).
- **Dependency rules.** No R1-R12 rule is broken. R7 limits `windows-sys` only in `vt`
  (only through `pty`) and forbids it in `oneterm-terminal`. `oneterm-workspace` is the
  shell layer, and R4 limits it only to no `*-ui` or backend crates. No cycle (R1), and the
  edge points down to a third-party leaf (R2). `python scripts/verify-dependency-graph.py`
  (crate graph policy and workspace version inheritance) passed in the gate below.
- **`cfg(windows)` rule** (`docs/agents/code-style.md`). The gate is on the FFI body
  only. `private_working_set()` exists on every OS (`None` off Windows), and
  `displayed_memory` is pure and not gated. It has no `allow(dead_code)`, and its tests
  run on all three CI runners.

## 2. Semantics and test strength

`displayed_memory(pws, resident) = pws.filter(>0).unwrap_or(resident)`: it uses the private
working set when one is present and not zero, and the resident figure otherwise. Commit
is never shown: `git grep 'virtual_memory('` over `crates/` finds only the module doc
sentence that says commit is not shown.

Mutation check (source edited, the two tests run with
`cargo test -p oneterm-workspace --lib displayed_memory`, source restored, tree clean):

| Mutant | Result |
| --- | --- |
| always resident (branches swapped) | FAILED: `displayed_memory_prefers_the_private_working_set` |
| `.filter(>0)` removed | FAILED: `displayed_memory_falls_back_to_resident_when_unavailable` (`Some(0)` case) |
| `unwrap_or(0)` instead of resident (private only) | FAILED: `displayed_memory_falls_back_to_resident_when_unavailable` (`None` case) |

Every mutant is killed, so the tests discriminate.

## 3. Measurement

Release build of `a3334841` (`cargo build -p oneterm-app --release`, shared target). A
copy was renamed `oneterm-us0137v.exe` so its perf-counter instance name is unique. It was
launched with a private `USERPROFILE`/`HOME` (update check off), window 1280x800, one cmd
tab, own pid 15036 only. For each sample the bar was read from a `PrintWindow` capture,
and the counters for the same pid were read right after it. The flood was
`cmd /c "for /l %i in (1,1,300000) do @echo line %i"` (it ran in 15 s, and the screen
ended at `line 300000`), followed by 20 s of idle time. All values are MiB (2^20), as the
bar uses.

| Sample | Time | Status bar `MEM` | `\Process(oneterm-us0137v)\Working Set - Private` | EX2 `PrivateWorkingSetSize` (external read) | `WorkingSet64` | `PrivateMemorySize64` (commit, the old figure) |
| --- | --- | --- | --- | --- | --- | --- |
| idle 1 (30 s) | 13:56:59 | 51.3 MB | 50.8 | 50.8 | 136.6 | 183.3 |
| idle 2 (+5 s) | 13:57:05 | 50.3 MB | 50.4 | 50.4 | 136.1 | 183.3 |
| after 300k lines 1 | 13:57:45 | 85.0 MB | 85.1 | 85.1 | 171.2 | 200.2 |
| after 300k lines 2 (+5 s) | 13:57:51 | 85.1 MB | 85.1 | 85.1 | 171.1 | 200.2 |

The bar agrees with the private working set counter within 0.5 MB, the size of one 2 s
tick of drift. It is about 85 MB below the full working set and 115-133 MB below commit.
This matches the implementer's figures (51.4 vs 51.6, 85.5 vs 85.4).

Task Manager: the "Processes" tab "Memory" column and the "Details" tab "Memory (active
private working set)" column report the private working set. That is the quantity of
the `Working Set - Private` counter and of `PrivateWorkingSetSize`, which agree exactly
above. Task Manager itself was not screen-read. That would mean driving a window this
session did not launch.

Cleanup: only pid 15036 and its direct children were stopped. The owner's `oneterm.exe`
(pid 12948, `dist\`) was not touched, and neither was an unrelated `scratchpad\fix\oneterm.exe`
started by another session.

## 4. Documentation and records

- The packet follows `docs/templates/work.md`: Created 2026-09-25, all sections present,
  owning docs reviewed (gui-layout, IN-0045, memory-attribution, code-style,
  crate-dependency-rules, `d6270811`, US-0112), documentation action and reconciliation
  written, evidence and gaps listed (the old-Windows fallback is proven by the unit test
  only; Linux and macOS were not run). There is no `Handoff` section, which is optional.
  See F1 and F2.
- `docs/gui-layout.md` § Status bar names the figure for each platform: Windows private
  working set, else the working set; Linux and macOS RSS (sysinfo `memory()` is RSS on
  both). It is accurate except for the version boundary (F1).
- `IN-0045.md`: the `US-0137` line and the owner question are closed and consistent.

## 5. Gate

`pwsh scripts/ci-local.ps1` with `CARGO_TARGET_DIR=D:\TrungKFC-Research\Rust\myTerm2\target`
and `CARGO_BUILD_JOBS=4`, with a temporary junction from `<worktree>\target` to the shared
target (for `vt-public-api.py`), which was removed afterwards. Exit 0; final line:

```
ci-local: all checks passed.
```

## Gaps

- Linux and macOS were not run. Their path is `private_working_set() = None`, which leads
  to `memory()`, and that path is unit-tested on every runner.
- No pre-22H2 or pre-September-2023 Windows was available, so which fallback path it takes
  (call fails or field 0) was not observed. Both paths are tested.
- Task Manager was not read directly (see section 3).
