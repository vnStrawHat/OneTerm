# Work: oneterm-vt is a packaged crate whose documentation stands alone

ID: US-0097
Intake: IN-0038
Created: 2026-09-15

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

- Change type: new capability
- Risk lane: high_risk (it makes a Rust API an external contract for the first time)
- Spec Intake: `IN-0038`

## Outcome

`cargo package -p oneterm-vt` succeeds, and the crate it packages is one a stranger can use: a
README that says what it is and is not and how to depend on it, a headless example that compiles and
runs, a CHANGELOG with the semver promise, the licence text, `#![warn(missing_docs)]` satisfied, and
rustdoc with no dangling references to documents that live only in this repository. The terminal
identifies itself as the embedder's product, not as the engine.

**Owner ruling 2026-09-15: the crate is not published to crates.io.** Other projects consume it as
a git dependency on this repository, pinned to a tag, so `crates/vt/Cargo.toml` carries an explicit
`publish = false` and there is no docs.rs page. Everything else the packet does is what makes that
dependency usable, and the manifest keeps its metadata so the flip is one line if the owner ever
decides otherwise.

## Scope

- [x] In scope: `crates/vt/Cargo.toml` (publish, metadata, features); `crates/vt/README.md`;
  `crates/vt/CHANGELOG.md`; `crates/vt/examples/headless.rs`; `#![warn(missing_docs)]` and the doc
  lines it demands; the 105-line doc-comment self-containment pass; `Config::product_name` and the
  two replies that read it; `scripts/vt-public-api.py` and `crates/vt/public-api.txt`; the new CI
  steps in `scripts/ci-local.{sh,ps1}` and `.github/workflows/ci.yml`.
- [x] Out of scope: the OSC mechanism (`US-0098`), the moves (`US-0099`, `US-0100`), the rename
  (`US-0101`), the conformance gaps (`US-0102`). Each of those adds its own CHANGELOG lines.
- [x] Out of scope: an MSRV CI job. Recorded as a gap in `packaging.md`; adding it costs CI minutes
  on every push and is the owner's call.
- [x] Out of scope: splitting the crate into its own repository (intake Open Decision 6).
- [x] Out of scope, on the owner's ruling: a `serde` feature. The `regex` optional dependency is
  left to `US-0100`, which is the packet that wires `SearchPattern::Regex`; declaring an optional
  dependency here that nothing uses would be a feature with no item behind it.

## Acceptance

- [x] `cargo package -p oneterm-vt` exits 0, and the file list it prints contains `README.md`,
  `CHANGELOG.md`, `LICENSE`, `NOTICE` and `examples/headless.rs`. 68 files, 883.5 KiB (228.4 KiB
  compressed); `fuzz/` is excluded on its own (it declares its own `[workspace]`), so no `exclude`
  key was needed. `public-api.txt` ships too, at 16 KiB: it describes the crate a reader is holding,
  and an `exclude` key is one more thing to keep in step for no gain. **The criterion said
  `cargo publish --dry-run`**; owner ruling 2026-09-15 makes that command unavailable
  (`publish = false`), and `cargo package` is what it becomes. `cargo package` works unchanged with
  `publish = false`: no `--no-verify` and no fallback was needed.
- [x] `grep -c 'publish = true' crates/*/Cargo.toml` totals **0** after owner ruling 2026-09-15,
  and `crates/vt/Cargo.toml` says `publish = false` explicitly so the decision is where a reader
  looks for it. Enforced in CI by `scripts/verify-dependency-graph.py` rather than by a shell grep:
  that script already reads `cargo metadata`, where a publishable package is `publish: null`, so
  the assertion is that the publishable list is **empty**. The comment there names the flip path.
- [x] `cargo doc -p oneterm-vt --no-deps --all-features` produces **zero** warnings with
  `#![warn(missing_docs)]` in force, and CI runs it under `RUSTDOCFLAGS=-D warnings`.
- [x] `cargo build -p oneterm-vt --no-default-features`, `--all-features` and `--examples` are each
  clean. The default feature set is empty, so CI runs the two distinct configurations
  (`--no-default-features --examples` and `--all-features --examples`) rather than three.
