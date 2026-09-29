# US-0150 pixel pairs: the before and after builds side by side, same home, same moment.
param([string] $Theme = 'Zed One Dark', [double] $FontSize = 0, [string] $Tag = 'dark', [string] $Mode = '')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\gui-lib.ps1"
$S = $PSScriptRoot
$out = "$S\pix"; New-Item -ItemType Directory -Force $out | Out-Null
$home_ = "$S\pixhome"
$b = Start-OneTerm -Exe "$S\bin-before\oneterm.exe" -Home_ $home_ -Theme $Theme -FontSize $FontSize -Mode $Mode -X 40 -Y 40
# The second instance shares the home (and so the breadcrumb path) without resetting it.
$saved = @{}
$envs = @{ USERPROFILE = $home_; HOME = $home_; HOTPATH_METRICS_SERVER_OFF = '1'; HOTPATH_OUTPUT_PATH = "$home_\hotpath-a.json" }
foreach ($k in $envs.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $envs[$k]) }
try {
  $pa = Start-Process -FilePath "$S\bin-after\oneterm.exe" -WorkingDirectory "$home_\cwd" -PassThru `
    -RedirectStandardOutput "$home_\stdout-a.log" -RedirectStandardError "$home_\stderr-a.log"
} finally { foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) } }
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 60 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 500; $h = [U50]::MainWindowOf([uint32]$pa.Id) }
$a = @{ Pid = $pa.Id; Hwnd = $h; Proc = $pa }
[void][U50]::SetWindowPos($h, [IntPtr]::Zero, 60, 60, 1280, 800, 0x0014)
Write-Host "before pid $($b.Pid), after pid $($a.Pid)"
try {
  Start-Sleep -Seconds 6
  foreach ($app in @($b, $a)) { Activate $app $false }
  Start-Sleep -Seconds 2
  # Same wall-clock second for both captures (the clock label).
  while ((Get-Date).Millisecond -gt 300) { Start-Sleep -Milliseconds 50 }
  [U50]::Capture($b.Hwnd, "$out\$Tag-before.png")
  [U50]::Capture($a.Hwnd, "$out\$Tag-after.png")
  $r = New-Object U50+RECT; [void][U50]::GetWindowRect($a.Hwnd, [ref]$r)
  $cr = New-Object U50+RECT; [void][U50]::GetClientRect($a.Hwnd, [ref]$cr)
  Write-Host "window $($r.Right - $r.Left)x$($r.Bottom - $r.Top), client $($cr.Right)x$($cr.Bottom), dpi $([U50]::GetDpiForWindow($a.Hwnd))"
  $W = $r.Right - $r.Left; $H = $r.Bottom - $r.Top
  Write-Host "whole window:" ([U50]::Diff("$out\$Tag-before.png", "$out\$Tag-after.png", 0, 0, $W, $H))
  Write-Host "top 80 px (title bar + tab bar):" ([U50]::Diff("$out\$Tag-before.png", "$out\$Tag-after.png", 0, 0, $W, 80))
  Write-Host "bottom 60 px (status bar):" ([U50]::Diff("$out\$Tag-before.png", "$out\$Tag-after.png", 0, $H - 60, $W, $H))
} finally {
  Stop-OneTerm $b; Stop-OneTerm $a
}
