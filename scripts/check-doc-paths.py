#!/usr/bin/env python3
"""Validate that the current-state docs only mention repository paths that exist.

Checked documents (the "current" navigation set — historical records under
``docs/archive/`` and the review checklists are deliberately not checked):

* ``docs/architecture.md`` — the architecture index (its only job is current paths),
* ``docs/agents/*.md`` — the agent guides (structure tree, dependency rules, …),
* ``docs/README.md`` — the documentation index,
* ``README.md`` and ``AGENTS.md`` at the repository root.

A "path" is any back-ticked token that starts with ``crates/``, ``docs/``,
``scripts/`` or ``vendor/``. Placeholders (``<name>``, ``*``, ``{a,b}``, ``…``)
are skipped.

``reference/`` is deliberately *not* checked: it holds gitignored local clones of
upstream projects (see ``docs/agents/dependencies.md``), so those paths are absent
in a fresh checkout and in CI. Checking them made the script fail by default
everywhere the clone was missing.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCUMENTS = [
    ROOT / "docs" / "architecture.md",
    ROOT / "docs" / "README.md",
    ROOT / "README.md",
    ROOT / "AGENTS.md",
    *sorted((ROOT / "docs" / "agents").glob("*.md")),
]
PATH_PATTERN = re.compile(r"`((?:crates|docs|scripts|vendor)/[^`]+)`")
# Tokens that are templates / globs rather than concrete paths.
PLACEHOLDER_CHARS = ("<", ">", "*", "{", "}", "…", " ", "|")


def is_checkable(path: str) -> bool:
    if any(char in path for char in PLACEHOLDER_CHARS):
        return False
    return True


def main() -> None:
    checked = 0
    missing: list[tuple[str, str]] = []
    for document in DOCUMENTS:
        text = document.read_text(encoding="utf-8")
        for path in sorted(set(PATH_PATTERN.findall(text))):
            # Trailing punctuation from prose ("`docs/foo.md`." is unusual but cheap to tolerate).
            candidate = path.rstrip(".,;:")
            if not is_checkable(candidate):
                continue
            checked += 1
            if not (ROOT / candidate).exists():
                missing.append((document.relative_to(ROOT).as_posix(), candidate))
    if missing:
        for document, path in missing:
            print(f"error: {document}: path does not exist: {path}", file=sys.stderr)
        raise SystemExit(1)
    print(f"Doc path check passed for {checked} current paths in {len(DOCUMENTS)} documents.")


if __name__ == "__main__":
    main()
