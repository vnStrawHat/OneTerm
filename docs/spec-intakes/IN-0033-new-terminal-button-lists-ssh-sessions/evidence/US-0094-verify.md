# Independent verification: US-0094 (IN-0033)

Verifier: a second agent, not the implementer.
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a5df640ad16fb7826`
Under review: `git diff 4dd57e7..39c9971` (`aff3919` docs, `39c9971` code), HEAD reset to `39c9971`.
Date: 2026-09-14.

## Verdict

**PASS-WITH-NOTES.**

The seam is right (no crate edge, one connect path, stable ids rather than positions), the
gate is green in this worktree at 60/1939/0/14, the menu shape matches the HLD, and the two
claims the packet only reasoned about — id-not-index routing, and a list rebuilt per open —
I reproduced in the running app: clicking the second of two identically labelled rows opened
the second host, and a session deleted in the SSH Sessions panel while the app ran was gone
from the "+" menu on the next open.

One defect is real and reproduced: **D1**, the section has no cap and no scrolling, so with
50 saved sessions ~19 rows fall off the bottom of a maximized window and cannot be reached by
mouse or keyboard. It is a one-line fix and it is not a documented trade-off anywhere in the
packet or the HLD. D2/D3 are minor; N4-N5 are gaps in the evidence, not in the code. Nothing
found is a correctness bug in the connect path.

## Defects and notes

### D1 — Medium — the saved-session list has no cap and no scrolling

- `crates/terminal-view/src/panel/terminal_panel.rs:626-637` appends one `PopupMenuItem` per
  saved session and never calls `PopupMenu::scrollable(true)` or `max_h(..)`.
- In `gpui-component 0.6.0` the popup applies its height limit **only** when `scrollable` is
  set: `popup_menu.rs:1449-1453` is `.when(self.scrollable, |this| this.max_h(max_height)
  .overflow_y_scroll().track_scroll(&self.scroll_handle))`, and the matching scrollbar is
  also `.when(self.scrollable, ..)`. `scrollable` defaults to `false`
  (`popup_menu.rs:338`) and is auto-enabled only on the `with_menu_items` path when the
  item count exceeds 20 (`popup_menu.rs:795-797`) — a path the `.item()` / `.menu()`
  builders used here never take.
- Consequence: with a long `ssh_session.json` the popup grows past the window; entries below
  the bottom edge have no scrollbar, and `set_selected_index`'s
  `scroll_handle.scroll_to_item(ix)` (`popup_menu.rs:882`) is a no-op without a scrolling
  container, so keyboard navigation cannot bring them into view either.
- **Reproduced** (§9): 50 saved sessions, maximized 1936x1048 window — the popup is cut off at
  `host-31`; `host-32`..`host-50` are off-screen, no scrollbar, nothing to scroll.
  `evidence/US-0094-verify-menu-50-sessions-clipped.png`.
- Fix is one line in the builder: `menu.scrollable(true)` (the "+" menu has no submenus, so
  the kit's submenu caveat on `scrollable` does not apply), or a `max_h`.
- Neither the packet nor the HLD states a bound on the list, so this is an unbounded UI built
  from user data with no stated ceiling, not a documented trade-off.

### D2 — Low — duplicate labels are indistinguishable in the menu

- `crates/session-ui/src/tree_builder.rs:44-56` maps each entry to its label only. Two saved
  sessions may legitimately share a label (nothing in `SshSessionStore::add` enforces
  uniqueness), and the menu then shows two identical rows with no way to tell them apart.
- The SSH Sessions panel does not have this problem: it renders `session_subtitle`
  (`user@host:port`) next to the label — the helper lives four lines above `menu_entries` in
  the same file (`tree_builder.rs:31-36`).
- Proven by `verify_duplicate_unicode_and_fifty_entries` (added below): two rows, same
  string, different ids — and in the GUI (§9), two rows reading `alpha` with nothing to tell
  `10.9.0.1` from `10.9.0.2`.
- Not a correctness bug — the click still opens the right session, because the id, not the
  text, is what travels — but it is a user-visible ambiguity that the panel already solved.

### D3 — Low — "what if the command is unset" has no fallback path, only a panic

- The task's premise of an unset (`None`) field does not apply: `WorkspaceCommands`
  (`crates/state/src/commands.rs:29-37`) holds plain `fn` pointers, not `Option`s, so a
  missing registration is a compile error at the single construction site
  (`crates/app/src/init.rs:62-63`) and in the two test doubles. Verified: those are the only
  three `WorkspaceCommands { .. }` literals in the workspace.
- What *can* fail at runtime is the global behind the command:
  `oneterm_session_ui::saved_ssh_sessions` (`crates/session-ui/src/lib.rs:37-39`) calls
  `SshSessionStore::global`, which is `cx.global::<SshSessionStoreGlobal>()`
  (`session_state.rs:574-576`) and panics if the session feature's `init` did not run.
  Today `crates/app/src/init.rs:41` runs it before `AppServices::install` at line 56, and the
  menu builder can only run after a window exists, so it is unreachable — but the failure
  mode is a panic in a menu builder, not an empty list.
- `oneterm_state::commands::commands(cx)` at `terminal_panel.rs:625` has the same property
  (`AppServices::global` panics by documented startup invariant). Both are pre-existing
  patterns; nothing here degrades gracefully, and nothing here is claimed to.

### N4 — Note — nothing automated pins the menu's section order

- The order (shells → separator → "New SSH Session" → separator → "SSH Sessions" label →
  rows) is proven only by screenshots. `PopupMenu` exposes no accessor for its items (only
  `is_empty()`), so a test cannot read the built menu back; reversing the two sections in the
  builder would fail nothing. The packet discloses the sibling gap ("the disabled hint has no
  unit test") but not this one.
- What *is* pinned by tests is the row mapping, and it bites hard (tamper results below).

### N5 — Note — the packet's Gate section under-reports the suite

- The packet quotes `cargo test --workspace` as "56 sections, 1568 passed, 0 failed,
  11 ignored". That is one ci-local step. The full `pwsh scripts/ci-local.ps1` run tallies
  **60 sections / 1939 passed / 0 failed / 14 ignored** (below). Not wrong, just narrower
  than "the gate".

## Verification performed

### 1. Crate graph and the fn-pointer registry

```
$ git diff 4dd57e7..39c9971 -- '*/Cargo.toml'
(empty)

