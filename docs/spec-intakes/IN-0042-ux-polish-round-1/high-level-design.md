# High-Level Design: UX polish round 1

Intake: IN-0042
Lane: normal
Date: 2026-09-17

## Idea

A first-time walkthrough of the shipped application (`research/ux-walkthrough-2026-09-16.md`)
produced 34 findings and 25 proposals, and the owner ruled that all of them ship, followed by
a before/after report. This is not one feature: it is nineteen small, independent changes to
surfaces that already exist, spread over eight crates, plus one closing evidence packet.

So the design work here is not "how do we build a thing". It is: **which crate owns each
change, which of them need a seam that does not exist yet, and how the round proves itself
visually.** Four changes need a seam decided before their packet starts — the `+` menu's
second dialog row, the right dock's width, the session form's scroll, and the key-binding
default migration — and those four are designed below. The remaining fifteen are
edits inside a single crate's own view code, and their packets carry their own detail.

The whole round is additive to existing surfaces. No new panel, no new persisted document,
no new dependency, no new external effect. One `WorkspaceCommands` field and one key-binding
default table are the only contract changes.

## Diagram

Crate ownership of the nineteen packets. The arrows are the existing dependency edges; only
the dashed one is new work, and it is a field on a type that already sits on that edge.

```text
                         crates/theme  ── US-0111 (contrast floor for muted.foreground)
                                │
        ┌───────────────────────┼───────────────────────────────┐
        │                       │                               │
crates/workspace (L3 shell)     │                       crates/settings-ui (L3)
  BUG-0067 dock mode toggle     │                         US-0121 About / Network / theme list
  US-0112  status bar eliding   │                         US-0122 sidebar nav + General
  US-0113  dock width clamp     │                         US-0123 default key bindings
        │                       │                               │
        └──────────┬────────────┴───────────────┬───────────────┘
                   │                            │
          crates/state (L2)  ◄──────────────────┘
            commands.rs      : WorkspaceCommands (+1 fn pointer, US-0114)
            form_dialog.rs   : FormDialog body scroll (US-0120)
            dock_persistence : docks.json owner (US-0113 clamp, US-0124 table state)
                   ▲
                   │ reads the registry
        ┌──────────┴───────────┐
        │                      │
crates/terminal-view (L3)      │        crates/session-ui (L3)
  US-0114 tab labels, + menu   │          BUG-0068 one error prefix
  US-0115 empty Space, search  │          US-0118  failed connect keeps the form
  US-0116 tab context menu     │          US-0119  panel header +, chevron, Delete
  US-0117 active Space cue     │          BUG-0069 Logging descenders
        ▲                      │          US-0120  form scroll, Advanced, colour row
        └──────────────────────┴───────── (R5: the one allowed same-layer edge)
                                                     ▲
crates/sftp-ui (L3)  US-0124                         │  registered at the composition root
crates/agent-ui (L3) US-0125                  crates/app/src/init.rs
                                                     ╎
                                    ╎╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╎  new: open_new_saved_session_dialog
                                                        (US-0114, dashed = added here)

crates/workspace + every crate above ── US-0126 (re-capture all 67 scenes, write the report)
```

Two packets reach code the project does not own and may not patch (`gpui-component` from
crates.io, no `[patch]` section): `US-0122` (the Settings sidebar scroll lives in
`gpui_component::setting::Settings`) and `US-0116` (the tab strip is
`gpui_component::dock::TabGroup`). Each must deliver a OneTerm-side solution or record the
limit with the `reference/gpui-kit/` file and line that shows why — see `IN-0042.md`
§Architecture and Boundary Questions.

## UI Wireframe

Only the surfaces whose shape changes are sketched. Everything else in the round is a colour,
a string, a spacing or a menu-row change that the packet describes in prose.

### `US-0114` — the `+` menu names the dialog it opens

The walkthrough's `F7`: the tab bar's "New SSH Session" opens **SSH Quick Connect**, while the
tree's "New Session" opens **New SSH Session**. Same action name, two different dialogs. The
row order is the owner's, fixed at `US-0094` acceptance, so the fix renames the existing last
row and adds one beside it — it does not reorder anything above.

