# Work: the engine's read model is called snapshot, not render

ID: US-0101
Intake: IN-0038
Created: 2026-09-15

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

- Change type: existing-contract change (rename only)
- Risk lane: normal
- Spec Intake: `IN-0038`

## Outcome

The module `crates/vt/src/render/` becomes `snapshot/`, and the seven `Render*` types become
`Snapshot*`. Nothing an embedder can observe at runtime changes. The crate stops claiming, in the
name of its largest public module, that it draws something -- which is the first thing a reader of
"an embeddable terminal core that contains no rendering" will check.

## Scope

- [ ] In scope: the module directory and its `mod.rs`; `RenderState`, `RenderUpdate`, `RenderRow`,
  `RenderContent`, `RenderCell`, `RenderCursor`, `RenderPlacement`; `Terminal::render_update`; the
  `EngineView` internal alias; every use site in `crates/terminal`, `crates/terminal-view` and
  `crates/tools`; the collision rename of `oneterm_terminal::SnapshotCell` to `ContentCell`.
- [ ] Out of scope: `ModeSnapshot`, `Palette`, `StyleRun`, `MouseEncoding`, `MouseProtocol`,
  `MouseReporting` -- none of them says "render" and all keep their names.
- [ ] Out of scope: `crates/terminal-view`'s own `render/` module, which genuinely does draw. It
  keeps its name; that is the point of the distinction.
- [ ] Out of scope: any change to the damage model, `SeqNo`, `RowId` or what a pull returns.

## Acceptance

- [ ] `grep -rn '\bRender[A-Z]' crates/vt/ crates/terminal/ crates/tools/` returns **0** lines.
  (`crates/terminal-view` keeps its own render types and is excluded on purpose.)
- [ ] `grep -rn 'render_update' crates/` returns 0 lines.
- [ ] `crates/terminal-view/src/render/` still exists and still contains that crate's own
  `Render*` types.
- [ ] `oneterm_terminal::SnapshotCell` is now `ContentCell`; the four files naming it are updated;
  no type named `SnapshotCell` exists in `crates/terminal`.
- [ ] `cargo test --workspace` green with **no** test assertion text changed. A verifier confirms
  with `git diff` that every changed line in a test file is a type name or an import, never an
  expected value.
- [ ] The 46 frozen parity corpus recordings replay byte-identically.
- [ ] `git diff --shortstat` shows a net line delta within +-5. A rename that grows the tree has
  changed something it should not have.
- [ ] `crates/vt/public-api.txt` (from `US-0097`) regenerates to a diff that contains **only**
  renames: every removed path has a matching added path differing by `Render` -> `Snapshot`.
- [ ] `cargo doc -p oneterm-vt --no-deps` warning-free; no doc link broken by the rename.

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

Before completion, list all five, and confirm `DEC-0015`'s body was annotated rather than rewritten.

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

- [ ] Rename `oneterm_terminal::SnapshotCell` to `ContentCell` first, alone, so the engine rename
  cannot collide. Four files.
- [ ] `git mv crates/vt/src/render crates/vt/src/snapshot` and rename the seven types and
  `render_update`. The compiler finds every site; do not grep-replace across the workspace, because
  `crates/terminal-view`'s own `Render*` types must survive.
- [ ] Update the five documents and annotate `DEC-0015`.
- [ ] Regenerate `crates/vt/public-api.txt`; confirm the diff is renames only.
- [ ] CHANGELOG line under `Unreleased` / `Changed`, naming every renamed item, because this is
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
- Platform: `cargo doc -p oneterm-vt --no-deps`; the `public-api.txt` diff;
  `pwsh scripts/ci-local.ps1`; `python scripts/check-doc-paths.py` (which covers
  `terminal-backend.md`).
- E2E: one manual Windows launch, enough to confirm the app still draws. A rename that compiles and
  passes the corpus cannot plausibly break rendering, so this is a smoke check, not a walk.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Record: the two greps returning 0; `git diff --shortstat`; the `public-api.txt` diff; the corpus
replay result.

No gaps expected. If the line delta exceeds +-5, say what else changed and why, rather than
adjusting the criterion.

## Handoff

Step 1 (the `ContentCell` rename) is a clean boundary and is worth landing on its own even if the
rest slips.
