# Work: The status bar elides the path from the left and keeps units

ID: US-0112
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

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

- [x] In scope:
  - `crates/workspace/src/widgets/breadcrumb.rs` — the cwd display. Truncate from the left,
    at a path separator, with a leading ellipsis.
  - `crates/workspace/src/widgets/status_text.rs` — the shared shortening helper the memory
    indicator goes through, so the unit is never what gets cut.
  - Every indicator that shares the same helper: fixing it in the helper rather than in the
    breadcrumb alone is the point (see Context).
- [x] Out of scope:
  - Which indicators the status bar shows and in what order.
  - The empty-Space case where the bar collapses to the clock — that is `F27`, documented as
    intended in `docs/gui-layout.md`, and not packeted.
  - Git status, network speed and CPU formatting, except where they route through the same
    helper and inherit the fix.
  - Making the status bar responsive in any other way (wrapping, hiding, priority ordering).

## Acceptance

- [x] At a window width where the cwd does not fit, the displayed path ends with the current
      directory name and begins with an ellipsis at a separator boundary — for example
      `…\scratchpad\ux\home`, never `…\fa2d0135-9c28-4`.
- [x] The truncation never cuts in the middle of a path component.
- [x] A path that fits is shown whole, with no ellipsis.
- [x] A single path component longer than the available width still renders something
      sensible (its tail, with the ellipsis) rather than an empty string or a panic.
- [x] The memory indicator shows its unit at every width it is visible at: `MEM 577.0 MB`, or
      a shorter form that still carries a unit, never `MEM 577.0`.
- [x] A focused test covers the elision at several widths, including the two degenerate cases
      (width smaller than the last component; empty path).
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Docs changed: `docs/gui-layout.md` §Status bar gained a paragraph — the bar neither wraps nor
scrolls, the breadcrumb is the only indicator that shortens, it elides from the left at a
separator, every other indicator is `Shorten::Never` so a value never loses its unit, the
budget is a character estimate rather than a measurement, and click-to-copy still copies the
full path. §Source map still resolves (no file moved or was added).

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

- [x] Read `status_text.rs` and list its callers before changing anything; the caller list
      decides whether the fix is one function or two.
- [x] Add the focused tests (they are pure string-and-width functions, so they come first).
- [x] Implement the left elision and the keep-the-unit rule.
- [x] Update `docs/gui-layout.md`.
- [x] Re-capture the scene at the same window width the walkthrough used.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
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

### What the packet assumed, and what was actually there

`status_text.rs` had **no shortening helper**. Nothing in OneTerm truncated anything: the
status bar let the breadcrumb grow to the full cwd, and the window clipped whatever ran past
its edge. That is why the head survived and the tail was lost, why there was no ellipsis, and
why `MB` disappeared — the right-hand indicators were pushed off the window by the left group,
not truncated. `format_memory` already produced `MEM 577.0 MB` correctly.

So the fix bounds the one unbounded indicator, which is still "fix it where the callers route
through" as the packet asked — building the helper rather than correcting it:

- `crates/workspace/src/widgets/status_text.rs` — `elide_path_left(path, max_chars)` (pure:
  drops leading components, cuts at a separator whenever one fits, falls back to the tail of
  an over-long single component), a `Shorten { Never, PathTail }` policy applied at render
  where the window width is known, and `path_budget(window)` = (viewport width - 440 px
  reserved for the bar's other contents) / (root font size x 0.375). The presentation
  arguments moved into a `Presentation { icon, copyable, shorten }` struct, because an eighth
  parameter tripped `clippy::too_many_arguments`.
- `crates/workspace/src/widgets/breadcrumb.rs` — the only `Shorten::PathTail` caller.
- `datetime_clock.rs`, `git_status.rs`, `net_speed.rs`, `resource.rs` — `Shorten::Never`,
  behaviour unchanged. Those are the callers the packet asked to be listed; each was re-read
  in the 900 px and 1900 px frames below and reads correctly.

Click-to-copy still copies the **sampled** path, not the elided one.

### Commands

- `cargo test -p oneterm-workspace` — 24 passed, six of them new:
  `a_path_that_fits_is_shown_whole` (fits, exactly fitting, empty),
  `a_long_path_keeps_its_tail_from_a_separator` (three budgets, Windows and POSIX
  separators), `the_cut_never_lands_inside_a_component` (every budget from 1 to the full
  length yields a tail of the original),
  `a_single_component_longer_than_the_budget_keeps_its_end`,
  `multi_byte_components_are_never_cut_mid_character` (CJK), and
  `only_a_path_label_is_shortened_and_never_a_value_with_its_unit`.
- `pwsh scripts/ci-local.ps1` — ended with "ci-local: all checks passed".

### GUI walk

- `evidence/US-0112-50-narrow-900.png` — 900 px window. The bar reads
  `...\fa2d0135-9c28-4a69-a255-ba7479e604cb\scratchpad\ux2\home` behind a leading ellipsis,
  cut at a separator, the current directory still visible — and `MEM 202.8 MB` keeps its unit.
  Compare `research/before/50-narrow-900.png`, which read `...\fa2d0135-9c28-4` and
  `MEM 577.0`.
- `evidence/US-0112-51-large-1900.png` — 1900 px window: the same path whole, no ellipsis.
- The walk ran under its own scratch `HOME` (`...\scratchpad\ux2\home`), so the path differs
  from the walkthrough's `...\ux\home` by one component; the shape of the elision is what the
  frames compare.

### Gaps

- The budget is in **characters**, derived from the window width and the root font size, not a
  text measurement: the status bar font is proportional, so the elision point drifts with the
  font and with unusually wide glyphs. The constants (440 px reserved, 0.375 rem mean advance)
  were calibrated against the before frames — about 6 px per character at the default 16 px
  root font — and err narrow. Measuring would mean laying the text out during render for one
  indicator.
- `Shorten::apply` and `path_budget` are separately testable, but the render call that joins
  them is not covered by a test; the two frames are its proof.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
