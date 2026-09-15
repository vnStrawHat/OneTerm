# Work: The "+" menu lists the saved SSH sessions and opens one

ID: US-0094
Intake: IN-0033
Created: 2026-09-14

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

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: `IN-0033`

## Outcome

The "+" (New Terminal) dropdown in the center tab bar lists every session saved in
`ssh_session.json` under an "SSH Sessions" section, by name, in storage order. Clicking one
opens the same connect dialog the SSH Sessions panel opens for that session, and a
successful connect lands in a new center terminal tab exactly as it does from the panel.
When nothing is saved, the section shows a disabled "No saved sessions" hint and every
other entry in the menu behaves exactly as it does today.

## Rework (acceptance, 2026-09-15)

The owner tried the built "+" menu before accepting it and asked for a different
layout. Per `docs/HARNESS.md` this is acceptance rework of this packet, not a new bug.

Owner's required order, verbatim (translated from Vietnamese, 2026-09-15):

1. the shell entries (unchanged)
2. a separator carrying the label "SSH Sessions"
3. the saved SSH sessions that have NO group — **title only** (no `user@host:port` subtitle)
4. for each group, in the store's order: a **dashed** separator whose label is the group
   name, then that group's sessions (title only)
5. a separator
6. "New SSH Session" (moved from its current position above the sessions to the END)

What changes:

- **The subtitle is removed.** `D2` of the independent verification had added
  `label — user@host:port` to every row so that two sessions sharing a label stayed
  distinguishable. The owner asked for the title only, so that disambiguation is gone:
  **two saved sessions with the same label are now indistinguishable in the menu**, and
  clicking either opens whichever one owns that row's id. The owner chose this trade
  knowingly; the tree in the right dock still shows the subtitle for telling them apart.
  A blank label (only a hand-edited file can produce one) still falls back to `host:port`,
  because an empty row would be worse than a slightly longer one.
