# Work: the pseudo-console transport moves into the core behind a default-on feature

ID: US-0104
Intake: IN-0038
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (a crate is deleted and its API re-homed; `oneterm-vt`'s
  public surface and dependency contract both change)
- Risk lane: high_risk. It changes the crate graph, it changes what `oneterm-vt` depends on by
  default, and it puts a process-spawning, `unsafe`-heavy, platform FFI module inside the crate whose
  public API `US-0097` made an external contract.
- Spec Intake: `IN-0038`

## Outcome

`crates/pty` no longer exists. Its 2 759 lines live at `crates/vt/src/pty/` behind a cargo feature
`pty` that is **on by default**, and `crates/local-shell` and `crates/tools` reach them through
`oneterm_vt::pty`. `cargo build -p oneterm-vt --no-default-features` still resolves to exactly six
leaf dependencies and still compiles an engine with no threads, no platform code and no transport.

OneTerm's local shell behaves identically: same ConPTY host resolution, same `DEC-0016` grace
period, same poll loop, same bytes.

This implements the owner ruling of 2026-09-15, which reverses intake decision (f) **for the PTY
only**. Clipboard backends, `TerminalSecurityPolicy`, URL policy and GPUI stay outside the core.

## Scope

- [ ] In scope: the file move `crates/pty/src/**` -> `crates/vt/src/pty/**`; the `pty` feature and
  the three optional dependencies in `crates/vt/Cargo.toml`; `pub mod pty` in `crates/vt/src/lib.rs`;
  the `use` lines in `crates/local-shell` (4 files) and `crates/tools` (1 file) and their manifests;
  root `Cargo.toml` (`members`, `[workspace.dependencies]`, `[profile.fast-dev.package]`);
  `scripts/dependency-graph-policy.json`; `deny.toml`; the CI `--no-default-features` step and its
  `cargo tree` assertion; `docs/agents/crate-dependency-rules.md` (layer line, R6, R7, R8, the
  verification block), `docs/agents/structure.md` (tree + three table rows),
  `docs/agents/dependencies.md`, `docs/architecture.md`, `docs/PROJECT.md`,
  `docs/terminal-backend.md`; the path reference inside `DEC-0016`.
- [ ] Out of scope: any change to what the transport **does**. Not one function body changes. A
  behaviour change discovered mid-move is a new packet, not a fold-in.
- [ ] Out of scope: the `EventedReadWrite` signature redesign that would remove `polling` from the
  public API. Decided against with a named trigger in
  [`low-level-design/pty.md`](low-level-design/pty.md), "Considered and rejected".
- [ ] Out of scope: shipping the bundled ConPTY pair inside the packaged crate. Decided against in
  [`low-level-design/pty.md`](low-level-design/pty.md), "Packaging decision", and in
  [`low-level-design/packaging.md`](low-level-design/packaging.md).
- [ ] Out of scope: guide chapter 13 itself. It is `US-0103`'s file; this packet only adds it to
  that packet's outline and gate list.
- [ ] **In** scope, consequently, three things `US-0097` turned into gates before this packet
  existed. All are mechanical and all fail CI if skipped:
  - `#![warn(missing_docs)]` coverage for the moved module -- about 30 doc lines (`Shell::new`,
    several `Options` and `WindowSize` fields).
  - The **rustdoc self-containment grep**. `crates/pty/src` carries **13** `///` / `//!` lines
    citing `DEC-0013`, `DEC-0016`, `BUG-0055`, `CORR-10` or
    `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md`, in `lib.rs`, `windows/child.rs`,
    `windows/conpty.rs` and `windows/pipe_tests.rs`. Inside `crates/vt/src` they fail the grep
    `US-0097` added to `ci-local`. They go by the HLD's three rules: most drop from `///` to plain
    `//`, and the two module-level "Design:" lines become absolute links to this repository.
  - `crates/vt/public-api.txt` and `python scripts/vt-public-api.py --check --no-doc`. `pub mod pty`
    adds roughly 13 public paths; the committed surface file is regenerated in the same commit so
    the diff is the reviewable record of what the module exposes.

## Acceptance

Every criterion is a command a verifier who distrusts this packet can run from the repository root.

