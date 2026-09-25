# IN-0045 S6 load: mimics a coding-agent TUI (the `claude` CLI) without running one.
#
#   python tui-mimic.py <seconds> [seed]
#
# It loops three phases until <seconds> have passed:
#   A  alternate screen, 30 full-screen frames/s for 10 s: cursor moves, 256-colour
#      and truecolour runs, a status line with a changing timer and spinner;
#   B  main screen, Ink-style repaint for 10 s: 30 frames/s of cursor-up + erase-line
#      over a 30-line block (how `claude` redraws), and a new permanent line every
#      5 frames, so history grows the way it does under the real CLI;
#   C  a burst of 10,000 log lines of random length (8..140 columns), some coloured.
# At the end it prints how many lines went to history and their mean length, so the
# history's used-cell fraction can be computed from the run (research doc, section C).
import os, random, shutil, sys, time

if os.name == "nt":  # let the console host pass VT sequences through
    import ctypes
    k = ctypes.windll.kernel32
    h = k.GetStdHandle(-11)
    m = ctypes.c_uint32()
    k.GetConsoleMode(h, ctypes.byref(m))
    k.SetConsoleMode(h, m.value | 0x0004)

dur = float(sys.argv[1]) if len(sys.argv) > 1 else 60
rnd = random.Random(int(sys.argv[2]) if len(sys.argv) > 2 else 1)
out = sys.stdout
WORDS = ("the agent reads files edits code runs tests fixes bugs plans refactors "
         "cargo build clippy warning error src lib main mod impl fn struct trait "
         "Result Option Vec String async await tokio gpui render frame cache").split()
SPIN = "|/-\\"
hist_lines = hist_chars = 0


def words(n):
    return " ".join(rnd.choice(WORDS) for _ in range(n))


def colour(s):
    if rnd.random() < 0.5:
        return f"\x1b[38;5;{rnd.randrange(16, 232)}m{s}\x1b[0m"
    r, g, b = (rnd.randrange(256) for _ in range(3))
    return f"\x1b[1;38;2;{r};{g};{b}m{s}\x1b[0m"


def frame_alt(n, cols, rows, t):
    buf = ["\x1b[H"]
    for y in range(1, rows):
        line = words(rnd.randrange(2, 14))[: cols - 4]
        buf.append(f"\x1b[{y};1H\x1b[2K{colour(line) if y % 3 else line}")
    buf.append(f"\x1b[{rows};1H\x1b[7m {SPIN[n % 4]} Working... {t:6.1f}s  frame {n} \x1b[0m")
    return "".join(buf)


t0 = time.time()
while time.time() - t0 < dur:
    cols, rows = shutil.get_terminal_size((100, 30))
    # A: alternate-screen frames
    out.write("\x1b[?1049h\x1b[?25l")
    end, n = time.time() + 10, 0
    while time.time() < end and time.time() - t0 < dur:
        out.write(frame_alt(n, cols, rows, time.time() - t0)); out.flush()
        n += 1; time.sleep(1 / 30)
    out.write("\x1b[?1049l\x1b[?25h"); out.flush()
    # B: Ink-style main-screen repaint of a 30-line block
    block = min(30, rows - 2)
    out.write("\n" * block)
    end, n = time.time() + 10, 0
    while time.time() < end and time.time() - t0 < dur:
        buf = [f"\x1b[{block}A"]
        for y in range(block):
            line = (f"{SPIN[n % 4]} Thinking... ({time.time() - t0:5.1f}s, {n * 37} tokens)"
                    if y == block - 1 else "  " + words(rnd.randrange(1, 12)))[: cols - 1]
            buf.append("\r\x1b[2K" + (colour(line) if y % 4 == 0 else line) + "\n")
        if n % 5 == 0:  # a permanent line scrolls into history
            line = "  " + words(rnd.randrange(2, 16))
            buf.append(line[: cols - 1] + "\n")
            hist_lines += 1; hist_chars += min(len(line), cols - 1)
        out.write("".join(buf)); out.flush()
        n += 1; time.sleep(1 / 30)
    # C: a burst of scrolling output
    buf = []
    for i in range(10_000):
        line = f"{i:5d} " + words(rnd.randrange(1, 24))
        line = line[: min(cols - 1, rnd.randrange(8, 141))]
        buf.append(colour(line) if i % 7 == 0 else line)
        hist_lines += 1; hist_chars += len(line)
    out.write("\n".join(buf) + "\n"); out.flush()

out.write(f"\nmimic done: {hist_lines} history lines, mean {hist_chars / max(hist_lines, 1):.1f} "
          f"chars, cols {cols}\n")