```text
   before                                  after
+------------------------------+     +------------------------------+
| Command Prompt               |     | Command Prompt               |
| PowerShell                   |     | PowerShell                   |
|------ SSH Sessions ----------|     |------ SSH Sessions ----------|
| [#] prod-web                 |     | [#] prod-web                 |
|- - - - - infra - - - - - - - |     |- - - - - infra - - - - - - - |
| [#] db-01                    |     | [#] db-01                    |
|------------------------------|     |------------------------------|
| New SSH Session              |     | Quick Connect...             |  <- renames the row to
+------------------------------+     | New Saved Session...         |     the dialog it opens;
                                     +------------------------------+     the new row opens the
                                                                          full session dialog
```

Tab labels in the same packet (`F1`): a local-shell tab reads its shell's display name
("Command Prompt", "PowerShell") instead of the fixed "Terminal", and a live OSC 0/2 title
still overrides it where a shell sets one.

```text
 before  [ Terminal x ][ Terminal x ][ Terminal x ][ dev@127.0.0.1:22 x ]
 after   [ Command Prompt x ][ PowerShell x ][ PowerShell 7 x ][ dev@127.0.0.1:22 x ]
```

### `US-0113` — the right dock width follows the window

At ~900 px the dock keeps its absolute ~490 px and the terminal is left ~410 px (`F9`,
`research/before/50-narrow-900.png`).

```text
 before, 900px window                      after, 900px window
+-------------------+---------------+     +---------------------------+-----+
| terminal ~410px   | dock ~490px   |     | terminal  ~585px          | ~315|
|                   | Session       |     |                           | px  |
|                   | (2 rows)      |     |                           |     |
|                   | No SFTP conn. |     |                           |     |
+-------------------+---------------+     +---------------------------+-----+
        the dock wins                       the dock is clamped to a share
                                            of the window; the user's own
                                            drag still wins above the clamp
```

### `US-0120` — the session form scrolls and folds its advanced fields

With Private Key selected the body is ~735 px and the footer sits at y≈900 in a 1000 px
window (`F20`); `crates/session-ui/src/session_dialog.rs:424` already carries a `ponytail:`
note saying `FormDialog` does not scroll.

```text
 before                                    after
+--------------------------------+        +--------------------------------+
| New SSH Session                |        | New SSH Session                |
| Label       [___] [#]          |        | Label     [___] [#][#][#][#] v  |  <- short colour
| Host        [______________]   |        | Host      [__________________]  |     row + Custom...
| Port        [____]             |        | Port      [____]                |
| Username    [______________]   |        | Username  [__________________]  |
| Auth        (o) Key ( ) Pass   |        | Auth      (o) Key ( ) Password  |
| Key file    [______________]   |        | Key file  [__________________]  |
| Passphrase  [______________]   |        | Passphrase[__________________]  |
| Jump host   [______________]   |        | > Advanced                      |  <- jump host,
| Port fwd    [______________]   |        |   (collapsed by default)        |     forwards,
| Port fwd    [______________]   |        | Group     [__________________]  |     agent fwd
| Agent fwd   [x]                |        | Logging   (o) Global ( ) On ... |
| Group       [______________]   |        +--------------------------------+
| Logging     (o) Global ( ) On  |        |            [Cancel]  [Save]     |  <- always on screen
+--------------------------------+        +--------------------------------+
|  [Cancel]  [Save]   <- y~900   |         the body scrolls when it still
+--------------------------------+         outgrows the window
```

### `US-0118` — a failed connect keeps the form and explains itself

