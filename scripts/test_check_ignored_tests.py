"""Regression tests for the ignored-test census.

The census is the union across platforms: an entry can be `cfg`-gated to one OS,
so a runner collects only a subset of the file. These pin the three rules that
follow from that -- a subset passes, an unrecorded test still fails, and
`--write` merges instead of overwriting.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import tempfile
import unittest
import unittest.mock
from pathlib import Path

CHECKER_PATH = Path(__file__).with_name("check-ignored-tests.py")
SPEC = importlib.util.spec_from_file_location("check_ignored_tests", CHECKER_PATH)
assert SPEC is not None and SPEC.loader is not None
check_ignored_tests = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check_ignored_tests)


class CensusTests(unittest.TestCase):
    """Run `main()` against a temporary census with a stubbed collector."""

    def run_main(self, recorded: list[str], collected: list[str], *arguments: str):
        """Return `(exit code, stdout, stderr, census text)` for one invocation."""
        with tempfile.TemporaryDirectory() as directory:
            census = Path(directory) / "ignored-tests.txt"
            census.write_text(
                "# header\n" + "".join(f"{name}\n" for name in recorded),
                encoding="utf-8",
            )
            out, err = io.StringIO(), io.StringIO()
            with unittest.mock.patch.multiple(
                check_ignored_tests,
                CENSUS=census,
                collect=lambda: sorted(collected),
            ):
                with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                    with unittest.mock.patch("sys.argv", ["check", *arguments]):
                        code = check_ignored_tests.main()
            return code, out.getvalue(), err.getvalue(), census.read_text(
                encoding="utf-8"
            )

    def test_a_subset_passes_and_reports_the_rest(self) -> None:
        """A platform that builds fewer of the recorded tests is not a failure."""
        code, out, err, _ = self.run_main(
            recorded=["linux_only", "windows_only"], collected=["linux_only"]
        )

        self.assertEqual(code, 0)
        self.assertIn("not built on this platform", out)
        self.assertIn("windows_only", out)
        self.assertEqual(err, "")

    def test_an_unrecorded_test_fails(self) -> None:
        """A collected test missing from the file is still the only error."""
        code, _, err, _ = self.run_main(recorded=["known"], collected=["known", "fresh"])

        self.assertEqual(code, 1)
        self.assertIn("+ fresh", err)
        self.assertIn("the ignore list changed", err)

    def test_write_merges_instead_of_overwriting(self) -> None:
        """Recording on one OS keeps the entries only another OS collects."""
        code, _, _, written = self.run_main(
            ["windows_only"], ["fresh", "linux_only"], "--write"
        )

        self.assertEqual(code, 0)
        entries = [line for line in written.splitlines() if not line.startswith("#")]
        self.assertEqual(entries, ["fresh", "linux_only", "windows_only"])


if __name__ == "__main__":
    unittest.main()
