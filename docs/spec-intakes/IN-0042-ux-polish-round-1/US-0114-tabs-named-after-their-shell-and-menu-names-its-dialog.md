# Work: Tabs are named after their shell and the "+" menu names the dialog it opens

ID: US-0114
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

Two labels stop lying. A local-shell tab is named after the shell it runs, so ten open tabs
are ten distinguishable tabs; and the `+` menu's last row names the dialog it actually opens,
with a second row beside it for the full session dialog that until now was reachable only by
right-clicking a row in the session tree.

## Findings and proposals covered

`P4` — *"Label local-shell tabs with the shell: pass the `ShellKind` display name instead of
`DEFAULT_TAB_TITLE` for `PanelSpec::Shell`/`DefaultShell` (the OSC 0/2 override in
`resolve_tab_label` keeps winning where a shell sets a title)."*

`P6` — *"Rename the `+` menu row to match the dialog it opens ("Quick Connect…"), and add a
second row "New Saved Session…" that dispatches the full dialog at workspace level."*

Addresses `F1` and `F7`, both **high**, quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F1 | centre tab bar | Every local-shell tab is labelled **"Terminal"**. Picking
> "PowerShell" from the `+` menu still produces a tab reading "Terminal", identical to the
> `cmd` tab beside it. With ten tabs open all ten read "Terminal". SSH tabs *are* named
> (`dev@127.0.0.1:22…`), so the asymmetry is visible in one strip. | Identification: the tab
> strip, the app's primary navigation, carries zero information. `resolve_tab_label` does use
> a live OSC 0/2 title, but `cmd.exe` and PowerShell on Windows emit none, so the fallback
> `DEFAULT_TAB_TITLE` (`terminal_panel.rs:113,181,186`) is what users always see. | **high** |
> 04, 19, 49 |

> | F7 | `+` menu vs Session tree | Two different dialogs answer the same command name. The
> tab bar's **"New SSH Session"** opens **"SSH Quick Connect"** (host/port/user/auth/save-
> checkbox). The tree's **"New Session"** opens **"New SSH Session"** (label, colour, group,
> jump host, port forwards, logging). Same `NewSession` action; the workspace handler routes
> to quick connect (`crates/app/src/init.rs:67`), the panel handler to the full dialog
> (`crates/session-ui/src/panel.rs:146`). | Consistency: the menu label promises the dialog
> you do not get, and the richer dialog is reachable only by right-clicking a row you may not
> have. | **high** | 13, 53 |

## Scope

- [x] In scope:
  - `crates/terminal-view/src/panel/terminal_panel.rs:113,179-188` — the `DEFAULT_TAB_TITLE`
    fallback for `PanelSpec::DefaultShell` and `PanelSpec::Shell`. Each gets the shell's
    display name instead.
  - `crates/terminal-view/src/panel/tab_title.rs` — `resolve_tab_label` and its tests; the
    OSC 0/2 title must keep winning where a shell sets one.
  - `crates/terminal-view/src/panel/terminal_panel.rs:713-715` — the menu's last row, renamed
    to name the dialog it opens, plus one new row beside it.
  - `crates/state/src/commands.rs` — one new `WorkspaceCommands` fn pointer for the full
    session dialog, and the rename of `open_new_session_dialog` to say what it opens.
  - `crates/app/src/init.rs:64-74` — registering both.
  - Every test double that builds a `WorkspaceCommands`.
- [x] Out of scope:
  - The `+` menu's row **order**, which is the owner's and fixed at `US-0094` acceptance
    (`docs/gui-layout.md`). This packet renames the last row and adds one beside it; nothing
    above the closing separator moves.
  - The `NewSession` action's target. Its key binding keeps opening quick connect, so no
    keystroke changes meaning under the user. `US-0123` is where key bindings move.
  - Tab rename (`US-0116`), the tab context menu (`US-0116`), and the session panel's own
    add affordance (`US-0119`).
  - The two dialogs themselves — their fields, their validation, their save behaviour
    (`US-0118`, `US-0120`).
  - SSH tab labels, which are already correct.

## Acceptance

