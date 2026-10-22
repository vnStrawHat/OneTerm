# Advisor code review (US-0201)

Date: 2026-10-22. Reviewed head: `809106f8` (two commits on `main` @ `52f7aa49`): `237d872b`
(the packet), `809106f8` (the code and the four doc edits). Verify branch `verify/us-0201` in the
coder's worktree; this file and the two pinning tests are the only additions.

## Verdict

**FAIL.** One Medium defect (finding 1): the rows the menu shows as disabled still run on
hover + Enter, so a dead terminal's greyed "Split Right" splits the tab and the greyed
"Duplicate Session" dispatches `DuplicateSession`. Reproduced on a window with the product's own
row and pinned by two `should_panic` tests. The fix is three lines in `menu.rs`; the wiring, the
liveness read, the placeholder menu, the menu-only decision and the records are all sound.

## Gate, re-run one crate per command

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-terminal-view` | 430 passed, 3 ignored (428 before the two pins) |
| `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `python scripts/check-english.py` | 1150 files pass |
| `python scripts/check-doc-paths.py` | 219 paths pass |

`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, the worktree's own `target`.

## The brief's attack list

1. **Is `alive()` read at right-click time, or could the menu be cached stale?** Read at
   right-click time. The kit rebuilds the menu on every open: `open_menu`
   (`reference/gpui-kit/crates/component/src/menu/context_menu.rs:416-476`) clears
   `menu_view`, then `PopupMenu::build` in a deferred callback calls the builder closure
   again. The closure (`crates/terminal-view/src/terminal_view/render.rs:362-365`) reads
   `session.read(cx).alive()` on each call. A session that dies while the menu is open keeps
   the live rows until the menu is reopened, which is the same window every other row has
   (Copy's selection, Log's running flag).

2. **Are the rows truly disabled in the kit?** Mouse: yes. `MenuItemElement::render`
   (`reference/gpui-kit/crates/component/src/menu/menu_item.rs:119-133`) only attaches
   `on_click` when `!disabled`, and `PopupMenu::render_item` (`popup_menu.rs:1326-1331`)
   does the same. Keyboard: **no**, see finding 1. `on_hover` is attached whether or not the
   row is disabled (`menu_item.rs:116-118`), it sets `selected_index = Some(ix)`
   (`popup_menu.rs:1243-1254`), and `confirm` (`popup_menu.rs:861-892`, the Enter binding)
   runs the item's handler or dispatches its action with no `disabled` check. Arrow keys are
   safe (`select_up` / `select_down` filter on `is_clickable()`, which excludes disabled
   rows, `popup_menu.rs:228-242`), with one kit oddity: `select_down` from no selection
   picks index 0 unconditionally (`:943-946`); index 0 is "New Terminal" in both of our
   menus, so no exposure here. A disabled `Submenu` would be safe on Enter (`confirm` has
   `_ => {}`), which is why the dead Duplicate Session row, now a plain `Item`, is the one
   that became reachable. The pinned crates.io `gpui-component-0.7.3` matches the reference
   on all of these lines (diffed).

3. **Placeholder menu.** Unaffected: `crates/terminal-view/src/space/render.rs:314` passes
   `false`, and the placeholder has no session to read. The kit still renders those rows
   enabled.

4. **Menu-only decision.** Sound and consistently recorded (packet Plan and Evidence,
   `docs/terminal-split/04-context-menu.md:59-65`, `IN-0018` LLD `input.md:190-192`,
   `docs/gui-layout.md:180`). The reasoning holds: `duplicate_session_to` is the tab menu's
   "Duplicate" reconnect path for a dropped SSH tab, and `split_active_at` is shared with the
   placeholder (an empty Space has no view). Nothing in the four docs contradicts the code.
   The two older records that describe Duplicate Session as unconditional
   (`IN-0003`, `IN-0006` HLD) are history and need no edit.

5. **Owning intake.** IN-0022 is acceptable: the rule is the liveness model US-0199 / US-0200
   built, and the row shape is US-0199's. The natural alternative (IN-0003, the Duplicate
   Session contract, or the terminal-split docs) would have split the liveness rule across
   two intakes; the owning docs for the menu layout were updated regardless.

6. **The untested `build_menu` branch.** Reviewed by hand: `menu.rs:106-145` picks
   `dead_duplicate_item()` when `!ctx.session_alive` and passes `!ctx.session_alive` to
   `split_items`; nothing else in the builder reads the flag. The coder's two tests pin the
   helpers' output. For a wiring test, see finding 4.

## Findings

1. **Medium, defect. A disabled Split row and the disabled Duplicate Session row still act
   on hover + Enter.** `crates/terminal-view/src/input/menu.rs:318-328` (`split_item`
   attaches `.on_click(...)` and the `Split*` action regardless of `disabled`) and
   `menu.rs:245-250` (`dead_duplicate_item` keeps the `DuplicateSession` action on purpose,
   "so the shortcut hint column does not change"). Kit side: `menu_item.rs:116-118` (hover is
   registered on a disabled row), `popup_menu.rs:861-892` (`confirm` runs the handler or
   dispatches the action without checking `disabled`). User path: right-click a dead
   terminal, move the pointer onto the greyed "Split Right", press Enter: the tab splits.
   On "Duplicate Session": `dispatch_confirm_action` focuses the action context and
   dispatches `DuplicateSession`, which `TerminalPanel` handles on the active Space, i.e. the
   dead one. Reproduced: `input::menu_tests::defect_1_hover_then_enter_splits_through_a_disabled_split_row`
   (a window hosting the product's `split_item(.., disabled = true)` for a `TerminalPanel`
   whose fake session is dead; right-click, hover the row, Enter; `leaf_count()` goes 1 to 2:
   "hover + Enter left 2 Spaces"). Pinned with `should_panic` together with the row-model
   test `defect_1_disabled_rows_still_carry_their_click_path` (a disabled `split_item` or
   `dead_duplicate_item` carries a handler or an action).
   **Fix shape (product, three lines):** in `split_item` attach the action and the
   `on_click` only when `!disabled` (an `if disabled { item } else { item.action(..).on_click(..) }`
   around the builder chain); in `dead_duplicate_item` drop `.action(..)`. A row with
   neither has nothing for `confirm` to run, whatever `selected_index` says. The shortcut
   hint disappears from the greyed rows, which is what a disabled row should show anyway;
   the column is laid out per row, so nothing else moves. Then drop the two `should_panic`
   attributes and the `defect_1_` prefixes (the second test flips green; the first proves
   the window path). Update the rustdoc at `menu.rs:242-244` and the packet Plan bullet that
   gives the hint-column rationale. Kit side, worth an upstream note but not a blocker:
   `confirm` should honour `disabled`, `MenuItemElement` should not register `on_hover` on a
   disabled row, and `select_down` from no selection should pick the first clickable row.

2. **Low, pre-existing, info only.** The same kit hole reaches every other disabled row that
   keeps a handler: Copy without a selection (`menu.rs:158`, a no-op copy), Log Start while
   running / Stop while not (`menu.rs:188,197`, the controller's own guards apply), and
   `panel/tab_title.rs:793`. The US-0199 "Input Channel" row (`channel_menu.rs:147`) and
   "No saved sessions" (`terminal_panel.rs:792`) carry nothing and are safe. Not this
   packet; a follow-up BUG if the owner wants the rule "a disabled row carries no click
   path" applied everywhere, or the upstream fix.

3. **Low, record.** `US-0201-disable-split-and-duplicate-on-dead-terminal.md` Plan ("the
   action stays so the shortcut hint column does not change") and `menu.rs:242-244` state
   the rationale finding 1 overturns; rewrite both with the fix. The IN-0022 packet row is
   already ticked `[x]` and the packet reads "Implemented" before merge; the coordinator's
   merge step should confirm that with `harness story` after the rework.

4. **Low, test gap, with a shape.** The `build_menu` wiring is not unit-tested because
   `PopupMenu::menu_items` is `pub(crate)` in the kit. Two routes exist, neither taken here:
   (a) the kit's headless observation API, `gpui_base::test_support::snapshots(window)`
   (`reference/gpui-kit/crates/base/src/test_support.rs:174-189`), lists every drawn
   `MenuItemElement` with its label and bounds; it is behind the `test-support` feature of
   `gpui-base`, which this crate's dev-dependencies do not enable (a one-line Cargo.toml
   change). With it, a window test can right-click a host that renders `build_menu` with a
   dead `FakeTerminalSession`, assert the labels (a "Duplicate Session" row and the four
   Splits present), and click each by bounds asserting `leaf_count()` stays 1 and no
   `DuplicateSession` reaches an `on_action` listener on the host. Note `MenuItemElement`
   publishes no accessibility disabled flag, so `ElementSnapshot::disabled()` is `None`
   there; the test must be behavioural, which only becomes meaningful after finding 1 is
   fixed. (b) A pure "row model" function is not worth it: the branch is two reads of one
   flag. Recommendation: (a), in the rework or as a follow-up; the ui-verifier walk covers
   the shape meanwhile.

## What the owner must still check by hand

- On screen (the ui-verifier walk): a dead tab's area menu shows the four Splits and
  "Duplicate Session" greyed, Duplicate Session without a submenu arrow, every other row as
  before; a live tab's menu unchanged; the empty-Space menu's Splits still enabled.
- Finding 1 by hand, before and after the fix: hover the greyed "Split Right" on a dead
  tab and press Enter.
- A real SSH drop (not only a local exit): the area menu greys the rows after the
  "SSH connection closed" banner; only the fake session was used here.

---

# US-0201 UI verification (ui-verifier)

## UI verification

Verifier: ui-verifier, 2026-10-22. Binary copy of head 809106f8 (debug), private profile (scratch USERPROFILE and cwd), only driver-spawned pids stopped. Screenshots: `C:\Users\trunglt\AppData\Local\Temp\claude\D--TrungKFC-Research-Rust-myTerm2\fa2d0135-9c28-4a69-a255-ba7479e604cb\scratchpad\uiv201-shots` prefix `uiv201-` (full-window shots including the ~8 px frame; driver clicks are client coordinates, x = shot x - 8). Desktop unlocked (shots not black).

Environment limit: every child shell dies at start in this sandbox, so every terminal is in the ended state.

| Item | Result | Shot |
|---|---|---|
| 1 Ended terminal menu: Duplicate Session and Split Right/Left/Up/Down greyed, no submenu arrow on Duplicate; clicking Split Right and Duplicate does nothing (menu stays open, no split) | PASS | 02-menu-dark, 03-click-splitright, 04-click-dup |
| 1 Keyboard: Down skips disabled rows (New Terminal, then Paste, then Select All), so Enter cannot reach a disabled row | PASS | 05-down2, 06-down3 |
| 1 Other rows unchanged: New Terminal, Copy (greyed, no selection), Paste, Select All, Clear, Log submenu, Close Terminal Tab; no Close Space with one Space | PASS | 02-menu-dark |
| 1 Close Space shown (Ctrl+Shift+X) when the tab has 2 Spaces | PASS | 09-dead-in-2space-menu |
| 2 Empty Space menu: New Terminal Here, four Splits enabled, Close Terminal Tab, Close Space | PASS | 08-empty-space-menu |
| 3 Live shell: Split enabled, Duplicate submenu | NOT WALKED (no shell survives) | |
| 4 Tab menu "Duplicate" on dead tab enabled (Input Channel disabled, as US-0200) | PASS | 10-tab-menu |
| 5 Dark (Zed One Dark) and light (Zed One Light): disabled rows dimmed like the tab menu's disabled Input Channel / Close Others | PASS | 02-menu-dark, 11-menu-light, 12-tabmenu-light |

### Defects

None against the acceptance list.

Observations (Info): (a) the key binding Ctrl+Shift+Right still splits a dead terminal (01-start to 07-after-split-chord); the packet scope is the menu, confirm that is intended. (b) Disabled-row text in the light theme is fairly low contrast (same token as the tab menu, so consistent).

### Owner-only checks

- Live shell: splits enabled and Duplicate Session submenu (In New Tab, Into Space #N, Split Right/Down).
- A shell that exits while the menu is open (row state at open time vs. live).
- SSH session drop (Closed without Exited) shows the same disabled rows.

---

# Advisor re-review (rework 1: no click path on a disabled row, window test)

Date: 2026-10-22. Reviewed head: `fbc8a1ea` (two commits on my `962e1979`): `4838fea5` (the
ui-verifier section above, cherry-picked) and `fbc8a1ea` (the fix, the rule applied to the other
disabled rows, the window tests, the `test-support` dev-dependency, `Cargo.lock`,
`THIRD-PARTY-NOTICES.md`, the records). Same worktree, `verify/us-0201`.

## Verdict

**PASS.** Finding 1 is fixed at the root and the fix is proven on a window with a positive
control; finding 2 is applied to every other disabled row that carried a handler; the records
match the code. The dev-only `test-support` feature is acceptable (below): the three new
`Cargo.lock` rows are never compiled, not even for the tests. Keep the window test. One Low
note (R1) on the gate, one Info (R2) on the notices list.

## Gate, re-run one crate per command

| Command | Result |
| --- | --- |
| `cargo clean -p oneterm-terminal-view` then `cargo test -p oneterm-terminal-view` | 431 passed, 3 ignored (the first run against the shared `target` reported the two new tests failing; a clean rebuild of the crate passes, see R1) |
| `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `python scripts/verify-dependency-graph.py` | 20 packages pass |
| `cargo package -p oneterm-vt --allow-dirty --list \| python scripts/verify-dependency-graph.py --package-list -` | package set passes, reaches nothing outside `crates/vt` |
| `python scripts/third-party-notices.py --check` | up to date |
| `python scripts/check-ignored-tests.py` | 19 ignored tests, all recorded |
| `python scripts/check-english.py`, `check-doc-paths.py` | 1151 files / 219 paths pass |
| `cargo deny check licenses bans advisories` (0.20.2) | licenses ok, bans ok (`multiple-versions = "warn"` covers `taffy` 0.13 + 0.14); advisories FAIL on `RUSTSEC-2026-0382` (certificate verification, `rustls-webpki` under `reqwest` / `tokio-rustls`), which fails identically on `main` @ `52f7aa49` and is unrelated to this packet |

