#!/usr/bin/env python3
"""Generate OneTerm's `external` completion catalogs from upstream docs.

Turns raw upstream documentation into the minimal per-command JSON catalogs
embedded by the `oneterm-completion` engine crate (see
`docs/auto-completion/07-external-assets-script.md`). It produces only the
`external` categories — `cmd` (from MicrosoftDocs windows-commands) and
`coreutils` (from Debian bookworm manpages). The `manual` categories
(`windows`, `linux`, `common`, incl. `git`) are hand-authored and never touched
by this script (it only validates them).

Which commands are generated (and kept) is driven by the curated whitelist in
`scripts/completion-commands.json` — one list per source. `generate` only writes
whitelisted commands and **prunes** any committed catalog file whose command is
not on the list (pruning runs even offline, so editing the whitelist alone trims
the committed catalogs).

Subcommands:
    download            fetch raw upstream sources into the git-ignored cache
    generate            parse the cache → write committed per-command JSON
    update              download + generate + print a diff summary
    validate            validate every committed catalog against the schema

Scope / safety (AGENTS.md core principle 7): only reads/writes inside the
workspace — `scripts/.cache/completion/` for raw downloads and
`crates/completion/assets/` for output. It never scans the whole disk.

Networking uses only the Python standard library. If a source cannot be fetched
(offline CI), `generate` still runs against whatever is cached, and the
committed catalogs remain the source of truth.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import urllib.request
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "scripts" / ".cache" / "completion"
ASSETS = ROOT / "crates" / "completion" / "assets"
SCHEMA_PATH = ASSETS / "catalog.schema.json"
COMMANDS_FILE = ROOT / "scripts" / "completion-commands.json"
SCHEMA_VERSION = 1

CMD_REPO = "https://github.com/MicrosoftDocs/windowsserverdocs.git"
CMD_SUBTREE = "WindowsServerDocs/administration/windows-commands"
COREUTILS_INDEX = "https://manpages.debian.org/bookworm/coreutils/index.html"
COREUTILS_MANPAGE = "https://manpages.debian.org/bookworm/coreutils/{name}.1.en.html"

USER_AGENT = "oneterm-completion-catalog/1.0 (+https://oneterm)"


# ── helpers ────────────────────────────────────────────────────────────────


def log(msg: str) -> None:
    print(f"[completion-catalog] {msg}", file=sys.stderr)


def http_get(url: str) -> str:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:  # noqa: S310
        return response.read().decode("utf-8", errors="replace")


def write_json_atomic(path: Path, node: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(node, indent=2, ensure_ascii=False, sort_keys=False) + "\n"
    path.write_text(text, encoding="utf-8")


def build_node(name: str, options: list[str]) -> dict:
    """Build a schema-valid leaf command node with stable ordering."""
    deduped = sorted(dict.fromkeys(options))
    return {
        "schema": SCHEMA_VERSION,
        "generated": date.today().isoformat(),
        "name": name,
        "options": deduped,
    }


def load_command_list(source: str) -> set[str]:
    """Return the curated set of command names for `source` (cmd/coreutils).

    The whitelist in `scripts/completion-commands.json` is the single source of
    truth for *which* commands get generated and kept; everything else is
    pruned. Names are compared case-insensitively (cmd docs vary in casing).
    """
    data = json.loads(COMMANDS_FILE.read_text(encoding="utf-8"))
    names = data.get(source, [])
    if not isinstance(names, list):
        raise SystemExit(f"completion-commands.json: '{source}' must be a list")
    return {str(n).strip().lower() for n in names if str(n).strip()}


def prune_to_list(out_dir: Path, allowed: set[str]) -> int:
    """Delete committed `<out_dir>/*.json` whose stem is not in `allowed`.

    Runs even when the upstream cache is absent, so trimming the whitelist alone
    (offline) prunes the committed catalogs to match. Returns the count removed.
    """
    if not out_dir.is_dir():
        return 0
    removed = 0
    for path in sorted(out_dir.glob("*.json")):
        if path.stem.lower() not in allowed:
            path.unlink()
            removed += 1
    if removed:
        log(f"{out_dir.name}: pruned {removed} file(s) not in the whitelist")
    return removed


# ── download ────────────────────────────────────────────────────────────────


def download_cmd() -> None:
    """Shallow sparse-clone the MicrosoftDocs windows-commands subtree."""
    dest = CACHE / "windowsserverdocs"
    dest.mkdir(parents=True, exist_ok=True)
    if not (dest / ".git").is_dir():
        run_git(["init", "-q"], dest)
        run_git(["remote", "add", "origin", CMD_REPO], dest)
        run_git(["config", "core.sparseCheckout", "true"], dest)
        (dest / ".git" / "info" / "sparse-checkout").write_text(
            CMD_SUBTREE + "/\n", encoding="utf-8"
        )
    log("fetching windows-commands (shallow)…")
    run_git(["pull", "--depth", "1", "-q", "origin", "main"], dest)


def download_coreutils() -> None:
    """Fetch the coreutils index + each utility manpage into the cache."""
    dest = CACHE / "coreutils"
    dest.mkdir(parents=True, exist_ok=True)
    log("fetching coreutils index…")
    index_html = http_get(COREUTILS_INDEX)
    (dest / "index.html").write_text(index_html, encoding="utf-8")
    names = parse_coreutils_index(index_html)
    allowed = load_command_list("coreutils")
    names = [n for n in names if n.lower() in allowed]
    log(f"downloading {len(names)} whitelisted coreutils utilities")
    for name in names:
        try:
            page = http_get(COREUTILS_MANPAGE.format(name=name))
        except Exception as exc:  # noqa: BLE001
            log(f"  skip {name}: {exc}")
            continue
        (dest / f"{name}.html").write_text(page, encoding="utf-8")


def run_git(args: list[str], cwd: Path) -> None:
    subprocess.run(["git", *args], cwd=cwd, check=True)


# ── parse: cmd ───────────────────────────────────────────────────────────────


CMD_FLAG_RE = re.compile(r"(?<![\w/])([/-][A-Za-z]{1,2})(?![A-Za-z])")


def generate_cmd() -> int:
    allowed = load_command_list("cmd")
    out_dir = ASSETS / "external" / "cmd"
    subtree = CACHE / "windowsserverdocs" / CMD_SUBTREE
    count = 0
    if subtree.is_dir():
        for md in sorted(subtree.glob("*.md")):
            name = md.stem.lower()
            if name in {"windows-commands", "index"} or name not in allowed:
                continue
            options = extract_cmd_options(md.read_text(encoding="utf-8", errors="replace"))
            if not options:
                continue
            write_json_atomic(out_dir / f"{name}.json", build_node(name, options))
            count += 1
        log(f"cmd: wrote {count} command files")
    else:
        log("cmd cache missing — skipping regenerate, pruning committed files only")
    # Always prune committed files down to the whitelist (works offline).
    prune_to_list(out_dir, allowed)
    return count


def _cmd_flags_from_text(text: str) -> set[str]:
    """Extract `/X`/`/XY` (and `-X`) short flags, uppercased for dedup."""
    out: set[str] = set()
    for m in CMD_FLAG_RE.finditer(text):
        tok = m.group(1)
        out.add(tok[0] + tok[1:].upper())
    return out


def extract_cmd_options(markdown: str) -> list[str]:
    """Extract flag tokens from a windows-command page.

    Scans only the fenced **syntax** code blocks and the **Parameters** section
    (not prose/link URLs), so doc links like `/windows-hardware/...` never leak
    in. Flags are restricted to `/`/`-` + 1–2 letters (real cmd switch shape).
    """
    options: set[str] = set()
    # Syntax code blocks (```...```).
    for block in re.findall(r"```[^\n]*\n(.*?)```", markdown, re.DOTALL):
        options |= _cmd_flags_from_text(block)
    # The Parameters section, up to the next `##` heading.
    section = re.search(
        r"##\s+Parameters\s*(.*?)(?:\n##\s|\Z)",
        markdown,
        re.DOTALL | re.IGNORECASE,
    )
    if section:
        options |= _cmd_flags_from_text(section.group(1))
    return sorted(options)


# ── parse: coreutils ─────────────────────────────────────────────────────────


def parse_coreutils_index(html: str) -> list[str]:
    names: set[str] = set()
    for match in re.finditer(r'href="/bookworm/coreutils/([a-z0-9_.-]+)\.1\.en\.html"', html):
        names.add(match.group(1))
    return sorted(names)


COREUTILS_OPT_RE = re.compile(r"(-[A-Za-z])(?:,\s*(--[A-Za-z][\w-]+))?|(--[A-Za-z][\w-]+)")


def generate_coreutils() -> int:
    allowed = load_command_list("coreutils")
    out_dir = ASSETS / "external" / "coreutils"
    src_dir = CACHE / "coreutils"
    count = 0
    if src_dir.is_dir():
        for page in sorted(src_dir.glob("*.html")):
            if page.name == "index.html":
                continue
            name = page.stem
            if name.lower() not in allowed:
                continue
            options = extract_coreutils_options(page.read_text(encoding="utf-8", errors="replace"))
            if not options:
                continue
            write_json_atomic(out_dir / f"{name}.json", build_node(name, options))
            count += 1
        log(f"coreutils: wrote {count} command files")
    else:
        log("coreutils cache missing — skipping regenerate, pruning committed files only")
    prune_to_list(out_dir, allowed)
    return count


def extract_coreutils_options(html: str) -> list[str]:
    """Extract short/long option tokens from a manpage OPTIONS section."""
    # Strip HTML tags to plain text before scanning.
    text = re.sub(r"<[^>]+>", " ", html)
    options: set[str] = set()
    for short, long_a, long_b in COREUTILS_OPT_RE.findall(text):
        for token in (short, long_a, long_b):
            if not token:
                continue
            token = re.split(r"[\[\]=]", token, maxsplit=1)[0]
            if len(token) >= 2:
                options.add(token)
    return sorted(options)


# ── validate ─────────────────────────────────────────────────────────────────


def validate_all() -> int:
    """Validate every committed catalog file against the schema (best-effort).

    Uses `jsonschema` if installed; otherwise falls back to structural checks
    that mirror the schema's required fields so CI without the dependency still
    catches malformed files.
    """
    try:
        import jsonschema  # type: ignore
    except ImportError:
        jsonschema = None

    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    errors = 0
    for path in sorted(ASSETS.rglob("*.json")):
        if path.name == "catalog.schema.json":
            continue
        try:
            node = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            log(f"INVALID {path.relative_to(ROOT)}: {exc}")
            errors += 1
            continue
        if jsonschema is not None:
            try:
                jsonschema.validate(node, schema)
            except jsonschema.ValidationError as exc:  # type: ignore[attr-defined]
                log(f"INVALID {path.relative_to(ROOT)}: {exc.message}")
                errors += 1
        elif not structural_check(node):
            log(f"INVALID {path.relative_to(ROOT)}: missing required 'name' / bad node")
            errors += 1
    if errors:
        log(f"validation failed for {errors} file(s)")
    else:
        log("all catalogs valid")
    return errors


def structural_check(node: object) -> bool:
    if not isinstance(node, dict) or not isinstance(node.get("name"), str):
        return False
    for opt in node.get("options", []):
        if not (isinstance(opt, str) or (isinstance(opt, dict) and "flag" in opt)):
            return False
    for child in node.get("subcommands", []):
        if not structural_check(child):
            return False
    return True


# ── CLI ──────────────────────────────────────────────────────────────────────


def cmd_download(args: argparse.Namespace) -> int:
    CACHE.mkdir(parents=True, exist_ok=True)
    if args.source in (None, "cmd"):
        download_cmd()
    if args.source in (None, "coreutils"):
        download_coreutils()
    return 0


def cmd_generate(args: argparse.Namespace) -> int:
    if args.source in (None, "cmd"):
        generate_cmd()
    if args.source in (None, "coreutils"):
        generate_coreutils()
    return validate_all()


def cmd_update(args: argparse.Namespace) -> int:
    cmd_download(args)
    rc = cmd_generate(args)
    print_git_diff_summary()
    return rc


def cmd_validate(_args: argparse.Namespace) -> int:
    return 1 if validate_all() else 0


def print_git_diff_summary() -> None:
    try:
        result = subprocess.run(
            ["git", "-C", str(ROOT), "diff", "--stat", "--", str(ASSETS)],
            capture_output=True,
            text=True,
            check=False,
        )
        print(result.stdout or "(no changes)")
    except Exception as exc:  # noqa: BLE001
        log(f"could not compute git diff: {exc}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    def add_source(p: argparse.ArgumentParser) -> None:
        p.add_argument("--source", choices=["cmd", "coreutils"], default=None)

    p_dl = sub.add_parser("download", help="fetch raw upstream docs into the cache")
    add_source(p_dl)
    p_dl.set_defaults(func=cmd_download)

    p_gen = sub.add_parser("generate", help="parse the cache → committed JSON")
    add_source(p_gen)
    p_gen.set_defaults(func=cmd_generate)

    p_up = sub.add_parser("update", help="download + generate + diff summary")
    add_source(p_up)
    p_up.set_defaults(func=cmd_update)

    p_val = sub.add_parser("validate", help="validate committed catalogs")
    p_val.set_defaults(func=cmd_validate)

    return parser


def main() -> int:
    args = build_parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
