# US-0150 window-control walk on ONE instance this script launches.
# Part 1 (no real input): WM_NCHITTEST at every title-bar area after idle frames.
# Part 2 (-Real): the real mouse, on this window only, made topmost; every input is
# preceded by a WindowFromPoint check that the point belongs to this window.
param([string] $Which = 'after', [switch] $Real, [string] $Theme = 'Zed One Dark')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\gui-lib.ps1"
$S = $PSScriptRoot
$out = "$S\walk"; New-Item -ItemType Directory -Force $out | Out-Null
$app = Start-OneTerm -Exe "$S\bin-$Which\oneterm.exe" -Home_ "$S\walkhome-$Which" -Theme $Theme -X 200 -Y 150
Write-Host "$Which pid $($app.Pid)"
$cr = New-Object U50+RECT; [void][U50]::GetClientRect($app.Hwnd, [ref]$cr)
$W = $cr.Right
$names = @{ 1 = 'client'; 2 = 'caption'; 8 = 'min'; 9 = 'max'; 20 = 'close' }
$points = [ordered]@{
  'empty caption (x=500)' = @(500, 17); 'empty caption (x=900)' = @(900, 17)
  'minimise' = @(($W - 85), 17); 'maximise' = @(($W - 51), 17); 'close' = @(($W - 17), 17)
  'terminal (x=500,y=300)' = @(500, 300)
  'app menu (x=45)' = @(45, 17); 'SSH Client toggle' = @(($W - 102 - 8 - 190 + 35), 17); 'None toggle' = @(($W - 102 - 8 - 25), 17)
}
function Report([string] $when) {
  $line = foreach ($k in $points.Keys) { $p = $points[$k]; $c = HitTest $app $p[0] $p[1]; "$k=$($names[$c])" }
  Write-Host "$when : $($line -join '; ')"
}
try {
  Activate $app $true
  Start-Sleep -Seconds 4   # a few blink frames: the cached content is being reused
  Report 'after 4 s idle (focused)'
  # A frame driven by hovering the toggles, then more idle frames.
  Hover $app ($W - 102 - 8 - 190 + 35) 17; Start-Sleep -Milliseconds 400
  Hover $app 500 300; Start-Sleep -Seconds 2
  Report 'after toggle hover + 2 s'
  # Double-click the caption through the OS path the hit test selects.
  $pt = ToScreen $app 500 17
  Post $app 0x00A3 2 (MkLp $pt.X $pt.Y)   # WM_NCLBUTTONDBLCLK, HTCAPTION
  Start-Sleep -Seconds 1
  Write-Host "posted caption double-click -> maximised: $([U50]::IsZoomed($app.Hwnd))"
  Report 'maximised'
  Post $app 0x00A3 2 (MkLp $pt.X $pt.Y); Start-Sleep -Seconds 1
  Write-Host "second double-click -> maximised: $([U50]::IsZoomed($app.Hwnd))"
  [void][U50]::SetWindowPos($app.Hwnd, [IntPtr]::Zero, 200, 150, 1280, 800, 0x0014)
  Start-Sleep -Milliseconds 500

  if ($Real) {
    $orig = New-Object U50+POINT; [void][U50]::GetCursorPos([ref]$orig)
    [void][U50]::SetWindowPos($app.Hwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0013)   # topmost, no move/size/activate
    Start-Sleep -Milliseconds 300
    function Mine([int] $cx, [int] $cy) {
      $p = ToScreen $app $cx $cy
      $hit = [U50]::GetAncestor([U50]::WindowFromPoint($p), 2)
      if ($hit -ne $app.Hwnd) { throw "point ($cx,$cy) is not over our window" }
      $p
    }
    try {
      # Hover close: the kit paints it with the danger colour.
      $p = Mine ($W - 17) 17; [void][U50]::SetCursorPos($p.X, $p.Y); Start-Sleep -Milliseconds 150
      [void][U50]::SetCursorPos($p.X, $p.Y + 1); Start-Sleep -Milliseconds 600
      [U50]::Capture($app.Hwnd, "$out\$Which-hover-close.png")
      # Hover maximise and wait for the Windows 11 snap-layout flyout.
      $p = Mine ($W - 51) 17; [void][U50]::SetCursorPos($p.X, $p.Y); Start-Sleep -Milliseconds 100
      [void][U50]::SetCursorPos($p.X, $p.Y + 1); Start-Sleep -Milliseconds 1800
      $r = New-Object U50+RECT; [void][U50]::GetWindowRect($app.Hwnd, [ref]$r)
      [U50]::CaptureScreen($r.Right - 420, $r.Top, 420, 260, "$out\$Which-hover-max-screen.png")
      # Drag the caption 120 px right and 60 px down.
      $p = Mine 500 17
      $before = New-Object U50+RECT; [void][U50]::GetWindowRect($app.Hwnd, [ref]$before)
      [void][U50]::SetCursorPos($p.X, $p.Y); Start-Sleep -Milliseconds 100
      [U50]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 100
      for ($i = 1; $i -le 12; $i++) { [void][U50]::SetCursorPos($p.X + 10 * $i, $p.Y + 5 * $i); Start-Sleep -Milliseconds 30 }
      [U50]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 400
      $after = New-Object U50+RECT; [void][U50]::GetWindowRect($app.Hwnd, [ref]$after)
      Write-Host "real drag: window moved by ($($after.Left - $before.Left), $($after.Top - $before.Top))"
      # Real double-click on the caption: maximise, then restore.
      $p = Mine 500 17; [void][U50]::SetCursorPos($p.X, $p.Y); Start-Sleep -Milliseconds 200
      foreach ($n in 1, 2) { [U50]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); [U50]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60 }
      Start-Sleep -Seconds 1
      Write-Host "real double-click -> maximised: $([U50]::IsZoomed($app.Hwnd))"
      Start-Sleep -Milliseconds 1500
      $p = Mine 500 17; [void][U50]::SetCursorPos($p.X + 3, $p.Y); Start-Sleep -Milliseconds 300
      [void][U50]::SetCursorPos($p.X, $p.Y); Start-Sleep -Milliseconds 300
      foreach ($n in 1, 2) { [U50]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); [U50]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60 }
      Start-Sleep -Seconds 1
      Write-Host "real double-click again -> maximised: $([U50]::IsZoomed($app.Hwnd))"
      if ([U50]::IsZoomed($app.Hwnd)) { [void][U50]::ShowWindow($app.Hwnd, 9); Start-Sleep -Seconds 1 }
      [void][U50]::GetClientRect($app.Hwnd, [ref]$cr); $W = $cr.Right
      # A real click on the Agent toggle: the cached toggles still take clicks.
      $p = Mine ($W - 102 - 8 - 190 + 105) 17; [void][U50]::SetCursorPos($p.X, $p.Y); Start-Sleep -Milliseconds 200
      [U50]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 50; [U50]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
      Start-Sleep -Seconds 1
      [U50]::Capture($app.Hwnd, "$out\$Which-after-agent-click.png")
      $cfg = Get-Content "$S\walkhome-$Which\.OneTerm\ui_config.json" -Raw
      Write-Host "ui_config after the Agent click: $($cfg -replace '\s+', ' ')"
    } finally {
      [void][U50]::SetCursorPos($orig.X, $orig.Y)
      [void][U50]::SetWindowPos($app.Hwnd, [IntPtr](-2), 0, 0, 0, 0, 0x0013)
    }
  }
} finally { Stop-OneTerm $app }
