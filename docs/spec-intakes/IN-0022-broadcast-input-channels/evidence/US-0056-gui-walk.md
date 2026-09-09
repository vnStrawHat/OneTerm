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
