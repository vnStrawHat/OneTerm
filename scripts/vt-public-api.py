#!/usr/bin/env python3
"""Print (or check) `oneterm-vt`'s public API surface, one path per line.

Other projects consume `oneterm-vt` as a git dependency, so its public surface is an
external contract. This script enumerates that surface from the rustdoc output, and
CI diffs the result against the committed snapshot: any change to what the crate
exposes then has to be an intentional line in a review.

    python scripts/vt-public-api.py                    # print the host's surface
    python scripts/vt-public-api.py --update           # regenerate the host's file
    python scripts/vt-public-api.py --check            # fail if the host's file is stale
    python scripts/vt-public-api.py --diff-platforms   # what the two files disagree on

**There are two snapshots, one per platform family** (`US-0104`): the `pty` module
publishes `PipeReader`, `PipeWriter` and `Options::escape_args` on Windows, and
`SignalMask` and `Options::child_signal_mask` on Unix, so one file cannot describe
both. `cargo doc` renders only the host's half, so the script reads, writes and
checks `public-api.windows.txt` on Windows and `public-api.unix.txt` everywhere
else, and says which one it used. `--diff-platforms` needs no rustdoc: it reports
every line the two files disagree on and **fails if any of them is outside
`oneterm_vt::pty`**, which is the invariant -- the two files may differ only by the
cfg-gated transport items.

A snapshot may open with `#` comment lines. They are a note to the reader (a file
derived by hand rather than generated on its own platform says so there) and are
ignored by every comparison.

The surface is read from the HTML rustdoc emits, not from `--output-format json`,
which is nightly-only and this repository pins a stable toolchain. That costs
signature-level detail: a method whose arguments change is invisible here, while an
item, field or variant that is added, removed or renamed is not.

Only paths an embedder can actually write are listed. rustdoc also emits a page
for an item at its defining path inside a private or `pub(crate)` module, and
those are not API: renaming such a module is an internal refactor, and a gate
that fires on one teaches people to regenerate without reading.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOC_ROOT = ROOT / "target" / "doc" / "oneterm_vt"
SURFACE_WINDOWS = ROOT / "crates" / "vt" / "public-api.windows.txt"
SURFACE_UNIX = ROOT / "crates" / "vt" / "public-api.unix.txt"
# The only module whose items are allowed to differ between the two snapshots.
PLATFORM_MODULE = "oneterm_vt::pty"
# A module's own index page links the modules **it** makes public, and only
# those, so walking the links from the crate root reaches every module path an
# embedder can write and no private one, however deeply nested.
MOD_LINK = re.compile(r'<a class="mod" href="([A-Za-z0-9_]+)/index\.html"')

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


def public_modules() -> set[str]:
    """Every module path an embedder can write, walked from the crate root."""
    modules = {"."}
    queue = [""]
    while queue:
        prefix = queue.pop()
        index = (DOC_ROOT / prefix / "index.html") if prefix else (DOC_ROOT / "index.html")
        if not index.is_file():
            continue
        for name in MOD_LINK.findall(index.read_text(encoding="utf-8", errors="replace")):
            path = f"{prefix}/{name}" if prefix else name
            if path not in modules:
                modules.add(path)
                queue.append(path)
    return modules


def surface() -> list[str]:
    modules = public_modules()
    lines: list[str] = []
    for page in DOC_ROOT.rglob("*.html"):
        name = ITEM.match(page.name)
        if not name:
            continue
        module = page.parent.relative_to(DOC_ROOT).as_posix()
        if module not in modules:
            continue
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


def host_surface_file() -> Path:
    """The snapshot this host's `cargo doc` can actually produce."""
    return SURFACE_WINDOWS if sys.platform.startswith("win") else SURFACE_UNIX


def body(path: Path) -> str:
    """A snapshot's content with its leading `#` note, if any, removed."""
    if not path.exists():
        return ""
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    while lines and lines[0].startswith("#"):
        lines.pop(0)
    return "".join(lines)


def diff_platforms() -> int:
    """Report what the two snapshots disagree on; fail on anything but `pty`."""
    windows = body(SURFACE_WINDOWS).splitlines()
    unix = body(SURFACE_UNIX).splitlines()
    # A member line ("    method spawn") belongs to the item above it, so carry
    # the owning path down; otherwise an added field reads as a bare `structfield`.
    def owned(lines: list[str]) -> list[str]:
        out, owner = [], ""
        for line in lines:
            if line.startswith("    "):
                out.append(f"{owner}{line}")
            else:
                owner = line.split(" ", 1)[1] if " " in line else line
                out.append(line)
        return out

    windows, unix = owned(windows), owned(unix)
    only_windows = [line for line in windows if line not in unix]
    only_unix = [line for line in unix if line not in windows]
    for line in only_windows:
        print(f"windows only: {line}")
    for line in only_unix:
        print(f"unix only:    {line}")
    stray = [line for line in only_windows + only_unix if PLATFORM_MODULE not in line]
    if stray:
        print(
            f"\nthe two snapshots may differ only inside `{PLATFORM_MODULE}`; "
            f"these are outside it:",
            file=sys.stderr,
        )
        for line in stray:
            print(f"  {line}", file=sys.stderr)
        return 1
    if not only_windows and not only_unix:
        print("the two snapshots are identical")
    else:
        print(f"\nthe delta is {len(only_windows) + len(only_unix)} lines, "
              f"all inside `{PLATFORM_MODULE}`")
    return 0


def build_docs() -> None:
    subprocess.run(
        ["cargo", "doc", "-p", "oneterm-vt", "--no-deps", "--all-features"],
        cwd=ROOT,
        check=True,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if the file is stale")
    parser.add_argument(
        "--update", "--write", dest="update", action="store_true",
        help="regenerate this host's file",
    )
    parser.add_argument("--no-doc", action="store_true", help="reuse the existing target/doc")
    parser.add_argument(
        "--diff-platforms", action="store_true",
        help="report what the two snapshots disagree on (no rustdoc needed)",
    )
    args = parser.parse_args()

    if args.diff_platforms:
        return diff_platforms()

    surface_file = host_surface_file()
    if not args.no_doc:
        build_docs()
    if not DOC_ROOT.is_dir():
        print(f"no rustdoc output at {DOC_ROOT}", file=sys.stderr)
        return 1

    text = "\n".join(surface()) + "\n"
    if args.update:
        # A regenerated file is no longer derived, so any note the old one
        # carried goes with it.
        surface_file.write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {surface_file.relative_to(ROOT)}")
        return 0
    if args.check:
        current = body(surface_file)
        if current == text:
            print(f"public API surface unchanged ({surface_file.name})")
            return 0
        print(
            f"oneterm-vt's public API surface changed ({surface_file.name}).\n"
            "If that was intended, run `python scripts/vt-public-api.py --update`,\n"
            "commit the diff, and add a CHANGELOG entry naming the item.",
            file=sys.stderr,
        )
        import difflib

        sys.stderr.writelines(
            difflib.unified_diff(
                current.splitlines(keepends=True),
                text.splitlines(keepends=True),
                f"committed ({surface_file.name})",
                "generated",
            )
        )
        return 1
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
