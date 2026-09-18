# US-0129 — independent verification

Verifier: adversarial second party. Nothing here was written by the implementer.
Target: `feat/session-menu-duplicate-move-to-group` @ `2555df02` (2 commits on `main` @ `b64b70bc`).
Method: read the diff, the packet, `docs/ssh-client-connect.md` §1.1/§6.5/§6.6,
`docs/gui-layout.md` and `docs/agents/persistence.md`; read the kit's
`popup_menu.rs`, `tree.rs` and `context_menu.rs` for every claim made about kit
behaviour; ran the suite, wrote and ran a throw-away `ssh_session.json` round-trip
test, mutated `copy_label` and `SESSION_MENU_ROWS` to prove the new tests bite,
restored the tree to `2555df02` afterwards; inspected all five committed frames.

## Verdict

**PASS — with four findings, none of which falsifies an acceptance line.**

Every claim in the packet's "What was built" reproduces. The copy is a whole-struct
clone under a fresh id at `source_index + 1`; the label rule fills gaps and matches
exactly; `set_group` is a real no-op when nothing would change; the submenu is built
at right-click time from the same `existing_group_names` the dialog uses; the schema
is untouched at v2 and a duplicate survives a real file round trip byte-for-byte
apart from `label` and `id`; Delete is still danger-styled and still confirmed.
`cargo test -p oneterm-session-ui` is 76/0 and `pwsh scripts/ci-local.ps1` ends
"ci-local: all checks passed."

What is worth fixing is robustness, not correctness: the **Move to Group submenu can
never scroll** (F1). The kit only auto-enables scrolling on a builder path this code
does not use, the group count is user data with no upper bound, and the two other
OneTerm popups in the same situation both call `.scrollable(...)` by hand. This one
does not.

## Claims

Every claim below is the implementer's, restated; the verdict and citation are mine.

### C1 — the row is `Duplicate Saved Session`, distinct from the tab menu's `Duplicate` — HOLDS

`crates/session-ui/src/tree_render.rs:69` defines `DUPLICATE_ROW_LABEL =
"Duplicate Saved Session"`, pinned by `tree_render.rs:470-471`. The live-tab
`Duplicate` is `oneterm_core::SshDuplicateConfig`, reached from
`crates/terminal-view/src/panel/tab_title.rs`; nothing in the US-0129 diff touches
it and the two share no function. Frame `US-0129-01` shows the row reading
`Duplicate Saved Session`.

### C2 — `copy_label` → `(copy)`, `(copy 2)`, … filling gaps, exact match — HOLDS

`session_state.rs:519-529`. The loop tries `"<label> (copy)"`, then `"<label>
(copy 2)"`, `3`, … and stops at the **first** candidate no entry carries, so a gap
is filled rather than skipped. Comparison is `entry.session.label == candidate` —
exact, not prefix, so two sessions may legitimately share a label and only the
invented candidate must be free. Proved load-bearing by mutation M1 below.

`suffix` is `u32` and `+= 1` would panic on overflow in a debug build; that needs
~4×10⁹ colliding labels in one store. Not a finding.

### C3 — `duplicate` = `session.clone()` with the label replaced, at `source_index + 1`, id from `next_session_id`, never reused — HOLDS

- Whole-struct clone: `session_state.rs:544-545` (`entries[index].session.clone()`,
  then only `session.label` is reassigned). `SshSession`
  (`session_state.rs:135-171`) is `Clone + PartialEq + Eq` over `String`, `u16`,
  `Option<String>`, `Option<PathBuf>`, `SshSessionId` (a `u64` newtype) and
  `Vec<PortForward>`. `PortForward` (`crates/core/src/port_forward.rs:12-38`) is
  plain owned data — `IpAddr`, `String`, `u16`. **There is no `Rc`, `Arc`, `Cell`
  or interior mutability anywhere in the graph**, so the clone is a deep copy and
  the copy shares no state with its source.
  `jump_host` is deliberately an *id*, so the copy points at the same jump session
  rather than cloning it — correct, and it cannot introduce a cycle that
  `jump_chain` (`session_state.rs:298-321`) does not already reject at use.
