# High-Level Design: Broadcast input channels

Intake: IN-0022
Lane: normal
Date: 2026-09-09

## Idea

Five fixed input channels (A..E) group terminal Spaces. A global `InputChannelRegistry`
(same shape as `AgentRegistry`) maps each member `TerminalView` to its channel and session
handle. The originating view writes to its own session exactly as today, then asks the
registry to repeat the same write on every other member of its channel. Membership is shown
by a lettered chip per channel in the tab strip and a coloured frame around each member
Space, both using the theme's `chart_1..chart_5` colours so A..E stay distinct in light and
dark themes.

The unit of membership is the Space (DEC-0009). A tab is a container: its chip row lists the
channels of its Spaces, and two tab-wide menu items apply a join or leave to every Space in
the tab.

## Diagram

```text
 Tab "fleet" (split into 3 Spaces)         InputChannelRegistry (crates/state, Global)
 ┌──────────────┬──────────────┐          ┌────────────────────────────────────────┐
 │ Space 1 [A]  │ Space 2 [A]  │          │ members: EntityId -> Member {          │
 │ web-01       │ web-02       │          │   channel: InputChannel,               │
 │  origin ─────┼──write───────┼─────────▶│   session: Entity<Box<dyn Session>>,   │
 ├──────────────┴──────────────┤          │   joined: u64 }                        │
 │ Space 3      htop (no chan) │          │ join / leave / close / members /       │
 └──────────────────────────────┘          │ channel_of / fan_out(origin, input)    │
                                           └──────┬────────────────┬────────────────┘
 Tab "db" (1 Space) [A]                           │ write          │ write
 ┌──────────────────────────────┐          ┌──────▼──────┐  ┌──────▼──────┐
 │ db-01                        │◀─────────│ Space 2     │  │ db-01       │  origin skipped,
 └──────────────────────────────┘          └─────────────┘  └─────────────┘  Space 3 untouched
```

`BroadcastInput` is the unit of fan-out and mirrors the four `TerminalSession` write methods:

```text
enum BroadcastInput<'a> { Bytes(&'a [u8]), Text(&'a str), Paste(&'a str), Interrupt }
```

## UI Wireframe

Context menu of a Space (submenu shown for a Space in channel B inside a split tab):

```text
+------------------------------+      +-----------------------------------+
| New Terminal          Ctrl+T |      |   Channel A                       |
| Duplicate Session          > |      | * Channel B                       |
| Split Right ...              |      |   Channel C                       |
| ---------------------------- |      |   Channel D                       |
| Input Channel              > |─────▶|   Channel E                       |
| ---------------------------- |      | --------------------------------- |
| Copy / Paste / Select All    |      |   Leave Channel                   |
| ...                          |      |   Close Channel B                 |
+------------------------------+      | --------------------------------- |
                                      |   Join All Spaces In Tab To B     |
                                      |   Leave With All Spaces In Tab    |
                                      +-----------------------------------+
```

- `Channel A..E` join **this Space**; the current channel is marked.
- `Leave Channel` and `Close Channel <X>` appear only when this Space is a member.
- The last section appears only when the tab has more than one Space. "Join All Spaces In
  Tab To <X>" needs this Space to be a member (it copies this Space's channel to its
  siblings); "Leave With All Spaces In Tab" appears when any Space in the tab is a member.

Tab strip and Space frames (tab "fleet" split into three Spaces, tab "db" with one Space):

```text
+-[A] fleet ------------+-[A] db --------+-[B][C] mixed --+- local ------+
|╔══════════[A]╦══════════[A]╗                                        |
|║ $ echo hi    ║ $ echo hi    ║  <- member Space: the channel badge in   |
|║ hi           ║ hi           ║     its top-right corner, plus a 1 px    |
|╠══════════════╩══════════════╣     frame in the channel colour        |
|│ htop ...                    │     (chart_1 for A; 55 % while another   |
|│                             │     Space is active)                     |
|└─────────────────────────────┘  <- non-member: no badge, today's theme |
|                                     border                              |
```

- One chip per distinct channel in the tab, ordered A..E ("[B][C]" above). A tab with no
  member Space shows no chip.
- Every member Space repeats that chip as a badge in its own top-right corner. The badge is
  the per-Space marker: in a tab of three Spaces where only one joined, the frame colour is
  too close to the active-Space border to be read on its own.
