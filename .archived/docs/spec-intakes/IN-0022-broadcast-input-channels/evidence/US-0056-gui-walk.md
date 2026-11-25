# US-0056 GUI walk (Windows 11, `fast-dev` build)

Driven with posted `WM_KEYDOWN` / `WM_CHAR` / mouse messages and captured with
`PrintWindow(hwnd, hdc, 2)` (the workstation was locked, so injected input was not
available). The seven channel actions ship unbound, so the walk bound them temporarily in
`target/ui_config.json` (`f5` = Join Channel A, `f6` = Join Channel B, `f8` = Close Channel);
that file is scratch state outside the repository and was restored afterwards.

| Screenshot | What it shows |
|---|---|
| `US-0056-submenu-dark.png` | The context menu of a member Space in a three-Space tab: "Input Channel" after the Split items, and the submenu with `* Channel A`, `Channel B`..`E`, `Leave Channel`, `Close Channel A`, `Join All Spaces In Tab To A`, `Leave With All Spaces In Tab`. |
| `US-0056-split-broadcast-dark.png` | A tab split into three Spaces, the two left ones in channel A, the right one a non-member. `echo hi` was typed once in the top-left Space: both members ran it, the non-member is untouched. The two members carry the channel frame; the non-member keeps the theme border. |
| `US-0056-lone-member-tab-dark.png` | The second tab, a single Space in channel A: it ran the same `echo hi` from the other tab, shows the `A` chip, and draws the 1 px channel frame that a lone Space would otherwise not have. |
| `US-0056-tab-chips-dark.png` | Tab-strip close-up: the split tab with a Space in A and a Space in B shows two chips in A..E order and truncates its title; the second tab shows one. |
| `US-0056-after-close-channel-dark.png` | After `Close Channel A` from the first tab: both tabs lost their chip and both frames returned to the theme rule — the registry observer repainted the other tab. |
| `US-0056-split-broadcast-light.png` | The same layout in the Zed One Light theme: chips `A` and `B`, the active member framed in `chart_1` (`#4078F2`), the inactive `B` member framed in `chart_2` at 55 % (sampled `#9DC99C` over the light background). |

## Rework walk (2026-09-09): the per-Space channel badge

