# scripts/bump-conpty.ps1 — Bump or verify the bundled ConPTY pair.
#
# OneTerm ships Windows Terminal's `conpty.dll` + `x64/OpenConsole.exe` next to
# oneterm.exe (crates/app/build.rs copies them) so local shells get Windows
# Terminal's console host instead of the inbox conhost.exe — see
# docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md.
#
# Both files come as a matched pair from the NuGet package
# `Microsoft.Windows.Console.ConPTY` (MIT, published by Microsoft from the
# microsoft/terminal repository). This script downloads one version of that
# package, checks the two binaries are Authenticode-signed by Microsoft, copies
# the x64 pair into crates/app/assets/, and records what it installed in
# crates/app/assets/conpty-manifest.json. THIRD-PARTY-NOTICES.md is generated
# from that manifest, so re-run `python scripts/third-party-notices.py` after a
# bump (CI's `--check` fails otherwise).
#
# Usage: pwsh scripts/bump-conpty.ps1 -Latest             # newest stable release
#        pwsh scripts/bump-conpty.ps1 -Version 1.24.260710001
#        pwsh scripts/bump-conpty.ps1 -Check              # offline: assets == manifest
#
# Windows only (the assets are Windows binaries and the signature check uses
# Authenticode), so there is no .sh twin. `-Check` is also enforced on every
# platform by scripts/third-party-notices.py --check, which CI runs.

