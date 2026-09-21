# Work: The "Run as administrator" rows and the documentation

ID: US-0132
Intake: IN-0043
Created: 2026-09-21

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (the user-reachable half) + existing-contract change (four
  owning documents)
- Risk lane: **high_risk**
- Risks, named so the review knows what to look at:
  1. **This is the packet that opens the door.** Everything before it is mechanism nobody
     can reach. Accepting this one makes a UAC prompt one click away from the "+" button,
     so the whole of `US-0130` and `US-0131` is re-verified here, not re-assumed.
  2. **A hit target that elevates by accident.** "Open a shell" and "open an elevated
     shell" must never be the same click, and the elevated entry must never be the default
     or the first thing under the cursor.
  3. **Reordering the owner-fixed rows.** The "+" menu's order below the shells was settled
     by owner decision during `US-0094` acceptance (`docs/gui-layout.md:104`, `IN-0033`).
     Moving anything below the shell block is a regression against an accepted decision,
     not a style choice.
  4. **Stale owning contracts.** Three documents currently describe behaviour
     unconditionally that is now conditional. Leaving them is how the next reader
     confidently derives the wrong thing.
- Spec Intake: `IN-0043` (Run a Windows shell as administrator)
- Decision inherited: `DEC-0019` (Accepted 2026-09-21, option A with M1-M7)
- Depends on: `US-0131` (and transitively `US-0130`)

## Outcome

A Windows user can start an elevated shell from the "+" (New Terminal) menu, and the
project's documentation says what that window is and what it costs.

- The "+" menu offers **one "Run as administrator" entry per Windows shell** — three
  entries, one each for Command Prompt, PowerShell and PowerShell 7 — reached either as
  rows under a labelled separator or through a `Run as administrator >` submenu. Either
  shape is acceptable; **neither may reorder the rows below the shell block**, whose order
  is owner-fixed.
- Picking one shows a UAC prompt and then a second, marked OneTerm window with that shell
  open in it. The window the user was working in keeps its tabs, its connections and its
  transfers, unelevated.
- In an already elevated window the entries are **absent**: every shell there is already
  administrator, and a `runas` from an elevated process would produce a second identical
  window.
- `docs/gui-layout.md`, `docs/terminal-backend.md`, `docs/auto-update.md` and
  `docs/crash-reporting.md` each describe the elevated window's behaviour in their own
  area, and a user-facing note says plainly what the elevated window does not have.

## Scope

- [ ] In scope:
  - `crates/terminal-view/src/panel/terminal_panel.rs:692-767` — the `#[cfg(windows)]`
    entries, placed with the shell block at the top, calling the `launch_elevated_shell`
    fn pointer added in `US-0130`; absent when `is_elevated()`.
  - `terminal_panel.rs:759-765` — `FIXED_ROWS` and the scroll estimate updated for the
    rows actually emitted, in both modes. (`ROW_HEIGHT` and the estimate's existing
    `ponytail:` caveat at `:754-758` are unchanged; this only corrects the count.)
  - The row label wording, from `ShellKind::display_name`
    (`crates/core/src/config/shell.rs:40-50`) — the single wording source `US-0114`
    established, so the entry, the tab it opens and the Settings dropdown cannot disagree.
  - `docs/gui-layout.md:104` — the "+" menu described top to bottom gains the elevated
    entries in their place, and the paragraph on the right dock gains the elevated
    window's subset.
  - `docs/terminal-backend.md` sections 6.1 and 6.2 — why an elevated child cannot join
    this process's pseudo-console, and the trusted-path resolution that replaces
    `COMSPEC` / `PATH` for an elevated shell.
  - `docs/auto-update.md` — one paragraph in "Installation behavior": the elevated
    instance never checks, downloads or installs, and why (an elevated helper can write
    `C:\Program Files` and leave files the ordinary instance cannot replace).
  - `docs/crash-reporting.md` — "Capture boundary" and "Recovery lifecycle": the elevated
    store is `crashes/elevated/` and the elevated window shows no recovery dialog, while
    promotion and the newest-20 retention still run there.
  - The user-facing note: what the elevated window has (local Windows shells, the user's
    theme and key bindings) and what it does not (SSH, SFTP, saved sessions, the Agent
    panel, updates, configuration writes, Explorer drag-and-drop).
