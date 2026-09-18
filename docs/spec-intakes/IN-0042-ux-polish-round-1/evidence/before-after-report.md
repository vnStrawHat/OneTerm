# Before/after report of the UX round (IN-0042)

Work packet: `US-0126`. Build: `main @e66f76e8`. Walk: 2026-09-17.

The walkthrough of 2026-09-16 recorded 34 findings and 25 proposals against `main @2f12628a`,
with 67 screenshots. The owner's ruling was *handle all of S, M and L, then make a report with
before/after images*. Eighteen packets did the work; this is the report.

**Result in one line:** 24 of the 34 findings are fixed, 8 are partly fixed with the owning
packet's own recorded limit, and 2 were observations no packet took. All 67 scenes were
re-captured.

---

## 1. Method

**One build, one walk, one process.** `cargo build -p oneterm-app --profile fast-dev` from the
worktree at `e66f76e8`, launched once (pid 4564, window handle 12258436). Nothing was rebuilt
mid-walk and no source file was changed: this packet measures the round, it does not fix it.

**Process-id discipline.** The owner runs their own OneTerm. The walk found its own window with
`EnumWindows` + `GetWindowThreadProcessId` filtered on its own pid, addressed only that pid's two
top-level windows (the workspace, and the Settings window at handle 5377686), and closed only
that pid at the end. No window was ever found by name or title, and no process was closed by
image name.

**Capture.** A `CopyFromScreen` probe on the earlier walk showed the desktop locked, and it still
is, so every frame is `PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` of the walk's own window and
every input was a posted `WM_*` message (`WM_LBUTTONDOWN/UP`, `WM_RBUTTONDOWN/UP` +
`WM_CONTEXTMENU`, `WM_MOUSEWHEEL`, `WM_KEYDOWN/UP`, `WM_CHAR`). The driver is the walkthrough's
own `gui.ps1`, copied unchanged into this walk's scratch area.

**State.** `target/{docks,ssh_session,terminal,ui_config,update_config}.json` were seeded from
the original walk's `cfgbak/`, so the session tree holds the same `DevServer`
(`root@192.168.13.128:22`, `#56B6C2`) and `PAM` (`pam.vpbanks.com.vn:4422`, `#7F1D1D`) entries,
the UI font size is 16, the theme is `Zed One Dark` and the shell is `cmd`. `USERPROFILE` and
`HOME` pointed at the original walk's scratch home, so the prompt path and `known_hosts` match
the before frames. The `UX Lab Box` session in the `Lab` group was created through the
application during the walk, exactly as before.

**Window sizes.** Every after frame has the same pixel dimensions as its before frame; this was
checked programmatically over all 67 pairs, not by eye. 1600x1000 for the workspace scenes,
1016x708 for the Settings window, 900x800 for `50`, 1900x1040 for `51`-`59`.

**Servers.** The SFTP, host-key and connect-success scenes ran against the repository's loopback
`sftp-dev-server` on `127.0.0.1:2266`, serving the original walk's `sftproot/`; it was stopped
afterwards. The failure path used `10.10.10.10`, as before. No real SSH host is reachable here.

**What could not be driven, and why it is a property of the method rather than of the
application.**

- **No Ctrl/Shift chord.** Posted messages do not set real modifier key state. The in-terminal
  search bar was reached by temporarily rebinding *Find* to `F2` through the application's own
  Key Bindings page and then resetting it to `ctrl-f`, which was verified on screen at the end of
  the walk. The same limit means `US-0123`'s central claim -- that `Ctrl-S`, `Ctrl-Q`, `Ctrl-G`
  and `Ctrl-Space` now reach the foreground program -- is **still unverified end to end**.
- **No double-click.** `WM_LBUTTONDBLCLK` does not register as one. Tab rename was reached
  through the new context menu row instead, which is itself part of what `US-0116` added.
- **No real hover.** The kit's tooltip delay needs a pointer that stays still; a posted
  `WM_MOUSEMOVE` does not. The `Aa` and `W` tooltips in the search bar exist in source and are
  still uncaptured.
- **No splitter drag.** A posted drag does not reach the kit's resize handle, so the dock's
  "feels absolute while held" behaviour is read from code, as `US-0113` recorded.

Afterwards the loopback server was stopped and `target/fast-dev` was deleted.

---

## 2. Findings F1-F34

Verdict vocabulary: **fixed**; **partly fixed** (some of the finding stands, with the reason
recorded in the named packet); **observation** (the walkthrough recorded it with no proposal
behind it, and `IN-0042` did not packet it). Every "partly fixed" row names where the reason is
recorded.