- A tab whose single Space is a member draws the badge and the frame too (today a single
  Space draws no frame); the chip alone is too easy to miss when a password is about to fan
  out.

## Data Flow

1. The user picks "Input Channel › Channel A" on a Space. The menu dispatches
   `JoinInputChannel(InputChannel::A)`; the panel handler resolves the active Space's
   `TerminalView` and calls `view.join_channel(A, cx)`, which calls
   `registry.join(entity_id, A, session.clone())`. Joining while in another channel moves the
   Space (one channel per Space). "Join All Spaces In Tab To A" calls the same for every
   `tree.terminal_views()` of the panel.
2. The registry notifies; tab strips read `registry.channels_in(&[entity_ids])` and Space
   frames read `registry.channel_of(entity_id)` during render.
3. A key press in a member Space goes through `on_key_down` → `send_key` as today. After its
   own write the view calls `registry.fan_out(entity_id, Bytes(&bytes), cx)`. Typed text
   (`replace_text_in_range` in `ime.rs`), paste (`paste_text` in `edit.rs`), and Ctrl+C
   (`interrupt`) do the same with `Text`, `Paste`, and `Interrupt`.
4. `fan_out` looks up the origin's channel; if none, it returns. Otherwise it iterates the
   channel's members in join order, skips the origin, and for each session runs
   `scroll_to_bottom` then the matching write method. Write errors are logged through
   `report_generated_input` and never stop the loop. Peer views are not involved, so nothing
   recurses and nothing is typed twice. Sibling Spaces in the same tab and same channel
   receive the input like any other member.
5. "Leave Channel" calls `registry.leave(entity_id)`; "Leave With All Spaces In Tab" calls it
   for every Space of the tab; "Close Channel X" calls `registry.close(X)`, which removes every
   member of X in every tab. A channel with no members is simply absent; there is no separate
   open/closed state.
6. Tree changes: split and Duplicate Session create a new `TerminalView`, which starts as a
   non-member. Dragging a tab into an empty Space moves the existing `TerminalView` entity
   (`take_active_terminal_view` → `fill_empty`), so membership travels with it. Close Space
   and Close Tab run `TerminalView::shutdown`, which calls `registry.leave(entity_id)`.

Out of scope on purpose: mouse reports, scroll, resize, completion navigation keys (they act
on the origin only; accepted completion text reaches peers as `Text`), a status-bar segment,
and persistence (sessions are not restored across restarts, see `docs/agents/persistence.md`).

## Crate placement

- `crates/core/src/input_channel.rs`: `InputChannel` (`A..E`, `label()`, `ALL`, `index()`),
  placed in `oneterm-core` because `oneterm-actions` is an L1 crate that depends on
  `oneterm-core` only (`docs/agents/crate-dependency-rules.md`), the same placement as
  `ShellKind` used by `AddPanelWithShell`.
- `crates/state/src/input_channel_registry.rs`: `InputChannelRegistry` + `Global` wrapper,
  `BroadcastInput`, `Member`. `oneterm-state` already depends on `oneterm-terminal`, so it
  holds `Entity<Box<dyn TerminalSession>>` directly; no callback type erasure is needed.
- `crates/actions`: `JoinInputChannel(InputChannel)`, `LeaveInputChannel`,
  `CloseInputChannel`. The two tab-wide items are menu closures on the panel; they need no
  action because nothing binds them to a key.
- `crates/terminal-view`: `TerminalDeps.input_channels: Option<Entity<InputChannelRegistry>>`
  resolved in `from_globals`; hooks in `input/keys.rs`, `terminal_view/ime.rs`,
  `input/edit.rs`; submenu in `input/menu.rs` (`MenuContext` gains the Space's channel and
  the tab's member count); chips in `panel/tab_title.rs`; frame colour in
  `space/render.rs`; `on_action` handlers in `panel/terminal_panel.rs`.
- `crates/settings-ui/src/key_bindings`: seven `BindableAction` entries (five joins, leave,
  close) in group "Input Channel", all `default: None`.
- Theme: `cx.theme().chart_1..chart_5` map to A..E in that order.

## Detail Design

- [x] Detail design: added (optional)
- Reason: two concerns are worth pinning before code: the registry and fan-out contract
  (`low-level-design/registry-and-fan-out.md`) and the menu, chip, and frame rules
  (`low-level-design/menu-chip-frame.md`).
