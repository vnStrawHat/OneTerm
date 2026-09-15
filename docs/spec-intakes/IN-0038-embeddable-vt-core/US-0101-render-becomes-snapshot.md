# Work: the engine's read model is called snapshot, not render

ID: US-0101
Intake: IN-0038
Created: 2026-09-15

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (rename only)
- Risk lane: normal
- Spec Intake: `IN-0038`

## Outcome

The module `crates/vt/src/render/` becomes `snapshot/`, and the seven `Render*` types become
`Snapshot*`. Nothing an embedder can observe at runtime changes. The crate stops claiming, in the
name of its largest public module, that it draws something -- which is the first thing a reader of
"an embeddable terminal core that contains no rendering" will check.

## Scope

- [x] In scope: the module directory and its `mod.rs`; `RenderState`, `RenderUpdate`, `RenderRow`,
  `RenderContent`, `RenderCell`, `RenderCursor`, `RenderPlacement`; `Terminal::render_update`; the
  `EngineView` internal alias; every use site in `crates/terminal`, `crates/terminal-view` and
  `crates/tools`; the collision rename of `oneterm_terminal::SnapshotCell` to `ContentCell`.
- [x] Out of scope: `ModeSnapshot`, `Palette`, `StyleRun`, `MouseEncoding`, `MouseProtocol`,
  `MouseReporting` -- none of them says "render" and all keep their names.
- [x] Out of scope: `crates/terminal-view`'s own `render/` module, which genuinely does draw. It
  keeps its name; that is the point of the distinction.
- [x] Out of scope: any change to the damage model, `SeqNo`, `RowId` or what a pull returns.

## Acceptance

- [x] `grep -rn '\bRender[A-Z]' crates/vt/ crates/terminal/ crates/tools/` returns **0** lines of
  Rust. (`crates/terminal-view` keeps its own render types and is excluded on purpose.) The literal
  grep returns 15: the CHANGELOG's own old -> new table, and alacritty's `RenderApi` inside the
  frozen parity recordings. See Evidence.
- [x] `grep -rn 'render_update' crates/` returns 0 lines outside that same CHANGELOG table.
- [x] `crates/terminal-view/src/render/` still exists and still contains that crate's own
  `Render*` types.
- [x] `oneterm_terminal::SnapshotCell` is now `ContentCell`; the four files naming it are updated;
  no type named `SnapshotCell` exists in `crates/terminal`.
- [x] `cargo test --workspace` green with **no** test assertion text changed. A verifier confirms
  with `git diff` that every changed line in a test file is a type name or an import, never an
  expected value.
- [x] The 45 frozen parity corpus recordings replay byte-identically.
- [ ] `git diff --shortstat` shows a net line delta within +-5. A rename that grows the tree has
  changed something it should not have. **Missed by one: net +6** over the `.rs` files. Every one
  of the six is rustfmt re-wrapping a line that a two-character-longer name pushed past the margin
  (three files, itemised under Evidence). Nothing was added; the criterion is reported as it
  stands rather than adjusted.
- [x] The public-API snapshot (from `US-0097`, split per platform by `US-0104`: regenerate the
  host's file with `--update` and hand-apply the same renames to the other, then confirm
  `--diff-platforms`) regenerates to a diff that contains **only**
  renames: every removed path has a matching added path differing by `Render` -> `Snapshot`.
- [x] `cargo doc -p oneterm-vt --no-deps` warning-free; no doc link broken by the rename.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` -- the owning
  design for this module. Its title, its type names and its prose all say "render". **Stale after
  this packet.**
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` -- the accepted
  decision behind the incremental read model. A decision record is a historical document and is
  **not** rewritten; instead it gains one line at the top noting the rename and the packet that did
  it.
