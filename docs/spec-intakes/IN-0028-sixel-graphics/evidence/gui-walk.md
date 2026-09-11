# GUI walk: Sixel in a local cmd shell (2026-09-11, Windows 11, fast-dev build)

`make_sixel.py` (in this folder) writes `card.six`: the text `before`, a 96 x 48 Sixel test
card (raster attributes, five colour registers, one data byte per column, `$` and `-`),
then `after`. It was printed with cmd's `type` in a local shell, so the bytes crossed
ConPTY (the bundled OpenConsole passes DCS through) before reaching the vendored `Term`.

| Step | Crop | Observed |
| --- | --- | --- |
| `type card.six` | `US-0067-sixel-at-cursor.png` | Four colour bands with the white diagonal drawn at the cursor, 96 x 48 px; `after` printed on the line below the image (3 rows at ~18 px line height, the third only partly covered). |
| `for /l %i in (1,1,70) do @echo line %i`, then 8 wheel notches up | `US-0067-sixel-scrollback-clipped.png` | The image scrolled into history with its text; scrolling back shows its bottom band at the top of the grid with the rest cut at the grid edge (clip = grid bounds ∩ image bounds). |
| `cls` | `US-0067-cls-removes-image.png` | The image is gone with its cells. |

## Rework walk (2026-09-11, libsixel `snake.six`, 600 x 450)

| Step | Crop | Observed |
| --- | --- | --- |
| `cat snake.six` (Git's `cat.exe`), first build | `US-0067-rework-conpty-cat-byte-loss.png` | garbled bands: ConPTY drops one byte per 32 KiB write inside the DCS (parser-free capture: 7 bytes missing). `type` shows the image intact. |
| `type snake.six`, `echo hi after image`, reworked build | `US-0067-rework-prompt-below-image.png` | image drawn as 60 x 23 cells (virtual 10 x 20 cell), prompt and echo below it; before the rework the echo vanished inside the image because conhost re-synced the cursor to its own row. |

Not walked: an image wider than the grid; an image taller than the screen; a real Sixel
producer (`img2sixel`, `chafa`) on this machine; SSH sessions (same engine path, no host
available); HiDPI (image pixels map 1:1 to logical pixels, so a 2x display upscales them).