- [x] `cargo run -p oneterm-vt --example headless` prints the events and the visible rows; its output
  is pasted into Evidence.
- [x] The self-containment grep returns **0** lines. Measured before the pass: **106** lines in 44
  files, because the grep as written also reads the eight `*_tests.rs` / `*_bench.rs` files the
  105/37 baseline excluded. All 44 were cleaned, so the criterion holds as written.
  ```bash
  grep -rn '^[[:space:]]*//[/!].*\(US-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|docs/spec-intakes\)' \
    crates/vt/src --include='*.rs' | grep -v 'https://github.com/'
  ```
  Baseline on `main` @ `36977ca` is **105** lines, in 37 files.
- [x] Every code block in `README.md` is compiled by something. Stronger than the criterion asks:
  `lib.rs` carries `#[cfg(doctest)] #[doc = include_str!("../README.md")]`, so all three Rust blocks
  *are* doctests of the file itself and cannot drift at all. `cargo test -p oneterm-vt --doc` runs
  them (3 passed). The two non-Rust blocks are the dependency tree (```` ```text ````) and the
  `cargo run` line (```` ```console ````), neither of which is code.
- [x] `crates/vt/public-api.txt` exists, is generated by `scripts/vt-public-api.py`, and
  regenerating it produces no diff (688 lines: every public item an embedder can name, plus its
  fields, variants, associated constants and inherent methods).
- [x] `XTVERSION` with `Config::default()` replies `\x1bP>|oneterm-vt(<engine version>)\x1b\\`; with
  `product_name = Some("OneTerm(0.5.2)")` it replies `\x1bP>|OneTerm(0.5.2)\x1b\\`, which is
  byte-for-byte what `main` replies today for OneTerm. Both asserted, in
  `da1_da2_dsr_xtversion_answers` and the new `product_name_owns_xtversion_and_da2`.
- [x] `DA2` with `product_name = Some("OneTerm(0.5.2)")` replies exactly what `main` replies today
  (`\x1b[>0;502;1c`), and a name with no version in it falls back to the engine's number rather
  than reporting zero. Both asserted.
- [x] `cargo test --workspace` green; `pwsh scripts/ci-local.ps1 -Full` green.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` -- the `oneterm-vt` dependency row and the "declared once in the
  workspace" rule. The `regex` and `serde` optional declarations must appear there.
- `docs/agents/crate-dependency-rules.md` -- R7 and R11. Publishing changes neither; the crate stays
  a leaf with no OneTerm dependency, which is precisely what makes it publishable.
- `docs/agents/structure.md` -- the crate responsibility table; the `vt` row gains "embeddable".
- `docs/PROJECT.md` -- "Stack and Surfaces" names `oneterm-vt` as OneTerm's engine; it gains the
  fact that it is also a library other projects depend on by git.
- `scripts/README.md` -- every script and which CI runs; `vt-public-api.py` is a new row.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` -- the API rules the
  README summarises.

### Documentation Action

**Update required**: `docs/agents/dependencies.md` (the optional-feature row),
`docs/agents/structure.md` (the `vt` row), `docs/PROJECT.md` (one sentence), `scripts/README.md`
(one row), plus the three new files in `crates/vt/`.

Reason: making the crate consumable outside this repository is a fact about it that none of these
documents currently states, and three of them are the documents an agent reads before touching the
workspace.

### Reconciliation

Changed:

- `docs/PROJECT.md` -- "Stack and Surfaces" now says `oneterm-vt` is the crate other projects
  depend on by git, that it is **not** published to crates.io, and that its API, rustdoc and reply
  bytes are an external contract.
- `docs/agents/structure.md` -- the `vt` row gains "Embeddable" (and that it is **not** on
  crates.io), the semver pointer and the rule that `///` and `//!` text carries no record citations;
  the directory tree gains `README.md`, `CHANGELOG.md`, `public-api.txt` and
  `examples/headless.rs`.
- `docs/agents/dependencies.md` -- the `oneterm-vt` row gains one sentence: the dependency list is
  now a promise to people outside the repository, and default features add nothing.
