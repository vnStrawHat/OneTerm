# Work: App-level defaults leave the single-Ctrl keys terminals own

ID: US-0123
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (shipped defaults — user-visible on upgrade)
- Risk lane: normal, with two gates: `DEC-0018` accepted **and** explicit owner acceptance of
  the new defaults before merge
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

`Ctrl-S`, `Ctrl-Q`, `Ctrl-G` and `Ctrl-Space` reach the program running in the terminal. The
application's own defaults move to chords a terminal does not own, users who rebound something
keep their choice, and a new default that collides with a surviving override never silently
double-binds a key.

## Findings and proposals covered

`P18` (effort M) — *"Rebase the app-level defaults off single-Ctrl keys that terminals own:
`ctrl-shift-s`/`ctrl-shift-n` for New SSH Session, `ctrl-shift-q` for Quit, `f1` or
`ctrl-shift-/` for About, and drop `ctrl-g` or move it to `ctrl-shift-g`. Ship a migration
that only rewrites users still on the old defaults."* Its doc column reads: *"a new
`docs/decisions/DEC-00xx` — this is a shipped-default change"*.

Addresses `F31` (medium), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F31 | default key bindings (code-read; chords not exercisable on a locked desktop) |
> App-level single-Ctrl bindings sit on terminal control characters: `ctrl-s` New SSH Session
> (XOFF), `ctrl-q` Quit (XON), `ctrl-g` Toggle Gutter (BEL), `ctrl-space` About (set-mark / IME
> toggle). `crates/settings-ui/src/key_bindings/key_bindings_actions.rs:80,89,98,107`. |
> Keyboard reach: in a terminal these keystrokes belong to the remote program. `ctrl-space` for
> *About* is also an odd use of a prime key. | medium | 27, 38 |

## Scope

- [ ] In scope:
  - `crates/settings-ui/src/key_bindings/key_bindings_actions.rs:80,89,98,107` — the four
    `default:` values, per `DEC-0018`.
  - `crates/settings-ui/src/key_bindings/state.rs` — `apply_key_bindings` gains the collision
    rule `DEC-0018` specifies: a surviving user override wins, and the action whose new default
    collided with it is left unbound and logged once.
  - A focused test over the default table and the migration rule.
  - The release note text listing the moved defaults.
- [ ] Out of scope:
  - `ctrl-w` (Close Panel) and `ctrl-t` (New Terminal Tab). `DEC-0018` keeps them by explicit
    exception and records them as its known soft spot. Changing them here would exceed the
    decision this packet implements.
  - The rebind UI, the capture flow, the conflict message (CORR-55) and the interceptor
    (CORR-56) — all working, all untouched.
  - `ui_config.json`'s schema. The `key_bindings` map's shape, key set and semantics are
    unchanged.
  - Which actions are rebindable at all.
  - The key-binding row's rendering (`US-0121`).

## Acceptance

- [ ] The four defaults in `key_bindings_actions.rs` are exactly what `DEC-0018` records, and
      no fifth default changed.
- [ ] A profile with **no** `key_bindings` entry for a moved action resolves to the new
      keystroke after upgrade, with no migration code run and no file rewritten.
- [ ] A profile with an entry that differs from the old default keeps that entry untouched.
- [ ] A profile whose surviving override holds a keystroke that is now another action's
      default: the override wins, the displaced action is unbound, and exactly one `warn` line
      names both action ids. The Key Bindings page shows the displaced action as unbound.
- [ ] No two shipped defaults hold the same keystroke in the same key context. Asserted over
      the whole table, not just the four changed rows.
