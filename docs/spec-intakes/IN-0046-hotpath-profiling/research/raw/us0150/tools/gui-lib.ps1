# US-0150 GUI helpers. Every function takes the hwnd of a window this script launched;
# nothing here enumerates or touches a process it did not start.
if (-not ('U50' -as [type])) {
Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public class U50 {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint flags);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
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
  public static void Capture(IntPtr h, string path) {
    RECT r; GetWindowRect(h, out r);
    using (var bmp = new Bitmap(r.Right - r.Left, r.Bottom - r.Top, PixelFormat.Format32bppArgb)) {
      using (var g = Graphics.FromImage(bmp)) { IntPtr dc = g.GetHdc(); PrintWindow(h, dc, 2); g.ReleaseHdc(dc); }
      bmp.Save(path, ImageFormat.Png);
    }
  }
  public static void CaptureScreen(int x, int y, int w, int hgt, string path) {
    using (var bmp = new Bitmap(w, hgt, PixelFormat.Format32bppArgb)) {
      using (var g = Graphics.FromImage(bmp)) { g.CopyFromScreen(x, y, 0, 0, new Size(w, hgt)); }
      bmp.Save(path, ImageFormat.Png);
    }
  }
  // Differing pixels of two PNGs inside a rectangle, and their bounding box.
  public static string Diff(string a, string b, int x0, int y0, int x1, int y1) {
    using (var A = new Bitmap(a)) using (var B = new Bitmap(b)) {
      if (A.Width != B.Width || A.Height != B.Height) return "size differs " + A.Width + "x" + A.Height + " vs " + B.Width + "x" + B.Height;
      int n = 0, minx = int.MaxValue, miny = int.MaxValue, maxx = -1, maxy = -1;
      x1 = Math.Min(x1, A.Width); y1 = Math.Min(y1, A.Height);
      for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) {
        if (A.GetPixel(x, y).ToArgb() != B.GetPixel(x, y).ToArgb()) { n++; minx = Math.Min(minx, x); miny = Math.Min(miny, y); maxx = Math.Max(maxx, x); maxy = Math.Max(maxy, y); }
      }
      return n == 0 ? "0" : n + " bbox (" + minx + "," + miny + ")-(" + maxx + "," + maxy + ")";
    }
  }
}
"@
}

function MkLp([int] $x, [int] $y) { [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)) }

# Launch one instance with a private home. Returns @{ Pid; Hwnd; Proc }.
function Start-OneTerm([string] $Exe, [string] $Home_, [string] $Theme = '', [double] $FontSize = 0,
                       [string] $Mode = '', [int] $X = 40, [int] $Y = 40, [int] $W = 1280, [int] $H = 800) {
  Remove-Item -Recurse -Force $Home_ -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force "$Home_\.OneTerm", "$Home_\cwd" | Out-Null
  '{"auto_check": false, "schema_version": 1}' | Set-Content "$Home_\.OneTerm\update_config.json"
  $ui = @{ schema_version = 1 }
  if ($Theme) { $ui.theme_name = $Theme }
  if ($FontSize -gt 0) { $ui.ui_font_size = $FontSize }
  if ($Mode) { $ui.right_dock_mode = $Mode }
  ($ui | ConvertTo-Json) | Set-Content "$Home_\.OneTerm\ui_config.json"
  $saved = @{}
  $envs = @{ USERPROFILE = $Home_; HOME = $Home_; HOTPATH_METRICS_SERVER_OFF = '1'; HOTPATH_OUTPUT_PATH = "$Home_\hotpath.json" }
  foreach ($k in $envs.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $envs[$k]) }
  try {
    $p = Start-Process -FilePath $Exe -WorkingDirectory "$Home_\cwd" -PassThru `
      -RedirectStandardOutput "$Home_\stdout.log" -RedirectStandardError "$Home_\stderr.log"
  } finally { foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) } }
  $hwnd = [IntPtr]::Zero
  for ($i = 0; $i -lt 60 -and $hwnd -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 500; $hwnd = [U50]::MainWindowOf([uint32]$p.Id) }
  if ($hwnd -eq [IntPtr]::Zero) { Stop-Process -Id $p.Id -Force; throw "no window for pid $($p.Id)" }
  [void][U50]::SetWindowPos($hwnd, [IntPtr]::Zero, $X, $Y, $W, $H, 0x0014)
  @{ Pid = $p.Id; Hwnd = $hwnd; Proc = $p }
}

function Stop-OneTerm($app) {
  [void][U50]::PostMessage($app.Hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
  if (-not $app.Proc.WaitForExit(20000)) { Stop-Process -Id $app.Pid -Force -ErrorAction SilentlyContinue }
  # Children (the shell) of the pid we launched.
  Get-CimInstance Win32_Process -Filter "ParentProcessId = $($app.Pid)" -ErrorAction SilentlyContinue |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

# Client point -> screen point.
function ToScreen($app, [int] $x, [int] $y) {
  $pt = New-Object U50+POINT; $pt.X = $x; $pt.Y = $y
  [void][U50]::ClientToScreen($app.Hwnd, [ref]$pt); $pt
}

# WM_NCHITTEST at a client point: 1 client, 2 caption, 8 min, 9 max, 20 close.
function HitTest($app, [int] $x, [int] $y) {
  # gpui answers from the hitboxes under its last known mouse position, so move there first.
  Hover $app $x $y; Start-Sleep -Milliseconds 150
  $pt = ToScreen $app $x $y
  [int][U50]::SendMessage($app.Hwnd, 0x0084, [IntPtr]::Zero, (MkLp $pt.X $pt.Y))
}

function Post($app, [uint32] $m, $w, $l) { [void][U50]::PostMessage($app.Hwnd, $m, [IntPtr]$w, [IntPtr]$l); Start-Sleep -Milliseconds 40 }
function Hover($app, [int] $x, [int] $y) { Post $app 0x200 0 (MkLp $x $y) }
function Click($app, [int] $x, [int] $y) { Post $app 0x200 0 (MkLp $x $y); Post $app 0x201 1 (MkLp $x $y); Post $app 0x202 0 (MkLp $x $y) }
function Activate($app, [bool] $on = $true) { Post $app 0x0006 ([int]$on) 0 }
