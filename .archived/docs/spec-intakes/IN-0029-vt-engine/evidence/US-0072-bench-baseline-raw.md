# vt-bench — vendored alacritty_terminal + vte (the engine being replaced)

Geometry 160x45, 100 MiB per fixture, median of 3 runs. Recorded, never gated.

## Tiers 1-2: parser only, and parse plus grid

| Fixture | parser MiB/s | parser ns/B | parse+grid MiB/s | parse+grid ns/B |
| --- | ---: | ---: | ---: | ---: |
| `plain_ascii` | 1192.0 | 0.80 | 94.0 | 10.15 |
| `long_lines` | 1345.7 | 0.71 | 107.3 | 8.89 |
| `heavy_sgr` | 379.1 | 2.52 | 178.5 | 5.34 |
| `tui_redraw` | 447.9 | 2.13 | 130.2 | 7.32 |
| `scroll_region` | 815.0 | 1.17 | 113.5 | 8.40 |
| `cjk_wide` | 587.4 | 1.62 | 137.0 | 6.96 |
| `dense_cells` | 220.1 | 4.33 | 180.8 | 5.28 |
| `scrolling` | 752.1 | 1.27 | 109.4 | 8.71 |
| `sixel` | 443.3 | 2.15 | 38.5 | 24.77 |
| `osc_9_7` | 65.9 | 14.46 | 40.9 | 23.30 |

## Tier 3: parse, grid and one snapshot build per frame (600 frames)

| Fixture | us/frame | cells/frame |
| --- | ---: | ---: |
| `plain_ascii` | 38.1 | 7200 |
| `long_lines` | 37.1 | 7200 |
| `heavy_sgr` | 37.7 | 7200 |
| `tui_redraw` | 42.3 | 7200 |
| `scroll_region` | 23.7 | 7200 |
| `cjk_wide` | 23.1 | 7200 |
| `dense_cells` | 22.8 | 7200 |
| `scrolling` | 22.7 | 7200 |
| `sixel` | 24.6 | 7200 |
| `osc_9_7` | 21.1 | 7200 |

## Tier 4: resize latency, 80x24 to 100x40 (its own geometry, deliberately)

| Scrollback rows | grow us | shrink us |
| ---: | ---: | ---: |
| 0 | 1186 | 20 |
| 10000 | 4704 | 3361 |
| 100000 | 53182 | 39146 |

## Tier 5: live heap after 10000 scrollback rows

Live heap, not process RSS: the design claim is what a grid of N rows costs, and RSS also counts retained allocator pages.

| Content | heap bytes | bytes/row |
| --- | ---: | ---: |
| plain | 41380491 | 4138 |
| unicode | 41380491 | 4138 |
| styled | 41380491 | 4138 |
| mixed | 41380491 | 4138 |

