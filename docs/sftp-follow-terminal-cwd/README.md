# SFTP follow Terminal CWD — Design (split into parts)

> **Status:** Historical design record. For current SFTP and terminal paths, see [`docs/architecture.md`](../architecture.md).

Feature: **a button on the SFTP Browser to jump to the current directory (`cwd`) of the SSH
session**. The user runs `cd` in the terminal → clicks the button → SFTP follows.

The document is split into smaller parts for easier reading/review; the full merged
version is at [`../sftp-follow-terminal-cwd.md`](../sftp-follow-terminal-cwd.md).

| Part | File | Contents |
|------|------|----------|
| 1 | [`01-overview.md`](01-overview.md) | Overview, goals, requirements, OSC 7 assumptions, manual vs auto-follow |
| 2 | [`02-current-state.md`](02-current-state.md) | Codebase current state: `cwd()`, `SftpPanel.load_dir`, `active_sftp`, gap analysis |
| 3 | [`03-high-level-design.md`](03-high-level-design.md) | Architecture, `CwdSource`, data flow, button state |
| 4 | [`04-low-level-design.md`](04-low-level-design.md) | Structs, function signatures, per-crate sample code, file change table |
| 5 | [`05-edge-cases-roadmap.md`](05-edge-cases-roadmap.md) | Edge cases, risks, testing, roadmap, DoD |

> After reviewing each part, re-run the merge step to update `../sftp-follow-terminal-cwd.md`.