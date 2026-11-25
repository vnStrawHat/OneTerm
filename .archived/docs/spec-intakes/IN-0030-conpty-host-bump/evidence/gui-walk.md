# GUI walk: local shell on the bumped console host (2026-09-12)

Build: `cargo build -p oneterm-app --profile fast-dev` with `CARGO_TARGET_DIR` pointed at a
scratch directory (never the repo `target/`). `crates/app/build.rs` copied the freshly bumped
pair next to the binary:

```text
oneterm.exe       0.5.2.0
conpty.dll        1.24.2607.10001   sha256 39fba2713e24…
x64/OpenConsole.exe 1.24.2607.10001 sha256 b7fd936c2668…
```

Machine: Windows 11 Enterprise, `ver` reports **10.0.26200.9168**. Previous bundled pair:
**1.23.2512.16003**.

## The bundled host is the one serving the shell

`Get-CimInstance Win32_Process` for OneTerm's pid (10344) with one local `cmd.exe` open:

```text
ProcessId Name            ExecutablePath
    12696 conhost.exe     C:\WINDOWS\system32\conhost.exe          (OneTerm's own debug console)
    16132 OpenConsole.exe <scratch>\target-conpty\fast-dev\x64\OpenConsole.exe
     8136 cmd.exe         C:\WINDOWS\system32\cmd.exe
```

`(Get-Process 16132).MainModule.FileVersionInfo.FileVersion` → `1.24.2607.10001`. The bundled
host, not the inbox one, hosts the shell — and `DEC-0005` still applies because that image lives
in OneTerm's own directory.

## Walk

| Step | Screenshot | Observed |
| --- | --- | --- |
| Local `cmd` opens; `echo hi`; `ver` | `US-0070-echo-and-version.png` | prompt renders, `hi` printed, `Microsoft Windows [Version 10.0.26200.9168]`. |
| `ping -t 127.0.0.1`, then a console `CTRL_C_EVENT` into that console | `US-0070-ctrl-c-interrupt.png` | ping printed its statistics, `Control-C` / `^C`; `ping.exe` gone; `cmd.exe` and OneTerm both alive; `echo after-interrupt` ran. |
| `type snake.six` (cmd, 600 x 450 libsixel image), then `echo hi after image` | `US-0070-sixel-type.png` | image rendered intact, prompt and the echo printed below it — same as the 1.23 reference `../../IN-0028-sixel-graphics/evidence/US-0067-rework-prompt-below-image.png`. **Sixel passthrough survives the bump.** |
| `cat snake.six` (Git's `cat.exe`, 32 KiB writes) | `US-0070-sixel-cat-byte-loss.png` | image rendered with the same thin speckled bands as on 1.23 (`../../IN-0028-sixel-graphics/evidence/US-0067-rework-conpty-cat-byte-loss.png`): **the 32 KiB DCS byte loss is NOT fixed in 1.24.2607.10001.** |
| 12 long lines, then restore + resize the window to ~1100 px wide | `US-0070-resize-reflow.png` | the lines re-wrap to the narrower grid and the shell keeps working (`echo resized-narrow`). |

## Caveats

- **Ctrl+C was injected as a console control event, not as a keystroke.** The workstation was
  locked during the walk (`LogonUI.exe` running, `GetForegroundWindow()` = 0), so real keyboard
  input cannot be delivered — posted `WM_KEYDOWN` does not update the modifier state GPUI reads,
  so a posted Ctrl+C is ignored by the app. The test therefore exercises the *console host's*
  ctrl-event routing (interrupt the child group, leave the shell and OneTerm alive), which is
  the property the bundled host is there for. OneTerm's own key handling (Ctrl+C → `0x03` into
  the PTY) is untouched by this work; a real-keystroke Ctrl+C on this build is **unverified**.
- The `cat` comparison is visual (same artefact pattern in the same places), not a byte-level
  re-measurement of the PTY stream; the raw-capture measurement from `IN-0028` was not repeated.
- Not walked: SSH sessions (unaffected — no ConPTY in that path), arm64 (ships no bundled pair),
  a release build, the Windows self-update path.
