# Work: The active Space is unmistakable

ID: US-0117
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
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

`P20`'s "shell name or `#N`" was resolved to **`#N`, on inactive Spaces only, with the shell
name on the chip's tooltip** by the `F-117.1` ruling — see the rework round in Evidence.

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

- [x] In scope:
  - `crates/terminal-view/src/space/render.rs` — the active-Space cue and the per-Space label.
  - `crates/terminal-view/src/theme/` — any token the stronger cue needs, read from the theme
    rather than hardcoded.
  - Reusing the existing channel-badge slot for the label, rather than adding a new overlay.
- [x] Out of scope:
  - **Reversing** `docs/terminal-split.md` §8 decisions 3 and 8 — the 1 px outer border, the
    1 px inner gutter, and the borderless single Space. `P20` strengthens the cue *inside*
    that rule; the walkthrough is explicit that this is "a stronger cue, not a redesign".
  - The single-Space case, which stays borderless.
  - Split mechanics, drag-tab-into-Space, and split persistence (not persisted by decision 4).
  - The empty-Space placeholder (`US-0115`).
  - Input channel membership and the channel badge's own meaning, which the label slot must
    not displace when a channel badge is present.

## Acceptance

- [x] With two Spaces side by side, a reviewer looking at a screenshot at 100 % identifies the
      active one without measuring. This is the acceptance; the exact width is the means.
- [ ] ~~Each **inactive** Space in a split carries a small chip saying *which* Space it is
      (`#N`), legible at the default font size, with what it holds on the chip's tooltip.~~
      **Withdrawn by the owner on 2026-09-17** — see "Acceptance rework" below. The chip is
      removed entirely; the 2 px ring is the whole cue.
- [x] A single Space stays borderless and unlabelled — nothing appears where nothing appeared
      before.
- [x] The chip does not collide with, hide, or replace the channel badge when a Space belongs
      to a broadcast channel. **Moot since the rework**: there is no chip, and the badge is
      back in the corner on its own, exactly as before this packet.
- [x] The cue and the chip take their colours from the theme; no colour literal is added to
      `crates/terminal-view`.
- [x] The cue is visible in both a light and a dark theme, and after `US-0111` raises the
      secondary-text tokens — `evidence/US-0117-09-light-split-two-terminals.png` and its
      focus-moved twin, measured at 2 px of `#526fff` on `Zed One Light`.
- [x] The terminal grid does not lose a row or a column to the chip — it overlays, it does not
      push content — **and** (added by the `F-117.1` ruling, because the first attempt met the
      letter of this and not its point) the first prompt line is readable under the chip in
      every captured frame.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Changed: `docs/terminal-split.md` decision 8 — **amended, not reversed**, with the reason the
amendment does not contradict it (the frame is still 1 px + 1 px for every Space and a lone
Space is still borderless; what got stronger is the *active cue*, painted over that frame as an
overlay so no geometry changes with focus), what the ring actually paints over (the gutter
and one pixel of terminal edge, **not** the outer border — an absolute child is laid out
against the padding box), and the inactive-Space number chip recorded beside it.
`docs/gui-layout.md` §Broadcast input channels — the badge slot now carries the Space label
beside the badge, in one row, so neither can hide the other.

Unchanged, reasons still valid: `docs/terminal-split.md` decision 9 (the empty Space is
`US-0115`'s, and the label rule is consistent with it — an empty Space keeps only its
placeholder); `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md` (membership is
untouched; the badge still means exactly what it meant); `docs/PROJECT.md`.
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

- [x] Re-measure the current cue on a fresh capture so the before number is this packet's own.
- [x] Widen the cue; capture; measure again against the acceptance.
- [x] Add the label in the badge slot; check the channel-badge collision case.
- [x] Amend `docs/terminal-split.md` §8.
- [x] Re-capture the scenes in both a light and a dark theme.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
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

### Evidence

Branch `worktree-agent-a8b32ce5d4725a1f5`, commit `feat(terminal-view): make the active Space unmistakable
and name each Space`.

Changed:

- `crates/terminal-view/src/space/render.rs` — `active_cue_ring`, `space_number_chip` and
  `space_chip_tooltip`, the corner slot that carries the chip and the badge in one row,
  plus a test.
- `docs/terminal-split.md` decision 8 (amended, not reversed) and `docs/gui-layout.md`
  §Broadcast input channels.
- No colour literal was added: the ring takes the colour `space_border_color` already returns,
  which is the channel colour for a member Space and `table_active_border` otherwise.

Checks:

- `cargo test -p oneterm-terminal-view --lib` — 352 passed, 0 failed. The pure logic is
  `space_chip_tooltip`: an SSH-style title keeps its text, a shell that announces itself as a
  path is named (`C:\WINDOWS\system32\cmd.exe` -> `cmd.exe`), and a silent shell has nothing
  to say rather than an empty tooltip. `space_border_color`'s existing tests still pass, so
  the channel rule is unchanged. **The ring's width and colour are element properties and are not queryable in the
  panel tests; they are proved by the measurement below.**
