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

