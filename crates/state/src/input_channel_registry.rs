//! `InputChannelRegistry` — who is in which broadcast input channel, and the
//! fan-out that repeats one Space's input on the others.
//!
//! Design: `docs/spec-intakes/IN-0022-broadcast-input-channels/` (HLD +
//! `low-level-design/registry-and-fan-out.md`). A member is one Space — one
//! `TerminalView` and its session, keyed by the view's `EntityId` (DEC-0009).
//! The registry lives here, below the feature crates, for the same reason
//! [`crate::AgentRegistry`] does: `oneterm-state` already depends on
//! `oneterm-terminal`, so it can hold the session entity directly and no
//! feature crate has to learn about another.
//!
//! Membership is in-memory only; nothing about channels reaches `docks.json`.

use std::collections::HashMap;

use gpui::{App, AppContext, Context, Entity, EntityId, Global};

use oneterm_core::{InputChannel, report_best_effort};
use oneterm_terminal::{TerminalSession, report_generated_input};

/// One member: the channel it joined, the session its peers write to, and the
/// join sequence that orders the fan-out.
struct Member {
    channel: InputChannel,
    session: Entity<Box<dyn TerminalSession>>,
    joined: u64,
}

/// One broadcastable input, mirroring the four `TerminalSession` write methods
/// the view calls for user input.
#[derive(Clone, Copy, Debug)]
pub enum BroadcastInput<'a> {
    /// An encoded keystroke.
    Bytes(&'a [u8]),
    /// Committed IME / typed text.
    Text(&'a str),
    /// Clipboard text (the peer applies its own bracketed-paste mode).
    Paste(&'a str),
    /// Ctrl+C.
    Interrupt,
}

/// Global membership of the five broadcast input channels.
#[derive(Default)]
pub struct InputChannelRegistry {
    members: HashMap<EntityId, Member>,
    next_seq: u64,
}

/// Global wrapper for `Entity<InputChannelRegistry>`.
pub struct InputChannelRegistryGlobal(pub Entity<InputChannelRegistry>);

impl Global for InputChannelRegistryGlobal {}

impl InputChannelRegistry {
    /// Get the global `Entity<InputChannelRegistry>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<InputChannelRegistryGlobal>().0.clone()
    }

    /// Get the global `Entity<InputChannelRegistry>` if initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<InputChannelRegistryGlobal>()
            .map(|g| g.0.clone())
    }

    /// Initialize the global registry.
    pub fn init(cx: &mut App) {
        if cx.try_global::<InputChannelRegistryGlobal>().is_none() {
            let entity = cx.new(|_| Self::default());
            cx.set_global(InputChannelRegistryGlobal(entity));
        }
    }

    /// Put `id` in `channel`, leaving whichever channel it was in. Moving to
    /// another channel counts as a fresh join, so the fan-out order of the new
    /// channel follows the move.
    pub fn join(
        &mut self,
        id: EntityId,
        channel: InputChannel,
        session: Entity<Box<dyn TerminalSession>>,
        cx: &mut Context<Self>,
    ) {
        let joined = match self.members.get(&id) {
            Some(member) if member.channel == channel => member.joined,
            _ => {
                self.next_seq += 1;
                self.next_seq
            }
        };
        self.members.insert(
            id,
            Member {
                channel,
                session,
                joined,
            },
        );
        cx.notify();
    }

    /// Remove `id` from its channel. Returns whether it was a member.
    pub fn leave(&mut self, id: EntityId, cx: &mut Context<Self>) -> bool {
        let removed = self.members.remove(&id).is_some();
        if removed {
            cx.notify();
        }
        removed
    }

    /// Remove every member of `channel`. Returns how many were removed.
    pub fn close(&mut self, channel: InputChannel, cx: &mut Context<Self>) -> usize {
        let before = self.members.len();
        self.members.retain(|_, member| member.channel != channel);
        let removed = before - self.members.len();
        if removed > 0 {
            cx.notify();
        }
        removed
    }

    /// The channel `id` is in, if any.
    pub fn channel_of(&self, id: EntityId) -> Option<InputChannel> {
        self.members.get(&id).map(|member| member.channel)
    }

    /// The members of `channel`, in join order.
    pub fn members(
        &self,
        channel: InputChannel,
    ) -> Vec<(EntityId, Entity<Box<dyn TerminalSession>>)> {
        let mut members: Vec<_> = self
            .members
            .iter()
            .filter(|(_, member)| member.channel == channel)
            .collect();
        members.sort_by_key(|(_, member)| member.joined);
        members
            .into_iter()
            .map(|(id, member)| (*id, member.session.clone()))
            .collect()
    }

    /// The distinct channels `ids` are in, ordered A..E — the tab's chips.
    pub fn channels_in(&self, ids: &[EntityId]) -> Vec<InputChannel> {
        let mut channels: Vec<_> = ids.iter().filter_map(|id| self.channel_of(*id)).collect();
        channels.sort_unstable();
        channels.dedup();
        channels
    }

    /// Repeat `input` on every peer of `origin`'s channel, in join order.
    ///
    /// A non-member origin writes nothing. The origin itself is skipped — it
    /// has already written to its own session — so nothing recurses and
    /// nothing is typed twice. A peer whose session rejects the write (an
    /// exited session behind the "session closed" banner) is logged, never
    /// fatal: the remaining peers still receive the input.
    pub fn fan_out(&self, origin: EntityId, input: BroadcastInput<'_>, cx: &mut App) {
        let Some(channel) = self.channel_of(origin) else {
            return;
        };
        // Snapshot the peers before the first `update`, so a session callback
        // that mutates the registry could not invalidate the iteration.
        let peers: Vec<_> = self
            .members(channel)
            .into_iter()
            .filter(|(id, _)| *id != origin)
            .map(|(_, session)| session)
            .collect();
        for peer in peers {
            peer.update(cx, |session, _| {
                session.scroll_to_bottom();
                match input {
                    BroadcastInput::Bytes(bytes) => {
                        report_generated_input("input channel key", session.write(bytes));
                    }
                    BroadcastInput::Text(text) => session.commit_text(text),
                    BroadcastInput::Paste(text) => {
                        report_best_effort("input channel paste", session.paste(text));
                    }
                    BroadcastInput::Interrupt => session.send_ctrl_c(),
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use oneterm_terminal::test_support::{FakeSessionProbe, FakeTerminalSession};

    use super::*;

    /// A session entity plus the probe recording what reaches it. The entity id
    /// doubles as the member id: one Space owns one session.
    fn session(
        cx: &mut TestAppContext,
    ) -> (EntityId, Entity<Box<dyn TerminalSession>>, FakeSessionProbe) {
        let (session, probe) = FakeTerminalSession::boxed(4, 20, "");
        let session = cx.update(|cx| cx.new(|_| session));
        (session.entity_id(), session, probe)
    }

    fn registry(cx: &mut TestAppContext) -> Entity<InputChannelRegistry> {
        cx.update(|cx| cx.new(|_| InputChannelRegistry::default()))
    }

    #[gpui::test]
    fn join_orders_members_and_moving_channel_re_joins(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let (first, first_session, _) = session(cx);
        let (second, second_session, _) = session(cx);

        registry.update(cx, |registry, cx| {
            registry.join(first, InputChannel::A, first_session.clone(), cx);
            registry.join(second, InputChannel::A, second_session, cx);
            // Re-joining the same channel keeps the position.
            registry.join(first, InputChannel::A, first_session.clone(), cx);
        });
        registry.read_with(cx, |registry, _| {
            let ids: Vec<_> = registry
                .members(InputChannel::A)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            assert_eq!(ids, vec![first, second], "members follow the join order");
            assert_eq!(registry.channel_of(first), Some(InputChannel::A));
        });

        registry.update(cx, |registry, cx| {
            registry.join(first, InputChannel::B, first_session, cx);
        });
        registry.read_with(cx, |registry, _| {
            assert_eq!(registry.channel_of(first), Some(InputChannel::B));
            assert_eq!(
                registry
                    .members(InputChannel::A)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>(),
                vec![second],
                "the other members of A are untouched"
            );
            assert_eq!(
                registry.channels_in(&[second, first]),
                vec![InputChannel::A, InputChannel::B],
                "chips are ordered A..E, not by join order"
            );
        });
    }

    #[gpui::test]
    fn leave_and_close_remove_members(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let (first, first_session, _) = session(cx);
        let (second, second_session, _) = session(cx);
        let (outsider, outsider_session, _) = session(cx);

        registry.update(cx, |registry, cx| {
            registry.join(first, InputChannel::A, first_session, cx);
            registry.join(second, InputChannel::A, second_session, cx);
            registry.join(outsider, InputChannel::B, outsider_session, cx);

            assert!(registry.leave(first, cx));
            assert!(!registry.leave(first, cx), "leaving twice is a no-op");
            assert_eq!(registry.close(InputChannel::A, cx), 1);
            assert_eq!(registry.close(InputChannel::A, cx), 0);
            assert_eq!(registry.channel_of(outsider), Some(InputChannel::B));
        });
    }

    #[gpui::test]
    fn fan_out_reaches_the_peers_of_the_origin_only(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let (origin, origin_session, origin_probe) = session(cx);
        let (peer, peer_session, peer_probe) = session(cx);
        let (outsider, outsider_session, outsider_probe) = session(cx);

        registry.update(cx, |registry, cx| {
            registry.join(origin, InputChannel::A, origin_session, cx);
            registry.join(peer, InputChannel::A, peer_session, cx);
            registry.join(outsider, InputChannel::B, outsider_session, cx);
            registry.fan_out(origin, BroadcastInput::Bytes(b"ls"), cx);
            registry.fan_out(origin, BroadcastInput::Text("a"), cx);
            registry.fan_out(origin, BroadcastInput::Paste("b"), cx);
            registry.fan_out(origin, BroadcastInput::Interrupt, cx);
        });

        assert_eq!(
            peer_probe.writes(),
            vec![
                b"ls".to_vec(),
                b"a".to_vec(),
                b"b".to_vec(),
                b"\x03".to_vec()
            ]
        );
        assert!(origin_probe.writes().is_empty(), "the origin is skipped");
        assert!(
            outsider_probe.writes().is_empty(),
            "another channel is untouched"
        );
    }

    #[gpui::test]
    fn fan_out_from_a_non_member_writes_nothing(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let (member, member_session, member_probe) = session(cx);
        let (outsider, _, _) = session(cx);

        registry.update(cx, |registry, cx| {
            registry.join(member, InputChannel::A, member_session, cx);
            registry.fan_out(outsider, BroadcastInput::Bytes(b"x"), cx);
            // The only member of its channel has no peers either.
            registry.fan_out(member, BroadcastInput::Bytes(b"x"), cx);
        });
        assert!(member_probe.writes().is_empty());
    }

    #[gpui::test]
    fn a_failing_peer_does_not_stop_the_fan_out(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let (origin, origin_session, _) = session(cx);
        let (dead, dead_session, dead_probe) = session(cx);
        let (alive, alive_session, alive_probe) = session(cx);
        dead_probe.fail_writes(true);

        registry.update(cx, |registry, cx| {
            registry.join(origin, InputChannel::A, origin_session, cx);
            registry.join(dead, InputChannel::A, dead_session, cx);
            registry.join(alive, InputChannel::A, alive_session, cx);
            registry.fan_out(origin, BroadcastInput::Bytes(b"ls"), cx);
        });
        assert!(dead_probe.writes().is_empty());
        assert_eq!(
            alive_probe.writes(),
            vec![b"ls".to_vec()],
            "a peer after the failing one still receives the input"
        );
    }
}