==> python scripts/verify-dependency-graph.py
Dependency graph policy passed for 21 workspace packages and 21 explicit members.
```

`crates/terminal-view` still has no `oneterm-session-ui` dependency; the list and the open
call cross at `oneterm_state::commands::WorkspaceCommands`, which is what R1/R5/R10 require
(session-ui → terminal-view is the one same-layer edge, so the reverse is a cycle). R12 is
untouched: no panel registration moved. Registration is in exactly one place
(`crates/app/src/init.rs:62-63`); the only other `WorkspaceCommands` literals are the two
test doubles (`crates/state/src/services.rs:149-150`,
`crates/terminal-view/src/panel/tests.rs:462-463`). Unset is not representable — see D3.

### 2. Stable ids, not positions

Two tests added by me (uncommitted, in this worktree):

| Test | File | Result |
| --- | --- | --- |
| `session_state::tests::verify_menu_row_id_survives_a_delete_of_an_earlier_session` | `crates/session-ui/src/session_state.rs` | ok |
| `tree_builder::tests::verify_duplicate_unicode_and_fifty_entries` | `crates/session-ui/src/tree_builder.rs` | ok |

The first builds the rows for A(id 1, `10.9.0.1`) and B(id 2, `10.9.0.2`), deletes A from the
store, then resolves the row that was B **through `SshSessionStore::get`** — the exact call
`open_saved_ssh_session` makes: it still returns `10.9.0.2`, and A's id resolves to `None`
(the documented no-op). The second covers a duplicate label, a non-ASCII label
(`日本-🚀`), and a 50-entry store (50 rows, 1:1, first and last checked). The empty store and
the whitespace-label → `host:port` fallback were already covered by the implementer's tests.

```
$ cargo test -p oneterm-session-ui
test session_state::tests::verify_menu_row_id_survives_a_delete_of_an_earlier_session ... ok
test tree_builder::tests::menu_entries_falls_back_to_host_and_port_for_a_blank_label ... ok
test tree_builder::tests::menu_entries_keeps_storage_order_and_ids ... ok
test tree_builder::tests::menu_entries_of_an_empty_store_is_empty ... ok
test tree_builder::tests::menu_entries_trims_a_padded_label ... ok
test tree_builder::tests::verify_duplicate_unicode_and_fifty_entries ... ok
test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Tampers (both reverted; `cargo test -p oneterm-session-ui` back to 57 passed / 0 failed
afterwards):

- **reverse the iteration** (`sessions.iter().rev()`) → 3 failures:
  `menu_entries_keeps_storage_order_and_ids`, `verify_duplicate_unicode_and_fifty_entries`,
  `verify_menu_row_id_survives_a_delete_of_an_earlier_session`.
- **hand the seam the position instead of the id** (`.enumerate()`, `(index as u64, name)`) →
  5 failures, including
  ```
  ---- session_state::tests::verify_menu_row_id_survives_a_delete_of_an_earlier_session
  assertion `left == right` failed
    left: [(0, "alpha"), (1, "bravo")]
   right: [(1, "alpha"), (2, "bravo")]
  ```
  i.e. the index-based design the packet argues against is caught by the suite.

Cap / scroll for 50 sessions: the model maps all 50 (test above); the **rendering** is the
problem — see D1 and the GUI walk.