- `pwsh scripts/ci-local.ps1` — see below.

Measured, on the walk's own captures (window rect 1400x900, PrintWindow at the window's own
scale, so capture scale is 1.0 and a pixel in the file is a pixel on screen):

| frame | accent run at y=400 | colour |
| --- | --- | --- |
| before (walkthrough `research/before/09-split-two-terminals.png`) | 1 px, x=561 | `rgb(82,139,255)` |
| `evidence/US-0117-09-split-two-terminals.png` | **2 px, x=461-462** | `rgb(82,139,255)` |
| `evidence/US-0117-09b-split-two-terminals-focus-moved.png` | **2 px, x=457-458** | `rgb(82,139,255)` |
| `evidence/US-0117-09-light-split-two-terminals.png` (Zed One Light) | **2 px, x=461-462** | `rgb(82,111,255)` = `#526fff` |

The numbers are unchanged by the rework round — the ring was never what `F-117.1` was about —
and were re-measured on the reworked build; the independent verifier measured the same runs
from the same pixels (`evidence/terminal-view-wave1-verify.md`).

Same colour, twice the width, and the two frames show the cue **moving** with focus: in `09`
the accent sits on the right Space's left edge, in `09b` on the left Space's right edge, with
the 3 px neutral separator (`rgb(62,68,81)`) beside it in both.

GUI walk (all frames re-captured on the reworked build except `05`, which the rework does not
touch; the app ran from a throwaway working directory, so its `target/ui_config.json` was the
scratch one and no repository file was changed):

- `evidence/US-0117-09-split-two-terminals.png` — two Spaces, the active one obvious at 100 %
  without measuring. Only the **inactive** Space carries a chip, reading `#0`; the active one
  carries the ring and nothing else. Both first prompt lines are fully legible, and they wrap
  at the same column, so the chip covers no glyph.
- `evidence/US-0117-09b-split-two-terminals-focus-moved.png` — focus moved: the ring is on the
  left Space and the chip has moved to the right one, now reading `#1`. The cue and the chip
  are never on the same Space.
- `evidence/US-0117-09-light-split-two-terminals.png` and `-09b-light-split-focus-moved.png`
  — the same pair in **Zed One Light**. This is the light-theme frame the packet owed.
- `evidence/US-0117-07-split-right-empty-space.png` — a terminal Space plus an empty one: the
  empty Space keeps only its placeholder, the terminal Space (inactive) carries `#0`.
- `evidence/US-0117-05-single-space.png` — a single Space: still borderless, still unmarked.
  Nothing appears where nothing appeared before.
- The terminal grid loses no row or column: the ring is an absolutely-positioned overlay inside
  the Space's existing padding box, and the padding is unchanged at 1 px, so the content box is
  identical whether the Space is active or not.

### Rework round — `F-117.1`, the corner label covered live output

Independent verification (`evidence/terminal-view-wave1-verify.md`) raised one major: the corner
label as first shipped was `#N` **plus the live session title**, in a chip up to 160 px wide with
a `background.opacity(0.75)` backdrop, on **every** Space in a split including the active one —
parked over the top-right of the terminal's own viewport. Its own captures showed the first
prompt line washed out under `#0 cmd.exe`. The packet's acceptance ("the terminal grid does not
lose a row or a column to the label") was satisfied literally, since nothing reflows, while
failing its intent: the top row of a running shell is not chrome, it is wherever the output is.

Reworked on a coordinator ruling, no owner round-trip:

| | before | after |
| --- | --- | --- |
| face | `#N` + session title | `#N` only |
| width | up to 160 px | ~18 px (glyph box measured 12x9 px in a 16 px-tall box with 3 px padding) |
| backdrop | `background` at 0.75 alpha | opaque `background`, chip-sized |
| shown on | every Space in a split | **inactive** Spaces only |
| what it holds | on the face | on the chip's **tooltip** |

The active Space gets no chip at all, because the 2 px ring already answers "where does my
typing go"; the chip carries the one thing the ring cannot, which is *which* Space this is. The
chip keeps the channel badge's own 16 px footprint and its place in the badge row, left of the
badge, so the row behaviour is unchanged. `background` under `muted_foreground` is the pairing
`scripts/check-theme-contrast.py` already holds at >= 4.5:1 in every built-in theme, so the chip
needs no private colour and stays legible on light.

`space_corner_label` is gone; `space_chip_tooltip` replaces it and is the tested pure piece.
`docs/terminal-split.md` decision 8 records the rework and why the first attempt was wrong.

Verified on the re-captured frames: in `US-0117-09-split-two-terminals.png` and its light twin,
the two Spaces' first prompt lines wrap at the same column and both read in full, so the chip
covers no glyph — which the 160 px label demonstrably did.

### Gaps

