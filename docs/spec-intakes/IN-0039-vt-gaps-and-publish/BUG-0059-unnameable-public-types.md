# Work: Four public methods return types nothing outside the crate can name

ID: BUG-0059
Intake: [`IN-0039`](IN-0039.md)
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
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

- [x] **The four types are nameable from outside the crate.** A doctest -- which compiles as an
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
- [x] **The gate catches the defect it was written for.**
      `python scripts/vt-public-api.py --check-nameable` run against `main`'s rustdoc **exits
      non-zero and names all four types together with the private module each is defined in**.
      Run against this branch it exits zero. Both outputs attached verbatim. A gate never seen to
      fail has not been tested, and this criterion fails if only the passing run is attached.
- [x] **The gate is in CI.** `.github/workflows/ci.yml`'s `vt-package` job runs
      `--check-nameable` beside the existing `--check` and `--diff-platforms`.
- [x] **The documentation build is clean with `missing_docs` on.**
      `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps` and the same with
      `--all-features` both exit zero.
- [x] **The surface diff is exactly four items.** After `--update` on both platforms,
      `git diff crates/vt/public-api.*.txt` shows the four added items with their fields and
      methods, and nothing else removed or renamed.
      `python scripts/vt-public-api.py --diff-platforms` still passes, so the two files still
      differ only inside `oneterm_vt::pty`.
- [x] **Nothing else in the workspace moved.** `cargo test --workspace` passes with no `#[allow]`
      added anywhere, and `pwsh scripts/ci-local.ps1` is green.
- [x] **The production diff is inside budget**: `crates/vt` +16 / -4 and
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

- [x] Add the four re-exports to `crates/vt/src/lib.rs`, in the existing alphabetical blocks.
- [x] Document `Placement`'s five fields and `SyncState`'s public methods; run
      `RUSTDOCFLAGS="-D warnings" cargo doc` until clean.
- [x] Add `#[non_exhaustive]` to `ResizeOutcome` and `Placement`, each with the one-line reason the
      doctrine asks for.
- [x] Cross-reference `Placement` and `SnapshotPlacement` in each other's docs.
- [x] Implement `--check-nameable` in `scripts/vt-public-api.py`; **run it against `main` first**
      and capture the failing output before writing any re-export.
- [x] Wire the new mode into the `vt-package` CI job.
- [x] Add the external-crate doctest.
- [x] Regenerate both surface files; on the platform that is not the host, use the `#` note the
      script already supports.
- [x] CHANGELOG entry; guide chapter 12's count and lists.

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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

E2E proof is **not applicable** and must be recorded as such rather than left blank: nothing a user
can see changes. Platform proof is the two surface files plus the `--no-default-features` build.

## Evidence and Gaps

Recorded, on `fix/vt-nameable-types` (`04246a79` the gate, `a93304d1` the fix):

**The gate fails on `main`.** `04246a79` adds the gate and nothing else, so this is `main`'s
surface:

```text
$ python scripts/vt-public-api.py --check-nameable --no-doc   # exit 1
oneterm-vt has public signatures naming types no embedder can write.
Re-export each from the crate root, or change the signature:
  oneterm_vt::Config: `CursorStyle` is not nameable (defined in `terminal::mode`)
  oneterm_vt::Terminal: `Placement` is not nameable (defined in `graphics`)
  oneterm_vt::Terminal: `ResizeOutcome` is not nameable (defined in `reflow`)
  oneterm_vt::Terminal: `CursorStyle` is not nameable (defined in `terminal::mode`)
  oneterm_vt::Terminal: `SyncState` is not nameable (defined in `snapshot::sync`)
```

**And it passes on the branch**: `every type in a public signature is nameable, but the 7 in
KNOWN_UNNAMEABLE`, exit 0.

**The external-crate proof.** A throwaway `nameprobe` crate outside the workspace, depending on
`crates/vt` by path, storing all four in struct fields and matching on `style.shape`. Against the
branch `cargo check` is clean; with only the three re-export files reverted to `main` it is