### 3. Menu ordering and shape

`git diff 4dd57e7..39c9971 -- crates/terminal-view/src/panel/terminal_panel.rs` leaves the
whole `#[cfg(windows)]` / `#[cfg(not(windows))]` shell block untouched; the only change above
the new separator is that `menu.separator().menu("New SSH Session", Box::new(NewSession))`
became `menu = menu.separator().menu("New SSH Session", Box::new(NewSession));` — the same
two items in the same order, tail expression turned into an assignment. Everything the user
sees above the new separator is byte-identical.

Below it: `separator` → `label("SSH Sessions")` → either the rows or
`PopupMenuItem::new("No saved sessions").disabled(true)`. In the kit a disabled item gets no
click listener (`popup_menu.rs:1269-1274`) and a `Label` renders `.disabled(true)
.cursor_default()` (`popup_menu.rs:1221`), so neither the hint nor the heading is clickable.
Confirm-by-keyboard on the hint closes the menu and does nothing else
(`popup_menu.rs:828-862`).

Tamper for the section order: **not possible** — see N4.

### 4. Freshness of the list

`Button::dropdown_menu` takes an `Fn` and the kit caches the built `PopupMenu` in
`use_keyed_state`, but it **clears that cache on `DismissEvent`**
(`gpui-component-0.6.0/src/menu/dropdown_menu.rs:117-147`: `state.menu = None` inside the
subscription), so the builder closure re-runs on the next open. The closure reads
`(commands.saved_ssh_sessions)(cx)` each time (`terminal_panel.rs:627`) and that reads the
live `SshSessionStore` entity, so an add / delete / rename done in the SSH Sessions panel
while the app runs shows up on the next open with no observer wiring. Verified by code reading
on both halves **and reproduced in the running app** (§9: delete in the panel → the row is
gone from the next "+" open, no restart).

Caveat worth knowing, not a defect of this packet: the store is loaded from
`ssh_session.json` once at `init`, so editing that file *outside* the running app is not
picked up. Nothing in the docs claims otherwise.

### 5. One connect path

```
$ grep -rn "open_connect_dialog" crates/ --include=*.rs
crates/session-ui/src/connect_dialog.rs:51:pub(crate) fn open_connect_dialog(
crates/session-ui/src/lib.rs:50:        connect_dialog::open_connect_dialog(session, id, window, cx);
crates/session-ui/src/panel.rs:164:  super::connect_dialog::open_connect_dialog(s, id, window, cx);
crates/session-ui/src/tree_render.rs:144,214:  open_connect_dialog(s, session_id, window, cx);
```

`lib.rs:44-52` is `store.get(id).cloned()` + that call — the same two lines
`SessionPanel::on_open_session` runs (`panel.rs:162-166`), same argument types, same id. No
second dialog, no second connect, nothing new in `crates/terminal-view` that talks to SSH.

### 6. Theme and icons

The new code adds no colour, no icon and no literal size: one `PopupMenu::label`, N
`PopupMenuItem::new(..)`, one `.disabled(true)`. All colours come from `cx.theme()` inside the
kit. The `#[cfg(windows)]` / `#[cfg(not(windows))]` split stays exactly where it was, around
the local-shell entries only, as the packet says.

### 7. Docs and records

- `docs/gui-layout.md` §Panel registration and presentation: accurate — names the three
  sections, the flat storage order, the empty-state hint, the `WorkspaceCommands` seam and
  why it exists. Source map gained two correct paths (`check-doc-paths.py` passes).
- `docs/ssh-client-connect.md` §1.1 and §1.3 decision 8: accurate — the "+" menu is recorded
  as a second entry into the same flow with the same `SshSessionId`, and decision 8 no longer
  reads as "the panel is the only way in".
- Acceptance boxes: each of the six is backed by evidence I checked. The four PNGs match
  their claims — I opened all four: `menu-with-two-saved-sessions.png` shows `prod-web` then
  `db-01` flat in the menu while the right dock nests `db-01` under `infra` in the same
  frame; `connect-dialog-for-clicked-session.png` shows "Connect to prod-web
  (root@10.77.0.11:22)" with banner `ssh://root@10.77.0.11:22`;
  `menu-empty-state.png` shows the greyed "No saved sessions" under the "SSH Sessions"
  heading; `connect-failed-notification.png` shows the timeout notification.
- Intake `IN-0033.md`: the `US-0094` packet line is ticked.
- The Evidence `harness.db` snippet inserts `intake` **without** an `id` (autoincrement),
  with `document_number` 33 and `story_id` `US-0094`, then inserts `story` `US-0094` with
  `intake_id = cur.lastrowid`. Shape is as required.

### 8. Gate — `pwsh scripts/ci-local.ps1` in this worktree (HEAD `39c9971`, clean)