- [ ] `cargo build -p oneterm-vt` (default, `pty` on) exits 0.
- [ ] `cargo build -p oneterm-vt --no-default-features` exits 0.
- [ ] `cargo build -p oneterm-vt --all-features` exits 0.
- [ ] `cargo tree -p oneterm-vt -e normal --no-default-features` prints exactly 7 lines: the crate
  plus `bitflags`, `log`, `memchr`, `rustc-hash`, `unicode-segmentation`, `unicode-width`. Pasted
  into Evidence verbatim.
- [ ] `cargo tree -p oneterm-vt -e normal` on `x86_64-pc-windows-msvc` shows 8 direct dependencies
  and 16 distinct crates; on `x86_64-unknown-linux-gnu` (`--target`) 8 direct and 11 distinct. Both
  pasted into Evidence. No `gpui*` and no `oneterm-*` in either.
- [ ] `cargo test -p oneterm-vt --features pty` runs the loopback tests moved from
  `crates/pty/src/loopback_tests.rs`, and
  `cargo test -p oneterm-vt --features pty -- --list | grep -c 'pty::loopback_tests'` is the same
  count as `cargo test -p oneterm-pty -- --list | grep -c 'loopback_tests'` on `main` @ `92ae9a6`.
  Same for `pty::windows::pipe_tests` and `pty::windows::pseudo_console_tests` on Windows.
- [ ] `cargo test -p oneterm-vt --no-default-features` is green: the whole engine suite passes with
  no transport compiled. **This replaces the "bring your own transport test with the feature off"
  the brief asked for** -- see Gaps for why that criterion cannot be written as stated.
- [ ] The workspace has no `oneterm-pty`. All three print nothing:
  ```bash
  test ! -d crates/pty && echo gone
  grep -rn 'oneterm.pty\|oneterm_pty' --include='*.rs' --include='*.toml' --include='*.md' \
    --include='*.json' --include='*.ps1' --include='*.sh' --include='*.yml' \
    crates/ docs/ scripts/ .github/ Cargo.toml deny.toml \
    | grep -v 'docs/spec-intakes/IN-0029' | grep -v 'docs/spec-intakes/IN-0038'
  cargo metadata --no-deps --format-version 1 | grep -o '"oneterm-pty"'
  ```
  The two `grep -v` exclusions are the historical record: `IN-0029`'s evidence files and this
  intake's own documents name the crate as history and must not be rewritten.
- [ ] `cargo package -p oneterm-vt --list | grep -i -e conpty -e openconsole` prints nothing
  **except** the source file `src/pty/windows/conpty.rs`, and the same list contains
  `src/pty/mod.rs`, `src/pty/unix.rs` and `src/pty/loopback_tests.rs`. No Microsoft binary appears.
- [ ] The packaging gate `US-0097` shipped still passes, **with the `pty` module in the list and
  nothing excluded to make it pass**:
  ```bash
  cargo package -p oneterm-vt --list | python scripts/verify-dependency-graph.py --package-list -
  ```
  There is nothing to exclude: `crates/pty` holds no binary, only the loader, and the bundled ConPTY
  pair lives in `crates/app/assets/`, which `cargo package` never walks. The required files
  (`README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE`, `examples/headless.rs`) are untouched, the
  "reaches nothing outside `crates/vt`" rule is satisfied because every added path is under
  `crates/vt/src/pty/`, and the 150-character path bound is not close: the longest added path,
  `crates/vt/src/pty/windows/pseudo_console_tests.rs`, is 48.
- [ ] `cargo package -p oneterm-vt` (the build form, not `--list`) still exits 0 -- it compiles the
  packaged crate out of tree, which is where a missing `#[cfg(feature = "pty")]` would surface.
- [ ] `python scripts/verify-dependency-graph.py` passes against the edited
  `scripts/dependency-graph-policy.json`, and that file contains no `oneterm-pty`.
- [ ] `python scripts/third-party-notices.py --check` passes **with no regeneration**: the bundle and
  its manifest did not move, and `polling` / `windows-sys` / `libc` were already in the app graph.
- [ ] OneTerm's local shell is unchanged: `cargo test -p oneterm-local-shell` green, including
  `session_orphan_tests` (the `DEC-0016` escalation probe) and `event_loop_tests`.
