# OneTerm UX walkthrough — 2026-09-16

> Copied verbatim into `IN-0042` on 2026-09-17 from the walkthrough session's
> scratchpad. This is the intake's research record: the findings and proposals
> below are the source the packets are cut from, and the text is not edited when
> a packet changes the behaviour it describes. The 67 `NN-*.png` frames it cites
> are in `before/` beside this file, under their original names; `gui.ps1` and
> `cfgbak/` stayed in the scratchpad and were not copied.

Read-only review of `main @2f12628a`. No tracked file was modified (`git status` clean at
the end); `harness.db` untouched.

---

## 1. Method

**Build.** `cargo build -p oneterm-app --profile fast-dev` →
`target/fast-dev/oneterm.exe` (exit 0). Launched from the repository root by this session
(pid 9604) with `USERPROFILE`/`HOME` pointed at a scratch home so the local shell and
`known_hosts` stayed out of the owner's profile. Only that pid's window (`hwnd 11865282`,
found via `EnumWindows` + `GetWindowThreadProcessId`) was ever addressed, and only that pid
was closed at the end. The owner's own OneTerm was never enumerated, focused or touched.

**Desktop locked.** A `CopyFromScreen` probe over a 200×200 region returned a single
distinct colour, so the desktop is locked. All screenshots are therefore
`PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` captures of my own window, and all input was
posted `WM_*` messages (`WM_LBUTTONDOWN/UP`, `WM_RBUTTONDOWN/UP` + `WM_CONTEXTMENU`,
`WM_MOUSEWHEEL`, `WM_KEYDOWN/UP`, `WM_CHAR`) — not `SendInput`. Driver:
`gui.ps1` in this folder.

**What that cost.** Posted messages do not set real modifier key state, so **no Ctrl/Shift
chord could be exercised** (Ctrl+F, Ctrl+T, Ctrl+Shift+Right, Ctrl+C …). I reached the
in-terminal search bar by temporarily rebinding *Find* to `F2` through the app's own Key
Bindings page, then **reset it to `ctrl-f`** (verified, screenshot 39 and after).
`WM_LBUTTONDBLCLK` likewise did not register as a double-click, so **double-click tab rename
could not be exercised** — only read in code (`crates/terminal-view/src/panel/tab_title.rs`
module doc). Everything else in the brief was walked.

**Live host.** No real SSH host is reachable. I used the repo's own loopback
`sftp-dev-server` (`cargo run -p oneterm-tools --bin sftp-dev-server -- --port 2222
--root <scratch>`) to walk the connect, host-key, SFTP browser, and dual-pane surfaces for
real; it was stopped afterwards. The unreachable-host error path used `10.10.10.10`.

**Config.** `target/{docks,ssh_session,terminal,ui_config,update_config}.json` were backed
up to `cfgbak/` before launch and `ssh_session.json` + `docks.json` restored after (the
`UX Lab Box` session I created through the app was removed; `ui_config.json` and
`terminal.json` came back byte-identical).

**Docs read (step 1).** `docs/gui-layout.md`, `docs/terminal-split.md`,
`docs/ssh-client-connect.md` §1/§3/§4/§9, `docs/sftp-browser-design.md` §1,
`docs/agent-panel-display.md` §1–3, `docs/auto-completion.md`, `docs/PROJECT.md`.

---

## 2. Findings

