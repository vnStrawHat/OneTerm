# IN-0046 hotpath measurement protocol (Windows, PowerShell 7).
#
# Launches ONE hotpath-enabled OneTerm build with a private config home, drives it only
# through messages posted to the window of the pid it launched (IN-0045 measure.ps1
# technique and click coordinates), then closes that window so `run()` returns and
# hotpath writes its report. It never enumerates, signals or closes a process it did not
# start itself.
#
#   pwsh -File hotpath-measure.ps1 -Exe <oneterm.exe> -Label time-idle  -Mode Idle
#   pwsh -File hotpath-measure.ps1 -Exe <oneterm.exe> -Label time-flood -Mode Flood
#   pwsh -File hotpath-measure.ps1 -Exe <oneterm.exe> -Label time-tui   -Mode Tui
#   ... -AllocMetric bytes|count for a `hotpath-profiling-alloc` build.
#
# Modes (window forced to 1280x800, default single cmd.exe tab):
#   Idle   launch, idle -IdleSeconds (60). Two lengths difference out the startup
#          (US-0145: steady-state CPU and allocations per frame).
#   Flood  cmd /c "for /l %i in (1,1,300000) do @echo line %i" in the tab, wait for that
#          cmd to exit, idle 5 s.
#   Tui    a second tab, then tui-mimic.py (IN-0045) in both tabs for -LoadSeconds,
#          switching the visible tab every 10 s; wait for both to exit, idle 5 s.
# -NoLigatures writes a terminal.json with `font.ligatures: false` (US-0146's fast path).
# -NoBlink writes `cursor.blink: false` (US-0150: an idle focused window without the blink
# frames, the most a blink that does not rebuild the window could save).
# The report (hotpath JSON) goes to <Out>\<Label>.json.
param(
  [Parameter(Mandatory)] [string] $Exe,
  [Parameter(Mandatory)] [string] $Label,
  [ValidateSet('Idle', 'Flood', 'Tui')] [string] $Mode = 'Idle',
  [string] $Out = $PSScriptRoot,
  [string] $AllocMetric = '',
  [int] $LoadSeconds = 180,
  [switch] $NoLigatures,
  [switch] $NoBlink,
  [int] $IdleSeconds = 60,
  # Post WM_ACTIVATE so gpui treats the window as active (the terminal is focused and
  # its cursor blinks) without taking the real foreground from the user (US-0145).
  [switch] $Activate,
  # The opposite: post WA_INACTIVE, so an idle run measures an unfocused window (no blink)
  # whether or not Windows handed the new window the foreground.
  [switch] $Inactive,
  # Start in Agent mode (the right dock shows the Agent panel, no cards) instead of SSH Client.
  [switch] $AgentPanel,
  [string] $Scratch = (Join-Path ([IO.Path]::GetTempPath()) 'oneterm-in0046')
)
$ErrorActionPreference = 'Stop'

if (-not ('HpProbe' -as [type])) {
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class HpProbe {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("kernel32.dll")] public static extern IntPtr OpenThread(uint access, bool inherit, uint tid);
  [DllImport("kernel32.dll")] public static extern bool QueryThreadCycleTime(IntPtr h, out ulong cycles);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
  public static ulong Cycles(uint tid) {
    IntPtr h = OpenThread(0x0800, false, tid); ulong c = 0;
    if (h != IntPtr.Zero) { QueryThreadCycleTime(h, out c); CloseHandle(h); }
    return c;
  }
  public static IntPtr MainWindowOf(uint want) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      RECT r; GetWindowRect(h, out r);
      if (p == want && IsWindowVisible(h) && GetParent(h) == IntPtr.Zero && (r.Right - r.Left) > 400) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
}