- The compiler is the guard the packet claims: `session_state.rs:1387-1393` asserts
  `SshSession { label: source.label, ..copy } == source`, which fails the moment a
  field is added to `SshSession` and not copied.
- Placement: `entries.insert(index + 1, …)` at `session_state.rs:546-552`, asserted
  at `session_state.rs:1380-1385`.
- Id: `session_state.rs:360-368` takes `SshSessionId(self.next_id)` **before** the
  insert and advances `next_id` only when the insert happened, so a missing id
  neither writes nor burns a counter value.
- Never reused, including after a delete: `remove` (`session_state.rs:398-405`)
  only `retain`s and never lowers `next_id`; and even a hand-edited file cannot
  break this, because the loader repairs the counter to
  `recorded_next.max(max_id + 1).max(1)` (`session_state.rs:649`). The existing
  `verify_menu_row_id_survives_a_delete_of_an_earlier_session` covers the
  delete-then-add shape; delete-then-duplicate takes the identical path.

### C4 — `set_group` / `set_group_in` — HOLDS

`session_state.rs:374-384` and `559-572`. Trims, treats blank as ungrouped, and
returns `false` (so **no notify and no save**) both when the id is gone and when the
value would not change. "Move to the same group" is therefore genuinely a no-op
write — verified in source and pinned by
`set_group_moves_exactly_one_session` (`session_state.rs:1408-1436`).

### C5 — one store write per action — HOLDS

`duplicate` and `set_group` each call `cx.notify()` once and `self.save(cx)` once.
`save` (`session_state.rs:449-465`) enqueues one snapshot into the single-flight
`PersistQueue` and spawns one background drain, exactly as `docs/agents/persistence.md`
§Schema owners describes. No UI-thread filesystem call.

### C6 — submenu via `PopupMenuItem::submenu` over `PopupMenu::build`, read at right-click time — HOLDS

`tree_render.rs:87-133` builds the submenu; `tree_render.rs:397-400` attaches it.
The cited kit support is real and the citation is accurate:
`reference/gpui-kit/crates/component/src/menu/popup_menu.rs:1388-1405` wires
`parent_menu` and `priority` for submenus that arrive through the public
`item()` + `PopupMenuItem::submenu()` path, and its comment names exactly this case.

"Read at right-click time" is correct, not merely plausible: `Tree::context_menu`
stores the builder (`reference/gpui-kit/crates/component/src/tree.rs:55-62`) and the
underlying `ContextMenuExt` invokes it from a `MouseDownEvent` handler
(`.../menu/context_menu.rs:277-311`), not from `render`. So one submenu entity is
created per right-click and it cannot list a stale group.

Keyboard navigation into it works for the same reason: `PopupMenu::active_submenu`
(`popup_menu.rs:802-812`) matches the `PopupMenuItem::Submenu` variant, and `right`
is bound to `SelectRight` (`popup_menu.rs:27`, handler at `popup_menu.rs:947-…`,
listener at `popup_menu.rs:1436`). I could not drive a live keypress (see Gaps), but
the code path is the same one `PopupMenu::submenu`-built children take, because the
parent link is wired identically.

### C7 — rows: `No group` (checked when ungrouped) / existing groups with the current one checked / separator / `New group…` — HOLDS, with one edge (F2)

`tree_render.rs:99-133`. `existing_group_names` is now `pub(crate)`
(`session_dialog.rs:119-128`) and is the one source for both the combobox and the
submenu — trimmed, non-empty, sorted, deduped. So the submenu cannot create `Infra`
beside `infra`, as claimed. Frame `US-0129-02` shows `✓ No group`, `infra`, a
separator and `New group…` for the ungrouped `DevServer`.

The zero-group case degrades correctly: `No group` (checked), separator,
`New group…` — the loop simply contributes nothing.

### C8 — `New group…` opens Properties with the Group combobox focused — HOLDS

