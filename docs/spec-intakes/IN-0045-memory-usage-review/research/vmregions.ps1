# IN-0045: walk the private committed regions of ONE pid (one you launched) with
# VirtualQueryEx and QueryWorkingSetEx, and print committed vs resident MB grouped by
# allocation (AllocationBase). Large untouched commits (ballast, preallocated
# buffers) show up as rows with a big Commit and a small Resident.
#   pwsh -File vmregions.ps1 -ProcessId <pid> [-Top 25]
param([Parameter(Mandatory)] [int] $ProcessId, [int] $Top = 25)
if (-not ('VmWalk' -as [type])) {
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public class VmWalk {
  [StructLayout(LayoutKind.Sequential)] public struct MBI {
    public IntPtr BaseAddress, AllocationBase; public uint AllocationProtect; public ushort PartitionId;
    public UIntPtr RegionSize; public uint State, Protect, Type;
  }
  [StructLayout(LayoutKind.Sequential)] public struct WSX { public IntPtr VirtualAddress; public UIntPtr Attr; }
  [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint a, bool i, uint p);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] static extern UIntPtr VirtualQueryEx(IntPtr h, IntPtr a, out MBI m, UIntPtr l);
  [DllImport("psapi.dll")] static extern bool QueryWorkingSetEx(IntPtr h, [In, Out] WSX[] b, uint cb);
  public class Alloc { public long Base; public long Commit; public long Resident; public int Regions; public string Kind; }
  public static List<Alloc> Walk(uint pid) {
    var res = new Dictionary<long, Alloc>();
    IntPtr h = OpenProcess(0x0400 | 0x0010, false, pid); // QUERY_INFORMATION | VM_READ
    if (h == IntPtr.Zero) throw new Exception("OpenProcess failed");
    try {
      long addr = 0; MBI m;
      while (VirtualQueryEx(h, (IntPtr)addr, out m, (UIntPtr)Marshal.SizeOf(typeof(MBI))) != UIntPtr.Zero) {
        long size = (long)m.RegionSize.ToUInt64();
        if (m.State == 0x1000 && m.Type == 0x20000) { // MEM_COMMIT, MEM_PRIVATE
          long b = m.AllocationBase.ToInt64(); Alloc a;
          if (!res.TryGetValue(b, out a)) { a = new Alloc { Base = b, Kind = (m.Protect & 0x100) != 0 ? "guard" : "" }; res[b] = a; }
          a.Commit += size; a.Regions++;
          int pages = (int)(size / 4096);
          var buf = new WSX[pages];
          for (int i = 0; i < pages; i++) buf[i].VirtualAddress = (IntPtr)(m.BaseAddress.ToInt64() + (long)i * 4096);
          if (QueryWorkingSetEx(h, buf, (uint)(pages * Marshal.SizeOf(typeof(WSX)))))
            for (int i = 0; i < pages; i++) if ((buf[i].Attr.ToUInt64() & 1) != 0) a.Resident += 4096;
        }
        addr = m.BaseAddress.ToInt64() + size;
        if (addr <= 0) break;
      }
    } finally { CloseHandle(h); }
    return new List<Alloc>(res.Values);
  }
}
"@
}
$all = [VmWalk]::Walk([uint32]$ProcessId)
$mb = 1MB
"pid $ProcessId private committed: {0:n1} MB, resident {1:n1} MB, allocations {2}" -f (($all | Measure-Object Commit -Sum).Sum / $mb), (($all | Measure-Object Resident -Sum).Sum / $mb), $all.Count
"by allocation size bucket (commit MB / resident MB / count):"
$all | Group-Object { if ($_.Commit -ge 32MB) { 'a >=32M' } elseif ($_.Commit -ge 8MB) { 'b 8-32M' } elseif ($_.Commit -ge 1MB) { 'c 1-8M' } else { 'd <1M' } } | Sort-Object Name | ForEach-Object {
  "  {0,-8} {1,8:n1} {2,8:n1} {3,6}" -f $_.Name, (($_.Group | Measure-Object Commit -Sum).Sum / $mb), (($_.Group | Measure-Object Resident -Sum).Sum / $mb), $_.Count
}
"top $Top allocations by commit:"
$all | Sort-Object Commit -Descending | Select-Object -First $Top | ForEach-Object {
  "  0x{0:x12} commit {1,7:n1} MB resident {2,7:n1} MB regions {3}" -f $_.Base, ($_.Commit / $mb), ($_.Resident / $mb), $_.Regions
}
