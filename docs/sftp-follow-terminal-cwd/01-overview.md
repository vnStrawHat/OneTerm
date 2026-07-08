# SFTP follow Terminal CWD — Part 1: Overview & goals

> Design document for the feature: **a "Sync SFTP Browser to the current directory of the
> SSH session" button**. When the user `cd`s to another directory in the terminal, clicking
> this button makes the SFTP Browser jump to that exact directory.
>
> **Related references:**
> - [`docs/sftp-browser-design.md`](../sftp-browser-design.md) — SFTP browser design + `SftpPanel`
> - [`docs/osc-sequences-checklist.md`](../osc-sequences-checklist.md) §F — OSC 7 (CWD)
> - [`docs/terminal-backend.md`](../terminal-backend.md) — `SshSession`, `TerminalSession`
> - [`docs/agents/structure.md`](../agents/structure.md) — crate structure & dependency graph
>
> **Parts of this document** (split for readability, merged back later):
> 1. `01-overview.md` — overview & goals (this file)
> 2. `02-current-state.md` — related codebase current state
> 3. `03-high-level-design.md` — high-level design (architecture, data flow)
> 4. `04-low-level-design.md` — detailed design (structs, functions, code)
> 5. `05-edge-cases-roadmap.md` — edge cases, risks, implementation roadmap

---

## 1.1. Feature description

The SFTP Browser (right panel) and the Terminal (SSH shell, center panel) are currently **two
independent streams** running on the same SSH connection. The directory the SFTP is browsing
(`cwd` of `SftpPanel`) is **unrelated** to the directory the shell is in (`pwd` on the
remote side). If the user runs `cd /var/log` in the terminal, SFTP stays at `~`.

This feature adds **one button on the SFTP Browser toolbar**. When the user clicks:

1. Read the current directory (`cwd`) of the SSH session attached to the active terminal tab.
2. Navigate the SFTP Browser to that exact directory (`load_dir`).

This is **manual sync** (sync on demand): each time the user wants SFTP to "follow" the shell
location, they click the button. No auto-follow by default (see §1.4 for the rationale and the
optional auto-follow extension).

### Example usage flow

```
Terminal:  user@host:~$ cd /var/www/html
SFTP:      still at /home/user
           │
           └─ user clicks the [⤢ Sync to terminal] button on the SFTP toolbar
                     │
                     └─ SFTP Browser jumps to /var/www/html
```

---

## 1.2. Functional requirements

| # | Requirement | Notes |
|---|---------|---------|
| R1 | The SFTP toolbar has a "sync to terminal cwd" button | Clear icon + tooltip |
| R2 | Click the button → SFTP navigates to the `cwd` of the active SSH session | Use the existing `load_dir` |
| R3 | `cwd` is read live at click time (not a stale snapshot) | Reflects the most recent `cd` |
| R4 | If `cwd` is unavailable (no OSC 7 received yet) → button disabled + explanatory tooltip | No crash, no wrong jump |
| R5 | Local shell tab or SSH without SFTP → button not shown (or disabled) | Consistent with `render_no_connection` |
| R6 | Don't break the crate architecture: `ui` doesn't import `ssh`/`local` | Communicate via the `TerminalSession` trait |
| R7 | (Extension, optional) Toggle "auto-follow" — auto-sync each time cwd changes | Not required for the first version |

---

## 1.3. Prerequisite: OSC 7 must work

The terminal `cwd` is determined via **OSC 7** (`ESC]7;file://host/path ST`). Per
[`osc-sequences-checklist.md`](../osc-sequences-checklist.md) §F, OneTerm **already supports**
parsing OSC 7 (self-parses in parallel because `alacritty_terminal` drops it), stores it in
`SessionState.cwd`, and exposes it via `TerminalSession::cwd() -> Option<PathBuf>`.

**Key point for SSH:** OSC 7 is emitted by the **remote-side shell**. It is only present when
the remote shell is configured to emit OSC 7 (via bash's `PROMPT_COMMAND`, zsh's `precmd`/`PS1`,
or the VTE integration that many distros preinstall at `/etc/profile.d/`). If the remote shell
does **not** emit OSC 7, then `cwd()` returns `None` and the feature cannot "follow".

→ This is the **foundational assumption** of the design. Handling the missing-OSC-7 case is
in R4 (disabled button + tooltip) and is discussed in depth in `05-edge-cases-roadmap.md`.
**Actively injecting shell integration on SSH login** to guarantee OSC 7 is always present is
an extension direction, also discussed in part 05.

---

## 1.4. Manual sync vs Auto-follow

| Option | Pros | Cons |
|-----------|-----|-------|
| **Manual (button)** — first version | Simple, no wasted read_dir, user-driven | Must click each time |
| **Auto-follow (toggle)** — extension | SFTP always matches the shell automatically | Each `cd` → 1 `read_dir` (bandwidth cost), can cause unwanted "jumps" while manipulating files |

The user's original request is "click a button and SFTP follows automatically" → this is
**manual sync** by nature. Auto-follow is left as an optional on/off toggle for later (R7),
because it changes UX significantly and costs resources when the user types many `cd` commands
in a row.

---

## 1.5. Design principles

1. **Reuse existing infrastructure** — `cwd()` (trait), `load_dir()` (SftpPanel),
   the `AppState.active_sftp` pattern already exist. Don't reinvent.
2. **Live read** — read `cwd` at click time, don't cache a stale snapshot.
3. **Respect layering** — `ui` only touches `dyn TerminalSession` (in `core`), does not
   import `ssh`/`local`.
4. **Fail safe** — missing OSC 7 / no SFTP → disabled, don't jump to the wrong place.
5. **Don't touch the SFTP backend** — the feature is pure UI + 1 `cwd` data channel; don't
   modify `sftp_task`, `SftpCmd`, the protocol.