- **The list is grouped.** `menu_entries` now returns sections: the ungrouped sessions
  first, then one section per group in the order the groups first appear in
  `ssh_session.json`. The intake's open decision "group the sessions? No" is reversed by
  the owner here; the store order (not the tree's alphabetical order) is kept, so the
  section list still follows the file.
- **"New SSH Session" moves to the end**, behind a plain separator.
- **The section heading and the group headings are separators that carry a label.** The
  kit's `PopupMenu` has a plain `Separator` and a plain `Label` but nothing that is both,
  so the row is composed from `PopupMenuItem::element(...).disabled(true)`: the label text
  plus a rule that fills the rest of the row, `border_dashed()` for a group. Colours come
  from `cx.theme()` (`muted_foreground`, `border`); no literal is introduced.
- **The heading labels sit in the middle** (owner, same session): rule, label, rule.
- **The popup's scrollbar is hidden** for the everyday menu (owner, same session). The kit
  applies its height cap only when the menu is `scrollable`, and a scrollable menu always
  draws a scrollbar because this app's theme sets `ScrollbarMode::Always` globally. So
  `scrollable` is now switched on only when the rows cannot fit the cap
  (`min(half the window, 450px)`), estimated from the row count at the kit's 26px item plus
  its 2px gap. A normal-sized menu therefore has no bar, and a long one still gets the cap
  and the scrolling that `D1` asked for. Marked with a `ponytail:` comment: the height is
  estimated, not measured, so a menu within a row of the cap can guess wrong by one row.
- Unchanged: the id-not-index seam, the kit's height cap for a long list, the disabled
  "No saved sessions" hint, the shell entries above the heading, and the connect path below
  the dialog.

## Scope

- [x] In scope:
  - `WorkspaceCommands` gains `saved_ssh_sessions` (read the saved list) and
    `open_saved_ssh_session` (open the connect dialog by stable id).
  - `crates/session-ui` implements both and exposes the pure mapping from stored entries to
    menu rows.
  - `TerminalPanel::title_suffix` renders the new section.
  - `crates/app/src/init.rs` wires the two new fn pointers.
  - Unit tests for the mapping; owning docs updated.
- [x] Out of scope:
  - Grouping the menu by the session `group` field (intake open decision: flat).
  - Opening the session into the active Space instead of a new tab — that is what the SSH
    Sessions panel does today and matching it verbatim is the point of this packet.
  - Any change to the connect dialog, credentials, jump chains, host-key policy or logging.
  - Search or filtering inside the menu.

## Acceptance

Superseded by the 2026-09-15 rework (kept for the record, no longer the contract):

- [~] ~~The section lists the names flat, in storage order, below the unchanged local shell
      entries **and "New SSH Session"**.~~ The order changed: the heading and the sessions
      now come *before* "New SSH Session", and the list is grouped.
- [~] ~~Two saved sessions that share a label are still distinguishable in the menu.~~
      Withdrawn by the owner with the subtitle: see Rework above.
- [~] ~~The row mapping maps N stored entries to N rows in storage order.~~ Replaced by
      the sectioned mapping below.

Current acceptance (all re-proved on the 2026-09-15 build):

- [x] The menu reads, top to bottom: the platform's local shell entries (unchanged), a
      separator labelled "SSH Sessions", the saved sessions that have no group, then for
      each group in store order a dashed separator labelled with the group name followed by
      that group's sessions, then a plain separator, then "New SSH Session".
      *Evidence:* `evidence/US-0094-rework-menu-grouped-order.png` — Command Prompt /
      PowerShell / PowerShell 7, the "SSH Sessions" rule, `prod-web` and `staging`, the
      dashed `infra` rule with `db-01` and `db-02`, the dashed `lab` rule with `sandbox`, a
      plain separator, "New SSH Session" last — the seeded file's order, with the right
      dock's tree showing the same five sessions in the same frame.
- [x] Every session row shows the session title only — no `user@host:port` subtitle.
      *Evidence:* the same screenshot (the tree beside it still shows the subtitles), plus
      the `menu_entries_*` tests.
- [x] A long list stays reachable: the section is capped and scrolls rather than running off
      the window, and a row reached only by scrolling still opens its own session.
      *Evidence:* `evidence/US-0094-rework-menu-50-sessions-scrollable.png` — 50 saved
      sessions, the popup capped inside the window with its scrollbar. The pre-rework
      `evidence/US-0094-menu-50-sessions-scrolled-to-end.png` and
      `evidence/US-0094-dialog-from-scrolled-row.png` still stand for a scrolled row opening
      its own session: the row-to-id routing is unchanged.
- [x] Clicking a listed session — including one inside a group — opens the connect dialog
      for **that** session (title and server banner name its host). *Evidence:*
      `evidence/US-0094-rework-dialog-from-grouped-session.png` — clicking `db-01` under the
      dashed `infra` heading opens "Connect to db-01 (admin@10.77.0.21:22)" with the banner
      `ssh://admin@10.77.0.21:22`.
- [x] With no sessions saved, the heading still shows with the disabled "No saved sessions"
      hint, and "New SSH Session" is still present at the end. *Evidence:*
      `evidence/US-0094-rework-menu-empty-state.png`.
- [x] The row mapping returns the ungrouped sessions as the first section and one section
      per group in the order the groups first appear in the store, title only, with a
      blank-or-whitespace group counted as ungrouped and the stable ids unchanged by a
      delete — proven by unit tests with no gpui dependency. *Evidence:* the eight
      `menu_entries_*` / `verify_*` tests listed under Focused proof, with their tamper
      results.
- [x] No new crate edge: `crates/terminal-view` still does not depend on
      `crates/session-ui`, and `crates/terminal-view/Cargo.toml` is unchanged.
      *Evidence:* `python scripts/verify-dependency-graph.py` — "Dependency graph policy
      passed for 21 workspace packages and 21 explicit members"; the changed-file list never
      included `crates/terminal-view/Cargo.toml`.
- [x] `pwsh scripts/ci-local.ps1` is green. *Evidence:* "ci-local: all checks passed",
      60 sections / 1979 passed / 0 failed / 14 ignored.

Owner tweaks asked for and accepted during the same rework:

- [x] The heading labels sit in the middle of their rule.
      *Evidence:* `evidence/US-0094-rework-menu-grouped-order.png`.
- [x] The everyday menu shows no scrollbar, while a menu too long for the cap still scrolls.
      *Evidence:* the same screenshot (5 sessions, no bar) against
      `evidence/US-0094-rework-menu-50-sessions-scrollable.png` (50 sessions, capped, bar
      present).

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Panel registration and presentation — states that `TerminalPanel`
  keeps the final empty tab so the tab bar and its `+` creation entry point survive. It
  names the entry point but never said what the entry point offers; that is what changes.
- `docs/ssh-client-connect.md` §1.1 and §1.3 — the connect flow, and design decision 8
  ("left-click = Open") which describes the SessionPanel as the only way in. The flow below
  the dialog is unchanged; only the set of surfaces that reach the dialog grows.
- `docs/agents/crate-dependency-rules.md` R1/R4/R5/R10 — fixes the mechanism: the saved list
  must reach `crates/terminal-view` through `oneterm_state::commands::WorkspaceCommands`,
  not through a crate edge. No change needed; the rules already allow exactly this.
- `docs/terminal-split.md` §9 — the empty-Space "New Terminal Here" menu. Reviewed and left
  alone: that menu spawns a local shell in place and is a different surface from the tab
  bar's "+". No change.
- `docs/agents/persistence.md` — reviewed because the menu reads `ssh_session.json`. The
  store is read-only here: no schema, no write path, no new document. No change.

### Documentation Action

Update required:

- `docs/gui-layout.md` §Panel registration and presentation — one sentence naming what the
  `+` menu offers, including the saved-session section, and pointing at its source file.
- `docs/ssh-client-connect.md` §1.1 — one sentence adding the "+" menu as a second entry
  point to the same connect dialog, and §1.3 decision 8 amended so it no longer reads as
  "the SessionPanel is the only way in".

Reason: both docs describe a user-visible contract that this work widens. Neither needs
redefining — the connect flow itself is untouched — but leaving them as they are would make
them describe a surface that no longer has a single entry point.

### Reconciliation

Docs changed:

- `docs/gui-layout.md` — §Panel registration and presentation now describes the `+` menu's
  three sections, the storage-order flat list, the empty-state hint, and why the list
  crosses at `WorkspaceCommands`; §Source map gained two lines for the menu and for the
  saved-session rows.
- `docs/ssh-client-connect.md` — §1.1 records the `+` menu as a second entry to the same
  flow with the same `SshSessionId`; §1.3 decision 8 no longer reads as if the SessionPanel
  is the only way in, and states that the menu reuses `open_connect_dialog` so the two
  surfaces cannot drift.

No-change reasons confirmed still valid for `docs/agents/crate-dependency-rules.md` (the
rules already describe the seam used, and no rule text needed to move),
`docs/terminal-split.md` (the empty-Space menu is a different surface and is untouched), and
`docs/agents/persistence.md` (`ssh_session.json` is read-only here; no schema, write path,
or new document).

Re-checked after the verification rework: the `docs/gui-layout.md` sentence says the menu
lists the sessions "flat by name in storage order", which is still true of the fixed build —
the row text gained the subtitle and the popup gained scrolling, neither of which changes the
contract that sentence states. `docs/ssh-client-connect.md` is likewise unaffected: the
connect flow below the dialog did not move. No further doc change is required by `D1`-`D3`.

Acceptance rework, 2026-09-15 — the order *is* the contract that sentence stated, so it had
to move:

- `docs/gui-layout.md` §Panel registration and presentation now spells the menu out in the
  owner's order, records that rows are title-only (and that duplicate labels are therefore
  indistinguishable here by design), that the two labelled separators are composed from a
  disabled `PopupMenuItem::element` because the kit has no item that is both, and that the
  popup scrolls — and so shows its bar — only past the kit's height cap. The §Source map
  lines still point at the same two files.