- [ ] Out of scope:
  - The launch, the command line and the trusted paths (`US-0130`); the marker, the
    restricted mode, the updater shutdown, the config matrix and the crash store
    (`US-0131`).
  - A modifier click (Shift-click a shell row to elevate it). Rejected in the HLD and
    again here: invisible, undiscoverable, it makes "open a shell" and "open an elevated
    shell" the same hit target, and it cannot be exercised by this project's GUI walks.
  - A "Restart as administrator" row. A reasonable later addition; a poor answer to "I
    want an admin shell", and `DEC-0019` rejected it as option C.
  - Any non-Windows menu change. The non-Windows arm
    (`terminal_panel.rs:700-701`) is untouched and renders nothing new.
  - A key binding for the elevated entries. Nothing asked for one, and an unbound
    accelerator that raises a consent prompt is not a feature worth guessing at.

## Acceptance

- [ ] On Windows, the "+" menu offers exactly one elevated entry per Windows shell, using
      the same wording as the ordinary rows (`ShellKind::display_name`), clearly marked as
      "Run as administrator".
- [ ] **The rows below the shell block are in their existing order**, unchanged: the
      "SSH Sessions" labelled separator, the ungrouped saved sessions, each group behind
      its dashed separator, the plain separator, "Quick Connect...", "New Saved
      Session...". Verified against `docs/gui-layout.md:104` clause by clause, because that
      order is an accepted owner decision.
- [ ] Opening an ordinary shell row is unchanged: one click, no prompt, a tab in this
      window. The elevated entry is a **different** target — no modifier, no long-press, no
      overlap.
- [ ] Picking an elevated entry shows a UAC prompt naming `oneterm.exe`. Accepting opens a
      second OneTerm window, marked, with that shell open in it, and `whoami /groups` there
      reports `Mandatory Label\High Mandatory Level`.
- [ ] Declining leaves everything exactly as it was: no window, no notification, no change
      to the launching window's tabs, SSH connections or in-flight transfers.
- [ ] In an already elevated window the elevated entries are **absent** — not disabled,
      not greyed.
- [ ] The popup scrolls (and so shows a scrollbar) only when its rows really exceed the
      kit's height cap, in both modes and with a long saved-session list: `FIXED_ROWS`
      matches the rows emitted.
- [ ] On Linux and macOS the "+" menu is byte-for-byte what it is today.
- [ ] `docs/gui-layout.md:104` describes the menu top to bottom **including** the new
      entries, and describes the elevated window's right dock.
- [ ] `docs/terminal-backend.md` sections 6.1/6.2 state why the elevated shell needs a
      second process and how its program is resolved.
- [ ] `docs/auto-update.md` states that the elevated instance never updates and why.
- [ ] `docs/crash-reporting.md` states the `crashes/elevated/` store and the suppressed
      dialog.
- [ ] The user-facing note lists, in plain words, what the elevated window does not have —
      including that drag-and-drop from Explorer does not work in an elevated window
      (UIPI; `crates/sftp-ui/src/render.rs:436-451`), which matters to anyone who elevates
      OneTerm by hand.
- [ ] `python scripts/check-doc-paths.py` and `python scripts/check-english.py` pass —
      `docs/gui-layout.md` and `docs/terminal-backend.md` are both in the path checker's
      document set.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` — line 104, the "+" menu top to bottom and the owner-fixed order
  settled during `US-0094` acceptance; the right-dock mode toggles; the persisted
  panel-name contract. **Changes with this packet.**
- `docs/terminal-backend.md` sections 6.1 (Configurable shell, line 390) and 6.2 (Spawn via
  `oneterm_vt::pty`, line 453). **Changes with this packet.**
- `docs/auto-update.md` — "Update check flow" (line 275) and "Installation behavior"
  (line 307, the Windows helper and the rollback copy under
  `<config>/updates/backup-<pid>-<ts>`). **Changes with this packet.**
- `docs/crash-reporting.md` — "Capture boundary", "Reconciliation and retention",
  "Recovery lifecycle". **Changes with this packet.**
- `docs/decisions/DEC-0019-elevated-shells-open-in-an-elevated-window.md` — the
  consequences list is the source of the user-facing note's content.
- `docs/spec-intakes/IN-0043-run-shell-as-administrator/low-level-design/elevated-instance.md`
  — section 7's M1 rows (the menu), section 8 (what a declined prompt does).
- `docs/spec-intakes/IN-0042-ux-polish-round-1/US-0114-tabs-named-after-their-shell-and-menu-names-its-dialog.md`
  — the one-wording-source rule the entry labels must follow.
- `docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/US-0094-new-terminal-menu-lists-saved-ssh-sessions.md`
  — the `WorkspaceCommands` seam and the menu order the owner settled at its acceptance.
- `docs/agents/code-style.md` — menu construction and `#[cfg]` conventions.

