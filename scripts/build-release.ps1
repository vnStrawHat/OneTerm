# scripts/build-release.ps1 — Build bản release cho myTerm2 (Windows).
#
# Chạy:  pwsh scripts/build-release.ps1           # debug-host default (x86_64)
#        pwsh scripts/build-release.ps1 -Target aarch64-pc-windows-msvc
#
# Kết quả:
#   - target/<triple>/release/myterm2.exe        (có nhúng app icon + version info)
#   - target/<triple>/release/conpty.dll          (build.rs tự copy)
#   - target/<triple>/release/x64/OpenConsole.exe (build.rs tự copy)
# Đồng thời stage thêm bản đóng gói sạch vào dist/myterm2-<triple>/ để phát hành.

[CmdletBinding()]
param(
    [string]$Target = "",            # vd. "aarch64-pc-windows-msvc"; rỗng = host triple
    [switch]$NoDist                  # Bỏ qua bước stage dist/
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    Write-Host "==> cargo build --release" -ForegroundColor Cyan
    if ($Target) {
        cargo build --release --target $Target
        $releaseDir = Join-Path $repoRoot "target/$Target/release"
        $distName   = "myterm2-$Target"
    } else {
        cargo build --release
        $hostTriple = (& rustc -vV | Select-String "^host:").ToString().Split(" ")[1]
        # Khi không truyền --target, cargo ghi trực tiếp ra target/release (không có subdir triple).
        $releaseDir = Join-Path $repoRoot "target/release"
        $distName   = "myterm2-$hostTriple"
    }
    if ($LASTEXITCODE -ne 0) { throw "Build thất bại." }

    $exe = Join-Path $releaseDir "myterm2.exe"
    if (-not (Test-Path $exe)) { throw "Không tìm thấy $exe." }
    Write-Host "OK: $exe" -ForegroundColor Green

    if ($NoDist) { return }

    # Stage thư mục dist sạch.
    $distDir = Join-Path $repoRoot "dist/$distName"
    if (Test-Path $distDir) { Remove-Item -Recurse -Force $distDir }
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null

    Copy-Item $exe -Destination $distDir
    # Runtime assets đã được build.rs copy ra releaseDir → copy tiếp vào dist.
    Copy-Item (Join-Path $releaseDir "conpty.dll") -Destination $distDir -ErrorAction SilentlyContinue
    if (Test-Path (Join-Path $releaseDir "x64")) {
        New-Item -ItemType Directory -Force -Path (Join-Path $distDir "x64") | Out-Null
        Copy-Item (Join-Path $releaseDir "x64/OpenConsole.exe") -Destination (Join-Path $distDir "x64") -ErrorAction SilentlyContinue
    }
    # File cấu hình mặc định (nếu tồn tại ở root) cho lần chạy đầu.
    foreach ($cfg in @("terminal.json","docks.json")) {
        if (Test-Path (Join-Path $repoRoot $cfg)) {
            Copy-Item (Join-Path $repoRoot $cfg) -Destination $distDir -ErrorAction SilentlyContinue
        }
    }

    Write-Host "==> dist staged tại: $distDir" -ForegroundColor Green
    Get-ChildItem -Recurse $distDir | ForEach-Object { Write-Host "  $($_.FullName.Substring($distDir.Length+1))" }
}
finally {
    Pop-Location
}