- [x] Opening "Command Prompt" and "PowerShell" from the `+` menu produces two tabs with two
      different labels, each naming its shell.
- [x] The default-shell tab (the one the application opens at startup) is named after the
      shell it actually spawned, not "Terminal".
- [x] A shell that emits an OSC 0/2 title still overrides the label, exactly as today —
      covered by an existing or new `tab_title.rs` test.
- [x] An SSH tab's label is unchanged.
- [x] The `+` menu's final rows read as the dialogs they open: the existing row names quick
      connect, and a new row opens the full "New SSH Session" dialog — the same dialog the
      session tree's context menu opens.
- [ ] The new row works with nothing saved and with sessions saved; it does not move, hide or
      reorder any row above the closing separator.
- [ ] Keyboard navigation reaches the new row, and clicking it dismisses the menu.
- [x] No crate edge is added: `crates/terminal-view` still does not depend on
      `crates/session-ui`.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Panel registration and presentation — describes the `+` menu row by
  row, top to bottom, ending *"a plain separator; and "New SSH Session" (`NewSession`) last"*,
  and states the order is the owner's. **Update required:** the final rows' names and the new
  row. The order sentence stays, because the order does not change.
- `docs/gui-layout.md` §Panel registration — also the home of the tab-label contract.
  **Update required:** a sentence saying a local-shell tab is named after its shell, with the
  OSC 0/2 title still winning.
- `docs/ssh-client-connect.md` §1.1 / §1.3 — which surfaces open which connect dialog. `F7` is
  precisely a disagreement about this. **Update required:** record that the `+` menu now
  reaches both dialogs and which row reaches which.
- `docs/agents/crate-dependency-rules.md` R1/R4/R5/R10 — reconfirmed: the new command is an
  fn pointer on a `crates/state` type, taking only `&mut Window, &mut App`, so `crates/state`
  names no feature type and no edge appears. **No change.**