[CmdletBinding()]
param(
    [string]$Version = "",           # Package version, for example 1.24.260710001.
    [switch]$Latest,                 # Use the newest non-preview version on nuget.org.
    [switch]$Check                   # Verify the tracked assets against the manifest, no network.
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $repoRoot "crates/app/assets"
$manifestPath = Join-Path $assets "conpty-manifest.json"
$packageId = "Microsoft.Windows.Console.ConPTY"
$feed = "https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty"

# Asset path (relative to crates/app/assets) → entry inside the .nupkg.
# Only the x64 pair is tracked: build.rs copies the binaries for x86_64 targets
# only, and an aarch64 build falls back to the OS ConPTY.
$layout = [ordered]@{
    "conpty.dll"           = "runtimes/win-x64/native/conpty.dll"
    "x64/OpenConsole.exe"  = "build/native/runtimes/x64/OpenConsole.exe"
}

function Get-AssetInfo([string]$name) {
    $path = Join-Path $assets $name
    if (-not (Test-Path $path)) { return $null }
    [pscustomobject]@{
        sha256       = (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant()
        file_version = (Get-Item $path).VersionInfo.FileVersion
    }
}

function Read-Manifest {
    if (-not (Test-Path $manifestPath)) { throw "Manifest not found: $manifestPath" }
    Get-Content $manifestPath -Raw | ConvertFrom-Json
}

# ── -Check: recompute the asset hashes and compare with the manifest ──────────
if ($Check) {
    $manifest = Read-Manifest
    $bad = 0
    foreach ($name in $layout.Keys) {
        $actual = Get-AssetInfo $name
        $expected = $manifest.files.$name
        if (-not $actual) {
            Write-Host "MISSING  $name" -ForegroundColor Red; $bad++
        } elseif ($actual.sha256 -ne $expected.sha256) {
            Write-Host "MISMATCH $name" -ForegroundColor Red
            Write-Host "         manifest $($expected.sha256)"
            Write-Host "         on disk  $($actual.sha256)"
            $bad++
        } else {
            Write-Host "OK       $name  $($expected.file_version)" -ForegroundColor Green
        }
    }
    if ($bad) {
        Write-Host "conpty-manifest.json does not describe crates/app/assets/ ($bad file(s))" -ForegroundColor Red
        exit 1
    }
    Write-Host "conpty-manifest.json matches the tracked assets ($($manifest.package) $($manifest.version))." -ForegroundColor Green
    exit 0
}

# ── Resolve the version to install ────────────────────────────────────────────
if (-not $Version -and -not $Latest) {
    throw "Pass -Version <x.y.z>, -Latest, or -Check."
}
if ($Latest) {
    $index = Invoke-RestMethod -Uri "$feed/index.json"
    # Skip pre-release versions (they carry a `-preview` suffix).
    $Version = @($index.versions | Where-Object { $_ -notmatch '-' })[-1]
    Write-Host "Latest stable $packageId is $Version" -ForegroundColor Cyan
}

$url = "$feed/$Version/microsoft.windows.console.conpty.$Version.nupkg"
$work = Join-Path ([System.IO.Path]::GetTempPath()) "conpty-bump-$Version"
New-Item -ItemType Directory -Force -Path $work | Out-Null
$nupkg = Join-Path $work "package.zip"

Write-Host "==> $url" -ForegroundColor Cyan
Invoke-WebRequest -Uri $url -OutFile $nupkg

# Opening the archive and extracting the two entries validates the download:
# a truncated or corrupt .nupkg fails here, before anything is copied.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead($nupkg)
try {
    foreach ($name in $layout.Keys) {
        $entry = $zip.GetEntry($layout[$name])
        if (-not $entry) { throw "$($layout[$name]) is not in $packageId $Version" }
        $staged = Join-Path $work (Split-Path $name -Leaf)
        [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $staged, $true)
    }
} finally {
    $zip.Dispose()
}

# The binaries are redistributed as-is, so require Microsoft's Authenticode
# signature before they are copied into the repository.
foreach ($name in $layout.Keys) {
    $staged = Join-Path $work (Split-Path $name -Leaf)
    $sig = Get-AuthenticodeSignature $staged
    if ($sig.Status -ne "Valid" -or $sig.SignerCertificate.Subject -notmatch "O=Microsoft Corporation") {
        throw "$name is not validly signed by Microsoft (status $($sig.Status), subject $($sig.SignerCertificate.Subject))"
    }
}

# ── Install + manifest ────────────────────────────────────────────────────────
$before = [ordered]@{}
foreach ($name in $layout.Keys) { $before[$name] = Get-AssetInfo $name }
$oldVersion = if (Test-Path $manifestPath) { (Read-Manifest).version } else { "(none)" }

foreach ($name in $layout.Keys) {
    $dst = Join-Path $assets $name
    New-Item -ItemType Directory -Force -Path (Split-Path $dst -Parent) | Out-Null
    Copy-Item (Join-Path $work (Split-Path $name -Leaf)) -Destination $dst -Force
}

$files = [ordered]@{}
foreach ($name in $layout.Keys) {
    $info = Get-AssetInfo $name
    $files[$name] = [ordered]@{
        entry        = $layout[$name]
        file_version = $info.file_version
        sha256       = $info.sha256
    }
}
$manifest = [ordered]@{
    package    = $packageId
    version    = $Version
    source     = $url
    project    = "https://github.com/microsoft/terminal"
    licence    = "MIT"
    downloaded = (Get-Date -Format "yyyy-MM-dd")
    files      = $files
}
Set-Content -Path $manifestPath -Value (($manifest | ConvertTo-Json -Depth 5) + "`n") -NoNewline -Encoding utf8

# ── Report ────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "$packageId $oldVersion -> $Version" -ForegroundColor Green
foreach ($name in $layout.Keys) {
    $old = $before[$name]
    $new = $files[$name]
    $oldText = if ($old) { "$($old.file_version) $($old.sha256.Substring(0, 12))" } else { "(absent)" }
    Write-Host ("  {0,-20} {1}  ->  {2} {3}" -f $name, $oldText, $new.file_version, $new.sha256.Substring(0, 12))
}
Write-Host ""
Write-Host "Next: python scripts/third-party-notices.py   (regenerates THIRD-PARTY-NOTICES.md from the manifest)" -ForegroundColor Yellow
