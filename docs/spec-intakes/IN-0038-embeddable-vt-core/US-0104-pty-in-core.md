# Work: the pseudo-console transport moves into the core behind a default-on feature

ID: US-0104
Intake: IN-0038
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
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

- [x] In scope: the file move `crates/pty/src/**` -> `crates/vt/src/pty/**`; the `pty` feature and
  the three optional dependencies in `crates/vt/Cargo.toml`; `pub mod pty` in `crates/vt/src/lib.rs`;
  the `use` lines in `crates/local-shell` (4 files) and `crates/tools` (1 file) and their manifests;
  root `Cargo.toml` (`members`, `[workspace.dependencies]`, `[profile.fast-dev.package]`);
  `scripts/dependency-graph-policy.json`; `deny.toml`; the CI `--no-default-features` step and its
  `cargo tree` assertion; `docs/agents/crate-dependency-rules.md` (layer line, R6, R7, R8, the
  verification block), `docs/agents/structure.md` (tree + three table rows),
  `docs/agents/dependencies.md`, `docs/architecture.md`, `docs/PROJECT.md`,
  `docs/terminal-backend.md`; the path reference inside `DEC-0016`.
- [x] Out of scope: any change to what the transport **does**. Not one function body changes. A
  behaviour change discovered mid-move is a new packet, not a fold-in.
- [x] Out of scope: the `EventedReadWrite` signature redesign that would remove `polling` from the
  public API. Decided against with a named trigger in
  [`low-level-design/pty.md`](low-level-design/pty.md), "Considered and rejected".
- [x] Out of scope: shipping the bundled ConPTY pair inside the packaged crate. Decided against in
  [`low-level-design/pty.md`](low-level-design/pty.md), "Packaging decision", and in
  [`low-level-design/packaging.md`](low-level-design/packaging.md).
- [x] Out of scope: guide chapter 13 itself. It is `US-0103`'s file; this packet only adds it to
  that packet's outline and gate list.
- [x] **In** scope, consequently, three things `US-0097` turned into gates before this packet
  existed. All are mechanical and all fail CI if skipped:
  - [x] `#![warn(missing_docs)]` coverage for the moved module -- about 30 doc lines (`Shell::new`,
    several `Options` and `WindowSize` fields).
  - [x] The **rustdoc self-containment grep**. `crates/pty/src` carries **13** `///` / `//!` lines
    citing `DEC-0013`, `DEC-0016`, `BUG-0055`, `CORR-10` or
    `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md`, in `lib.rs`, `windows/child.rs`,
    `windows/conpty.rs` and `windows/pipe_tests.rs`. Inside `crates/vt/src` they fail the grep
    `US-0097` added to `ci-local`. They go by the HLD's three rules: most drop from `///` to plain
    `//`, and the two module-level "Design:" lines become absolute links to this repository.
  - [x] The public-API snapshot and `python scripts/vt-public-api.py --check --no-doc`. `pub mod pty`
    adds roughly 13 public paths; the committed surface file is regenerated in the same commit so
    the diff is the reviewable record of what the module exposes.

## Acceptance

Every criterion is a command a verifier who distrusts this packet can run from the repository root.

- [x] `cargo build -p oneterm-vt` (default, `pty` on) exits 0.
- [x] `cargo build -p oneterm-vt --no-default-features` exits 0.
- [x] `cargo build -p oneterm-vt --all-features` exits 0.
- [x] `cargo tree -p oneterm-vt -e normal --no-default-features` prints exactly 7 lines: the crate
  plus `bitflags`, `log`, `memchr`, `rustc-hash`, `unicode-segmentation`, `unicode-width`. Pasted
  into Evidence verbatim.
- [x] `cargo tree -p oneterm-vt -e normal` on `x86_64-pc-windows-msvc` shows 8 direct dependencies
  and 16 distinct crates; on `x86_64-unknown-linux-gnu` (`--target`) 8 direct and 11 distinct. Both
  pasted into Evidence. No `gpui*` and no `oneterm-*` in either.
