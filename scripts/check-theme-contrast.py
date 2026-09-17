#!/usr/bin/env python3
"""Check that secondary text in the built-in themes clears the WCAG AA contrast floor.

Secondary text (`muted.foreground`, `tab.foreground`, `table.head.foreground`) carries the
most data-bearing values on a row -- host addresses, SFTP dates, key-binding chips,
placeholders -- so it has to be readable, not decorative. This script computes the WCAG
relative-luminance contrast ratio of each secondary foreground token against every surface
it is actually drawn on, for every variant of every theme in `crates/theme/themes/`, and
fails when one drops below 4.5:1.

Stdlib only, offline, no dependencies.

Usage:
    python scripts/check-theme-contrast.py              # fail on any variant below the floor
    python scripts/check-theme-contrast.py --report     # print the full ratio table
    python scripts/check-theme-contrast.py --self-test  # check the ratio maths itself
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
THEMES_DIR = REPO_ROOT / "crates" / "theme" / "themes"

# WCAG 2.1 AA for body text.
FLOOR = 4.5

# The surfaces each secondary foreground token is composited over in OneTerm. A token that
# sits on more than one surface must clear the floor on all of them, so the reported ratio
# is the worst of the set.
#
# `muted.foreground` -- the window body and the empty-Space placeholder (`background`),
# dialogs and the completion overlay (`popover.background`), the session tree
# (`list.background`, `sidebar.background`), the SFTP table (`table.background`), the tab
# strip subtitle (`tab_bar.background`), the title bar and the status bar.
# `tab.foreground` -- inactive tab labels, drawn on the tab strip.
# `table.head.foreground` -- SFTP / session column headers.
SURFACES: dict[str, tuple[str, ...]] = {
    "muted.foreground": (
        "background",
        "popover.background",
        "sidebar.background",
        "title_bar.background",
        "status_bar.background",
        "list.background",
        "list.even.background",
        "table.background",
        "table.even.background",
        "tab_bar.background",
    ),
    "tab.foreground": ("tab_bar.background", "tab.background"),
    "table.head.foreground": ("table.head.background",),
}

# Token fallbacks, mirroring `ThemeColor::apply_config` in
# `reference/gpui-kit/crates/component/src/theme/schema.rs`: a theme that omits a token
# inherits the one named here, so the check measures what the application actually draws.
FALLBACKS: dict[str, str] = {
    "popover.background": "background",
    "sidebar.background": "background",
    "title_bar.background": "background",
    "status_bar.background": "title_bar.background",
    "list.background": "background",
    "list.even.background": "list.background",
    "table.background": "list.background",
    "table.even.background": "list.even.background",
    "list.head.background": "list.background",
    "table.head.background": "list.head.background",
    "tab_bar.background": "background",
    "tab.background": "background",
    "tab.foreground": "foreground",
    "table.head.foreground": "muted.foreground",
}

HEX = re.compile(r"#[0-9a-fA-F]{3,8}\b")


def parse_hex(value: str) -> tuple[float, float, float, float]:
    """Return (r, g, b, a) in 0..1 from a `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` string."""
    text = value.lstrip("#")
    if len(text) in (3, 4):
        text = "".join(ch * 2 for ch in text)
    if len(text) == 6:
        text += "ff"
    if len(text) != 8:
        raise ValueError(f"not a hex color: {value!r}")
    r, g, b, a = (int(text[i : i + 2], 16) / 255 for i in (0, 2, 4, 6))
    return r, g, b, a


def luminance(rgb: tuple[float, float, float]) -> float:
    """WCAG 2.1 relative luminance."""

    def channel(c: float) -> float:
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(fg: tuple[float, float, float], bg: tuple[float, float, float]) -> float:
    a, b = luminance(fg), luminance(bg)
    lo, hi = min(a, b), max(a, b)
    return (hi + 0.05) / (lo + 0.05)


def composite(
    top: tuple[float, float, float, float], under: tuple[float, float, float]
) -> tuple[float, float, float]:
    """Source-over of a translucent colour onto an opaque one."""
    r, g, b, a = top
    return tuple(c * a + u * (1 - a) for c, u in zip((r, g, b), under))  # type: ignore[return-value]


def stops(value: str) -> list[tuple[float, float, float, float]]:
    """Every colour a token names: one for a hex, all stops for a `linear-gradient(...)`."""
    found = HEX.findall(value)
    if not found:
        raise ValueError(f"no colour in token value {value!r}")
    return [parse_hex(hit) for hit in found]


def resolve(colors: dict[str, str], token: str) -> str:
    """The raw value a variant uses for `token`, following the kit's fallback chain."""
    seen = set()
    name = token
    while name not in colors:
        if name in seen:
            raise ValueError(f"fallback cycle at {name}")
        seen.add(name)
        if name not in FALLBACKS:
            raise ValueError(f"{token}: no value and no fallback (stopped at {name})")
        name = FALLBACKS[name]
    return colors[name]


