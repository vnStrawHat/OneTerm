param([string] $LogOf, [int] $Seconds = 60, [string] $Out, [string] $SymPath, [int] $Delay = 8)
# Waits for `label=... pid=N` in the measure script's output file, then samples that pid's UI thread.
$pidLine = $null
for ($i = 0; $i -lt 120 -and -not $pidLine; $i++) {
  Start-Sleep -Milliseconds 500
  $pidLine = Select-String -Path $LogOf -Pattern 'pid=(\d+)' -ErrorAction SilentlyContinue | Select-Object -First 1
}
if (-not $pidLine) { throw 'no pid' }
$appPid = [int]$pidLine.Matches[0].Groups[1].Value
Start-Sleep -Seconds $Delay
$ui = (Get-Process -Id $appPid).Threads | Sort-Object StartTime | Select-Object -First 1
Add-Type -Path (Join-Path $PSScriptRoot 'sampler.cs')
$text = [ThreadSampler]::Run($appPid, $ui.Id, $Seconds, $SymPath, ($Out -replace '\.txt$', '.folded'))
$text | Set-Content -Encoding utf8 $Out
Write-Host "sampled pid $appPid tid $($ui.Id) -> $Out"
