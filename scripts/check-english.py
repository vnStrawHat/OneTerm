#!/usr/bin/env python3
"""Check repository comments and governance text for Vietnamese prose.

User-facing locale strings are intentionally excluded: translations are data, not
repository comments or contributor-facing documentation.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCOPES = [
    ROOT / "AGENTS.md",
    ROOT / "Cargo.toml",
    ROOT / ".github",
    ROOT / "crates",
    ROOT / "docs" / "agents",
    ROOT / "docs" / "architecture.md",
    ROOT / "scripts",
]
SUFFIXES = {".md", ".py", ".rs", ".toml", ".yml", ".yaml"}
VIETNAMESE = re.compile(
    "[\u0103\u00e2\u0111\u00ea\u00f4\u01a1\u01b0"
    "\u1ea5\u1ea7\u1ea9\u1eab\u1ead\u1eaf\u1eb1\u1eb3\u1eb5\u1eb7"
    "\u1ebf\u1ec1\u1ec3\u1ec5\u1ec7\u1ed1\u1ed3\u1ed5\u1ed7\u1ed9"
    "\u1edb\u1edd\u1edf\u1ee1\u1ee3\u1ee9\u1eeb\u1eed\u1eef\u1ef1"
    "\u1ea3\u1ea1\u1ec9\u1ecb\u1ee7\u1ee5\u1ef3\u1ef7\u1ef9\u1ef5]",
    re.IGNORECASE,
)


def files_in_scope(scope: Path) -> list[Path]:
    if scope.is_file():
        return [scope]
    return [path for path in scope.rglob("*") if path.is_file()]


def contributor_text(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    if path.suffix in {".md", ".yml", ".yaml"}:
        return text
    if path.suffix in {".py", ".toml"}:
        return "\n".join(
            line.split("#", 1)[1] for line in text.splitlines() if "#" in line
        )

    # Rust source: inspect line and block comments, not string literals such as
    # translated menu labels.
    comments: list[str] = []
    in_block = False
    for line in text.splitlines():
        current = line
        if in_block:
            comments.append(current)
            if "*/" in current:
                in_block = False
            continue
        if "/*" in current:
            before, after = current.split("/*", 1)
            comments.append(after)
            in_block = "*/" not in after
        if "//" in current:
            comments.append(current.split("//", 1)[1])
    return "\n".join(comments)


def main() -> int:
    violations: list[tuple[Path, int, str]] = []
    seen: set[Path] = set()
    for scope in SCOPES:
        for path in files_in_scope(scope):
            if path in seen or path.suffix not in SUFFIXES:
                continue
            seen.add(path)
            text = contributor_text(path)
            for line_number, line in enumerate(text.splitlines(), start=1):
                if VIETNAMESE.search(line):
                    violations.append((path.relative_to(ROOT), line_number, line.strip()))

    if violations:
        print("Non-English contributor-facing text found:")
        for path, line_number, line in violations:
            print(f"  {path}:{line_number}: {line}")
        return 1

    print(f"English contributor-text check passed for {len(seen)} files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
