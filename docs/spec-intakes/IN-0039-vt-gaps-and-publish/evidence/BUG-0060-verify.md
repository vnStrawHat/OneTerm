# Independent verification: BUG-0060, the last seven unnameable public types

Verifier: independent agent, worktree `.claude/worktrees/agent-a533a4b44b086d845`, branch reset to
`fix/vt-nameable-types-2` `428e9951` (base `main` `980bf5da`). Nothing committed, nothing pushed,
`harness.db` read from a copy only. `CARGO_BUILD_JOBS=2` throughout.
Diffstat against `main`: 13 files, +283 / -81 -- matches the packet.

## Verdict

**PASS WITH NOTES.** Every load-bearing claim reproduces. All seven types are nameable from a real
external crate and usable, not merely importable: the four spans/watermark go in struct fields with
real values, `ModeState` matches exhaustively on all five variants, `Invalidation` is constructed
outside and passed to `Selection::invalidated_by`, and `ColorOverrides::get`/`iter` are callable
while all four narrowed mutators are `E0624`. The gate is strict with the ledger gone -- it fails on
a freshly introduced private type in a public signature and on a reverted re-export. The production
diff is visibility, re-export and documentation only. `ci-local.ps1 -Full` is green. The notes are
record accuracy; none is a defect in shipped code.

## Findings

| # | Severity | Finding |
| --- | --- | --- |
| F1 | medium | **The harness snippet contradicts the packet it belongs to.** `BUG-0060-remaining-unnameable-types.md:493-530` proposes `status="planned"`, `unit_proof=0`, `integration_proof=0`, `platform_proof=0`, `evidence=None`, `last_verified_*=None`, while the packet's own blocks tick `Implemented` (line 17) and unit/integration/platform/verify proof (lines 344-350). Every sibling row already in `harness.db` is `implemented` with `1,1,0,1` (`BUG-0059`, `US-0106`) . Applied as written, the row would misreport the packet. The columns themselves are correct: they match the real `story` schema exactly, and `intake_id=44` is right (`intake.id=44` is IN-0039, verified on a copy of `harness.db`). Recommended row: `status='implemented'`, `unit_proof=1`, `integration_proof=1`, `e2e_proof=0`, `platform_proof=1`, `last_verified_result='pass'`. |
| F2 | low | **The intake row's notes stop before this packet.** `intake.notes` for id 44 reads "IN-0039 COMPLETE 2026-09-16: BUG-0059, US-0105, US-0106, US-0107 merged into main" while `intake.story_id` already lists `BUG-0060`. Whoever applies F1's row should extend the note, or a reader will conclude the intake closed without its last packet. |
| F3 | low | **Status block leaves `Planned` unticked.** `BUG-0060-remaining-unnameable-types.md:14` -- `- [ ] Planned` / `- [x] Implemented`. `BUG-0059-unnameable-public-types.md` and `docs/templates/work.md` both tick `Planned` and keep it ticked. Cosmetic, but it is the block the harness synchronizes. |
| F4 | low | **A dangling packet link, inherited.** `BUG-0059-unnameable-public-types.md:289` writes "**[`BUG-0060`](IN-0039.md) is the proposed packet**" -- the link target is the intake, not `BUG-0060-remaining-unnameable-types.md`. Pre-existing on `main`; one word to fix while the file is open. |
| F5 | info | **`#[non_exhaustive]` on `Invalidation` does cost an outside `match`, and the guide says so.** Verified from the external crate: an exhaustive seven-arm match is `error[E0004]: non-exhaustive patterns: `_` not covered ... `Invalidation` is marked as non-exhaustive`. The packet's claim is that embedders construct rather than match, which is true of the only public consumer (`Selection::invalidated_by`), and `12-versioning.md:66-72` states the wildcard cost explicitly. No change wanted; recorded so the trade-off is on the record from outside the crate. |

## What was verified, and how

**1. The gate is strict, and green.** `RUSTDOCFLAGS=-D warnings cargo doc -p oneterm-vt --no-deps`
then `python scripts/vt-public-api.py --check-nameable --no-doc` -> **exit 0**,
`every type in a public signature is nameable`, no ledger tail.
`grep -n KNOWN_UNNAMEABLE scripts/vt-public-api.py` -> nothing; the only surviving `seen` is the
docstring's limits sentence (`scripts/vt-public-api.py:21`), as claimed.

