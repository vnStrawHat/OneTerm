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
  - Added during the rework: `crates/workspace/src/layout/statusbar.rs` (which region each
    indicator sits in, and the measured split between the two that shorten) and
    `crates/workspace/src/widgets/git_status.rs` (the branch elides too).
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
      a shorter form that still carries a unit, never `MEM 577.0`. Re-proved at 700, 900 and
      1200 px after the verification found it cut at 700 px.
- [x] The dock-toggle button stays on the bar at every width, and the git branch is elided with
      an ellipsis rather than hard-cut when it does not fit. Added after verification: the
      first implementation moved the defect onto those two.
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

Docs changed: `docs/gui-layout.md` §Status bar, rewritten with the rework — the centre region
is what shrinks and the pinned ends never do, the two unbounded indicators are the cwd and the
branch, the split between them is measured through the window's text system, the path elides
from the left at a separator and the branch from the right keeping its head, a shortened
indicator carries a tooltip with its full value, and click-to-copy still copies the sampled
path. §Source map still resolves (no file moved or was added).

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

### Rework after independent verification (M1, m3, m4)

`evidence/workspace-wave1-verify.md` **failed** this packet: the first implementation reserved
a fixed 440 px for "everything else", which ignored the git-status and net-speed indicators.
At 700 px that still cut `MEM 174.1 M` and clipped the dock button away, and at 700/900/1200 px
the git branch was hard-cut mid-token with no ellipsis (`worktree-ager`). `F10`'s defect had
been moved onto the next unbounded indicator, not removed. The estimate also erred *wide* —
~6.2 px per character measured against the 6.0 px modelled — the opposite of what this section
claimed.

Fixed at the root, in two parts:

1. **The layout guarantees the pinned ends.** The breadcrumb and git status moved from the
   bar's left region into its **centre** region, which GPUI Kit lays out as `flex-1` with a
   zero basis (`reference/gpui-kit/crates/component/src/status_bar.rs`): it takes what is left
   and can never push the ends. The clock, the speeds, the CPU/memory indicator and the dock
   button therefore keep their width at every window size — `MB` and the button cannot be
   clipped by a long path any more, whatever the arithmetic does.
2. **The budget is measured, not estimated.** `measure_status_text` shapes a string through
   the window's text system at the bar's own font size (`text_xs`, 0.75 rem).
   `build_status_bar` measures every label, subtracts the bar's fixed pieces (icons,
   separators, the button, padding — the only constants left, and none of them depends on the
   text), and divides the rest: the branch gets what it asks for while the path keeps at least
   80 px, and below that the branch gives way down to 40 px. Each indicator then picks the
   longest elision that fits its budget by bisecting over the character budget and measuring
   (`fit_to_width`), so the character rules stay pure and testable while the fitting is exact.
3. **The branch elides too**, keeping the head that identifies it (`elide_head`:
   `worktree-agent-aa…`), through the same helper.

Also from the verification: **m3** — `elide_path_left(_, 0)` returned `…`, one column over
budget, which the new bisection would have accepted as "fitting"; both elisions now return
`""` at a budget of zero, and a test sweeps every budget to prove nothing ever comes back wider
than its budget. The `the_cut_never_lands_inside_a_component` test only asserted a suffix
relation, which the over-long-component branch satisfies while cutting inside a component; it
is now `the_cut_lands_on_a_separator_while_one_fits` and asserts the boundary itself. **m4** —
a shortened indicator now carries a tooltip with its full value; an unshortened copyable one
still reads "Click to copy".

### What the packet assumed, and what was actually there

`status_text.rs` had **no shortening helper**. Nothing in OneTerm truncated anything: the
status bar let the breadcrumb grow to the full cwd, and the window clipped whatever ran past
its edge. That is why the head survived and the tail was lost, why there was no ellipsis, and
why `MB` disappeared — the right-hand indicators were pushed off the window by the left group,
not truncated. `format_memory` already produced `MEM 577.0 MB` correctly.

So the fix bounds the one unbounded indicator, which is still "fix it where the callers route
through" as the packet asked — building the helper rather than correcting it:

- `crates/workspace/src/widgets/status_text.rs` — the shortening rules
  (`elide_path_left`, `elide_head`), the measurement (`measure_status_text`), the width search
  (`fit_to_width`), and a `Shorten { Never, PathTail(Budget), HeadFirst(Budget) }` policy whose
  budget is a shared `Rc<Cell<Pixels>>` the bar refreshes each frame. The presentation
  arguments moved into a `Presentation { icon, copyable, shorten }` struct, because an eighth
  parameter tripped `clippy::too_many_arguments`.
- `crates/workspace/src/layout/statusbar.rs` — the layout change and `divide_centre`, the one
  place that sees every label at once.
- `crates/workspace/src/widgets/breadcrumb.rs` (`PathTail`) and `git_status.rs` (`HeadFirst`).
- `datetime_clock.rs`, `net_speed.rs`, `resource.rs` — `Shorten::Never`, behaviour unchanged.
  Those are the callers the packet asked to be listed; each was re-read in the 700/900/1200 px
  frames below and reads correctly.

Click-to-copy still copies the **sampled** path, not the elided one.

### Commands

- `cargo test -p oneterm-workspace` — 34 passed. This packet's, after the rework:
  `a_path_that_fits_is_shown_whole`, `nothing_ever_comes_back_wider_than_its_budget`
  (every budget from 0 up, both rules), `a_long_name_keeps_the_head_that_identifies_it`,
  `a_long_path_keeps_its_tail_from_a_separator`,
  `the_cut_lands_on_a_separator_while_one_fits`,
  `a_single_component_longer_than_the_budget_keeps_its_end`,
  `multi_byte_components_are_never_cut_mid_character`,
  `the_width_search_picks_the_longest_elision_that_fits` (measured, in a test window),
  `only_an_unbounded_label_shortens_and_never_a_value_with_its_unit`, and the four
  `statusbar::tests::*` over `divide_centre`.
- Superseded (first implementation) — 24 passed, six of them new:
  `a_path_that_fits_is_shown_whole` (fits, exactly fitting, empty),
  `a_long_path_keeps_its_tail_from_a_separator` (three budgets, Windows and POSIX
  separators), `the_cut_never_lands_inside_a_component` (every budget from 1 to the full
  length yields a tail of the original),
  `a_single_component_longer_than_the_budget_keeps_its_end`,
  `multi_byte_components_are_never_cut_mid_character` (CJK), and
  `only_a_path_label_is_shortened_and_never_a_value_with_its_unit`.
- `pwsh scripts/ci-local.ps1` — ended with "ci-local: all checks passed".

### GUI walk, after the rework

Own build, own pid, `PrintWindow`, shell cwd deep inside this worktree so the git indicator is
live and the branch is long (`worktree-agent-aac9bb2c34fe0673d`, 36 characters).

- `evidence/US-0112-rework-700-git-cwd.png` — 700 px, the width the verification failed on:
  path `…\workspace`, branch `worktree-agent-aa… (+703 -149)`, **`MEM 197.7 MB`** with its unit
  and the dock button both on the bar. Compare `evidence/verify-US-0112-700-git-cwd.png`.
- `evidence/US-0112-rework-900-git-cwd.png` — 900 px: path `…\workspace\src\layout\workspace`,
  the branch whole, `MEM 198.0 MB`, dock button present.
- `evidence/US-0112-rework-1200-git-cwd.png` — 1200 px: a longer tail of the path, the branch
  whole, `MEM 186.4 MB`, dock button present.
- In all three: every shortened value carries an ellipsis, and nothing is cut without one.

### GUI walk (first implementation, superseded)

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

- The bar's **non-text chrome** is still constants (`BAR_CHROME`, `ICON_CHROME`): icon size,
  separator width, the dock button and the bar's padding are read off the kit's styles rather
  than measured. They do not depend on the text, and an error in them now only makes the centre
  region's two labels slightly shorter or longer than they had to be — it can no longer reach
  the pinned ends, which is the half that mattered.
- **Shaping runs per frame.** `fit_to_width` bisects, so a path takes about seven `layout_line`
  calls per frame, plus one per other label. `layout_line` is cached by gpui, and this was not
  profiled; if the status bar ever shows up in a profile, caching the result per (text, budget)
  is the next step.
- The tooltip shows the full sampled value, but there is no test for it — it is a render-time
  branch, like the elision call itself.
- Only Windows was walked; the elision rules treat `/` and `\` alike, and the measurement is
  platform-independent, but no macOS or Linux frame was taken.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