- [ ] Resetting a moved action from the Key Bindings page restores the **new** default.
- [ ] The release notes for the version carrying this packet list the four moved defaults.
- [ ] `DEC-0018` is Accepted and the owner has accepted the new defaults, both recorded here
      before merge.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/decisions/DEC-0018-app-shortcuts-leave-single-ctrl-keys-to-the-terminal.md` — the
  decision this packet implements: the rule, the four new defaults, the migration rule, the
  collision rule and the `ctrl-w`/`ctrl-t` exception. **Update required:** its Status moves from
  Proposed to Accepted when the owner accepts, and its Consequences checkboxes are reconciled
  after implementation.
- `crates/settings-ui/src/key_bindings/mod.rs` module doc — describes where overrides live
  (`ui_config.json`'s `key_bindings` map), the snapshot-and-reapply strategy, and the
  conflict rules CORR-55/CORR-56. **Update required:** the collision-at-apply rule is new
  behaviour in `apply_key_bindings` and belongs in this doc, which is the only place the
  strategy is written down.
- `crates/settings/src/ui_config.rs:45-49` — *"Per-action key-binding overrides: action id →
  keystroke string… Missing entries fall back to the built-in default."* This sentence is why
  no migration code is needed. **No change** — but quote it in Evidence, because it is the
  load-bearing fact.
- `docs/gui-layout.md` — read for whether it lists any default keystroke; the §Panel
  registration paragraph mentions `Ctrl-T` for `AddPanel`, which this packet does not move.
  **No change expected;** confirm.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `DEC-0018` (status and consequences) and the `key_bindings/mod.rs` module doc
(the collision rule). Plus the release notes.

Reason: this packet changes behaviour users have in their fingers, and it adds a decision to a
function that previously only registered what it was given.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- **The migration is the table edit.** `overrides_from_effective`
  (`state.rs:125-138`) omits every entry equal to the built-in default, and `init_state`
  (`state.rs:56-77`) resolves `override.or(default)`. So a user still on the old default has no
  entry, and changing the default moves them. A user who rebound has an entry and keeps it.
  That is precisely `P18`'s "only rewrites users still on the old defaults", achieved with no
  migration code. Writing a migration that rewrites files would be worse: it converts defaults
  into overrides and breaks the next default change for everyone.
- **The one case that needs code** is the collision. `conflicting_action`
  (`state.rs:142-163`) runs only in the rebind capture UI. `apply_key_bindings`
  (`state.rs:88-110`) registers everything it is handed, so a user override equal to a new
  default produces two bindings on one keystroke. `DEC-0018` settles the rule; this packet
  implements it. It is the first time `apply_key_bindings` decides rather than registers —
  worth a comment at the site as well as the module doc.
- **Proof has a real ceiling.** `F31` is a code read: the walkthrough drove the application
  with posted `WM_*` messages, which do not set modifier state, so no Ctrl chord was ever
  delivered and none can be in a repeat walk. The focused tests prove the table and the
  migration rule. The claim "`Ctrl-S` now reaches the shell" can only be proven by a human at
  a real keyboard. Do that, or record it as unverified — `DEC-0018`'s first Consequence says so
  explicitly, and this packet must not claim it either way without evidence.
- **The Reset path.** Resetting an action writes its default into `effective` and then omits it
  on save. After this packet, Reset restores the new keystroke. Walk it, because a user's first
  reaction to a moved default may well be to press Reset.
- `research/before/27-settings-keybindings.png` and `38-keybindings-edit-menu.png` show the
  table as shipped; they are the before pictures.

### The shipped table, before and after

`crates/settings-ui/src/key_bindings/key_bindings_actions.rs`, group `App Menu`. Only the four
rows `DEC-0018` names change; every other default in `BINDABLE_ACTIONS` is untouched, and the
table-wide test asserts that no two defaults share a keystroke in one key context.

| Action | id | Old default | New default | Why it moved |
|---|---|---|---|---|
| New SSH Session | `new_ssh_session` | `ctrl-s` | `ctrl-shift-n` | `^S` is XOFF — stops terminal output |
| Quit | `quit` | `ctrl-q` | `ctrl-shift-q` | `^Q` is XON — resumes terminal output |
| About OneTerm | `about` | `ctrl-space` | `f1` | `^@`/NUL is set-mark, and the IME toggle on several input methods |
| Toggle Gutter | `toggle_gutter` | `ctrl-g` | *(unbound)* | `^G` is BEL and the readline/Emacs abort; a view toggle earns no default |

Unchanged by explicit exception (`DEC-0018`'s recorded soft spot): `close_panel` = `ctrl-w`,
`new_terminal_tab` = `ctrl-t`. Unchanged because they were never in scope: `toggle_zoom` =
`shift-escape`, `open_settings` = `ctrl-,`, and every `Edit Menu`, `Terminal Context Menu`,
`Input Channel`, `Session Tabs Context Menu` and `SFTP Context Menu` row.

`f1` was checked against the gpui-component snapshot before being shipped as a default, as
this packet's Risks require: no binding, action or keystroke string `f1` exists anywhere under
`reference/gpui-kit/crates/`, so `apply_key_bindings`'s name-based snapshot filter has nothing
to miss.

### The migration, and the one case that needs code

The load-bearing fact, quoted as the packet asks — `crates/settings/src/ui_config.rs`:

> Per-action key-binding overrides: action id → keystroke string… Missing entries fall back to
> the built-in default.

`overrides_from_effective` (`state.rs`) writes only entries that differ from the built-in
default, and `init_state` resolves `override.or(default)`. So a user still on an old default
has no entry and the table edit moves them; a user who rebound has an entry and keeps it. No
migration code, no file rewritten — as `DEC-0018` and the intake's high-level design both say.

The collision is the exception, and it is implemented in `apply_key_bindings`, which is where
`DEC-0018` puts it. The rule as code: an action still sitting on its shipped default loses that
keystroke to any *other* action in the same key context whose keystroke is a user override
parsing to the same key. The displaced action is emptied, so it is unbound both in the keymap
and on the Key Bindings page — one source of truth rather than a keymap and a page that
disagree. `apply_key_bindings` runs at startup and after every rebind and reset, so a collision
reintroduced by **Reset** (which writes a default back without going through the capture UI's
`conflicting_action` check) is caught too. The `warn` fires only when the resolution actually
changes something, so it is one line per collision, not one per apply.

The rule is the pure function `collisions_with_overrides(&effective)`, which is where this
packet's confidence comes from: the three `DEC-0018` cases plus the table-wide assertion are
unit tests over `(BINDABLE_ACTIONS, overrides)` and need no window.

### Where the release note comes from

`.github/workflows/release.yml` generates the notes from Conventional Commit **subjects** only
(the body is read solely to detect a `BREAKING CHANGE:` trailer, which promotes the same
subject line into a "Breaking Changes" section). There is no changelog file for the
application. The four moved defaults are therefore carried by this packet's commit: a `!`
subject plus a `BREAKING CHANGE:` trailer listing old → new, so the release names the change in
its Breaking Changes section and the table itself is one `git show` away. Recorded here because
"the release notes list the four moved defaults" cannot be satisfied more literally with the
current generator.

## Plan

- [ ] Confirm `DEC-0018` is Accepted and the owner has accepted the defaults. **Do not start
      before this** — the whole packet is the decision's implementation.
- [ ] Write the focused tests first: the table assertions and the three migration cases.
- [ ] Edit the four defaults.
- [ ] Implement the collision rule in `apply_key_bindings`.
- [ ] Update `DEC-0018`, the module doc, and the release notes.
- [ ] Walk the Key Bindings page; get the real-keyboard check done or record it as unverified.

## Decisions

`DEC-0018` — `docs/decisions/DEC-0018-app-shortcuts-leave-single-ctrl-keys-to-the-terminal.md`.
This packet implements it and does not restate its rationale.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-settings-ui` over:
   - the shipped table: no two defaults share a keystroke in the same key context; the four
     moved rows hold exactly what `DEC-0018` records;
   - the migration, as a pure function over `(BINDABLE_ACTIONS, overrides)`: no entry moves to
     the new default; a differing entry is untouched; an entry colliding with a new default
     wins, and the displaced action resolves to unbound.
   These are the only automated proofs and they are where this packet's confidence comes from.
