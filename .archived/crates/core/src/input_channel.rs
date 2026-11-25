//! [`InputChannel`] — the five broadcast input channels a terminal Space can
//! join (`docs/spec-intakes/IN-0022-broadcast-input-channels/`).
//!
//! The channel is a leaf value so `oneterm-actions` — which depends on
//! `oneterm-core` only — can carry it in its join action, the same placement
//! [`crate::ShellKind`] has for `AddPanelWithShell`. Membership itself lives in
//! `oneterm_state::InputChannelRegistry`.

use serde::Deserialize;

/// One of the five fixed broadcast input channels.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Deserialize)]
pub enum InputChannel {
    A,
    B,
    C,
    D,
    E,
}

impl InputChannel {
    /// Every channel, in display order.
    pub const ALL: [InputChannel; 5] = [Self::A, Self::B, Self::C, Self::D, Self::E];

    /// The single letter shown in the menu, the tab chips and the Space frame.
    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
        }
    }

    /// Position in [`Self::ALL`] — selects the theme's `chart_1..chart_5`.
    pub fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
            Self::C => 2,
            Self::D => 3,
            Self::E => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_and_index_follow_the_declaration_order() {
        for (position, channel) in InputChannel::ALL.into_iter().enumerate() {
            assert_eq!(channel.index(), position);
            assert_eq!(channel.label(), ["A", "B", "C", "D", "E"][position]);
        }
        // Ord is what `channels_in` sorts the tab chips by.
        assert!(InputChannel::A < InputChannel::E);
        // The action derive deserializes the variant name.
        assert_eq!(
            serde_json::from_str::<InputChannel>("\"C\"").expect("variant name"),
            InputChannel::C
        );
    }
}
