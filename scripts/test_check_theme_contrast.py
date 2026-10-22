"""Pinning tests for `check-theme-contrast.py` (advisor review of `US-0198` rework 6).

Each test names the defect it pins; flip its expectation (or delete it) when the script is fixed.
Run with `python -m unittest scripts/test_check_theme_contrast.py`.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import sys
import unittest
from pathlib import Path

CHECKER_PATH = Path(__file__).with_name("check-theme-contrast.py")
SPEC = importlib.util.spec_from_file_location("check_theme_contrast", CHECKER_PATH)
assert SPEC is not None and SPEC.loader is not None
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


def run_main() -> tuple[int, str]:
    """`main()` with no flags; its exit code and everything it printed."""
    out = io.StringIO()
    argv = sys.argv
    sys.argv = [str(CHECKER_PATH)]
    try:
        with contextlib.redirect_stdout(out):
            code = checker.main()
    finally:
        sys.argv = argv
    return code, out.getvalue()


class KnownBelowTests(unittest.TestCase):
    """`KNOWN_BELOW` exempts (variant, fill) pairs that read below the glyph floor."""

    def setUp(self) -> None:
        self.saved = checker.KNOWN_BELOW

    def tearDown(self) -> None:
        checker.KNOWN_BELOW = self.saved

    def test_a_known_pair_that_now_reads_is_a_stale_entry_and_fails_the_run(self) -> None:
        """DEFECT (rework 6, F1): `main()` only looks up `KNOWN_BELOW` for rows that are below
        the floor, so an entry whose pair has since been retuned (or was never below) is kept
        for ever and reported nowhere. A stale exemption must fail the run, the way a stale
        `#[ignore]` census entry fails `check-ignored-tests.py`."""
        passing = [
            (row[1], row[4])
            for row in checker.rows()
            if row[3] == checker.PICKED_GLYPH and row[5] >= row[6]
        ]
        self.assertTrue(passing, "at least one variant clears the glyph floor on some fill")
        checker.KNOWN_BELOW = frozenset({*self.saved, passing[0]})
        code, output = run_main()
        self.assertEqual(code, 1, f"a stale KNOWN_BELOW entry passed silently:\n{output}")
        self.assertIn(passing[0][0], output)

    def test_the_summary_does_not_claim_every_glyph_clears_the_floor_while_known_rows_are_below(
        self,
    ) -> None:
        """DEFECT (rework 6, F2): with five KNOWN rows below 3:1 the last line still reads
        `all >= 4.5:1 (glyphs >= 3.0:1)`. The summary must say how many rows are exempt."""
        code, output = run_main()
        self.assertEqual(code, 0)
        known_lines = [line for line in output.splitlines() if ": KNOWN " in line]
        if not known_lines:
            self.skipTest("no KNOWN_BELOW rows are below the floor today")
        summary = output.strip().splitlines()[-1]
        self.assertIn(
            f"{len(known_lines)} known",
            summary.lower(),
            f"the summary claims every glyph clears the floor:\n{summary}",
        )


if __name__ == "__main__":
    unittest.main()