`tree_render.rs:121-129` calls `open_session_dialog(…, true)`; `session_dialog.rs:318`
takes `focus_group: bool`; `session_dialog.rs:657-663` defers the focus through
`FormDialog::on_render` (`crates/state/src/form_dialog.rs:220-240`, which runs the
hook on every rebuild) and `common::defer_initial_focus_once`
(`crates/session-ui/src/common.rs:46-59`, a `Cell<bool>` take-once plus
`window.defer`). Every other caller passes `false`, so the `Cell` starts `false` and
the helper returns immediately — nothing else changed behaviour
(`lib.rs:54`, `panel.rs:214`, `panel.rs:261`, `panel.rs:311`, `tree_render.rs:364`,
`tree_render.rs:388`).

Frame `US-0129-05` shows the focus ring on the Group combobox and on no other field,
with the Label field unringed — which frame `US-0129-03` (a plain Duplicate, so
`focus_group = false`) does not have. The two frames together are a real control
pair, not a single unfalsifiable shot.

### C9 — no schema change; `add()` and `rename_group()` reused in shape — HOLDS

`CURRENT_SCHEMA_VERSION = 2` (`session_state.rs:48`) is untouched; `save_snapshot`
(`session_state.rs:467-489`) writes the same three keys. `duplicate` is `add`'s id
handling (`session_state.rs:327-333`) with an `insert` instead of a `push`;
`set_group_in` is `rename_group_in`'s field write narrowed to one id. I proved the
document shape independently — see Round trip below. `docs/agents/persistence.md`
needed no change and the packet's stated reason still holds.

### C10 — `SESSION_MENU_ROWS` = [Open, Properties, Duplicate, MoveToGroup, Separator, NewSession, Separator, Delete] — HOLDS

`tree_render.rs:51-60`, rendered in order at `tree_render.rs:336-418`. Frame
`US-0129-01` shows exactly that, with `Delete` in the danger colour. Delete is still
`PopupMenuItem::element` painting `cx.theme().danger` and still routed through
`confirm_delete_session` (`tree_render.rs:405-415`; the confirmation dialog at
`panel.rs:55-90` is untouched by this diff, with `ok_variant(ButtonVariant::Danger)`
and `show_cancel(true)`).

## Findings

Ranked. None blocks acceptance.

### F1 — the Move to Group submenu never scrolls, and the group count is unbounded — moderate

`PopupMenu::scrollable` defaults to `false`
(`reference/gpui-kit/crates/component/src/menu/popup_menu.rs:338`) and the kit turns
it on in exactly one place: `with_menu_items`, when a menu exceeds 20 items
(`popup_menu.rs:794-797`). `PopupMenu::item()` (`popup_menu.rs:702-706`) never sets
it — and `move_to_group_submenu` (`tree_render.rs:99-133`) builds every row with
`.item()`. `max_height` is likewise only consulted `when(self.scrollable, …)`
(`popup_menu.rs:1452-1456`), so an unscrollable menu has **no height cap at all**.

The submenu's row count is `groups + 3`, and nothing bounds the number of groups a
user can create. At ~30 groups the popup is taller than the 918px window the
evidence frames were taken in. `anchored().snap_to_window_with_margin`
(`popup_menu.rs:1351-1362`) can translate a popup to fit, but it cannot shrink one,
so the overflowing rows — including `New group…`, which is *last* — become
unreachable with no scrollbar and no affordance.

**This is not a kit quirk nobody knew about — it is OneTerm's own established
pattern, and this is the one place that skipped it.** Both other popups in the app
whose row count is user data set it by hand, with the same reasoning and nearly the
same comment:

- the centre `+` menu — `crates/terminal-view/src/panel/terminal_panel.rs:759-765`,
  `menu.scrollable(px((FIXED_ROWS + session_rows) as f32 * ROW_HEIGHT) > cap)` with
  `cap = min(window_height * 0.5, 450px)`; documented in `docs/gui-layout.md`;
- the SFTP overflow menu — `crates/sftp-ui/src/render.rs:288-298`, the same estimate
  against the same cap.

