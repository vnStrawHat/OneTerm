# US-0145: sample the UI thread of a OneTerm build while `hotpath-measure.ps1` drives it.
#
#   pwsh -File run-sample.ps1 -Exe <oneterm.exe> -Label smp-main-idle [-Mode Tui -Seconds 90 -Delay 25]
#
# The build needs symbols: `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`,
# `CARGO_PROFILE_RELEASE_STRIP=none` and, because fat LTO with debug info crashed rustc
# here, `CARGO_PROFILE_RELEASE_LTO=thin`, in a separate `CARGO_TARGET_DIR`. The .pdb must
# sit next to the .exe. Output: <Out>\<Label>.txt (inclusive/exclusive tables) and
# <Out>\<Label>.folded (root;...;leaf count), which `fold.py` summarises.
param([string] $Exe, [string] $Label, [ValidateSet('Idle', 'Tui')] [string] $Mode = 'Idle',
      [int] $Seconds = 60, [int] $Delay = 8,
      [string] $Out = (Join-Path ([IO.Path]::GetTempPath()) 'oneterm-us0145'))
$r = (Resolve-Path "$PSScriptRoot\..\..").Path
New-Item -ItemType Directory -Force $Out | Out-Null
$log = "$Out\$Label.log"
Remove-Item $log -ErrorAction SilentlyContinue
$job = Start-Job -ScriptBlock {
  param($r, $o, $log, $exe, $label, $mode, $secs)
  if ($mode -eq 'Idle') {
    pwsh -File "$r\hotpath-measure.ps1" -Exe $exe -Label $label -Mode Idle -IdleSeconds ($secs + 15) -Activate -Out $o -Scratch "$o\runs" *> $log
  } else {
    pwsh -File "$r\hotpath-measure.ps1" -Exe $exe -Label $label -Mode Tui -LoadSeconds ($secs + 20) -Activate -Out $o -Scratch "$o\runs" *> $log
  }
} -ArgumentList $r, $Out, $log, $Exe, $Label, $Mode, $Seconds
pwsh -File "$PSScriptRoot\sample.ps1" -LogOf $log -Seconds $Seconds -Out "$Out\$Label.txt" -SymPath (Split-Path $Exe) -Delay $Delay
Wait-Job $job | Out-Null
Get-Content $log