def measure(colors: dict[str, str], fg_token: str, bg_token: str) -> float:
    """Worst-case contrast of `fg_token` over `bg_token`, across gradient stops."""
    # An opaque window body is what every translucent surface ends up over.
    body = stops(resolve(colors, "background"))[0][:3]
    worst = None
    for bg in stops(resolve(colors, bg_token)):
        surface = composite(bg, body)
        for fg in stops(resolve(colors, fg_token)):
            ratio = contrast(composite(fg, surface), surface)
            worst = ratio if worst is None else min(worst, ratio)
    assert worst is not None
    return worst


def variants() -> list[tuple[str, str, str, dict[str, str]]]:
    """(file name, variant name, mode, colors) for every variant of every built-in theme."""
    out = []
    for path in sorted(THEMES_DIR.glob("*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        for theme in document.get("themes", []):
            out.append((path.name, theme["name"], theme.get("mode", "dark"), theme["colors"]))
    return out


def rows() -> list[tuple[str, str, str, str, str, float]]:
    """(file, variant, mode, fg token, worst surface, worst ratio) for every secondary token."""
    out = []
    for file_name, name, mode, colors in variants():
        for fg_token, surfaces in SURFACES.items():
            measured = [(bg, measure(colors, fg_token, bg)) for bg in surfaces]
            bg, ratio = min(measured, key=lambda item: item[1])
            out.append((file_name, name, mode, fg_token, bg, ratio))
    return out


def self_test(quiet: bool = False) -> int:
    """The ratio maths, against values whose contrast is known independently.

    Cheap enough that `main` runs it on every invocation: a check whose arithmetic is wrong
    passes every theme silently, which is worse than no check at all.
    """
    white, black = (1.0, 1.0, 1.0), (0.0, 0.0, 0.0)
    assert abs(contrast(white, black) - 21.0) < 1e-6, "black on white is 21:1"
    assert abs(contrast(white, white) - 1.0) < 1e-6, "a colour on itself is 1:1"
    # #767676 on white is the canonical "just clears AA" grey.
    grey = parse_hex("#767676")[:3]
    assert 4.5 <= contrast(grey, white) < 4.6, "#767676 on #ffffff clears AA by a hair"
    assert contrast(parse_hex("#777777")[:3], white) < 4.5, "one step lighter no longer does"
    # The finding this check exists for: #5c6370 on #23272e measured 2.48:1.
    assert abs(contrast(parse_hex("#5c6370")[:3], parse_hex("#23272e")[:3]) - 2.48) < 0.01
    # Pairing a foreground with the wrong surface passes text that is unreadable: the same
    # grey over white looks fine, over the panel it does not.
    assert contrast(parse_hex("#5c6370")[:3], white) > FLOOR
    # Alpha is composited, not ignored: a 60% panel over the body sits between the two.
    blended = composite(parse_hex("#1c1b1a99"), parse_hex("#100f0f")[:3])
    assert luminance(parse_hex("#100f0f")[:3]) < luminance(blended) < luminance(
        parse_hex("#1c1b1a")[:3]
    )
    # A gradient is measured at its worst stop, not its first.
    colors = {"background": "#ffffff", "muted.foreground": "#767676",
              "title_bar.background": "linear-gradient(180deg, #ffffff, #cccccc)"}
    assert measure(colors, "muted.foreground", "title_bar.background") < 4.5
    if not quiet:
        print("check-theme-contrast: self-test passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", action="store_true", help="print every variant, not just failures")
    parser.add_argument("--self-test", action="store_true", help="check the ratio maths and exit")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    self_test(quiet=True)
    measured = rows()
    failures = [row for row in measured if row[5] < FLOOR]

    if args.report:
        width = max(len(row[1]) for row in measured)
        for file_name, name, mode, fg_token, bg_token, ratio in measured:
            mark = "FAIL" if ratio < FLOOR else "ok  "
            print(f"{mark} {name:<{width}} {mode:<5} {fg_token:<22} on {bg_token:<24} {ratio:5.2f}:1")
        print(f"\n{len(measured)} measurements, {len(failures)} below {FLOOR}:1")

    if failures:
        if not args.report:
            for file_name, name, mode, fg_token, bg_token, ratio in failures:
                print(f"{file_name}: {name} ({mode}): {fg_token} on {bg_token} is {ratio:.2f}:1")
        print(
            f"check-theme-contrast: {len(failures)} secondary token(s) below the {FLOOR}:1 floor",
            file=sys.stderr,
        )
        return 1

    print(f"check-theme-contrast: {len(measured)} measurements, all >= {FLOOR}:1")
    return 0


if __name__ == "__main__":
    sys.exit(main())
