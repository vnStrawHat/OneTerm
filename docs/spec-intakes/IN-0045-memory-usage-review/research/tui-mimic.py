# IN-0045 S6/S7 load: mimics a coding-agent TUI (the `claude` CLI) without running one.
#
#   python tui-mimic.py <seconds> [seed]                   # S6 (phase 2), unchanged
#   python tui-mimic.py --rich --minutes <N> [seed]         # S7 (phase 3)
#   python tui-mimic.py --link-repaint --minutes <N>        # phase 4 OSC 8 run
#
# --link-repaint does only this: an Ink-style repaint, 30 times a second, of a 3-line
# block whose middle line holds one OSC 8 link WITHOUT `id=` (the same URI every time),
# the way `claude` redraws a hint line when it believes the terminal supports links.
#
# It loops three phases until the time is up:
#   A  alternate screen, 30 full-screen frames/s for 10 s: cursor moves, 256-colour
#      and truecolour runs, a status line with a changing timer and spinner;
#   B  main screen, Ink-style repaint for 10 s: 30 frames/s of cursor-up + erase-line
#      over a 30-line block (how `claude` redraws), and a new permanent line every
#      5 frames, so history grows the way it does under the real CLI;
#   C  a burst of 10,000 log lines of random length (8..140 columns), some coloured.
# At the end it prints how many lines went to history and their mean length, so the
# history's used-cell fraction can be computed from the run (research doc, section C).
#
# --rich adds what the plain mode under-represents (phase 3, hypotheses H2 and H3):
#   glyphs   emoji (some with VS16), box drawing (the rounded input box), braille and
#            star spinners, Vietnamese text with diacritics, the tree and bullet marks;
#   styles   bold/dim/italic/underline/curly underline/strike mixes, 24-bit fg (a
#            theme palette, 20 % random), 24-bit bg (diff-style lines), underline colour;
#   modes    synchronized output around every frame, bracketed paste and focus
#            reporting on, a short alternate-screen excursion every 3 s in phase B,
#            a window title (OSC 0) change every second, a cursor-shape change
#            (DECSCUSR) every 2 s, a bell every 60 s, and OSC 8 hyperlinks with a
#            new URL on every 50th burst line.
import argparse, os, random, shutil, sys, time

if os.name == "nt":  # let the console host pass VT sequences through, in UTF-8
    import ctypes
    k = ctypes.windll.kernel32
    h = k.GetStdHandle(-11)
    m = ctypes.c_uint32()
    k.GetConsoleMode(h, ctypes.byref(m))
    k.SetConsoleMode(h, m.value | 0x0004)
    k.SetConsoleOutputCP(65001)
sys.stdout.reconfigure(encoding="utf-8")

ap = argparse.ArgumentParser()
ap.add_argument("seconds", nargs="?", type=float, default=60)
ap.add_argument("seed", nargs="?", type=int, default=1)
ap.add_argument("--minutes", type=float, help="duration in minutes (overrides seconds)")
ap.add_argument("--rich", action="store_true", help="phase-3 glyph, style and mode mix")
ap.add_argument("--link-repaint", action="store_true", help="phase-4 implicit OSC 8 repaint")
args = ap.parse_args()
dur = args.minutes * 60 if args.minutes is not None else args.seconds
rich = args.rich
rnd = random.Random(args.seed)
out = sys.stdout
WORDS = ("the agent reads files edits code runs tests fixes bugs plans refactors "
         "cargo build clippy warning error src lib main mod impl fn struct trait "
         "Result Option Vec String async await tokio gpui render frame cache").split()
VIET = ("Tiếng Việt người được những phải trường hợp kiểm tra lỗi sửa chữa đoạn mã "
        "chạy thử nghiệm biên dịch cảnh báo tệp thư mục hoàn thành đã xong rồi").split()
EMOJI = "✅ 🔧 📁 🚀 ⚠️ 🤖 ✨ 💡 🧪 📝 🔍 ❌ 👍 🎉 🐛 ☑️".split()
SPIN = "|/-\\"
BRAILLE = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
STARS = "·✢✳✶✻✽"
PALETTE = [(215, 119, 87), (177, 185, 249), (78, 186, 101), (255, 107, 128),
           (255, 193, 7), (153, 153, 153), (80, 80, 80), (130, 170, 255)]
GREY = "\x1b[38;2;136;136;136m"
hist_lines = hist_chars = 0


def words(n):
    if not rich:
        return " ".join(rnd.choice(WORDS) for _ in range(n))
    pick = []
    for _ in range(n):
        r = rnd.random()
        pick.append(rnd.choice(VIET) if r < 0.3 else rnd.choice(EMOJI) if r < 0.36 else rnd.choice(WORDS))
    return " ".join(pick)


def colour(s):
    if not rich:
        if rnd.random() < 0.5:
            return f"\x1b[38;5;{rnd.randrange(16, 232)}m{s}\x1b[0m"
        r, g, b = (rnd.randrange(256) for _ in range(3))
        return f"\x1b[1;38;2;{r};{g};{b}m{s}\x1b[0m"
    attrs = [a for a, p in (("1", .3), ("2", .1), ("3", .2), ("4", .1), ("4:3", .05), ("9", .03))
             if rnd.random() < p]
    r, g, b = rnd.choice(PALETTE) if rnd.random() < 0.8 else (rnd.randrange(256) for _ in range(3))
    attrs.append(f"38;2;{r};{g};{b}")
    if rnd.random() < 0.15:  # diff-style background
        attrs.append("48;2;%d;%d;%d" % rnd.choice([(34, 92, 43), (122, 41, 54), (40, 40, 40)]))
    if rnd.random() < 0.05:
        attrs.append(f"58:2::{rnd.randrange(256)}:{rnd.randrange(256)}:{rnd.randrange(256)}")
    return f"\x1b[{';'.join(attrs)}m{s}\x1b[0m"


