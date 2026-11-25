# US-0142 independent verification

- Date: 2026-09-25
- Commit under test: `d2c24359` (branch `feat/in-0046-hotpath-profiling`, one commit on main
  `d27f06ca`; merges cleanly into main `8c0a6bd7` = v0.6.5, `git merge-tree` exit 0)
- Packet: [`US-0142`](../US-0142-hotpath-feature.md); intake [`IN-0046`](../IN-0046.md);
  design [`high-level-design.md`](../high-level-design.md)
- Host: Windows 11 Enterprise 10.0.26200, toolchain 1.96.0, worktree's own `target/`,
  `CARGO_BUILD_JOBS=4`. No app instance was launched (no GUI step in this verification).

## Verdict: PASS (documentation findings F1-F3, none blocking; F4-F5 informational)

Every code claim holds. Without the feature `hotpath` is absent from the graph and the
release binary is byte-for-byte the size of main's with 92 differing bytes, reproduced
exactly. The alloc build keeps `OomResilientAlloc` as the inner allocator through a
pass-through `CountingAllocator`. `oneterm-vt` keeps R7 and its `--no-default-features`
contract, and the full gate plus `cargo deny` is green. The findings concern the records:
the site count is off by one, the `oneterm-vt` README's feature table was not updated, and the
packet contradicts itself about `THIRD-PARTY-NOTICES.md`.

## Evidence

### 1. Zero cost when off

- `cargo tree -p oneterm-app -e normal -i hotpath`: `package ID specification 'hotpath' did not
  match any packages`. `cargo tree -p oneterm-app -e features | Select-String hotpath`: 0 lines.
  `cargo tree --workspace -e normal,build | Select-String hotpath`: 0 lines.
- With `--features hotpath-profiling`, `cargo tree -i hotpath --depth 1` lists all seven
  dependents: app, highlight, local-shell, ssh, terminal, terminal-view, vt. The fan-out is
  complete.
- Release `oneterm.exe` built in this worktree: `d2c24359` = **45,976,064 B**, `d27f06ca` =
  **45,976,064 B**, **92 differing bytes** (byte-wise compare). This matches the packet's
  figures exactly. `hotpath` occurs 0 times in the branch binary's bytes.
- Macro sites: every site is `#[cfg_attr(feature = "hotpath-profiling", ...)]`. Without the
  feature the attribute is removed before expansion, so no runtime branch exists. With a leaf
  feature alone, `hotpath-macros-0.26.1/src/lib_off.rs` `measure_impl` / `main_impl` return
  `item` unchanged.

### 2. Allocator

- `hotpath-macros-0.26.1/src/lib_on.rs:421-425`: with `hotpath-alloc` (and without
  `hotpath-alloc-meta`, which OneTerm does not enable) `main` emits `#[global_allocator] static
  __HOTPATH_GLOBAL_ALLOCATOR: hotpath::CountingAllocator<oom::OomResilientAlloc> =
  CountingAllocator::with(oom::OomResilientAlloc)`.
- `hotpath-0.26.1/src/lib_on/functions/allocator.rs:41-89`: `alloc`, `dealloc`, `alloc_zeroed`
  and `realloc` forward to the inner `A` unchanged. Only thread-local counters are touched.
  `OomResilientAlloc`'s ballast release and retry loop (`oom.rs`) therefore stay on the path
  of every allocation.
- `crates/app/src/lib.rs:32-34`: `GLOBAL_ALLOC` is `#[cfg(not(feature =
  "hotpath-profiling-alloc"))]`. The timing-only feature keeps it, and it is compiled out only
  where hotpath declares the allocator. That the build compiles at all proves there is exactly
  one `#[global_allocator]` in each configuration.
- `cargo test -p oneterm-app` with no feature, `--features hotpath-profiling` and `--features
  hotpath-profiling-alloc`: each run gives `26 passed; 0 failed`, exit 0.