```text
 before, after a 20s failure               after
+--------------------------------+        +--------------------------------+
| SSH Quick Connect              |        | SSH Quick Connect              |
| Host [10.10.10.10]             |        | Host [10.10.10.10]             |
| User [dev]                     |        | User [dev]                     |
| Password [          ]  <- wiped|        | Password [**********] <- kept   |
| [x] Save to SSH Sessions       |        | [x] Save to SSH Sessions        |
|                                |        | (!) SSH connect failed: timed   |  <- inline, the
|          [Cancel]  [Connect]   |        |     out after 20 s              |     same one-prefix
+--------------------------------+        |          [Cancel]  [Connect]    |     text BUG-0068
      ... and a toast in the far          +--------------------------------+     produces
      opposite corner that auto-           and the ticked Save either saved
      dismisses, and nothing was           the session or says why it did not
      saved although Save was ticked
```

### `US-0119` and `US-0125` — panel headers

```text
 Session panel header (US-0119)          Agent panel (US-0125)
+-----------------------------+        +-----------------------------+
| Session                 [+] |        | Agents                      |  <- header, matching
+-----------------------------+        +-----------------------------+     the SSH Client
| v  infra                    |        |                             |     sections
|      prod-web               |        |        [ icon ]             |
| >  lab                      |        |   No agents are running     |  <- plain language,
+-----------------------------+        |   A coding agent that       |     not "OSC 20308"
| (right-click the blank area |        |   reports its status shows  |
|  -> New Session)            |        |   up here while it works.   |
+-----------------------------+        +-----------------------------+
   chevron disclosure, not the
   maximise arrow (F23); Delete
   behind a separator, danger-styled
```

### `US-0124` — the SFTP table fits the dock it ships in

```text
 before, docked at ~490px                  after
+--------------------------------+        +--------------------------------+
| Name       | Date Modified | Pe|        | Name          | Size | Modified |
|------------|---------------|---|        |---------------|------|----------|
| build/     | 2026-09-16    |   |        | build/        |   -- | 09-16    |
| <--- horizontal scrollbar --->|        +--------------------------------+
+--------------------------------+         no horizontal scrollbar; the rest
  Size is not visible at all                of the columns behind a chooser

 dual pane (US-0124, second half)
+------------------+--------+------------------+
| local            |  [ > ] | remote           |   <- explicit transfer controls
|                  |  [ < ] |                  |      between the panes, and one
+------------------+--------+------------------+      shared action list behind
                                                      both the ... and the
                                                      right-click menu
```

## Data Flow

### 1. The `+` menu's second row reaches the full session dialog (`US-0114`)

The `+` menu is built in `crates/terminal-view`; the full "New SSH Session" dialog lives in
`crates/session-ui`. `oneterm-session-ui` already depends on `oneterm-terminal-view` — the
single same-layer edge R5 allows, because opening an SSH session builds a `TerminalPanel` —
so the reverse edge is both a cycle (R1) and a second cross-feature edge (R5). The seam that
already exists for exactly this is `oneterm_state::commands::WorkspaceCommands`, an
fn-pointer registry built only at the composition root.

Today `crates/state/src/commands.rs` carries:

```rust
/// Open the "New SSH session" quick-connect dialog.
pub open_new_session_dialog: fn(&mut Window, &mut App),
```

registered in `crates/app/src/init.rs:64-70` as
`open_new_session_dialog: oneterm_session_ui::open_quick_connect_dialog`. The name says
"new session" and the target is quick connect — which is `F7` restated in the registry
itself. The packet:

1. Renames the existing field to `open_quick_connect_dialog` so the field, the function and
   the menu row all say the same thing.
2. Adds one field beside it, `open_new_saved_session_dialog: fn(&mut Window, &mut App)`,
   registered to the `crates/session-ui` entry point that opens the full dialog — the same
   one `crates/session-ui/src/panel.rs` reaches from the tree's context menu.
3. The `+` menu's builder closure calls `commands(cx).open_new_saved_session_dialog(...)` for
   the new row, exactly as the saved-session rows already call `open_saved_ssh_session`.

No new mechanism, no new edge, no persisted change. `WorkspaceCommands` is `Copy` and is
constructed in one place plus the crates' test doubles, so widening it is a compile-time
change every test double must follow — that is the packet's integration proof.

The `NewSession` action keeps its current target (quick connect) so the key binding does not
change meaning under the user; `US-0123` is where the key bindings themselves move.