```text
error[E0432]: unresolved imports `oneterm_vt::CursorStyle`, `oneterm_vt::Placement`,
`oneterm_vt::ResizeOutcome`, `oneterm_vt::SyncState`
```

The same four names, from a real external crate rather than a doctest. That crate was deleted
after the run; the standing proof is the crate-root doctest, which `cargo test -p oneterm-vt
--doc` runs (36 passed).

**The surface diff** is +18 / -0 in each of `public-api.windows.txt` (generated on this Windows
host) and `public-api.unix.txt` (mirrored by hand under the `#` note it already carries, so the
Unix half is the weaker one): four items with their fields and methods, nothing removed or
renamed. `--diff-platforms` still reports six lines, all inside `oneterm_vt::pty`.

**Budget.** `crates/vt` is **+60 / -13** against a budget of +16 / -4, and
`scripts/vt-public-api.py` **+98 / -0** against +50. Measured with
`git diff --numstat main...HEAD`, excluding the two generated surface files. (Independent
verification measured +57 / -13 at `6d8a442c`; the three extra lines are the `Placement`
construction note its finding F8 asked for.) Both are over by more than half, so both are stated
rather than buried:

| Where | Cost | Budgeted? |
| --- | --- | --- |
| `crates/vt/src`: the four re-exports, two `#[non_exhaustive]` marks, `Placement`'s five fields, `SyncState::new`, the two cross-references, and the crate-root doctest | +40 / -9 | the +16 / -4 counted the first four items and neither the cross-references nor the doctest, both of which the Plan asks for |
| CHANGELOG and guide chapter 12 | +20 / -4 | the Documentation Action asks for both; the +16 counted neither |
| the two surface files | +36 | generated |
| the gate: its docstring, the ledger below and the ledger's staleness check | +98 | the +50 assumed the gate would find nothing but the four |

**Tests.** `cargo test -p oneterm-vt` in all three feature states (default,
`--no-default-features`, `--all-features`), `--doc`, and `cargo test --workspace`.
`RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps` and the same `--all-features`, both
clean. `pwsh scripts/ci-local.ps1`: 24 steps, `ci-local: all checks passed`. `-Full` additionally
runs `cargo deny`, which cannot reach `github.com/rustsec/advisory-db` through this machine's
proxy -- an environment failure, unrelated to this branch and reproducible on `main`.

**`crates/vt/README.md` unchanged**, as the packet predicted: it names none of the four.

**The gate found seven more instances of the same defect, and this packet does not fix them.**
Written as designed it fires on every public signature, and the crate has eleven unnameable
types, not four -- the four fixed here and these seven:

| Type | Defined in | Reached through |
| --- | --- | --- |
| `ModeState` | `terminal::mode` | `Mode::inert_state`, which returns `Option<ModeState>` -- an **enum** whose five variants have no path, so the value can be `{:?}`-printed and not matched, compared or converted. The worst of the seven, and the priority for the follow-up |
| `StrSpan`, `ByteSpan`, `ParamSpans` | `events::batch` | `VtEvent`'s variant payloads. Passing one straight back into `EventBatch::str` / `bytes` / `params` **does** compile, because the binding is inferred; what is impossible is writing the type down -- storing a span in a struct field or returning one from a helper |
| `ColorOverrides` | `terminal::color` | `Terminal::colors` |
| `Watermark` | `snapshot::state` | `SnapshotState::watermark` |
| `Invalidation` | `selection` | `Selection::invalidate` |

Re-exporting them is not in this packet's scope: each needs the same `#[non_exhaustive]` decision,
field documentation and CHANGELOG line the four got, and that is a designed packet rather than a
drive-by. The LLD's edge case offers "a re-export or an entry in a short allow-list" and says no
allow-list is added speculatively. These are not speculative -- they are seven named, verified
findings -- so they are a `KNOWN_UNNAMEABLE` ledger in the script. The ledger may only **shrink**:
a name in it that stops firing fails the gate too, so it cannot rot into a permanent exemption.
**[`BUG-0060`](IN-0039.md) is the proposed packet that empties it**, `ModeState` first. This is the one place the packet's "four
names and no more" scope and its "the gate passes on the branch" acceptance could not both hold,
and the ledger keeps both true without hiding the other seven.