- `high-level-design.md` — the wireframe, the decision list and the data-flow step for the
  menu section are redrawn for the grouped, title-only order with "New SSH Session" last.
- `IN-0033.md` — the open decision "group the sessions? No" records the owner's reversal.
- `docs/ssh-client-connect.md` — re-reviewed, unchanged: the set of surfaces that reach the
  connect dialog did not grow or shrink, and the flow below the dialog did not move.
- `docs/agents/crate-dependency-rules.md`, `docs/terminal-split.md`,
  `docs/agents/persistence.md` — re-reviewed, no-change reasons above still hold. The seam
  is the same one, only its payload type changed (`SavedSshSessionSections`).

## Context

- The "+" button is `TerminalPanel::title_suffix` in
  `crates/terminal-view/src/panel/terminal_panel.rs`. It is already a `Button` with
  `.dropdown_menu(...)`, so the popup and its anchor exist; only the closure body grows.
- `Button::dropdown_menu` takes an `Fn`, not an `FnOnce`, and the kit runs it on every open.
  Reading the store inside the closure therefore needs no observer: add or delete a session
  and the next open shows it.
- `oneterm-session-ui` depends on `oneterm-terminal-view` (R5's single allowed same-layer
  edge). The reverse edge is both a cycle and a rule break, which is why the two new fields
  go on `WorkspaceCommands` in `crates/state`. `crates/terminal-view/src/panel/duplicate.rs`
  already reaches `crates/session-ui` the same way.
- The command must carry the session's stable `SshSessionId`, not its index. Schema v2
  assigns those ids specifically so that deleting or reordering another session cannot
  retarget a pending action; an index would silently connect to the wrong host.
- `SshSessionId` is a newtype over `u64` whose field is private, and `crates/state` cannot
  name the type, so the fn pointers carry the raw `u64`.

## Plan

- [x] Add `saved_ssh_sessions` and `open_saved_ssh_session` to `WorkspaceCommands`, and to
      its test doubles (`crates/state/src/services.rs`,
      `crates/terminal-view/src/panel/tests.rs`).
- [x] Give `SshSessionId` a raw-`u64` round trip for the seam.
- [x] Add the entries mapping plus its unit tests in `crates/session-ui`.
- [x] Export `saved_ssh_sessions` / `open_saved_ssh_session` from `crates/session-ui`.
- [x] Render the section in `TerminalPanel::title_suffix`.
- [x] Wire both at the composition root in `crates/app/src/init.rs`.
- [x] Update the two owning docs.
- [x] `cargo fmt --all`, then `pwsh scripts/ci-local.ps1`.
- [x] GUI walk on the Windows desktop: empty state, two seeded sessions, click one.

Acceptance rework, 2026-09-15:

- [x] Reopen the packet, record the owner's order verbatim and redraw the HLD wireframe.
- [x] Turn `menu_entries` into sections (ungrouped first, then groups in store order),
      title only, and name the seam type `SavedSshSessionSections`.
- [x] Rewrite the model's unit tests for the sections; keep the id-not-index tests.
- [x] Rebuild the menu in the owner's order with a composed labelled separator, dashed per
      group, and move "New SSH Session" to the end.
- [x] Owner tweaks in the same session: centre the heading labels; hide the popup's
      scrollbar unless the rows exceed the kit's cap.
- [x] Update `docs/gui-layout.md`, the HLD and the intake's open decision.
- [x] `cargo fmt --all`, `pwsh scripts/ci-local.ps1`, and the GUI walk with a seeded store
      (2 ungrouped, `infra` × 2, `lab` × 1) plus a 50-session store for the cap.

## Decisions

No new decision record. The two choices this work makes (plain button rather than split
button; flat list in storage order) are local to this surface, reversible, and recorded in
`high-level-design.md` and the intake's Open Decisions. The rule that forces the
`WorkspaceCommands` seam is already `docs/agents/crate-dependency-rules.md` R1/R5/R10.

## Verification Plan

- Focused: the row-mapping unit tests — N entries in order, empty store, blank-label
  fallback, padded label.
- Unit: `cargo test -p oneterm-session-ui`, `-p oneterm-terminal-view`, `-p oneterm-state`.
- Integration: `cargo test --workspace` — every `WorkspaceCommands` literal must still
  compile with the two new fields.
- Static: `python scripts/verify-dependency-graph.py` — no new crate edge.
- E2E: Windows interactive desktop, `cargo build -p oneterm-app --profile fast-dev` from
  this worktree, `target/ssh_session.json` seeded in the worktree only. Open the menu with
  an empty store (hint), then with two saved sessions (both names), then click one and
  confirm the connect dialog names that session. No reachable host is required: a failed
  connect proves the dialog reached the connect path.
- Platform: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Code