| id | symptom (walkthrough) | packet | verdict | what changed, and what did not | before / after |
|---|---|---|---|---|---|
| F1 | Every local-shell tab is labelled "Terminal" | `US-0114` | fixed | Tabs read `Command Prompt` / `PowerShell`; an OSC 0/2 title still wins where a shell sets one. | [before](../research/before/04-two-tabs.png) / [after](after/04-two-tabs.png) |
| F2 | Dock-collapse button leaves the title bar claiming SSH Client; clicking it does nothing | `BUG-0067` | fixed | The dock button writes `None` to the control, and clicking a mode always reopens the dock with the same panel instance and width. | [before](../research/before/24b-after-clicking-sshclient-again.png) / [after](after/24b-after-clicking-sshclient-again.png) |
| F3 | Toast reads "SSH connect failed: SSH connect failed: ..." | `BUG-0068` | fixed | One prefix, in the toast and in the new inline error. | [before](../research/before/17b-connect-timeout-22s.png) / [after](after/17b-connect-timeout-22s.png) |
| F4 | Password cleared on Connect; no inline error, no retry | `US-0118` | fixed | The dialog stays open, the typed password survives, and the failure is stated inline as well as in the toast. | [before](../research/before/16-connect-failed.png) / [after](after/16-connect-failed.png) |
| F5 | "Save to SSH Sessions" ticked, connect failed, nothing saved, no message | `US-0118` | fixed | The dialog now says the session was not saved and why, with the tick left on. Saving before a successful connect was **declined** in `US-0118` Decisions (`CORR-54` stands). | [before](../research/before/16-connect-failed.png) / [after](after/16-connect-failed.png) |
| F6 | All secondary text at 2.48:1 dark / 2.18:1 light, below WCAG AA | `US-0111` | fixed | `muted.foreground`, `tab.foreground` and `table.head.foreground` reach at least 4.5:1 on every surface they are drawn on, in all 24 theme files (39 variants), enforced by `scripts/check-theme-contrast.py` in CI. | [before](../research/before/34-theme-light-main.png) / [after](after/34-theme-light-main.png) |
| F7 | Two different dialogs answer the same command name | `US-0114` | fixed | The `+` menu row reads `Quick Connect...` and a new `New Saved Session...` row opens the full dialog. Merging the two dialogs was declined as a separate outcome (`US-0114` Gaps). | [before](../research/before/13-new-ssh-session-dialog.png) / [after](after/13-new-ssh-session-dialog.png) |
| F8 | No way to add a session from the right dock once the list is non-empty | `US-0119` | fixed | A `+` in the Session header, and a `New Session` menu on the blank area below the list. | [before](../research/before/52-session-empty-area-menu.png) / [after](after/52-session-empty-area-menu.png) |
| F9 | At ~900 px the dock keeps ~490 px and the terminal gets under half | `US-0113` | fixed | The dock is capped at 35 % of the window with a 240 px floor, re-applied at four seams and on resize. Below about 685 px the floor takes over and the terminal no longer keeps the majority -- `US-0113`'s stated choice, recorded in its Gaps. | [before](../research/before/50-narrow-900.png) / [after](after/50-narrow-900.png) |
| F10 | cwd hard-cut mid-token keeping the head; `MEM 577.0` loses its unit | `US-0112` | fixed | The path elides from the left at a separator and keeps the tail; the git branch elides from the right keeping its head; MEM keeps its unit; the pinned right-hand end can no longer be pushed off. | [before](../research/before/50-narrow-900.png) / [after](after/50-narrow-900.png) |
| F11 | Leftmost tab clipped to a bare close cross; no overflow chevrons, no tab list | `US-0116` | partly fixed | Clipping fixed (it was never upstream) and a tab list delivered. **Chevrons are not:** the strip's scroll behaviour belongs to the kit's `TabBar` and nothing on the OneTerm side can add one. Recorded in `US-0116` Evidence, which also records that the upstream follow-up is owed and was **not filed**. | [before](../research/before/19-many-tabs.png) / [after](after/19-many-tabs.png) |
| F12 | A tab has no context menu at all | `US-0116` | fixed | `Rename...` / `Duplicate` / `Close` / `Close Others` / `Close to the Right`, acting on the right-clicked tab, with confirmation on the bulk rows. Right-click deliberately no longer activates the tab. | [before](../research/before/59-tab-context-menu.png) / [after](after/59-tab-context-menu.png) |
| F13 | `...` menu holds one item, duplicating the button beside it; "Zoom In" is the wrong word | `US-0116` | partly fixed | The menu now carries the tab list it needed. The **zoom row and its wording stay**: the kit appends its own row after the panel's rows and owns the string (`t!("Dock.Zoom In")` compiled into the kit crate), and `docs/PROJECT.md` forbids patching `gpui-component`. Recorded in `US-0116` Evidence and in `docs/gui-layout.md`. | [before](../research/before/20-tabbar-more-menu.png) / [after](after/20-tabbar-more-menu.png) |
| F14 | Settings sidebar sub-item does not scroll to its group | `US-0122` | partly fixed | The two long pages were split into short pages, so every group is on screen when its row is clicked. **The upstream defect is untouched:** the click still does not scroll, the split routes around it. `US-0122` Evidence, with the upstream report written out in its Handoff and **not filed**. | [before](../research/before/30-settings-completion.png) / [after](after/30-settings-completion.png) |
| F15 | Section chevrons navigate instead of collapsing; sidebar grows past the fold | `US-0122` | partly fixed | The caret collapses a group and twelve short pages fit the 708 px window collapsed and with one page open. The **row** still navigates rather than toggling -- the kit passes `click_to_open(true)` -- and the sidebar overflows with several pages expanded. `US-0122` grades both acceptance lines partial. | [before](../research/before/31-settings-appearance.png) / [after](after/31-settings-appearance.png) |
| F16 | Settings > General is one field, triple-named; ~85 % empty | `US-0122` | fixed | General is Theme / Interface / Shell, and no page states its own name three times. | [before](../research/before/26-settings-general.png) / [after](after/26-settings-general.png) |
| F17 | Proxy URL and Verify Certificates live under About; no "Check now" there | `US-0121` | fixed | Network is its own page between SSH and About; the Settings About page has a `Check Now` running the same action as the dialog. | [before](../research/before/36-settings-about-updates.png) / [after](after/36-settings-about-updates.png) |
| F18 | Theme dropdown opens at the top of ~40 interleaved entries, tick off-screen | `US-0121` | fixed | The list opens under `Dark themes` / `Light themes` headings with the current theme first and ticked. The kit cannot open a popup scrolled to its checked row, so ordering is the mechanism; the first Down-arrow still highlights the heading (`US-0121` Gaps). | [before](../research/before/32-theme-dropdown.png) / [after](after/32-theme-dropdown.png) |
| F19 | Descenders clipped in the Logging block | `BUG-0069` | fixed | Checkbox and radio labels render their descenders whole through a shared `control_label`. Two notes from the packet: the finding's own diagnosis was wrong (the "Logging" heading was never clipped -- the controls were), and **two `sftp-ui` sites with the identical cause are still unfixed**, recorded in `BUG-0069` Gaps. | [before](../research/before/11-session-property-dialog.png) / [after](after/11-session-property-dialog.png) |
| F20 | Private-key form is ~735 px and the footer sits at y~900 in a 1000 px window | `US-0120` | fixed | The `FormDialog` body scrolls, and Jump host / Port forwards / Agent forwarding fold into an Advanced disclosure that opens itself when the session uses them. Save now sits at y~813. | [before](../research/before/12-session-dialog-privatekey.png) / [after](after/12-session-dialog-privatekey.png) |
| F21 | Unlabelled colour square opening a 130-swatch palette | `US-0120` | fixed | A labelled eight-swatch row with `Custom...` for the full picker, whose own featured row is now the same eight. The swatches are not tab stops; the picker beside them is the keyboard route (`US-0120` Gaps). | [before](../research/before/54-session-color-picker.png) / [after](after/54-session-color-picker.png) |
| F22 | Enter does not create a typed group; the no-match area is a bare icon | `US-0118` | fixed | Enter creates the group and closes the dropdown; the no-match area reads "No groups yet. Type a name to create one." Enter with a match already highlighted is deliberately unchanged (`US-0118` Gaps). | [before](../research/before/56-group-typed.png) / [after](after/56-group-typed.png) |
| F23 | Group disclosure uses the diagonal maximise arrow | `US-0119` | fixed | A chevron. | [before](../research/before/57-session-tree-with-group.png) / [after](after/57-session-tree-with-group.png) |
| F24 | Global New Session in the top slot; Delete unseparated and unstyled; "Property" | `US-0119` | partly fixed | Open / Properties / separator / New Session / separator / Delete, with Delete in the danger colour and a confirmation. **Duplicate and Move to Group are still absent**, declined in `US-0119` Gaps as new capabilities needing their own packets. | [before](../research/before/10-session-context-menu.png) / [after](after/10-session-context-menu.png) |
| F25 | Empty-Space placeholder never mentions New Terminal Here; no shell picker | `US-0115` | partly fixed | The placeholder leads with "Right-click -> New Terminal Here". The **shell picker is deliberately not added**: a decision recorded in `US-0115` keeps the empty Space's menu spawning the default shell. | [before](../research/before/07-split-right-empty-space.png) / [after](after/07-split-right-empty-space.png) |
| F26 | The active Space is marked by a single 1-pixel line | `US-0117` | fixed | A 2 px accent ring on the active Space and a small `#N` chip on each inactive one. The chip's tooltip reads the raw session title rather than the tab label (`US-0117` Gaps, deliberate: routing it through the tab label would re-open a double-lease class of bug). | [before](../research/before/09-split-two-terminals.png) / [after](after/09-split-two-terminals.png) |
| F27 | Status bar collapses to the clock in an empty Space | -- | observation, not packeted | Unchanged, as `IN-0042` set out: the walkthrough recorded it with no proposal, and `docs/gui-layout.md` documents the behaviour as intended. | [before](../research/before/07-split-right-empty-space.png) / [after](after/07-split-right-empty-space.png) |
| F28 | Agent panel has no header; empty state names the raw protocol | `US-0125` | fixed | An `Agents` header matching the SSH Client headers, and plain-language copy. One mention of OSC 20308 stays as a dimmed footnote on the owner's instruction; a link out of the application was declined. | [before](../research/before/22-agent-panel.png) / [after](after/22-agent-panel.png) |
| F29 | Docked SFTP table needs a horizontal scrollbar and hides Size | `US-0124` | fixed | Defaults are Name / Size / Date Modified with Name measured against the panel, so nothing scrolls horizontally at any dock width; the local pane's always-empty fourth column is gone and the other columns are behind a Columns chooser. | [before](../research/before/44-sftp-connected.png) / [after](after/44-sftp-connected.png) |
| F30 | No transfer affordance between the panes; the two menus disagree | `US-0124` | fixed | `Upload` and `Download` sit at each pane's inner edge and both menus render one shared action list. Two limits stay, recorded in `US-0124` Gaps: **uploading onto an existing remote file still overwrites without asking**, and menu items are never greyed out. | [before](../research/before/47-sftp-expanded.png) / [after](after/47-sftp-expanded.png) |
| F31 | App defaults sit on terminal control characters (`ctrl-s`, `ctrl-q`, `ctrl-g`, `ctrl-space`) | `US-0123` | fixed | New SSH Session is `ctrl-shift-n`, Quit `ctrl-shift-q`, About `f1`, Toggle Gutter unbound, with a migration that only rewrites users still on the old defaults. `DEC-0018` is accepted by the owner. **That the freed keys now reach the shell is unverified**, and cannot be verified by this capture method (`DEC-0018` Consequences). | [before](../research/before/27-settings-keybindings.png) / [after](after/27-settings-keybindings.png) |
| F32 | Binding printed twice; chip at 2.48:1; capture replaces the whole row | `US-0111`, `US-0121` | partly fixed | The chip meets the contrast floor and a row at its default prints its binding once. **Capture still replaces the whole row** -- `P11` did not propose changing it and `US-0121` records it as out of scope. | [before](../research/before/39-keybinding-capture.png) / [after](after/39-keybinding-capture.png) |
| F33 | The only menu is "OneTerm"; no menu for Find, split, copy/paste | -- | observation, not packeted | Unchanged, as `IN-0042` set out: the absent Edit/View/Help menus are a deliberate choice recorded in `app_menus.rs`. | [before](../research/before/25-app-menu.png) / [after](after/25-app-menu.png) |
| F34 | Search bar shows `0/0` before typing; bare `Aa`/`W`; no regex toggle | `US-0115` | partly fixed | The counter is blank until a query exists and a distinct "No matches" state replaces `0/0`. The `Aa` and `W` toggles turned out to **already carry tooltips** (the walkthrough never hovered them), and the **regex toggle was declined** in `US-0115` as a new capability `P14` did not propose. | [before](../research/before/40-search-bar.png) / [after](after/40-search-bar.png) |

