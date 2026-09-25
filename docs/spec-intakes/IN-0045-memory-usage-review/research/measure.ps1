# IN-0045 memory measurement protocol (Windows, PowerShell 7).
#
# Launches ONE OneTerm executable with a private config home, drives it only through
# messages posted to the window of the pid it launched, and records the memory
# counters of that pid (and, separately, of its descendant processes: the ConPTY
# host and the shells) after each scenario. It never enumerates, signals or closes
# any process it did not start itself.
#
#   pwsh -File measure.ps1 -Exe <path\oneterm.exe> -Label main -Mode Full   # S1..S5
#   pwsh -File measure.ps1 -Exe <path\oneterm.exe> -Label v0.6.0 -Mode Bisect # S1, S3
#   pwsh -File measure.ps1 -Exe <path\oneterm.exe> -Label s6-10k -Mode S6 -Seed <dir>
#
# Counters (GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2, values in MB = 2^20 B):
#   WS      = WorkingSetSize         resident pages, private + shared (DLL images, fonts)
#   PrivWS  = PrivateWorkingSetSize  resident private pages. Task Manager "Details" tab
#             "Memory (active private working set)" and the "Processes" tab "Memory"
#             column show this number for the process row.
#   Commit  = PrivateUsage           commit charge ("Commit size" / Private Bytes);
#             includes committed pages that were never touched.
#   Tree*   = the same counters summed over the descendants (OpenConsole/conhost, cmd).
#             The "Processes" tab groups them under the app row when it is expanded.
#
# Scenarios (window forced to 1280x800 so every build gets the same grid):
#   S1  launch, default single local shell, idle 30 s
#   S2  two more local-shell tabs ("+" then "Command Prompt", twice), idle 20 s (Full, S2)
#   S3  in the active tab: cmd /c "for /l %i in (1,1,300000) do @echo line %i",
#       wait for that cmd to exit, idle 20 s
#   S4  close the two extra tabs (their close buttons), idle 20 s    (Full only)
#   S5  idle 120 s more                                             (Full only)
#   S6  (Mode S6, after S1) a second tab, then tui-mimic.py (a claude-like TUI load) in
#       both tabs for -LoadSeconds, switching the visible tab every 10 s so both views
#       render; wait for both loads to exit, idle 20 s. Commit and private WS of the
#       app pid are also sampled every 250 ms during the load into <Label>-<Run>-trace.csv.
#       -LoadWidth/-LoadHeight resize the window once both tabs exist (a larger grid).
#
# Options: -Seed <dir> copies prepared config files (e.g. a terminal.json with a toggle
# off) into the private .OneTerm; -UpdateCheck leaves the daily update check on (it is
# off by default); -Hold <s> keeps the instance alive afterwards for vmregions.ps1.
# The click coordinates assume the v0.5+ tab bar at 1280x800 (v0.4.2 differs).
param(
  [Parameter(Mandatory)] [string] $Exe,
  [Parameter(Mandatory)] [string] $Label,
  [ValidateSet('Full', 'Bisect', 'S1', 'S2', 'S6')] [string] $Mode = 'Full',
  [string] $Csv = (Join-Path $PSScriptRoot 'measurements.csv'),
  [string] $Scratch = (Join-Path ([IO.Path]::GetTempPath()) 'oneterm-in0045'),
  [int] $Run = 1,
  [string] $Note = '',
  [string] $Seed = '',
  [switch] $UpdateCheck,
  [int] $Hold = 0,
  [int] $LoadSeconds = 180,
  [int] $LoadWidth = 0,
  [int] $LoadHeight = 0
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

if (-not ('MemProbe' -as [type])) {
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public class MemProbe {
  [StructLayout(LayoutKind.Sequential)] public struct PMC2 {
    public uint cb; public uint PageFaultCount;
    public UIntPtr PeakWorkingSetSize, WorkingSetSize, QuotaPeakPagedPoolUsage, QuotaPagedPoolUsage,
      QuotaPeakNonPagedPoolUsage, QuotaNonPagedPoolUsage, PagefileUsage, PeakPagefileUsage,
      PrivateUsage, PrivateWorkingSetSize;
    public ulong SharedCommitUsage;
  }
  [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("psapi.dll")] static extern bool GetProcessMemoryInfo(IntPtr h, out PMC2 c, uint cb);
  public static double[] Read(uint pid) {
    IntPtr h = OpenProcess(0x1000 /* QUERY_LIMITED_INFORMATION */, false, pid);
    if (h == IntPtr.Zero) return null;
    try {
      PMC2 c; if (!GetProcessMemoryInfo(h, out c, (uint)Marshal.SizeOf(typeof(PMC2)))) return null;
      const double MB = 1048576.0;
      return new double[] { c.WorkingSetSize.ToUInt64() / MB, c.PrivateWorkingSetSize.ToUInt64() / MB,
                            c.PrivateUsage.ToUInt64() / MB, c.PeakWorkingSetSize.ToUInt64() / MB };
    } finally { CloseHandle(h); }
  }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  // Top-level visible windows owned by exactly this pid (the pid we launched).
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
  # Children of the pid we launched, found by parent pid (never by name).
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

$script:Rows = @()
function Snap([string] $scenario) {
  $m = [MemProbe]::Read([uint32]$script:AppPid)
  $kids = Get-Descendants $script:AppPid
  $t = @(0.0, 0.0, 0.0)
  foreach ($k in $kids) { $km = [MemProbe]::Read([uint32]$k.ProcessId); if ($km) { for ($i = 0; $i -lt 3; $i++) { $t[$i] += $km[$i] } } }
  $proc = Get-Process -Id $script:AppPid
  $row = [pscustomobject]@{
    date = (Get-Date -Format 'yyyy-MM-dd HH:mm'); label = $Label; run = $Run; scenario = $scenario
    ws = [math]::Round($m[0], 1); privws = [math]::Round($m[1], 1); commit = [math]::Round($m[2], 1); peakws = [math]::Round($m[3], 1)
    threads = $proc.Threads.Count; handles = $proc.HandleCount
    children = $kids.Count; tree_ws = [math]::Round($t[0], 1); tree_privws = [math]::Round($t[1], 1); tree_commit = [math]::Round($t[2], 1)
    note = $Note
  }
  $script:Rows += $row
  $row | Export-Csv -Path $Csv -Append -NoTypeInformation
  $row | Format-Table -AutoSize | Out-String | Write-Host
}

function LP($x, $y) { [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)) }
function Post($m, $w, $l) { [void][MemProbe]::PostMessage($script:Hwnd, $m, [IntPtr]$w, [IntPtr]$l); Start-Sleep -Milliseconds 40 }
# Posted key-downs cannot hold Ctrl (gpui reads the modifier state, which a posted
# message does not set), so tabs are opened and closed with posted mouse clicks.
function Click($x, $y) {
  Post 0x200 0 (LP $x $y); Post 0x201 1 (LP $x $y); Post 0x202 0 (LP $x $y); Start-Sleep -Milliseconds 900
}
function Cap([string] $name) {
  $bmp = New-Object System.Drawing.Bitmap(1280, 800)
  $g = [System.Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [void][MemProbe]::PrintWindow($script:Hwnd, $dc, 2); $g.ReleaseHdc($dc); $g.Dispose()
  $bmp.Save((Join-Path $work "$name.png")); $bmp.Dispose()
}
function Type-Text([string] $s) { foreach ($c in $s.ToCharArray()) { Post 0x102 ([int]$c) 1 }; Start-Sleep -Milliseconds 300 }
function Enter { Post 0x100 0x0D 1; Post 0x101 0x0D 1 }

# ---- launch with a private home --------------------------------------------------
$home_ = Join-Path $Scratch "home-$Label-$Run"
$work = Join-Path $Scratch "cwd-$Label-$Run"
Remove-Item -Recurse -Force $home_, $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $home_, $work, "$home_\.OneTerm" | Out-Null
# The automatic update check is off unless -UpdateCheck: it runs at most once a day for a
# real user, and a GitHub round-trip in every run would add noise between builds.
if (-not $UpdateCheck) { '{"auto_check": false, "schema_version": 1}' | Set-Content "$home_\.OneTerm\update_config.json" }
# -Seed <dir>: copy prepared config files (e.g. a terminal.json with a toggle off).
if ($Seed) { Copy-Item "$Seed\*" "$home_\.OneTerm\" -Force }
$savedUP = $env:USERPROFILE; $savedHOME = $env:HOME
$env:USERPROFILE = $home_; $env:HOME = $home_   # release config_dir() = %USERPROFILE%\.OneTerm
try {
  $p = Start-Process -FilePath $Exe -WorkingDirectory $work -PassThru `
    -RedirectStandardOutput (Join-Path $work 'stdout.log') -RedirectStandardError (Join-Path $work 'stderr.log')
} finally { $env:USERPROFILE = $savedUP; $env:HOME = $savedHOME }
$script:AppPid = $p.Id
$script:Hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 60 -and $script:Hwnd -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 500; $script:Hwnd = [MemProbe]::MainWindowOf([uint32]$p.Id) }
if ($script:Hwnd -eq [IntPtr]::Zero) { Stop-Process -Id $p.Id -Force; throw "no window for pid $($p.Id)" }
Write-Host "label=$Label pid=$($p.Id) hwnd=$($script:Hwnd) home=$home_"
[void][MemProbe]::SetWindowPos($script:Hwnd, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0014)

try {
  Start-Sleep -Seconds 30
  Snap 'S1'; Cap 'S1'
  if ($Mode -eq 'S1') { return }

  if ($Mode -eq 'S6') {
    Copy-Item (Join-Path $PSScriptRoot 'tui-mimic.py') "$home_\m.py"
    $load = "python `"$home_\m.py`" $LoadSeconds"
    Click 740 49; Click 615 81; Start-Sleep -Seconds 3     # tab 2 (now visible)
    if ($LoadWidth -gt 0) { [void][MemProbe]::SetWindowPos($script:Hwnd, [IntPtr]::Zero, 0, 0, $LoadWidth, $LoadHeight, 0x0014); Start-Sleep -Seconds 2 }
    Type-Text "$load 2"; Enter
    Click 80 50; Start-Sleep -Seconds 1                     # back to tab 1
    Type-Text "$load 1"; Enter
    $trace = @(); $sw = [Diagnostics.Stopwatch]::StartNew(); $next = 10; $tab = 1
    while ($sw.Elapsed.TotalSeconds -lt $LoadSeconds + 600) {
      $m = [MemProbe]::Read([uint32]$script:AppPid)
      $trace += [pscustomobject]@{ t = [math]::Round($sw.Elapsed.TotalSeconds, 2); privws = [math]::Round($m[1], 1); commit = [math]::Round($m[2], 1) }
      if ($sw.Elapsed.TotalSeconds -ge $next) {
        $next += 10
        if ($sw.Elapsed.TotalSeconds -gt $LoadSeconds -and -not (Get-Descendants $script:AppPid | Where-Object { $_.CommandLine -like '*m.py*' })) { break }
        if ($tab -eq 1) { Click 220 50; $tab = 2 } else { Click 80 50; $tab = 1 }
      }
      Start-Sleep -Milliseconds 250
    }
    $trace | Export-Csv -Path (Join-Path $work "$Label-$Run-trace.csv") -NoTypeInformation
    Write-Host ("load took {0:n0} s; trace in $work" -f $sw.Elapsed.TotalSeconds)
    Start-Sleep -Seconds 20
    Snap 'S6'; Cap 'S6'
    return
  }

  if ($Mode -in 'Full', 'S2') {
    # Tab-bar "+" (x=740,y=49 at 1280x800), then its "Command Prompt" row, twice.
    for ($n = 0; $n -lt 2; $n++) { Click 740 49; Click 615 81; Start-Sleep -Seconds 3 }
    Start-Sleep -Seconds 20
    Snap 'S2'; Cap 'S2'
    if ($Mode -eq 'S2') { return }
  }

  $cmd = 'cmd /c "for /l %i in (1,1,300000) do @echo line %i"'
  Type-Text $cmd; Enter
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Start-Sleep -Seconds 3
  while ($sw.Elapsed.TotalMinutes -lt 15) {
    $busy = Get-Descendants $script:AppPid | Where-Object { $_.CommandLine -like '*300000*' }
    if (-not $busy) { break }
    Start-Sleep -Seconds 2
  }
  Write-Host ("fill took {0:n0} s" -f $sw.Elapsed.TotalSeconds)
  Start-Sleep -Seconds 20
  Snap 'S3'; Cap 'S3'
  if ($Mode -eq 'Bisect') { return }

  # The close buttons of the third and second "Command Prompt" tabs (156 px each).
  Click 458 50; Start-Sleep -Seconds 2; Click 302 50
  Start-Sleep -Seconds 20
  Snap 'S4'; Cap 'S4'
  Start-Sleep -Seconds 120
  Snap 'S5'
} finally {
  # -Hold <s>: keep the instance alive after the last scenario (for vmregions.ps1).
  if ($Hold -gt 0) { Write-Host "holding pid $($script:AppPid) for $Hold s"; Start-Sleep -Seconds $Hold }
  # Only the pid we launched and the descendants it spawned.
  $tree = @(Get-Descendants $script:AppPid)
  Stop-Process -Id $script:AppPid -Force -ErrorAction SilentlyContinue
  foreach ($k in $tree) { Stop-Process -Id $k.ProcessId -Force -ErrorAction SilentlyContinue }
}