Same method, same build (`CARGO_TARGET_DIR=target/in22-target`, `fast-dev`). The scratch
`target/ui_config.json` bound `f2` = Split Right, `f3` = Split Down, `f5` = Join Channel A and
`f6` = Join Channel B so that every step is a plain key (posted messages carry no modifier
state); terminals were put into the new Spaces through the placeholder menu ("New Terminal
Here") with posted mouse messages. The file was restored afterwards and every launched
instance was stopped.

| Screenshot | What it shows |
|---|---|
| `US-0056-rework-one-member-dark.png` | One tab, three Spaces, only the top-right one in channel A: it carries the `A` badge in its top-right corner. The active Space is the bottom-right non-member, so the blue active border and the channel-A frame are both on screen — the badge is what tells the member apart. |
| `US-0056-rework-two-channels-dark.png` | The same tab with the two right Spaces in channel A and the left one in channel B: badges `A`, `A`, `B`, and the tab strip shows the matching `A` and `B` chips. |
| `US-0056-rework-one-member-light.png` | The first case in the Zed One Light theme: the badge keeps the chip's look (white letter on `chart_1`) against the light background. |

## Rework 2 walk (2026-09-09): the channel frame removed

Same method and build (`CARGO_TARGET_DIR=target/in22-target`, `fast-dev`). The scratch
`target/ui_config.json` bound `f2` = Split Right, `f3` = Split Down, `f5` = Join Channel A and
`f6` = Join Channel B; terminals were put into the new Spaces through the placeholder menu
("New Terminal Here"). The file was restored afterwards and every launched instance was
stopped.

| Screenshot | What it shows |
|---|---|
| `US-0056-rework2-one-member-dark.png` | One tab, three Spaces, only the top-right one in channel A: it carries the `A` badge and nothing else. The active Space is the bottom-right non-member, so the only coloured border on screen is its own active border. |
| `US-0056-rework2-two-channels-dark.png` | The same tab with the two right Spaces in channel A and the left one in channel B: badges `A`, `A`, `B` and the matching `A`/`B` tab chips; no Space is framed in a channel colour. |

Pixel samples (Zed One Dark: `border` `#3E4451`, `table.active.border` `#528BFF`, channel A
`chart.1` `#61AFEF`, channel B `chart.2` `#98C379`):

- `US-0056-rework2-one-member-dark.png`: the channel-A member's top border is `#3E4451`
  (the theme's inactive rule), the active non-member's gutter is `#528BFF`.
- `US-0056-rework2-two-channels-dark.png`: the active channel-B member is `#3E4451` outside
  and `#528BFF` in the gutter; both inactive channel-A members are `#3E4451`. The channel
  colours appear only in the badges and the tab chips.
- Single-Space fast path: a lone Space that joins channel A gains the badge and stays
  borderless — its window edges sample the same `#23272E` background as before the join.

## Rework 4 walk (2026-09-10): the chip letter centred, the channel border

Same method and build (`CARGO_TARGET_DIR=target/in22-target`, `fast-dev`, window forced to
1280x800 at device scale 1.0, Zed One Dark). The scratch `target/ui_config.json` bound
`f2` = Split Right, `f3` = Split Down, `f5` = Join Channel A and `f6` = Join Channel B;
terminals were put into the new Spaces through the placeholder menu ("New Terminal Here").
The file was restored afterwards and every launched instance was stopped.

### Chip centring, measured

Both chips are exactly 16x16 px in the capture (tab chip at `(20,42)`, Space badge at
`(771,71)`). "Ink" is every pixel of the chip that is not the flat channel colour; the
sub-pixel span is read off the anti-aliasing ramp of the glyph's bottom row.

| | before | after |
|---|---|---|
| tab chip, ink margins L / R | 2 / 4 | 3 / 3 |
| Space badge, ink margins L / R | 2 / 4 | 3 / 3 |
| tab chip, ink margins T / B | 4 / 3 | 4 / 3 |
| Space badge, ink margins T / B | 3 / 4 | 4 / 3 |
| sub-pixel ink span (x) | 2.90 .. 11.15 | 3.85 .. 12.10 |
| horizontal offset from the box centre | **-0.95 px** | **-0.03 px** |
| vertical offset from the box centre | +0.77 px | +0.77 px |

The horizontal shift the owner reported is gone (0.03 px, well inside the 1 px bar). The
vertical offset is unchanged at 0.77 px and is a property of the font, not of the layout:
Segoe UI's ascent (12.95 px at 12 px) plus descent (3.01 px) fills the 16 px line box almost
exactly, which puts the baseline at 12.97 px, while the cap height of "A" is only 8.40 px —
so the cap ink sits 0.77 px below the box centre. It is inside the 1 px bar, so no pixel
nudge was added; a nudge would be a font-specific magic number. The tab chip and the badge
now agree to the pixel in both directions, which they did not before (`line_height(px(16.))`
pins the line box instead of leaving it to the flex box's rounding).

| Screenshot | What it shows |
|---|---|
| `US-0056-rework4-chip-before.png` | The tab chip (left) and the Space badge (right) at 8x before the fix, with the exact box centre drawn in magenta. The "A" clearly sits left of the vertical centre line. |
| `US-0056-rework4-chip-after.png` | The same two chips at 8x after the fix: the "A" straddles the centre line. |

### Channel border

| Screenshot | What it shows |
|---|---|
| `US-0056-rework4-border-dark.png` | One tab, three Spaces: the left one in channel A (inactive), the top-right one in channel B (active), the bottom-right one a non-member. Two chips (`A`, `B`) on the tab title. |

Pixel samples (Zed One Dark: `border` `#3E4451`, `table.active.border` `#528BFF`, channel A
`chart.1` `#61AFEF`, channel B `chart.2` `#98C379`). Each Space paints a 1 px outer `border`
and a 1 px inner gutter that carries `space_border_color`, so the gutter is the second pixel
in from the edge:

- Channel-A Space (inactive), left edge at y=400: `#3E4451` then `#61AFEF` — the channel
  colour at full strength although the Space is not active.
- Channel-B Space (active), top edge at x=600: `#3E4451` then `#98C379`.
- Non-member Space (inactive), bottom edge at x=600: gutter and outer border are both
  `#3E4451`, the theme's inactive rule.
- Non-member Space made active (`s6`, not kept): right edge gutter `#528BFF`, while the two
  member Spaces kept `#61AFEF` and `#98C379` — membership beats activity, activity still
  marks a Space in no channel.
- Single-Space fast path: unchanged, a lone Space stays borderless whether or not it is a
  member; the badge alone marks it.