- [ ] Ten-launch probe from `IN-0031`: open and close ten local shells in the running app and confirm
  no orphan `cmd.exe` and no orphan `OpenConsole.exe` remain, and that the log line
  `conpty: bundled` still appears -- i.e. the bundled host still resolves from the executable's
  directory after the source moved crate. Cheap, and it is the only check that proves the loader's
  runtime path resolution survived.
- [ ] `docs/terminal-backend.md` and `docs/agents/structure.md` name `oneterm_vt::pty`, not
  `oneterm-pty`, and `python scripts/check-doc-paths.py` passes (it checks both files).
- [ ] LOC: `git diff --stat main` shows the move as a rename. Production delta is net zero plus the
  manifest, module header and `missing_docs` lines; the number is pasted into Evidence with the
  dependency-count table.
- [ ] The self-containment grep returns **0** lines over `crates/vt/src` (baseline for the moved
  files on `main` @ `92ae9a6` is **13**, in `lib.rs`, `windows/child.rs`, `windows/conpty.rs` and
  `windows/pipe_tests.rs`), and `python scripts/vt-public-api.py --check --no-doc` passes against a
  regenerated `crates/vt/public-api.txt` whose diff is only the `pty` paths.
- [ ] `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features` exits 0 with
  `missing_docs` in force.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/agents/crate-dependency-rules.md` -- **R7 is the rule this packet invalidates.** It says
  `pty` and `vt` "depend on no OneTerm crate", and it says the engines are platform-free by
  implication. The replacement text for the L0 paragraph, R6, R7, R8 and the verification block is
  written out in [`low-level-design/pty.md`](low-level-design/pty.md), "Rule changes".
- `docs/agents/structure.md` -- the directory tree, the `pty` / `vt` / `local-shell` / `tools` rows.
- `docs/agents/dependencies.md` -- the "Local shell PTY" row, the `oneterm-vt` row, the "Windows
  FFI" row and the `### oneterm-pty's direct dependencies (US-0071)` subsection.
- `docs/architecture.md` -- the "PTY transport" and "Backend" rows name `crates/pty/src/*`.
- `docs/PROJECT.md` -- "local PTY via `oneterm-pty` (`crates/pty`...)".
- `docs/terminal-backend.md` -- the local read loop and the transport it drives.
- [`DEC-0013`](../../decisions/DEC-0013-bundled-conpty-host-and-bump-script.md) -- the bundled pair,
  its bump script and its manifest. **Reviewed, no change**: the assets, `crates/app/build.rs`,
  `scripts/bump-conpty.ps1` and the notice pipeline are all in `crates/app`, and the loader resolves
  the DLL against the running executable's directory, not against the source tree. Nothing in the
  decision becomes false. The one consequence this packet adds -- that an external embedder gets the
  inbox host -- is new documentation, not a change to the decision.
- [`DEC-0016`](../../decisions/DEC-0016-terminate-a-shell-that-outlives-its-pseudo-console.md) --
  `ChildExitWatcher::drop` and the 2 s `CHILD_EXIT_GRACE`. **Update required, path only**: the
  decision names `crates/pty/windows/child.rs`, which becomes `crates/vt/src/pty/windows/child.rs`.
  The guarantee, the bound and the scope are unchanged, and the tests that pin it move with the code.
- [`DEC-0005`](../../decisions/DEC-0005-terminate-only-oneterm-s-own.md) -- **reviewed, no change**:
  the updater's `OpenConsole.exe` sweep is `crates/update`'s and matches on install-directory image
  paths. Untouched by a source move in another crate.
- [`DEC-0014`](../../decisions/DEC-0014-oneterm-owns-its-vt-engine.md) -- **reviewed, no change**:
  it names `crates/pty` as "extracted before the engine" in a historical clause. History stays true;
  rewriting it would falsify the record.
- [`low-level-design/packaging.md`](low-level-design/packaging.md) -- the manifest, the package file
  list and the `cargo package --list | verify-dependency-graph.py --package-list -` gate `US-0097`
  shipped, and the "effect on existing checks" table. **Update required.**
- [`low-level-design/api-surface.md`](low-level-design/api-surface.md) -- the `pub` list and the
  semver promise. **Update required**: `pub mod pty` and a `polling`-is-public clause.