### Documentation Action

**Update required.** Four owning documents change, and they are the reason this packet is
not "just a menu row":

- `docs/gui-layout.md:104` — the elevated entries in the top-to-bottom description, and the
  elevated window's reduced right dock.
- `docs/terminal-backend.md` sections 6.1/6.2 — the trusted-path resolution and the
  pseudo-console constraint.
- `docs/auto-update.md` "Installation behavior" — the elevated instance does not update.
- `docs/crash-reporting.md` "Capture boundary" and "Recovery lifecycle" — the split store
  and the suppressed dialog.

Plus the user-facing note. This packet also carries the documentation `US-0130` and
`US-0131` deliberately deferred, which is why it is the last of the three and why its
reconciliation covers all four documents rather than only the menu.

Reason: three of these documents currently state unconditionally something that becomes
conditional, and the fourth describes a menu that grows entries. All four are
user-reachable contracts and all four go stale on the day this packet ships.

### Reconciliation

Before completion, list the four documents as changed, confirm the user-facing note exists,
and confirm `docs/gui-layout.md`'s top-to-bottom menu description matches the built menu
clause by clause in both modes.

**Done.** Four documents changed, plus the user-facing note:

| Document | What changed |
| --- | --- |
| `docs/gui-layout.md` | the top-to-bottom "+" menu description names the `Run as administrator ›` row in its place; the owner-fixed-order sentence now says *why* a submenu at the top is what keeps that order, what picking one does, and that the row and the whole SSH block are absent in an elevated window; the dock-composition section gains the elevated window's reduced dock and its three markers |
| `docs/terminal-backend.md` | new section 6.1.1, "Elevated windows resolve their shell from a trusted table" — the three paths, why not `%COMSPEC%` / `PATH` / `terminal.json`, why the resolution runs in the elevated process, the stated cost of a pwsh outside `%ProgramFiles%`, and the two exit codes; section 6.2 gains the "why an elevated shell needs a second process" note (`PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` vs `ShellExecuteEx`) |
| `docs/auto-update.md` | "Installation behavior" opens with M2: an elevated window never checks, downloads or installs, and *why* — the helper under an administrator token can leave files the ordinary instance cannot replace |
| `docs/crash-reporting.md` | "Capture boundary" gains `crashes/elevated/` and the reason (an ACL the medium-integrity instance cannot delete would wedge pruning); "Recovery lifecycle" gains the suppressed dialog, with promotion and retention still running there |
| `README.md` (the user-facing note) | a "Run as administrator (Windows)" section: what the elevated window is, that it is marked from the token, what it has, what it does not have, that it writes no settings, and that Explorer drag-and-drop does not work into any elevated window. The Auto-update and Crash-reporting bullets cross-reference it |

`docs/gui-layout.md`'s description was checked clause by clause against the built menu in
the unelevated mode (`evidence/US-0132-plus-menu-run-as-admin.png`): three shells, the
`Run as administrator ›` row, the "SSH Sessions" labelled separator, the sessions (here the
disabled "No saved sessions" hint), the plain separator, "Quick Connect...", "New Saved
Session...". The elevated mode's clause — three shells and nothing else — is covered by the
`menu_rows` unit test and by the owner's checklist step 4 in `US-0131`.

## Context

- **The menu is built on every open**, in `TerminalPanel::title_suffix`
  (`terminal_panel.rs:681-767`), so no state is cached and nothing needs invalidating when
  elevation status or the saved-session list changes.
- **The row order below the shells is owner-fixed** (`docs/gui-layout.md:104`, settled in
  `US-0094` acceptance). Putting the elevated entries with the shell block at the top is
  what keeps them out of that order; anything that pushes "SSH Sessions" or the two closing
  rows around is a regression against an accepted decision.
