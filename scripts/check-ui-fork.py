#!/usr/bin/env python3
"""Verify the reviewed baseline for OneTerm's local gpui-component UI fork."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "reference" / "gpui-component"
FORK = ROOT / "crates" / "ui" / "src"
BASELINE = ROOT / "scripts" / "ui-fork-baseline.json"
MODULES = ("dock", "resizable", "tab", "history.rs")


def digest(path: Path) -> str:
    normalized = path.read_bytes().replace(b"\r\n", b"\n")
    return hashlib.sha256(normalized).hexdigest()


def selected_files(base: Path) -> dict[str, Path]:
    files: dict[str, Path] = {}
    for module in MODULES:
        path = base / module
        if path.is_file():
            files[module] = path
        elif path.is_dir():
            for child in sorted(path.rglob("*")):
                if child.is_file():
                    files[child.relative_to(base).as_posix()] = child
    return files


def pinned_revision() -> str:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return manifest["workspace"]["dependencies"]["gpui-component"]["rev"]


def reference_revision() -> str:
    return subprocess.check_output(
        ["git", "-C", str(REFERENCE), "rev-parse", "HEAD"], text=True
    ).strip()


def update_baseline(expected_rev: str) -> int:
    if not REFERENCE.exists():
        print("error: reference/gpui-component is required to update the baseline", file=sys.stderr)
        return 1
    actual_rev = reference_revision()
    if actual_rev != expected_rev:
        print(
            f"error: reference is at {actual_rev}; checkout {expected_rev} first",
            file=sys.stderr,
        )
        return 1

    local_files = selected_files(FORK)
    upstream_files = selected_files(REFERENCE / "crates" / "ui" / "src")
    if set(local_files) != set(upstream_files):
        print("error: local and upstream selected file sets differ", file=sys.stderr)
        return 1

    baseline = {
        "upstream_revision": expected_rev,
        "files": {
            name: {
                "local_sha256": digest(local_files[name]),
                "upstream_sha256": digest(upstream_files[name]),
            }
            for name in sorted(local_files)
        },
    }
    BASELINE.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
    print(f"Updated {BASELINE.relative_to(ROOT)} for {len(local_files)} files.")
    return 0


def verify_baseline(expected_rev: str) -> int:
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    errors: list[str] = []
    if baseline["upstream_revision"] != expected_rev:
        errors.append(
            "baseline revision does not match Cargo.toml; run "
            "python scripts/check-ui-fork.py --update after reviewing upstream"
        )

    local_files = selected_files(FORK)
    baseline_files = baseline["files"]
    if set(local_files) != set(baseline_files):
        errors.append("local fork file set differs from the reviewed baseline")
    for name in sorted(set(local_files) & set(baseline_files)):
        if digest(local_files[name]) != baseline_files[name]["local_sha256"]:
            errors.append(f"unreviewed local fork change: {name}")

    if REFERENCE.exists() and reference_revision() == expected_rev:
        upstream_files = selected_files(REFERENCE / "crates" / "ui" / "src")
        for name in sorted(set(upstream_files) & set(baseline_files)):
            if digest(upstream_files[name]) != baseline_files[name]["upstream_sha256"]:
                errors.append(f"reference content differs from baseline: {name}")

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    changed = sum(
        entry["local_sha256"] != entry["upstream_sha256"]
        for entry in baseline_files.values()
    )
    print(
        f"UI fork baseline passed for {len(baseline_files)} files; "
        f"{changed} files contain reviewed OneTerm deltas."
    )
    return 0


def main() -> int:
    expected_rev = pinned_revision()
    if len(sys.argv) == 2 and sys.argv[1] == "--update":
        return update_baseline(expected_rev)
    if len(sys.argv) != 1:
        print("usage: check-ui-fork.py [--update]", file=sys.stderr)
        return 2
    return verify_baseline(expected_rev)


if __name__ == "__main__":
    raise SystemExit(main())
