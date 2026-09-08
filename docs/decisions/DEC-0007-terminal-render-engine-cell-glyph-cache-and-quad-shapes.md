# DEC-0007 Terminal view: from-scratch GPUI render engine with quad-drawn symmetric shapes

Date: 2026-09-08

## Status

accepted

## Context

`crates/terminal-view` grew by accretion and borrowed structure from several external
terminal renderers. The owner asked for a rebuild that is designed on GPUI's own element
model, keeps every feature, and treats performance as a requirement. Box-drawing glyphs
rendered through fonts do not join cleanly across cells and differ per font.

## Decision

Future work on the terminal view inherits these choices:

1. The terminal **engine** (grid, VT parsing, PTY/SSH pumps) stays in `crates/terminal`
   on the vendored `alacritty_terminal`; the **view** crate owns rendering and input only.
2. The render engine is written from scratch against the GPUI API. No external terminal
   renderer (Zed, wezterm, alacritty's GL renderer, Windows Terminal, ...) is used as a
   reference or copied from; only GPUI's own sources and docs are consulted.
3. Box drawing (U+2500-257F), block elements (U+2580-259F), braille (U+2800-28FF),
   powerline (U+E0B0-E0BF) and related symbols are painted with quads (and paths for
   arcs/diagonals), never with font glyphs.
4. Every such shape is defined by geometry that is symmetric about the cell's vertical and
   horizontal center axes: rects are computed from the cell center and half-extents, and
   mirrored variants are derived by reflection, not by separate left-to-right or
   top-to-bottom accumulation. Tests assert the symmetry.
5. Per-frame work is bounded by dirty rows: unchanged rows reuse their row plan; shaped
   text is cached per cluster/style; an idle terminal produces no shaping and no plans.

## Alternatives

- [x] Selected approach described above.
- [ ] Keep the old crate and patch it: rejected by the owner (patchy, borrowed).
- [ ] Rewrite the VT engine too: out of scope; `crates/terminal` already isolates it and all
      backends depend on it.
- [ ] Draw box glyphs with the font: rejected (seams, font dependence).

## Consequences

- [ ] Benefit to confirm: identical or better frame cost on DOOM-fire; seamless box drawing across fonts.
- [ ] Tradeoff: old rendering docs become historical and are superseded by IN-0018's HLD.