- **`FIXED_ROWS = 7`** (`terminal_panel.rs:763`) is 3 shells + the "SSH Sessions" heading +
  the closing separator + the two closing rows. In an elevated window the emitted set is
  3 shells and nothing else; in a normal window it grows by the elevated entries. The
  constant becomes a computed count rather than a literal.
- **A submenu costs one row; three rows cost three.** Either satisfies the owner's
  constraint, and the deciding factor is how the real menu reads with a long saved-session
  list, which is why the choice is made here and not in the design
  (`IN-0043.md` Open Decisions).
- **The wording source is `ShellKind::display_name`** (`shell.rs:40-50`), by `US-0114`. The
  elevated entry for `Pwsh` reads "PowerShell 7", the same as the ordinary row and the tab
  it opens.
- **Labelled separators already exist** (`terminal_panel.rs:39-66`, `labelled_separator`
  with `SeparatorRule::Solid` / `Dashed`), composed from a disabled `PopupMenuItem::element`
  because the kit's `PopupMenu` has a separator and a label but not one item that is both.
  If the rows-under-a-separator shape is chosen, this is the helper to use — nothing new is
  written.
- **A shield glyph would be a new asset.** `crates/theme/assets/icons/` has no shield SVG,
  and `crates/theme/build.rs` generates the `AppIcon` variant from whatever is dropped in.
  The gpui-kit `IconName` (Lucide) set is the other option. Either way, the entry must read
  correctly with no icon at all: the text is the marker, the glyph is decoration.

## Plan

- [ ] Build both shapes behind the same fn-pointer call and look at each against a real
      menu with a long saved-session list; keep the one that reads better, delete the
      other, and record the choice and the reason in `IN-0043.md` Open Decisions.
- [ ] `FIXED_ROWS` becomes a computed count.
- [ ] Suppress the entries when `is_elevated()`.
- [ ] The four documentation updates and the user-facing note.
- [ ] Full manual E2E (E1-E10 of the detail design), because this is the packet that makes
      the feature reachable; `pwsh scripts/ci-local.ps1`.

## Decisions

- `DEC-0019` — option A and mitigation M1 (the elevated window offers no elevated entries
  of its own). Not repeated here.
- The presentation shape (rows versus submenu) is settled **in this packet** and recorded
  in `IN-0043.md`; it does not need a `DEC` of its own, because nothing downstream inherits
  it.

## Verification Plan

Focused / unit (`cargo test -p oneterm-terminal-view`):

- [ ] The menu row count (`FIXED_ROWS` + session rows) equals the rows actually emitted, in
      both modes and for an empty, a short and a long saved-session list.
- [ ] The elevated entries are emitted on Windows when not elevated and **not** emitted
      when elevated.
- [ ] The entry label for each kind equals `ShellKind::display_name(kind)` — the `US-0114`
      one-wording-source test.
- [ ] Every entry dispatches `launch_elevated_shell` with the matching `ShellKind`, and no
      entry exists for a kind `ElevatedShell::from_shell_kind` rejects.

Integration:

- [ ] `cargo test --workspace`.

E2E (manual, Windows interactive desktop). The full list from the detail design, because
this packet is the one that exposes the feature. Evidence in `evidence/`:

- [ ] **E1** — the menu with the entries in place and the rows below in their fixed order;
      the UAC prompt naming `oneterm.exe`.
- [ ] **E2** — the elevated window carrying all three markers.
- [ ] **E3** — `whoami /groups` showing `Mandatory Label\High Mandatory Level`.
- [ ] **E4** — the elevated window's "+" menu: three shells, no elevated entries, no SSH
      block; and no right dock or mode toggles.
- [ ] **E5** — the normal window untouched: tabs alive, a live SSH connection still
      responding.
- [ ] **E6** — declined prompt: no window, no notification, no change.
- [ ] **E7** — the five configuration documents byte-identical before and after an elevated
      session.
- [ ] **E8** — the elevated About dialog's one-line updates text.
- [ ] **E9** — a crash report in `crashes/elevated/`, no dialog in the elevated window.
- [ ] **E10** — `--elevated-shell bash` and `--wat`: one message box each, exit code 2, no
      window.