| File | Change |
| --- | --- |
| `crates/state/src/commands.rs` | two fn-pointer fields on `WorkspaceCommands` |
| `crates/state/src/services.rs` | the bundle test double gained both fields |
| `crates/session-ui/src/session_state.rs` | `SshSessionId::raw` / `from_raw` |
| `crates/session-ui/src/tree_builder.rs` | `menu_entries` (label + `session_subtitle`) + 5 unit tests |
| `crates/session-ui/src/lib.rs` | `saved_ssh_sessions`, `open_saved_ssh_session`, and the store-ordering invariant |
| `crates/terminal-view/src/panel/terminal_panel.rs` | the menu section, `scrollable(true)` |
| `crates/terminal-view/src/panel/tests.rs` | the duplicate-flow test double gained both fields |
| `crates/app/src/init.rs` | composition-root wiring + the feature-init-before-install ordering note |
| `docs/gui-layout.md`, `docs/ssh-client-connect.md` | owning-doc updates |

`crates/terminal-view/Cargo.toml` is untouched: no new dependency, no new crate edge. The
connect path is not duplicated anywhere — `open_saved_ssh_session` is a store lookup plus the
existing `open_connect_dialog` call, which is the same two lines `SessionPanel::on_open_session`
runs.

### Focused proof

`cargo test -p oneterm-session-ui` — **57 passed, 0 failed, 0 ignored** (plus 0 doc-tests);
`cargo test -p oneterm-terminal-view` — **288 passed, 0 failed, 3 ignored**. The six new
tests:

| Test | Asserts |
| --- | --- |
| `menu_entries_keeps_storage_order_and_ids` | three entries (ids 7, 2, 5; one of them grouped) come back in file order with their own ids, not sorted and not grouped |
| `menu_entries_of_an_empty_store_is_empty` | the empty state at the model level |
| `menu_entries_falls_back_to_the_subtitle_for_a_blank_label` | a hand-edited whitespace label renders `10.0.0.9:2222`, not an invisible row |
| `menu_entries_trims_a_padded_label` | `"  staging  "` renders `staging — …` |
| `verify_duplicate_unicode_and_fifty_entries` | two rows sharing the label `alpha` stay distinguishable by their subtitle; a non-ASCII label survives; a 50-entry store maps 1:1 with first and last ids intact |
| `verify_menu_row_id_survives_a_delete_of_an_earlier_session` | the clicked row's id still resolves through `SshSessionStore::get` to its own host after an *earlier* session is deleted, and the deleted id resolves to `None` |

The last two were specified by the independent verification and are adopted here (see
Reconciliation for why they were re-written rather than applied as a patch).

Tamper check, each reverted afterwards:

- iterating the store with `.rev()` → `menu_entries_keeps_storage_order_and_ids` FAILED with
  `left: [(5, "db"), (2, "alpha"), (7, "prod")]` against
  `right: [(7, "prod"), (2, "alpha"), (5, "db")]`.
- disabling the blank-label fallback →
  `menu_entries_falls_back_to_the_subtitle_for_a_blank_label` FAILED with
  `left: [(3, "")]` against `right: [(3, "10.0.0.9:2222")]`.
- handing the seam the **position** instead of the id (`.enumerate()`) → **5 failures**,
  including `verify_menu_row_id_survives_a_delete_of_an_earlier_session` and
  `verify_duplicate_unicode_and_fifty_entries`. The positional design this packet argues
  against is now caught by the suite rather than only by argument.

### Gate

`pwsh scripts/ci-local.ps1` from this worktree — exit 0, all ten steps, ending
`ci-local: all checks passed.` `verify-dependency-graph.py` passed for 21 workspace packages
and 21 explicit members; `check-doc-paths.py` passed for 120 paths in 10 documents;
`check-english.py` passed for 777 files.

Summed over **every** `test result:` line of that run — the whole gate, not just its
`cargo test --workspace` step:

| | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| **measured** | **60** | **1941** | **0** | **14** |

(The first submission of this packet quoted 56/1568/0/11, which was the `cargo test
--workspace` step alone and understated the gate — `N5` from the independent verification.
The verifier measured 60/1939/0/14 at `39c9971`; the two extra passes here are the two
adopted verification tests.)

### E2E (Windows interactive desktop)

`cargo build -p oneterm-app --profile fast-dev` in this worktree. `fast-dev` inherits `dev`,
so `debug_assertions` is on and `oneterm_core::config_dir()` is the relative path `target`
— the app was therefore launched with its working directory set to **this worktree**, so it
read and wrote only `<worktree>/target/*.json`. The owner's real config in the main checkout
was never read or modified. The driver used is a worktree variant of the earlier session's
`gui.ps1` that takes the working directory as a parameter (posted window messages +
`PrintWindow`, every command addressed to the pid in a pid-file).

Safety, because the owner runs Claude Code inside their own `oneterm.exe`: `Get-Process
oneterm` was recorded before every launch (owner pids `2504, 14804` both times), the app was
started with `Start-Process -PassThru` and identified as the pid that was not there before
(`13816`, then `13200`), and only that pid's window was driven, screenshotted and stopped.
Nothing was ever matched by process name or window title for input or for closing; after both
walks `2504, 14804` were still running.

