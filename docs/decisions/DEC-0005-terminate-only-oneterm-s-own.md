# DEC-0005 Terminate only OneTerm's own OpenConsole.exe before a Windows update

Date: 2026-08-20

## Status

Accepted

## Context

On Windows, OneTerm's terminal backend (ConPTY) launches `OpenConsole.exe` from
the install directory via `conpty.dll`. When a pseudoconsole teardown is skipped
or deadlocks, an `OpenConsole.exe` can outlive OneTerm and keep an open handle
on the install-directory binaries. The detached update helper then cannot
overwrite those files (Windows forbids overwriting an open image), so the update
rolls back to the old build and never relaunches on the new version.

The updater must proactively terminate the offending console host, but
`OpenConsole.exe` is a shared Windows component: Windows Terminal and other apps
run their own copies. Killing by process name would terminate unrelated,
in-use console hosts belonging to other applications.

## Decision

Before scheduling the Windows update helper — while OneTerm is still running,
immediately before `cx.quit()` — enumerate processes and terminate an
`OpenConsole.exe` **only when its resolved full image path lies inside
OneTerm's own install directory**. Identification must use the process image
path (`QueryFullProcessImageNameW`, compared after `canonicalize` on both
sides), never a bare process-name match. OneTerm.exe itself is terminated by the
normal `cx.quit()`; the helper already waits for its PID to exit.

Future work that touches update-time process cleanup must preserve this
install-directory scoping and must not fall back to name-based matching.

## Alternatives

- [x] Selected: enumerate via ToolHelp, filter `OpenConsole.exe`, resolve each
  image path, and terminate only those under the install directory.
- [ ] Kill every `OpenConsole.exe` by name — rejected: would kill Windows
  Terminal / other apps' console hosts that are legitimately in use.
- [ ] Let the helper `.cmd` retry `xcopy` until the lock clears — rejected: the
  host may never exit on its own, so the update would hang or keep rolling back;
  it also could not distinguish which host holds the lock.
- [ ] Walk the process tree to kill only direct children — rejected as
  unnecessary: the orphaned host is no longer a child after OneTerm quits, and
  the install-directory path test is both simpler and sufficient.

## Consequences

- [x] Benefit to confirm: the helper can overwrite install-dir binaries, so the
  update completes and relaunches (confirmed by the reporting user after the
  manual kill, and reproduced by the automated helper run).
- [ ] Tradeoff or follow-up to address: relies on `QueryFullProcessImageNameW`
  and `canonicalize` succeeding; if the path cannot be resolved the host is left
  alone (logged), and the update may still roll back in that rare case. Requires
  the `Win32_System_Diagnostics_ToolHelp` windows-sys feature in the update crate.