*Strictness, break A (a new private type).* Added
`pub fn probe_unnameable(&self) -> Option<crate::terminal::ClusterCarry>` to `Selection`
(`crates/vt/src/selection/mod.rs:226`), rebuilt docs -> **exit 1**:
`oneterm_vt::Selection: `ClusterCarry` is not nameable (defined in `terminal::dispatch`)`. Reverted.
(The brief asked for a type under `crate::grid::anchor`; that module's four types -- `Anchor`,
`AnchorId`, `AnchorKind`, `Anchors` -- are all re-exported through the **public** `grid` module
(`crates/vt/src/grid/mod.rs:22`), so none of them is unnameable and none would prove anything.
`ClusterCarry` (`crates/vt/src/terminal/dispatch.rs:52`, `pub(crate)` inside a `pub(crate)` module)
is the equivalent that actually is private.)

*Strictness, break B (a reverted re-export).* Removed `Invalidation` from
`crates/vt/src/lib.rs:109`, rebuilt docs -> **exit 1**:
`oneterm_vt::Selection: `Invalidation` is not nameable (defined in `selection`)`. Restored; tree
clean at `428e9951`.

**2. External crate probe** (scratchpad, path dependency, own `[workspace]` and `CARGO_TARGET_DIR`,
deleted after the run). Positive half, `cargo run`:

```
probe ok: title="hello" reply_bytes=11 osc_params=4 watermark=Watermark(SeqNo(0))
inert=["not supported", "reset", "reset"] cleared=6/7 overrides=0
```

It stores `StrSpan`, `ByteSpan`, `ParamSpans` and `Watermark` in `struct Probe` fields with **real
values** drained from a live `feed` (`OSC 0` title, `CSI c` reply, `OSC 777` forwarded), reads
`Watermark`'s public `.0` and its `Ord`, matches `ModeState` **exhaustively across all five
variants** by path in a `fn describe(ModeState)`, walks `Mode::inert_state()` over four modes,
constructs **all seven** `Invalidation` variants outside the crate and passes each to
`Selection::invalidated_by` (six clear a fresh `Simple` selection at row 0), and names
`&ColorOverrides` in its own signature before calling `get(ColorKey::Foreground)` and `iter()`.
The only compile errors on the first attempt were my own (`Pos.row` needs `RowId`, and `OSC 777`
needs an `OscRoute::Forward` route) -- all seven names resolved from the crate root immediately.

Negative half, `cargo check` on a second bin, all errors expected:
`error[E0624]: method `set` is private`, and the same for `reset`, `reset_indexed`, `reset_all`;
plus the `E0004` in F5. `ColorOverrides::get`/`iter` compile from outside. Probe deleted.

**3. No behaviour change.** `git diff main...HEAD -- crates/vt/src` is seven files and every hunk is
one of: a name added to an existing `pub use` (`events/mod.rs`, `lib.rs`, `selection/mod.rs`,
`snapshot/mod.rs`, `terminal/mod.rs`), `pub` -> `pub(crate)` on four `ColorOverrides` methods
(`crates/vt/src/terminal/color.rs:120,130,137,148`), `#[non_exhaustive]` + doc lines on
`Invalidation` (`crates/vt/src/selection/mod.rs:85`), five new doc comments, two intra-doc links to
the private `Anchors` demoted to code spans (`crates/vt/src/selection/mod.rs:73-74`), and one dead
`pub(crate) use color::ColorOverrides;` folded into the `pub use` beside it. No expression changes.

Tests, all **exit 0**: `cargo test -p oneterm-vt` **819**, `--no-default-features` **791**,
`--all-features` **832**, `--doc` **44**, `cargo test -p oneterm-tools` **16** -- every number
matches the packet. `crates/tools/tests/corpus_check.rs` is inside that last run and
`crates/tools/src/corpus_replay.rs:319` still reads `term.colors().iter()`, unchanged, compiling
against the narrowed type.

**4. The doctest fails before the re-exports and passes after.** With the doctest kept and only the
five root `pub use` lines rewritten to `main`'s text, `cargo test -p oneterm-vt --doc` ->
`error[E0432]: unresolved imports `oneterm_vt::ByteSpan`, `oneterm_vt::ColorOverrides`,
`oneterm_vt::Invalidation`, `oneterm_vt::ModeState`, `oneterm_vt::ParamSpans`,
`oneterm_vt::StrSpan`, `oneterm_vt::Watermark``, `43 passed; 1 failed`. Restored -> `44 passed`.
Seven unresolved imports, exactly as the packet quotes.