| Screenshot | Shows |
| --- | --- |
| `evidence/US-0094-menu-empty-state.png` | no `ssh_session.json` at all: Command Prompt / PowerShell / PowerShell 7, separator, New SSH Session, separator, the "SSH Sessions" label and the greyed "No saved sessions" hint. Everything above the new separator is what it was before this change. |
| `evidence/US-0094-menu-with-two-saved-sessions.png` | `target/ssh_session.json` seeded with `prod-web` (id 1, ungrouped) and `db-01` (id 2, group `infra`): the menu lists both, flat, in file order, each with its `user@host:port`, while the right dock's tree in the same frame nests `db-01` under `infra` — the flat-vs-grouped decision, visible side by side. |
| `evidence/US-0094-connect-dialog-for-clicked-session.png` | clicking the `prod-web` row opens "Connect to prod-web (root@10.77.0.11:22)" with banner `ssh://root@10.77.0.11:22` — the clicked session, not another one. |
| `evidence/US-0094-connect-failed-notification.png` | Connect reaches the real connect path: the error notification "SSH connect failed: SSH connect failed: timed out after 20 s". |
| `evidence/US-0094-menu-50-sessions-scrollable.png` | 50 saved sessions (two sharing the label `alpha`, one label-less, the rest `host-04`..`host-50`): the popup is capped inside the window with a scrollbar, and the two `alpha` rows are told apart by `root@10.9.0.1:22` vs `root@10.9.0.2:22`. The label-less entry renders `10.9.0.3:2222`. |
| `evidence/US-0094-menu-50-sessions-scrolled-to-end.png` | the same menu wheel-scrolled to the end: `host-50 — ops@10.9.1.50:22` is on screen with the scrollbar thumb at the bottom. |
| `evidence/US-0094-dialog-from-scrolled-row.png` | clicking that last row opens "Connect to host-50 (ops@10.9.1.50:22)" — a row reachable only after scrolling still routes to its own session. |

The first four were re-taken on the fixed build so that no screenshot in this packet shows
the pre-`D1`/`D2` rendering. The three 50-session shots are new.

The app log for that run confirms the backend was actually entered:

```text
[2026-09-14T12:28:27Z INFO  oneterm_ssh::session] SshSession::connect: host=10.77.0.11, port=22, user=root, rows=24, cols=80
[2026-09-14T12:28:27Z INFO  oneterm_ssh::session] SshSession: connecting to 10.77.0.11:22
[2026-09-14T12:28:47Z ERROR oneterm_ssh::session] SshSession: connect failed: SSH connect failed: timed out after 20 s
```

### Independent verification and the rework it caused

`US-0094` was independently verified at `39c9971` and came back **PASS-WITH-NOTES** with one
medium defect. All four items are addressed in the follow-up commit:

| Item | Verdict | What changed |
| --- | --- | --- |
| **D1** Medium — the section had no cap and no scrolling, so 50 saved sessions ran ~19 rows off the bottom of a maximized window, unreachable by mouse *and* by keyboard (the kit applies its height cap only when `scrollable` is set, and `scroll_to_item` is a no-op outside a scrolling container) | valid, reproduced | `menu.scrollable(true)` in the builder. The kit's default cap is `min(half the window, 450px)`, so no magic number is introduced. Re-proved with 50 sessions: scrollbar present, `host-50` reachable, and a scrolled row still opens its own session. |
| **D2** Low — rows carried the label only, so two saved sessions sharing a label were indistinguishable | valid | `menu_entries` now renders `label — user@host:port` using `session_subtitle`, the helper the session tree already uses four lines above it. A blank label shows the subtitle alone, which subsumes the old `host:port` fallback. |
| **D3** Low — `saved_ssh_sessions` reaches `SshSessionStore::global`, which panics rather than degrading if the session feature's `init` has not run | valid but unreachable | Documented at both ends rather than given a fallback: the doc comment on `saved_ssh_sessions` states the invariant, and `crates/app/src/init.rs` now says why the feature `init`s must precede `AppServices::install`. A silent empty list would hide a wiring bug, and the panic matches the documented startup invariant `AppServices::global` already uses. No runtime code added. |
| **N4/N5** Notes — the section *order* is pinned by nothing automated, and the Gate section quoted one step as if it were the whole gate | valid | Both now disclosed: N4 in Gaps below, N5 in the Gate table above. |

Two tests the verification specified are adopted here. Its worktree
(`agent-a5df640ad16fb7826`) had already been cleaned up when I went to `git apply` its diff,
so they were re-written from the report's specification rather than applied verbatim; both
assert the properties it named, and the positional-id tamper it used fails them here too.

### Acceptance rework proof (2026-09-15)

Code, on top of the above:

| File | Change |
| --- | --- |
| `crates/state/src/commands.rs` | `SavedSshSessionSections` type alias; `saved_ssh_sessions` returns it |
| `crates/state/src/services.rs`, `crates/terminal-view/src/panel/tests.rs` | the two test doubles follow the type |
| `crates/session-ui/src/tree_builder.rs` | `menu_entries` returns sections, title only; 8 unit tests |
| `crates/session-ui/src/session_state.rs` | the id-survives-delete test reads the section's rows |
| `crates/session-ui/src/lib.rs` | the seam's doc comment |
| `crates/terminal-view/src/panel/terminal_panel.rs` | `SeparatorRule` + `labelled_separator`, the menu rebuilt in the owner's order, conditional `scrollable` |
| `docs/gui-layout.md`, `high-level-design.md`, `IN-0033.md` | owning-doc updates |

`crates/terminal-view/Cargo.toml` is still untouched and `crates/app/src/init.rs` needed no
change: the seam's shape changed, not its wiring.

Focused proof — `cargo test -p oneterm-session-ui` **60 passed / 0 failed / 0 ignored**,
`cargo test -p oneterm-terminal-view` **304 passed / 0 failed / 3 ignored**. The model tests:

| Test | Asserts |
| --- | --- |
| `menu_entries_lists_ungrouped_sessions_in_storage_order` | one section with an empty group name, file order, titles only |
| `menu_entries_of_an_empty_store_is_empty` | no sections at all (the view renders the hint) |
| `menu_entries_of_only_grouped_sessions_has_no_ungrouped_section` | no empty leading section when nothing is ungrouped |
| `menu_entries_orders_groups_by_first_appearance` | ungrouped first, then `zeta` before `alpha` because that is the store's order |
| `menu_entries_treats_a_blank_group_as_ungrouped` | `""` and `"  "` are no group; `" infra "` is trimmed |
| `menu_entries_falls_back_to_the_subtitle_for_a_blank_label` | a hand-edited blank label still renders `10.0.0.9:2222` |
| `menu_entries_trims_a_padded_label` | `"  staging  "` renders `staging`, with no subtitle |
| `verify_duplicate_unicode_and_fifty_entries` | duplicate labels are now deliberately identical text (only the id differs), a non-ASCII label survives, 50 entries map 1:1 |
| `verify_menu_row_id_survives_a_delete_of_an_earlier_session` (in `session_state.rs`) | the clicked row's id still resolves to its own host after an earlier session is deleted |

Tamper check, each reverted afterwards:

- sorting the group sections by name → `menu_entries_orders_groups_by_first_appearance`
  FAILED, `alpha` ahead of `zeta` where the store has `zeta` first.
- accepting a blank `group` as a real group → `menu_entries_treats_a_blank_group_as_ungrouped`
  FAILED with two sections named `""` instead of one.

Gate — `pwsh scripts/ci-local.ps1` exit 0, ending `ci-local: all checks passed.` Summed over
every `test result:` line of that run:

| | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| **measured** | **60** | **1979** | **0** | **14** |

(Baseline before this rework: 60 / 1976 / 0 / 14. The eight model tests replace five.)

E2E — Windows interactive desktop, `cargo build -p oneterm-app --profile fast-dev` in this
worktree, the app launched with its working directory set to the worktree so
`oneterm_core::config_dir()` ("target" under `debug_assertions`) resolved inside it. The
store was seeded at `<worktree>/target/ssh_session.json` with `prod-web` and `staging`
(ungrouped), `db-01` and `db-02` (group `infra`) and `sandbox` (group `lab`), and separately
with 50 ungrouped sessions for the cap; the file was deleted afterwards. The owner's real
config was never read or written.

Safety, because the owner runs Claude Code inside their own `oneterm.exe`: `Get-Process
oneterm` was recorded before every launch (owner pid `14804`, the same one before and after
every walk), each instance was started with `Start-Process -PassThru` and identified as the
pid that was not there before (`20772, 19436, 20376, 8128, 20420, 17676, 3788, 17948`), and
only that pid's window was driven, captured and stopped. Nothing was ever matched by process
name or window title.

| Screenshot | Shows |
| --- | --- |
| `evidence/US-0094-rework-menu-grouped-order.png` | the owner's order end to end, labels centred in their rules, the group rules dashed, no scrollbar, and the right dock's tree of the same five sessions beside it |
| `evidence/US-0094-rework-dialog-from-grouped-session.png` | clicking `db-01` under the dashed `infra` heading opens "Connect to db-01 (admin@10.77.0.21:22)" |
| `evidence/US-0094-rework-menu-empty-state.png` | no `ssh_session.json`: the heading, the greyed "No saved sessions", the separator and "New SSH Session" last |
| `evidence/US-0094-rework-menu-50-sessions-scrollable.png` | 50 saved sessions: the popup capped inside the window with its scrollbar — the `D1` fix still engages when it is needed |

### Gaps

- **The scrollbar decision is an estimate, not a measurement.** `scrollable` is switched on
  when `rows × 28px` exceeds the kit's cap; 28px is the kit's 26px item plus its 2px gap,
  and the menu's own bounds do not exist while it is being built. A menu within a row of the
  cap can therefore guess wrong by one row — either a bar that was not needed, or a menu one
  row taller than the cap. Marked in the source with a `ponytail:` comment. The clean fix
  belongs in the kit: a per-menu scrollbar-visibility option, since the only control today
  is the app-wide `ScrollbarMode`, which OneTerm deliberately sets to `Always`.
- **Duplicate labels are indistinguishable in this menu.** Title-only rows are the owner's
  explicit choice, replacing the subtitle `D2` had added. The click still routes by the
  session's stable id, so it opens the right host — the user simply cannot tell the two rows
  apart by reading them. The right dock's tree still shows `user@host:port`.
- **Nothing automated pins the menu's section order.** That local shells come first, then
  "New SSH Session", then the "SSH Sessions" heading and the rows, is proven only by the
  screenshots above: `PopupMenu` exposes no accessor for its built items (only `is_empty()`),
  so a test cannot read the menu back, and swapping the two sections in the builder would
  fail nothing. What *is* pinned by tests is the row mapping, and it bites (see the tamper
  results). Raised as `N4` by the independent verification.
- **The connect-success path was not observed.** No reachable SSH host is available here, so
  the walk stops at a timed-out attempt. That proves the menu reaches the connect path with
  the right session, and everything past the dialog is the SSH Sessions panel's own
  already-shipped code (the same `open_connect_dialog` call), but a tab actually opening from
  the menu was not seen end to end.
- **Windows only.** The new section is platform-independent — only the local shell entries
  above it are `#[cfg]`-split — but macOS and Linux were not run.
- **The disabled hint has no unit test.** Rendering it needs gpui and the branch is three
  lines; it is covered by `menu_entries_of_an_empty_store_is_empty` on the model side and by
  the empty-state screenshot. The same applies to `scrollable(true)`: the cap and the
  scrollbar are the kit's, and the proof is the two 50-session screenshots, not a test.