**Aggregate:** fixed 24, partly fixed 8, observation (not packeted) 2. Nothing is recorded as not
fixed: every finding a packet took moved, and the eight partial rows each name a kit limit or a
recorded decision rather than an omission.

All six findings the walkthrough marked **high** -- `F1`, `F2`, `F6`, `F7`, `F9`, `F14` -- were
taken: five are fixed and `F14` is partly fixed, with the upstream mechanism named. The eight
partial rows are `F11`, `F13`, `F14`, `F15`, `F24`, `F25`, `F32`, `F34`; the two observations are
`F27` and `F33`.

---

## 3. The 67 scenes

Every after frame was taken from the same build, at the same window size as its before frame, and
the pair links below are relative to this file. Per-scene notes -- what changed in the frame, or
why it is a replacement -- are in [`after/index.json`](after/index.json).

Two scenes no longer exist as the walkthrough found them and carry a **replacement surface**
under the same file name:

- **`31-settings-appearance`** -- `US-0122` folded the Appearance page onto General, so the after
  frame is General with its sidebar sub-items expanded, showing Mode and Color Theme in the Theme
  group and no Appearance row anywhere in the twelve-page sidebar.
- **`54-session-color-picker`** -- `US-0120` replaced the 130-swatch popup as the colour control
  with a labelled eight-swatch row and moved the full picker behind `Custom...`. The after frame
  shows both, so the reader can see what took the popup's place and where the popup went.

