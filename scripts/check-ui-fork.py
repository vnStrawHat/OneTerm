#!/usr/bin/env python3
"""Verify the reviewed baseline for OneTerm's vendored gpui-component patch."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORK = ROOT / "vendor" / "gpui-component" / "src"
PATCH_FILE = ROOT / "vendor" / "patches" / "gpui-component" / "0001-OneTerm-add-TabPanel-set_active_panel.patch"
BASELINE = ROOT / "scripts" / "ui-fork-baseline.json"
UPSTREAM_URL = "https://github.com/longbridge/gpui-component"
PATCH_MODULES = ("dock/tab_panel.rs",)


def digest(path: Path) -> str:
    normalized = path.read_bytes().replace(b"\r\n", b"\n")
    return hashlib.sha256(normalized).hexdigest()


def source_files(base: Path) -> dict[str, Path]:
    return {
        path.relative_to(base).as_posix(): path
        for path in sorted(base.rglob("*"))
        if path.is_file()
    }


def is_patch_file(name: str) -> bool:
    return any(name == module or name.startswith(f"{module}/") for module in PATCH_MODULES)


def pinned_revision() -> str:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return manifest["workspace"]["dependencies"]["gpui-component"]["rev"]


def clone_upstream(destination: Path, revision: str) -> Path:
    subprocess.run(
        ["git", "clone", "--filter=blob:none", "--no-checkout", UPSTREAM_URL, str(destination)],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(destination), "checkout", "--detach", revision],
        check=True,
    )
    actual = subprocess.check_output(
        ["git", "-C", str(destination), "rev-parse", "HEAD"], text=True
    ).strip()
    if actual != revision:
        raise RuntimeError(f"upstream checkout is at {actual}, expected {revision}")
    return destination / "crates" / "ui" / "src"


def update_baseline(expected_rev: str) -> int:
    if not PATCH_FILE.exists():
        print(f"error: missing source patch: {PATCH_FILE.relative_to(ROOT)}", file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory(prefix="oneterm-gpui-component-") as temporary:
        upstream_root = clone_upstream(Path(temporary) / "upstream", expected_rev)
        local_files = source_files(FORK)
        upstream_files = source_files(upstream_root)

        if set(local_files) != set(upstream_files):
            print("error: vendor and upstream source file sets differ", file=sys.stderr)
            return 1

        files = {
            name: {
                "local_sha256": digest(local_files[name]),
                "upstream_sha256": digest(upstream_files[name]),
                "patch_file": is_patch_file(name),
            }
            for name in sorted(local_files)
        }

    unexpected = [
        name
        for name, entry in files.items()
        if not entry["patch_file"] and entry["local_sha256"] != entry["upstream_sha256"]
    ]
    if unexpected:
        for name in unexpected:
            print(f"error: change outside the approved patch surface: {name}", file=sys.stderr)
        return 1

    baseline = {
        "upstream_repository": UPSTREAM_URL,
        "upstream_revision": expected_rev,
        "patch_file": PATCH_FILE.relative_to(ROOT).as_posix(),
        "patch_sha256": digest(PATCH_FILE),
        "patch_modules": list(PATCH_MODULES),
        "files": files,
    }
    BASELINE.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
    print(f"Updated {BASELINE.relative_to(ROOT)} for {len(files)} source files.")
    return 0


def verify_baseline(expected_rev: str) -> int:
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    errors: list[str] = []
    if not PATCH_FILE.exists():
        errors.append(f"missing source patch: {PATCH_FILE.relative_to(ROOT)}")
    if baseline.get("patch_file") != PATCH_FILE.relative_to(ROOT).as_posix():
        errors.append("baseline patch file path is incorrect")
    elif PATCH_FILE.exists() and digest(PATCH_FILE) != baseline.get("patch_sha256"):
        errors.append("source patch differs from the reviewed baseline")
    if baseline.get("upstream_repository") != UPSTREAM_URL:
        errors.append("baseline upstream repository is incorrect")
    if baseline["upstream_revision"] != expected_rev:
        errors.append(
            "baseline revision does not match Cargo.toml; run "
            "python scripts/check-ui-fork.py --update after reviewing upstream"
        )
    if tuple(baseline.get("patch_modules", ())) != PATCH_MODULES:
        errors.append("baseline patch module list is incorrect")

    local_files = source_files(FORK)
    baseline_files = baseline["files"]
    if set(local_files) != set(baseline_files):
        errors.append("vendored source file set differs from the reviewed baseline")
    for name in sorted(set(local_files) & set(baseline_files)):
        entry = baseline_files[name]
        if digest(local_files[name]) != entry["local_sha256"]:
            errors.append(f"unreviewed vendored source change: {name}")
        if entry.get("patch_file") != is_patch_file(name):
            errors.append(f"incorrect patch classification: {name}")
        if not is_patch_file(name) and entry["local_sha256"] != entry["upstream_sha256"]:
            errors.append(f"unapproved delta outside patch surface: {name}")

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    changed = sum(
        entry["local_sha256"] != entry["upstream_sha256"]
        for entry in baseline_files.values()
    )
    delta_label = "file contains" if changed == 1 else "files contain"
    print(
        f"UI vendor baseline passed for {len(baseline_files)} source files; "
        f"{changed} {delta_label} reviewed OneTerm deltas."
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
