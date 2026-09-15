# scripts/ci-local.ps1 — run the same quality gate as .github/workflows/ci.yml, locally.
#
# Usage:
#   pwsh scripts/ci-local.ps1           # fmt, clippy, test + the Python policy checks
#   pwsh scripts/ci-local.ps1 -Full     # also: cargo deny (needs cargo-deny installed)
#
# Stops at the first failing command and prints it. Keep this list in sync with
# ci.yml and AGENTS.md §4 (scripts/ci-local.sh is the bash twin).

[CmdletBinding()]
param(
    [switch]$Full
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

function Invoke-Step {
    param([Parameter(Mandatory)][string[]]$Command)
    Write-Host ""
    Write-Host "==> $($Command -join ' ')"
    & $Command[0] @($Command[1..($Command.Length - 1)])
    if ($LASTEXITCODE -ne 0) {
        Write-Error "ci-local: FAILED: $($Command -join ' ')"
        exit 1
    }
}

Invoke-Step @("cargo", "fmt", "--all", "--", "--check")
Invoke-Step @("cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings")
# `terminal-diagnostics` guards ~200 lines no other step compiles (`US-0090`).
Invoke-Step @("cargo", "clippy", "--workspace", "--all-targets", "--features", "oneterm-app/terminal-diagnostics", "--", "-D", "warnings")
Invoke-Step @("cargo", "test", "--workspace")
# IN-0029 R-28: the VT engine's integrity walk is bounded to the rows an
# operation touched unless `vt-paranoid` is on. This is where the unbounded
# whole-history invariants are gated.
Invoke-Step @("cargo", "test", "-p", "oneterm-vt", "--features", "vt-paranoid")
# `regex` gates a whole matcher, so the suite runs under it too: the literal
# half must answer identically with the feature on.
Invoke-Step @("cargo", "test", "-p", "oneterm-vt", "--features", "regex")

# Other projects consume `oneterm-vt` as a git dependency, so its package, its
# feature matrix and its documentation are part of the gate. The default set is
# `pty` (`US-0104`), so the two builds below really are different
# configurations and only the first one is transport-free.
Invoke-Step @("cargo", "build", "-p", "oneterm-vt", "--no-default-features", "--examples")
# Building it is not running it: `tests/engine_without_pty.rs` is gated
# `#[cfg(not(feature = "pty"))]`, so this is the only step that executes it.
Invoke-Step @("cargo", "test", "-p", "oneterm-vt", "--no-default-features")
Invoke-Step @("cargo", "build", "-p", "oneterm-vt", "--all-features", "--examples")
# The six-dependency claim the README makes is a claim about
# `--no-default-features` specifically now that a *default* feature adds
# dependencies. Without this assertion it would be prose.
Write-Host ""
Write-Host "==> cargo tree -p oneterm-vt -e normal --no-default-features"
$vtTree = cargo tree -p oneterm-vt -e normal --no-default-features --prefix none
if ($LASTEXITCODE -ne 0) {
    Write-Error "ci-local: FAILED: cargo tree -p oneterm-vt --no-default-features"
    exit 1
}
$vtLeaves = (($vtTree | Select-Object -Skip 1 | ForEach-Object { ($_ -split ' ')[0] } |
    Sort-Object -Unique) -join ' ')
if ($vtLeaves -ne 'bitflags log memchr rustc-hash unicode-segmentation unicode-width') {
    Write-Error ("ci-local: FAILED: oneterm-vt --no-default-features must be exactly six leaf " +
        "dependencies, got: $vtLeaves")
    exit 1
}
Invoke-Step @("cargo", "run", "-p", "oneterm-vt", "--example", "headless")
$previousRustdocFlags = $env:RUSTDOCFLAGS
$env:RUSTDOCFLAGS = "-D warnings"
try {
    # No features first: an intra-doc link to a cfg-gated item resolves under
    # `--all-features` and is broken in a default build, which is the build most
    # embedders get (`US-0101` verification note 4).
    Invoke-Step @("cargo", "doc", "-p", "oneterm-vt", "--no-deps")
    # `--all-features` must be the **last** rustdoc: the API check below reads
    # `target/doc` with `--no-doc`, and the surface it compares against is the
    # all-features one. Swapping these two silently drops every gated item.
    Invoke-Step @("cargo", "doc", "-p", "oneterm-vt", "--no-deps", "--all-features")
} finally {
    $env:RUSTDOCFLAGS = $previousRustdocFlags
}
# Two snapshots, one per platform family (`US-0104`): `--check` compares the
# host's, `--diff-platforms` asserts the other one differs only inside
# `oneterm_vt::pty` and needs no rustdoc, `--check-nameable` fails on a public
# signature naming a type an embedder cannot write (`BUG-0059`).
Invoke-Step @("python", "scripts/vt-public-api.py", "--check", "--no-doc")
Invoke-Step @("python", "scripts/vt-public-api.py", "--check-nameable", "--no-doc")
Invoke-Step @("python", "scripts/vt-public-api.py", "--diff-platforms")

# What the package carries, and that it reaches nothing outside `crates/vt`.
# `--allow-dirty` because an agent runs this gate with uncommitted work; the
# workflow packages a clean checkout without it.
Write-Host ""
Write-Host "==> cargo package -p oneterm-vt --list | verify-dependency-graph.py --package-list -"
# Both halves are checked: a pipeline's `$LASTEXITCODE` is the last command's,
# so piping straight into python would report python's status for a `cargo
# package` that failed.
$packageList = cargo package -p oneterm-vt --allow-dirty --list
if ($LASTEXITCODE -ne 0) {
    Write-Error "ci-local: FAILED: cargo package -p oneterm-vt --list"
    exit 1
}
$packageList | python scripts/verify-dependency-graph.py --package-list -
if ($LASTEXITCODE -ne 0) {
    Write-Error "ci-local: FAILED: the oneterm-vt package is missing a required file"
    exit 1
}

# Published rustdoc must read for somebody who does not have this repository:
# no work-packet, decision or intake citations in `///` or `//!` text, and no
# bare `crates/...` or `docs/...` path either -- a consumer's vendored copy has
# neither. A link to the public repository is the one allowed form.
Write-Host ""
Write-Host "==> rustdoc self-containment (crates/vt/src)"
$citations = Get-ChildItem -Path "crates/vt/src" -Recurse -Filter "*.rs" |
    Select-String -Pattern '^\s*//[/!].*(US-0\d{3}|BUG-0\d{3}|DEC-0\d{3}|IN-0\d{3}|crates/|docs/)' |
    Where-Object { $_.Line -notmatch 'https://github\.com/' }
if ($citations) {
    $citations | ForEach-Object { Write-Host $_ }
    Write-Error "ci-local: FAILED: the crate rustdoc cites a document only this repository has"
    exit 1
}

# The embedder's guide is `//!` text too: every chapter is `include_str!`d into
# an empty module, so it is published rustdoc and obeys the same rule. The
# chapters are Markdown, so there is no comment prefix to match on.
Write-Host ""
Write-Host "==> rustdoc self-containment (crates/vt/docs/guide)"
$guideCitations = Get-ChildItem -Path "crates/vt/docs/guide" -Recurse -Filter "*.md" |
    Select-String -Pattern '(US-0\d{3}|BUG-0\d{3}|DEC-0\d{3}|IN-0\d{3}|crates/|docs/)' |
    Where-Object { $_.Line -notmatch 'https://github\.com/' }
if ($guideCitations) {
    $guideCitations | ForEach-Object { Write-Host $_ }
    Write-Error "ci-local: FAILED: the embedder guide cites a document only this repository has"
    exit 1
}

Invoke-Step @("python", "scripts/verify-dependency-graph.py")
Invoke-Step @("python", "scripts/check-doc-paths.py")
Invoke-Step @("python", "-m", "unittest", "scripts/test_check_english.py")
Invoke-Step @("python", "scripts/check-english.py")
Invoke-Step @("python", "scripts/completion-catalog.py", "validate")
Invoke-Step @("python", "scripts/third-party-notices.py", "--check")

if ($Full) {
    if (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
        Invoke-Step @("cargo", "deny", "check", "licenses", "bans", "advisories")
    } else {
        Write-Warning "ci-local: cargo-deny not installed (cargo install cargo-deny); skipping"
    }
}

Write-Host ""
Write-Host "ci-local: all checks passed."
