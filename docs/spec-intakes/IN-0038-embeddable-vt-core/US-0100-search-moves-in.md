# Work: scrollback search is the engine's, with regex behind an optional feature

ID: US-0100
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

- Change type: existing-contract change plus a new capability (the `regex` feature)
- Risk lane: normal
- Spec Intake: `IN-0038`

## Outcome

`crates/terminal/src/search.rs` becomes `oneterm_vt::search`. An embedder can search a terminal's
scrollback with the two-phase shape the module was designed around -- copy under your lock, match
without it -- and can opt into regular expressions with `features = ["regex"]`. The default
dependency set does not move: `cargo tree -p oneterm-vt -e normal` stays seven lines.

OneTerm's own search stays literal and does **not** enable the feature.

## Scope

- [ ] In scope: `crates/terminal/src/search.rs` and its tests; the new `crates/vt/src/search/`
  module; `GridText` becoming `pub`; `SearchPattern`; the `regex` cargo feature; the re-export block
  in `crates/terminal/src/lib.rs`.
- [ ] Out of scope: `crates/terminal-view/src/terminal_view/search.rs` -- the overlay, its highlight
  state and its navigation stay in the view.
- [ ] Out of scope: `TerminalSession::search` and its `SessionKind` plumbing -- the trait is
  OneTerm's contract and does not move.
- [ ] Out of scope: a regex search **UI**. This packet makes the engine capable; exposing it to a
  user is a separate product decision with its own packet.
- [ ] Out of scope: matching across a wrapped-line boundary. The literal scanner cannot do it today
  and the regex path will not either; the limitation is documented, not fixed.
- [ ] Out of scope: changing the copy strategy. `R-19` deliberately copies the grid; this packet
  documents the cost and leaves it.

## Acceptance

- [ ] `crates/terminal/src/search.rs` no longer exists; `oneterm_vt::search` does.
- [ ] Every test in the current `mod tests` exists in `crates/vt/src/search/`, unchanged in input and
  expectation. Verifier method: diff against `git show main:crates/terminal/src/search.rs`.
- [ ] `cargo tree -p oneterm-vt -e normal` is exactly **7 lines** with default features.
- [ ] `cargo tree -p oneterm-vt -e normal --features regex` adds exactly `regex`, `regex-automata`,
  `regex-syntax` and `aho-corasick`, and nothing else.
- [ ] The whole literal test suite passes identically with and without `--features regex`. A feature
  changes availability, never behaviour.
- [ ] `cargo build -p oneterm-vt --no-default-features` is clean, and `SearchPattern` still compiles
  in an embedder's `match` with a `_` arm (proved by a doctest).
- [ ] `grep -n 'oneterm-vt' crates/terminal/Cargo.toml` shows **no** `features = [...]`: OneTerm does
  not enable regex.
- [ ] `python scripts/third-party-notices.py --check` passes with no regeneration.
- [ ] Regex cases, each a test: a literal-equivalent pattern returns the same matches as
  `SearchPattern::Literal`; an anchored `^` matches only at column 0; a pattern that can match the
  empty string terminates and advances at least one column per match; `case_sensitive` or
  `whole_word` with a regex trips a debug assertion and is documented as ignored.
- [ ] `cargo test --workspace` green with `crates/terminal-view` unmodified except `use` paths.
- [ ] Manual Windows walk: open the search overlay in a session with several thousand scrollback
  rows, search a string that appears in history and on screen, navigate forwards and backwards,
  confirm the highlights land on the right cells after new output scrolls the grid.

## Documentation

### Owning Docs Reviewed

- `docs/agents/crate-dependency-rules.md` -- R7 and R10, as in `US-0099`.
- `docs/agents/dependencies.md` -- the "Terminal helpers" row already declares `regex 1` in the
  workspace; the `oneterm-vt` row gains the optional dependency.
- `docs/agents/structure.md` -- the crate responsibility table and the tree.
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` -- `SearchMatch` is
  keyed by `RowId` because of this decision; the doc comment citing it must become a
  self-contained sentence under `US-0097`'s rule.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/` -- the owning design for the search
  overlay, which stays in the view.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` -- `R-19`, the rule
  that keeps the copy.

### Documentation Action

**Update required**: `docs/agents/structure.md` (two rows and the tree) and
`docs/agents/dependencies.md` (the `oneterm-vt` row gains `regex`, optional, default-off, and a
sentence saying OneTerm does not enable it).

Reason: `dependencies.md` is the authoritative list of what each crate depends on, and an optional
dependency that nothing in the workspace enables is exactly the kind of fact that rots silently if
it is not written down.

### Reconciliation

Before completion, list both edits.

## Context

`crates/terminal/src/search.rs` is 418 lines: `SearchOptions`, `SearchMatch` (with `grid_line` and
`display_row`), `GridText` (`pub(crate)`) with `from_terminal`, `search_grid_text`, and a
`#[cfg(test)]` `search_term` wrapper. It imports `oneterm_vt::{CellWidth, RowId, Terminal}` and
nothing else. Consumers outside the crate: `crates/terminal-view/src/terminal_view/search.rs`.

`regex 1` is already declared in root `[workspace.dependencies]` and `regex 1.12.4` is already in
`THIRD-PARTY-NOTICES.md` via `oneterm-highlight`, so the licence surface does not move in either
direction.

Design: [`low-level-design/encoding-and-search.md`](low-level-design/encoding-and-search.md).

## Plan

- [ ] Move the file verbatim to `crates/vt/src/search/{mod.rs, literal.rs, search_tests.rs}`,
  changing only `use` paths and raising `GridText` to `pub` with documentation. Commit alone;
  workspace green with a re-export.
- [ ] Introduce `SearchPattern`, with `Literal` only, and change `search_grid_text`'s signature.
  The existing tests move to the new signature mechanically.
- [ ] Add the `regex` feature, `crates/vt/src/search/regex.rs`, `SearchPattern::Regex`, and its
  tests. Wire `#[cfg_attr(docsrs, doc(cfg(feature = "regex")))]`.
- [ ] Document `GridText::from_terminal`'s cost in `char`s (rows times columns) rather than leaving
  it implicit, and document the wrapped-line limitation on `SearchPattern`.
- [ ] Re-export block; `structure.md` and `dependencies.md`; CHANGELOG line.
- [ ] Add `--features regex` to the `ci-local` scripts' `cargo test` line for `oneterm-vt`.

## Decisions

None of its own. Owner decision (a) settles the move and the feature gate. Whether OneTerm ever
exposes regex search to a user is a future product decision and is explicitly not taken here.

## Verification Plan

- Focused: the moved literal suite under both feature states; the four regex cases.
- Unit: `cargo test -p oneterm-vt`, `cargo test -p oneterm-vt --features regex`,
  `cargo test --workspace`.
- Integration: `cargo test -p oneterm-terminal`.
- Platform: both `cargo tree` invocations; `cargo build --no-default-features`;
  `python scripts/third-party-notices.py --check`; `pwsh scripts/ci-local.ps1`.
- E2E: the manual search walk in Acceptance.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Record: the test-module diff; both `cargo tree` outputs; the `third-party-notices --check` result;
`git diff --stat`; the manual walk.

Known gaps, all pre-existing and deliberately unchanged: the full-grid copy (`R-19`), no matching
across a wrapped line, ASCII-only case folding in the literal scanner, and no regex search UI.

## Handoff

The verbatim move (Plan step 1) is a clean boundary. The regex half (steps 3 and 4) is independently
reviewable and could be deferred to its own packet if the feature turns out to be contentious --
say so rather than half-landing it.
