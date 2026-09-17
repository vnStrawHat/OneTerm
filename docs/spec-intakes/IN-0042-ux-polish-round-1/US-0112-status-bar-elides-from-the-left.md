# Work: The status bar elides the path from the left and keeps units

ID: US-0112
Intake: IN-0042
Created: 2026-09-17

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

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

When the status bar runs out of room, it drops the part of the path nobody needs. The
breadcrumb keeps its tail — the directory the user is actually in — and marks the truncation;
the memory indicator never loses its unit.

## Findings and proposals covered

`P9` — *"Elide the status-bar path from the **left** (`…\scratchpad\ux\home`) and keep the
unit on MEM."*

Addresses `F10` (medium), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F10 | status bar (narrow) | The cwd is hard-cut mid-token with no ellipsis —
> `…\fa2d0135-9c28-4` — keeping the *head* of the path and dropping the directory the user is
> actually in. `MEM 577.0` loses its `MB`. | Information: truncation drops the only part that
> matters. `crates/workspace/src/widgets/breadcrumb.rs` / `status_text.rs`. | medium | 50 |

## Scope

- [ ] In scope:
  - `crates/workspace/src/widgets/breadcrumb.rs` — the cwd display. Truncate from the left,
    at a path separator, with a leading ellipsis.
  - `crates/workspace/src/widgets/status_text.rs` — the shared shortening helper the memory
    indicator goes through, so the unit is never what gets cut.
  - Every indicator that shares the same helper: fixing it in the helper rather than in the
    breadcrumb alone is the point (see Context).
- [ ] Out of scope:
  - Which indicators the status bar shows and in what order.
  - The empty-Space case where the bar collapses to the clock — that is `F27`, documented as
    intended in `docs/gui-layout.md`, and not packeted.
  - Git status, network speed and CPU formatting, except where they route through the same
    helper and inherit the fix.
  - Making the status bar responsive in any other way (wrapping, hiding, priority ordering).

## Acceptance

- [ ] At a window width where the cwd does not fit, the displayed path ends with the current
      directory name and begins with an ellipsis at a separator boundary — for example
      `…\scratchpad\ux\home`, never `…\fa2d0135-9c28-4`.
- [ ] The truncation never cuts in the middle of a path component.
- [ ] A path that fits is shown whole, with no ellipsis.
- [ ] A single path component longer than the available width still renders something
      sensible (its tail, with the ellipsis) rather than an empty string or a panic.
- [ ] The memory indicator shows its unit at every width it is visible at: `MEM 577.0 MB`, or
      a shorter form that still carries a unit, never `MEM 577.0`.
- [ ] A focused test covers the elision at several widths, including the two degenerate cases
      (width smaller than the last component; empty path).
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Status bar — lists the bar's contents and says each text indicator
  carries a leading icon that hides with its label, and that terminal-derived widgets resolve
  the active panel through the dock tree. It says nothing about truncation. **Update
  required:** one sentence on how the bar shortens what it cannot fit.
- `docs/gui-layout.md` §Source map — already points at the widgets directory; confirm the
  entry still resolves after the change. **No change expected.**
- `crates/workspace/src/widgets/status_text.rs` — the shared helper; read before deciding
  where the fix goes, because that decision is the whole packet.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/gui-layout.md` §Status bar gains a sentence stating that the bar elides
the path from the left at a separator and never truncates a value away from its unit.

Reason: truncation behaviour is what the user sees at any window narrower than comfortable,
and the document currently describes the bar as if it always fits.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- The bug is a truncation *direction*, which makes it a one-function fix — but only if the
  function is the shared one. `status_text.rs` is named in the finding alongside
  `breadcrumb.rs` precisely because the shortening is shared: patching the breadcrumb alone
  would leave every other indicator cutting the same way, and the memory unit would stay
  broken. Fix it where the callers route through, then let the breadcrumb pass "elide left, at
  a separator" as its own rule.
- The `MEM 577.0` case is the same bug from the other side: a value and its unit are one
  token, and the shortener treats the whole string as cuttable text. The fix is for the
  indicator to hand over the parts it must keep, not for the shortener to learn about
  megabytes.
- Ladder check: no new dependency and no layout engine. This is string work plus the width
  the caller already has, and it belongs in the helper that already exists rather than in a
  new one.
- `research/before/50-narrow-900.png` is the frame `F10` measured — the ~900 px window with
  `…\fa2d0135-9c28-4` and `MEM 577.0`. It is the before picture and the scene to re-capture.
- Note for the walk: the path shown in that frame is the walkthrough's scratch `HOME`, so the
  after frame will show a different path unless the same scratch home is used. Use one, or say
  in Evidence which path was in view — the point is the *shape* of the elision, and a reviewer
  needs to be able to compare it.

## Plan

- [ ] Read `status_text.rs` and list its callers before changing anything; the caller list
      decides whether the fix is one function or two.
- [ ] Add the focused tests (they are pure string-and-width functions, so they come first).
- [ ] Implement the left elision and the keep-the-unit rule.
- [ ] Update `docs/gui-layout.md`.
- [ ] Re-capture the scene at the same window width the walkthrough used.

## Decisions

None. Which end of a path to keep is not a choice future work inherits; it is the obvious
answer once someone looks at the frame.

## Verification Plan

1. **Focused:** unit tests in `crates/workspace/src/widgets/` over the elision function:
   a path that fits, a path elided at a separator, a path whose last component alone exceeds
   the width, an empty path, and a Windows path with a drive letter. Plus one test that a
   value-with-unit indicator keeps its unit when shortened. These are pure functions, so this
   is where the proof actually lives.
2. **Unit:** `cargo test -p oneterm-workspace`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `50-narrow-900.png` — the ~900 px window. The after frame must show the tail of the path
     with a leading ellipsis, and `MEM` with its unit.
   - `51-large-1900.png` — regression: at 1900 px the path is shown whole with no ellipsis.
   - `01-first-launch.png` — regression: the default window, nothing changed for a path that
     fits.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Fixing only the breadcrumb.** The tempting one-line change is in `breadcrumb.rs`. It
  leaves every other indicator cutting the wrong way and does not fix `MEM` at all. The
  packet's acceptance covers both symptoms specifically so this shortcut fails review.
- **Width in the wrong unit.** A character count is not a pixel width in a proportional font,
  and the status bar text is not monospaced. If the existing helper works in characters, keep
  it working in characters and accept the approximation rather than introducing text
  measurement — but say so in Evidence, because it means the elision point drifts with the
  font size.
- **Multi-byte and wide characters.** A path with CJK components or an emoji must not be cut
  mid-character. The focused tests include one such path.
- **Changing more indicators than intended.** Fixing the shared helper is correct, but it
  changes every caller's behaviour at once. List the callers in Evidence and confirm each
  still reads correctly in the 1900 px and 900 px frames.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