- [`high-level-design.md`](high-level-design.md) -- the "what stays outside" table, the module map
  and the dependency budget. **Update required**, and the dependency-budget rule "default features
  add nothing" is replaced outright.
- [`IN-0038.md`](IN-0038.md) -- decision (f), the parity comparison table, the packet table and the
  dependency order. **Update required.**
- [`US-0103`](US-0103-embedder-guide.md) -- the chapter list. **Update required**: thirteen chapters,
  chapter 13 gated on this packet.
- `docs/agents/error-policy.md` -- **reviewed, no change**: `on_resize` already returns an error
  rather than panicking and keeps doing so.
- `docs/agents/persistence.md` -- **reviewed, no change**: no persisted schema is involved.

### Documentation Action

**Update required**, in this order: the six repository policy documents above, the two decision path
references, the four intake documents, and `US-0103`'s outline. The one that matters most is
`crate-dependency-rules.md` R7: until it is rewritten, this packet's own change violates the rule it
is merged under, and an agent reading R7 next week would be told the engine has no platform
dependencies while `cargo tree` says otherwise.

**No contract change** for: `DEC-0013`, `DEC-0005`, `DEC-0014`, `error-policy.md`,
`persistence.md`, `docs/osc-agent-status.md`, `docs/osc-sequences-checklist.md` (no OSC or DCS
involved), `docs/terminal-split.md`, `docs/ssh-client-connect.md`, `docs/sftp-browser-design.md`
(the SSH backend does not use the pseudo-console transport at all).

### Reconciliation

Before completion, list every document above with "changed" or "reviewed, no change" and the
one-line reason, and confirm that `grep -rn 'oneterm.pty' docs/agents/ docs/*.md` returns nothing
outside `docs/spec-intakes/`.

## Context

Baselines measured on `main` @ `92ae9a6`.

| Fact | Value |
| --- | --- |
| `crates/pty` source | 2 759 lines, 9 files; 492 in three dedicated test files |
| `crates/pty` direct dependencies | `polling` (public), `log`, `windows-sys` (Windows), `libc` (Unix) |
| `oneterm-vt` direct dependencies today | 6, all leaves |
| `oneterm-vt` after, `--no-default-features` | 6 direct, 6 total, 7 tree lines |
| `oneterm-vt` after, default, Windows | 8 direct, 16 total |
| `oneterm-vt` after, default, Linux | 8 direct, 11 total |
| Reverse dependencies of `oneterm-pty` | `oneterm-local-shell`, `oneterm-tools` |
| Bundled ConPTY pair | `crates/app/assets/`, 1.1 MB, **not** in `crates/pty` |
| CI `--no-default-features` step | **present since `US-0097`**: `cargo build -p oneterm-vt --no-default-features --examples` in `scripts/ci-local.{sh,ps1}` and `.github/workflows/ci.yml`. This packet adds the `cargo tree` assertion beside it, not a second build. |
| CI packaging gate | **present since `US-0097`**: `cargo package -p oneterm-vt` and `cargo package -p oneterm-vt --list \| python scripts/verify-dependency-graph.py --package-list -` (the local scripts pass `--allow-dirty`). Must keep passing with `src/pty/**` in the list. |

The full design, the rejected alternative on the trait signatures, the rule replacement text and the
packaging decision are in [`low-level-design/pty.md`](low-level-design/pty.md). rio-vt's feature
shape, which this copies, is `default = ["pty"]` with `pty = ["dep:corcovado",
"dep:teletypewriter"]` over target-gated optional dependencies.

## Plan

- [ ] Commit 1, pure rename: `git mv crates/pty/src crates/vt/src/pty`, `crates/pty/src/lib.rs` ->
  `crates/vt/src/pty/mod.rs`, add `pub mod pty;` to `crates/vt/src/lib.rs`. No content edits, so
  `git diff -M` shows renames and a reviewer reads nothing.
- [ ] Commit 2, the feature: `crates/vt/Cargo.toml` gains `default = ["pty"]`,
  `pty = ["dep:polling", "dep:windows-sys", "dep:libc"]` and the three optional declarations; the
  `pub mod pty;` line gains `#[cfg(feature = "pty")]`; `crates/pty/Cargo.toml` is deleted.