No scene was left uncaptured.

| scene | before / after | scene | before / after |
|---|---|---|---|
| 01-first-launch | [before](../research/before/01-first-launch.png) / [after](after/01-first-launch.png) | 30b-settings-sidebar-logging | [before](../research/before/30b-settings-sidebar-logging.png) / [after](after/30b-settings-sidebar-logging.png) |
| 02-plus-menu | [before](../research/before/02-plus-menu.png) / [after](after/02-plus-menu.png) | 31-settings-appearance | [before](../research/before/31-settings-appearance.png) / [after](after/31-settings-appearance.png) |
| 03-plus-menu-kbdnav | [before](../research/before/03-plus-menu-kbdnav.png) / [after](after/03-plus-menu-kbdnav.png) | 32-theme-dropdown | [before](../research/before/32-theme-dropdown.png) / [after](after/32-theme-dropdown.png) |
| 04-two-tabs | [before](../research/before/04-two-tabs.png) / [after](after/04-two-tabs.png) | 33-theme-light-settings | [before](../research/before/33-theme-light-settings.png) / [after](after/33-theme-light-settings.png) |
| 05-typing | [before](../research/before/05-typing.png) / [after](after/05-typing.png) | 34-theme-light-main | [before](../research/before/34-theme-light-main.png) / [after](after/34-theme-light-main.png) |
| 06-terminal-context-menu | [before](../research/before/06-terminal-context-menu.png) / [after](after/06-terminal-context-menu.png) | 35-settings-about | [before](../research/before/35-settings-about.png) / [after](after/35-settings-about.png) |
| 07-split-right-empty-space | [before](../research/before/07-split-right-empty-space.png) / [after](after/07-split-right-empty-space.png) | 36-settings-about-updates | [before](../research/before/36-settings-about-updates.png) / [after](after/36-settings-about-updates.png) |
| 08-empty-space-menu | [before](../research/before/08-empty-space-menu.png) / [after](after/08-empty-space-menu.png) | 36b-settings-about-end | [before](../research/before/36b-settings-about-end.png) / [after](after/36b-settings-about-end.png) |
| 09-split-two-terminals | [before](../research/before/09-split-two-terminals.png) / [after](after/09-split-two-terminals.png) | 37-about-dialog | [before](../research/before/37-about-dialog.png) / [after](after/37-about-dialog.png) |
| 10-session-context-menu | [before](../research/before/10-session-context-menu.png) / [after](after/10-session-context-menu.png) | 38-keybindings-edit-menu | [before](../research/before/38-keybindings-edit-menu.png) / [after](after/38-keybindings-edit-menu.png) |
| 11-session-property-dialog | [before](../research/before/11-session-property-dialog.png) / [after](after/11-session-property-dialog.png) | 39-keybinding-capture | [before](../research/before/39-keybinding-capture.png) / [after](after/39-keybinding-capture.png) |
| 12-session-dialog-privatekey | [before](../research/before/12-session-dialog-privatekey.png) / [after](after/12-session-dialog-privatekey.png) | 40-search-bar | [before](../research/before/40-search-bar.png) / [after](after/40-search-bar.png) |
| 12b-session-dialog-agent | [before](../research/before/12b-session-dialog-agent.png) / [after](after/12b-session-dialog-agent.png) | 41-search-matches | [before](../research/before/41-search-matches.png) / [after](after/41-search-matches.png) |
| 13-new-ssh-session-dialog | [before](../research/before/13-new-ssh-session-dialog.png) / [after](after/13-new-ssh-session-dialog.png) | 42-search-scrollback | [before](../research/before/42-search-scrollback.png) / [after](after/42-search-scrollback.png) |
| 14-quick-connect-filled | [before](../research/before/14-quick-connect-filled.png) / [after](after/14-quick-connect-filled.png) | 43-hostkey-prompt | [before](../research/before/43-hostkey-prompt.png) / [after](after/43-hostkey-prompt.png) |
| 15-connect-inflight | [before](../research/before/15-connect-inflight.png) / [after](after/15-connect-inflight.png) | 44-sftp-connected | [before](../research/before/44-sftp-connected.png) / [after](after/44-sftp-connected.png) |
| 15b-connect-15s | [before](../research/before/15b-connect-15s.png) / [after](after/15b-connect-15s.png) | 45-sftp-context-menu | [before](../research/before/45-sftp-context-menu.png) / [after](after/45-sftp-context-menu.png) |
| 16-connect-failed | [before](../research/before/16-connect-failed.png) / [after](after/16-connect-failed.png) | 46-sftp-delete-confirm | [before](../research/before/46-sftp-delete-confirm.png) / [after](after/46-sftp-delete-confirm.png) |
| 17-connect-timeout-20s | [before](../research/before/17-connect-timeout-20s.png) / [after](after/17-connect-timeout-20s.png) | 47-sftp-expanded | [before](../research/before/47-sftp-expanded.png) / [after](after/47-sftp-expanded.png) |
| 17b-connect-timeout-22s | [before](../research/before/17b-connect-timeout-22s.png) / [after](after/17b-connect-timeout-22s.png) | 48-sftp-overflow-menu | [before](../research/before/48-sftp-overflow-menu.png) / [after](after/48-sftp-overflow-menu.png) |
| 18-completion-overlay | [before](../research/before/18-completion-overlay.png) / [after](after/18-completion-overlay.png) | 49-sftp-after-tab-switch | [before](../research/before/49-sftp-after-tab-switch.png) / [after](after/49-sftp-after-tab-switch.png) |
| 18b-completion-overlay | [before](../research/before/18b-completion-overlay.png) / [after](after/18b-completion-overlay.png) | 50-narrow-900 | [before](../research/before/50-narrow-900.png) / [after](after/50-narrow-900.png) |
| 18c-completion-ps | [before](../research/before/18c-completion-ps.png) / [after](after/18c-completion-ps.png) | 51-large-1900 | [before](../research/before/51-large-1900.png) / [after](after/51-large-1900.png) |
| 19-many-tabs | [before](../research/before/19-many-tabs.png) / [after](after/19-many-tabs.png) | 52-session-empty-area-menu | [before](../research/before/52-session-empty-area-menu.png) / [after](after/52-session-empty-area-menu.png) |
| 20-tabbar-more-menu | [before](../research/before/20-tabbar-more-menu.png) / [after](after/20-tabbar-more-menu.png) | 53-new-session-from-tree | [before](../research/before/53-new-session-from-tree.png) / [after](after/53-new-session-from-tree.png) |
| 21-tabbar-4th-button | [before](../research/before/21-tabbar-4th-button.png) / [after](after/21-tabbar-4th-button.png) | 54-session-color-picker | [before](../research/before/54-session-color-picker.png) / [after](after/54-session-color-picker.png) |
| 22-agent-panel | [before](../research/before/22-agent-panel.png) / [after](after/22-agent-panel.png) | 55-group-combobox | [before](../research/before/55-group-combobox.png) / [after](after/55-group-combobox.png) |
| 23-dock-none | [before](../research/before/23-dock-none.png) / [after](after/23-dock-none.png) | 56-group-typed | [before](../research/before/56-group-typed.png) / [after](after/56-group-typed.png) |
| 24a-dock-collapsed-via-button | [before](../research/before/24a-dock-collapsed-via-button.png) / [after](after/24a-dock-collapsed-via-button.png) | 57-session-tree-with-group | [before](../research/before/57-session-tree-with-group.png) / [after](after/57-session-tree-with-group.png) |
| 24b-after-clicking-sshclient-again | [before](../research/before/24b-after-clicking-sshclient-again.png) / [after](after/24b-after-clicking-sshclient-again.png) | 58-tab-rename-dialog | [before](../research/before/58-tab-rename-dialog.png) / [after](after/58-tab-rename-dialog.png) |
| 25-app-menu | [before](../research/before/25-app-menu.png) / [after](after/25-app-menu.png) | 59-tab-context-menu | [before](../research/before/59-tab-context-menu.png) / [after](after/59-tab-context-menu.png) |
| 26-settings-general | [before](../research/before/26-settings-general.png) / [after](after/26-settings-general.png) | | |
| 27-settings-keybindings | [before](../research/before/27-settings-keybindings.png) / [after](after/27-settings-keybindings.png) | | |
| 28-settings-terminal | [before](../research/before/28-settings-terminal.png) / [after](after/28-settings-terminal.png) | | |
| 29-settings-ssh | [before](../research/before/29-settings-ssh.png) / [after](after/29-settings-ssh.png) | | |
| 30-settings-completion | [before](../research/before/30-settings-completion.png) / [after](after/30-settings-completion.png) | | |

