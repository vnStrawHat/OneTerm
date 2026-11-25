//! Input channels (specs/oneterm, "Spaces"): five, A to E. What is typed in a space of a channel
//! goes to the other members, in any tab; the space's menu joins and leaves, its header shows
//! a chip of the channel's colour.

use quadra::{Align, AnyView, Justify, Role, Signal, View, div, dynamic, row, text};
use quadra_design::{MenuItem, token};
use quadra_term::Session;

use crate::model::{Space, Tab};

const CHANNELS: u8 = 5;

fn letter(channel: u8) -> char {
    char::from(b'A' + channel)
}

/// The colour of a channel: the kit's series, which stay apart in either theme.
pub(crate) fn colour(channel: u8) -> quadra::Token {
    token::SERIES[usize::from(channel)]
}

/// What the space's user sends goes to the others of its channel; the space leaves its
/// channel once its program ended (a key would close it). Called in the space's scope.
pub(crate) fn follow(tabs: Signal<Vec<Tab>>, id: u64, channel: Signal<Option<u8>>, session: &Session) {
    session.on_input(move |input| {
        // A key on a space whose program ended closes it, before it is told.
        let Some(mine) = quadra::untrack(|| channel.try_get()).flatten() else { return };
        for peer in members(tabs, mine) {
            if peer.id != id {
                peer.session.deliver(input);
            }
        }
    });
    let exited = session.exited();
    quadra::effect(move || {
        if exited.get().is_some() {
            channel.set(None);
        }
    });
}

/// The spaces of a channel, in every tab.
fn members(tabs: Signal<Vec<Tab>>, channel: u8) -> Vec<Space> {
    let all = tabs
        .with_untracked(|tabs| tabs.iter().flat_map(|tab| tab.spaces.get_untracked()).collect::<Vec<_>>());
    all.into_iter().filter(|it| it.channel.get_untracked() == Some(channel)).collect()
}

/// The space's Input channel: the five to join (its own checked), Leave and Close while it is
/// a member, and in a tab of several spaces, the tab's all joined to its channel or all out.
/// Disabled once its program ended.
pub(crate) fn menu(tabs: Signal<Vec<Tab>>, space: &Space) -> MenuItem {
    let (me, mine) = (space.channel, space.channel.get_untracked());
    let tab = tabs.with_untracked(|tabs| {
        tabs.iter().find(|tab| tab.spaces.with_untracked(|it| it.contains(space))).copied()
    });
    let siblings = tab.map(|tab| tab.spaces.get_untracked()).unwrap_or_default();
    let mut items: Vec<MenuItem> = (0..CHANNELS)
        .map(|c| {
            let item = MenuItem::new(format!("Channel {}", letter(c))).dot(colour(c));
            item.checked(mine == Some(c)).on_select(move || me.set(Some(c)))
        })
        .collect();
    if let Some(c) = mine {
        items.push(MenuItem::separator());
        items.push(MenuItem::new("Leave channel").on_select(move || me.set(None)));
        items.push(MenuItem::new(format!("Close channel {}", letter(c))).on_select(move || {
            members(tabs, c).iter().for_each(|it| it.channel.set(None));
        }));
    }
    let any = siblings.iter().any(|it| it.channel.get_untracked().is_some());
    if siblings.len() > 1 && (mine.is_some() || any) {
        items.push(MenuItem::separator());
    }
    if let Some(c) = mine.filter(|_| siblings.len() > 1) {
        let all = siblings.clone();
        items.push(MenuItem::new(format!("Join all spaces in tab to {}", letter(c))).on_select(move || {
            all.iter()
                .filter(|it| it.edit.is_none() && it.session.exited().get_untracked().is_none())
                .for_each(|it| it.channel.set(Some(c)));
        }));
    }
    if siblings.len() > 1 && any {
        items.push(MenuItem::new("Leave with all spaces in tab").on_select(move || {
            siblings.iter().for_each(|it| it.channel.set(None));
        }));
    }
    let ended = space.session.exited().get_untracked().is_some();
    MenuItem::new("Input channel").disabled(ended).submenu(items)
}

/// A member's chip in its header: the channel's letter on its colour.
pub(crate) fn chip(channel: Signal<Option<u8>>) -> impl View {
    dynamic(move || match channel.try_get().flatten() {
        None => AnyView::new(div()),
        Some(c) => AnyView::new(
            row()
                .size(14.0, 14.0)
                .radius(token::RADIUS_SM)
                .shrink(0.0)
                .align_items(Align::Center)
                .justify(Justify::Center)
                .bg(colour(c))
                .text_color(token::TEXT_ON_STATUS)
                .text_size(10.0)
                .font_weight(600)
                .role(Role::Image)
                .label(format!("Channel {}", letter(c)))
                .child(text(letter(c).to_string())),
        ),
    })
}
