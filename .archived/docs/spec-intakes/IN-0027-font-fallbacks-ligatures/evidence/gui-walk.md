# GUI walk: fallbacks and ligatures (2026-09-11, Windows 11, fast-dev build)

No Nerd Font is installed on the test machine, so the fallback mechanism was exercised with
`Segoe Fluent Icons`, a system font whose icons live in the same private-use range
(U+E000-F8FF) Nerd Fonts use. `terminal.json` was swapped between two configurations and the
app restarted; a local `cmd` shell printed UTF-8 samples through
`chcp 65001 & powershell -NoProfile -Command "Get-Content -Encoding utf8 <file>"`.

| Config | `font.fallbacks` | `font.ligatures` | Crop |
| --- | --- | --- | --- |
| A | `["Segoe Fluent Icons"]` | `true` | `US-0065-ligatures-on.png`, `US-0064-fallback-segoe-fluent-icons.png` |
| B | `[]` | `false` | `US-0064-US-0065-fallbacks-empty-ligatures-off.png` |

Sample 1 (ligatures): `|=> -> != <= === www |ab|` over `|a  b  c  d  e   fgh |cd|`.
Sample 2 (fallback): `|U+E700 U+E70F U+E8A5 U+E77B|ab|`.

Observed:

- A: `=>`, `->`, `!=`, `<=`, `===` and `www` render as Lilex ligatures; `|ab|` sits exactly
  above `|cd|`, so the glyphs after the ligatures stayed in their columns. The four
  private-use code points render as the Segoe icons (menu, pen, document, person).
- B: the same pairs render as separate glyphs; the four code points render as tofu boxes
  (the system fallback has no mapping for them).
- Segoe icon glyphs are wider than a Lilex cell and overflow into the following space cell;
  this is the font's shape, not a placement error (Nerd Font "Mono" variants are cell-sized).

Not walked: toggling the Settings UI switch / input while the app runs (the change path is
the same `TerminalSettings` observer the font family and weight fields already use; the
`CachedFont` and `StyleKey` unit tests cover the cache invalidation).