Scene-by-scene differences worth naming beyond the finding rows:

- **`43`, `44`-`49`** read `127.0.0.1:2266`, because this walk's loopback server used port 2266;
  the original used 2222. Nothing else about those scenes differs in state.
- **`50`** was reached with ten tabs open where the original reached it with three, because the
  walks passed through that width at different points. The dock width and the status bar -- what
  the scene is for -- are comparable.
- **`58`** is the only scene whose before frame shows nothing happening: the old build's only
  route to tab rename was a double-click, which posted messages cannot deliver. The after frame
  shows the dialog reached through the context menu row `US-0116` added.

---

## 4. Open items across the round

Collected from the packets' own Gaps and Handoff sections, so the owner sees what was not fixed
beside what was.

### 4.1 Upstream, recorded and **not filed**

Two full upstream reports are written out, ready to file, in `US-0122`'s Handoff. Neither has
been filed: that session had no issue tracker access and does not open issues on the project's
behalf.

1. `setting::Settings` -- a sidebar sub-item cannot scroll to a group below the fold of a freshly
   opened page (the mechanism behind `F14`).
2. `setting::Settings` -- searching blanks the content pane, and clearing the query lands on the
   wrong page (found during `US-0122`'s rework, and the split raised its exposure from six pages
   to twelve).

