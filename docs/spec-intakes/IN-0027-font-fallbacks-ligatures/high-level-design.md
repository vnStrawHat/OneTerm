# High-Level Design: Font fallbacks and ligatures

Intake: IN-0027
Lane: normal
Date: 2026-09-11

## Idea

Two additive font settings ride the existing `terminal.json` -> `TerminalSettings` ->
`terminal_font()` -> `RenderInputs` path:

- `font.fallbacks: [String]` becomes `gpui::Font::fallbacks`. The platform text system
  (DirectWrite on Windows) consults those families, in order, before the system fallback for
  any glyph the primary family lacks. Families that are not installed are skipped.
- `font.ligatures: bool` decides the `calt` feature value that `terminal_font()` already
  emits (today hard-coded to `0`); an explicit entry in `font.features` still wins.

Ligatures were disabled because GPUI's `force_width` post-pass numbers glyphs, not cells: a
ligature is one glyph for two or more characters, so every glyph after it lands one cell too
far left. The painter now anchors each glyph at the cell its byte index belongs to and keeps
GPUI's within-cell offset for combining marks. The shaped line is untouched (it is shared by
the GPUI and `GlyphCache` caches); only the paint-time `x` changes.

## Diagram

```text
 terminal.json font{family,size,weight,features,fallbacks,ligatures}
        │ apply.rs / persist.rs
        ▼
 TerminalSettings{font_features, font_fallbacks, font_ligatures}
        │ terminal_view/render.rs::terminal_font  (CachedFont key: family, weight, features,
        ▼                                          fallbacks, ligatures)
 gpui::Font{family, weight, features: calt=ligatures + overrides, fallbacks}
        │ RenderInputs.font  ── style_key() hashes features AND fallbacks ──▶ plan cache reset
        ▼
 FontSet (4 variants clone fallbacks) ──▶ GlyphCache::shape(text, force_width=cell)
        ▼
 element.rs::paint_shaped_line
   for glyph: cell = cell_of(glyph.index)          // run text byte -> run column
              x = col0 + cell*cell_width + (glyph.x - x of first glyph in that cell)
```

## UI Wireframe

Settings > Terminal > Font gains two rows under Line Height:

```text
+-- Font ---------------------------------------------------------------------+
| Font Family      [ Lilex                                  v ]                |
| Font Size        [ 15   ] +-                                                 |
| Font Weight      [ Normal v ]                                                |
| Line Height      [ 1.2  ] +-                                                 |
| Fallback Fonts   [ Symbols Nerd Font Mono, Symbols Nerd Font          ]      |
|                  Comma-separated families tried for glyphs the font lacks.   |
| Ligatures        [x]                                                         |
|                  Render calt ligatures (=>, ->, !=) when the font has them.   |
+------------------------------------------------------------------------------+
```

Terminal grid, before and after `US-0065`, font with `=>` ligature, cells marked `|`:

```text
before  |=>|x |   (the arrow glyph spans two cells, `x` is painted in cell 2 over its tail)
after   |=>|  |x   (`x` stays in cell 3)
```

## Data Flow

1. `FontConfig` (serde default) reads `fallbacks` (default
   `["Symbols Nerd Font Mono", "Symbols Nerd Font"]`) and `ligatures` (default `true`).
2. `TerminalSettings::from_config` / `to_config` copy both; `set_font_fallbacks` /
   `set_font_ligatures` mutators mirror the existing `set_font_features`.
3. `terminal_font()` builds `FontFallbacks::from_fonts(list)` (`None` when empty) and emits
   `("calt", ligatures as u32)` before the user feature overrides.
4. `CachedFont::matches` and `RenderInputs::style_key` include the fallback list so the view
   rebuilds the font and drops the plan cache when the list changes; `ensure_fonts` already
   clears the `GlyphCache` whenever the `Font` value differs.
5. `FontSet::new` clones the base font per variant, so fallbacks reach bold/italic runs.
6. `paint_shaped_line` receives the run's cell width and maps `glyph.index` to a cell by
   counting the run text's characters up to that byte, skipping zero-width characters
   (they share their base cell). Each run stores the byte offsets where its cells begin
   (`cell_starts`, one `u32` per column) so the mapping is a binary search.
7. Settings UI: an `Input` field (comma-separated list, trimmed, empty entries dropped) and
   a `Switch`; both use the existing `set()` helper and `on_reset` pattern.

## Detail Design

- [x] Detail design: not needed
- Reason: two settings fields and one paint-time position rule; the glyph mapping is covered
  by unit tests in `element_tests.rs` / `row_plan.rs`.
