#!/usr/bin/env python3
"""Validate current source paths listed by the architecture index."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ARCHITECTURE = ROOT / "docs" / "architecture.md"
PATH_PATTERN = re.compile(r"`((?:crates|docs|scripts)/[^`]+)`")


def main() -> None:
    text = ARCHITECTURE.read_text(encoding="utf-8")
    paths = sorted(set(PATH_PATTERN.findall(text)))
    missing = [path for path in paths if not (ROOT / path).exists()]
    if missing:
        for path in missing:
            print(f"error: architecture index path does not exist: {path}", file=sys.stderr)
        raise SystemExit(1)
    print(f"Architecture path check passed for {len(paths)} current paths.")


if __name__ == "__main__":
    main()
