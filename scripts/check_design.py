"""The design language's ladders, checked: a distance, a radius or a text size written as a
number in a view is a step of its ladder, or carries its reason on its line or the line
before (`// design: why`). The design language and its ladders are Quadra's (its
specs/design/checklist.md, its tools/check_design.py): the sets below are kept as they are
there.

    python scripts/check_design.py          the gate's check: exits 1 when a number is off
    python scripts/check_design.py --list   every number off its ladder, and those with a reason
"""
import io
import os
import re
import sys

ROOTS = ["crates"]
DISTANCES = {0.0, 1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0}
RADII = {0.0, 1.0, 2.0, 4.0, 6.0, 10.0}
TEXT = {10.0, 12.0, 14.0, 16.0, 18.0, 23.0, 28.0}
NUMBER = re.compile(r"(?<![\w.])\d+\.\d+(?![\w.])")
# A call and its ladder: every number written in what the call is given is checked, so a
# distance chosen between two (`if small { 4.0 } else { 8.0 }`) is too, and one kept in a
# `const` of any of the files and given by its name.
CHECKS = [
    (re.compile(r"\.(?:gap|gap_x|gap_y|padding|padding_x|padding_y|padding_left|padding_right"
                r"|padding_top|padding_bottom)\("), DISTANCES, "distance"),
    (re.compile(r"\.radius\("), RADII, "radius"),
    (re.compile(r"\.text_size\("), TEXT, "text size"),
    (re.compile(r"\btext\([^()]*(?:\([^()]*\)[^()]*)*\)\.size\("), TEXT, "text size"),
]


CONST = re.compile(r"\bconst ([A-Z][A-Z0-9_]*): f32 = (\d+\.\d+);")
NAME = re.compile(r"(?<!\w)[A-Z][A-Z0-9_]*\b")


def given(code, start):
    """What a call is given: from `start` to the bracket that closes it, or the line's end."""
    depth = 1
    for at in range(start, len(code)):
        depth += {"(": 1, ")": -1}.get(code[at], 0)
        if depth == 0:
            return code[start:at]
    return code[start:]


def sources():
    for root in ROOTS:
        for folder, _, names in os.walk(root):
            parts = folder.replace("\\", "/").split("/")
            if "tests" in parts or "target" in parts:
                continue
            for name in sorted(names):
                if name.endswith(".rs"):
                    yield os.path.join(folder, name).replace("\\", "/")


def main():
    off, excused = [], []
    # A number kept under a name is read where the name is given to a call, in any file.
    named = {}
    for path in sources():
        for name, number in CONST.findall(io.open(path, encoding="utf-8").read()):
            named.setdefault(name, set()).add(number)
    for path in sources():
        lines = io.open(path, encoding="utf-8").read().split("\n")
        for at, line in enumerate(lines, 1):
            code = line.split("//", 1)[0]
            # The reason: on the line, or alone on the line before it.
            reason = "// design:" in line or lines[at - 2].strip().startswith("// design:")
            for pattern, ladder, kind in CHECKS:
                for call in pattern.finditer(code):
                    said = given(code, call.end())
                    for number in NUMBER.findall(said) + [n for name in NAME.findall(said) for n in named.get(name, ())]:
                        if float(number) in ladder:
                            continue
                        entry = f"{path}:{at}: {kind} {number}"
                        (excused if reason else off).append(entry)
    if "--list" in sys.argv:
        print("\n".join(off + [it + " (with a reason)" for it in excused]))
    print(f"design: {len(off)} off the ladders, {len(excused)} with a reason")
    if off and "--list" not in sys.argv:
        print("\n".join(off[:40]))
        sys.exit(1)


main()
