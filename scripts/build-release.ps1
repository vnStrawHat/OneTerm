# scripts/build-release.ps1 — Build a OneTerm release for Windows.
#
# Usage: pwsh scripts/build-release.ps1
#        pwsh scripts/build-release.ps1 -Target aarch64-pc-windows-msvc
#
# The release binary is `oneterm` (gated by the `release-bin` feature in
# crates/app/Cargo.toml). The development binary is `oneterm-debug` (the default
# `dev-bin` feature). Passing --no-default-features --features release-bin makes
# the release build produce only `oneterm`.
#
# Outputs:
#   - target/<triple>/release/oneterm.exe         (binary with icon + version info)
#   - target/<triple>/release/conpty.dll          (copied by build.rs)
#   - target/<triple>/release/x64/OpenConsole.exe (copied by build.rs)
# The script also stages a clean distribution in dist/oneterm-<triple>/.

[CmdletBinding()]
param(
    [string]$Target = "",            # For example, aarch64-pc-windows-msvc; empty uses the host triple.
    [switch]$NoDist                  # Skip staging dist/.
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    # Build only `oneterm`: enable release-bin and disable the default dev-bin.
    $releaseArgs = @("build", "--release", "--no-default-features", "--features", "release-bin")
    Write-Host "==> cargo $($releaseArgs -join ' ')" -ForegroundColor Cyan
    if ($Target) {
        cargo @releaseArgs --target $Target
        $releaseDir = Join-Path $repoRoot "target/$Target/release"
        $distName   = "oneterm-$Target"
    } else {
        cargo @releaseArgs
        $hostTriple = (& rustc -vV | Select-String "^host:").ToString().Split(" ")[1]
        # Without --target, Cargo writes directly to target/release.
        $releaseDir = Join-Path $repoRoot "target/release"
        $distName   = "oneterm-$hostTriple"
    }
    if ($LASTEXITCODE -ne 0) { throw "Build failed." }

    $exe = Join-Path $releaseDir "oneterm.exe"
    if (-not (Test-Path $exe)) { throw "Release binary not found: $exe" }
    Write-Host "OK: $exe" -ForegroundColor Green

    if ($NoDist) { return }

    # Create a clean distribution directory.
    $distDir = Join-Path $repoRoot "dist/$distName"
    if (Test-Path $distDir) { Remove-Item -Recurse -Force $distDir }
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null

    Copy-Item $exe -Destination $distDir
    # Copy the runtime assets that build.rs placed in the release directory.
    Copy-Item (Join-Path $releaseDir "conpty.dll") -Destination $distDir -ErrorAction SilentlyContinue
    if (Test-Path (Join-Path $releaseDir "x64")) {
        New-Item -ItemType Directory -Force -Path (Join-Path $distDir "x64") | Out-Null
        Copy-Item (Join-Path $releaseDir "x64/OpenConsole.exe") -Destination (Join-Path $distDir "x64") -ErrorAction SilentlyContinue
    }
    # Copy optional default configuration files for the first run.
    foreach ($cfg in @("terminal.json", "docks.json")) {
        if (Test-Path (Join-Path $repoRoot $cfg)) {
            Copy-Item (Join-Path $repoRoot $cfg) -Destination $distDir -ErrorAction SilentlyContinue
        }
    }

    Write-Host "==> Distribution staged at: $distDir" -ForegroundColor Green
    Get-ChildItem -Recurse $distDir | ForEach-Object { Write-Host "  $($_.FullName.Substring($distDir.Length + 1))" }
}
finally {
    Pop-Location
}
