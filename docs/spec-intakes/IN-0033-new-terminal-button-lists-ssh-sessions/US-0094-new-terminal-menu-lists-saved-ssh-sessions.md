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

- [x] With two sessions saved, opening the "+" menu shows an "SSH Sessions" section listing
      both names, in the order they appear in `ssh_session.json`, below the unchanged local
      shell entries and "New SSH Session".
      *Evidence:* `evidence/US-0094-menu-with-two-saved-sessions.png` — `prod-web` then
      `db-01`, file order, flat, while the right dock's tree shows `db-01` nested under its
      `infra` group in the same frame.
- [x] Clicking a listed session opens the connect dialog for **that** session (title and
      server banner name its host), and Connect starts a real connection attempt — reaching
      the host is not required.
      *Evidence:* `evidence/US-0094-connect-dialog-for-clicked-session.png` (titled
      "Connect to prod-web (root@10.77.0.11:22)", banner `ssh://root@10.77.0.11:22`) and
      `evidence/US-0094-connect-failed-notification.png`, plus the app log line
      `oneterm_ssh::session] SshSession::connect: host=10.77.0.11, port=22, user=root`.
- [x] With no sessions saved, the section shows a disabled "No saved sessions" hint and the
      rest of the menu is unchanged from today.
      *Evidence:* `evidence/US-0094-menu-empty-state.png`.
- [x] The row mapping maps N stored entries to N rows in storage order, an empty store to no
      rows, and a blank label to `host:port`, proven by unit tests.
      *Evidence:* the four `menu_entries_*` tests in `crates/session-ui/src/tree_builder.rs`,
      with the tamper results recorded below.
- [x] No new crate edge: `crates/terminal-view` still does not depend on
      `crates/session-ui`, and `crates/terminal-view/Cargo.toml` is unchanged.
      *Evidence:* `python scripts/verify-dependency-graph.py` — "Dependency graph policy
      passed for 21 workspace packages and 21 explicit members"; `git status` never lists
      `crates/terminal-view/Cargo.toml`.
- [x] `pwsh scripts/ci-local.ps1` is green. *Evidence:* "ci-local: all checks passed."

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
| `crates/session-ui/src/tree_builder.rs` | `menu_entries` + 4 unit tests |
| `crates/session-ui/src/lib.rs` | `saved_ssh_sessions`, `open_saved_ssh_session` |
| `crates/terminal-view/src/panel/terminal_panel.rs` | the menu section |
| `crates/terminal-view/src/panel/tests.rs` | the duplicate-flow test double gained both fields |
| `crates/app/src/init.rs` | composition-root wiring |
| `docs/gui-layout.md`, `docs/ssh-client-connect.md` | owning-doc updates |

`crates/terminal-view/Cargo.toml` is untouched: no new dependency, no new crate edge. The
connect path is not duplicated anywhere — `open_saved_ssh_session` is a store lookup plus the
existing `open_connect_dialog` call, which is the same two lines `SessionPanel::on_open_session`
runs.

### Focused proof

`cargo test -p oneterm-session-ui` — **55 passed, 0 failed, 0 ignored** (plus 0 doc-tests),
including the four new tests:

| Test | Asserts |
| --- | --- |
| `menu_entries_keeps_storage_order_and_ids` | three entries (ids 7, 2, 5; one of them grouped) come back in file order with their own ids, not sorted and not grouped |
| `menu_entries_of_an_empty_store_is_empty` | the empty state at the model level |
| `menu_entries_falls_back_to_host_and_port_for_a_blank_label` | a hand-edited whitespace label renders `10.0.0.9:2222`, not an invisible row |
| `menu_entries_trims_a_padded_label` | `"  staging  "` renders `staging` |

Tamper check, both reverted afterwards:

- iterating the store with `.rev()` → `menu_entries_keeps_storage_order_and_ids` FAILED with
  `left: [(5, "db"), (2, "alpha"), (7, "prod")]` against
  `right: [(7, "prod"), (2, "alpha"), (5, "db")]`.
- disabling the blank-label fallback →
  `menu_entries_falls_back_to_host_and_port_for_a_blank_label` FAILED with
  `left: [(3, "")]` against `right: [(3, "10.0.0.9:2222")]`.

### Gate

`pwsh scripts/ci-local.ps1` from this worktree — exit 0, all ten steps, ending
`ci-local: all checks passed.` `verify-dependency-graph.py` passed for 21 workspace packages
and 21 explicit members; `check-doc-paths.py` passed for 120 paths in 10 documents;
`check-english.py` passed for 777 files.