- `docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/high-level-design.md` —
  the `+` menu's wireframe and its "New SSH Session is last" decision. It is a historical
  design record for a shipped intake, not a current contract; **no change**, but read it so
  the rename does not contradict a decision it settled.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/gui-layout.md` (tab labels, and the `+` menu's final rows) and
`docs/ssh-client-connect.md` §1 (which surface opens which dialog).

Reason: both changes alter documented UI text and documented entry points.

### Reconciliation

Changed: `docs/gui-layout.md` §Panel registration and presentation (the tab-label contract
and the `+` menu's two closing rows; the "order is the owner's" sentence is untouched because
the order is untouched) and its source-map line; `docs/ssh-client-connect.md` §1.1 (which `+`
row reaches which dialog, and the `WorkspaceCommands` field names).

Unchanged, reasons still valid: `docs/agents/crate-dependency-rules.md` — the new command is an
fn pointer on a `crates/state` type taking only `&mut Window, &mut App`, so no feature type is
named and no edge appears; `scripts/verify-dependency-graph.py` in the gate confirms it.
`docs/spec-intakes/IN-0033-.../high-level-design.md` — a historical record of a shipped intake;
read, and the rename contradicts nothing it settled, because the order it fixed is intact.
`docs/PROJECT.md` — no change.

The two `IN-0042` records that quote the old field name
(`high-level-design.md:231,235`, `IN-0042.md:168`) are left as written: they describe the state
before this packet, which is what they are for.
## Context

- The tab-label half is genuinely small: `terminal_panel.rs:179-188` currently writes
  `DEFAULT_TAB_TITLE.to_string()` for both local-shell arms, while the `PanelSpec::Shell(kind)`
  arm already has the `ShellKind` in hand and the `DefaultShell` arm can resolve it from the
  same place `spawn_local_view` does. `DEFAULT_TAB_TITLE` stays as the reset-tab fallback
  (its doc comment at line 112 says it is both; keep the reset half).
- The `+` menu half is where the seam is. `crates/terminal-view` builds the menu;
  `crates/session-ui` owns the dialog. `oneterm-session-ui` already depends on
  `oneterm-terminal-view` — the one same-layer edge R5 allows — so the reverse edge is a cycle
  (R1). The registry that exists for exactly this is
  `oneterm_state::commands::WorkspaceCommands`, which the menu already reads: the saved-session
  rows call `commands.open_saved_ssh_session` (`terminal_panel.rs:705-709`). One more field
  beside it, registered in `crates/app/src/init.rs`, and the new row is three lines.
- The existing field is misnamed, and that is part of `F7`:
  ```rust
  /// Open the "New SSH session" quick-connect dialog.
  pub open_new_session_dialog: fn(&mut Window, &mut App),
  ```
  registered as `open_new_session_dialog: oneterm_session_ui::open_quick_connect_dialog`. The
  field says "new session", the function says "quick connect". Rename the field to
  `open_quick_connect_dialog` in the same packet — it is a compiler-checked rename across a
  handful of call sites, and leaving two similarly named fields where one is honest and one is
  not would recreate the confusion inside the code.
- `WorkspaceCommands` is `Copy` and built at one composition root plus the crates' test
  doubles (for example `crates/terminal-view/src/panel/tests.rs`). Widening it is a
  compile-time change every double must follow; that is the integration proof, not extra
  coverage.
- The menu's `scrollable` estimate uses `FIXED_ROWS: usize = 6`
  (`terminal_panel.rs:735-737`), commented as "3 shells + the SSH Sessions heading + the
  closing separator and New SSH Session". Adding a row makes that 7. Miss this and a long
  saved list gets its scroll decision wrong by one row.
- `research/before/04-two-tabs.png`, `19-many-tabs.png`, `49-sftp-after-tab-switch.png`
  (tab labels) and `13-new-ssh-session-dialog.png`, `53-new-session-from-tree.png`,
  `02-plus-menu.png` (the two dialogs and the menu) are the before pictures.

## Plan

- [x] Tab labels first — smallest half, own tests in `tab_title.rs`.
- [x] Rename the existing `WorkspaceCommands` field; let the compiler find the call sites.
- [x] Add the new field, register it, add the menu row, bump `FIXED_ROWS`.
- [x] Update the two docs.
- [x] Re-capture the scenes.

## Decisions

None. The row text is wording inside an owner-fixed order, and the seam is the one
`IN-0033` already established for this exact hop.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-terminal-view` over `tab_title.rs` — a local shell of
   each kind maps to its own label; an OSC 0/2 title overrides it; a reset tab still falls
   back to `DEFAULT_TAB_TITLE`. These are pure label functions and are the only automated
   proof of the label half.
2. **Unit:** `cargo test -p oneterm-terminal-view`, `cargo test -p oneterm-session-ui`,
   `cargo test -p oneterm-state`.
3. **Integration:** `cargo test --workspace` — `WorkspaceCommands` gained a field and lost a
   name, so every test double must still compile. This is a compile check on the widened type,
   not coverage of the menu; say so in Evidence.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `04-two-tabs.png` — two local tabs with two different names.
   - `19-many-tabs.png` — the ten-tab strip that read "Terminal" ten times.
   - `02-plus-menu.png` — the menu with both final rows.
   - `13-new-ssh-session-dialog.png` — the dialog the renamed row opens (quick connect).
   - `53-new-session-from-tree.png` — the dialog the **new** row opens, which must be the same
     dialog this frame shows from the tree.
   - `03-plus-menu-kbdnav.png` — keyboard navigation reaches the new row.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Reordering the menu by accident.** The order is owner-fixed. Adding a row next to the last
  one is allowed; moving anything above the closing separator is not. The re-captured
  `02-plus-menu.png` is the check.
- **`FIXED_ROWS` drift.** The scroll estimate is a hand-maintained constant with a `ponytail:`
  note admitting it is estimated. One more row means one more count. A menu with a long saved
  list is where this shows up, so seed several sessions before capturing.
- **Breaking the OSC title override.** `resolve_tab_label` is the function that decides; the
  change is to what it falls back to, not to its precedence. A test that asserts the override
  still wins is mandatory, not optional.
- **The rename touching more than expected.** `open_new_session_dialog` may be referenced in
  docs and test doubles as well as code. The compiler catches the code; grep the docs.
