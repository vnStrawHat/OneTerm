# Work: The active Space is unmistakable

ID: US-0117
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

With a Space split in two, a glance answers "where does my typing go". The active Space's cue
is visible without measuring pixels, and each Space says which shell it holds.

## Findings and proposals covered

`P20` (effort M) — *"Strengthen the active-Space cue: widen the accent gutter to 2 px and give
each Space a small corner label (shell name or `#N`), reusing the channel-badge slot."*

Addresses `F26` (medium), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F26 | split Spaces | The active Space is marked by a **single 1-pixel accent line** on one
> edge (measured: `rgb(82,139,255)` at x=561 in shot 09). At a glance the two Spaces are
> indistinguishable, and there is no per-Space label saying which shell it holds. | Feedback:
> "where does my typing go" is answered by one pixel. (Design choice recorded in
> `terminal-split.md` §8, so a stronger cue, not a redesign.) | medium | 09 |

`F27` is listed beside `F26` in `P20`'s "fixes" column, but `F27` — the status bar collapsing
to the clock in an empty Space — is documented as intended in `docs/gui-layout.md` and the
walkthrough records it as low severity with no behaviour change proposed. This packet does not
change it; `US-0126` reports it as an observation.

## Scope

- [ ] In scope:
  - `crates/terminal-view/src/space/render.rs` — the active-Space cue and the per-Space label.
  - `crates/terminal-view/src/theme/` — any token the stronger cue needs, read from the theme
    rather than hardcoded.
  - Reusing the existing channel-badge slot for the label, rather than adding a new overlay.
- [ ] Out of scope:
  - **Reversing** `docs/terminal-split.md` §8 decisions 3 and 8 — the 1 px outer border, the
    1 px inner gutter, and the borderless single Space. `P20` strengthens the cue *inside*
    that rule; the walkthrough is explicit that this is "a stronger cue, not a redesign".
  - The single-Space case, which stays borderless.
  - Split mechanics, drag-tab-into-Space, and split persistence (not persisted by decision 4).
  - The empty-Space placeholder (`US-0115`).
  - Input channel membership and the channel badge's own meaning, which the label slot must
    not displace when a channel badge is present.

## Acceptance

- [ ] With two Spaces side by side, a reviewer looking at a screenshot at 100 % identifies the
      active one without measuring. This is the acceptance; the exact width is the means.
- [ ] Each Space carries a small label naming what it holds, legible at the default font size.
- [ ] A single Space stays borderless and unlabelled — nothing appears where nothing appeared
      before.
- [ ] The label does not collide with, hide, or replace the channel badge when a Space belongs
      to a broadcast channel.
- [ ] The cue and the label take their colours from the theme; no colour literal is added to
      `crates/terminal-view`.
- [ ] The cue is visible in both a light and a dark theme, and after `US-0111` raises the
      secondary-text tokens.
- [ ] The terminal grid does not lose a row or a column to the label — the label overlays or
      sits in existing chrome, it does not push content.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-split.md` §8 — the border and gutter decisions, including decision 3
  (borderless single Space) and decision 8 (1 px outer border + 1 px inner gutter).
  **Update required: amend, do not reverse.** Record the strengthened cue and the per-Space
  label as an amendment that keeps those decisions intact, and say why the amendment does not
  contradict them.
- `docs/terminal-split.md` §9 — the empty Space. Read to keep the label rule consistent with
  what an empty Space shows. **No change** (that is `US-0115`).
- `docs/gui-layout.md` §Broadcast input channels — the channel badge and the Space badge slot
  this packet reuses (`crates/terminal-view/src/space/render.rs` is listed in its source map).
  **Update required** if the slot's contents change; confirm during implementation.
- `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md` — read because the badge
  slot belongs to that decision's surface. **No change expected.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/terminal-split.md` §8 (amendment), and `docs/gui-layout.md`
§Broadcast input channels if the badge slot's contents change.

Reason: the active-Space cue is an explicitly recorded design decision; strengthening it
without recording the amendment would leave the document describing a cue that no longer
exists, and the next reader would "restore" it.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- The measurement is in the finding: `rgb(82,139,255)` at a single x in
  `research/before/09-split-two-terminals.png`. That is the accent colour, one pixel wide.
  Going to 2 px is what `P20` proposes and is the smallest change that could work — but "a
  reviewer identifies the active Space from the screenshot" is the acceptance, and 2 px may
  not clear it. Measure the after frame; if 2 px is still not enough, the options inside the
  §8 decisions are a brighter token or a label that itself carries the active state, not a
  thicker border.
- The label content: "shell name or `#N`". `US-0114` gives local shells real names, so the
  shell name becomes genuinely useful once it lands — but this packet does not depend on it,
  because an unnamed Space can fall back to `#N`. Decide which, and prefer the one that stays
  correct when a Space holds an SSH session.
- Reusing the channel-badge slot is `P20`'s own suggestion and is the lazy path: the slot
  exists, it is positioned, and it is already themed. The risk is collision when a channel
  badge is actually present, which the acceptance covers.
- Ladder: no new element type, no new overlay layer, no new theme token if an existing accent
  token serves.
- `research/before/09-split-two-terminals.png` is the before picture;
  `07-split-right-empty-space.png` shows the single-plus-empty case that must not gain chrome.

## Plan

- [ ] Re-measure the current cue on a fresh capture so the before number is this packet's own.
- [ ] Widen the cue; capture; measure again against the acceptance.
- [ ] Add the label in the badge slot; check the channel-badge collision case.
- [ ] Amend `docs/terminal-split.md` §8.
- [ ] Re-capture the scenes in both a light and a dark theme.

## Decisions

None. The §8 amendment is recorded in the design document it amends, which is where a reader
looking for the border rule will be. Creating a `DEC` for a 1 px change inside an existing
decision would scatter the rule across two files.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-terminal-view` — whatever pure logic the label needs
   (which Space gets which `#N`, what an SSH Space's label reads). The cue's width and colour
   are element properties and are not queryable in the panel tests; their proof is the GUI
   walk plus a pixel measurement, and Evidence must say so.
2. **Unit:** `cargo test -p oneterm-terminal-view`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `09-split-two-terminals.png` — two Spaces, one active. Record the measured cue width and
     colour in Evidence, as the walkthrough did, so before and after are comparable as numbers
     and not only as pictures. Capture the same frame after switching focus to the other
     Space, so the cue is shown moving.
   - `07-split-right-empty-space.png` — a Space plus an empty Space.
   - `05-typing.png` — a single Space: still borderless, still unlabelled.
   - The same split frame in a light theme (the walkthrough's `34-theme-light-main.png` route),
     because a cue tuned on dark can vanish on light.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Reversing a recorded decision by accident.** §8 fixed the 1 px border deliberately. A
  change that reads as "we made the borders thicker" is out of scope; the change is to the
  *active* cue. State the difference in the amendment.
- **Chrome creep.** A label on every Space in every split is permanent visual weight in the
  application's primary surface. Keep it small, keep it in the existing slot, and check the
  two-by-two split case, not just the side-by-side one.
- **Badge collision.** A Space in a broadcast channel already shows a badge. If the label
  takes that slot, one of the two is lost. Walk that case explicitly.
- **Light themes.** An accent that reads clearly on `#23272e` can disappear on a light
  background. Both themes are in the verification plan for this reason.
- **Measuring the wrong frame.** `PrintWindow` captures at the window's own scale; a
  high-DPI capture can make a 1 px cue look like 2. Record the capture scale alongside the
  measurement.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