- **A row's text can outgrow the popup's width.** `label — user@host:port` is longer than the
  label alone, and the kit clamps `max_w`; a very long label plus a long user and host will
  be clipped. Not observed at 50 realistic entries, and the id still routes the click, so
  this is a legibility ceiling rather than a correctness one.
- **`menu_entries` has one caller.** It is `pub(crate)` in `crates/session-ui` and is used
  only by `saved_ssh_sessions`. It lives in `tree_builder.rs` rather than a new module
  because that file already owns "turn store entries into list rows" and already had the
  `host:port` helper next to it.
- The deleted-between-open-and-click case (`get` returns `None`, nothing happens) is
  reasoned from the store API, not exercised: provoking it needs a delete between the menu
  render and the click.

### harness.db rows

Not written from this worktree — `harness.db` is gitignored and worktree-local, so a row
written here would be lost. To mirror in the main checkout after merge:

```python
import sqlite3

conn = sqlite3.connect("harness.db")
cur = conn.cursor()

cur.execute(
    """
    INSERT INTO intake (input_type, summary, risk_lane, affected_docs, story_id,
                        doc_path, document_number, design_doc, notes)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    """,
    (
        "new_spec",
        'The "+" (New Terminal) button must also list the sessions saved under SSH Sessions, '
        "so a saved SSH session opens from the same place a local shell does.",
        "normal",
        "docs/gui-layout.md,docs/ssh-client-connect.md",
        "US-0094",
        "docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/IN-0033.md",
        33,
        "docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/high-level-design.md",
        "Owner request 2026-09-14, made in Vietnamese and recorded in English because "
        "scripts/check-english.py bans Vietnamese in docs. terminal-view cannot depend on "
        "session-ui (R1 cycle + R5), so the saved list crosses at "
        "oneterm_state::commands::WorkspaceCommands.",
    ),
)
intake_id = cur.lastrowid

cur.execute(
    """
    INSERT INTO story (id, title, risk_lane, packet_doc, status,
                       unit_proof, integration_proof, e2e_proof, platform_proof,
                       evidence, verify_command, last_verified_at, last_verified_result,
                       notes, intake_id)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """,
    (
        "US-0094",
        'The "+" menu lists the saved SSH sessions and opens one',
        "normal",
        "docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/"
        "US-0094-new-terminal-menu-lists-saved-ssh-sessions.md",
        "implemented",
        1,
        1,
        1,
        1,
        "docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/"
        "US-0094-new-terminal-menu-lists-saved-ssh-sessions.md section 'Evidence and Gaps'. "
        "Two fn pointers on WorkspaceCommands (saved_ssh_sessions, open_saved_ssh_session) carry "
        "the saved list and the open-by-id call from crates/session-ui to the menu built in "
        "crates/terminal-view, so no crate edge is added and R1/R5 hold; the connect path is "
        "reused, not duplicated. Stable ids, not indexes, cross the seam so a concurrent delete "
        "cannot retarget the click. FOCUSED: 6 tests (5 menu_entries in "
        "crates/session-ui/src/tree_builder.rs, 1 id-survives-delete in session_state.rs); "
        "iterating .rev() fails the ordering test, disabling the blank-label fallback fails the "
        "fallback test, and handing the seam the position instead of the id fails 5 (all "
        "reverted). cargo test -p oneterm-session-ui 57 passed / 0 failed; "
        "-p oneterm-terminal-view 288 passed / 0 failed / 3 ignored. "
        "GATE pwsh scripts/ci-local.ps1 exit 0, all ten steps, 'ci-local: all checks passed', "
        "summed over every test-result line: 60 sections / 1941 passed / 0 failed / 14 ignored. "
        "E2E Windows desktop, fast-dev build, app launched with its working directory set to the "
        "worktree so config_dir() ('target') resolved inside the worktree and the owner's real "
        "config was never touched: empty-state hint with no ssh_session.json, then both seeded "
        "names flat in file order with their user@host:port while the right dock tree nested "
        "db-01 under its group, then clicking prod-web opened 'Connect to prod-web "
        "(root@10.77.0.11:22)' and Connect reached the backend (oneterm_ssh::session "
        "SshSession::connect host=10.77.0.11 ... timed out after 20 s); then 50 seeded sessions "
        "showing the scrollbar, host-50 reachable by wheel, and clicking it opening 'Connect to "
        "host-50 (ops@10.9.1.50:22)'. 7 screenshots under evidence/US-0094-*.png. "
        "INDEPENDENT VERIFICATION at 39c9971: PASS-WITH-NOTES, one medium defect, all closed in "
        "the follow-up commit -- D1 (no cap/scroll: 50 sessions ran ~19 rows off the window, "
        "unreachable by mouse and keyboard) fixed by menu.scrollable(true), which is what makes "
        "the kit apply its min(half-window, 450px) cap; D2 (duplicate labels indistinguishable) "
        "fixed by rendering 'label - user@host:port' via the existing session_subtitle helper; "
        "D3 (SshSessionStore::global panics if the feature init has not run) documented at both "
        "ends rather than given a fallback, since a silent empty list would hide a wiring bug and "
        "the panic matches the AppServices::global startup invariant; N4/N5 disclosed in the "
        "packet. The verifier's 2 tests are adopted, re-written from its report because its "
        "worktree was cleaned up before its diff could be applied. "
        "GAPS: nothing automated pins the menu SECTION ORDER (PopupMenu has no item accessor, so "
        "the screenshots are the only proof); no reachable host, so the connect-success path is "
        "unobserved; Windows only; the disabled hint and scrollable(true) have no unit test "
        "(both need gpui); a very long label plus user@host can be clipped by the popup's max_w.",
        "pwsh scripts/ci-local.ps1",
        "2026-09-14T20:05:00",
        "pass",
        "Implemented on worktree agent-a85e44f034eb1c863 off main @4dd57e7 in three commits: "
        "aff3919 records (before any code), 39c9971 implementation, and the verification-rework "
        "commit on top. NOT merged, NOT pushed. Docs changed: docs/gui-layout.md (Panel "
        "registration and presentation + Source map) and docs/ssh-client-connect.md (1.1 and "
        "decision 8). Safety: the owner runs Claude Code inside their own oneterm.exe, so "
        "Get-Process oneterm was recorded before every launch (owner pids 2504, 14804 each time), "
        "the app was started with Start-Process -PassThru, and only the launched pid (13816, "
        "13200, 14180, 15896, 17068) was driven, screenshotted and stopped; nothing was matched "
        "by process name or window title, and both owner pids were alive after every walk.",
        intake_id,
    ),
)

conn.commit()
```