### 2. The dock width becomes proportional (`US-0113`)

Where the width lives today:

- `crates/workspace/src/layout/workspace/mod.rs:33` — `DEFAULT_RIGHT_DOCK_WIDTH = px(480.)`,
  used only when no saved layout provides one.
- `crates/workspace/src/layout/workspace/layout.rs:41-45,76` — both layout builders read
  `DockArea::dock_size(Right)` and write it straight back through `set_dock_size`, so a
  restart or a centre reset preserves whatever absolute width was there.
- `crates/workspace/src/layout/workspace/actions.rs:169-172` — `switch_right_dock_mode` does
  the same snapshot-and-restore around a mode swap.
- `crates/state/src/dock_persistence.rs` — `DockDocument` owns `docks.json` but **flattens**
  the kit's own layout fields into `dock_fields: BTreeMap<String, Value>`. The width is one of
  those kit fields; OneTerm does not name it and should not start naming it.

So the width is an absolute pixel count that OneTerm reads and writes at four seams and never
compares with the window. The change is a single clamp function in `crates/workspace`, applied
at every seam that already calls `set_dock_size`:

```text
window width ──┐
               ├─> clamp_right_dock_width(requested, window_width) ──> set_dock_size(Right, ..)
saved/px ──────┘
```

1. The clamp is pure: `(requested px, window px) -> px`. It is the packet's focused unit test,
   and it is the only new logic — a value the user dragged wider than the ceiling comes back
   at the ceiling, a narrower one is returned unchanged, and a window narrower than the
   floor-plus-minimum-terminal collapses the dock instead of squeezing it.
2. It is applied where the width is already set: the two builders in `layout.rs`, the mode
   swap in `actions.rs`, and on window resize. The kit still owns the dragged value and still
   persists it; OneTerm only decides what it applies.
3. `docks.json` is unchanged — no field added, no schema bump. A user whose saved width is
   above the ceiling simply gets the ceiling applied on the next launch, and the kit rewrites
   the clamped value on the next save. The packet confirms that before implementing rather
   than assuming it.

The exact shape — a percentage ceiling, a ceiling plus auto-collapse below a threshold, or a
fully proportional width — is the packet's call against the measurement in
`research/before/50-narrow-900.png`. The acceptance is fixed here: at ~900 px the terminal
keeps the majority of the window.

### 3. The session form gets a scrolling body (`US-0120`)

`crates/state/src/form_dialog.rs` is the shared "form in a dialog" scaffold every feature
crate uses: a titled `DialogContent`, a body built by a `ContentFn`, and a footer with Cancel
plus one confirm button. Nothing in it scrolls, which
`crates/session-ui/src/session_dialog.rs:424` already records:

```text
// Wide enough for one port-forward row per line. ponytail: FormDialog does
// not scroll; a session with more than about five forwards outgrows a
// 1080p window, make the dialog body scroll when that happens.
.width(px(560.))
```

This packet is that note coming due. The scroll goes into `FormDialog`, not into
`session_dialog.rs`, because the ceiling is the scaffold's: every dialog built on it has the
same failure on a short window, and one guard in the shared builder is a smaller change than
a guard in each caller.

```text
FormDialog::open
  └── DialogContent
        ├── body:   ContentFn output, wrapped in a max-height + scroll container
        │           (max height derived from the window, so the footer is always visible)
        └── footer: Cancel + confirm, outside the scroll container
```

Two consequences the packet must hold: the footer never scrolls out of reach (it is the whole
point), and a body that fits gets no scrollbar — the app theme's `ScrollbarMode::Always` means
a scroll container that is always present is a scrollbar that is always visible, which would
be a visible regression on every small dialog in the application.

The second half of the packet — folding Jump host, Port forwards and Agent forwarding into an
"Advanced" disclosure and replacing the 130-swatch popup with the eight-swatch row plus
"Custom…" — is local to `session_dialog.rs` and adds no seam.

### 4. The key-binding default migration (`US-0123`)

