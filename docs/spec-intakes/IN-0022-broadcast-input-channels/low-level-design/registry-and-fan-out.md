# Low-Level Design: Input channel registry and fan-out

Intake: IN-0022
HLD: ../high-level-design.md
Topic: registry-and-fan-out
Date: 2026-09-09

## Concern

The `InputChannelRegistry` in `crates/state` and the four points in `crates/terminal-view`
that repeat an origin's input on its channel peers.

## Design

`InputChannel` (`oneterm-core`):

```text
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Deserialize)]
pub enum InputChannel { A, B, C, D, E }
impl InputChannel {
    pub const ALL: [InputChannel; 5];
    pub fn label(self) -> &'static str;   // "A".."E"
    pub fn index(self) -> usize;          // 0..5, selects chart_1..chart_5
}
```

`Deserialize` is required by the `gpui::Action` derive on `JoinInputChannel(InputChannel)`,
the same way `ShellKind` serves `AddPanelWithShell`.

`InputChannelRegistry` (`oneterm-state`, `Entity` behind a `Global` marker, `init` idempotent
like `AgentRegistry::init`):

```text
pub struct Member { channel: InputChannel, session: Entity<Box<dyn TerminalSession>>, joined: u64 }
pub struct InputChannelRegistry { members: HashMap<EntityId, Member>, next_seq: u64 }
```

- `join(id, channel, session, cx)`: insert or replace; `joined = next_seq++` only when the
  entry is new or its channel changes (moving channel counts as a fresh join for ordering).
  Calls `cx.notify()`.
- `leave(id, cx) -> bool`: remove; notify only when something was removed.
- `close(channel, cx) -> usize`: remove every member of that channel; returns the count.
- `channel_of(id) -> Option<InputChannel>`.
- `members(channel) -> Vec<(EntityId, Entity<Box<dyn TerminalSession>>)>` sorted by `joined`.
- `channels_in(ids: &[EntityId]) -> Vec<InputChannel>`: distinct channels, sorted A..E (feeds
  the tab chips).
- `fan_out(origin, input, cx)`: `channel_of(origin)` or return; for every member of that
  channel except `origin`, in `joined` order: `session.update(cx, |s, _| { s.scroll_to_bottom();
  apply(s, input) })`. `apply` maps `Bytes` → `write`, `Text` → `commit_text`, `Paste` →
  `paste`, `Interrupt` → `send_ctrl_c`. `write` and `paste` results go through the same
  "generated input" logging the view uses today; nothing aborts the loop.

`fan_out` is a `&self` read over a snapshot of the member list taken before the first
`session.update`, so a session callback that mutates the registry (none exists today) could
not invalidate the iteration.

Hooks (`crates/terminal-view`), each one line after the existing own-session write:

| Site | Existing call | Added call |
| --- | --- | --- |
| `input/keys.rs` `send_key` | `s.write(&bytes)` | `deps.fan_out(id, Bytes(&bytes), cx)` |
| `terminal_view/ime.rs` `replace_text_in_range` | `s.commit_text(text)` | `deps.fan_out(id, Text(text), cx)` |
| `input/edit.rs` `paste_text` | `s.paste(text)` | `deps.fan_out(id, Paste(text), cx)` |
| `input/keys.rs` `interrupt` | `s.send_ctrl_c()` | `deps.fan_out(id, Interrupt, cx)` |

`send_key`, `interrupt`, and `paste_text` take the session entity today; they gain the
origin `EntityId` and `&TerminalDeps` (or a small `BroadcastOrigin { id, registry }` copied
out of the view) so the classifier and encoders stay pure. `TerminalDeps.fan_out` is a no-op
when `input_channels` is `None` (tests, headless).

`TerminalView::shutdown` calls `registry.leave(cx.entity_id())` next to
`AgentRegistry::remove_terminal`.

## Interfaces

```text
// oneterm-state
impl InputChannelRegistry {
    pub fn global(cx: &App) -> Entity<Self>;
    pub fn try_global(cx: &App) -> Option<Entity<Self>>;
    pub fn init(cx: &mut App);
    pub fn join(&mut self, id: EntityId, channel: InputChannel,
                session: Entity<Box<dyn TerminalSession>>, cx: &mut Context<Self>);
    pub fn leave(&mut self, id: EntityId, cx: &mut Context<Self>) -> bool;
    pub fn close(&mut self, channel: InputChannel, cx: &mut Context<Self>) -> usize;
    pub fn channel_of(&self, id: EntityId) -> Option<InputChannel>;
    pub fn members(&self, channel: InputChannel) -> Vec<(EntityId, Entity<Box<dyn TerminalSession>>)>;
    pub fn channels_in(&self, ids: &[EntityId]) -> Vec<InputChannel>;
    pub fn fan_out(&self, origin: EntityId, input: BroadcastInput<'_>, cx: &mut App);
}
pub enum BroadcastInput<'a> { Bytes(&'a [u8]), Text(&'a str), Paste(&'a str), Interrupt }

// oneterm-terminal-view
impl TerminalView {
    pub(crate) fn join_channel(&mut self, channel: InputChannel, cx: &mut Context<Self>);
    pub(crate) fn leave_channel(&mut self, cx: &mut Context<Self>);
    pub(crate) fn channel(&self, cx: &App) -> Option<InputChannel>;
}
```

## Edge Cases and Failure Modes

- [ ] Origin not a member: `fan_out` returns without touching any session.
- [ ] Only member of its channel: no peer writes; own write unchanged.
- [ ] Joining channel B while in A: the Space leaves A implicitly; A's other members are
  untouched.
- [ ] A peer session has already exited (tab kept open by the "session closed" banner): the
  write fails, is logged like a failed key write today, and the remaining peers still receive
  the input.
- [ ] A peer is on the alternate screen (`vim`): bytes are delivered unchanged; the user
  chose membership for that Space. Documented in `docs/terminal-split.md`.
- [ ] Paste over the size limit: the origin's own `paste` reports the error and writes
  nothing; `fan_out` is only called after a successful own write, so peers get nothing either.
- [ ] `shutdown` on a non-member: `leave` returns `false`, no notify.
- [ ] Registry not initialised (tests): `TerminalDeps.input_channels` is `None`; every hook
  is a no-op and `channel()` is `None`.

## Verification

- [ ] `crates/state` unit tests with a `MockSession` (the `crates/terminal` test-support
  session that records writes): join order, move between channels, leave, close count,
  `channels_in` order, `fan_out` skips origin, `fan_out` on a non-member writes nothing.
- [ ] `crates/terminal-view` tests: `send_key` on a member view writes the same bytes to two
  peer mock sessions and not to a third view outside the channel; `shutdown` removes the
  member; a view without a registry still writes to its own session.