Known gaps to state rather than discover:

- The LLD's stated algorithm -- collect every `<a class=...>` in the signature and flag the ones
  whose href resolves to a private module -- cannot work, and the implementation does the
  opposite. rustdoc renders a page only for a **publicly reachable** item, so it emits **no link
  at all** for a type in a `pub(crate)` module: there is no private-module href to find. The gate
  therefore flags a type this crate defines that appears **unlinked** in a rendered signature,
  which is the same question answered from the other side. Two consequences of reading unlinked
  text: enum variant names are unlinked too, so a variant opening its own line is stripped before
  the scan, and an identifier is reported only if this crate actually defines a type by that name.
- `Placement` is `#[non_exhaustive]` and derives no `Default`, so an external crate cannot
  construct one **at all**: no struct literal, no functional update (`E0639`), no
  constructor. That is deliberate and now documented on the type. No API in the crate takes
  a `Placement` -- they are returned by `Terminal::placements` and read -- so a `Default`
  would exist only to let an embedder synthesise one for its own painter test, which is a
  reason to add it when somebody has that test, not before. `ResizeOutcome` derives
  `Default` and has `pub` fields, so default-then-assign works there.
- The gate reads rendered rustdoc HTML, so it sees only **named types in the rendered signature**.
  A type reachable solely through an associated type, a where-clause bound or a macro-generated
  impl is not checked. Rustdoc JSON would fix this and is nightly-only against a pinned stable
  toolchain -- the same limit `scripts/vt-public-api.py` already documents for its main mode.
- The gate can only report a type it found a definition for. Its `DEFINITION` pattern
  matched `pub` and `pub(crate)` only, which made a `pub(super)` or `pub(in path)` type
  invisible; widened here to any parenthesised visibility, which is one regex alternation
  and no new finding today. The `NAMED` pattern still wants three characters or more, so a
  type named in one or two would be skipped -- none exists, and loosening it would read
  every generic parameter as a type.
- The gate cannot see a *signature change*, only an item added, removed or renamed. Unchanged.
- The Windows surface file is regenerated on a Windows host and the Unix one in CI (or by hand with
  the script's `#` note). Whichever half is produced by hand is the weaker half, and the packet
  should say which it was.

## Harness Row

The harness database is not edited by this packet's session. This is the row it owes, for whoever
applies it. The columns are `harness.db`'s real `story` schema, read from a copy of the shipped
database -- **not** the shape the `US-0104` packet's snippet uses, which does not match it:

```python
import sqlite3

with sqlite3.connect("harness.db") as db:
    db.execute(
        """INSERT INTO story (
            id, title, created_at, risk_lane, contract_doc, packet_doc, status,
            unit_proof, integration_proof, e2e_proof, platform_proof,
            evidence, verify_command, last_verified_at, last_verified_result,
            notes, intake_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
        (
            "BUG-0059",
            "Four public methods return types nothing outside the crate can name",
            "2026-09-15",
            "high_risk",
            "docs/spec-intakes/IN-0039-vt-gaps-and-publish/low-level-design/api-surface.md",
            "docs/spec-intakes/IN-0039-vt-gaps-and-publish/BUG-0059-unnameable-public-types.md",
            "implemented",
            1, 1, 0, 1,
            "docs/spec-intakes/IN-0039-vt-gaps-and-publish/evidence/BUG-0059-verify.md",
            "pwsh scripts/ci-local.ps1",
            "2026-09-16",
            "pass",
            "E2E not applicable: nothing a user can see changes. cargo deny is proxy-blocked "
            "on this host and was not run.",
            44,
        ),
    )
```

`e2e_proof` is 0 on purpose, for the reason the Proof block already gives.

## Handoff

Next owner after this packet: any of `US-0105`, `US-0106` or `US-0107`, which are mutually
independent and each regenerate the surface files again. Blockers: none. This packet depends on
nothing.