```
ci-local: all checks passed.        [exited with code 0]
```

All ten steps ran (`cargo fmt --check`, `clippy --workspace --all-targets -D warnings`,
`cargo test --workspace`, `cargo test -p oneterm-vt --features vt-paranoid`,
`verify-dependency-graph.py`, `check-doc-paths.py`, `test_check_english.py`,
`check-english.py`, `completion-catalog.py validate`, `third-party-notices.py --check`).

Summed over every `test result:` line in the run:

| | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| expected (main 60/1935/0/14 + 4 new tests) | 60 | 1939 | 0 | 14 |
| **measured** | **60** | **1939** | **0** | **14** |

`Dependency graph policy passed for 21 workspace packages and 21 explicit members.`
`Doc path check passed for 120 current paths in 10 documents.`
`English contributor-text check passed for 777 files.`

### 9. GUI spot-check

Run on the interactive Windows desktop from this worktree
(`cargo build -p oneterm-app --profile fast-dev`, `Finished fast-dev profile ... in 4m 21s`).
`fast-dev` inherits `dev`, so `config_dir()` is the relative `target` — the app was launched
with its working directory set to **this worktree** and read only
`<worktree>\target\ssh_session.json`, which I seeded and deleted again afterwards. The
owner's real config was never read or written.

Safety (the owner runs Claude Code inside their own `oneterm.exe`): `Get-Process oneterm`
before launch → **2504, 14804** (a third, 16180, exited on its own before the launch and was
never touched). Started with `Start-Process -PassThru` through the pid-file driver
(`scratchpad/guiwt.ps1`, the implementer's worktree variant, used read-only); the new pid was
**7488**, and `(Get-Process -Id 7488).Path` was confirmed to be
`...\agent-a5df640ad16fb7826\target\fast-dev\oneterm.exe` before anything was posted to it.
Every click, key and screenshot addressed that pid's window handle only — never a name or
title. After the walk: `oneterm pids after: 2504,14804`, both owner processes alive, 7488
gone.

Seed: 50 sessions in storage order — id 1 `alpha`/`10.9.0.1`, id 2 `alpha`/`10.9.0.2` (same
label on purpose), id 3 label `"   "`/`10.9.0.3:2222`, id 4 `日本-nihon`, ids 5..50
`host-05`..`host-50`.

| Step | Result | Screenshot |
| --- | --- | --- |
| Open the "+" menu with 50 saved sessions | Shells → separator → New SSH Session → separator → "SSH Sessions" → the 50 rows, storage order. The whitespace label renders `10.9.0.3:2222` (the fallback) and `日本-nihon` renders correctly. **The popup runs off the bottom of a maximized 1936x1048 window at `host-31`: ~19 rows are cut off, with no scrollbar and nothing to scroll** — D1, confirmed. | `evidence/US-0094-verify-menu-50-sessions-clipped.png` |
| Click the fallback row `10.9.0.3:2222` (storage position 3, a different position from the panel's sorted list) | Dialog banner `ssh://root@10.9.0.3:2222` — right host **and** the non-default port. The id, not a position, is what routed it. Cosmetic aside: the dialog title is `Connect to    (root@10.9.0.3:2222)` — `connect_dialog` uses the raw label, so the fallback is menu-only (pre-existing, out of this packet's scope). | `evidence/US-0094-verify-dialog-blank-label-fallback-row.png` |
| Cancel, reopen the menu, click the **second** `alpha` | Dialog `Connect to alpha (root@10.9.0.2:22)` — the second duplicate, not the first. Duplicate labels route correctly; only the user cannot tell the rows apart (D2). | `evidence/US-0094-verify-dialog-second-duplicate-label.png` |
| With the app still running: right-click the first `alpha` in the SSH Sessions panel → Delete ("SSH session deleted."), then reopen the "+" menu | The menu now shows **one** `alpha`, the rest shifted up by one row. The list is rebuilt per open, at runtime, with no restart and no observer — the packet's freshness claim, proven rather than reasoned. | `evidence/US-0094-verify-menu-after-runtime-delete.png` |

Not attempted: a successful connect (no reachable host here), and macOS/Linux — the same gaps
the packet already declares.

### 10. Commit trailers

Both commits in `4dd57e7..39c9971` end with exactly:

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

(`aff3919` docs, `39c9971` code; both also carry `Refs: IN-0033, US-0094`.)

## Working-tree state

Nothing committed, nothing pushed. Uncommitted in this worktree:

- the two verification tests from §2 (`crates/session-ui/src/session_state.rs`,
  `crates/session-ui/src/tree_builder.rs`, +67 lines, tests only);
- this file and the four `US-0094-verify-*.png` screenshots from §9.

Both tampers were reverted and re-verified (57 passed / 0 failed). The seeded
`target\ssh_session.json` was deleted after the walk. The main checkout and the other
worktrees were never written to.