function Get-Descendants([int] $root) {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CommandLine
  $out = @(); $frontier = @($root)
  while ($frontier.Count) {
    $next = @()
    foreach ($p in $frontier) {
      foreach ($k in @($all | Where-Object { $_.ParentProcessId -eq $p -and $_.ProcessId -ne $root })) { $out += $k; $next += [int]$k.ProcessId }
    }
    $frontier = $next
  }
  $out
}
function LP($x, $y) { [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)) }
function Post($m, $w, $l) { [void][HpProbe]::PostMessage($script:Hwnd, $m, [IntPtr]$w, [IntPtr]$l); Start-Sleep -Milliseconds 40 }
function Click($x, $y) { Post 0x200 0 (LP $x $y); Post 0x201 1 (LP $x $y); Post 0x202 0 (LP $x $y); Start-Sleep -Milliseconds 900 }
function Type-Text([string] $s) { foreach ($c in $s.ToCharArray()) { Post 0x102 ([int]$c) 1 }; Start-Sleep -Milliseconds 300 }
function Enter { Post 0x100 0x0D 1; Post 0x101 0x0D 1 }
function Wait-Gone([string] $pattern, [int] $minutes) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.Elapsed.TotalMinutes -lt $minutes) {
    if (-not (Get-Descendants $script:AppPid | Where-Object { $_.CommandLine -like $pattern })) { break }
    Start-Sleep -Seconds 2
  }
  $sw.Elapsed.TotalSeconds
}

$home_ = Join-Path $Scratch "home-$Label"
$work = Join-Path $Scratch "cwd-$Label"
Remove-Item -Recurse -Force $home_, $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $home_, $work, "$home_\.OneTerm", $Out | Out-Null
'{"auto_check": false, "schema_version": 1}' | Set-Content "$home_\.OneTerm\update_config.json"
if ($NoLigatures -or $NoBlink) {
  $terminal = @{ schema_version = 1 }
  if ($NoLigatures) { $terminal.font = @{ ligatures = $false } }
  if ($NoBlink) { $terminal.cursor = @{ blink = $false } }
  ($terminal | ConvertTo-Json) | Set-Content "$home_\.OneTerm\terminal.json"
}
if ($AgentPanel) { '{"right_dock_mode": "agent"}' | Set-Content "$home_\.OneTerm\ui_config.json" }
$Out = (Resolve-Path $Out).Path   # the app runs in $work, so a relative path would land there
$report = Join-Path $Out "$Label.json"
Remove-Item $report -ErrorAction SilentlyContinue

$saved = @{}
$envs = @{ USERPROFILE = $home_; HOME = $home_; HOTPATH_OUTPUT_PATH = $report; HOTPATH_OUTPUT_FORMAT = 'json-pretty'
           HOTPATH_METRICS_SERVER_OFF = '1'; HOTPATH_LIMIT = '0'; HOTPATH_ALLOC_METRIC = $AllocMetric }