Acceptance rework, 2026-09-15 — the `story` row for `US-0094` goes through reopened and back
to implemented. Run in the main checkout after merge (the rows above must exist first):

```python
import sqlite3

conn = sqlite3.connect("harness.db")
cur = conn.cursor()

cur.execute(
    """
    UPDATE story
       SET status = ?,
           evidence = evidence || ?,
           last_verified_at = ?,
           last_verified_result = ?,
           notes = notes || ?
     WHERE id = 'US-0094'
    """,
    (
        "implemented",
        " ACCEPTANCE REWORK 2026-09-15 (reopened, then re-implemented): the owner tried the "
        "built menu and asked for a different layout, so per docs/HARNESS.md this packet was "
        "reopened rather than a new BUG filed. New order: shells, a separator labelled 'SSH "
        "Sessions', the ungrouped sessions, then per group in store order a dashed separator "
        "carrying the group name and that group's sessions, then a separator and 'New SSH "
        "Session' last. Rows are TITLE ONLY, which withdraws the subtitle D2 had added: two "
        "saved sessions sharing a label are now indistinguishable in this menu, the owner's "
        "explicit choice (the right dock's tree still shows user@host:port); a blank "
        "hand-edited label still falls back to host:port. menu_entries returns sections "
        "(ungrouped first, then groups in first-appearance order) and the seam type is named "
        "SavedSshSessionSections in crates/state; the id-not-index seam is unchanged. The kit "
        "has a Separator and a Label but no item that is both, so each heading is a disabled "
        "PopupMenuItem::element -- rule, centred label, rule -- with border_dashed() for a "
        "group and theme colours only. Two owner tweaks in the same session: the labels are "
        "centred, and the popup's scrollbar is hidden unless the rows exceed the kit's cap "
        "(min(half the window, 450px)), because the kit caps the height only when scrollable "
        "and this app's theme sets ScrollbarMode::Always. FOCUSED: 8 menu_entries tests plus "
        "the id-survives-delete test; sorting the group sections fails the store-order test "
        "and accepting a blank group fails the blank-group test (both reverted). "
        "cargo test -p oneterm-session-ui 60 passed / 0 failed; -p oneterm-terminal-view 304 "
        "passed / 0 failed / 3 ignored. GATE pwsh scripts/ci-local.ps1 exit 0, 60 sections / "
        "1979 passed / 0 failed / 14 ignored. E2E Windows desktop, fast-dev, working directory "
        "set to the worktree so config_dir() resolved inside it and the owner's real config was "
        "never touched: seeded 2 ungrouped + infra x2 + lab x1 (the owner's order rendered, "
        "labels centred, group rules dashed, no scrollbar), clicking db-01 under the dashed "
        "infra heading opened 'Connect to db-01 (admin@10.77.0.21:22)', the empty store showed "
        "the hint with New SSH Session last, and a 50-session store still capped the popup with "
        "its scrollbar. 4 screenshots under evidence/US-0094-rework-*.png; the seed file was "
        "deleted afterwards. NEW GAP: the scrollbar decision estimates 28px per row rather than "
        "measuring the popup, so a menu within a row of the cap can guess wrong by one row "
        "(ponytail: comment in the source); the clean fix is a per-menu scrollbar option in the "
        "kit.",
        "2026-09-15T09:30:00",
        "pass",
        " Acceptance rework implemented on worktree agent-a580e6e3c74ab401d off main @621e4c9 "
        "in four commits (reopen record, implementation, the owner tweaks, evidence). NOT merged, NOT pushed. Docs "
        "changed: docs/gui-layout.md, the intake's high-level-design.md and IN-0033.md. Safety: "
        "Get-Process oneterm recorded before every launch (owner pid 14804 alive before and "
        "after every walk), Start-Process -PassThru, and only the launched pids (20772, 19436, "
        "20376, 8128, 20420, 17676, 3788, 17948) were driven, captured and stopped; nothing was "
        "matched by process name or window title.",
    ),
)

conn.commit()
```

## Handoff

Implemented, independently verified (PASS-WITH-NOTES), reworked for that verification, and
then reworked again for the owner's acceptance feedback on 2026-09-15 — that last pass on
worktree `agent-a580e6e3c74ab401d` off `main` @ `621e4c9`. Not merged, not pushed. All four
verification items (`D1`-`D3`, `N4`/`N5`) and the owner's layout, centring and scrollbar
requests are closed. Next owner: merge, then mirror the `harness.db` rows above — the two
inserts first, then the acceptance-rework update — in the main checkout.