- `docs/terminal-backend.md` -- section 5.2 "Snapshot vs live borrow" already uses the right word
  for the concept and mentions the engine types by name; those mentions change.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/` -- the owning design for
  `crates/terminal-view`, which consumes `RenderUpdate` and draws. Its references change; its own
  vocabulary does not.
- `docs/agents/structure.md` -- the directory tree names `crates/vt/src/render/`.

### Documentation Action

**Update required**: `damage-and-render-state.md` (type names and prose; the file name stays, since
renaming a design document breaks every link into it and buys nothing),
`docs/terminal-backend.md` section 5.2, the `IN-0018` references, `docs/agents/structure.md`, and a
one-line note at the top of `DEC-0015`.

Reason: five documents name types that will not exist, and one of them (`terminal-backend.md`) is in
`check-doc-paths.py`'s `DOCUMENTS` list.

### Reconciliation

Changed:

1. `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` -- type names,
   the `crates/vt/src/snapshot/` path, the "snapshot state" prose and the title. **File name kept**,
   and a note at the top says what `US-0101` renamed.
2. `docs/terminal-backend.md` -- section 5.2 and the § 3 output path (`snapshot_update`,
   `SnapshotState`, `SnapshotRow` / `SnapshotCell`) and the `content.rs` line of its tree.
3. `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` -- the seven
   places that named the **engine's** types. Its own `RenderState`, `RenderInputs` and
   `src/render/*` file table are untouched, which is the distinction the packet is about.
4. `docs/agents/structure.md` -- the `crates/vt/src/` tree entry.
5. `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` -- **annotated, not
   rewritten**: one block at the top names the rename and says the record keeps its original
   wording. The body, the title and the file name are byte-identical to before.

Also changed, beyond the five the packet named, because they describe current engine code and would
otherwise name types that no longer exist:

6. `docs/architecture.md` -- the `crates/vt/src/render/` path, which `scripts/check-doc-paths.py`
   validates and which would have failed the gate, plus the row's prose.
7. `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` and the seven other current-mechanics
   files under its `low-level-design/` (`cell-and-style`, `events-and-api`, `graphics`,
   `grid-and-scrollback`, `reflow-and-resize`, `selection`, `testing-and-bench`) -- the same
   mechanical map.

Deliberately **not** changed, as records of work already done: `IN-0029.md`, the `US-00xx` packets
and everything under `IN-0029-vt-engine/evidence/` and `research/`, and
`low-level-design/migration.md` (a completed migration from the forked engine, whose file
references no longer exist either way). `IN-0038`'s own `api-surface.md` and `high-level-design.md`
already name the post-rename types.

## Context

Use-site counts outside `crates/vt`, measured on `main` @ `36977ca`:

| Type | Uses |
| --- | ---: |
| `RenderState` | 24 |
| `RenderUpdate` | 23 |
| `RenderContent` | 8 |
| `RenderRow` | 7 |
| `RenderCell` | 3 |
| `RenderCursor` | 3 |
| `RenderPlacement` | 3 |

The collision: `crates/terminal/src/content.rs:119` already publishes `SnapshotCell`, named by
`crates/terminal-view/src/terminal_view/completion.rs` and `src/url/detect.rs`. It is an owned,
narrowed row cell for the URL and completion scanners -- `ContentCell` is the accurate name and the
adapter is the crate that should yield, since the engine's use of "snapshot cell" is the literal one.

The naming rationale is in [`low-level-design/api-surface.md`](low-level-design/api-surface.md),
section "Renamed: render -> snapshot", and is the answer to owner decision (c).

## Plan

- [x] Rename `oneterm_terminal::SnapshotCell` to `ContentCell` first, alone, so the engine rename
  cannot collide. Four files.
- [x] `git mv crates/vt/src/render crates/vt/src/snapshot` and rename the seven types and
  `render_update`. The compiler finds every site; do not grep-replace across the workspace, because
  `crates/terminal-view`'s own `Render*` types must survive.
- [x] Update the five documents and annotate `DEC-0015`.
- [x] Regenerate the host's public-API snapshot and mirror the renames into the other
  platform's file; confirm the diff is renames only and `--diff-platforms` still passes.
- [x] CHANGELOG line under `Unreleased` / `Changed`, naming every renamed item, because this is
  exactly the kind of change the semver promise says must be named.

## Decisions

None of its own. Owner decision (c) settles that the rename happens; the choice between `snapshot`
and `frame` is argued in `api-surface.md` and is a naming judgement, not a constraint future work
inherits -- it needs no decision record.

## Verification Plan

- Focused: none new. A rename with no behaviour change needs no new test; a new test here would
  assert that the compiler works.
- Unit: `cargo test --workspace` with no assertion text changed.
- Integration: the parity corpus replay.
- Platform: `cargo doc -p oneterm-vt --no-deps`; the public-API snapshot diff;
  `pwsh scripts/ci-local.ps1`; `python scripts/check-doc-paths.py` (which covers
  `terminal-backend.md`).
- E2E: one manual Windows launch, enough to confirm the app still draws. A rename that compiles and
  passes the corpus cannot plausibly break rendering, so this is a smoke check, not a walk.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Branch `refactor/vt-snapshot`, three commits on top of `main` @ `98a72148`.

- `grep -rnE 'Render[A-Z]' crates/vt crates/terminal crates/tools` -> **0 lines of Rust**. It is
  not literally 0 lines: 7 are the CHANGELOG's own old -> new table, which has to name the old
  types, and 8 are inside the frozen parity recordings, where the captured `vim` session is editing
  alacritty's `RenderApi`. Neither is source this packet may touch.
  `grep -rnE 'render_update' crates/` -> the same one CHANGELOG table row, nothing else.
  `crates/terminal-view/src/render/` still holds that crate's `RenderState`, `RenderInputs` and
  `RenderImage`.
- `crates/terminal/src/content.rs` publishes `ContentCell`; no `SnapshotCell` is defined in
  `crates/terminal`. `SharedTerminal::render_demand_raised` (`US-0090`) is untouched -- it is the
  adapter's own primitive and says nothing about this module.
- `git diff main HEAD --shortstat -- 'crates/***.rs'`: 36 files changed, 324 insertions(+),
  318 deletions(-) -- **net +6**, one line over the +-5 the criterion names. All six are rustfmt
  re-wrapping a `use` list or an `assert_eq!` that a two-character-longer name pushed past the
  margin: `crates/vt/src/lib.rs` +1, `crates/vt/src/snapshot/snapshot_tests.rs` +3,
  `crates/vt/tests/engine_without_pty.rs` +2. Nothing was added.
- Mechanical proof that the `.rs` diff is only renames: applying the rename map to each changed
  file's **old** text yields the same word multiset as its new text, for 36 of 36 changed `.rs`
  files (6 of them only after allowing rustfmt's reflow, listed above).
- `cargo test --workspace`: green. No test assertion text changed -- every changed line in a test
  file is a type name, an import or a comment.
- `vt-corpus check`: **45 recordings, 45 passed, 0 failed**. (The packet said 46; the vendored set
  is 45, as `THIRD-PARTY-NOTICES.md` records.)
- Public-API snapshots: `--update` on Windows gives 47 insertions / 47 deletions, every removed
  path matched by an added one differing by `Render` -> `Snapshot`, plus the block reordering the
  new initial forces (`Snapshot*` now sorts after `Side`, where `Render*` sorted before
  `ResizePolicy`). The Unix file was hand-mirrored with the same map and re-sorted;
  `--diff-platforms` still reports exactly the six `oneterm_vt::pty` lines.
- `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features`: warning-free; no
  intra-doc link broken. The crate's rustdoc still cites no `US-`/`BUG-`/`DEC-`/`IN-` record and no
  bare `crates/` or `docs/` path.

Gaps:

- **E2E is not proven.** No Windows launch was made from this worktree: the owner runs their agent
  session inside OneTerm, so this agent does not start or stop the app. The packet itself calls this
  a smoke check on a change that compiles and replays the corpus byte-identically.

## Handoff

Step 1 (the `ContentCell` rename) is a clean boundary and is worth landing on its own even if the
rest slips.
