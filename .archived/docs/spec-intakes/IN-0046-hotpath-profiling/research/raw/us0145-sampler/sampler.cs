using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// A poor man's sampling profiler for one thread of one process (no admin, no ETW; US-0145):
// poll the thread's cycle counter every 200 us; when it moved, suspend the thread, walk
// its stack with DbgHelp, resume, and count one sample for that stack. Stacks caught in a
// message wait are dropped when symbolised. Limits: the suspend itself advances the
// counter, so in practice every poll samples (about 290 samples a second, the walk is
// the bottleneck) and the result is a time-uniform profile of the thread's non-waiting
// stacks; the thread is slowed while it is walked; inlined frames fold into their caller.
public static class ThreadSampler {
  [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint a, bool i, uint pid);
  [DllImport("kernel32.dll")] static extern IntPtr OpenThread(uint a, bool i, uint tid);
  [DllImport("kernel32.dll")] static extern uint SuspendThread(IntPtr h);
  [DllImport("kernel32.dll")] static extern uint ResumeThread(IntPtr h);
  [DllImport("kernel32.dll")] static extern bool GetThreadContext(IntPtr h, IntPtr ctx);
  [DllImport("kernel32.dll")] static extern bool QueryThreadCycleTime(IntPtr h, out ulong c);
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern IntPtr LoadLibraryW(string n);
  [DllImport("kernel32.dll", CharSet = CharSet.Ansi)] static extern IntPtr GetProcAddress(IntPtr m, string n);
  [DllImport("winmm.dll")] static extern uint timeBeginPeriod(uint p);
  [DllImport("dbghelp.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool SymInitializeW(IntPtr h, string path, bool invade);
  [DllImport("dbghelp.dll")] static extern uint SymSetOptions(uint o);
  [DllImport("dbghelp.dll")] static extern bool StackWalk64(uint machine, IntPtr hProc, IntPtr hThread, IntPtr frame, IntPtr ctx, IntPtr readMem, IntPtr funcTable, IntPtr modBase, IntPtr translate);
  [DllImport("dbghelp.dll", CharSet = CharSet.Unicode)] static extern bool SymFromAddrW(IntPtr h, ulong addr, out ulong disp, IntPtr symInfo);

  public static string Run(int pid, int tid, int seconds, string symPath, string foldedPath) {
    timeBeginPeriod(1);
    IntPtr hP = OpenProcess(0x0410, false, (uint)pid);
    SymSetOptions(0x2 | 0x4);
    if (!SymInitializeW(hP, symPath, true)) return "SymInitialize failed " + Marshal.GetLastWin32Error();
    IntPtr hT = OpenThread(0x0002 | 0x0008 | 0x0040, false, (uint)tid);
    if (hT == IntPtr.Zero) return "OpenThread failed";
    IntPtr ctxRaw = Marshal.AllocHGlobal(1232 + 16);
    IntPtr ctx = new IntPtr((ctxRaw.ToInt64() + 15) & ~15L);
    IntPtr frame = Marshal.AllocHGlobal(1024);
    IntPtr dbg = LoadLibraryW("dbghelp.dll");
    IntPtr fta = GetProcAddress(dbg, "SymFunctionTableAccess64");
    IntPtr gmb = GetProcAddress(dbg, "SymGetModuleBase64");
    var stacks = new Dictionary<string, ulong>();
    ulong last; QueryThreadCycleTime(hT, out last);
    ulong total = 0, walked = 0; int samples = 0, bursts = 0; bool wasRunning = false;
    var sw = Stopwatch.StartNew();
    var zero = new byte[1232];
    var zf = new byte[1024];
    long periodTicks = Stopwatch.Frequency / 5000; // 200 us
    long next = sw.ElapsedTicks;
    while (sw.Elapsed.TotalSeconds < seconds) {
      next += periodTicks;
      while (sw.ElapsedTicks < next) Thread.SpinWait(20);
      ulong cyc; if (!QueryThreadCycleTime(hT, out cyc)) break;
      ulong d = cyc - last; last = cyc; total += d;
      bool ran = d >= 1000; if (ran && !wasRunning) bursts++; wasRunning = ran;
      if (!ran) continue; // it did not run since the last look: waiting
      SuspendThread(hT);
      Marshal.Copy(zero, 0, ctx, 1232);
      Marshal.WriteInt32(ctx, 0x30, 0x10000B); // CONTEXT_FULL (AMD64)
      var pcs = new List<ulong>();
      if (GetThreadContext(hT, ctx)) {
        long rip = Marshal.ReadInt64(ctx, 0xF8), rsp = Marshal.ReadInt64(ctx, 0x98), rbp = Marshal.ReadInt64(ctx, 0xA0);
        Marshal.Copy(zf, 0, frame, 1024);
        Marshal.WriteInt64(frame, 0, rip); Marshal.WriteInt32(frame, 12, 3);
        Marshal.WriteInt64(frame, 32, rbp); Marshal.WriteInt32(frame, 44, 3);
        Marshal.WriteInt64(frame, 48, rsp); Marshal.WriteInt32(frame, 60, 3);
        for (int i = 0; i < 160; i++) {
          if (!StackWalk64(0x8664, hP, hT, frame, ctx, IntPtr.Zero, fta, gmb, IntPtr.Zero)) break;
          ulong pc = (ulong)Marshal.ReadInt64(frame, 0);
          if (pc == 0) break;
          pcs.Add(pc);
        }
      }
      ResumeThread(hT);
      if (pcs.Count == 0) continue;
      string key = string.Join(",", pcs);
      ulong v; stacks.TryGetValue(key, out v); stacks[key] = v + 1;
      walked += 1; samples++;
    }
    // Symbolize.
    var names = new Dictionary<ulong, string>();
    IntPtr si = Marshal.AllocHGlobal(88 + 4096);
    Func<ulong, string> name = pc => {
      string n; if (names.TryGetValue(pc, out n)) return n;
      Marshal.Copy(new byte[88], 0, si, 88);
      Marshal.WriteInt32(si, 0, 88); Marshal.WriteInt32(si, 80, 2000);
      ulong disp;
      n = SymFromAddrW(hP, pc, out disp, si) ? Marshal.PtrToStringUni(new IntPtr(si.ToInt64() + 84), Marshal.ReadInt32(si, 76)) : ("0x" + pc.ToString("x"));
      names[pc] = n; return n;
    };
    var incl = new Dictionary<string, ulong>();
    var excl = new Dictionary<string, ulong>();
    ulong waits = 0;
    foreach (var kv in stacks.ToList()) {
      var top = kv.Key.Split(',').Take(3).Select(s => name(ulong.Parse(s)));
      if (top.Any(n => n.Contains("GetMessage") || n.Contains("WaitFor") || n.Contains("NtDelayExecution"))) { waits += kv.Value; walked -= kv.Value; stacks.Remove(kv.Key); }
    }
    foreach (var kv in stacks) {
      var fns = kv.Key.Split(',').Select(s => name(ulong.Parse(s))).ToList();
      ulong e; excl.TryGetValue(fns[0], out e); excl[fns[0]] = e + kv.Value;
      foreach (var f in fns.Distinct()) { ulong x; incl.TryGetValue(f, out x); incl[f] = x + kv.Value; }
    }
    var folded = new StringBuilder();
    foreach (var kv in stacks) {
      var fns = kv.Key.Split(',').Select(s => name(ulong.Parse(s)).Replace(";", ":")).Reverse();
      folded.Append(string.Join(";", fns)).Append(' ').Append(kv.Value).Append('\n');
    }
    File.WriteAllText(foldedPath, folded.ToString());
    var sb = new StringBuilder();
    sb.AppendFormat("thread {0}: {1:n1} Mcycles in {2:n1} s; {3} samples taken while it ran, {4} in a wait dropped, {5} busy samples; {6} wake-ups\n", tid, total / 1e6, sw.Elapsed.TotalSeconds, samples, waits, walked, bursts);
    sb.AppendLine("== inclusive (share of busy samples) ==");
    foreach (var kv in incl.OrderByDescending(k => k.Value).Take(150)) sb.AppendFormat("{0,6:p1}  {1}\n", kv.Value / (double)walked, kv.Key);
    sb.AppendLine("== exclusive ==");
    foreach (var kv in excl.OrderByDescending(k => k.Value).Take(60)) sb.AppendFormat("{0,6:p1}  {1}\n", kv.Value / (double)walked, kv.Key);
    return sb.ToString();
  }
}
