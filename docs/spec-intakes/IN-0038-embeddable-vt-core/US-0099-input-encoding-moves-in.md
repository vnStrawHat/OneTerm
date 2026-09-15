# Work: key and mouse encoding are the engine's

ID: US-0099
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

- Change type: existing-contract change (a crate boundary moves; no behaviour changes)
- Risk lane: normal
- Spec Intake: `IN-0038`

## Outcome

`crates/terminal/src/key_encode.rs` and `mouse_encode.rs` become `crates/vt/src/input/key.rs` and
`mouse.rs`, published as `oneterm_vt::input`. An embedder can turn a key press or a mouse click into
the bytes the terminal expects without writing a second copy of the X10 / 1005 / SGR-1006 rules.
`encode_key` takes a `&ModeSnapshot` instead of a bare `app_cursor: bool`, and `Terminal` gains a
convenience method that reads its own modes.

Not one byte that reaches a PTY changes.

## Scope

- [ ] In scope: both files and their `#[cfg(test)]` modules; the new `crates/vt/src/input/` module;
  the `encode_key` signature change; `Terminal::encode_key`; the re-export block in
  `crates/terminal/src/lib.rs`.
- [ ] Out of scope: `crates/terminal-view/src/input/keys.rs` and `mouse.rs`, which map GPUI events
  to `KeySpec` and own every scroll, selection and shift-tracking side effect. They keep calling the
  encoder and are not rewritten in this packet.
- [ ] Out of scope: wiring `KeyboardFlags` (the Kitty keyboard protocol) into `encode_key`. It lives
  in `crates/vt` already and is not consumed by the encoder today; connecting them is a behaviour
  change and belongs to its own packet.
- [ ] Out of scope: `paste.rs` (bracketed paste plus OneTerm sanitising).

## Acceptance

- [ ] `crates/terminal/src/key_encode.rs` and `mouse_encode.rs` no longer exist.
- [ ] Every test in their current `mod tests` exists in `crates/vt/src/input/`, **unchanged in input
  and expectation**. A verifier runs
  `git show main:crates/terminal/src/key_encode.rs > /tmp/before.rs` and diffs the test module
  against the new one; the only permitted differences are `use` paths and the `app_cursor` argument
  form.
- [ ] An equivalence test asserts that for every `NamedKey` and every `KeyMods` combination,
  `encode_key(key, mods, &modes)` with `modes.app_cursor == true` produces exactly what the old
  three-argument form produced with `app_cursor = true`, and likewise for `false`. This test is
  written **before** the boolean form is deleted and deleted with it.
- [ ] `cargo tree -p oneterm-vt -e normal` is still 7 lines.
- [ ] `cargo tree -p oneterm-terminal -e normal` shows no new dependency.
- [ ] `cargo test --workspace` green with `crates/terminal-view` unmodified except for its `use`
  paths.
- [ ] `crates/terminal` production lines drop by about 620; `crates/vt` rises by the same. Net
  workspace delta within +-20 lines.
- [ ] Manual Windows walk: in `vim` over SSH, arrows, Home/End, PageUp/PageDown, F1 to F12, Ctrl and
  Alt combinations, and application-cursor mode entered and left. In `htop`, mouse click, drag,
  release and wheel, with `? 1006` on and off.

## Documentation

### Owning Docs Reviewed

- `docs/agents/crate-dependency-rules.md` -- R7 (the engine is gpui-free) and R10 (a shared type
  goes in the lowest crate that needs it). R10 is the rule that makes this move correct rather than
  merely tidy: `KeySpec` is used by `terminal` and `terminal-view`, and the lowest crate that can
  hold it is `vt`.
- `docs/agents/structure.md` -- the crate responsibility table; both the `vt` and `terminal` rows
  change.
- `docs/terminal-backend.md` -- reviewed; its input section describes the view layer, which does not
  move. No change expected, to be confirmed.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/` -- the owning design for
  `crates/terminal-view`, including its input layer. Reviewed to confirm the view keeps its side
  effects.

### Documentation Action

**Update required**: `docs/agents/structure.md` (two rows and the directory tree). Everything else is
a no-change: the view layer's contract is untouched, and `terminal-backend.md` describes the view's
input path, not the encoder's.

Reason: the tree in `structure.md` lists files per crate and would name two files that no longer
exist.

### Reconciliation

Before completion, list `structure.md`'s change and confirm the `terminal-backend.md` no-change
reason still holds.

## Context

Measured on `main` @ `36977ca`:

| File | Lines | Imports |
| --- | ---: | --- |
| `crates/terminal/src/key_encode.rs` | 572 (about 237 production, 335 test) | **none at all** |
| `crates/terminal/src/mouse_encode.rs` | 475 (about 188 production, 287 test) | `oneterm_vt::{ModeSnapshot, MouseEncoding}` |

`key_encode.rs` has zero imports, which is the clearest possible signal that it is not adapter code.
`mouse_encode.rs` already depends on the engine. Consumers outside `crates/terminal`:
`crates/terminal-view/src/input/{keys.rs, mouse.rs}` and their test files, plus
`crates/local-shell/src/session_tests.rs`.

Design: [`low-level-design/encoding-and-search.md`](low-level-design/encoding-and-search.md).

## Plan

- [ ] Create `crates/vt/src/input/{mod.rs, key.rs, key_tests.rs, mouse.rs, mouse_tests.rs}` by
  moving the files verbatim, changing only `use` paths. Commit this alone; `cargo test --workspace`
  must be green with `crates/terminal` re-exporting.
- [ ] Add the `&ModeSnapshot` form of `encode_key` alongside the boolean form, plus the equivalence
  test.
- [ ] Switch `crates/terminal-view` to the new form; delete the boolean form and its equivalence
  test in the same commit.
- [ ] Add `Terminal::encode_key`.
- [ ] Re-export block in `crates/terminal/src/lib.rs`; `missing_docs` lines for the newly public
  items.
- [ ] `structure.md` and a CHANGELOG line.

## Decisions

None. The move is owner decision (b) and the signature narrowing is a consequence of it, not a
choice future work must inherit.

## Verification Plan

- Focused: the moved test suites; the equivalence test.
- Unit: `cargo test -p oneterm-vt`, `cargo test --workspace`.
- Integration: `cargo test -p oneterm-local-shell -p oneterm-ssh`.
- Platform: `pwsh scripts/ci-local.ps1`; `cargo tree -p oneterm-vt -e normal`.
- E2E: the manual Windows walk in Acceptance. This is the packet where a silent regression would be
  invisible to every automated check and immediately obvious to a user, so the walk is not optional.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Record: the test-module diff against `main`; the equivalence test before it was deleted;
`git diff --stat`; `cargo tree` output; the manual walk with which keys and mouse modes were
exercised.

Known gap: `KeyboardFlags` still does not affect `encode_key`, so the Kitty keyboard protocol is
recognised by the engine and ignored by the encoder. Pre-existing, unchanged, and now more visible
because both live in the same crate.

## Handoff

The verbatim move (Plan step 1) is a clean session boundary.
