#!/usr/bin/env python3
"""Validate that the current-state docs only mention repository paths that exist.

Checked documents (the "current" navigation set — historical records under
``docs/archive/`` and the review checklists are deliberately not checked):

* ``docs/architecture.md`` — the architecture index (its only job is current paths),
* ``docs/agents/*.md`` — the agent guides (structure tree, dependency rules, …),
* ``docs/README.md`` — the documentation index,
* ``docs/terminal-backend.md`` — the backend design, whose "File layout (current)"
  section is a tree of paths and nothing else,
* ``README.md`` and ``AGENTS.md`` at the repository root.

A "path" is any token that starts with ``crates/``, ``docs/`` or ``scripts/``,
back-ticked **or bare**: bare so that a path inside an ASCII tree diagram or a
fenced code block is checked too, which is where the rot ``US-0090`` fixed had
been hiding. Placeholders (``<name>``, ``*``, ``{a,b}``, ``…``) are skipped.

Known blind spot: a tree diagram lists its leaves as bare file names under a
directory line, so a deleted *leaf* (``engine_shim.rs``) is still invisible.
Reconstructing paths from box-drawing indentation costs more than it catches.

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
    ROOT / "docs" / "terminal-backend.md",
    ROOT / "README.md",
    ROOT / "AGENTS.md",
    *sorted((ROOT / "docs" / "agents").glob("*.md")),
]
# A repository path, back-ticked or bare. The look-behind keeps the match from
# starting mid-token, so `reference/gpui-kit/crates/component/src/` (the pinned
# upstream clone, deliberately unchecked) is not mistaken for `crates/…`.
PATH_PATTERN = re.compile(r"(?<![\w/.-])((?:crates|docs|scripts)/[^`\s)\]\"']+)")
# Tokens that are templates / globs rather than concrete paths.
PLACEHOLDER_CHARS = ("<", ">", "*", "{", "}", "…", " ", "|")
# Prose and markup punctuation that can trail a bare path.
TRAILING = ".,;:!?`\"'"
# A citation suffix on a path: `…rs:120`, `…rs:120-160`, `…rs::test_name`.
CITATION = re.compile(r"(::|:\d+(-\d+)?$).*$")


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
            candidate = CITATION.sub("", path.rstrip(TRAILING)).rstrip(TRAILING)
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