- [ ] Commit 3, the consumers: `crates/local-shell` (manifest + 4 `use` lines),
  `crates/tools` (manifest + 1 `use` line + one comment), root `Cargo.toml`,
  `scripts/dependency-graph-policy.json`, `deny.toml`.
- [ ] Commit 4, CI: add the `cargo tree -e normal --no-default-features` assertion beside the
  `--no-default-features` build step `US-0097` already put in `scripts/ci-local.sh`,
  `scripts/ci-local.ps1` and `.github/workflows/ci.yml`. One step, not a second build.
- [ ] Commit 5, the crate's own docs: the self-containment pass over the 13 citation lines, the
  ~30 `missing_docs` lines, and a regenerated `crates/vt/public-api.txt`. Its own commit so the
  review is a diff of comment prefixes plus one generated file.
- [ ] Commit 6, repository docs: the six policy documents, the two decision path references, the
  four intake documents, `US-0103`'s outline, and the `crates/vt/CHANGELOG.md` entry -- the feature
  is an embedder-visible change and `US-0097` seeded the `Unreleased` section for exactly this.
- [ ] Run the acceptance commands; paste the two `cargo tree` outputs and the ten-launch probe result
  into Evidence.

## Dependency count table

Paste the measured version into Evidence; this is the expected one.

| Build | Direct | Distinct | Notes |
| --- | --- | --- | --- |
| `--no-default-features`, any target | 6 | 6 | identical to `main` today |
| default, `x86_64-pc-windows-msvc` | 8 | 16 | `+polling`, `+windows-sys 0.59`; transitively `cfg-if`, `concurrent-queue`, `crossbeam-utils`, `pin-project-lite`, `windows-sys 0.61.2`, `windows-link`, `windows-targets`, `windows_x86_64_msvc` |
| default, `x86_64-unknown-linux-gnu` | 8 | 11 | `+polling`, `+libc`; transitively `cfg-if`, `rustix`, `linux-raw-sys`; `bitflags` shared |
| OneTerm's app graph, before vs after | -- | **0 delta** | all three were already reachable through `local-shell` and `tools` |

## Ordering

- **After `US-0097`** -- merged into `main` @ `0558fa2`, so this gate is satisfied. `US-0097` owns
  the manifest, the explicit `publish = false`, the packaging gate, the README and
  `#![warn(missing_docs)]`. Landing this first would have written those lines twice and put an
  undocumented module under a lint that was about to be switched on.
- **Independent of `US-0098`, `US-0099`, `US-0100`.** Disjoint file sets: OSC dispatch, `input/`,
  `search/`. This packet touches `lib.rs` (one line), `Cargo.toml` (the feature table) and files none
  of them opens. It can run in parallel with all three.
- **Before `US-0101`, or coordinate on `lib.rs`.** `US-0101` renames `render` to `snapshot` and
  rewrites the `pub use render::{...}` block in `lib.rs`. Both packets edit `lib.rs`, in different
  places; whichever lands second rebases a one-line conflict. Going first is cheaper because this
  packet's `lib.rs` edit is a single added line.
- **Gates `US-0103` chapter 13.** `US-0103` is last overall regardless.

Revised intake dependency order: `BUG-0058` -> `US-0097` -> { `US-0098`, `US-0099`, `US-0100`,
`US-0101`, **`US-0104`** } -> `US-0102` -> `US-0103`.

## Decisions

- No new decision record. The reversal of decision (f) is the **owner's ruling of 2026-09-15**,
  recorded in [`IN-0038.md`](IN-0038.md)'s decision table with its date; an intake decision table is
  where an owner ruling belongs, and duplicating it as a `DEC-` would create two sources for one
  fact.
- The sub-decision this packet does own -- **the traits are gated with the feature rather than living
  in a feature-independent module** -- is recorded in
  [`low-level-design/pty.md`](low-level-design/pty.md) with its rejected alternative and a named
  revisit trigger. It binds this packet and the `polling` public-dependency clause in
  `api-surface.md`; a future redesign is a new packet.
- [`DEC-0016`](../../decisions/DEC-0016-terminate-a-shell-that-outlives-its-pseudo-console.md) stays
  accepted and in force. Path reference only.

