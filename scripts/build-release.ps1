# scripts/build-release.ps1 — Build and stage a OneTerm release for Windows.
#
# The release workflow (.github/workflows/release.yml) calls THIS script for the
# Windows target, so local and CI packaging produce the same layout.
#
# Usage: pwsh scripts/build-release.ps1
#        pwsh scripts/build-release.ps1 -Target aarch64-pc-windows-msvc
#        pwsh scripts/build-release.ps1 -NoDist            # build only, do not stage dist/
#
# The release binary is `oneterm` (gated by the `release-bin` feature in
# crates/app/Cargo.toml). The development binary is `oneterm-debug` (the default
# `dev-bin` feature). Passing --no-default-features --features release-bin makes
# the release build produce only `oneterm`. Only `oneterm-app` is built (-p): the
# other workspace members (diagnostics in crates/tools, …) are not part of a release.
#
# Outputs (VERSION = repo-root VERSION file, TRIPLE = target triple):
#   - target/<triple>/release/oneterm.exe                   (binary with icon + version info)
#   - target/<triple>/release/conpty.dll                    (copied by build.rs)
#   - target/<triple>/release/x64/OpenConsole.exe           (copied by build.rs)
#   - dist/oneterm-<VERSION>-<triple>/                      (exe + runtime assets)
#   - dist/oneterm-<VERSION>-<triple>.zip + .sha256         (archive + checksum)
#
# The staged directory contains only build outputs — never developer state such as a
# repo-root terminal.json / docks.json (release builds create ~/.OneTerm/ on first run).

[CmdletBinding()]
param(
    [string]$Target = "",            # For example, aarch64-pc-windows-msvc; empty uses the host triple.
    [switch]$NoDist                  # Skip staging dist/.
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    $version = (Get-Content (Join-Path $repoRoot "VERSION") -Raw).Trim()

    # Build only `oneterm`: enable release-bin and disable the default dev-bin.
    $releaseArgs = @("build", "-p", "oneterm-app", "--release", "--no-default-features", "--features", "release-bin")
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repoRoot "target" }
    if ($Target) {
        Write-Host "==> cargo $($releaseArgs -join ' ') --target $Target" -ForegroundColor Cyan
        cargo @releaseArgs --target $Target
        $triple = $Target
        $releaseDir = Join-Path $targetDir "$Target/release"
    } else {
        Write-Host "==> cargo $($releaseArgs -join ' ')" -ForegroundColor Cyan
        cargo @releaseArgs
        $triple = (& rustc -vV | Select-String "^host:").ToString().Split(" ")[1]
        # Without --target, Cargo writes directly to target/release.
        $releaseDir = Join-Path $targetDir "release"
    }
    if ($LASTEXITCODE -ne 0) { throw "Build failed." }

    $exe = Join-Path $releaseDir "oneterm.exe"
    if (-not (Test-Path $exe)) { throw "Release binary not found: $exe" }
    Write-Host "OK: $exe" -ForegroundColor Green

    if ($NoDist) { return }

    # Create a clean distribution directory (build outputs only).
    $distName = "oneterm-$version-$triple"
    $distRoot = Join-Path $repoRoot "dist"
    $distDir = Join-Path $distRoot $distName
    $zipPath = Join-Path $distRoot "$distName.zip"
    foreach ($stale in @($distDir, $zipPath, "$zipPath.sha256")) {
        if (Test-Path $stale) { Remove-Item -Recurse -Force $stale }
    }
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null

    Copy-Item $exe -Destination $distDir
    # Copy the runtime assets that build.rs placed in the release directory.
    $conpty = Join-Path $releaseDir "conpty.dll"
    if (Test-Path $conpty) { Copy-Item $conpty -Destination $distDir }
    $openConsole = Join-Path $releaseDir "x64/OpenConsole.exe"
    if (Test-Path $openConsole) {
        New-Item -ItemType Directory -Force -Path (Join-Path $distDir "x64") | Out-Null
        Copy-Item $openConsole -Destination (Join-Path $distDir "x64")
    }

    Write-Host "==> Distribution staged at: $distDir" -ForegroundColor Green
    Get-ChildItem -Recurse -File $distDir | ForEach-Object { Write-Host "  $($_.FullName.Substring($distDir.Length + 1))" }

    # Archive + checksum (same names the release workflow publishes).
    Compress-Archive -Path $distDir -DestinationPath $zipPath -CompressionLevel Optimal
    $hash = (Get-FileHash -Algorithm SHA256 $zipPath).Hash.ToLowerInvariant()
    # sha256sum-compatible line: "<hash>  <file>"
    [System.IO.File]::WriteAllText("$zipPath.sha256", "$hash  $distName.zip`n")
    Write-Host "==> Archive: $zipPath" -ForegroundColor Green
    Write-Host "$hash  $distName.zip"
}
finally {
    Pop-Location
}
