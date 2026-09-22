#!/usr/bin/env python3
"""Census of every `#[ignore]`d test in the workspace.

An ignored test runs in no gate by default, so it is the one place a check can
stop running without anything going red. Five of them are the only automated
proof of `IN-0043`'s M1 restore gate and three of its M4 guards, and they are
run explicitly by `ci-local` (see `AGENTS.md` section 4). That scoping assumes
the next elevation test lands in one of the three crates the gate names.

This makes the assumption checkable: every ignored test the current platform
collects must be recorded in `scripts/ignored-tests.txt`, so **any** new ignored
test anywhere fails the gate until someone records it and decides whether it
belongs in the elevation run. Same pattern as
`oneterm_actions::elevated_policy` — an exhaustive table where unclassified
fails the build — applied to the ignore list.

The census is the **union across platforms**, not a per-platform snapshot: some
ignored tests are `cfg`-gated to one OS (`session::session_orphan_tests::
orphan_liveness_table` spawns `cmd.exe` and is `cfg(all(test, windows))`), so a
Linux runner never collects them. The check is therefore a subset test —
recorded-but-not-collected entries are reported as informational, never as a
failure — and `--write` merges what it collects into the file instead of
overwriting it, so re-recording on one OS never drops another OS's entries.

Usage:
    python scripts/check-ignored-tests.py            # verify
    python scripts/check-ignored-tests.py --write    # re-record after a review
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CENSUS = ROOT / "scripts" / "ignored-tests.txt"

# `<name>: test`, one per line, per test binary.
TEST_LINE = re.compile(r"^(?P<name>.+): test$")


def collect() -> list[str]:
    """Every ignored test name the workspace reports, normalized and sorted."""
    result = subprocess.run(
        ["cargo", "test", "--workspace", "--", "--ignored", "--list"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout + result.stderr)
        raise SystemExit("check-ignored-tests: `cargo test --list` failed")

    names = set()
    for line in result.stdout.splitlines():
        matched = TEST_LINE.match(line.strip())
        if not matched:
            continue
        # A doc-test is reported as a source path plus a line number; the path
        # separator differs per host, so the census stores one spelling.
        names.add(matched.group("name").replace("\\", "/"))
    return sorted(names)


def read_census() -> list[str]:
    if not CENSUS.exists():
        return []
    return sorted(
        line.strip()
        for line in CENSUS.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="re-record the census after deliberately adding or removing an ignored test",
    )
    arguments = parser.parse_args()

    observed = collect()
    expected = read_census()

    if arguments.write:
        header = (
            "# Every `#[ignore]`d test in the workspace, one per line.\n"
            "#\n"
            "# An ignored test runs in no gate by default. This file is checked by\n"
            "# `scripts/check-ignored-tests.py` so a new one cannot appear unnoticed:\n"
            "# when the gate fails here, decide what the test is before recording it.\n"
            "#\n"
            "# This is the union across platforms: some entries are `cfg`-gated to one\n"
            "# OS, so a given runner collects only a subset. The check requires every\n"
            "# test it collects to be recorded, and `--write` merges rather than\n"
            "# overwrites, so recording on one OS never drops another OS's entries.\n"
            "#\n"
            "# The five `an_elevated_*` / `the_elevated_*` entries are security checks\n"
            "# (`IN-0043` M1/M4). They flip a process-global switch, so they cannot share\n"
            "# a test process — which is why they are ignored rather than ordinary — and\n"
            "# `ci-local` runs them explicitly, per crate, with `--test-threads=1`. A new\n"
            "# elevation test belongs in one of the three crates that run names, or the\n"
            "# gate must grow a fourth line.\n"
            "#\n"
            "# Re-record with: python scripts/check-ignored-tests.py --write\n"
        )
        merged = sorted(set(expected) | set(observed))
        CENSUS.write_text(header + "\n".join(merged) + "\n", encoding="utf-8")
        print(f"check-ignored-tests: recorded {len(merged)} ignored tests")
        return 0

    unrecorded = [name for name in observed if name not in expected]
    elsewhere = [name for name in expected if name not in observed]

    if elsewhere:
        print(
            f"check-ignored-tests: {len(elsewhere)} recorded elsewhere, "
            "not built on this platform:"
        )
        for name in elsewhere:
            print(f"  . {name}")

    if not unrecorded:
        print(f"check-ignored-tests: {len(observed)} ignored tests, all recorded")
        return 0

    for name in unrecorded:
        print(f"  + {name}", file=sys.stderr)
    print(
        "\ncheck-ignored-tests: the ignore list changed.\n"
        "An ignored test runs in no gate by default, so decide what this one is:\n"
        "  * a security check that flips the elevation global -> it must also be run\n"
        "    explicitly by `ci-local` (AGENTS.md section 4); if its crate is not one of\n"
        "    the three already listed there, add a fourth line.\n"
        "  * a benchmark, a measurement or a visual-review table -> nothing more to do.\n"
        "Then re-record: python scripts/check-ignored-tests.py --write",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