2. **Unit:** `cargo test -p oneterm-settings-ui`, `cargo test -p oneterm-settings`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `27-settings-keybindings.png` — the App Menu group showing the new defaults.
   - `38-keybindings-edit-menu.png` — the Edit Menu group, unchanged.
   Plus, not in the walkthrough: a seeded `ui_config.json` with (a) no entries, (b) an entry
   differing from the old default, and (c) an entry colliding with a new default — launch on
   each and capture the Key Bindings page, so all three migration cases have a frame. The
   collision case must also show the displaced action as unbound, and the `warn` line must be
   in the log.
6. **Manual, at a real keyboard (not scriptable here):** with a terminal focused, press
   `Ctrl-S` and confirm the shell stops output rather than the application opening a dialog;
   press `Ctrl-Q` and confirm output resumes rather than the application quitting. If this
   cannot be done, say so in Evidence and mark `F31` as verified only at the table level.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Taking a keystroke a user is already using.** The collision rule exists for this and is the
  packet's most important test. Getting it wrong means two actions on one key, and gpui will
  pick one.
- **Shipping without the owner's acceptance.** This is a change users feel on upgrade with no
  prompt. Both gates are in the acceptance for that reason.
- **Claiming the outcome from the table.** The table says what OneTerm binds. It does not prove
  the keystroke reaches the shell. `DEC-0018` and this packet both say so; do not let a green
  test suite become "verified".
- **Quietly moving `ctrl-w` or `ctrl-t` too.** They are readline keys and the rule arguably
  covers them, but `DEC-0018` keeps them by explicit exception. Moving them here would ship an
  unaccepted default change.
- **`f1` colliding with the platform or the kit.** Check that nothing in the gpui-component
  snapshot already binds `f1` before shipping it as a default — `apply_key_bindings` strips
  snapshot bindings only for rebindable action *names*, so a kit binding on `f1` for a
  different action would survive.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.

Blocked until: `DEC-0018` is Accepted and the owner has accepted the four new defaults.