`cargo test --workspace` tallied separately: **56 sections, 1568 passed, 0 failed,
11 ignored.**

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
| `evidence/US-0094-menu-with-two-saved-sessions.png` | `target/ssh_session.json` seeded with `prod-web` (id 1, ungrouped) and `db-01` (id 2, group `infra`): the menu lists both, flat, in file order, while the right dock's tree in the same frame nests `db-01` under `infra` — the flat-vs-grouped decision, visible side by side. |
| `evidence/US-0094-connect-dialog-for-clicked-session.png` | clicking the `prod-web` row opens "Connect to prod-web (root@10.77.0.11:22)" with banner `ssh://root@10.77.0.11:22` — the clicked session, not another one. |
| `evidence/US-0094-connect-failed-notification.png` | Connect reaches the real connect path: the error notification "SSH connect failed: SSH connect failed: timed out after 20 s". |

The app log for that run confirms the backend was actually entered:

```text
[2026-09-14T12:28:27Z INFO  oneterm_ssh::session] SshSession::connect: host=10.77.0.11, port=22, user=root, rows=24, cols=80
[2026-09-14T12:28:27Z INFO  oneterm_ssh::session] SshSession: connecting to 10.77.0.11:22
[2026-09-14T12:28:47Z ERROR oneterm_ssh::session] SshSession: connect failed: SSH connect failed: timed out after 20 s
```

### Gaps

- **The connect-success path was not observed.** No reachable SSH host is available here, so
  the walk stops at a timed-out attempt. That proves the menu reaches the connect path with
  the right session, and everything past the dialog is the SSH Sessions panel's own
  already-shipped code (the same `open_connect_dialog` call), but a tab actually opening from
  the menu was not seen end to end.
- **Windows only.** The new section is platform-independent — only the local shell entries
  above it are `#[cfg]`-split — but macOS and Linux were not run.
- **The disabled hint has no unit test.** Rendering it needs gpui and the branch is three
  lines; it is covered by `menu_entries_of_an_empty_store_is_empty` on the model side and by
  the empty-state screenshot.
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
        "cannot retarget the click. FOCUSED: 4 menu_entries tests in "
        "crates/session-ui/src/tree_builder.rs; iterating .rev() fails the ordering test and "
        "disabling the blank-label fallback fails the fallback test (both reverted). "
        "cargo test -p oneterm-session-ui 55 passed / 0 failed. GATE pwsh scripts/ci-local.ps1 "
        "exit 0, all ten steps, 'ci-local: all checks passed'; cargo test --workspace tallied "
        "56 sections 1568 passed / 0 failed / 11 ignored. E2E Windows desktop, fast-dev build, "
        "app launched with its working directory set to the worktree so config_dir() ('target') "
        "resolved inside the worktree and the owner's real config was never touched: empty-state "
        "hint with no ssh_session.json, then both seeded names flat in file order while the right "
        "dock tree nested db-01 under its group, then clicking prod-web opened 'Connect to "
        "prod-web (root@10.77.0.11:22)' and Connect reached the backend "
        "(oneterm_ssh::session SshSession::connect host=10.77.0.11 ... timed out after 20 s). "
        "Screenshots: evidence/US-0094-{menu-empty-state,menu-with-two-saved-sessions,"
        "connect-dialog-for-clicked-session,connect-failed-notification}.png. "
        "GAPS: no reachable host, so the connect-success path is unobserved; Windows only; the "
        "disabled hint has no unit test (needs gpui, 3-line branch); the deleted-between-open-"
        "and-click no-op is reasoned, not exercised.",
        "pwsh scripts/ci-local.ps1",
        "2026-09-14T19:30:00",
        "pass",
        "Implemented on worktree agent-a85e44f034eb1c863 off main @4dd57e7. NOT merged, NOT "
        "pushed. Docs changed: docs/gui-layout.md (Panel registration and presentation + Source "
        "map) and docs/ssh-client-connect.md (1.1 and decision 8). Safety: the owner runs Claude "
        "Code inside their own oneterm.exe, so Get-Process oneterm was recorded before each "
        "launch (owner pids 2504, 14804), the app was started with Start-Process -PassThru, and "
        "only the launched pid (13816, then 13200) was driven, screenshotted and stopped; nothing "
        "was matched by process name or window title.",
        intake_id,
    ),
)

conn.commit()
```

## Handoff

Implemented and proven on worktree `agent-a85e44f034eb1c863` off `main` @ `4dd57e7`. Not
merged, not pushed. Next owner: merge, then mirror the two `harness.db` rows above in the
main checkout.
