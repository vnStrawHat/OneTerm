"""Regression tests for the contributor-facing English checker."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

CHECKER_PATH = Path(__file__).with_name("check-english.py")
SPEC = importlib.util.spec_from_file_location("check_english", CHECKER_PATH)
assert SPEC is not None and SPEC.loader is not None
check_english = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check_english)


class ContributorTextTests(unittest.TestCase):
    """Verify release-script suffixes and diagnostics are inspected."""

    def test_shell_and_powershell_are_supported(self) -> None:
        self.assertIn(".sh", check_english.SUFFIXES)
        self.assertIn(".ps1", check_english.SUFFIXES)

    def test_complete_release_script_is_inspected(self) -> None:
        vietnamese = "kh\u00f4ng t\u00ecm th\u1ea5y"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for suffix in (".sh", ".ps1"):
                path = root / f"release{suffix}"
                path.write_text(f'echo "{vietnamese}"\n', encoding="utf-8")

                text = check_english.contributor_text(path)

                self.assertIsNotNone(check_english.VIETNAMESE.search(text))


if __name__ == "__main__":
    unittest.main()