foreach ($k in $envs.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $(if ($envs[$k]) { $envs[$k] } else { $null })) }
try {
  $p = Start-Process -FilePath $Exe -WorkingDirectory $work -PassThru `
    -RedirectStandardOutput (Join-Path $work 'stdout.log') -RedirectStandardError (Join-Path $work 'stderr.log')
} finally { foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) } }
$script:AppPid = $p.Id
$script:Hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 60 -and $script:Hwnd -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 500; $script:Hwnd = [HpProbe]::MainWindowOf([uint32]$p.Id) }
if ($script:Hwnd -eq [IntPtr]::Zero) { Stop-Process -Id $p.Id -Force; throw "no window for pid $($p.Id)" }
Write-Host "label=$Label pid=$($p.Id) mode=$Mode"
[void][HpProbe]::SetWindowPos($script:Hwnd, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0014)
# Re-posted every 20 s and after every tab click: a real activation change elsewhere can
# deactivate the window again. Each post costs one refresh frame, in every build alike.
function Keep-Active { if ($Activate) { Post 0x0006 1 0 } elseif ($Inactive) { Post 0x0006 0 0 } }
Start-Sleep -Seconds 2; Keep-Active
# The UI thread's CPU time without the startup (US-0145): the process's first thread,
# read after the window settled and again before it closes. hotpath's own thread table
# averages over the whole run, startup included, at 0.1 % resolution.
# CPU time is charged per 15.6 ms clock tick on Windows, too coarse for a thread at 1 %,
# so the cycle counter (QueryThreadCycleTime) is read as well.
function Ui-Thread { (Get-Process -Id $script:AppPid).Threads | Sort-Object StartTime | Select-Object -First 1 }
function Ui-CpuMs { (Ui-Thread).TotalProcessorTime.TotalMilliseconds }
Start-Sleep -Seconds 3
$uiTid = [uint32](Ui-Thread).Id
$uiCpu0 = Ui-CpuMs; $uiCyc0 = [HpProbe]::Cycles($uiTid); $uiClock = [Diagnostics.Stopwatch]::StartNew()

try {
  switch ($Mode) {
    'Idle' { for ($t = 0; $t -lt $IdleSeconds; $t += 20) { Start-Sleep -Seconds ([Math]::Min(20, $IdleSeconds - $t)); Keep-Active } }
    'Flood' {
      Start-Sleep -Seconds 5
      Type-Text 'cmd /c "for /l %i in (1,1,300000) do @echo line %i"'; Enter
      Start-Sleep -Seconds 3
      $t = Wait-Gone '*300000*' 15
      Write-Host ("flood took {0:n0} s" -f $t)
      Start-Sleep -Seconds 5
    }
    'Tui' {
      Start-Sleep -Seconds 5
      Copy-Item (Join-Path $PSScriptRoot '..\..\IN-0045-memory-usage-review\research\tui-mimic.py') "$home_\m.py"
      $load = "python `"$home_\m.py`" $LoadSeconds"
      Click 740 49; Click 615 81; Start-Sleep -Seconds 3
      Type-Text "$load 2"; Enter
      Click 80 50; Start-Sleep -Seconds 1
      Type-Text "$load 1"; Enter
      $sw = [Diagnostics.Stopwatch]::StartNew(); $tab = 1
      while ($sw.Elapsed.TotalSeconds -lt $LoadSeconds + 600) {
        Start-Sleep -Seconds 10
        if ($sw.Elapsed.TotalSeconds -gt $LoadSeconds -and -not (Get-Descendants $script:AppPid | Where-Object { $_.CommandLine -like '*m.py*' })) { break }
        if ($tab -eq 1) { Click 220 50; $tab = 2 } else { Click 80 50; $tab = 1 }
        Keep-Active
      }
      Write-Host ("load took {0:n0} s" -f $sw.Elapsed.TotalSeconds)
      Start-Sleep -Seconds 5
    }
  }
  $uiCpu = (Ui-CpuMs) - $uiCpu0; $uiCyc = [HpProbe]::Cycles($uiTid) - $uiCyc0; $uiSec = $uiClock.Elapsed.TotalSeconds
  Write-Host ("ui-thread steady: {0:n0} ms CPU in {1:n1} s = {2:n3} % of a core; {3:n1} Mcycles = {4:n2} Mcycles/s; tid {5}" -f $uiCpu, $uiSec, (100 * $uiCpu / 1000 / $uiSec), ($uiCyc / 1e6), ($uiCyc / 1e6 / $uiSec), $uiTid)
  # Close the window we own: `run()` returns and the hotpath guard writes the report.
  Post 0x0010 0 0
  if (-not $p.WaitForExit(60000)) { Write-Host 'did not exit after WM_CLOSE' }
} finally {
  $tree = @(Get-Descendants $script:AppPid)
  if (-not $p.HasExited) { Stop-Process -Id $script:AppPid -Force -ErrorAction SilentlyContinue }
  foreach ($k in $tree) { Stop-Process -Id $k.ProcessId -Force -ErrorAction SilentlyContinue }
}
if (Test-Path $report) { Write-Host "report: $report ($((Get-Item $report).Length) bytes)" } else { Write-Host 'NO REPORT' }
# US-0150: AccessKit, once a UI Automation / MSAA client touches the window, stays on and
# turns OneTerm's cached views off, so an idle run from such an instance is not comparable.
$a11y = @(Select-String -Path (Join-Path $work 'stderr.log') -Pattern 'Accessibility activated' -ErrorAction SilentlyContinue)
if ($a11y.Count) { Write-Host "a11y: ACTIVATED during the run (cached views off; flag this run): $($a11y[0].Line)" }
else { Write-Host 'a11y: not activated' }