A third upstream follow-up is owed and also unfiled: `US-0116` records that the tab strip's lack
of overflow chevrons belongs to the kit's `TabBar`, and that "this session opened no upstream
issue, and that follow-up is still owed". `BUG-0069` found a genuine upstream cause as well --
`Checkbox` and `Radio` in `gpui-component-0.6.0` wrap their label in a one-em line box that cannot
be overridden from outside -- and worked around it locally without filing anything.

### 4.2 The two light themes whose primary text is below the floor

`US-0111` raised secondary text above 4.5:1 everywhere but does not touch `foreground`, and in two
light themes that inverts the hierarchy:

| theme | `foreground` | `muted.foreground` | `tab.foreground` |
|---|---|---|---|
| Solarized Light | **4.39:1** | 5.02:1 | 5.05:1 |
| Everforest Light | **4.71:1** | 5.05:1 | 5.00:1 |

Their secondary text now has more contrast than their primary. The independent verification
confirmed these are the only two inversions among the 39 variants. `US-0111` records the work as
the same script with `foreground` added to the token list.

**Closed 2026-09-18 by `US-0127`**
(`docs/spec-intakes/IN-0042-ux-polish-round-1/US-0127-primary-text-contrast-floor.md`), which
added the primary-text tokens and a second rule -- a primary token must out-read
`muted.foreground` on every surface both are drawn on -- and moved the flagged values in
lightness only. Two corrections to the numbers above, both from attributing each token to the
surface the kit actually paints it on: the headline ratios are `foreground` scored against the
**tab strip**, where these themes draw `tab.foreground` instead, so **no variant's primary text
was in fact below 4.5:1** (Solarized Light bottomed out at 4.53:1 on a hovered row); and there
were **three** inverted variants, not two -- **Ayu Light** was inverted on all four of its
surfaces, including `popover.foreground` at 4.88:1 against `muted.foreground`'s 5.91:1. The
defect the section describes is real and is entirely the hierarchy: the 4.5:1 floor alone would
never have caught it.

