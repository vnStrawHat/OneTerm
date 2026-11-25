//! Finding in a space (specs/oneterm, "Find"): the find bar at the top right of a space, over its
//! terminal, opened by the action `Find` (`Ctrl+F`): the query and its switches, where the
//! current match is of how many, the next and the one before, and Close.

use std::cell::Cell;
use std::rc::Rc;

use quadra::{
    Action, Align, Div, Justify, Key, KeyEvent, Length, Mods, MountCx, NodeId, NodeRef, Signal, View, column,
    div, effect, row, show, signal, text,
};
use quadra_design::{icon_button, icons, input, size, token};
use quadra_term::{FindQuery, Session};

/// Opens the find bar of the space the focus is in, or puts the focus back in its query.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Find;

impl Action for Find {
    const NAME: &'static str = "oneterm::Find";
}

/// What the bar shows and sets.
#[derive(Clone, Copy)]
struct Bar {
    open: Signal<bool>,
    query: Signal<String>,
    case: Signal<bool>,
    word: Signal<bool>,
    pattern: Signal<bool>,
}

/// `inner` (the space's terminal) with the find bar over it while it is open.
pub(crate) fn around<V: View + 'static>(session: &Session, inner: V) -> Around<V> {
    Around { session: session.clone(), inner }
}

pub(crate) struct Around<V> {
    session: Session,
    inner: V,
}

impl<V: View + 'static> View for Around<V> {
    fn mount(self, cx: &mut MountCx) -> NodeId {
        let Around { session, inner } = self;
        // Built again (a tab shown again), the bar is as the session's finding left it.
        let was = session.find_query();
        let b = Bar {
            open: signal(!was.text.is_empty()),
            query: signal(was.text),
            case: signal(was.case),
            word: signal(was.word),
            pattern: signal(was.pattern),
        };
        // What the bar says is looked for, while it is open; closed, nothing.
        let (finds, finding) = (session.clone(), Cell::new(false));
        effect(move || {
            let open = b.open.get();
            let (case, word, pattern) = (b.case.get(), b.word.get(), b.pattern.get());
            let query = FindQuery { text: b.query.get(), case, word, pattern };
            if open || finding.replace(false) {
                finding.set(open);
                finds.find(&if open { query } else { FindQuery::default() });
            }
        });
        let (me, at, handle) = (NodeRef::new(), NodeRef::new(), cx.handle.clone());
        // The focus in the query once the bar is there; back to the terminal once it closed.
        let (to_bar, to_term) = (handle.clone(), handle);
        let (bar_ref, me_ref) = (at.clone(), me.clone());
        let focus_bar = move || {
            let bar = bar_ref.clone();
            to_bar.with(move |tree| {
                tree.after_layout(move |tree| {
                    if let Some(bar) = bar.get() {
                        tree.focus_first(bar);
                    }
                })
            });
        };
        let close = Rc::new(move || {
            b.open.set(false);
            if let Some(me) = me_ref.get() {
                to_term.with(move |tree| _ = tree.focus_first(me));
            }
        });
        let shown = move || bar(b, &session, close.clone(), at.clone());
        let at_right = row().absolute().top(6.0).right(16.0).left(16.0).justify(Justify::End);
        let over = show(move || b.open.get(), shown).container(at_right);
        column()
            .grow(1.0)
            .min_height(0.0)
            .node_ref(&me)
            .on_action::<Find>(move |_| {
                b.open.set(true);
                focus_bar();
            })
            .child(inner)
            .child(over)
            .mount(cx)
    }
}

/// The bar: the query, Match case, Whole word and Regular expression, the count, a rule, Previous, Next, Close.
fn bar(b: Bar, s: &Session, close: Rc<dyn Fn()>, at: NodeRef) -> Div {
    let step = |back: bool| {
        let s = s.clone();
        move || s.find_next(back)
    };
    let switch = |body: &'static str, name: &'static str, on: Signal<bool>| {
        icon_button(body, name).toggled(on).on_click(move || on.set(!on.get_untracked()))
    };
    let found = s.found();
    let none = move || !b.query.with(String::is_empty) && found.get().1 == 0;
    let said = move || match (b.query.with(String::is_empty), found.get()) {
        (true, _) => String::new(),
        (false, (_, 0)) => "No results".to_string(),
        (false, (i, n)) => format!("{i} of {n}"),
    };
    let tone = move || if none() { token::TEXT_3 } else { token::TEXT_2 };
    let count =
        div().width(64.0).shrink(0.0).text_size(size::TEXT_SM.size).text_color(tone).child(text(said));
    let next = step(false);
    // 220 wide, less in a narrow space: the bar stays inside it.
    let query = input(b.query).fill().icon(icons::SEARCH);
    let query = query.placeholder("Find").label("Find").invalid(none).on_submit(move |_| next());
    let query = row().width(224.0).min_width(Length::Px(96.0)).padding_right(token::SPACE_XS).child(query);
    let rule = div().width(1.0).height(16.0).shrink(0.0).bg(token::LINE);
    let closes = close.clone();
    let line = row()
        .align_items(Align::Center)
        .gap(token::SPACE_XXS)
        .min_width(Length::Px(0.0))
        .child(query)
        .child(switch(icons::CASE_SENSITIVE, "Match case", b.case))
        .child(switch(icons::WHOLE_WORD, "Whole word", b.word))
        .child(switch(icons::REGEX, "Regular expression", b.pattern))
        .child(div().padding_x(token::SPACE_SM).child(count))
        .child(rule)
        .child(div().width(4.0))
        .child(icon_button(icons::ARROW_UP, "Previous match").disabled(none).on_click(step(true)))
        .child(icon_button(icons::ARROW_DOWN, "Next match").disabled(none).on_click(step(false)))
        .child(icon_button(icons::X, "Close").on_click(move || closes()));
    let (next, back) = (step(false), step(true));
    let keys = move |e: &mut KeyEvent| match e.key {
        Key::Escape => {
            close();
            e.stop();
        }
        // Enter in the query is the next match (its submit); with Shift, the one before.
        Key::F3 | Key::Enter if e.key == Key::F3 || e.mods.contains(Mods::SHIFT) => {
            if e.mods.contains(Mods::SHIFT) {
                back()
            } else {
                next()
            }
            e.stop();
        }
        _ => {}
    };
    let plane = column()
        .min_width(Length::Px(0.0))
        .padding(token::SPACE_XS)
        .bg(token::OVERLAY_BG)
        .border(1.0)
        .border_color(token::LINE);
    plane.radius(token::RADIUS_MD).elevation(1).node_ref(&at).on_key_down(keys).child(line)
}