The persisted shape makes this much smaller than it looks. `ui_config.json` stores
`key_bindings: HashMap<String, String>` — action id to keystroke — and
`crates/settings-ui/src/key_bindings/state.rs`'s `overrides_from_effective` **omits every
entry equal to the built-in default**. `init_state` then resolves each action as
`override.or(default)`.

```text
ui_config.json                 BINDABLE_ACTIONS
 key_bindings: {                default: Some("ctrl-s")
   "quit": "ctrl-alt-q"   ──┐        │
 }                          │        │
                            v        v
              init_state:  override.or(default)  ──> effective keystroke
```

So:

- A user who never rebound New SSH Session has **no entry** for it. Changing the default in
  `key_bindings_actions.rs:80` moves them to the new keystroke with no migration code at all.
- A user who deliberately set `ctrl-s` also has no entry, because it equalled the old default
  and was omitted on save. They are indistinguishable from the first user and move too. That
  is exactly `P18`'s "rewrite only users still on the old defaults" — the sparse map already
  expresses the rule, so the migration is *the default table edit itself*.
- A user who rebound to something else keeps their entry and is untouched.

One real hazard remains, and it is what the packet's focused test is for: a user whose
existing override **collides with a new default**. If someone bound Quit to `ctrl-shift-q`
by hand and `ctrl-shift-q` then becomes the New SSH Session default, `apply_key_bindings`
binds both — `conflicting_action` only runs at capture time, not at apply time. The packet
resolves that at apply: when a new default collides with a surviving user override, the
override wins and the action whose default collided is left unbound, logged once. That rule,
the four new defaults, and the fact that no migration code is needed for the common case are
what `DEC-0018` records.

Nothing here is a schema change: the map's shape, its key set and its semantics are unchanged.

### 5. The before/after evidence (`US-0126`, and every packet's E2E step)

The round is judged visually, so the evidence plan is fixed here rather than improvised at
the end.

```text
scratchpad/ux/NN-*.png  (67 frames, the walk of main @2f12628a)
        │
        │  copied with this intake, unedited, names unchanged
        v
research/before/NN-*.png   <- frozen. Never re-captured, never replaced.
        │
        │  each packet re-captures only the scenes its acceptance names,
        │  under the same numbers, into its own evidence
        v
evidence/<ID>-NN-*.png     <- per-packet proof, captured on that packet's build
        │
        │  US-0126 re-walks the whole application once, on the closed round
        v
evidence/after/NN-*.png    <- 67 frames, same scenes, same numbers
        │
        v
evidence/before-after-report.md
   one row per finding F1-F34: the before frame, the after frame, the packet
   that addressed it, and a verdict (fixed / partially fixed / not fixed with
   the recorded limit / observation only, not packeted)
```

Rules the packets inherit:

- `research/before/` is the baseline and is immutable. A packet that wants to show a
  difference cites the before frame by path; it never regenerates it.
- Scene numbers are the walkthrough's. An after frame named `24b` shows what `24b` showed,
  from the same state, so the pair is comparable at a glance.
- Every GUI walk launches its own build, drives **only its own process id**, and captures
  with `PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` — the owner runs their own OneTerm and
  no walk may enumerate, focus, or close a window by name or title.
- Scenes the walkthrough could not exercise stay unexercised unless the packet finds another
  route, and the packet says so: no Ctrl/Shift chord and no double-click can be driven by
  posted messages, which is why `US-0123`'s proof is a code read plus the focused test on the
  default table rather than a captured keystroke.
- `F27` and `F33` are observations with no proposal; the report carries them with that verdict
  rather than leaving them unexplained.

## Detail Design

- [ ] Detail design: not needed
- Reason: normal lane. The four seams that needed a decision are decided above; the other
  fifteen packets are edits inside one crate's own view code, each with its own acceptance
  and verification plan in its packet. No authorization, migration, secret or external-effect
  surface is touched, so nothing here meets the high-risk trigger that would require a
  `low-level-design/` file. `US-0116` and `US-0122` may each produce a short design note
  inside their own packet once the `reference/gpui-kit/` read settles what is reachable from
  this side; that is packet-local and does not need a shared detail design.