### 4.3 The pre-existing flaky test

`oneterm-terminal::handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`
fails deterministically when run alone and passes only inside `cargo test --workspace`. The
`sftp-agent-wave1-verify.md` verifier ran it three times standalone after a green gate and got
three failures (`crates/terminal/src/handle.rs:364`, "the renderer waited 65 chunks, not one"),
and recorded: "This is a load-dependent test in `crates/terminal`, which **this diff does not
touch at all** -- it is pre-existing and unrelated to `US-0124` / `US-0125`. It is worth its own
`BUG` (the bound, or the test's shape, is wrong -- a test that only passes under contention is not
a test)." ~~**No `BUG` packet has been opened for it.**~~ **Closed 2026-09-17 by
`BUG-0070`** (`docs/spec-intakes/IN-0032-terminal-crate-tidy/BUG-0070-pump-yield-test-only-passes-under-load.md`),
which found the test's shape wrong rather than the product: the count started at the renderer's
raise, which an unthrottled pump outruns before the raise is even visible to it. Test only.

### 4.4 `F1` is taken from terminal programs

`DEC-0018`'s Consequences record, after the owner accepted `f1` for About: every `BINDABLE_ACTIONS`
row has `context: None` and gpui dispatches a matched binding before any key-down listener, so
while OneTerm is focused `F1` no longer reaches the terminal view's key map. **A user loses F1 help
in `mc`, `nano`, `htop`, `vim` and `less`.** The record calls this "the same class of cost the
record set out to remove from `^S`, `^Q`, `^G` and `^@`, on a key used by fewer programs", and says
moving About again is an amendment to `DEC-0018` plus a packet.

**Closed 2026-09-17 by the owner's ruling**: About ships with **no default binding at all**, so
`F1` goes back to the foreground program. `DEC-0018` is amended (Status, the Decision table, the
Consequence above) and `US-0123` carries the code as acceptance rework.

### 4.5 A live data-loss path left open

`US-0124`: "Uploading onto an existing remote file still overwrites it without asking, exactly as
the drag-and-drop and menu uploads did before this packet; only the download direction confirms."
The new `Upload` button makes the existing action visible without changing it. The packet puts a
change to the transfer mechanism out of scope and says it is worth its own packet, which was not
opened.

### 4.6 Remaining gaps by packet