- `scripts/README.md` -- a `vt-public-api.py` row, the publish assertion added to the
  `verify-dependency-graph.py` row, and the `vt-package` CI job named in the header.
- `.github/workflows/ci.yml`, `scripts/ci-local.sh`, `scripts/ci-local.ps1` -- the new gate.
- `AGENTS.md` section 4 -- the same steps, because that list is what an agent runs before reporting
  a task done and it is supposed to match `ci.yml`.
- `IN-0038.md` -- Open Decisions 2, 3, 5 and 6 ticked with their ruling text and provenance, and six
  places in the body above them aligned with Open Decision 2.
- `docs/spec-intakes/IN-0009-.../` renamed to `IN-0009-completion-utf8-prefix-slicing/`, because its
  118-character directory name broke a git dependency's checkout on Windows. No document referenced
  the old path; `harness.db` does, and the Harness note below names both rows.
- `low-level-design/packaging.md` and `low-level-design/api-surface.md` -- reconciled with what was
  built: the manifest as it is, the README's whole-file doctest route, and the HTML-reading
  public-API script.

New crate-level files: `crates/vt/README.md`, `crates/vt/CHANGELOG.md`,
`crates/vt/examples/headless.rs`, `crates/vt/public-api.txt`, `crates/vt/LICENSE`,
`crates/vt/NOTICE`, `crates/vt/tests/product_name.rs`, `scripts/vt-public-api.py`, and
`evidence/US-0097-verify.md`.

Reviewed, no change needed:

- `docs/agents/crate-dependency-rules.md` -- R7 and R11 are unchanged by publishing. The crate stays
  a leaf with no OneTerm dependency, which is exactly what makes it publishable; no rule moved.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` -- the API rules the
  README summarises are still accurate. The README restates them for an outside reader rather than
  changing them, and the one behavioural change (`XTVERSION`'s default string) is recorded in
  `crates/vt/CHANGELOG.md`, which is where an embedder looks.
- `docs/terminal-backend.md` -- describes the adapter's use of the engine, which is unchanged;
  `adapter_config` gained one field whose value reproduces today's bytes exactly.

## Context

Baselines measured on `main` @ `36977ca`:

| Fact | Value |
| --- | --- |
| `cargo tree -p oneterm-vt -e normal` | 7 lines (the crate plus 6 leaf dependencies) |
| rustdoc citations to internal records | 105 lines, 37 non-test files |
| `publish` | inherited `false` from root `Cargo.toml:32` |
| README / CHANGELOG / examples | none |
| `missing_docs` | not enabled |

`XTVERSION` is at `crates/vt/src/terminal/dispatch.rs:1127-1134` and `DA2` at `dispatch.rs:467`;
both read `env!("CARGO_PKG_VERSION")`, which is the workspace version and today happens to be both
the engine's and the app's.

The full design is in [`low-level-design/packaging.md`](low-level-design/packaging.md); the
`product_name` rationale is in the HLD.

## Plan

- [x] Manifest: an explicit `publish = false` (owner ruling 2026-09-15), `readme`, `keywords`,
  `categories`, a rewritten `description`, and copies of `LICENSE` and `NOTICE` inside the crate
  directory so the package carries them. No `[package.metadata.docs.rs]`: there is no docs.rs page.
  The `regex` / `serde` optional declarations are **not** here either: Open Decision 3 ruled out
  `serde`, and `regex` belongs to `US-0100`, the packet that gives the feature an item to gate.
- [x] `Config::product_name: Option<Cow<'static, str>>` plus the two reply sites; `adapter_config`
  in `crates/terminal/src/handle.rs` sets it so OneTerm's bytes do not change.
- [x] `#![warn(missing_docs)]` and the documentation it demands: 263 items, the bulk in `cell`,
  `grid`, the event types and `Terminal`'s own accessors.
