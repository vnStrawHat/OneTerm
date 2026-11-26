//! A host's forwards in its editor (specs/oneterm, "M8 in detail"): a row a rule, its type, the
//! port listened on and where connections go, and under it a sentence of what it does; what is
//! saved is ssh's word for each (`8080:localhost:80`, `R9000:localhost:3000`, `D1080`).

use oneterm_ssh::{Forward, Way};
use quadra::{Div, Signal, Track, column, div, dynamic, signal, text};
use quadra_design::{Variant, button, hstack, icon, icon_button, icons, input, label, select, size, token};

use crate::locked::shown;

/// The types: Local opens a port here, Remote on the host, Dynamic a SOCKS proxy here.
const WAYS: [&str; 3] = ["Local", "Remote", "Dynamic"];
/// The way each one's connections go, said beside its name in the menu.
const GOES: [&str; 3] = ["this computer \u{2192} host", "host \u{2192} this computer", "SOCKS proxy"];
const DYNAMIC: usize = 2;

/// A rule as its row holds it: the type's place in `WAYS`, and what is typed.
#[derive(Clone, Copy)]
pub(super) struct Rule {
    way: Signal<usize>,
    listen: Signal<String>,
    to: Signal<String>,
}

impl Rule {
    fn new(way: usize, listen: String, to: String) -> Rule {
        Rule { way: signal(way), listen: signal(listen), to: signal(to) }
    }

    /// The row of a saved forward.
    pub(super) fn of(forward: &Forward) -> Rule {
        let to = match (forward.way, forward.host.contains(':')) {
            (Way::Dynamic, _) => String::new(),
            (_, true) => format!("[{}]:{}", forward.host, forward.port),
            (_, false) => format!("{}:{}", forward.host, forward.port),
        };
        let way = match forward.way {
            Way::Local => 0,
            Way::Remote => 1,
            Way::Dynamic => DYNAMIC,
        };
        Rule::new(way, forward.listen.to_string(), to)
    }

    /// The rule in ssh's word, as it is now (read untracked); `None` for a row left empty.
    fn word(&self) -> Option<String> {
        let (listen, to) = (self.listen.get_untracked(), self.to.get_untracked());
        let (listen, to, way) = (listen.trim(), to.trim(), self.way.get_untracked());
        match way {
            _ if listen.is_empty() && (to.is_empty() || way == DYNAMIC) => None,
            DYNAMIC => Some(format!("D{listen}")),
            1 => Some(format!("R{listen}:{to}")),
            _ => Some(format!("{listen}:{to}")),
        }
    }

    /// What the rule does, or what it still lacks: it follows what is typed.
    fn says(&self) -> String {
        let (listen, to) = (self.listen.get(), self.to.get());
        let (listen, to) = (listen.trim(), to.trim());
        if !listen.parse::<u16>().is_ok_and(|port| port > 0) {
            return "Give the port to listen on, 1 to 65535.".into();
        }
        match self.way.get() {
            DYNAMIC => format!("Port {listen} on this computer is a SOCKS proxy through the host."),
            _ if Forward::parse(&format!("{listen}:{to}")).is_none() => {
                "Give the destination as host:port, such as localhost:80.".into()
            }
            1 => format!("Port {listen} on the host opens {to} as seen from this computer."),
            _ => format!("Port {listen} on this computer opens {to} as seen from the host."),
        }
    }
}

/// The forwards the rows say, or the word of the first row that is none.
pub(super) fn read(rules: Signal<Vec<Rule>>) -> Result<Vec<Forward>, String> {
    let words = rules.with_untracked(|rules| rules.iter().filter_map(Rule::word).collect::<Vec<_>>());
    words.into_iter().map(|word| Forward::parse(&word).ok_or(word)).collect()
}

/// The columns of a row: the type, the port, the arrow, the destination (the room left), Remove.
fn columns() -> Div {
    let tracks = [Track::px(104.0), Track::px(72.0), Track::px(14.0), Track::fr(1.0), Track::px(28.0)];
    div().grid(tracks).gap_x(token::SPACE_MD).align_items(quadra::Align::Center)
}

/// The rules, each with its sentence, and Add forward under them.
pub(super) fn list(rules: Signal<Vec<Rule>>) -> quadra_design::Form {
    let rows = dynamic(move || {
        let all = rules.get();
        if all.is_empty() {
            return quadra_design::form().child(label("No forwards."));
        }
        let heads = ["Type", "Listen port", "", "Destination", ""].map(label);
        let mut list = quadra_design::form().child(columns().children(heads));
        for (at, rule) in all.into_iter().enumerate() {
            let remove = icon_button(icons::TRASH_2, "Remove forward")
                .on_click(move || rules.update(|rules| _ = rules.remove(at)));
            let goes = move || rule.way.get() != DYNAMIC;
            let field = |value, hint: &str, name: &'static str| input(value).placeholder(hint).label(name);
            let line = columns()
                .child(select(WAYS, rule.way).details(GOES).label("Type"))
                .child(field(rule.listen, "8080", "Listen port").mono())
                // A Dynamic rule keeps the two places, empty: Remove stays at the row's end.
                .child(
                    column().child(
                        div()
                            .display(shown(goes))
                            .child(icon(icons::ARROW_RIGHT).size(14.0).color(token::TEXT_2)),
                    ),
                )
                .child(column().child(
                    column().display(shown(goes)).child(field(rule.to, "localhost:80", "Destination").mono()),
                ))
                .child(remove);
            let says = div()
                .text_size(size::TEXT_SM.size)
                .text_color(token::TEXT_2)
                .child(text(move || rule.says()));
            list = list.child(column().gap(token::SPACE_XS).child(line).child(says));
        }
        list
    });
    let add = button("Add forward")
        .icon(icons::PLUS)
        .variant(Variant::Secondary)
        .on_click(move || rules.update(|rules| rules.push(Rule::new(0, String::new(), String::new()))));
    // The region in a column, so that what it holds is as wide as the form.
    quadra_design::form().child(rows.container(column())).child(hstack().child(add))
}
