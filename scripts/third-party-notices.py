#!/usr/bin/env python3
"""Generate / verify THIRD-PARTY-NOTICES.md from the resolved Cargo dependency graph.

Usage:
    python scripts/third-party-notices.py            # rewrite THIRD-PARTY-NOTICES.md
    python scripts/third-party-notices.py --check    # exit 1 when the file is stale

Works offline: it only runs `cargo metadata` (which resolves against Cargo.lock) and
reads the crate manifests already in the local Cargo cache. Every crate reachable from
the shipped binary (`oneterm-app`, normal + build dependencies, all release targets) is
listed with its version, declared SPDX licence expression and source. The hand-written
header (bundled non-Rust components, vendored forks, GPL analysis) lives in
``HEADER`` below — edit it here, not in the generated file.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "THIRD-PARTY-NOTICES.md"
APP_PACKAGE = "oneterm-app"
# Same target set as deny.toml [graph].targets — the platforms OneTerm ships for.
TARGETS = (
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)
# Crates whose manifest has no `license` field; the effective licence was established by
# inspection (docs/license-analysis.md §2.1) and is clarified the same way in deny.toml.
LICENSE_CLARIFICATIONS = {
    "gpui_util": "Apache-2.0 (no manifest field; LICENSE-APACHE in the Zed monorepo)",
    "gpui_shared_string": "Apache-2.0 (no manifest field; LICENSE-APACHE in the Zed monorepo)",
}

HEADER = """\
# Third-party notices

OneTerm is licensed under the Apache License 2.0 (see [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE)). The distributed binaries also contain or redistribute the
third-party components below. This file is **generated** by
`python scripts/third-party-notices.py` (CI verifies it with `--check`); edit the
script's header text or the manifests, not this file.

## 1. Bundled non-Rust components (Windows releases)

| Component | Version | Source | Licence |
|---|---|---|---|
| `conpty.dll` | 1.23.2512.16003 (Windows Terminal) | <https://github.com/microsoft/terminal> | MIT — Copyright (c) Microsoft Corporation |
| `x64/OpenConsole.exe` | 1.23.2512.16003 (Windows Terminal) | <https://github.com/microsoft/terminal> | MIT — Copyright (c) Microsoft Corporation |

`crates/app/build.rs` copies both files from `crates/app/assets/` next to `oneterm.exe`
so ConPTY uses Windows Terminal's console host instead of the system `conhost.exe`
(correct Ctrl+C delivery). They are unmodified upstream binaries; SHA-256 of the tracked
copies: `conpty.dll` `1f5ffd52ff118db975eeb25bac0051f4ceff3e051313fa03a5afffa9e75ee502`,
`OpenConsole.exe` `6b2915a9a91c0738346a6c6a7b3ee2b74e26582b0c92b1b16066e72570dddd68`.

MIT licence text (Windows Terminal):

> Copyright (c) Microsoft Corporation.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this
> software and associated documentation files (the "Software"), to deal in the Software
> without restriction, including without limitation the rights to use, copy, modify,
> merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
> permit persons to whom the Software is furnished to do so, subject to the following
> conditions: The above copyright notice and this permission notice shall be included
> in all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED
> "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
> TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
> NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
> CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
> OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
> DEALINGS IN THE SOFTWARE.

## 2. Vendored and patched forks (`vendor/`)

| Crate | Upstream | Base revision | Licence | OneTerm delta |
|---|---|---|---|---|
| `vte` 0.15.0 | <https://crates.io/crates/vte> | 0.15.0 | Apache-2.0 OR MIT | `vendor/patches/vte/` |
| `alacritty_terminal` 0.26.1-dev | <https://github.com/zed-industries/alacritty> | `fcf32feacb367b75ec84dd40f041e4fd411d3cc1` | Apache-2.0 | `vendor/patches/alacritty_terminal/` |
| `gpui-component` 0.5.2 | <https://github.com/longbridge/gpui-component> | `ea6b194db04cc7c0474851f07c7d5b7a9df6a98b` | Apache-2.0 | `vendor/patches/gpui-component/` |

