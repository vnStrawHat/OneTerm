# US-0150 F1: read the UI Automation tree of ONE instance this script launches, at idle,
# several times after AccessKit has activated. Windows PowerShell 5.1 (UIAutomationClient).
param([string] $Exe, [string] $Tag, [int] $Samples = 5, [int] $GapSeconds = 3, [switch] $Inactive)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\gui-lib.ps1"
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$S = $PSScriptRoot
$out = "$S\uia"; New-Item -ItemType Directory -Force $out | Out-Null
$app = Start-OneTerm -Exe $Exe -Home_ "$S\uiahome-$Tag" -Theme 'Zed One Dark' -X 200 -Y 150
$walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
function Dump($el, [int] $depth, [System.Collections.Generic.List[string]] $lines) {
  if ($depth -gt 12) { return }
  $c = $el.Current
  $lines.Add(('  ' * $depth) + $c.ControlType.ProgrammaticName.Replace('ControlType.', '') + $(if ($c.Name) { " `"$($c.Name)`"" } else { '' }))
  $child = $walker.GetFirstChild($el)
  while ($child) { Dump $child ($depth + 1) $lines; $child = $walker.GetNextSibling($child) }
}
try {
  Start-Sleep -Seconds 4
  if ($Inactive) { Activate $app $false } else { Activate $app $true }
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($app.Hwnd)
  [void]$root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)   # activates AccessKit
  Start-Sleep -Seconds 2
  $log = "$out\$Tag.txt"; Remove-Item $log -ErrorAction SilentlyContinue
  for ($i = 1; $i -le $Samples; $i++) {
    Start-Sleep -Seconds $GapSeconds   # idle frames (blink, clock) in between: reused frames
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($app.Hwnd)
    $lines = New-Object 'System.Collections.Generic.List[string]'
    Dump $root 0 $lines
    # The six title-bar nodes: MenuBar > Button "OneTerm", ToolBar > three Buttons (the toggles).
    $have = @()
    for ($k = 0; $k -lt $lines.Count; $k++) {
      $t = $lines[$k].Trim()
      if ($t -eq 'MenuBar' -or $t -eq 'Button "OneTerm"') { $have += $t }
      if ($t -eq 'ToolBar') {
        $have += $t
        $ind = $lines[$k].Length - $lines[$k].TrimStart().Length
        for ($j = $k + 1; $j -lt $lines.Count -and ($lines[$j].Length - $lines[$j].TrimStart().Length) -gt $ind; $j++) {
          if ($lines[$j].Trim().StartsWith('Button')) { $have += "ToolBar>$($lines[$j].Trim())" }
        }
      }
    }
    $summary = "sample $i : $($lines.Count) nodes; title-bar nodes present $(@($have).Count)/6 [$(@($have) -join ', ')]"
    Write-Host $summary
    Add-Content $log "== $summary"
    Add-Content $log ($lines -join "`r`n")
  }
} finally { Stop-OneTerm $app }