| id | screen | what the user experiences | why it hurts | sev | shot |
|---|---|---|---|---|---|
| F1 | centre tab bar | Every local-shell tab is labelled **"Terminal"**. Picking "PowerShell" from the `+` menu still produces a tab reading "Terminal", identical to the `cmd` tab beside it. With ten tabs open all ten read "Terminal". SSH tabs *are* named (`dev@127.0.0.1:22…`), so the asymmetry is visible in one strip. | Identification: the tab strip, the app's primary navigation, carries zero information. `resolve_tab_label` does use a live OSC 0/2 title, but `cmd.exe` and PowerShell on Windows emit none, so the fallback `DEFAULT_TAB_TITLE` (`terminal_panel.rs:113,181,186`) is what users always see. | **high** | 04, 19, 49 |
| F2 | title bar ↔ tab bar | The tab bar's 4th trailing button closes the right dock, but the title-bar segmented control still shows **SSH Client** selected. Clicking **SSH Client** then does nothing — the dock stays closed. Recovery requires clicking Agent or None first, then SSH Client. | State consistency / dead end. Root cause in code: `mode_toggle_group` reads the persisted `UiConfig.right_dock_mode` and its `on_click` only dispatches `when ix != current_ix` (`crates/workspace/src/layout/title_bar.rs:141-154`); the dock-collapse button never updates that config. | **high** | 21, 24a, 24b |
| F3 | SSH connect (failure) | An unreachable host produces, after 20 s, a bottom-right toast reading **"SSH connect failed: SSH connect failed: timed out after 20 s"** — the prefix is doubled. | Polish / trust. `AppError::Connect` already renders `"SSH {phase} failed: {message}"` (`crates/core/src/error.rs:129`) and the notification prefixes it again (`crates/session-ui/src/common.rs:418`). | medium | 17b |
| F4 | SSH connect (failure) | The password field is **cleared the moment Connect is pressed** and stays empty after the failure; the dialog offers no inline error and no Retry. The toast is in the opposite corner from the modal and auto-dismisses. | Error recovery. Every retry costs a full re-type of the password; a user who looked away for 25 s sees only an empty form and a re-enabled button with no explanation (15, 16 show exactly that state). | medium | 15, 16, 17b |
| F5 | SSH connect | **"Save to SSH Sessions"** was ticked, the connect failed, and nothing was saved — with no message. | Feedback: an explicit user intent is silently dropped. | medium | 14, 16 |
| F6 | whole app (both themes) | All secondary text — session host addresses, search placeholders, empty-state copy, key-binding chips, `Default:` hints, `+`-menu shortcut hints, SFTP dates, Update Status — renders at **contrast 2.48:1** (`#5c6370` on `#23272e`), below WCAG AA's 4.5:1. The light theme is no better: inactive tab labels **2.18:1**, host text **2.37:1**. Primary text on the same rows is 13:1. | Accessibility. Single token: `muted.foreground` (and `tab.foreground`) in `crates/theme/themes/*.json` — `zed-one-dark.json:49,87`. The most data-bearing values on a row are the least readable ones. | **high** | 01, 27, 34 |
| F7 | `+` menu vs Session tree | Two different dialogs answer the same command name. The tab bar's **"New SSH Session"** opens **"SSH Quick Connect"** (host/port/user/auth/save-checkbox). The tree's **"New Session"** opens **"New SSH Session"** (label, colour, group, jump host, port forwards, logging). Same `NewSession` action; the workspace handler routes to quick connect (`crates/app/src/init.rs:67`), the panel handler to the full dialog (`crates/session-ui/src/panel.rs:146`). | Consistency: the menu label promises the dialog you do not get, and the richer dialog is reachable only by right-clicking a row you may not have. | **high** | 13, 53 |
| F8 | Session panel | With sessions present there is **no way to add one from the right dock**: no `+` in the "Session" header, and right-clicking the blank area below the list produces no menu. (The empty-list state does offer "Right-click → New Session" — `crates/session-ui/src/render.rs:68`.) | Discoverability: the add affordance disappears exactly when the list stops being empty. | medium | 01, 52 |
| F9 | narrow window (~900 px) | The right dock keeps its absolute ~490 px, so the terminal is left ~410 px — under half the window — while the dock shows a two-row session list and "No SFTP connection." | Density: the primary surface loses to two mostly-empty panels on any laptop-width window. | **high** | 50 |
| F10 | status bar (narrow) | The cwd is hard-cut mid-token with no ellipsis — `…\fa2d0135-9c28-4` — keeping the *head* of the path and dropping the directory the user is actually in. `MEM 577.0` loses its `MB`. | Information: truncation drops the only part that matters. `crates/workspace/src/widgets/breadcrumb.rs` / `status_text.rs`. | medium | 50 |
| F11 | tab strip | The leftmost tab is clipped to a bare `×` with no label, and stays clipped after the window is widened to 1900 px even though there is free space to the right of the strip. No overflow chevrons, no tab list. | Navigation + safety: a close button on an unidentifiable tab. (The strip is `gpui_component::dock::TabGroup` — upstream.) | medium | 19, 51 |
| F12 | tab strip | A tab has **no context menu at all** (right-click just activates it). No Rename, Close Others, Close to the Right, Duplicate. Rename exists only as an unadvertised double-click. | Keyboard/mouse reach: the standard tab operations have no discoverable entry point. | medium | 59 |
| F13 | tab bar `…` menu | The overflow menu holds exactly one item, **"Zoom In  Shift+Esc"**, duplicating the ⤢ button immediately to its left. "Zoom In" also reads as font zoom in a terminal app; it means "zoom this panel to fill the workspace". | Redundancy + wording. The one thing a 10-tab strip needs (a tab list) is absent. | low | 20 |
| F14 | Settings sidebar | Clicking a sub-item does **not** scroll to its group. On Terminal, clicking "Completion" (10th) landed on Font (2nd); clicking "Logging" (5th) landed mid-Font. On Key Bindings, "Edit Menu" left the page on "App Menu". | Navigation: a 10-group page is effectively unnavigable by its own sidebar. `gui-layout.md` documents a related upstream index quirk, but every Terminal group *is* titled, so the documented workaround does not explain it. | **high** | 30, 30b, 38 |
| F15 | Settings sidebar | Section headers show a chevron but clicking one navigates instead of collapsing, so the sidebar grows to ~25 rows and About/Appearance fall below the fold in the default 708 px window. | Density / affordance mismatch. | low | 26, 38, 31 |
| F16 | Settings → General | The whole page is one field, "UI Font Size", with the section header, item title and description each restating it ("Interface / UI font size." → "UI Font Size" → "Interface font size in px."). ~85 % of the page is empty. | First impression: the settings landing page reads as unfinished. The same triple-naming runs through Terminal ("Shell / Shell for new local terminals." → "Shell / Choose shell kind."). | medium | 26, 28 |
| F17 | Settings → About | **Proxy URL** and **Verify Certificates** live under *About*, and the Updates group there has no "Check now" — the manual check exists only in the separate About **dialog**, where the update *settings* are not. | Categorisation: network settings are not "about", and the update action and its settings are split across two surfaces. | medium | 35, 36b, 37 |
| F18 | Settings → Appearance | The theme dropdown opens at the top of ~40 entries with the first row highlighted, not scrolled to the current theme (the ✓ was off-screen for "Zed One Dark"). Light and dark themes are interleaved in one list while "Mode" is a separate Light/Dark setting. | Orientation: the user cannot see what is selected without scrolling, and the two controls can contradict each other. | medium | 32 |
| F19 | New/Edit SSH Session dialog | In the **Logging** block — the last row — descenders are clipped flat: "Loggin**g**" and "Use glo**b**a**l**" lose the tails of their `g`s. Adjacent labels ("Jump host", "Group") render their `p`s intact, so it is specific to that block. | Legibility / polish, in a dialog the user meets on every session edit. `crates/session-ui/src/session_dialog.rs:396-419`. | medium | 11, 12 |
| F20 | New/Edit SSH Session dialog | With Private Key selected the form is ~735 px of fields and the footer sits at y≈900 in a 1000 px window; the code already carries a `ponytail:` note that `FormDialog` does not scroll (`session_dialog.rs:424-426`), so a session with several port forwards pushes Save off-screen. | Reachability on short windows / laptops. | medium | 12 |
| F21 | session colour | The colour control is an **unlabelled square** beside the Label field; clicking it opens a 130-swatch palette plus an HSLA tab plus a hex field — to choose the tint of an 8 px square. | Discoverability (nothing says it is a colour) + decision cost far above the payoff. | low | 11, 54 |
| F22 | group combobox | Typing a new group name and pressing **Enter** does not create it — only clicking `+ Create "Lab"` does — and after creating, the dropdown stays open showing `Create "Lab"` again. The no-match area shows a bare inbox icon with no text. | Keyboard reach + feedback. | low | 55, 56 |
| F23 | Session tree | A group's expand/collapse glyph is the **diagonal maximise arrow** (↙↗) — the same icon the app uses for "zoom panel" on the SFTP Browser header and the tab bar. | Icon consistency: one glyph, two unrelated meanings; a tree disclosure conventionally reads as a chevron. | low | 57 |
| F24 | session context menu | The menu is `New Session` / `Open` / `Delete` / `Property`. The global "New Session" sits in the top (most-misclicked) slot of an item-specific menu, `Delete` sits directly under `Open` with no separator and no destructive styling, and "Property" should read "Properties" (or "Edit…"). No Duplicate, no Move to Group. | Safety + wording. (SFTP's Delete *does* confirm and *is* styled danger — see 46 — so the app already has the better pattern.) | medium | 10 |
| F25 | empty Space | The placeholder reads "Drag a terminal tab here / or right-click to split" and never mentions **New Terminal Here**, which is the first item of its own context menu and the likeliest action. That item also spawns the *default* shell only — no shell picker, unlike the `+` menu. | Discoverability: the placeholder advertises the two rarer actions and hides the common one. | medium | 07, 08 |
| F26 | split Spaces | The active Space is marked by a **single 1-pixel accent line** on one edge (measured: `rgb(82,139,255)` at x=561 in shot 09). At a glance the two Spaces are indistinguishable, and there is no per-Space label saying which shell it holds. | Feedback: "where does my typing go" is answered by one pixel. (Design choice recorded in `terminal-split.md` §8, so a stronger cue, not a redesign.) | medium | 09 |
| F27 | status bar | With an empty Space active the status bar collapses to just the clock — breadcrumb, git, network and the folder icon all vanish. | Feedback: reads as broken rather than "nothing to show". (Documented in `gui-layout.md` as intended.) | low | 07 |
| F28 | Agent panel | The Agent panel has **no header**, while SSH Client mode shows "Session" and "SFTP Browser" headers. Its empty-state hint names the raw protocol ("Agents that emit OSC 20308 appear here") with no link. | Consistency + jargon. The empty state is otherwise well done. | low | 22 |
| F29 | SFTP browser (docked) | At the default dock width the table shows Name / Date Modified / Pe… and needs a horizontal scrollbar; **Size is not visible at all**. Even expanded to 1900 px the remote pane still scrolls horizontally and the local pane carries an always-empty 4th column. | Density: default column widths never fit the panel they ship in. | medium | 44, 47 |
| F30 | SFTP browser | The dual pane has **no transfer affordance between the panes** — no arrows, no toolbar. The `⋮` menu and the right-click menu offer the same actions in different order, one with icons and one without, and only the right-click menu has Edit/Refresh. | Discoverability of the central action + menu consistency. | medium | 45, 47, 48 |
| F31 | default key bindings (code-read; chords not exercisable on a locked desktop) | App-level single-Ctrl bindings sit on terminal control characters: `ctrl-s` New SSH Session (XOFF), `ctrl-q` Quit (XON), `ctrl-g` Toggle Gutter (BEL), `ctrl-space` About (set-mark / IME toggle). `crates/settings-ui/src/key_bindings/key_bindings_actions.rs:80,89,98,107`. | Keyboard reach: in a terminal these keystrokes belong to the remote program. `ctrl-space` for *About* is also an odd use of a prime key. | medium | 27, 38 |
| F32 | Key Bindings page | Every row prints the binding twice — the chip (`Ctrl+T`) and "Default: ctrl-t" — even when they are equal, and the chip renders at 2.48:1 while Edit/Reset sit at 13:1. Entering capture replaces the whole row, so you lose sight of which action you are rebinding. | Density + F6 + orientation. | low | 27, 39 |
| F33 | app menu | The only menu is "OneTerm" (About / Appearance / Theme / Settings… / Quit), opened by clicking the app name — which carries no chevron or affordance at rest. Find, split, copy/paste and every terminal action have **no menu at all**, only right-click or a key binding. | Discoverability for new users. (Edit/View/Help removal is deliberate — `app_menus.rs:3-5`.) | low | 25 |
| F34 | in-terminal search | The bar shows `0/0` before anything is typed; the modifier toggles are bare `Aa` and `W` with no visible labels; no regex toggle although `oneterm-vt` ships a `regex` feature. | Minor polish on an otherwise good feature. | low | 40, 41 |

Counts: **high 6**, **medium 17**, **low 11** (34 total).

---

## 3. Proposals

### Quick wins (S)

| # | change | fixes | touches | doc |
|---|---|---|---|---|
| P1 | Raise `muted.foreground` (and `tab.foreground`) in the built-in themes until secondary text reaches ≥ 4.5:1 on its own surface; add the ratio as a check in `scripts/` if it should stay true. | F6, F32 | `crates/theme/themes/*.json` (`zed-one-dark.json:49,87`) | `docs/gui-layout.md` (a sentence on the contrast floor for `muted.foreground`) |
| P2 | Drop the `ix != current_ix` guard in the mode toggle's `on_click` and make `SetRightDockMode` re-open the dock for the already-selected mode; or have the dock-collapse button write `RightDockMode::None` to `UiConfig`. | F2 | `crates/workspace/src/layout/title_bar.rs:141-154`, `crates/workspace/src/layout/workspace/actions.rs:115` | `docs/gui-layout.md` § Dock composition |
| P3 | Stop double-prefixing the connect error: `format!("{error}")`, since `AppError::Connect` already says "SSH connect failed: …". | F3 | `crates/session-ui/src/common.rs:418` | `docs/ssh-client-connect.md` §9.2 |
| P4 | Label local-shell tabs with the shell: pass the `ShellKind` display name instead of `DEFAULT_TAB_TITLE` for `PanelSpec::Shell`/`DefaultShell` (the OSC 0/2 override in `resolve_tab_label` keeps winning where a shell sets a title). | F1 | `crates/terminal-view/src/panel/terminal_panel.rs:113,179-188` (+ `tab_title.rs` tests) | `docs/gui-layout.md` § Panel registration |
| P5 | Keep the password on a failed connect (only clear on success) and show the error inline in the dialog as well as the toast. | F4 | `crates/session-ui/src/common.rs:412-424` | `docs/ssh-client-connect.md` §9.2 |
| P6 | Rename the `+` menu row to match the dialog it opens ("Quick Connect…"), and add a second row "New Saved Session…" that dispatches the full dialog at workspace level. | F7 | `crates/terminal-view/src/panel/terminal_panel.rs:716`, `crates/app/src/init.rs:67` | `docs/gui-layout.md` (the `+` menu order is owner-fixed — see §4) |
| P7 | Add a `+` button to the Session panel header and a context menu on the blank area below the list, both dispatching the full new-session dialog. | F8 | `crates/session-ui/src/render.rs:60-76`, `crates/session-ui/src/panel.rs` | `docs/ssh-client-connect.md` §6 |
| P8 | Swap the group disclosure glyph from the maximise arrow to a chevron. | F23 | `crates/session-ui/src/tree_builder.rs` | — |
| P9 | Elide the status-bar path from the **left** (`…\scratchpad\ux\home`) and keep the unit on MEM. | F10 | `crates/workspace/src/widgets/breadcrumb.rs`, `crates/workspace/src/widgets/status_text.rs` | `docs/gui-layout.md` § Status bar |
| P10 | Give the empty-Space placeholder its own first line — "Right-click → New Terminal Here" — and put `Delete` behind a separator with danger styling in the session context menu (SFTP already does this). | F25, F24 | `crates/terminal-view/src/space/render.rs`, `crates/session-ui/src/tree_render.rs:185-215` | `docs/terminal-split.md` §9 |
| P11 | Fix the clipped descenders in the Logging block, and drop the redundant "Default: …" line on key-binding rows that are at their default. | F19, F32 | `crates/session-ui/src/session_dialog.rs:396-419`, `crates/settings-ui/src/key_bindings/key_bindings_ui.rs` | — |
| P12 | Give the Settings About page a "Check for Updates" button (same action as the dialog), and move Proxy URL / Verify Certificates to their own "Network" page. | F17 | `crates/settings-ui/src/about.rs`, `crates/settings-ui/src/updates/`, `crates/settings-ui/src/panel.rs:85-93` | `docs/auto-update.md` |
| P13 | Open the theme dropdown scrolled to the current theme, and split it into "Light"/"Dark" sections. | F18 | `crates/settings-ui/src/appearance.rs` | — |
| P14 | Hide the `0/0` counter until a query exists; add tooltips to `Aa`/`W`. | F34 | `crates/terminal-view/src/terminal_view/search.rs:300-312` | — |

### Worth an intake (M/L)

| # | change | fixes | touches | effort | doc |
|---|---|---|---|---|---|
| P15 | Make the right dock width proportional (or clamp it to ~35 % of the window) and auto-collapse below a threshold, so the terminal keeps the majority at laptop widths. | F9 | `crates/workspace/src/layout/workspace/layout.rs`, `.../mod.rs` (`set_dock_size`), `crates/state/src/dock_persistence.rs` | M | `docs/gui-layout.md` § Dock composition |
| P16 | Fix Settings sidebar sub-item navigation so a sub-item scrolls to its group. Upstream `gpui_component::setting::Settings` owns the scroll and `PROJECT.md` forbids patching it, so this is either an upstream fix or a OneTerm-side page wrapper. | F14, F15 | `crates/settings-ui/src/panel.rs`, `crates/settings-ui/src/terminal/mod.rs`, upstream `gpui-component` | M–L | `docs/gui-layout.md` § Settings window (replaces the untitled-group paragraph) |
| P17 | Give tabs a context menu (Rename, Duplicate, Close, Close Others, Close to the Right) and the strip an overflow/tab-list button; make the leftmost tab never clip to a bare `×`. | F11, F12, F13 | `crates/terminal-view/src/panel/tab_title.rs`, `crates/workspace/src/layout/workspace/dock_skin.rs`, upstream `TabGroup` | M | `docs/gui-layout.md` § Panel registration |
| P18 | Rebase the app-level defaults off single-Ctrl keys that terminals own: `ctrl-shift-s`/`ctrl-shift-n` for New SSH Session, `ctrl-shift-q` for Quit, `f1` or `ctrl-shift-/` for About, and drop `ctrl-g` or move it to `ctrl-shift-g`. Ship a migration that only rewrites users still on the old defaults. | F31 | `crates/settings-ui/src/key_bindings/key_bindings_actions.rs:50-160` | M | a new `docs/decisions/DEC-00xx` — this is a shipped-default change |
| P19 | Give the SFTP table sensible default column widths for the docked (≈490 px) case — Name + Size + Date, the rest behind a column chooser — and add explicit transfer controls between the dual panes, plus one shared action list for the `⋮` and right-click menus. | F29, F30 | `crates/sftp-ui/src/` (table + menus), `crates/state/src/dock_persistence.rs` (`sftp_table_state`) | M | `docs/sftp-browser-design.md` §4 |
| P20 | Strengthen the active-Space cue: widen the accent gutter to 2 px and give each Space a small corner label (shell name or `#N`), reusing the channel-badge slot. | F26, F27 | `crates/terminal-view/src/space/render.rs`, `crates/terminal-view/src/theme/` | M | `docs/terminal-split.md` §8 (amend, do not reverse the 1 px decision) |
| P21 | Give Settings→General real content — pull Appearance's Mode/Theme and the shell default up to it, or fold General away — and cut the header/title/description triple-naming to two levels across every page. | F16 | `crates/settings-ui/src/general.rs`, `crates/settings-ui/src/appearance.rs`, `crates/settings-ui/src/terminal/*.rs` | M | `docs/gui-layout.md` § Settings window |
| P22 | Make the session form survive short windows: scroll the `FormDialog` body (the `ponytail:` note at `session_dialog.rs:424` already flags this) and collapse Jump host / Port forwards / Agent forwarding into an "Advanced" disclosure. | F20 | `crates/state/src/form_dialog.rs`, `crates/session-ui/src/session_dialog.rs` | M | `docs/ssh-client-connect.md` §4 |
| P23 | Replace the 130-swatch colour popup with the eight-swatch row it already renders on top (plus "Custom…" for the full picker), and label the control. | F21 | `crates/session-ui/src/session_dialog.rs` (colour field) | S–M | `docs/gui-layout.md` (US-0110 paragraph) |
| P24 | Save a quick-connect session even when the connect fails (or tell the user it was not saved), and let Enter commit a new group in the combobox. | F5, F22 | `crates/session-ui/src/quick_connect_dialog.rs:99-140`, `crates/session-ui/src/session_dialog.rs` (group combobox) | S–M | `docs/ssh-client-connect.md` §1.3 |
| P25 | Give the Agent panel a header consistent with the SSH Client sections and plain-language empty-state copy. | F28 | `crates/agent-ui/src/view.rs`, `crates/workspace/src/layout/workspace/dock_skin.rs` | S–M | `docs/agent-panel-display.md` §1.1 |

### Settled by owner decision — not proposed

- **The `+` menu's row order** (shells → "SSH Sessions" → ungrouped sessions → per-group →
  "New SSH Session" last) is the owner's, fixed at the acceptance of `US-0094` / `IN-0033`
  (`docs/gui-layout.md`). P6 only renames/adds a row; it does not reorder.
