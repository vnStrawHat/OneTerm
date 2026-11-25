# US-0150: the CPU/MEM hover table (US-0148) stays live over a cached status bar.
param([string] $Which = 'v2')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\gui-lib.ps1"
$S = $PSScriptRoot
$out = "$S\tooltip"; New-Item -ItemType Directory -Force $out | Out-Null
$app = Start-OneTerm -Exe "$S\bin-$Which\oneterm.exe" -Home_ "$S\tiphome-$Which" -Theme 'Zed One Dark' -X 200 -Y 150
try {
  Start-Sleep -Seconds 5
  Activate $app $true
  $cr = New-Object U50+RECT; [void][U50]::GetClientRect($app.Hwnd, [ref]$cr)
  # The CPU/MEM label sits left of the dock button at the bar's right end.
  $x = $cr.Right - 100; $y = $cr.Bottom - 14
  Hover $app ($x - 3) $y; Start-Sleep -Milliseconds 200; Hover $app $x $y
  Start-Sleep -Seconds 2
  for ($i = 0; $i -lt 4; $i++) {
    Hover $app $x $y
    [U50]::Capture($app.Hwnd, "$out\$Which-tip-$i.png")
    Start-Sleep -Milliseconds 2100
  }
} finally { Stop-OneTerm $app }