## Verification Plan

- Focused: the two `cargo tree` assertions; the moved-test count comparison against `main`.
- Unit: `cargo test -p oneterm-vt --features pty`; `cargo test -p oneterm-vt --no-default-features`;
  `cargo test -p oneterm-vt --features vt-paranoid` (unchanged, must stay green).
- Integration: `cargo test --workspace`; `cargo test -p oneterm-local-shell` in particular, for
  `event_loop_tests` and `session_orphan_tests`.
- Platform: `pwsh scripts/ci-local.ps1`; the three feature-matrix builds;
  `cargo package --list -p oneterm-vt`; `python scripts/verify-dependency-graph.py`;
  `python scripts/third-party-notices.py --check`.
- E2E: the `IN-0031` ten-launch probe on Windows -- ten local shells opened and closed, no orphan
  `cmd.exe`, no orphan `OpenConsole.exe`, and `conpty: bundled` in the log. This is the only check
  that proves the bundled-host loader still resolves after the move.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Record: both `cargo tree` outputs verbatim; the dependency count table with measured numbers; the
moved-test counts before and after; the `cargo package --list` grep results; the ten-launch probe
result including the `conpty: bundled` log line; `git diff --stat` showing renames.

Known gaps to carry forward:

- **The "bring your own transport with the feature OFF" criterion cannot be written as the brief
  states it.** It asks for a test in `vt` that implements `EventedReadWrite` / `EventedPty` /
  `OnResize` over an in-memory pair with `pty` off. Those traits name `polling::Poller`,
  `polling::Event` and `polling::PollMode` in their signatures, so with the feature off they do not
  exist to implement, and making them exist means making `polling` unconditional -- which costs the
  `--no-default-features` build the six-dependency headline that the same brief makes an acceptance
  criterion. The two asks are mutually exclusive. What this packet proves instead: with the feature
  **on**, `pty::loopback_tests` implements all three traits over TCP sockets with no pseudo-console
  (the existing, moved proof); with the feature **off**, the entire engine suite passes, which is
  the stronger statement that the engine needs no transport at all. Writing an extra feature-off
  test that feeds `Terminal` from a `Vec<u8>` would add nothing -- every existing engine test already
  is that test.
- **`polling` becomes `oneterm-vt`'s only public dependency**, on a crate other projects now pin by
  git tag. A `polling` major bump is a breaking `oneterm-vt` change and a minor version bump under
  the `api-surface.md` promise. Trigger and redesign recorded in
  [`low-level-design/pty.md`](low-level-design/pty.md).
- **An external embedder on Windows gets the inbox `conhost.exe`**, which swallows Sixel DCS
  (`DEC-0013`), unless they bundle their own ConPTY pair. Documented in the README, the module
  rustdoc and guide chapter 13; not fixable from inside the crate.
- **Feature unification makes `cargo tree -p oneterm-terminal` show `polling`** inside the workspace,
  because `local-shell` turns `pty` on for the whole build. R7's claim is only provable by the
  explicit `--no-default-features` invocation, and the rule text says so.
- **`cargo doc` shows only the host platform's half of the module.** `pty::PseudoConsole` is a
  different type on Windows and Unix, so a reader who runs `cargo doc -p oneterm-vt --no-deps --open`
  on Linux never sees ConPTY and vice versa. There is no docs.rs page to fix this centrally (owner
  ruling 2026-09-15), so the module-level rustdoc in `pty/mod.rs` must describe **both** platforms
  in prose, and guide chapter 13 must not assume the reader can see the other half's items.
- **Unix `openpty` has no process-spawning test.** Only Windows has `pseudo_console_tests.rs`. That
  gap exists on `main` and is not made worse or better here; it is recorded because a published crate
  invites the question.

## Handoff

The six-commit split in **Plan** is the handoff boundary: a session that lands commits 1 to 3 leaves
the workspace building and testing green with the documentation stale, which is an honest incomplete
state -- though not a CI-green one, because commit 5's self-containment grep and public-API file are
gates. Commits 4 to 6 are mechanical. The stop condition for "done" is the acceptance grep returning
nothing outside `docs/spec-intakes/` and `pwsh scripts/ci-local.ps1` green.