- **Two dialogs still exist.** This packet makes the labels honest; it does not merge the
  dialogs. If the owner's reaction to the after frames is "why are there two", that is a new
  outcome and a new packet, not a widening of this one.

## Evidence and Gaps

### Evidence

Branch `ux/tabs-spaces-menus`, commit `feat(terminal-view): name local tabs after their shell
and the "+" menu after its dialogs`.

Changed:

- `crates/core/src/config/shell.rs` — `ShellKind::display_name`, the one list the "+" menu's
  rows and the tab labels both read.
- `crates/terminal-view/src/panel/tab_title.rs` — `shell_tab_title` plus its tests.
- `crates/terminal-view/src/panel/terminal_panel.rs` — both local-shell arms of `from_spec`,
  the shell rows built from `display_name`, the two closing menu rows, `FIXED_ROWS` 6 -> 7.
- `crates/state/src/commands.rs` — `open_new_session_dialog` renamed to
  `open_quick_connect_dialog`; new `open_new_saved_session_dialog` beside it.
- `crates/session-ui/src/lib.rs` — `open_new_saved_session_dialog`, one line onto the existing
  `session_dialog::open_session_dialog`.
- `crates/app/src/init.rs`, `crates/state/src/services.rs`,
  `crates/terminal-view/src/panel/tests.rs`, `crates/workspace/src/layout/workspace/actions.rs`
  — the registration and every test double.
- `docs/gui-layout.md`, `docs/ssh-client-connect.md` §1.1.

Checks:

- `cargo test -p oneterm-terminal-view --lib` — 351 passed, 0 failed. The label half is covered
  by three new `tab_title.rs` tests: every `ShellKind` gets its own label and none reads
  "Terminal"; a custom shell is named after its program's file stem and falls back to
  `DEFAULT_TAB_TITLE` with no program; and a live OSC 0/2 title still wins over the shell name
  while `None` / `""` fall back to it.
- `cargo check --workspace --all-targets` — clean. This is the integration proof for the
  widened `WorkspaceCommands`: it is a compile check that every double follows the new shape,
  **not** coverage of the menu itself.
- `pwsh scripts/ci-local.ps1` — see below.

GUI walk (own binary, own pid, `fast-dev`, 1400x900, PrintWindow):

- `evidence/US-0114-02-plus-menu.png` — the menu closes with "Quick Connect... Ctrl+S" and
  "New Saved Session..."; the three shells, the "SSH Sessions" heading and the disabled
  "No saved sessions" hint are all still above the closing separator, unmoved.
- `evidence/US-0114-04-two-tabs.png` — "Command Prompt" and "PowerShell", two labels.
- `evidence/US-0114-19-many-tabs.png` — ten tabs, each naming its shell; the frame that used
  to read "Terminal" ten times.
- `evidence/US-0114-13-quick-connect-dialog.png` — the renamed row opens **SSH Quick Connect**.
- `evidence/US-0114-53-new-saved-session-dialog.png` — the new row opens the full **New SSH
  Session** dialog, the same one the session tree opens.
- The startup tab is named after the shell settings actually spawned: every capture shows the
  first tab as "Command Prompt", never "Terminal".

### Gaps

- **Keyboard navigation to the new row is not captured.** The walkthrough driver posts `WM_*`
  messages and cannot deliver the arrow/Enter sequence into a popup reliably; the row is a
  plain `PopupMenuItem`, which the kit treats identically to the rows above it for the mouse
  and the arrow keys, but that is reasoning, not a frame. `03-plus-menu-kbdnav.png` was not
  taken.
- **No SSH tab was opened**, so "an SSH tab's label is unchanged" rests on the code path being
  untouched (`PanelSpec::Session` still takes its `title` argument) rather than on a capture.
  No reachable host in this environment.
- **`FIXED_ROWS` is still an estimate.** It is now 7 and the `ponytail:` note above it still
  stands; the scroll decision was not exercised with a long saved list, because no sessions are
  saved in the walk profile.
- Two dialogs still exist. This packet made the labels honest; merging them would be a new
  outcome and a new packet.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
