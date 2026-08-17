# scripts/ci-local.ps1 — run the same quality gate as .github/workflows/ci.yml, locally.
#
# Usage:
#   pwsh scripts/ci-local.ps1           # fmt, clippy, build, test + the Python policy checks
#   pwsh scripts/ci-local.ps1 -Full     # also: vendor/refresh.sh --check (network, needs bash) + cargo deny
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
Invoke-Step @("cargo", "build", "--workspace")
Invoke-Step @("cargo", "test", "--workspace")
Invoke-Step @("python", "scripts/verify-dependency-graph.py")
Invoke-Step @("python", "scripts/check-ui-fork.py")
Invoke-Step @("python", "scripts/check-doc-paths.py")
Invoke-Step @("python", "-m", "unittest", "scripts/test_check_english.py")
Invoke-Step @("python", "scripts/check-english.py")
Invoke-Step @("python", "scripts/completion-catalog.py", "validate")
Invoke-Step @("python", "scripts/benchmark-scale.py", "--list")

if ($Full) {
    if (Get-Command bash -ErrorAction SilentlyContinue) {
        Invoke-Step @("bash", "vendor/refresh.sh", "--check")
    } else {
        Write-Warning "ci-local: bash not found; skipping vendor/refresh.sh --check"
    }
    if (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
        Invoke-Step @("cargo", "deny", "check", "licenses", "bans", "advisories")
    } else {
        Write-Warning "ci-local: cargo-deny not installed (cargo install cargo-deny); skipping"
    }
}

Write-Host ""
Write-Host "ci-local: all checks passed."