- [x] The self-containment pass, by the three rules in the HLD: 106 lines over 44 files.
- [x] `README.md`, `CHANGELOG.md` (with the semver promise), `examples/headless.rs`. The README's
  **Documentation** section names the embedder's guide as **planned**, with no URL: there is no
  `guide` module yet, and after owner ruling 2026-09-15 there is no docs.rs page either. The render
  is `cargo doc -p oneterm-vt --no-deps --open` rather than `scripts/vt-docs.{sh,ps1}`, which are
  `US-0103`'s and do not exist. The README also gains an **Install** section with the git dependency
  form, a tag recommendation, and the sentence that the semver promise applies to tags.
- [x] `scripts/vt-public-api.py` and the committed `public-api.txt`. It reads the rustdoc **HTML**,
  not `--output-format json`: the JSON format is nightly-only and `rust-toolchain.toml` pins stable
  1.96.0. The cost is signature-level detail, stated in the script's own docstring.
- [x] CI: the two feature-matrix builds with `--examples`, the example actually run, the doc build
  under `-D warnings`, the public-api diff, the packaged-file-set check and the self-containment
  grep, in both `ci-local` scripts and in a new `vt-package` job in the workflow. The packaging gate
  is `cargo package -p oneterm-vt --list` piped into the file-set check everywhere; the workflow
  also runs a plain `cargo package -p oneterm-vt`, which verifies the packaged crate builds. The
  local scripts pass `--allow-dirty`, because an agent runs the gate **with** uncommitted work.
- [x] Update the five documents listed above.

## Decisions

- [`DEC-0017`](../../decisions/DEC-0017-osc-routing-table-not-handler-registry.md) is `US-0098`'s,
  not this packet's. This packet records no decision of its own; the publish policy, semver promise
  and MSRV rule live in `packaging.md` and `api-surface.md` and become binding when the owner accepts
  the intake.

## Verification Plan

- Focused: the two reply tests (`XTVERSION`, `DA2`) with and without `product_name`.
- Unit: `cargo test -p oneterm-vt`; `cargo test --doc -p oneterm-vt` for the README doctests.
- Integration: `cargo test --workspace`; `cargo test -p oneterm-terminal` proves OneTerm's replies
  are unchanged.
- Platform: `cargo package -p oneterm-vt`; `cargo doc --all-features`; the three builds;
  `pwsh scripts/ci-local.ps1`.
- E2E: manual Windows walk -- start a local shell, run a program that probes `XTVERSION` (`tmux -V`
  inside the session, or `printf '\033[>0q'` and read the reply), confirm the string is unchanged.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

**Package.** `cargo package -p oneterm-vt`: `Packaged 68 files, 883.5KiB (228.4KiB compressed)`,
verified build clean, exit 0. The exact command the gate runs is

```console
$ cargo package -p oneterm-vt --list |
    python scripts/verify-dependency-graph.py --package-list -
Dependency graph policy passed for 21 workspace packages and 21 explicit members.
Package set passed: the oneterm-vt package carries CHANGELOG.md, LICENSE, NOTICE, README.md,
examples/headless.rs, and reaches nothing outside crates/vt.
```

The local scripts add `--allow-dirty` so the gate runs with uncommitted work; a doctored list
missing `LICENSE` fails with exit 1. `fuzz/` is excluded on its own (it declares its own
`[workspace]`), and no
directory approaches 1 MB, so no `exclude` key was added; `public-api.txt` ships at 16 KiB, which is
fine -- it describes the crate the reader is holding. Two file names carried internal record ids and
would have shipped: `tests/us0087_cleanup_rows.rs` and `src/terminal/verify_bug0058_tests.rs`,
renamed to `tests/cleanup_rows.rs` and `src/terminal/dcs_routing_tests.rs`; every `tests/*.rs` header
was rewritten too.

