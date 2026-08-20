# High-Level Design: Windows self-update rolls back after launch and fails to relaunch

Intake: IN-0012
Lane: normal
Date: 2026-08-20

## Idea

Make the Windows in-app update reliably replace the installed files and
relaunch OneTerm, and record the whole process to a log for debugging.

Two defects broke the 0.4.0 → 0.4.1 update:

1. **Root cause — file locks.** OneTerm's terminal backend (ConPTY) launches
   `OpenConsole.exe` from the install directory. If a pseudoconsole teardown
   is skipped or deadlocks, an `OpenConsole.exe` outlives the app and keeps an
   open handle on the install-directory binaries. The detached helper's
   `xcopy` then cannot overwrite them, so it rolls the old build back — the app
   never comes back on the new version.
2. **Helper-script fragility.** The 0.4.0 helper used `xcopy /Y` (no `/R`), so
   read-only installed binaries could not be overwritten (errorlevel 4 →
   rollback), and it ran `if errorlevel 1 goto restore_backup` right after
   `start` without resetting errorlevel, so a non-fatal errorlevel from the
   prior `xcopy` could roll back a launch that actually succeeded.

Fix: before quitting, terminate only the `OpenConsole.exe` processes whose
resolved image path is inside OneTerm's install directory; harden the helper
(`/R` on every `xcopy`, `(call )` to reset errorlevel before `start`); and
write a single timestamped `update.log` across both phases.

## Diagram

```text
User clicks "Install and Restart"
        │
        ▼
apply_install_result ──► install_staged_update ──► schedule_windows_update (Rust, app still alive)
                                                        │  append_update_log(update.log): scheduled, dirs
                                                        │  terminate_console_hosts_in_dir():
                                                        │     ToolHelp snapshot → OpenConsole.exe
                                                        │     → OpenProcess → QueryFullProcessImageNameW
                                                        │     → if image under install_dir: TerminateProcess
                                                        │  write helper .cmd, spawn detached (CREATE_NO_WINDOW)
                                                        ▼
                                        cx.quit()  (OneTerm process exits)
                                                        │
                                                        ▼
                        helper .cmd  (no console; logs every step to update.log)
                          wait for OneTerm PID to exit
                          xcopy install → backup           (/E /I /H /R /Y, output → log)
                          xcopy package → install          (/E /I /H /R /Y, output → log)
                          (call )   reset errorlevel
                          start "" install\oneterm.exe
                          if errorlevel 1 → restore_backup
                          else rmdir backup+staging, del self
```

## UI Wireframe

N/A — no UI surface. The existing "Install Update" confirmation dialog and
status text are unchanged; all changes are in the post-confirmation install
mechanics and logging.

## Data Flow

1. User confirms the update; `install_staged_update` dispatches to
   `schedule_windows_update` while the app is still running.
2. Rust opens `<config>/updates/update.log` and records the scheduled version
   and the install/package directories.
3. Rust enumerates processes (ToolHelp), and for each `OpenConsole.exe` resolves
   its full image path; if it lives under the install directory it is
   terminated and the kill is logged.
4. Rust writes the helper `.cmd`, passes all paths + `ONETERM_UPDATE_LOG` via
   environment variables, spawns it detached, logs "helper launched", and quits.
5. The helper waits for the OneTerm PID to exit, backs up the install dir, copies
   the new package over it (read-only files included via `/R`), resets
   errorlevel, and starts the new binary.
6. On success it removes the backup and staging dirs and deletes itself; on any
   `xcopy`/launch failure it restores the backup and exits non-zero. Every step,
   plus raw `xcopy` output and the errorlevel on each abort, is appended to
   `update.log`.

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane; the change is localized to one file
  (`crates/update/src/install.rs`) and fully covered by the HLD data flow, the
  unit tests on the generated script, and DEC-0005. No cross-cutting concern
  warrants a separate low-level design.