The kit's note that submenus "only work when the parent menu is not `scrollable`"
(`popup_menu.rs:57-59`) constrains the **parent**, not this submenu, which has no
children of its own — so the fix is the same one call on the built submenu, and the
two existing sites give the row-height estimate to copy.

Not reproduced in the GUI (see Gaps); the code path is unambiguous.

### F2 — the submenu's checked state compares an untrimmed stored group against trimmed row labels — minor

`tree_render.rs:92-96` takes `current` as the **raw** `session.group.clone()`, while
the rows come from `existing_group_names`, which trims
(`session_dialog.rs:121-124`). A session stored with `"group": " infra "` therefore
renders `infra` **unchecked** and `No group` unchecked too, so the submenu shows no
check at all and misreports where the session lives. Clicking `infra` then writes
the trimmed name, which `set_group_in` sees as a real change, so it is not even the
no-op it looks like.

Only reachable from a hand-edited `ssh_session.json` — the dialog trims on save
(`non_empty`, `session_dialog.rs:110-114`) and `rename_group_in` trims too. Fix is
one `.map(str::trim)` on `current` at `tree_render.rs:92-96`, which also makes the
check consistent with the write.

### F3 — Duplicate persists the copy before the dialog opens, and Cancel does not undo it — minor, undocumented

`tree_render.rs:378-395` calls `store.duplicate(...)` — which notifies **and saves**
— and only then opens Properties on the copy. Pressing **Cancel** in that dialog
leaves `X (copy)` in `ssh_session.json`. This is a defensible design (it is what
makes the row "Duplicate" rather than "New from…"), and §6.5 does say the copy is
written first, but neither the packet nor the design says Cancel keeps it, and the
dialog's own Cancel button means "discard" everywhere else in the app. One sentence
in `docs/ssh-client-connect.md` §6.5 closes it.

### F4 — one evidence bullet claims something its frame does not show — minor, documentation

The packet's Evidence frames section says
`evidence/US-0129-04-moved-into-group.png` shows "`target/ssh_session.json` … with
`schema_version` still 2 and no new field". The frame is a window capture of the
tree only; no file contents are visible in it. The underlying claim is true — I
verified it independently below — but it is attributed to an artifact that does not
carry it, which is the kind of line a later reader would take on trust.

### F5 — where the menu-order coverage actually lives — note, not a defect

`the_session_menu_leads_with_the_session_and_ends_with_delete`
(`tree_render.rs:431-443`) filters separators out before comparing, so it cannot see
a separator move. Mutation M2 below (moving the separator above `Duplicate`)
**passed** that test and was caught only by
`duplicate_and_move_to_group_join_the_session_block` (`tree_render.rs:450-472`).
The order *is* pinned; it is pinned by one assertion, not two.

## Recommendation on `prod (copy) (copy)`

**Keep the rule as it is. Do not strip a trailing ` (copy N)` first.**

1. A label is free text. `prod (copy)` may be a name the user chose and kept; a
   stripping rule would silently treat it as a derived name and produce
   `prod (copy 2)` — which reads as *a second copy of `prod`*, not as a copy of
   `prod (copy)`. That is a wrong statement about provenance, and the user cannot
   tell it happened.
2. Stripping needs a parser for a user-facing string (`" (copy)"`, `" (copy 12)"`,
   `" (copy)  "`, `" (Copy)"`, a non-ASCII digit…), and every parser of user text
   grows cases. The current rule is four lines and has none.
3. The cost of the long label is paid immediately and visibly: the Properties dialog
   opens on the copy with the Label field right at the top, which is precisely where
   a user who dislikes `prod (copy) (copy)` fixes it.

The packet already records this as a deliberate choice with the owner's agreement;
it is the right one and needs no change.

## Commands

All from the worktree, `CARGO_BUILD_JOBS=4`.