def spinner(n):
    if not rich:
        return SPIN[n % 4]
    return BRAILLE[n % len(BRAILLE)] if n // 60 % 2 else STARS[n % len(STARS)]


def input_box(cols):
    w = max(cols - 3, 10)
    text = f"> {words(3)}"[: w - 2]
    return [f"{GREY}╭{'─' * w}╮\x1b[0m",
            f"{GREY}│\x1b[0m {text}{' ' * max(w - 1 - len(text), 0)}{GREY}│\x1b[0m",
            f"{GREY}╰{'─' * w}╯\x1b[0m"]


def sync(s):
    return f"\x1b[?2026h{s}\x1b[?2026l" if rich else s


def frame_alt(n, cols, rows, t):
    buf = ["\x1b[H"]
    for y in range(1, rows):
        line = words(rnd.randrange(2, 14))[: cols - 4]
        if rich and y % 5 == 0:
            line = ("  ⎿  " if y % 10 else "● ") + line
        buf.append(f"\x1b[{y};1H\x1b[2K{colour(line) if y % 3 else line}")
    buf.append(f"\x1b[{rows};1H\x1b[7m {spinner(n)} Working... {t:6.1f}s  frame {n} \x1b[0m")
    return sync("".join(buf))


def modes(n, t):
    """--rich: the per-frame side channels (title, cursor shape, bell)."""
    s = ""
    if n % 30 == 0:
        s += f"\x1b]0;{spinner(n)} {words(2)[:30]} ({t:.0f}s)\x07"
    if n % 60 == 0:
        s += f"\x1b[{rnd.randrange(0, 7)} q"
    if n % 1800 == 0:
        s += "\x07"
    return s


t0 = time.time()
cols = 0
if args.link_repaint:
    out.write("\n\n\n")
    n = 0
    while time.time() - t0 < dur:
        out.write(f"\x1b[3A\r\x1b[2K  Thinking... frame {n}\n"
                  "\r\x1b[2K  see \x1b]8;;https://docs.anthropic.com/en/docs/claude-code\x1b\\docs"
                  "\x1b]8;;\x1b\\ for help\n\r\x1b[2K> \n")
        out.flush()
        n += 1; time.sleep(1 / 30)
    out.write(f"\nmimic done: {n} link repaints\n")
    sys.exit(0)
if rich:
    out.write("\x1b[?2004h\x1b[?1004h")
while time.time() - t0 < dur:
    cols, rows = shutil.get_terminal_size((100, 30))
    # A: alternate-screen frames
    out.write("\x1b[?1049h\x1b[?25l")
    end, n = time.time() + 10, 0
    while time.time() < end and time.time() - t0 < dur:
        t = time.time() - t0
        out.write(frame_alt(n, cols, rows, t) + (modes(n, t) if rich else ""))
        out.flush()
        n += 1; time.sleep(1 / 30)
    out.write("\x1b[?1049l\x1b[?25h"); out.flush()
    # B: Ink-style main-screen repaint of a 30-line block
    block = min(30, rows - 2)
    out.write("\n" * block)
    end, n = time.time() + 10, 0
    while time.time() < end and time.time() - t0 < dur:
        t = time.time() - t0
        buf = [f"\x1b[{block}A"]
        box = input_box(cols) if rich else []
        for y in range(block):
            if y >= block - len(box):
                line = box[y - (block - len(box))]
            elif y == block - 1 - len(box):
                line = f"{spinner(n)} Thinking... ({t:5.1f}s, {n * 37} tokens)"[: cols - 1]
            else:
                line = ("  " + words(rnd.randrange(1, 12)))[: cols - 1]
                line = colour(line) if y % 4 == 0 else line
            buf.append("\r\x1b[2K" + line + "\n")
        if n % 5 == 0:  # a permanent line scrolls into history
            line = "  " + words(rnd.randrange(2, 16))
            buf.append(line[: cols - 1] + "\n")
            hist_lines += 1; hist_chars += min(len(line), cols - 1)
        s = sync("".join(buf))
        if rich:
            s += modes(n, t)
            if n % 90 == 45:  # a short alternate-screen excursion (a pager, a picker)
                s += "\x1b[?1049h" + frame_alt(n, cols, rows, t) + "\x1b[?1049l"
        out.write(s); out.flush()
        n += 1; time.sleep(1 / 30)
    # C: a burst of scrolling output
    buf = []
    for i in range(10_000):
        line = f"{i:5d} " + words(rnd.randrange(1, 24))
        line = line[: min(cols - 1, rnd.randrange(8, 141))]
        hist_lines += 1; hist_chars += len(line)
        if rich and i % 50 == 0:  # a new hyperlink target every time
            line = f"\x1b]8;;file:///C:/src/mod{int(time.time() * 1000)}_{i}.rs\x1b\\{line}\x1b]8;;\x1b\\"
        buf.append(colour(line) if i % 7 == 0 else line)
    out.write("\n".join(buf) + "\n"); out.flush()

if rich:
    out.write("\x1b[?2004l\x1b[?1004l\x1b[0 q")
out.write(f"\nmimic done: {hist_lines} history lines, mean {hist_chars / max(hist_lines, 1):.1f} "
          f"chars, cols {cols}\n")