- [ ] The menu with a saved-session list long enough to scroll, in both modes, confirming
      the scrollbar appears only when the rows really do not fit.

Platform / release:

- [ ] `pwsh scripts/ci-local.ps1` green, including `python scripts/check-doc-paths.py`
      (both changed documents are in its set) and `python scripts/check-english.py`.
- [ ] Linux and macOS build and test green with the menu unchanged there.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### The presentation shape, settled here

**One `Run as administrator ›` submenu row**, placed directly after the three Windows shell
rows and before the "SSH Sessions" separator, listing the same three shells with the same
`ShellKind::display_name` wording. Owner ruling, 2026-09-21, recorded in `IN-0043.md`'s Open
Decisions. A submenu rather than three top-level rows because three rows double the menu's
shell block for a case that is not the everyday one, and because one row keeps the
owner-fixed order below the shells untouched by construction rather than by care. It is a
separate hit target from the ordinary shell rows, with no modifier and no overlap.

`crates/terminal-view/src/panel/terminal_panel.rs`: the submenu is `#[cfg(windows)]`, it is
built through `PopupMenu::submenu`, each row calls the `launch_elevated_shell` fn pointer
with its `ShellKind`, and an elevated window returns from the menu builder before reaching
it. `FIXED_ROWS` is gone; `menu_rows(elevated, session_rows)` counts what each mode emits,
with `ELEVATED_SUBMENU_ROWS` at 1 on Windows and 0 elsewhere, and `menu_scrolls` keeps the
estimate's existing `ponytail:` caveat.

### Commands

- `pwsh scripts/ci-local.ps1` — final line **`ci-local: all checks passed.`** That run
  includes `cargo clippy --workspace --all-targets -- -D warnings` (twice, once with
  `terminal-diagnostics`), `cargo test --workspace`, `python scripts/check-doc-paths.py`
  ("Doc path check passed for 203 current paths in 11 documents"),
  `python scripts/check-english.py` ("passed for 979 files") and
  `python scripts/check-theme-contrast.py`, which passes **untouched** — the proof the
  elevated marker was done as a border and not as text on a new surface.
- `python scripts/verify-dependency-graph.py` — passed for 20 workspace packages;
  `crates/core` still has no `windows-sys`, and the menu still reaches the effect through
  `WorkspaceCommands` rather than a crate edge (R1/R5).

### E2E actually performed here

- **The menu.** `evidence/US-0132-plus-menu-run-as-admin.png` — the "+" menu of an ordinary
  (non-elevated) window with the submenu open: `Command Prompt`, `PowerShell`,
  `PowerShell 7`, `Run as administrator ›` opening onto the same three names, then the
  "SSH Sessions" separator, "No saved sessions", the plain separator, "Quick Connect...",
  "New Saved Session...". The rows below the shell block are in their existing order, and no
  scrollbar appears at this length.
- Driven with the project's posted-message walk against a `fast-dev` build launched by this
  session and addressed only by its own pid. **No submenu row was clicked**: clicking one
  raises a UAC prompt, which this session must not do.

### Gaps

- **E1's second half and E2-E9 are not run.** The consent prompt is drawn by the AppInfo
  service on the secure desktop, where posted messages cannot reach, and this session is
  forbidden to raise one. Everything past the click — the prompt, the elevated window, its
  markers, `whoami /groups`, its reduced menu and dock, the five hashes, the About text, the
  crash store, the declined path — is the owner's manual checklist, written out step by step
  in `US-0131`'s Evidence. **Nothing in this packet's acceptance that depends on an elevated
  window may be read as proven.**
- The elevated mode's menu (three shells, nothing else) is proven by unit test only; no
  elevated window was looked at.
- Over-the-shoulder elevation needs a second account; none is available here.
- Group Policy "Automatically deny elevation requests" is **not** exercised. Its path is the
  generic "any other `ShellExecuteExW` failure" branch, which is also unexercised at run
  time: reaching it needs a real failure.
- No shield glyph was added. `crates/theme/assets/icons/` has no shield SVG and the row
  reads correctly without one — the text is the marker, a glyph would be decoration.

## Handoff

This is the last packet of `IN-0043`. On acceptance the intake closes; anything discovered
during it that is not one of these three outcomes becomes a new `US`/`BUG` under this
intake rather than an extension of this packet.