| Command | Result |
|---|---|
| `cargo test -p oneterm-session-ui` | **76 passed, 0 failed** — matches the packet exactly |
| `cargo test -p oneterm-session-ui verify_duplicate_round_trips` (throw-away, then reverted) | 1 passed — see Round trip |
| `cargo test -p oneterm-session-ui copy_label` under mutation **M1** | **FAILED** at `session_state.rs:1348` — `left: "prod (copy 3)"`, `right: "prod (copy 2)"` |
| `cargo test -p oneterm-session-ui tree_render` under mutation **M2** | **1 FAILED** at `tree_render.rs:459` — `left: [Open, Properties]`, `right: [Open, Properties, Duplicate, MoveToGroup]` (the other two passed — see F5) |
| `pwsh scripts/ci-local.ps1` | ends `ci-local: all checks passed.` |

Every mutation and the throw-away test were reverted; `git status` showed a clean
tree at `2555df02` before this file was added, and nothing under `crates/` differs
from the commit under review.

### Round trip through `ssh_session.json` (mine, not the implementer's)

A throw-away test built a source with a non-default value in **every** field
(`port`, `username`, `auth_method = PrivateKey`, `key_path`, `color`, `group`,
`logging = On`, `jump_host`, a `PortForward::Dynamic`, `agent_forwarding = true`),
ran `duplicate_in`, wrote the document through the real `SshSessionStore::save_snapshot`
to a temporary directory and read it back through the real `load_from`:

- raw JSON `schema_version == 2`, `next_session_id == 6`;
- the copy's JSON **key set is identical to the source's** — the duplicate
  introduces no field and drops none (`serde` `skip_serializing_if` behaves the same
  on both, which is the sharpest available check that the copy is field-for-field);
- ids reload as `[3, 5, 4]` — the copy is still directly after its source after a
  real serialise/parse cycle, so "right after the source" is a property of the file,
  not only of the in-memory `Vec`;
- `needs_resave == false` — a document containing a duplicate needs no migration;
- `SshSession { label: source.label, ..copy } == source` after the round trip.

### Mutations (both reverted; the tree is byte-identical to `2555df02`)

- **M1** — `copy_label` rewritten to count existing `"<label> (copy…"` labels
  instead of searching for the lowest free candidate, i.e. gap-filling removed.
  Caught by the gap assertion.
- **M2** — `SESSION_MENU_ROWS` separator moved from after `MoveToGroup` to after
  `Properties`. Caught, by one of the three tests.

### Frames

All five committed frames are 1600×918 PNGs and were inspected, not merely listed.

- `US-0129-01-session-context-menu.png` — Open / Properties / Duplicate Saved
  Session / Move to Group ▸ (chevron present) / separator / New Session
  `Ctrl+Shift+N` / separator / Delete in red. Matches `SESSION_MENU_ROWS` row for row.
- `US-0129-02-move-to-group-submenu.png` — submenu open beside its row with
  `✓ No group`, `infra`, separator, `New group…`. Opens leftwards because the parent
  menu is against the right dock edge; that is `snap_to_window_with_margin`, not a defect.
- `US-0129-03-duplicate-and-properties.png` — tree shows `Staging` then
  `Staging (copy)`; **Edit SSH Session** open on the copy with host `10.0.0.12`,
  port `2222`, username `deploy`, Password auth, and no focus ring on Group.
- `US-0129-04-moved-into-group.png` — `DevServer` now inside `infra` and gone from
  the root. See F4 for what this frame does *not* show.
- `US-0129-05-new-group-focuses-the-field.png` — same dialog on `Staging`, focus
  ring on the **Group** combobox only.

## Gaps in this verification

- **No GUI walk of my own.** No driver for this app exists in `scripts/`, and the
  owner runs Claude inside OneTerm, so process handling here is deliberately
  conservative. F1 (30 groups) and the Right-arrow keyboard path are therefore
  argued from kit source with exact citations, not from a frame. F1 in particular
  would be settled by one screenshot of a 30-group submenu.
- **`(copy 2)` in the GUI** remains unwalked, as the packet already records.
- **The deferred focus** is proved by frames 03/05 as a control pair, not by an
  assertion — unchanged from the packet's own gap, and I found no way to assert it
  from a unit test either.