- `cargo clippy --workspace --all-targets --features oneterm-app/hotpath-profiling-alloc -- -D
  warnings`, the same with `oneterm-app/hotpath-profiling`, and `cargo clippy -p
  oneterm-terminal-view --all-targets --features hotpath-profiling -- -D warnings` (the leaf
  alone, which runs `lib_off`, including the `HotpathGuardBuilder` in `element_tests.rs`): all
  exit 0.

### 3. Dependency policy

- R7: `cargo tree -p oneterm-vt -e normal --no-default-features --features hotpath-profiling`
  adds only `hotpath` and `hotpath-macros`. The macros crate is proc-macro only, and without
  hotpath's `hotpath` feature it has no `syn`/`quote`. hotpath's default feature `threads = []`
  adds nothing. No platform crate is added, and no OneTerm crate. The gate's `cargo tree -p
  oneterm-vt -e normal --no-default-features` six-leaf step passed.
- Gate steps `cargo build/test -p oneterm-vt --no-default-features`, `--all-features
  --examples`, `cargo doc --all-features` with `-D warnings`, `vt-public-api.py --check
  --no-doc` / `--check-nameable` / `--diff-platforms`, the package-list check and the rustdoc
  self-containment grep: all passed. The vt source diff adds attribute lines only and no doc
  comment.
- `deny.toml` sets `[graph] all-features = true`, so the gate's `cargo deny check licenses bans
  advisories` covers hotpath's timing and alloc graph: `advisories ok, bans ok, licenses ok`.
- `scripts/third-party-notices.py` runs `cargo metadata --locked` without `--all-features` and
  walks the resolved graph from `oneterm-app`. An optional dependency that no default feature
  enables is not in `resolve.nodes[].deps`, so omitting hotpath follows the script's stated
  scope ("reachable from the shipped binary"). `--check` passed.
- `verify-dependency-graph.py` (workspace inheritance: every manifest uses `workspace = true`)
  passed.

### 4. Instrumentation correctness

- 17 `hotpath::measure` sites plus `hotpath::main` on `run()`. Each site is an attribute
  only; no signature changed.
- `pty_read` (`crates/local-shell/src/event_loop.rs:645-648`) is `reader.read(buf)`, called
  with the same `self.pty.reader()` and the same slice as the inline code it replaces. It
  contains no `?` and no early return, and the `match` arms at the call site are unchanged.
  It is generic (`&mut impl Read`), so it monomorphises to the same concrete reader type
  (`fn reader(&mut self) -> &mut Self::Reader`) and inlines. The identical release size is
  consistent with that.
- One async site, `copy_sequential`. hotpath's `measure` supports async functions (timing
  wraps the future). The workspace clippy with the feature compiles it, so no `Send`
  regression exists in `oneterm-ssh`'s spawn sites. Its `N/A` in allocation mode and its
  I/O-bound timing are documented in the evaluation doc (§ 1 and § 7) and in the packet's
  Gaps section.
- `run()` returns normally (the gpui `run` closure is the last statement), so the guard drops
  and the report is written. The hotpath metrics server binds `127.0.0.1` only
  (`metrics_server.rs:81`), and when the port is busy it skips the server rather than failing.

### 5. Records

- Intake, HLD and packet follow `docs/templates/{spec-intake,design,work}.md` heading for
  heading. All are dated 2026-09-25. Candidates US-0143..US-0147 are listed in the intake and
  ranked in the evaluation doc's § 5.
- Spot checks against `research/raw/*.json`:
  - `time-flood.json`: `Pump::advance` 298,706 calls, avg 1.28 µs, total 382.79 ms;
    `TerminalElement::prepaint` 969 calls, 995.76 µs. UI thread 16.6 %, `PTY owner` 7.3 %,
    conout 4.8 %. All match § 4.2.
  - `alloc-count-flood.json` / `alloc-bytes-flood.json`: `Parser::advance` 300,012
    allocations, 203.7 MB, avg 715 B (p50 712 B); `scan_line_into` 130,348 / 44.6 MB. Both
    match.
  - `alloc-bytes-tui.json`: `scan_line_into` 170.1 MB. Matches.
  - `frame-time.txt`: flood avg off 1576/1565/1565 µs, on 1747/1724/1775 µs. That is a mean of
    1568.7 → 1748.7 µs, **+11.47 %**. Matches.

### 6. Gate

`$env:CARGO_BUILD_JOBS=4; pwsh scripts/ci-local.ps1 -Full` at `d2c24359`, run after the
worktree's `target/release` was deleted: exit 0, 135 `test result: ok` lines, 0 failed. The
last step, `cargo deny check licenses bans advisories`, printed `advisories ok, bans ok,
licenses ok`. Final line:

```
ci-local: all checks passed.
```

## Findings

### F1 (low, docs): the site count is 17, not 16

`research/hotpath-evaluation.md:30` ("Sixteen sites in six crates") and the commit message
("Sixteen sites") give the wrong count. The HLD diagram lists 17 sites, and so does a search
for `hotpath::measure`:

- vt: `Terminal::feed`, `Terminal::snapshot_update`, `Parser::advance`, `Screen::scroll_up`,
  `Screen::scroll_down`
- terminal: `Pump::advance`, `TerminalContent::refill`
- terminal-view: `PlanCache::update`, `class_rows_into`, `build_row_plan`,
  `GlyphCache::shape`, `TerminalElement::prepaint`, `TerminalElement::paint`,
  `GridPainter::paint`
- highlight: `scan_line_into`
- local-shell: `pty_read`
- ssh: `copy_sequential`

Fix: "Seventeen" in the evaluation doc.

### F2 (low-medium, external contract docs): `crates/vt/README.md` feature table not updated

`oneterm-vt` is consumed by git dependency, and README § Features (lines 196-206) is the
consumer's list of features. It still lists only `pty`, `vt-paranoid` and `regex`, and says
"Two add dependencies". `hotpath-profiling` is now a third. CHANGELOG promise 5 says a
non-default feature carries the full promise. Only the CHANGELOG was updated; the packet's
Documentation Action does not mention the README. Fix: add a `hotpath-profiling | off |` row
(it adds `hotpath` and `hotpath-macros`, measures nothing unless the final binary enables
`hotpath/hotpath` and starts a guard) and make "Two" read "Three".

### F3 (low, records): the packet contradicts itself on `THIRD-PARTY-NOTICES.md`

Scope says "`THIRD-PARTY-NOTICES.md` regenerated", and Documentation Action says "Update
required: … `THIRD-PARTY-NOTICES.md` (generated)". Reconciliation and Evidence say correctly
that it needed no change. Fix: drop it from Scope and Documentation Action.

### F4 (info): the `oneterm-vt` public contract in a normal lane

`AGENTS.md` treats public contracts as high-risk. The lane choice is defensible: a new
default-off feature is a patch bump under CHANGELOG promise 2, and no public item changed
(the `vt-public-api.py` checks pass). The feature name is now part of the promise, though,
and it pins vt's view of hotpath to 0.26. An embedder that enables `oneterm-vt/hotpath-profiling`
with a different hotpath major gets two hotpath copies, and vt's sites silently report
nothing. The packet should mention this, or the README row from F2 can carry it.

### F5 (info, measurement): a per-line site nested in the parse timing

The HLD says to keep sites at batch granularity. `Screen::scroll_up` runs once per scrolled
line (299,967 calls in the flood) inside `Parser::advance`, so the profiling build's
`Parser::advance` average (0.95 µs) includes one nested measure per line. US-0143's estimate
("-5 to -10 % of `Parser::advance`") therefore rests on an instrumented baseline. US-0143
should measure its gain with the `scroll_up` site removed, or on the uninstrumented
`vt` bench.

## Gaps

- No profiling build was launched here. That the app starts with the feature, and that the
  report names every site, rests on the committed `research/raw/*.json` (all reports are
  `hotpath` 0.26.1 JSON with `oneterm_app::run` as the caller).
- The SFTP load (d) was not run by the author, and so `copy_sequential` has never been
  observed in a report. That was already stated in the packet.
- Release-build per-site overhead is not measured (the author's § 7). It was not re-measured
  here.