- **`F-117.4` is not fixed, deliberately.** The chip's tooltip reads `session.title()` raw, so it
  ignores `TabTitleMode::Default` (the setting that says "do not use OSC titles") and a manual
  tab rename, both of which the tab label honours. Routing it through `tab_label_with_title`
  would mean reading the `TerminalPanel` entity from inside `SpaceTree::render`, which runs while
  that panel is already leased — the same double-lease class of bug the `US-0116` walk hit. It
  needs the setting passed down the render call instead, which is more than a tooltip is worth
  here. The face (`#N`) cannot lie; only the hover detail can.
- **No broadcast-channel collision frame.** The chip and the badge share one `h_flex` row, chip
  left of badge, so neither can overlay the other by construction, and a Space with no channel
  simply has no badge in the row. That is the design, not a capture: joining a channel needs a
  second Space and two menu hops the walk did not take. The collision case is **unwalked** —
  though the chip is now ~18 px rather than up to 160 px, so the verifier's related note (a wide
  label plus a badge overrunning a narrow Space) no longer has a mechanism.
- **No two-by-two split frame.** Only the side-by-side case was captured, so the "chrome creep"
  risk the packet names is only half checked.
- `#N` is the Space's **stable `SpaceId`**, not a positional index: `display_number` returns
  the raw id (`crates/terminal-view/src/space/tree.rs:24-29`), which is allocated
  monotonically and never reused, so after a few split/close cycles a two-Space split can
  read `#0` and `#5`. It matches what the empty-Space placeholder already prints, so the
  surfaces agree; changing the numbering would be a separate decision about an existing
  surface. (Corrected in the rework round — this sentence previously said "0-based",
  which implies a positional numbering the code does not provide.)

## Acceptance rework 2026-09-17 — the ring is the whole cue, the chip is gone

The owner tried the built round and ruled: **remove the `#N` chip on split Spaces entirely.
Keep the 2 px active-Space ring.**

This is acceptance rework of this packet: the chip is this packet's own addition, and the
ruling came while trying this round's build.

### The reasoning the ruling settles

The chip answered "which Space is this?" — a question the ring already answers in the only form
that matters ("this one is taking my typing"), and that nothing else in the application asks the
user to know. `#N` is a stable `SpaceId`, so it is not even a position: a split can read `#0` and
`#5`, which is a number with no meaning outside the code. Against that, the chip paints opaque
theme background over the first two cells of a running shell's top row, permanently, on every
inactive Space. The first attempt at it was already reworked once for exactly this
(`F-117.1`, the 160 px label); the ruling finishes the job rather than shrinking it a second
time.

Removing it also removes two open items the packet was carrying: `F-117.4` (the chip's tooltip
read `session.title()` raw and ignored `TabTitleMode::Default` and manual renames — a tooltip
that can lie) and the unwalked badge-collision case. Neither has a subject any more.

### What changed

| Where | Change |
|---|---|
| `crates/terminal-view/src/space/render.rs` | `space_number_chip` and `space_chip_tooltip` deleted, with their tests and the `Tooltip`/`Stateful`/`SharedString` imports they alone needed. The corner slot goes back to what it was before this packet: the channel badge, absolutely positioned 5 px from the top and right, and nothing else — so a Space with no channel has no corner element at all. `active_cue_ring` and `space_border_color` are untouched. |
| `docs/terminal-split.md` | Decision 8: the number-chip paragraph is gone; the ring paragraph keeps its own text and gains one sentence recording that the chip was tried, reworked and then removed. |
| `docs/gui-layout.md` | The broadcast-input-channels paragraph drops the two chip sentences and reads as it did before this packet. |

`crate::panel::trim_path_title` stays — the tab label is its other caller.

### Frames

Re-taken on this session's own `fast-dev` build, in the default dark theme, at 1600x1000, with
`USERPROFILE`/`HOME` and the working directory pointed at a scratch tree so the configuration is
isolated. Same walk as the round's scenes 07-09: right-click the terminal, Split Right,
right-click the empty Space, New Terminal Here.

- `evidence/US-0117-rw2-09-split-two-terminals.png` — **scene 09.** Two terminals side by side,
  the 2 px ring on the right Space. **Neither Space carries a `#N` chip**; compare with
  `evidence/after/09-split-two-terminals.png`, where the left Space's top-right reads `#0` over
  the first line of the shell's output.
- `evidence/US-0117-rw2-09b-split-focus-moved.png` — **scene 09b.** After clicking the left
  Space: the ring has moved, and again neither Space carries a chip. The ring alone says which
  Space is active, in both frames.

The empty-Space step was captured too and shows the same thing at the moment the chip was most
visible before: in `evidence/after/08-empty-space-menu.png` the left Space reads `#0`; in this
walk it reads nothing.

### Gaps carried

- **`F-117.4` and the badge-collision case are closed by deletion, not by a fix.** If a
  per-Space label is ever wanted again it inherits both problems fresh, and it should start from
  why the ring was not enough rather than from this packet's chip.
- **The two-by-two split is still uncaptured**, as the original Gaps record. Removing the chip
  removes the only thing that made a narrow Space's chrome grow, so the "chrome creep" risk the
  packet named has no mechanism left.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
