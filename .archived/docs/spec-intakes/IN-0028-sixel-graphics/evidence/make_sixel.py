"""Write a 96x48 Sixel test card: four colour bands with a white diagonal, raster
attributes set, one column per data byte, so the terminal needs the full
grammar (raster, colour registers, repeat, $, -)."""
import sys

W, H = 96, 48
out = sys.argv[1]
cols = [(100, 0, 0), (0, 100, 0), (0, 0, 100), (100, 100, 0)]
pixels = [[0] * W for _ in range(H)]  # colour register per pixel (0 = band, 4 = white)
for y in range(H):
    for x in range(W):
        pixels[y][x] = x * 4 // W
        if abs(x - y * 2) < 2:
            pixels[y][x] = 4
s = "\x1bPq"
s += f'"1;1;{W};{H}'
for i, (r, g, b) in enumerate(cols):
    s += f"#{i};2;{r};{g};{b}"
s += "#4;2;100;100;100"
for band in range(H // 6):
    for reg in range(5):
        s += f"#{reg}"
        for x in range(W):
            bits = 0
            for bit in range(6):
                if pixels[band * 6 + bit][x] == reg:
                    bits |= 1 << bit
            s += chr(0x3F + bits)
        s += "$"
    s += "-"
s += "\x1b\\"
with open(out, "wb") as f:
    f.write(b"before\r\n" + s.encode() + b"after\r\n")
print(len(s), "bytes")
