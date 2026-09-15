#!/usr/bin/env python3
"""Print (or check) `oneterm-vt`'s public API surface, one path per line.

`oneterm-vt` is the workspace's only published crate, so its public surface is an
external contract. This script enumerates that surface from the rustdoc output, and
CI diffs the result against the committed `crates/vt/public-api.txt`: any change to
what the crate exposes then has to be an intentional line in a review.

    python scripts/vt-public-api.py            # print the surface
    python scripts/vt-public-api.py --write    # regenerate crates/vt/public-api.txt
    python scripts/vt-public-api.py --check    # fail if the file is out of date

The surface is read from the HTML rustdoc emits, not from `--output-format json`,
which is nightly-only and this repository pins a stable toolchain. That costs
signature-level detail: a method whose arguments change is invisible here, while an
item, field or variant that is added, removed or renamed is not.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOC_ROOT = ROOT / "target" / "doc" / "oneterm_vt"
SURFACE = ROOT / "crates" / "vt" / "public-api.txt"

# One page per public item: `struct.Terminal.html`, `enum.VtEvent.html`, ...
ITEM = re.compile(r"^(struct|enum|trait|fn|constant|type|union|macro)\.(.+)\.html$")
# Fields and variants can only be inherent; methods must be filtered (see below).
MEMBER = re.compile(r'id="(structfield|variant|associatedconstant|method)\.([A-Za-z0-9_]+)"')
# Everything from here down the page belongs to a trait impl, not to the item.
TRAIT_IMPLS = re.compile(r'id="(trait-implementations|synthetic-implementations|blanket)')


def members(page: Path) -> list[str]:
    html = page.read_text(encoding="utf-8", errors="replace")
    # ponytail: rustdoc emits inherent impls before trait impls, so cutting the page
    # at the first trait-impl heading is enough to drop `clone`, `fmt` and friends.
    # If rustdoc reorders its sections this over-reports rather than under-reports.
    cut = TRAIT_IMPLS.search(html)
    if cut:
        html = html[: cut.start()]
    return sorted({f"{kind} {name}" for kind, name in MEMBER.findall(html)})


def surface() -> list[str]:
    lines: list[str] = []
    for page in DOC_ROOT.rglob("*.html"):
        name = ITEM.match(page.name)
        if not name:
            continue
        module = page.parent.relative_to(DOC_ROOT).as_posix()
        prefix = "oneterm_vt" + ("" if module == "." else "::" + module.replace("/", "::"))
        kind, item = name.groups()
        path = f"{prefix}::{item}"
        lines.append(f"{kind} {path}")
        lines.extend(f"    {member}" for member in members(page))
    return sorted_blocks(lines)


def sorted_blocks(lines: list[str]) -> list[str]:
    """Sort items by path, keeping each item's members under it."""
    blocks: list[list[str]] = []
    for line in lines:
        if line.startswith("    "):
            blocks[-1].append(line)
        else:
            blocks.append([line])
    blocks.sort(key=lambda block: block[0].split(" ", 1)[1])
    return [line for block in blocks for line in block]


def build_docs() -> None:
    subprocess.run(
        ["cargo", "doc", "-p", "oneterm-vt", "--no-deps", "--all-features"],
        cwd=ROOT,
        check=True,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if the file is stale")
    parser.add_argument("--write", action="store_true", help="regenerate the file")
    parser.add_argument("--no-doc", action="store_true", help="reuse the existing target/doc")
    args = parser.parse_args()

    if not args.no_doc:
        build_docs()
    if not DOC_ROOT.is_dir():
        print(f"no rustdoc output at {DOC_ROOT}", file=sys.stderr)
        return 1

    text = "\n".join(surface()) + "\n"
    if args.write:
        SURFACE.write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {SURFACE.relative_to(ROOT)}")
        return 0
    if args.check:
        current = SURFACE.read_text(encoding="utf-8") if SURFACE.exists() else ""
        if current == text:
            print("public API surface unchanged")
            return 0
        print(
            "oneterm-vt's public API surface changed.\n"
            "If that was intended, run `python scripts/vt-public-api.py --write`,\n"
            "commit the diff, and add a CHANGELOG entry naming the item.",
            file=sys.stderr,
        )
        import difflib

        sys.stderr.writelines(
            difflib.unified_diff(
                current.splitlines(keepends=True),
                text.splitlines(keepends=True),
                "committed",
                "generated",
            )
        )
        return 1
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
