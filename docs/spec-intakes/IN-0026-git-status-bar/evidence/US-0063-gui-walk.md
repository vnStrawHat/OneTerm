# US-0063 GUI walk (Windows 11, `fast-dev` build, default dark theme)

Driven with posted window messages (`WM_CHAR` typing + Enter) into the default `cmd` shell
and captured with `PrintWindow`, then cropped to the status bar strip. The working tree had
the uncommitted US-0063 changes, so the branch label carries the dirty marker.

| Screenshot | What it shows |
|---|---|
| `US-0063-repo-dirty-dark.png` | After `cd /d D:\TrungKFC-Research\Rust\myTerm2`: clock, breadcrumb, then `feat/git-status-bar*`. |
| `US-0063-outside-repo-dark.png` | After `cd /d C:\Windows`: the git indicator is hidden (the separator after the breadcrumb stays, as it does for the breadcrumb itself before the first OSC 7). |

Rework (icons, same day): every `StatusText` indicator gained a leading icon.
`US-0063-icons-left-dark.png` shows clock, folder, and git-branch icons before the clock,
breadcrumb, and `feat/git-status-bar*`; `US-0063-icons-right-dark.png` shows the CPU icon
before the resource indicator. The network icon was not captured (SSH-only indicator).

Rework 2 (bright text + diffstat, same day): `US-0063-diffstat-dark.png` shows
`feat/git-status-bar* (+195 -58)` in the foreground colour with `+195` in the theme success
colour and `-58` in the danger colour; `git diff --numstat HEAD` summed to the same numbers
at capture time.

Rework 3 (all text foreground, same day): `US-0063-foreground-dark.png` and
`US-0063-foreground-right-dark.png` show the clock, breadcrumb, git label, and CPU/memory
indicator all in the theme foreground colour; the diffstat counts keep their colours.

Not walked: an SSH session (filtered by `SessionKind::Ssh` in `TerminalPanel::local_cwd`,
no real host available), a detached HEAD, and ahead/behind counts — all three label forms are
covered by the `parse_porcelain` unit test.
