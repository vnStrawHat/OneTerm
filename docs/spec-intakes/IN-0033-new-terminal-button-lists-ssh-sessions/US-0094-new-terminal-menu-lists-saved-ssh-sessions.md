# Work: The "+" menu lists the saved SSH sessions and opens one

ID: US-0094
Intake: IN-0033
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] In scope:
  - `WorkspaceCommands` gains `saved_ssh_sessions` (read the saved list) and
    `open_saved_ssh_session` (open the connect dialog by stable id).
  - `crates/session-ui` implements both and exposes the pure mapping from stored entries to
    menu rows.
  - `TerminalPanel::title_suffix` renders the new section.
  - `crates/app/src/init.rs` wires the two new fn pointers.
  - Unit tests for the mapping; owning docs updated.
- [ ] Out of scope:
  - Grouping the menu by the session `group` field (intake open decision: flat).
  - Opening the session into the active Space instead of a new tab — that is what the SSH
    Sessions panel does today and matching it verbatim is the point of this packet.
  - Any change to the connect dialog, credentials, jump chains, host-key policy or logging.
  - Search or filtering inside the menu.

## Acceptance

- [ ] With two sessions saved, opening the "+" menu shows an "SSH Sessions" section listing
      both names, in the order they appear in `ssh_session.json`, below the unchanged local
      shell entries and "New SSH Session".
- [ ] Clicking a listed session opens the connect dialog for **that** session (title and
      server banner name its host), and Connect starts a real connection attempt — reaching
      the host is not required.
- [ ] With no sessions saved, the section shows a disabled "No saved sessions" hint and the
      rest of the menu is unchanged from today.
- [ ] The row mapping maps N stored entries to N rows in storage order, an empty store to no
      rows, and a blank label to `host:port`, proven by unit tests.
- [ ] No new crate edge: `crates/terminal-view` still does not depend on
      `crates/session-ui`, and `crates/terminal-view/Cargo.toml` is unchanged.
- [ ] `pwsh scripts/ci-local.ps1` is green.

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

To be completed before this packet is marked implemented: list the docs changed, or confirm
the recorded no-change reasons above still hold.

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

- [ ] Add `saved_ssh_sessions` and `open_saved_ssh_session` to `WorkspaceCommands`, and to
      its test doubles (`crates/state/src/services.rs`,
      `crates/terminal-view/src/panel/tests.rs`).
- [ ] Give `SshSessionId` a raw-`u64` round trip for the seam.
- [ ] Add the entries mapping plus its unit tests in `crates/session-ui`.
- [ ] Export `saved_ssh_sessions` / `open_saved_ssh_session` from `crates/session-ui`.
- [ ] Render the section in `TerminalPanel::title_suffix`.
- [ ] Wire both at the composition root in `crates/app/src/init.rs`.
- [ ] Update the two owning docs.
- [ ] `cargo fmt --all`, then `pwsh scripts/ci-local.ps1`.
- [ ] GUI walk on the Windows desktop: empty state, two seeded sessions, click one.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To be recorded after implementation: commands run, results, screenshots, and anything
skipped, unavailable, partial, or failing. The `harness.db` rows this packet would insert go
here as a runnable snippet — `harness.db` is gitignored and worktree-local, so it is mirrored
in the main checkout after merge rather than written from here.

## Handoff

Records written before any implementation edit, on worktree `agent-a85e44f034eb1c863` off
`main` @ `4dd57e7`.
