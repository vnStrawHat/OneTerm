"""scripts/release-notes.py <range> [previous tag]: a release's notes on stdout, from the
subjects of the commits in a git range (the release workflow's; specs/oneterm, "The repository").
Conventional Commits are grouped by type, a scope set in bold, each line linked to its commit
(GITHUB_REPOSITORY names the repository); breaking changes are listed first as well; the
commit that bumps the version is left out."""
import os
import re
import subprocess
import sys

GROUPS = {
    "feat": "Features", "fix": "Bug Fixes", "perf": "Perf", "refactor": "Refactor", "build": "Build",
    "ci": "CI", "test": "Tests", "docs": "Docs", "style": "Styles", "chore": "Chores", "revert": "Reverts",
    "other": "Others",
}
SUBJECT = re.compile(r"^(?P<type>[A-Za-z]+)(?:\((?P<scope>[^)]+)\))?(?P<breaking>!)?:\s*(?P<what>.+)$")
BUMP = re.compile(r"^chore\(release\):\s*v?\d+\.\d+\.\d+")


def notes(log, repository, previous):
    grouped, breaking = {key: [] for key in GROUPS}, []
    for record in log.split("\x1e"):
        record = record.strip("\n")
        if not record:
            continue
        full, short, subject, body = record.split("\x1f", 3)
        if BUMP.match(subject):
            continue
        match = SUBJECT.match(subject)
        kind = match.group("type").lower() if match else "other"
        scope = f'**{match.group("scope")}:** ' if match and match.group("scope") else ""
        what = match.group("what") if match else subject
        item = f"- {scope}{what} ([`{short}`](https://github.com/{repository}/commit/{full}))"
        grouped[kind if kind in GROUPS else "other"].append(item)
        if (match and match.group("breaking")) or re.search(r"(^|\n)BREAKING[ -]CHANGE:\s*", body):
            breaking.append(item)
    out = ["## What's Changed", "", f"Changes since `{previous}`." if previous else "Initial release changes.", ""]
    sections = [("Breaking Changes", breaking)] + [(GROUPS[key], grouped[key]) for key in GROUPS]
    for title, items in sections:
        if items:
            out += [f"### {title}", "", *items, ""]
    if not any(items for _, items in sections):
        out += ["No notable changes.", ""]
    return "\n".join(out)


def demo():
    log = "\x1e".join([
        "a1\x1fa\x1ffeat(dock)!: columns\x1f",
        "b2\x1fb\x1ffix: a crash\x1fBREAKING CHANGE: settings",
        "c3\x1fc\x1fchore(release): v0.7.1\x1f",
        "d4\x1fd\x1fMerge something\x1f",
    ])
    text = notes(log, "o/r", "v0.7.0")
    assert "Changes since `v0.7.0`." in text and "v0.7.1" not in text
    assert text.count("**dock:** columns ([`a`](https://github.com/o/r/commit/a1))") == 2, text
    assert text.index("### Breaking Changes") < text.index("### Features") < text.index("### Bug Fixes")
    assert "### Others\n\n- Merge something" in text
    assert "No notable changes." in notes("", "o/r", "") and "Initial release changes." in notes("", "o/r", "")


if __name__ == "__main__":
    if sys.argv[1:] == ["--check"]:
        demo()
    else:
        asked = ["git", "log", sys.argv[1], "--format=%H%x1f%h%x1f%s%x1f%b%x1e", "--no-merges"]
        log = subprocess.check_output(asked, text=True, encoding="utf-8")
        print(notes(log, os.environ.get("GITHUB_REPOSITORY", ""), sys.argv[2] if len(sys.argv) > 2 else ""))