`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, the worktree's own `target`.

## The dependency question

The deviation: `crates/terminal-view/Cargo.toml` dev-dependency
`gpui-component = { workspace = true, features = ["test-support"] }`, which puts `gpui-fast
0.1.10`, `gpui-fast-macros 0.1.10` and `taffy 0.14.0` into `Cargo.lock` (+3 in the notices).

- **What the feature is.** `gpui-component-0.7.3/Cargo.toml`: `test-support = ["gpui/test-support",
  "gpui-base/test-support", "gpui-fast?/test-support"]`; `gpui-base-0.7.3`: `test-support =
  ["gpui/test-support", "gpui-fast?/test-support"]`. `gpui-fast` is an optional dependency of
  both (their `gpui-fast` feature, which nothing here enables), and the `?` is a weak feature
  reference: it enables `gpui-fast/test-support` only if `gpui-fast` is already on.
- **Why it is in the lockfile anyway.** Cargo records an optional dependency named by a weak
  feature in `Cargo.lock` even when it is not activated (rust-lang/cargo issue 10801); the
  dependency lists of `gpui-base` and `gpui-component` in the lock gained `gpui-fast` for that
  reason, and `taffy 0.14` is `gpui-fast`'s own layout crate.
- **Is it compiled?** No. `cargo tree --workspace -e normal -i gpui-fast` and `-i taffy@0.14.0`
  print nothing, and so does `cargo tree -p oneterm-terminal-view -e normal,dev,build -i
  gpui-fast`: the crate is not in the test graph either. Release builds, `oneterm-app`, and
  the `oneterm-vt` package (no gpui at all; the package-list check passes) are untouched.
- **Licences.** `gpui-fast` / `gpui-fast-macros` Apache-2.0 (longbridge/gpui-fast), `taffy`
  MIT; both already in `deny.toml`'s allow list (`licenses ok`).
- **Policy.** `docs/agents/dependencies.md` §"Adding a dependency" is about a new workspace
  declaration; this adds none (a feature on a dependency the workspace already declares) and
  the lock growth is a resolver artefact, so no table row, no issue. The notices file is
  regenerated as line 124 asks. `crate-dependency-rules.md` R1-R12 are unaffected
  (`verify-dependency-graph.py` passes).
- **Recommendation: keep.** The window test is the only automated proof that the rows the
  menu greys do nothing on the kit's Enter path, and its positive control shows the harness
  reaches `confirm`. The cost is three never-built rows in the lock and the notices. If the
  owner prefers a clean lock, the alternative is to drop the window test and keep only the
  row-model test (`disabled_rows_carry_no_click_path`), which pins the fix but not the kit
  path; not recommended.

## The attack list

1. **Does the Enter path of the window test exercise `confirm`?** Yes. The control
   `a_live_terminals_split_row_splits_on_hover_and_enter` drives the same hover +
   `simulate_keystrokes("enter")` on a live "Split Right" and `leaf_count()` goes 1 to 2; the
   only route from Enter to `split_active_at` is `PopupMenu::confirm` (`popup_menu.rs:861-892`,
   the `enter` binding at `:23`) running the row's handler. The dead test then drives the same
   path plus a `simulate_click` on each of the five rows and asserts one Space and zero
   `DuplicateSession` dispatches at the host's `on_action`. Mutation reproduced: with
   `.when(true, ..)` in `split_item` (the click path back on a disabled row) exactly
   `disabled_rows_carry_no_click_path` and
   `a_dead_terminals_menu_greys_split_and_duplicate_and_they_do_nothing` fail ("Split Right
   carries a click path while disabled"; "the disabled \"Split Right\" row split the tab"),
   the other five menu tests pass; restored, all pass.

2. **Do the other disabled rows lose only their hint?** Yes. Copy: `edit_item(.., disabled)`
   (`menu.rs:340-357`) drops the `TerminalCopy` action and the handler only when
   `!has_selection`; the live row is unchanged and the key binding itself lives on the
   terminal's key context, not on the menu row. Log Start / Stop (`menu.rs:185-209`) carried
   `NoAction` (no hint to lose). `close_scope_row` (`panel/tab_title.rs:789-800`) never had an
   action. Paste / Select All / Clear pass `false`.

3. **Any other disabled row in `crates/terminal-view` with a click path?** None. Sweep of every
   `.disabled(` in non-test sources: `menu.rs:188,199,251,324,351` (all `.when(!d, ..)` or no
   path), `channel_menu.rs:147` ("Input Channel", no path), `tab_title.rs:794` (fixed),
   `terminal_panel.rs:82` (the "SSH Sessions" heading, an element item with no handler) and
   `:792` ("No saved sessions", no path).

4. **Records.** `04-context-menu.md:59-67`, IN-0018 LLD `input.md:190-196`, the packet
   (Scope, Acceptance, Plan rule, Evidence) and the rustdoc on `split_item` /
   `dead_duplicate_item` all state the one rule and match the code. The packet's status block
   is "Reopened (acceptance rework)" with "Implemented" ticked; consistent with a rework
   awaiting merge.

## Findings

- **R1, Low, process.** The first `cargo test -p oneterm-terminal-view` in this worktree after
  the coder's commits reported `disabled_rows_carry_no_click_path` and the dead-terminal
  window test failing with the pre-fix messages, while the sources on disk were the fixed
  ones; `touch menu.rs` (or `cargo clean -p oneterm-terminal-view`) and a rebuild passed, and
  the mutation check proved the binary now tracks the source. A stale fingerprint in the
  shared `target` (the worktree-clobber note in the project memory), not a product defect;
  recorded so the next reviewer who sees the pre-fix message rebuilds before filing it.
- **R2, Info, notices.** `THIRD-PARTY-NOTICES.md` now lists `gpui-fast`, `gpui-fast-macros` and
  `taffy 0.14.0` although nothing links them: `scripts/third-party-notices.py` reads the
  `cargo metadata` resolve, which carries the weak-feature rows the lock carries, while
  `cargo tree` prunes them. Over-inclusive, harmless; a note in the script or a prune of
  packages `cargo tree -e normal,build` does not reach would remove the three rows. Not
  this packet.

## Still by hand

Unchanged: a live shell's menu (splits enabled, Duplicate Session submenu) and a real SSH drop;
the ui-verifier could not walk either here.