- **Session colour squares** on both the `+` menu and the tree, resolved by one
  `session_color_hex` with the `#56B6C2` fallback — `US-0110`. P23 keeps this, it only
  changes the picker.
- **1 px outer border + 1 px inner gutter** per Space, and a borderless single Space —
  `docs/terminal-split.md` §8, decision 8 and 3. P20 strengthens the cue inside that rule.
- **Splits are not persisted** across restart (`terminal-split.md` decision 4) and
  **channel membership is in-memory** — not raised.
- **No Edit/View/Help menus**; their actions live in key bindings and in-app UI
  (`app_menus.rs:3-5`). F33 is recorded as an observation only.
- **Passwords are never persisted** (`ssh-client-connect.md` §1.3 decision 1,
  `DEC-0001`). P5 keeps the password in memory for the retry only, within the open dialog.
- **Connect runs async and the dialog reports failure by notification**
  (`ssh-client-connect.md` §1.3 decision 7) — P5 adds an inline echo, it does not make
  connect blocking.
- **Startup resets the centre to one terminal tab** and never builds the saved centre
  (`gui-layout.md` § Persistence, `DEC-0016`) — not raised.
- **`oneterm-vt` is not published to crates.io** (owner ruling 2026-09-15) — out of scope.

---

## 4. What already works well

