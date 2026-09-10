# Work: Input Channel menu, tab chips, Space frame, and key bindings

ID: US-0056
Intake: IN-0022
Created: 2026-09-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: IN-0022

## Outcome

From a Space's context menu the user joins channel A..E, leaves, closes the channel, or
applies a join or leave to every Space in the tab. Every member Space shows a badge with its
channel letter, every tab shows one chip per channel present in it, and the seven actions
are bindable in Settings › Key Bindings. Depends on US-0055.

## Scope

- [x] In scope: actions in `crates/actions`; `on_action` handlers and
  `join_tab_to_channel` / `leave_tab_channels` / `tab_channels` on `TerminalPanel`;
  `MenuContext` fields and the "Input Channel" submenu; `channel_color` in
  `crates/terminal-view/src/theme`; chips in `tab_title.rs`; the member badge in
  `space/render.rs`; registry observer on the panel; `BindableAction`
  entries; owning-doc updates.
- [x] Out of scope: status-bar segment; persistence; a tab-strip context menu.

## Acceptance

- [x] Submenu shows `Channel A..E` with the current one marked; `Leave Channel` and
  `Close Channel <X>` only for a member; the two tab-wide items only when the tab has more
  than one Space (join-all only when this Space is a member, leave-all only when any Space
  in the tab is a member).
- [x] Member Spaces draw the channel badge in their top-right corner, the lone Space of an
  unsplit tab included, and are framed in their channel colour whether or not they are
  active; a Space in no channel keeps the theme's active/inactive rule, and the single-Space
  fast path stays borderless. The chip letter is centred within 1 px of the 16 px box.
- [x] Tab chips list the distinct channels of the tab in A..E order and disappear when the
  tab has no member.