**5. Surface and docs.** `RUSTDOCFLAGS=-D warnings cargo doc -p oneterm-vt --no-deps` and the same
`--all-features`: both exit 0, no warning. `vt-public-api.py --check --no-doc` exit 0
(`public API surface unchanged`), `--check-nameable --no-doc` exit 0, `--diff-platforms` exit 0,
`the delta is 6 lines, all inside `oneterm_vt::pty`` (`Options::escape_args`, `PipeReader`,
`PipeWriter` / `Options::child_signal_mask`, `SignalMask`, `SignalMask::current`).

Snapshot deltas vs `main`: `public-api.windows.txt` **+22 / -0**, `public-api.unix.txt`
**+23 / -1**. The 22 added lines are **byte-identical** in both files (diffed line by line); the
unix file's extra pair is only its `#` header date, `2026-09-15` -> `2026-09-16`, which correctly
declares it hand-derived. `ColorOverrides` lists `method get` and `method iter` and nothing else in
both files (`public-api.windows.txt:84`, `public-api.unix.txt:88`); no `fn set` or `fn reset*`
appears under it. Nothing removed or renamed anywhere.

**6. Records.** Guide chapter 12's counts regenerate from source:
`grep -rn -A 3 '#\[non_exhaustive\]' crates/vt/src/ | grep -E 'pub (struct|enum)'` yields
**fourteen** -- nine enums (`Invalidation`, `KeyEventKind`, `KeySpec`, `NamedKey`, `OscRoute`,
`Progress`, `SearchPattern`, `ShellMark`, `VtEvent`) and five structs (`KeyEvent`, `ModeSnapshot`,
`Placement`, `ResizeOutcome`, `SearchOptions`) -- exactly what `12-versioning.md:62,66-72,74`
now says. The CHANGELOG has the `### Added` entry naming all seven with the signature each was
reached through and singling out `Invalidation` as un-callable, **and** a `### Changed` entry
headed "Breaking on paper only" for the four narrowed mutators with the unreachability argument
(`crates/vt/CHANGELOG.md:57-71,224-228`) -- the breaking note the brief asked for is present.
`crates/vt/README.md` and `crates/vt/docs/guide/04-events.md` are correctly unchanged: neither
claims a span cannot be named.

Budget disclosure checks out to the line. `git diff --numstat main...HEAD`: `crates/vt/src` +54/-15,
CHANGELOG +20/-0, guide +8/-6 = **+82 / -21** against a +55/-12 budget, and
`scripts/vt-public-api.py` **+5 / -26** against +2/-22 -- the packet's own table, unrounded, with
the 49-per-cent overrun stated rather than buried.

**7. `pwsh scripts/ci-local.ps1 -Full`** -- **`ci-local: all checks passed.`**, exit 0, **26**
steps. That covers `cargo fmt --check`, both `clippy --workspace --all-targets -D warnings` runs,
`cargo test --workspace`, `oneterm-vt` under `vt-paranoid`, `regex` and `--no-default-features`,
the `--no-default-features` and `--all-features` example builds, `cargo run --example headless`,
both `-D warnings` doc builds, all three `vt-public-api.py` modes, `cargo package --list` against
`verify-dependency-graph.py`, both rustdoc self-containment greps, `verify-dependency-graph`,
`check-doc-paths`, `check-english` (905 files), `completion-catalog`, `third-party-notices`, and
-- unlike the `BUG-0059` verification, where it was network-blocked --
**`cargo deny check licenses bans advisories`**, which ran and emitted only pre-existing
`license-exception-not-encountered` and duplicate-crate warnings unrelated to this branch.

## What could not be verified

- **Unix.** Everything ran on Windows. `public-api.unix.txt` is hand-mirrored (its header says so)
  and its first real check is the Linux CI job. Its correctness here rests on the argument that
  nothing in this packet is `cfg`-gated, which the source diff supports but does not prove.
- **`.github/workflows/ci.yml`.** Read, not executed; no CI run exists for this branch.
- **`harness.db`.** Read from a copy; nothing written, as instructed. F1 and F2 are therefore
  recommendations, not applied changes.
- **The gate's four stated limits.** Unchanged by this packet and untested here: a type reached
  only through an associated type, a where-clause bound or a macro impl, or named in one or two
  characters, is still invisible. "The seven are the last seven" remains a claim about what the
  gate can see.

Full `ci-local -Full` output: session scratchpad `ci-local.log` (not committed).
