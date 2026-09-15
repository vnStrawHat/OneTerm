# Work: Four public methods return types nothing outside the crate can name

ID: BUG-0059
Intake: [`IN-0039`](IN-0039.md)
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

- Change type: **bug**
- Risk lane: high_risk (inherited: the crate's public surface is an external contract)
- Spec Intake, when required: [`IN-0039`](IN-0039.md)

## Outcome

Four `pub` methods on `Terminal` return types that no crate outside `oneterm-vt` can write down.
After this packet all four are nameable at a documented path, and a machine gate fails the build
the next time a public signature refers to a private type.

The defect, reported by an outside developer who hit it while building against the crate:

```text
Terminal::resize(..)     -> ResizeOutcome    error[E0425]: cannot find type `ResizeOutcome`
Terminal::cursor_style() -> CursorStyle      error[E0425]: cannot find type `CursorStyle`
Terminal::sync()         -> &SyncState       (module `terminal` is private)
Terminal::placements()   -> &[Placement]     (module `graphics` is private)
```

The values are usable by field access, so this is not a functional break -- it is a break in what
an embedder can *write*: the types cannot be stored in a struct, returned from a function, or
matched on by path.

## Scope

- [x] In scope:
  - Re-export `ResizeOutcome`, `CursorStyle`, `SyncState` and `Placement` from the crate root.
  - Document `Placement`'s five fields and `SyncState`'s undocumented public methods, which
    `#![warn(missing_docs)]` starts requiring the moment they become publicly reachable.
  - Mark `ResizeOutcome` and `Placement` `#[non_exhaustive]`.
  - Add `--check-nameable` to `scripts/vt-public-api.py` and wire it into the `vt-package` CI job.
  - Regenerate `crates/vt/public-api.windows.txt` and `crates/vt/public-api.unix.txt`.
  - A CHANGELOG entry naming the four items.
- [x] Out of scope:
  - Renaming `Placement` to `GraphicPlacement`, or deleting `Terminal::placements()`. Both were
    considered; see [`low-level-design/api-surface.md`](low-level-design/api-surface.md).
  - Any other item on the public surface. This packet adds four names and no more.
  - `Config`'s missing `#[non_exhaustive]` mark -- a real drift finding, owned by `US-0106`, which
    is the packet that adds a `Config` field.
  - Anything to do with publishing. `publish = false` is untouched.

## Acceptance

Each criterion is a command a hostile verifier can run, with a stated expected result.

- [ ] **The four types are nameable from outside the crate.** A doctest -- which compiles as an
      external crate, the only place this can be proven -- contains:

      ```rust
      use oneterm_vt::{CursorStyle, Placement, ResizeOutcome, SyncState};
      let _: Option<ResizeOutcome> = None;
      let _: Option<CursorStyle> = None;
      let _: Option<SyncState> = None;
      let _: Option<Placement> = None;
      ```

      `cargo test -p oneterm-vt --doc` passes. On `main` the same doctest fails with four
      `E0432`/`E0425` errors; both outputs are attached.
- [ ] **The gate catches the defect it was written for.**
      `python scripts/vt-public-api.py --check-nameable` run against `main`'s rustdoc **exits
      non-zero and names all four types together with the private module each is defined in**.
      Run against this branch it exits zero. Both outputs attached verbatim. A gate never seen to
      fail has not been tested, and this criterion fails if only the passing run is attached.
- [ ] **The gate is in CI.** `.github/workflows/ci.yml`'s `vt-package` job runs
      `--check-nameable` beside the existing `--check` and `--diff-platforms`.
- [ ] **The documentation build is clean with `missing_docs` on.**
      `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps` and the same with
      `--all-features` both exit zero.
- [ ] **The surface diff is exactly four items.** After `--update` on both platforms,
      `git diff crates/vt/public-api.*.txt` shows the four added items with their fields and
      methods, and nothing else removed or renamed.
      `python scripts/vt-public-api.py --diff-platforms` still passes, so the two files still
      differ only inside `oneterm_vt::pty`.
- [ ] **Nothing else in the workspace moved.** `cargo test --workspace` passes with no `#[allow]`
      added anywhere, and `pwsh scripts/ci-local.ps1` is green.
- [ ] **The production diff is inside budget**: `crates/vt` +16 / -4 and
      `scripts/vt-public-api.py` +50, measured with `git diff --stat` and attached. A diff more
      than 50 per cent over budget is a finding to explain in Evidence, not a silent overrun.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/api-surface.md`](low-level-design/api-surface.md) -- this packet's owning
  design: the four types, the `#[non_exhaustive]` decision per type, the two rejected alternatives,
  and the gate's algorithm and stated limits.
- [`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
  -- the accepted public surface, the nine-clause semver promise, and the `#[non_exhaustive]`
  doctrine that decides which of the four get the mark.
- `crates/vt/src/lib.rs` -- the re-export block and its header comment explaining which modules are
  public and why.
- `crates/vt/docs/guide/12-versioning.md` -- counts the `#[non_exhaustive]` types ("Eight public
  types are marked"). Two more are marked here, so the count and both lists change.
- `scripts/vt-public-api.py` -- its docstring states what the gate does and does not catch; the new
  mode extends both halves of that statement.
- `crates/vt/CHANGELOG.md` -- clause 2 of the promise makes four new re-exports a **patch**-level
  entry.

### Documentation Action

**Update required.** Four owning docs must change with the code:

| Doc | Change |
| --- | --- |
| `crates/vt/CHANGELOG.md` | an `### Added` entry under `[Unreleased]` naming all four types, and stating that they were previously returned but unnameable |
| `crates/vt/docs/guide/12-versioning.md` | the `#[non_exhaustive]` count goes from eight to ten, and `ResizeOutcome` and `Placement` join the struct list -- which changes what a caller must do to construct them |
| `scripts/vt-public-api.py` docstring | the new mode, what it checks, and its three stated limits |
| `crates/vt/public-api.{windows,unix}.txt` | regenerated; they are committed artefacts of the surface, not derived files |

Reason: this is a change to the crate's public surface, and the crate's own promise says every such
change is a CHANGELOG entry naming the item. The guide's count is a published number that would
become wrong.

`crates/vt/README.md` needs **no** change: it does not enumerate types.

### Reconciliation

Before completion, list the docs actually changed and confirm the guide's `#[non_exhaustive]` count
matches `grep -c non_exhaustive` over the public types.

## Context

- `graphics::Placement`'s five fields (`id`, `anchor`, `cols`, `rows`, `pixel_size`) carry **no**
  doc comments today. They are exempt from `missing_docs` only because their module is
  `pub(crate)`. This is why the packet is +16 rather than the +4 the evaluation estimated, and it
  is the packet's largest single cost.
- `Placement::anchor` is an `AnchorId`, which is already public at `oneterm_vt::grid::AnchorId`. No
  second type has to be exposed.
- `SyncState`'s fields are all private, so it needs no `#[non_exhaustive]` -- it is already
  unconstructible from outside.
- `Placement` will sit beside the existing public `SnapshotPlacement`. They are different types:
  `Placement` names a position by `AnchorId` (it follows its content through scroll and reflow),
  `SnapshotPlacement` by resolved `RowId` and column. Each one's doc comment gains a line naming
  the other.
- `scripts/vt-public-api.py` already walks every public module and opens every item page. The new
  mode reuses both, which is why it is about 50 lines and not a parser project.

## Plan

- [ ] Add the four re-exports to `crates/vt/src/lib.rs`, in the existing alphabetical blocks.
- [ ] Document `Placement`'s five fields and `SyncState`'s public methods; run
      `RUSTDOCFLAGS="-D warnings" cargo doc` until clean.
- [ ] Add `#[non_exhaustive]` to `ResizeOutcome` and `Placement`, each with the one-line reason the
      doctrine asks for.
- [ ] Cross-reference `Placement` and `SnapshotPlacement` in each other's docs.
- [ ] Implement `--check-nameable` in `scripts/vt-public-api.py`; **run it against `main` first**
      and capture the failing output before writing any re-export.
- [ ] Wire the new mode into the `vt-package` CI job.
- [ ] Add the external-crate doctest.
- [ ] Regenerate both surface files; on the platform that is not the host, use the `#` note the
      script already supports.
- [ ] CHANGELOG entry; guide chapter 12's count and lists.

## Decisions

No new decision record. The `#[non_exhaustive]` doctrine and the semver promise this packet applies
are already accepted in
[`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md);
the per-type application and the two rejected alternatives are recorded in this intake's
[`low-level-design/api-surface.md`](low-level-design/api-surface.md), which is the right altitude
for them.

## Verification Plan

- `cargo test -p oneterm-vt --doc` -- the nameability doctest, and the same doctest failing on
  `main`.
- `python scripts/vt-public-api.py --check-nameable` on `main` (must fail, naming four types) and
  on the branch (must pass).
- `python scripts/vt-public-api.py --check` and `--diff-platforms`.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps`, and with `--all-features`.
- `cargo test --workspace` -- `crates/terminal` and `crates/terminal-view` are separate crates, so
  the compiler is what proves `#[non_exhaustive]` breaks nothing there.
- `cargo build -p oneterm-vt --no-default-features` -- the four types are all outside `pty`, so the
  transport-free build must gain them too.
- `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

E2E proof is **not applicable** and must be recorded as such rather than left blank: nothing a user
can see changes. Platform proof is the two surface files plus the `--no-default-features` build.

## Evidence and Gaps

After implementation, record: the failing `--check-nameable` run against `main` verbatim; the
passing run; the failing and passing doctest; `git diff --stat` against the budget; the surface
file diff; and the `ci-local` output.

Known gaps to state rather than discover:

- The gate reads rendered rustdoc HTML, so it sees only **named types in the rendered signature**.
  A type reachable solely through an associated type, a where-clause bound or a macro-generated
  impl is not checked. Rustdoc JSON would fix this and is nightly-only against a pinned stable
  toolchain -- the same limit `scripts/vt-public-api.py` already documents for its main mode.
- The gate cannot see a *signature change*, only an item added, removed or renamed. Unchanged.
- The Windows surface file is regenerated on a Windows host and the Unix one in CI (or by hand with
  the script's `#` note). Whichever half is produced by hand is the weaker half, and the packet
  should say which it was.

## Handoff

Next owner after this packet: any of `US-0105`, `US-0106` or `US-0107`, which are mutually
independent and each regenerate the surface files again. Blockers: none. This packet depends on
nothing.
