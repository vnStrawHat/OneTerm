"""Differing-pixel clusters (20x10 cells) between the before/after captures of one tag."""
import sys
from PIL import Image

tag = sys.argv[1]
d = sys.argv[2] if len(sys.argv) > 2 else "pix"
a = Image.open(f"{d}/{tag}-before.png").convert("RGBA")
b = Image.open(f"{d}/{tag}-after.png").convert("RGBA")
pa, pb = a.load(), b.load()
w, h = a.size
cells = {}
for y in range(h):
    for x in range(w):
        if pa[x, y] != pb[x, y]:
            k = (x // 20 * 20, y // 10 * 10)
            cells[k] = cells.get(k, 0) + 1
total = sum(cells.values())
print(f"{tag}: {total} differing pixels; title+tab bar rows (y<80): "
      f"{sum(v for (x, y), v in cells.items() if y < 80)}")
for (x, y), v in sorted(cells.items(), key=lambda kv: (kv[0][1], kv[0][0])):
    print(f"  cell x={x}-{x + 19} y={y}-{y + 9}: {v}")