Each fork is pristine upstream plus the listed patch set (`vendor/README.md`); the
upstream `LICENSE-*` files are kept inside each vendored tree.

## 3. Copyleft analysis

Three Zed crates (`zlog`, `ztracing`, `ztracing_macro`) declare `GPL-3.0-or-later`.
`ztracing`/`ztracing_macro` are dual-licensed (Apache-2.0 chosen); `zlog` is GPL-only
but is compiled only behind `cfg(ztracing)` / `cfg(test)` and is not linked into the
release binary. Details and verification method: `docs/license-analysis.md`;
enforcement: `deny.toml` (`cargo deny check licenses`, CI job `cargo-deny`).

`crates/tools/src/bin/doom-fire.rs` (developer diagnostic, **not** part of any release)
is a Rust port of DOOM-fire-zig and keeps that project's GPL-3.0 licence; the
`oneterm-tools` package therefore declares `Apache-2.0 AND GPL-3.0-only`.

## 4. Rust crates linked into the release binary

Every crate reachable from `oneterm-app` (normal + build dependencies) for the release
targets in `deny.toml`. Licence = the SPDX expression declared by the crate; source =
crates.io unless a git URL is shown. Full licence texts ship inside each crate's
package in the Cargo registry / git checkout.

"""


def cargo_metadata() -> dict:
    cmd = ["cargo", "metadata", "--format-version", "1", "--locked"]
    for target in TARGETS:
        cmd += ["--filter-platform", target]
    return json.loads(subprocess.check_output(cmd, cwd=ROOT).decode("utf-8"))


def reachable_from(meta: dict, root_name: str) -> set[str]:
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    root_id = next(p["id"] for p in meta["packages"] if p["name"] == root_name)
    seen: set[str] = set()
    stack = [root_id]
    while stack:
        pkg_id = stack.pop()
        if pkg_id in seen:
            continue
        seen.add(pkg_id)
        for dep in nodes[pkg_id]["deps"]:
            kinds = {k.get("kind") for k in dep["dep_kinds"]} or {None}
            # Skip dev-dependency-only edges: they are not linked into the binary.
            if kinds <= {"dev"}:
                continue
            stack.append(dep["pkg"])
    return seen


def source_label(pkg: dict) -> str:
    source = pkg.get("source") or ""
    if source.startswith("registry+"):
        return "crates.io"
    if source.startswith("git+"):
        url = source[len("git+"):].split("?", 1)[0].split("#", 1)[0]
        return url
    if not source:
        return "vendored fork (vendor/, see section 2)"
    return source


def render(meta: dict) -> str:
    # OneTerm's own workspace crates are first-party (and their version changes on
    # every release), so they are left out of the third-party table.
    wanted = reachable_from(meta, APP_PACKAGE) - set(meta["workspace_members"])
    packages = sorted(
        (p for p in meta["packages"] if p["id"] in wanted),
        key=lambda p: (p["name"].lower(), p["version"]),
    )
    lines = [HEADER, "| Crate | Version | Licence | Source |", "|---|---|---|---|"]
    for pkg in packages:
        licence = (
            pkg.get("license")
            or LICENSE_CLARIFICATIONS.get(pkg["name"])
            or ("(see LICENSE file in package)" if pkg.get("license_file") else "UNKNOWN")
        )
        lines.append(f"| `{pkg['name']}` | {pkg['version']} | {licence} | {source_label(pkg)} |")
    lines.append("")
    lines.append(f"_{len(packages)} third-party packages._")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    check = "--check" in sys.argv[1:]
    text = render(cargo_metadata())
    if check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current.replace("\r\n", "\n") != text:
            print(
                "error: THIRD-PARTY-NOTICES.md is stale; run `python scripts/third-party-notices.py`",
                file=sys.stderr,
            )
            raise SystemExit(1)
        print("THIRD-PARTY-NOTICES.md is up to date.")
        return
    OUTPUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {OUTPUT.relative_to(ROOT).as_posix()}")


if __name__ == "__main__":
    main()