**Citations.** 106 rustdoc lines in 44 files before, 0 after. **23** rustdoc lines across 23 files
keep their design document as an absolute `https://github.com/vnStrawHat/OneTerm/...` link, which is
the form the HLD allows; everything else became a plain `//` comment or plain English. (An earlier
draft of this section said "six", counting the forks' reports rather than the files.) The CI
grep also covers `BUG-NNNN`, which the criterion's pattern omits: it caught four more rustdoc lines,
all `BUG-0058`. Two `BUG-0051` mentions survive in `reflow/reflow_tests.rs` as plain `//` comments,
which is what the rule asks for. Internal vocabulary with no id at all (`trap 40`, `correction C8`,
`deviation D4`, `R-56`) was found by grepping the **generated HTML** rather than the source, because
that is what the rule is actually about; `Mode`'s variants were the last rendered offenders.

**Documentation.** 263 `missing documentation` warnings before, 0 after.
`RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` is clean; it found one
pre-existing defect on the way, a public `parser` doc linking to the private `state` module.

**Identity.** `XTVERSION` before: `\x1bP>|OneTerm(0.5.2)\x1b\\` unconditionally. After:
`\x1bP>|oneterm-vt(0.5.2)\x1b\\` with `Config::default()`, and `\x1bP>|OneTerm(0.5.2)\x1b\\` for
OneTerm, whose adapter now sets `product_name`. `DA2` is `\x1b[>0;502;1c` in both cases.

The name is **sanitised once**, where the reply is built, so `XTVERSION` and `DA2` can never
disagree: C0, `DEL` and C1 controls are dropped, the result is cut to 64 bytes on a character
boundary, and a name that sanitises to nothing falls back to the engine's own. `DA2`'s number
saturates each version component at 99, so no name an embedder builds from a string it did not
control can overflow the arithmetic. `crates/vt/tests/product_name.rs` (8 tests, adopted from the
verification) pins all of it.

**The example.**

```console
$ cargo run -p oneterm-vt --example headless
-- events --
title      "headless demo"
osc 7      "file://localhost/tmp"
osc 1337   "SetUserVar=demo"
Repaint
104 bytes fed, 0 unhandled sequences
-- screen (Full) --
|hello world                     |
|                                |
|  row three                     |
|                                |
```

**Surface.** `crates/vt/public-api.txt`, **688** lines over 100 public items, regenerated with no
diff by `python scripts/vt-public-api.py --check`. It lists only paths an embedder can write: the
crate root and the three `pub mod`s, read from `lib.rs` rather than hard-coded. Two break tests,
both run: renaming the private `parser::params` module leaves the file unchanged (exit 0,
"public API surface unchanged"), and renaming the public `cluster_width` fails it with the expected
two-line diff (exit 1).

**Gate.** `cargo test -p oneterm-vt` 374 + 6 + 8 + 7 + 3 doctests (two ignored in the lib suite,
one in `parser_limits`); `pwsh scripts/ci-local.ps1 -Full` green, run once with an untracked file
under `crates/vt/` present to prove the local gate no longer needs a clean tree.

Known gaps to carry forward:

- CI does not build at the MSRV, so `rust-version = 1.96.0` is a claim rather than a proof. The
  README says so in as many words rather than implying a check that does not exist.
- The crate is not published, and will not be: **owner ruling 2026-09-15** on Open Decision 2.
  Other projects consume it as a git dependency. The `rev` form is measured -- a scratch consumer
  outside the workspace built and ran against this branch on the default `CARGO_HOME` -- but the
  `tag` form cannot be until the application releases `v0.5.3`, because every existing tag predates
  the crate.
- **No project actually depends on it yet.** The consumer that proved the README was written for
  that purpose and deleted. The first real embedder will find whatever a five-minute scratch crate
  did not.
- `product_name`'s default string is Open Decision 5, ruled `oneterm-vt(<crate version>)`. All four
  decisions this packet leans on (2, 3, 5, 6) are recorded in `IN-0038.md` as owner rulings dated
  2026-09-15.
- **No E2E walk.** The manual Windows walk (start a local shell, probe `XTVERSION`, confirm the
  string is unchanged) was not run: the owner runs this session inside OneTerm, so driving the GUI
  here is unsafe. The reply is pinned by two unit tests against the exact byte strings, and OneTerm's
  half of it is one `concat!` in `adapter_config`, but nobody has watched a real `tmux -V` see it.
- `scripts/vt-public-api.py` reads rustdoc HTML, so it detects an item, field, variant or method
  that is added, removed or renamed, but **not** a signature change. Rustdoc JSON would, and is
  nightly-only; revisit if the pinned toolchain ever moves. `--no-doc` also trusts whatever is in
  `target/doc`, so run out of order it can validate stale HTML; every caller in CI builds the docs
  in the step before.
- The MSRV rule (a raise is a minor bump) is published in `CHANGELOG.md` and the README, but intake
  Open Decision 4 is still open and no CI job builds at 1.96.0.
- `StrSpan`, `ByteSpan` and `ParamSpans` are public-in-private: they appear in `VtEvent`'s variants
  but are not re-exported, so an embedder can destructure them and can never name them or read their
  docs. Found during the doc pass, out of scope here, and it belongs on `US-0098`'s api-surface list.
- `FeedStats::grapheme_truncated` and `FeedStats::style_table_exhausted` are never written. They are
  documented as reserved rather than as working counters; wiring them is a separate change.
- **Out of scope, for a later BUG:** `oneterm-terminal`'s
  `handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks` failed once under
  load during the verification and passed 3/3 in isolation and inside the green gate. It is a
  two-thread scheduling test with a hard-coded 250 us sleep (`crates/terminal/src/handle.rs`), and
  nothing in this packet's production diff reaches it. Recorded so the next person to see it red
  does not chase `US-0097`.

## Verification notes closed

Independent verification: [`evidence/US-0097-verify.md`](evidence/US-0097-verify.md),
**PASS-WITH-NOTES** at `2e1495c`. Seven medium findings, six low and one informational. All are
closed; the rulings on the seven mediums are the coordinator's.

| Finding | Closed by |
| --- | --- |
| F1 dead guide link | The README's **Documentation** section names the guide as planned and carries no URL. `US-0103` adds the link with the guide. |
| F2 no licence in the tarball | `LICENSE` and `NOTICE` copied into `crates/vt/`, so `cargo package` ships them. `scripts/verify-dependency-graph.py --package-list -` asserts both, plus `README.md`, `CHANGELOG.md` and `examples/headless.rs`, from a `cargo package --list` on stdin; CI and both `ci-local` scripts pipe it. A doctored list missing one fails with exit 1. |
| F3 unvalidated `product_name` spliced into a DCS | Sanitised in `Terminal::new` (see R4): every C0, `DEL` and C1 control dropped, cut to 64 bytes on a character boundary, empty result falls back to `oneterm-vt(<version>)`. Documented on the field, which is what an embedder reads. Pinned by `tests/product_name.rs` (8 tests), including the verifier's `ESC \`, `0x9c`, `BEL` and 64 KiB cases. |
| F4 `DA2` overflow and collision | Each version component saturates at 99 before packing, so the answer is at most `999999` and no name can overflow it. The collision it buys (`1.0.100` reports what `1.0.99` reports) is documented on the field and on `version_number`, and pinned by a test. `P(429497.0.0)` now answers `999999`-shaped bytes instead of panicking in a debug build. |
| F5 `public-api.txt` is not the public API | The script keeps only paths whose module is the crate root or one of the crate's `pub mod`s, read from `lib.rs` so a future `pub mod` needs no edit. 764 lines to 688; 164 item lines to 100. Both break tests run: a private-module rename passes, a public-item rename fails. |
| F6 rulings claimed but not recorded | `IN-0038.md` Open Decisions 2, 3, 5 and 6 are ticked with their ruling text and the provenance "Owner ruling 2026-09-15" -- the owner ruled in the coordination session that day and the coordinator relayed it. (This row said something else in an earlier draft; see "The provenance, corrected" below.) |
| F7 `ci-local` unrunnable with uncommitted work | Every gate runs `cargo package -p oneterm-vt --list` into the file-set check: offline, and with `--allow-dirty` locally it works with a dirty tree, which is the state `AGENTS.md` tells every agent to run the gate in. `cargo publish --dry-run` is gone from `ci.yml` too, because owner ruling 2026-09-15 made it unavailable. Proven by a green `-Full` run with an untracked file under `crates/vt/`. |
| F8 evidence over-claims | Every number in Evidence re-measured: 23 repository links in 23 files (not six), 67 files and 875.6 KiB (not 63/64 and 857.7 KiB), the `BUG-0051` lines identified as plain `//` comments, `tests/parser_limits.rs` cleaned, `src/terminal/verify_bug0058_tests.rs` renamed, and the harness snippet's file count corrected. |
| F9 dangling doc fragment | `grid/row.rs` fixed, and the whole file re-read for the same accident. |
| F10 three CI lists differ | `cargo run -p oneterm-vt --example headless` added to both `ci-local` scripts and to `AGENTS.md` section 4, which now also names the packaged-file-set step. |
| F11 rustdoc inaccuracies | All five: `scrollback_limit` says the grid clamps and the field reports what you asked for; `Terminal::colors` names `OSC 12` and the bright and dim keys; `Terminal::new` says it clamps the size; `VtEvent::Repaint` names `Terminal::render_update` and `RenderState` with resolving intra-doc links and no invented "snapshot"; `RowHeader::occ` has its "not checked by the integrity assertions" statement back. |
| F12 record ids in `Cargo.toml` | The `[features]` comment is rewritten with no record ids and no internal document names; `Cargo.toml.orig` ships, so it is the same reader. |
| F13 contract documents not reconciled | `packaging.md` and `api-surface.md` now describe what was built: the manifest as it is, the README's whole-file doctest route, and the HTML-reading public-API script. |
| F14 `AGENTS.md` missing from Reconciliation | Listed below. |

Two findings the verification recorded as correct-but-unproven stay open and are in Gaps: the E2E
walk, and the MSRV.

## Verification notes closed, second round

Re-verification at `2e1495c`'s successor `3fd76f7`: **PASS-WITH-NOTES**, 13 of the 14 first-round
findings closed, two mediums and six smaller notes raised. The report is the same file,
[`evidence/US-0097-verify.md`](evidence/US-0097-verify.md), section "Re-verification at `3fd76f7`".

| Finding | Closed by |
| --- | --- |
| R1(a) the Install block names a tag with no crate in it | `v0.5.2` predates `crates/vt`, and the crate inherits the application's version, so no existing tag can carry it. The README now pins a `rev`, names `branch = "main"` as the tracking form, and says tags work from the next release (`v0.5.3`) onward. |
| R1(b) a git dependency fails on Windows under the default `CARGO_HOME` | The cause was ours: `docs/spec-intakes/IN-0009-prevent-terminal-completion-.../` is a 118-character directory name, which put three files at 161 to 197 characters, and libgit2 honours neither `core.longpaths` nor `LongPathsEnabled`. Renamed to `IN-0009-completion-utf8-prefix-slicing/`, and `verify-dependency-graph.py` now fails any tracked path over 150 characters. Proven by a scratch consumer outside the repository, on the **default** `CARGO_HOME`: it builds and runs. |
| R2 the close-note's provenance | Corrected below: the owner ruled, the coordinator relayed. |
| R3 the API gate misses a nested `pub mod` | `vt-public-api.py` walks the module links rustdoc puts in each module's own index, so it reaches every publicly nameable module at any depth. The verifier's probe (`pub mod probe` inside `grid`) now fails the gate with `+struct oneterm_vt::grid::probe::Probe`; the private-module rename still passes. |
| R4 the sanitiser ran per query | It runs once, in `Terminal::new`, which stores the sanitised name. 2 000 queries against a 1 MiB control-only name: **11.64 s before, 5.8 ms after**. Seven hostile cases adopted as `tests/product_name_hostile.rs`. |
| R5 `IN-0038.md`'s body still described the pre-ruling plan | Six places aligned with Open Decision 2. |
| R6 residual numbers | Re-measured, not re-asserted: 68 files and 883.5 KiB packaged, `public-api.txt` at 16 KiB, 374 + 6 + 8 + 7 + 3 tests, 23 repository links in 23 files, and `tests/product_name.rs` named as 8 tests. |
| R7 the package gate's exit status | PowerShell captures the list and checks both statuses; the workflow step sets `pipefail`; the bash script already had it. Proven in both shells by pointing them at a package name that does not exist: each stops. |
| R8 a load-sensitive workspace test | Not this packet's; recorded in Gaps. |

### The provenance, corrected

An earlier draft of this packet said the four Open Decisions were recorded as "Coordinator default
2026-09-15, presented to the owner, no objection recorded". That is not what happened and not what
the record says. **The owner ruled on Open Decisions 2, 3, 5 and 6 in the coordination session on
2026-09-15**; the coordinator relayed those rulings, and `IN-0038.md` records them as
"Owner ruling 2026-09-15", which is correct. The verification was right to ask: the phrase had been
copied into seven documents including a shipped source comment, and the packet's own note
contradicted it.

### The consumer build, on the default `CARGO_HOME`

```console
$ cargo run                       # scratch crate outside the workspace, deleted afterwards
   Compiling memchr v2.8.3
   Compiling unicode-segmentation v1.13.3
   Compiling rustc-hash v2.1.3
   Compiling bitflags v2.13.2
   Compiling unicode-width v0.2.2
   Compiling oneterm-vt v0.5.2 (file:///D:/TrungKFC-Research/Rust/myTerm2?branch=feat%2Fvt-publishable#bf292660)
   Compiling vtconsumer v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.25s
     Running `target\debug\vtconsumer.exe`
title: "a title"
identity replies: "\u{1b}P>|vtconsumer(9.9.9)\u{1b}\\\u{1b}[>0;90909;1c"
rows: 24
```

Six leaf dependencies compiled and nothing else, `Config::product_name` reachable and answering the
consumer's own identity in both replies, and the README's first block compiling unmodified outside
the workspace.

### Harness note

The `IN-0009` rename moves two rows' paths. The coordinator updates `harness.db`; this session does
not write it.

- `doc_path` / `packet_doc` old:
  `docs/spec-intakes/IN-0009-prevent-terminal-completion-from-slicing-strings-at-invalid-utf-8-boundaries-when-matching-prefixes-from-history-or-catalogs/IN-0009.md`
  and `.../BUG-0011-prevent-unicode-prefix-match-crash.md`
- new: `docs/spec-intakes/IN-0009-completion-utf8-prefix-slicing/IN-0009.md` and
  `docs/spec-intakes/IN-0009-completion-utf8-prefix-slicing/BUG-0011-prevent-unicode-prefix-match-crash.md`

The third file, `high-level-design.md`, moves with them and is not referenced by the database.

## Harness row

`harness.db` was not written by this task. The story row, for whoever runs the harness:

```python
#!/usr/bin/env python3
"""Insert the US-0097 story row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    id="US-0097",
    title="oneterm-vt is a publishable crate whose documentation stands alone",
    created_at="2026-09-15T00:00:00",
    risk_lane="high_risk",
    contract_doc="docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/packaging.md",
    packet_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/US-0097-publishable-crate.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=1,
    evidence=(
        "cargo package packages 67 files incl. README, CHANGELOG, "
        "LICENSE, NOTICE and examples/headless.rs; 263 missing_docs warnings "
        "closed and 106 record citations removed across 44 files; sanitised "
        "Config::product_name with 8 tests; public-api.txt (688 lines) gated by "
        "scripts/vt-public-api.py; new vt-package CI job."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "Independently verified PASS-WITH-NOTES at 2e1495c "
        "(evidence/US-0097-verify.md); all 14 findings closed in the rework. "
        "No serde feature and no regex optional dep (IN-0038 Open Decision 3; "
        "regex is US-0100's). public-api.txt is read from rustdoc HTML because "
        "--output-format json is nightly-only, so signature changes are not "
        "detected. e2e_proof=0: the manual XTVERSION walk was not run because the "
        "owner runs this session inside OneTerm."
    ),
    intake_id=43,
)

with sqlite3.connect(DB) as db:
    db.execute(
        "INSERT INTO story ({}) VALUES ({})".format(
            ", ".join(ROW), ", ".join("?" * len(ROW))
        ),
        tuple(ROW.values()),
    )
print("inserted US-0097")
```

## Handoff

The self-containment pass is the part most likely to cross a session: 37 files, mechanical, and the
grep in Acceptance is the stop condition. Everything else is a day's work.