- [x] `Close Channel` from one tab repaints every other tab's chips and badges.
- [x] Seven actions appear in Settings › Key Bindings, group "Input Channel", unbound.
- [x] E2E: a tab split into three Spaces (two in A, one non-member) plus a second tab in A;
  `echo hi` typed once appears in the three members only. Screenshots in `evidence/`.
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0022-broadcast-input-channels/low-level-design/menu-chip-frame.md`
  — rules this packet implements.
- `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md`.
- `docs/terminal-split.md` — Space tree, Close Space, drag-into-Space.
- `docs/gui-layout.md` — context menu and tab strip composition.
- `README.md` § Features — user-facing feature list.

### Documentation Action

- Update required: `docs/terminal-split.md` (membership follows the view entity; new
  Spaces are non-members; the alternate-screen note), `docs/gui-layout.md` (submenu, chips,
  frame), `README.md` (feature bullet under Terminal split).

Reason: three owning docs describe surfaces this packet changes.

### Reconciliation

- `docs/terminal-split.md` — new "Broadcast input channels (IN-0022)" section: membership
  follows the view entity, new Spaces from Split / Duplicate Session are non-members, the
  frame rule for members, and the alternate-screen note (broadcast is not suppressed there).
- `docs/gui-layout.md` — new "Broadcast input channels" section (submenu order, chips, Space
  frame, the registry observer) plus three source-map entries.
- `README.md` — feature bullet under "Terminal split (Spaces)".
- `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md` — Status `accepted`.
- `docs/spec-intakes/IN-0022-broadcast-input-channels/low-level-design/menu-chip-frame.md` —
  reviewed, unchanged: the implementation follows it except for the deviations listed below.

## Context

- `DuplicateSession` is the end-to-end pattern: `actions!` entry, `BindableAction`, menu
  item with `.action(...)` for the shortcut hint, `on_action` on the panel root.
- `render_tab_strip` already hosts a conditional per-tab badge (the recording dot).
- `space_border_color` is a pure function with a truth-table test in `space/tests.rs`.
- The popup menu has no radio mark; the `* ` prefix marks the current channel.

## Plan

- [x] Actions + `BindableAction` entries.
- [x] Panel handlers and tab-wide helpers; registry observer.
- [x] `MenuContext` fields and submenu; menu tests.
- [x] `channel_color`, chips, badge; tests.
- [x] Docs, GUI evidence, gate.

## Decisions

- DEC-0009.

## Verification Plan

- `cargo test -p oneterm-terminal-view -p oneterm-settings-ui -p oneterm-actions`
- GUI walk with PrintWindow screenshots (light and dark theme).
- `pwsh scripts/ci-local.ps1`

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, `CARGO_TARGET_DIR=target/in22-target` because the user's own
OneTerm holds `target/`):

- `cargo test -p oneterm-terminal-view -p oneterm-settings-ui -p oneterm-actions` — pass
  (306 passed, 2 ignored). New tests: `channel_menu_items` conditions and
  `channel_item_label` (`input/menu_tests.rs`), `space_border_color` channel branch
  (`space/render.rs`), `channel_color` mapping (`theme/input_channel.rs`),
  `tab_channel_helpers_cover_every_space_of_the_tab` (`panel/tests.rs`), and
  `the_seven_input_channel_actions_ship_unbound` (`key_bindings_actions.rs`).
- `pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`.
- GUI walk with PrintWindow screenshots in `evidence/`, described in
  `evidence/US-0056-gui-walk.md`: the three-Space split with two members of channel A, the
  second tab in the same channel, `echo hi` typed once reaching the three members only, the
  submenu, the two-chip tab strip, the repaint after `Close Channel`, and the same layout in
  the light theme.

Deviations from the LLD:

- The submenu items call the registry (and, for the two tab-wide items, the panel) directly
  instead of dispatching the actions. The menu belongs to the Space that was right-clicked,
  which is not necessarily the active one, while the actions act on the active Space — the
  same reason the Split and Duplicate items already call the panel. The items still carry
  their action for the shortcut hint, exactly like `Split Right`.
- Non-current channel labels are indented by two spaces (`"  Channel A"`) so the `* ` marker
  of the current one does not shift the column.
- `SpaceTree::render` takes the registry handle as a parameter. Reading it out of the panel
  inside `render_leaf` would re-enter the panel entity that is being rendered.
- `MenuContext.tab_spaces` counts terminal Spaces (`TerminalPanel::terminal_space_count`),
  not leaves: an empty Space cannot join a channel.
- The item list is a pure `channel_menu_items` + `channel_item_label` pair (as the LLD's
  fallback allows) because `PopupMenu` keeps its items private.

Gaps: the GUI walk ran while the workstation was locked, so it used posted window messages;
the seven actions were bound temporarily in the scratch `target/ui_config.json` to reach them
by key (they ship unbound, as the Key Bindings test asserts). No status-bar segment and no
persistence — out of scope here.

## Rework 2026-09-09

Owner feedback on `evidence/US-0056-split-broadcast-dark.png` (translated): "I cannot tell
apart the case of one tab with three Spaces where only one Space joined channel A."

Cause: the frame is the only per-Space marker, and in the dark theme the channel-A colour
(`chart_1`) is nearly the same blue as `table_active_border`, the active-Space border, while
the inactive member's 55 % frame reads like the plain theme border. A frame colour alone
cannot say which Space is a member.

Change: every member Space now draws the tab chip again as a badge in its top-right corner
(bold channel letter, `channel_color`, text `cx.theme().background`), inset 3 px from the top
and 15 px from the right so it clears the terminal's 12 px scrollbar track. It renders in both
paths of `space/render.rs` (split and the single-Space fast path, which becomes `.relative()`),
takes no keyboard focus and carries no click handler. The chip builder moved from
`panel/tab_title.rs` into `theme/input_channel.rs` (`channel_chip` plus the pure
`channel_chip_style`), so the tab chip and the Space badge are one function. The frame rule is
unchanged and is now the secondary cue.

New test: `every_channel_chip_shows_its_letter_on_its_channel_color`
(`theme/input_channel.rs`) asserts the label/background/foreground triple for A..E.

Docs updated with the change: this packet, `low-level-design/menu-chip-frame.md` (Space badge
section, interfaces, verification), `high-level-design.md` (wireframe and bullets),
`docs/gui-layout.md`, `docs/terminal-split.md`.

Evidence (rework GUI walk, described in `evidence/US-0056-gui-walk.md`):
`evidence/US-0056-rework-one-member-dark.png` (three Spaces, only one in channel A, the active
Space being a non-member), `evidence/US-0056-rework-two-channels-dark.png` (two Spaces in A,
one in B), `evidence/US-0056-rework-one-member-light.png` (the first case in the light theme).

Verification: `cargo test -p oneterm-terminal-view` and `pwsh scripts/ci-local.ps1`.

## Rework 2 2026-09-09

Owner feedback: "Remove the coloured frame."

Change: a member Space no longer draws a frame in its channel colour. The top-right badge
added by the first rework is the only per-Space marker, and Space borders return to the
pre-IN-0022 rule for every Space: `table_active_border` for the active Space, `border` for
the inactive ones, and no border at all on the single-Space fast path. That path keeps
`.relative()` and the badge, so a lone member is still marked without changing the pixel
layout of an unsplit tab. `space_border_color(is_active, active, inactive)` is back to its
original signature and its original truth-table test; the channel-branch test is gone, and
`space/render.rs` no longer imports `channel_color` (the chip builder in
`theme/input_channel.rs` still uses it, so the function itself stays).

Docs updated with the change: this packet, `low-level-design/menu-chip-frame.md` (the Space
frame section dropped, the badge section states the border rule), `high-level-design.md`
(Idea, wireframe, bullets), `IN-0022.md` (surfaces and safety bullets), `docs/gui-layout.md`,
`docs/terminal-split.md`, `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md`,
`README.md`.

Evidence (described in `evidence/US-0056-gui-walk.md` under "Rework 2"):
`evidence/US-0056-rework2-one-member-dark.png` (one tab, three Spaces, only one in channel A,
the active Space a non-member) and `evidence/US-0056-rework2-two-channels-dark.png`
(two Spaces in A, one in B). Pixel sampling of both confirms the member borders equal the
theme rule.

Verification: `cargo test -p oneterm-terminal-view` and `pwsh scripts/ci-local.ps1`.

## Rework 3 2026-09-10

Owner feedback: "The channel chip on the tab title is not square", then "put the Space
badge at top 5 px, right 5 px".

Change: `channel_chip` is a fixed 16 px square (`size`, flex-centred letter, rounded
corners kept) instead of a padded box whose width followed the letter, so the tab chip and
the Space badge share one footprint. The badge moved from 3 px / 15 px to 5 px from the top
and right edges of the Space; it now overlaps the scrollbar track, which only shows a thumb
at the top when the view is scrolled up. Docs updated: `low-level-design/menu-chip-frame.md`
and `docs/terminal-split.md`.

Evidence: `cargo test -p oneterm-terminal-view` and `pwsh scripts/ci-local.ps1` (recorded
below in Evidence and Gaps). No new screenshot: the owner placed the badge by hand in the
running build and asked for exactly that offset.

## Rework 4 2026-09-10

Owner feedback: "The letter A inside `channel_chip` is not centred; it is shifted to the
left." and "The Space border colour must be the channel colour; a Space in no channel uses
the default colour."

### The letter is one pixel left of centre

Measured on the `fast-dev` build at device scale 1.0 (Zed One Dark, tab chip at window
`(20,42)-(35,57)`, Space badge at `(771,71)-(786,86)`; both boxes are exactly 16x16 px):

| | tab chip | Space badge |
|---|---|---|
| ink bounding box | `(2,4)-(11,12)` | `(2,3)-(11,11)` |
| margins L / R | 2 / 4 | 2 / 4 |
| margins T / B | 4 / 3 | 3 / 4 |
| ink centre minus box centre | dx -1.00, dy +0.50 | dx -1.00, dy -0.50 |

Reading the anti-aliasing ramp of the bottom row gives the sub-pixel span: the ink runs from
x 2.90 to x 11.15 (8.25 px wide), where a centred glyph would run from 3.855 to 12.145 — a
**0.95 px shift to the left**. Vertically both chips are within half a pixel, so only the
horizontal shift is a defect.

Cause: `justify_center` centres the *text node's rounded box*, not the glyph. A diagnostic
build that painted the text node's own background showed that box as **9 px wide at offset 3**
inside the 16 px square (and 12 px tall at offset 2). Segoe UI Bold "A" at `text_xs` (12 px)
has an advance of 8.4375 px and an ink width of 8.29 px, so the ideal left offset is
`(16 - 8.4375) / 2 = 3.78`. Taffy rounds the text node's layout to whole pixels: the box
becomes 9 px wide and lands at 3. The glyph is painted at that box's left edge, so all of the
0.56 px of rounding slack is added on the right — the ink ends up 0.95 px left of centre. The
same rounding is why the tab chip and the badge disagree by a pixel vertically.

Change: the chip no longer uses flex to place the glyph. `channel_chip` keeps the 16 px
square and gives the text node the whole square instead: no `.flex()` (a `div` is a block box,
so the text node fills its width), `.line_height(px(16.))` so the line box is the full square,
and `.text_center()` so GPUI aligns the shaped run inside the text bounds in floating point at
paint time (`paint_line` -> `aligned_origin_x` with the element bounds as the align width).
No integer rounding sits between the box and the glyph any more, and no pixel nudge is needed.
`line_height` also pins the vertical placement, which the theme's own line height had left to
the flex box.

### The Space border carries the channel colour

`space_border_color(is_active, active, inactive)` becomes
`space_border_color(is_active, channel: Option<Hsla>, active, inactive)`: a member Space uses
its channel colour at full strength whether or not it is the active Space, a non-member keeps
today's `table_active_border` / `border` rule. The split path passes
`channel.map(|ch| channel_color(ch, cx))`. The single-Space fast path stays borderless — a
lone Space is marked by its badge only, as rework 2 asked. The truth-table test grew the two
membership rows.

The colour clash that removed the frame in rework 2 is no longer a problem: the badge added
in rework 1 is what says *which* channel a Space is in, so the border only has to repeat it,
and the owner asked for the repetition.

Docs updated with the change: this packet, `low-level-design/menu-chip-frame.md` (chip
centring rule, border rule in its new form), `high-level-design.md` (Idea, wireframe,
bullets), `IN-0022.md`, `docs/gui-layout.md`, `docs/terminal-split.md`, `README.md`, and
`evidence/US-0056-gui-walk.md` ("Rework 4 walk").

### Result

Re-measured on the same build and window: the ink's horizontal offset from the box centre
went from -0.95 px to **-0.03 px** in the tab chip and in the Space badge alike (margins
L/R 2/4 -> 3/3). The vertical offset stays at +0.77 px in both, inside the 1 px bar; it is
the font's own asymmetry (Segoe UI's ascent 12.95 px + descent 3.01 px fill the 16 px line
box, so the baseline lands at 12.97 px while the cap height of "A" is 8.40 px), so no pixel
nudge was added. The tab chip and the badge now agree to the pixel in both directions.

Border pixel samples on `evidence/US-0056-rework4-border-dark.png` (Zed One Dark, gutter =
second pixel in from the edge): channel-A Space (inactive) `#61AFEF`, channel-B Space
(active) `#98C379`, non-member (inactive) `#3E4451`, the same non-member made active
`#528BFF`.

New test: `member_space_uses_its_channel_color_active_or_not` (`space/render.rs`), next to
the existing `selected_space_uses_active_gutter_color` truth table.

Evidence: `evidence/US-0056-rework4-chip-before.png`, `evidence/US-0056-rework4-chip-after.png`
(both chips at 8x with the box centre drawn), `evidence/US-0056-rework4-border-dark.png`,
and the "Rework 4 walk" section of `evidence/US-0056-gui-walk.md`.

Verification: `cargo test -p oneterm-terminal-view` (282 passed, 2 ignored) and
`pwsh scripts/ci-local.ps1`.

## Handoff

IN-0022 is complete as specified: US-0055 (registry + fan-out) and US-0056 (menu, chips,
frame, key bindings) are implemented. Open follow-ups stay out of scope: a status-bar
channel segment, persistence of membership (see DEC-0009), and a tab-strip context menu.
