# US-0150: before vs final (v2 = title bar + status bar) with repetitions, interleaved.
param([string] $Tag = 's3')
$S = $PSScriptRoot
$measure = 'D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a7cf104c6d0581cae\docs\spec-intakes\IN-0046-hotpath-profiling\research\hotpath-measure.ps1'
$out = "$S\raw3"; New-Item -ItemType Directory -Force $out | Out-Null
$plan = @()
foreach ($round in 1..4) { foreach ($b in 'before', 'v2') { $plan += , @($b, "idleF$round", @('-Mode', 'Idle', '-IdleSeconds', '90', '-Activate')) } }
foreach ($round in 1..3) { foreach ($b in 'before', 'v2') { $plan += , @($b, "idleU$round", @('-Mode', 'Idle', '-IdleSeconds', '90', '-Inactive')) } }
foreach ($round in 1..3) { foreach ($b in 'before', 'v2') { $plan += , @($b, "tui$round", @('-Mode', 'Tui', '-LoadSeconds', '120', '-Activate')) } }
foreach ($p in $plan) {
  $b, $load, $a = $p
  $label = "us0150-$Tag-$b-$load"
  pwsh -File $measure -Exe "$S\bin-$b\oneterm.exe" -Label $label -Out $out -Scratch "$S\runs" @a *> "$out\$label.log"
  Write-Host "$label :" (Select-String -Path "$out\$label.log" -Pattern 'ui-thread steady' | ForEach-Object Line)
}
Write-Host SERIESDONE