- **Host-key approval** (43): fingerprint, algorithm, an explicit "verify through a trusted
  channel" line, and a danger-styled *Trust and Connect* against a neutral *Cancel*. Exactly
  the right hierarchy for a security decision.
- **SFTP delete** (46): names the file, confirms, and styles the destructive button danger.
  This is the pattern the session tree's Delete should copy.
- **In-terminal search** (40–42): searches the full scrollback (not just the viewport),
  scrolls the viewport to the match, highlights *all* matches with the active one brighter,
  and shows an accurate `n/total` — I verified all six "System" matches were tinted.
- **Completion overlay** (18, 18c): correct prefix highlighting, works in both `cmd` and
  PowerShell, and the `history` / `command` tags are genuinely useful. It also handles a
  wrapped multi-row prompt correctly.
- **The `+` menu** (02, 03): shells, saved sessions grouped with rules-and-label separators,
  colour squares matching the tree, full arrow-key navigation, and a re-read of the store on
  every open.
- **Agent panel empty state** (22): icon, headline, and a one-line explanation of what would
  appear — better than most empty states in the app.
- **SFTP follows the active tab** (44, 49): switching from the SSH tab to a local tab cleanly
  returns the browser to "No SFTP connection".
- **Dual-pane SFTP zoom** (47): expanding the browser really does zoom the node to fill the
  workspace, with the Session section hidden — the `IN-0025` behaviour works as documented.
- **Connect in flight** (15): the button becomes a disabled "Connecting", Cancel stays live,
  and the dialog does not block the rest of the app.
- **Key Bindings page** (27, 39): grouped by origin, per-row Edit/Reset, Esc-to-cancel
  capture, and the reset genuinely restores the default (verified round-trip on *Find*).
- **Terminal rendering** (05, 40): colours, alignment, reflow on split, and the scrollback
  all behaved correctly through every resize in this walk.

---

*Screenshots referenced above are the `NN-*.png` files in this folder; the driver is
`gui.ps1`; the pre-walk config backup is `cfgbak/`.*