| packet | what it recorded as still standing |
|---|---|
| `US-0111` | Primary text in Solarized Light and Everforest Light (4.2); hover/selection surfaces photographed on one theme only; user-supplied themes are not checked, as scoped. |
| `US-0112` | Non-text chrome is still hard-coded constants; the per-frame `fit_to_width` bisection is unprofiled; the tooltip that shows the full value has no test; only Windows was walked. |
| `US-0113` | A real splitter drag could not be exercised; below about 685 px the dock takes its 240 px floor and the terminal loses the majority; `P15`'s auto-collapse was deliberately not implemented. |
| `US-0114` | Keyboard navigation into the new `+` menu row is argued from source, not captured; no SSH tab was opened, so "an SSH tab's label is unchanged" rests on an untouched code path; `FIXED_ROWS` is still an estimate. |
| `US-0115` | The regex toggle is declined; the `Aa`/`W` tooltips are uncaptured; the in-terminal search bar still has no owning design section. |
| `US-0116` | Overflow chevrons and the `...` menu's zoom row and wording are upstream (4.1); "Close Others" was never walked, so its confirmation dialog is uncaptured; the 220 px tab ceiling is a judgement, not a measurement. |
| `US-0117` | The `#N` chip's tooltip ignores the tab-title mode and manual renames; no broadcast-channel collision frame and no two-by-two split frame were taken; `#N` is the stable `SpaceId`, not a positional index. |
| `BUG-0067` | The guard removal is not reachable from a headless test; no test drives the right-dock mode sync end to end; a legacy `ui_config.json` can show a stale mode until the first dock notification (left standing, self-correcting). |
| `BUG-0068` | Records no gaps of its own. |
| `BUG-0069` | Two `sftp-ui` sites (`render.rs:276`, `edit.rs:588`) still clip; `CONTROL_LABEL_LINE_HEIGHT = 1.5` is pinned by no test; glyph paint is not queryable, so the proof is a pixel-row measurement. |
| `US-0118` | A match on screen with no row highlighted still ignores Enter (deliberate); a host-key failure on a retried connect has no dialog left, only the toast; the duplicate-with-jump-chain path was closed by reading the call graph, not by walking it. |
| `US-0119` | Duplicate and Move to Group are absent; header crowding at a narrow dock width was not captured; the `row_was_right_clicked` flag remains a `bool` rather than a counter. |
| `US-0120` | The four SFTP `FormDialog` dialogs were not captured; the port-forward list was not walked with the wheel; the colour swatches are not tab stops; the 260 px chrome cap is a constant, not a measurement. |
| `US-0121` | The kit cannot open a dropdown scrolled to its checked row, so ordering is the mechanism; the first Down-arrow still highlights the heading; the list reorders between opens; Network is a one-group page; `F32`'s capture-replaces-the-row symptom is untouched. |
| `US-0122` | The upstream scroll defect is untouched and the split routes around it; twelve pages is more sidebar than six and it overflows when several are expanded; the page budget is walked, not asserted; the `P16` page wrapper was not built, and that is now a choice. |
| `US-0123` | The release-notes clause is the round's one explicitly **NOT MET** acceptance line -- `.github/workflows/release.yml` builds notes from commit subjects, so the rendered notes name no keystroke; override-vs-override collisions are not handled; a collision resolution becomes a persisted unbind; `F1`'s cost (4.4). |
| `US-0124` | Upload overwrites without asking (4.5); menu items are never greyed out; the "one list" invariant is structural, not tested; the `...` menu's height is estimated; the column-resize drag could not be driven from the implementer's session and has no after-frame; everything ran against the loopback server. |
| `US-0125` | The header's appearance is proven by screenshots, not assertions; the populated frame was produced by hand-emitting the sequence, not by a real agent; a link out of the application was declined. |

### 4.7 Method limits this walk inherits

Restating them here because a reader will otherwise take these rows as verified: **no Ctrl/Shift
chord, no double-click, no real hover and no splitter drag can be delivered by posted messages.**
Anything resting on those -- `US-0123`'s freed keys reaching the shell, the search bar's tooltips,
the dock splitter's feel -- is unverified end to end and needs a human at a real keyboard.

---

## 5. Verification of this report

The completeness check `US-0126` asked for, run over the merge result:

```
before 67 after 67
missing []
extra []
scenes 67 all named: True
findings 34 F1-F34 complete: True
report cites every finding: True
```

A second check compared the pixel dimensions of all 67 pairs with `System.Drawing`: every after
frame matches its before frame exactly, so no comparison in this report is between two different
window sizes.

Documentation gates:

```
python scripts/check-english.py     -> passed
python scripts/check-doc-paths.py   -> passed
```

This packet changes no source, so the round's test results are the implementation packets' own and
are cited rather than re-run; every one of them records `pwsh scripts/ci-local.ps1` ending in
"ci-local: all checks passed".