- [x] `cargo test -p oneterm-vt --features pty` runs the loopback tests moved from
  `crates/pty/src/loopback_tests.rs`, and
  `cargo test -p oneterm-vt --features pty -- --list | grep -c 'pty::loopback_tests'` is the same
  count as `cargo test -p oneterm-pty -- --list | grep -c 'loopback_tests'` on `main` @ `92ae9a6`.
  Same for `pty::windows::pipe_tests` and `pty::windows::pseudo_console_tests` on Windows.
- [x] `cargo test -p oneterm-vt --no-default-features` is green: the whole engine suite passes with
  no transport compiled. **This replaces the "bring your own transport test with the feature off"
  the brief asked for** -- see Gaps for why that criterion cannot be written as stated.
- [x] The workspace has no `oneterm-pty`. All three print nothing:
  ```bash
  test ! -d crates/pty && echo gone
  grep -rn 'oneterm.pty\|oneterm_pty' --include='*.rs' --include='*.toml' --include='*.md' \
    --include='*.json' --include='*.ps1' --include='*.sh' --include='*.yml' \
    crates/ docs/ scripts/ .github/ Cargo.toml deny.toml \
    | grep -v 'docs/spec-intakes/' \
    | grep -v 'docs/decisions/DEC-0014' \
    | grep -v 'crate-dependency-rules.md'
  cargo metadata --no-deps --format-version 1 | grep -o '"oneterm-pty"'
  ```
  **The exclusion list is `docs/spec-intakes/` wholesale, not `IN-0029` and `IN-0038` by name.**
  As first written this criterion could not pass: `IN-0031` names `crates/pty` and `oneterm-pty`
  across its packet, its high-level design and 14 lines of `evidence/BUG-0055-verify.md`, and
  `IN-0032` names them in its high-level design and `US-0090`, for exactly the reason `IN-0029` was
  excluded -- they are the record of what was true when they were written, and rewriting them would
  falsify it. Two further exclusions, each deliberate and each one line:
  - `docs/decisions/DEC-0014` names `crates/pty` in a historical clause ("extracted **before** the
    engine"). The Owning Docs review below already rules it "reviewed, no change" for this reason.
  - `docs/agents/crate-dependency-rules.md` carries the sentence this packet was told to write
    verbatim: "there is no separate `oneterm-pty` crate." It is the one place a future agent looks
    for the crate graph, and telling them the crate is gone is the point. A hygiene grep must not
    delete the sentence that explains the hygiene.

  With those three exclusions the grep returns nothing, and so does
  `grep -rn 'oneterm.pty' docs/agents/ docs/*.md` apart from that one sentence -- which is what the
  Reconciliation section below actually asks for.
- [x] `cargo package -p oneterm-vt --list | grep -i -e conpty -e openconsole` prints nothing
  **except** the source file `src/pty/windows/conpty.rs`, and the same list contains
  `src/pty/mod.rs`, `src/pty/unix.rs` and `src/pty/loopback_tests.rs`. No Microsoft binary appears.
- [x] The packaging gate `US-0097` shipped still passes, **with the `pty` module in the list and
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
- [x] `cargo package -p oneterm-vt` (the build form, not `--list`) still exits 0 -- it compiles the
  packaged crate out of tree, which is where a missing `#[cfg(feature = "pty")]` would surface.
- [x] `python scripts/verify-dependency-graph.py` passes against the edited
  `scripts/dependency-graph-policy.json`, and that file contains no `oneterm-pty`.
- [x] `python scripts/third-party-notices.py --check` passes **with no regeneration**: the bundle and
  its manifest did not move, and `polling` / `windows-sys` / `libc` were already in the app graph.
- [x] OneTerm's local shell is unchanged: `cargo test -p oneterm-local-shell` green, including
  `session_orphan_tests` (the `DEC-0016` escalation probe) and `event_loop_tests`.
- [ ] Ten-launch probe from `IN-0031`: open and close ten local shells in the running app and confirm
  no orphan `cmd.exe` and no orphan `OpenConsole.exe` remain, and that the log line
  `conpty: bundled` still appears -- i.e. the bundled host still resolves from the executable's
  directory after the source moved crate. Cheap, and it is the only check that proves the loader's
  runtime path resolution survived.
- [x] `docs/terminal-backend.md` and `docs/agents/structure.md` name `oneterm_vt::pty`, not
  `oneterm-pty`, and `python scripts/check-doc-paths.py` passes (it checks both files).
- [x] LOC: `git diff --stat main` shows the move as a rename. Production delta is net zero plus the
  manifest, module header and `missing_docs` lines; the number is pasted into Evidence with the
  dependency-count table.
- [x] The self-containment grep returns **0** lines over `crates/vt/src` (baseline for the moved
  files on `main` @ `92ae9a6` is **13**, in `lib.rs`, `windows/child.rs`, `windows/conpty.rs` and
  `windows/pipe_tests.rs`), and `python scripts/vt-public-api.py --check --no-doc` passes against a
  regenerated public-API snapshot whose diff is only the `pty` paths.
- [x] The public-API snapshot is **two files**, one per platform family, and
  `python scripts/vt-public-api.py --diff-platforms` reports a delta that is entirely inside
  `oneterm_vt::pty` and exits 0. See "The platform split" below.
- [x] `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features` exits 0 with
  `missing_docs` in force.
- [x] `pwsh scripts/ci-local.ps1` green.

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

- [x] Commit 1, pure rename: `git mv crates/pty/src crates/vt/src/pty`, `crates/pty/src/lib.rs` ->
  `crates/vt/src/pty/mod.rs`, add `pub mod pty;` to `crates/vt/src/lib.rs`. No content edits, so
  `git diff -M` shows renames and a reviewer reads nothing.
- [x] Commit 2, the feature: `crates/vt/Cargo.toml` gains `default = ["pty"]`,
  `pty = ["dep:polling", "dep:windows-sys", "dep:libc"]` and the three optional declarations; the
  `pub mod pty;` line gains `#[cfg(feature = "pty")]`; `crates/pty/Cargo.toml` is deleted.
- [x] Commit 3, the consumers: `crates/local-shell` (manifest + 4 `use` lines),
  `crates/tools` (manifest + 1 `use` line + one comment), root `Cargo.toml`,
  `scripts/dependency-graph-policy.json`, `deny.toml`.
- [x] Commit 4, CI: add the `cargo tree -e normal --no-default-features` assertion beside the
  `--no-default-features` build step `US-0097` already put in `scripts/ci-local.sh`,
  `scripts/ci-local.ps1` and `.github/workflows/ci.yml`. One step, not a second build.
- [x] Commit 5, the crate's own docs: the self-containment pass over the 13 citation lines, the
  ~30 `missing_docs` lines, and a regenerated public-API snapshot. Its own commit so the
  review is a diff of comment prefixes plus one generated file.
- [x] Commit 6, repository docs: the six policy documents, the two decision path references, the
  four intake documents, `US-0103`'s outline, and the `crates/vt/CHANGELOG.md` entry -- the feature
  is an embedder-visible change and `US-0097` seeded the `Unreleased` section for exactly this.
- [x] Run the acceptance commands; paste the two `cargo tree` outputs and the ten-launch probe result
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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
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


## Evidence (2026-09-15, `feat/vt-pty`)

Six commits became three, for one reason: the packet's commit 1 wired `pub mod pty` without the
feature or the manifest, which does not compile, and commits 4 to 6 are the same review unit. What
the split preserves is the one thing that mattered -- **commit 1 is still a pure rename**, nine
files with a zero-byte content delta, so `git diff -M` reads as renames and a reviewer reads
nothing.

| Commit | What |
| --- | --- |
| 1 | `git mv` only: `crates/pty/src/**` -> `crates/vt/src/pty/**`, `lib.rs` -> `mod.rs`, and `oneterm-pty` out of `members`, `[workspace.dependencies]` and `[profile.fast-dev.package]`. |
| 2 | The feature, the wiring, the consumers, the policy JSON, `deny.toml`, `missing_docs`, the self-containment pass, the public-API snapshot. Green on its own. |
| 3 | The `cargo tree` assertion in the three CI entry points, the README and CHANGELOG, and the repository policy documents. |

### `cargo tree -p oneterm-vt -e normal --no-default-features`

```text
oneterm-vt v0.5.2 (.../crates/vt)
|-- bitflags v2.13.2
|-- log v0.4.34
|-- memchr v2.8.2
|-- rustc-hash v2.1.2
|-- unicode-segmentation v1.13.3
`-- unicode-width v0.2.2
```

Seven lines, six leaf crates, identical to `main` today. Asserted in CI now, not read by eye.

### `cargo tree -p oneterm-vt -e normal`, both targets

`--target x86_64-pc-windows-msvc`:

```text
oneterm-vt v0.5.2 (.../crates/vt)
|-- bitflags v2.13.2
|-- log v0.4.34
|-- memchr v2.8.2
|-- polling v3.11.0
|   |-- cfg-if v1.0.4
|   |-- concurrent-queue v2.5.0
|   |   `-- crossbeam-utils v0.8.21
|   |-- pin-project-lite v0.2.17
|   `-- windows-sys v0.61.2
|       `-- windows-link v0.2.1
|-- rustc-hash v2.1.2
|-- unicode-segmentation v1.13.3
|-- unicode-width v0.2.2
`-- windows-sys v0.59.0
    `-- windows-targets v0.52.6
        `-- windows_x86_64_msvc v0.52.6
```

`--target x86_64-unknown-linux-gnu`:

```text
oneterm-vt v0.5.2 (.../crates/vt)
|-- bitflags v2.13.2
|-- libc v0.2.186
|-- log v0.4.34
|-- memchr v2.8.2
|-- polling v3.11.0
|   |-- cfg-if v1.0.4
|   `-- rustix v1.1.4
|       |-- bitflags v2.13.2
|       `-- linux-raw-sys v0.12.1
|-- rustc-hash v2.1.2
|-- unicode-segmentation v1.13.3
`-- unicode-width v0.2.2
```

No `gpui*` and no `oneterm-*` in either.

### Dependency count table, measured

| Build | Direct | Distinct | Predicted | Matches |
| --- | --- | --- | --- | --- |
| `--no-default-features` | 6 | 6 | 6 / 6 | yes |
| default, `x86_64-pc-windows-msvc` | 8 | 16 | 8 / 16 | yes |
| default, `x86_64-unknown-linux-gnu` | 8 | 11 | 8 / 11 | yes |
| OneTerm's app graph | -- | 0 delta | 0 delta | yes -- `python scripts/third-party-notices.py --check` passes with no regeneration |

### Tests

Re-measured after the rebase onto `main` @ `491dff8`, so the totals carry `US-0100`'s search tests
as well.

- `cargo test -p oneterm-vt`: 410 passed, 0 failed, 2 ignored (lib), plus the integration targets,
  of which `pty_contract` is 3. 24 of the lib tests are `pty::*`.
- `cargo test -p oneterm-vt --no-default-features`: 386 passed, 0 failed, 2 ignored.
  410 - 386 = 24, the whole transport suite, and the engine suite passes with no transport
  compiled -- plus `engine_without_pty`, 1 passed, which is the same statement from outside.
- `cargo test -p oneterm-vt --features vt-paranoid`: 410 passed, 0 failed.
- `cargo test -p oneterm-vt --features regex`: 415 passed, 0 failed.
- `cargo test -p oneterm-local-shell`: 33 passed, 0 failed, 2 ignored, including `event_loop_tests`
  and `session_orphan_tests`.
- `cargo test --workspace`: 0 failures.
- Moved-test count: `#[test]` attributes across the nine files, `92ae9a6:crates/pty/src` **30**, and
  `crates/vt/src/pty` **30**. Not one test was added, removed or renamed. By suite:
  `pty::loopback_tests` 2, `pty::windows::pipe::pipe_tests` 2,
  `pty::windows::pseudo_console_tests` 6, `pty::windows::child::tests` 4,
  `pty::windows::conpty::tests` **7**, `pty::windows::tests` 3 -- 24, which is the measured total
  two lines above. (An earlier revision said 8 for `conpty::tests` and therefore summed to 25;
  `conpty.rs` has seven `#[test]` attributes.)

### Package

```text
$ cargo package -p oneterm-vt --list | grep -i -e conpty -e openconsole
src/pty/windows/conpty.rs
```

One hit, and it is the loader's source. The list also carries `src/pty/mod.rs`, `src/pty/unix.rs`,
`src/pty/windows.rs`, `src/pty/loopback_tests.rs` and the four files under `src/pty/windows/`.
`cargo package -p oneterm-vt --list | python scripts/verify-dependency-graph.py --package-list -`
passes with nothing excluded, and `cargo package -p oneterm-vt` (the build form) exits 0.

### Rustdoc

The self-containment grep over `crates/vt/src` returns **0** lines (baseline for the moved files
was 13). Eleven `missing_docs` items were documented: `Shell::new`, four `WindowSize` fields, two
`EventedReadWrite` associated types and four of its methods. One latent broken intra-doc link
surfaced with the move -- `[`PipeReader::read`]` in `windows/pipe.rs`, a trait method rustdoc cannot
resolve as an inherent -- and became a code span; the doc gate had never run over this code.
`RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features` exits 0, and
`python scripts/vt-public-api.py --check --no-doc` passes against a regenerated snapshot whose
`pty` diff is 28 lines. The snapshot is two files, one per platform family -- see
"The platform split" below; this paragraph named the single `public-api.txt` it replaced.

### LOC

`git diff --stat -M main` shows the nine files as renames. Inside them the delta is
**+110 / -73, net +37**, and the three numbers agree -- an earlier revision said `-97`, which does
not subtract from 110 to 37. Per file, from `git diff --numstat` over the commit that edited them:

| File | + | - |
| --- | --- | --- |
| `pty/mod.rs` | 61 | 22 |
| `pty/windows/conpty.rs` | 17 | 18 |
| `pty/windows/child.rs` | 13 | 12 |
| `pty/windows/pseudo_console_tests.rs` | 8 | 10 |
| `pty/windows/pipe.rs` | 5 | 5 |
| `pty/unix.rs` | 4 | 4 |
| `pty/windows.rs` | 1 | 1 |
| `pty/windows/pipe_tests.rs` | 1 | 1 |
| `pty/loopback_tests.rs` | 0 | 0 |
| **total** | **110** | **73** |

The +37 is the module header (both platforms in prose, the absolute design link, the threading
model), the eleven `missing_docs` lines and the citation rewrites. No function body changed except
paths (`crate::` -> `crate::pty::`), log prefixes and thread names that spelled a crate that no
longer exists, and three test-only string markers. `crates/pty/Cargo.toml` is -24;
`crates/vt/Cargo.toml` is +18.

### The platform split of the public-API snapshot

`pub mod pty` is the first cfg-dependent public surface this crate has had, and `cargo doc` renders
only the host's half, so one committed file cannot describe both platforms. `US-0104` splits it:

| | |
| --- | --- |
| `crates/vt/public-api.windows.txt` | generated here by `python scripts/vt-public-api.py --update` on `x86_64-pc-windows-msvc` |
| `crates/vt/public-api.unix.txt` | **derived by hand from the Windows file on 2026-09-15**, because `rustup target list --installed` on this machine holds only `x86_64-pc-windows-msvc`, so `cargo doc --target x86_64-unknown-linux-gnu` had no standard library to document against. The file says so in a `#` note at the top, which every comparison ignores, and it is regenerated on the first Unix host to run `--update`. |

`scripts/vt-public-api.py` selects by `sys.platform`: `--check` compares the host's file only and
names it, `--update` rewrites the host's file only and names it, and `--diff-platforms` needs no
rustdoc at all -- it reports every line the two files disagree on and **exits non-zero if any of
them is outside `oneterm_vt::pty`**. That is the invariant the split has to keep, and it is the
reason the derivation is safe to hand-write: the engine's 682 common lines are byte-identical
because one file is a copy of the other, and the only edits are the six the transport's `cfg`
attributes require.

Measured output of `python scripts/vt-public-api.py --diff-platforms`:

```text
windows only: oneterm_vt::pty::Options    structfield escape_args
windows only: struct oneterm_vt::pty::PipeReader
windows only: struct oneterm_vt::pty::PipeWriter
unix only:    oneterm_vt::pty::Options    structfield child_signal_mask
unix only:    struct oneterm_vt::pty::SignalMask
unix only:    oneterm_vt::pty::SignalMask    method current

the delta is 6 lines, all inside `oneterm_vt::pty`
```

Six lines, exactly the predicted set. The Unix side was read off `crates/vt/src/pty/unix.rs`:
`pub struct SignalMask` has one public inherent method, `current` (`apply` is private and the tuple
field is private, so no `structfield` line), and `unix::PseudoConsole` has the same two public
methods as the Windows one, `spawn` and `child_pid`, so its block is unchanged. Both files are 716
surface lines.

The first Linux CI run is the check on the derivation: it either passes, or `--check` prints a
unified diff naming exactly what a real `cargo doc` on Linux found that the hand derivation did not.

`low-level-design/packaging.md` said "CI runs `cargo doc` on Windows, where the ConPTY half is the
one that renders". That was wrong -- the `Packaged crate (oneterm-vt)` job runs on `ubuntu-latest` --
and the sentence is corrected there.

### The adopted external-embedder tests

The independent verification wrote its own crates **outside** the workspace, each with its own
`[workspace]` table, so they consumed `oneterm-vt` exactly as a consumer does: public API only, no
`super::`, no private item. They are adopted here rather than cited, because a test that lives in
somebody's scratch directory is not a gate.

| Adopted as | What it pins |
| --- | --- |
| `crates/vt/tests/pty_contract.rs` (`#![cfg(all(windows, feature = "pty"))]`) | Three tests. A real `cmd.exe` child runs, its bytes come back, `next_child_event` reports the exit **without a read**, and the pid is gone after the drop. `on_resize` on both sides of the child's exit returns `io::Result` and never panics. Dropping a console whose child is still running (`ping -n 30`) ends that child inside the documented two-second grace. Also, by construction: `Options` is buildable from `Default` without naming a cfg-gated field, which is what makes portable embedder code possible. |
| `crates/vt/tests/engine_without_pty.rs` (`#![cfg(not(feature = "pty"))]`) | The engine feeds and renders with no transport compiled -- "bring your own transport", from the outside, as a gate rather than as prose. |

They are integration tests, so nothing in them can reach a private item, and they run in
`cargo test --workspace` and `cargo test -p oneterm-vt --no-default-features` respectively.

**Process hygiene is part of the test, not around it.** Every child is tracked by the pid
`child_pid()` returned and liveness is asked of that pid alone, through
`tasklist /FI "PID eq <pid>"`. Nothing is matched, enumerated or terminated by image name: this
machine runs other consoles. Measured after the run: `3 passed`, and no new `cmd.exe`, `PING.EXE`
or `OpenConsole.exe` pid.

The verifier's third artefact does not become a test. A binary that names `oneterm_vt::pty` must
**fail** to compile with the feature off, and `trybuild` is a dev-dependency this embeddable crate
should not grow for one expectation. It is a build step instead, run here against a throwaway crate
outside the workspace whose only dependency is
`oneterm-vt = { path = "...", default-features = false }`:

```text
$ cargo build --bin usepty
error[E0433]: cannot find `pty` in `oneterm_vt`
 --> src\bin\usepty.rs:5:31
 --> ...\crates\vt\src\lib.rs:45:9
exit=101
```

### `windows-sys` features come from the workspace, not from this crate

`cargo package` flattens `windows-sys.workspace = true` into the nine features the root
`[workspace.dependencies]` table lists, so a consumer of the packaged crate gets all nine. The
transport uses **seven**: `Win32_Foundation`, `Win32_Security`, `Win32_Storage_FileSystem`,
`Win32_System_Console`, `Win32_System_LibraryLoader`, `Win32_System_Pipes` and
`Win32_System_Threading`. The two it never names are `Win32_System_Diagnostics_ToolHelp`
(`crates/update`'s install-directory process sweep) and `Win32_System_IO`.

**Not trimmed here, and it is not the one-line change it looks like.** Cargo's
`features = [...]` on an inherited dependency only *adds*; subtracting means `crates/vt` stops
inheriting and writes `windows-sys = { version = "0.59", optional = true, features = [...] }` of
its own, which contradicts `docs/agents/dependencies.md`'s "declared once in root
`[workspace.dependencies]`" rule and creates a second place the version can drift. The cost today
is a superset of features on a crate that is compiled from source anyway, which is a compile-time
non-event.

Trigger to do it: **the day `oneterm-vt` stops being a member of this workspace** -- its own
repository, or a crates.io release -- at which point the inheritance is gone regardless and the
seven features are written out explicitly. A second, weaker trigger: a workspace crate leaving and
silently changing the union under the engine, which is exactly the failure this note exists to
name.

### Gaps found while doing it

1. **The acceptance grep could not pass as written**, and is rewritten above: the exclusion is
   `docs/spec-intakes/` wholesale, because `IN-0031` and `IN-0032` name the old crate throughout
   their evidence for exactly the reason `IN-0029` was excluded, plus `DEC-0014`'s historical clause
   and the one deliberate sentence in `crate-dependency-rules.md`.
2. **The ten-launch probe was not run**, and most of what it was for is now covered another way.
   It drives the GPUI application, and the owner runs Claude inside a running `oneterm.exe`;
   opening and closing ten shells in a second instance from a worktree build is not the cheap
   check the packet assumed.

   What was run instead is the independent verifier's probe, which is better than the packet's
   first attempt because it observes **which host was loaded** rather than inferring it. From
   `target/debug/`, where `crates/app/build.rs` had already staged `conpty.dll` and
   `x64/OpenConsole.exe`, `pty-throughput.exe` is started with `-PassThru` and its loaded modules
   read:

   ```text
   spawned pid=25380
   loaded: ...\agent-ac736b06b9eb6cf72\target\debug\conpty.dll
   exit=0
   new pids: (none)
   ```

   That is direct proof that the loader, now compiled into `crates/vt`, still resolves `conpty.dll`
   against the **running executable's** directory and still prefers the bundled host over
   `kernel32`, and that the child and its console host were reaped. The resolution order itself is
   pinned by `pty::windows::conpty::tests::conpty_api_prefers_the_bundled_host` and
   `..._falls_back_to_the_system_host`, both green, which stage a real `conpty.dll` in a scratch
   directory.

   **Still unverified**: the `conpty: bundled` log line itself (`pty-throughput` installs no
   logger, so the line has nowhere to go) and the orphan count across ten open/close cycles in the
   GPUI application. The `E2E proof` box stays unticked for exactly this.
3. The `pty` module rustdoc had to stop linking `[`windows::conpty`]` -- `windows` is a private
   module, and `-D warnings` makes a public-to-private intra-doc link an error. It is prose now.

## Harness Row

The harness database is not edited by this packet's session. This is the row it owes, for whoever
applies it (`intake_id` 43, proof columns as the block above: unit 1, integration 1, e2e **0**,
platform 1):

```sql
INSERT INTO story (
    key, intake_id, title, kind, risk, status,
    proof_unit, proof_integration, proof_e2e, proof_platform, proof_verify,
    path
) VALUES (
    'US-0104', 43,
    'The pseudo-console transport moves into the core behind a default-on feature',
    'US', 'high_risk', 'implemented',
    1, 1, 0, 1, 1,
    'docs/spec-intakes/IN-0038-embeddable-vt-core/US-0104-pty-in-core.md'
);
```

`proof_e2e` is 0 on purpose: the ten-launch probe in the GPUI application was not run, and the
`conpty: bundled` log line is unobserved. Everything else in "Gaps found while doing it" is closed.

## Verification notes closed

Independent verification: [`evidence/US-0104-verify.md`](evidence/US-0104-verify.md), verdict
**PASS-WITH-NOTES**. All thirteen findings are closed in this packet or in the branch:

| | Finding | Closed by |
| --- | --- | --- |
| F1 | `firecap.bin` committed by accident | Deleted and added to `.gitignore`. It is `pty-throughput`'s capture file, written into the current directory -- the repository root, when the bundled-loader probe ran -- and swept in by a `git add -A`. |
| F2 | status block ticked states never entered | `Changed`, `Reopened` and `Retired` unticked. |
| F3 | `E2E proof` ticked for a check that did not run | Unticked; the Gaps section already said so. |
| F4 | six commits behind `main`, one README sentence dies on merge | Rebased onto `491dff8`. The README's feature paragraph now names **two** features that add dependencies, `pty` and `regex`. Both snapshots regenerated against the post-`US-0100` surface and the Unix one re-derived; `--diff-platforms` still shows exactly the six `pty` lines. The verifier's loaded-module probe is adopted above. |
| F5 | `structure.md`'s `vt/` tree not extended | `src/pty/` added with its four sub-entries, `public-api.txt` replaced by the two snapshot files, and the "modules named by path" count corrected to five (`grid`, `intern`, `parser`, `search`, `pty`). |
| F6 | `scripts/README.md` names the old single file | Row rewritten: two snapshots, `--update` / `--check` / `--diff-platforms`. |
| F7 | LOC delta `-97` does not add up | `-73`, with the per-file table above. |
| F8 | `conpty::tests` counted 8, total said 24 | **7**, and the suites now sum to 24. |
| F9 | Rustdoc paragraph described the pre-split snapshot | Rewritten to point at "The platform split". |
| F10 | two repository-path citations survived on public items | Both rewritten, and **the grep was widened**: `crates/` and `docs/` now count as citations in `///` and `//!` text, in all three CI entry points. That found six more inside `crates/vt/src` (four of them pre-existing, in `graphics`, `parser`, `selection`, `terminal`); all twelve are fixed and the grep returns zero. |
| F11 | packaged manifest inherits the workspace `windows-sys` union | Recorded above with the trigger, and the two unused features named precisely. The verifier named `Win32_Storage_FileSystem` as one of them; it is used (`ReadFile` / `WriteFile` in `windows/pipe.rs`). The unused pair is `Win32_System_Diagnostics_ToolHelp` and `Win32_System_IO`. |
| F12 | `polling` reaches `oneterm-terminal` unconditionally | **Fixed, not just recorded.** The adapter never names `oneterm_vt::pty`, so the root `[workspace.dependencies]` entry now carries `default-features = false` and the two members that do name it -- `local-shell` and `tools` -- take `features = ["pty"]`. `cargo tree -p oneterm-terminal -e normal` is clean of `polling` and `windows-sys`, and R7's verification column says so. An outside embedder still gets the transport by default. |
| F13 | the Unix snapshot cannot be executed on this host | Left derived, and the header says so. The `Packaged crate (oneterm-vt)` job runs on `ubuntu-latest` and runs `vt-public-api.py --check --no-doc` against real Linux rustdoc, so **the first CI run on that job is the first real check of this file**; it either passes or prints a unified diff naming what differs. |

One correction to the packet's own earlier wording, from the verifier's review: `crates/app` is not
untouched. `crates/app/assets/` is -- the bundled pair, its manifest and the bump script never move
-- but `crates/app/build.rs` has one word changed in a doc comment, `oneterm-pty` to
`oneterm_vt::pty`.

## Handoff

The six-commit split in **Plan** is the handoff boundary: a session that lands commits 1 to 3 leaves
the workspace building and testing green with the documentation stale, which is an honest incomplete
state -- though not a CI-green one, because commit 5's self-containment grep and public-API file are
gates. Commits 4 to 6 are mechanical. The stop condition for "done" is the acceptance grep returning
nothing outside `docs/spec-intakes/` and `pwsh scripts/ci-local.ps1` green.
