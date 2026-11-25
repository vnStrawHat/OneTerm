//! Saved hosts (specs/oneterm, "M8 in detail"): added in the dialog, listed in the rail by group,
//! filtered, connected to on a click (recent, counted on the tab, named in the status bar),
//! saved from quick connect. The sessions are `echo()`: no host is reached.

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{Launch, NewSession, OneTerm, keymap};
use quadra::{Key, Mods, Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

struct App {
    h: Harness,
    made: Rc<RefCell<Vec<Launch>>>,
    settings: Rc<Settings>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 800.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, settings) = (Rc::new(RefCell::new(Vec::new())), Settings::memory());
    let (kept, given) = (made.clone(), settings.clone());
    let sessions: NewSession = Rc::new(move |launch| {
        kept.borrow_mut().push(launch);
        echo()
    });
    h.mount(move || {
        provide(given.clone());
        let app = OneTerm::new(sessions);
        // No key is known: the Connect dialog asks to trust it (never the user's own file).
        app.model.ui().opening.known_hosts.set(".tmp/hosts-tests/known_hosts".into());
        app.view()
    });
    h.settle();
    App { h, made, settings }
}

impl App {
    /// Clicks a host's row with the pointer, `times` in a row.
    fn click_host(&mut self, name: &str, times: usize) {
        let row = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(row).center();
        for _ in 0..times {
            self.h.click(at.x, at.y);
        }
        self.h.settle();
    }

    /// The Connect dialog's Connect.
    fn connect(&mut self) {
        self.h.get_by_role(Role::Button, "Connect").click();
        self.h.settle();
    }

    fn add(&mut self, name: &str, group: &str, target: &str) {
        self.h.get_by_role(Role::Button, "Add SSH host").click();
        self.h.settle();
        for (id, value) in [("host-name", name), ("host-group", group), ("host-target", target)] {
            self.h.get_by_test_id(id).set_value(value);
        }
        self.h.settle();
        self.h.get_by_role(Role::Button, "Add host").click();
        self.h.settle();
    }

    fn rows(&self) -> Vec<Vec<String>> {
        self.settings.value("ssh.hosts", Vec::<Vec<String>>::new()).get()
    }
}

#[test]
fn a_host_added_is_saved_listed_and_connects_from_its_dialog() {
    let mut a = app();
    // A local space has no network in the status bar.
    assert!(!a.h.has_text("local shell"));
    assert!(a.h.query(|n| n.semantics.name == "Network").is_empty(), "no network for a local space");
    a.add("prod-api-01", "Production", "trung@10.20.1.14");
    a.add("", "Staging", "trung@10.30.2.8:2222");
    assert_eq!(a.rows()[0], ["prod-api-01", "Production", "trung@10.20.1.14", "Auto", "", "", ""]);
    assert_eq!(a.rows()[1][..3], ["10.30.2.8", "Staging", "trung@10.30.2.8:2222"]);
    assert!(a.h.has_text("Production") && a.h.has_text("Staging"));

    let tabs = a.made.borrow().len();
    // A click opens its Connect dialog; its Connect connects.
    a.click_host("prod-api-01", 1);
    assert!(a.h.has_text("Connect to prod-api-01"), "{}", a.h.semantics());
    assert_eq!(a.made.borrow().len(), tabs, "nothing before Connect");
    a.connect();
    assert!(!a.h.has_text("Connect to prod-api-01"), "the dialog closes");
    let launch = a.made.borrow().last().cloned().unwrap();
    assert_eq!(launch.ssh.map(|it| it.target.label()), Some("trung@10.20.1.14".into()));
    assert!(a.h.has_text("1 ssh \u{B7} 1 space"), "the tab counts its kinds");
    assert!(a.h.has_text("prod-api-01"), "the status bar names the host");
    let recent = a.settings.value("ssh.recent", Vec::<Vec<String>>::new()).get();
    assert_eq!(recent[0][0], "trung@10.20.1.14");
}

#[test]
fn a_host_reached_through_another_carries_its_way_and_forwards() {
    let mut a = app();
    a.add("bastion", "", "ops@203.0.113.5");
    a.h.get_by_role(Role::Button, "Add SSH host").click();
    a.h.settle();
    for (id, value) in
        [("host-name", "db"), ("host-target", "trung@10.0.0.9"), ("host-forwards", "5432:localhost:5432")]
    {
        a.h.get_by_test_id(id).set_value(value);
    }
    a.h.get_by_test_id("host-jump").click();
    a.h.settle();
    a.h.get_by_role(Role::MenuItem, "bastion").click();
    a.h.settle();
    a.h.get_by_role(Role::Button, "Add host").click();
    a.h.settle();
    assert_eq!(a.rows()[1][5..], ["bastion", "5432:localhost:5432"]);

    a.click_host("db", 2);
    a.connect();
    let remote = a.made.borrow().last().cloned().unwrap().ssh.unwrap();
    let hops: Vec<String> = remote.jumps.iter().map(|it| it.target.label()).collect();
    assert_eq!(hops, ["ops@203.0.113.5"]);
    assert_eq!(remote.forwards[0].label(), "5432:localhost:5432");
}

#[test]
fn the_new_menu_searches_hosts_and_connects_to_a_typed_target() {
    let mut a = app();
    a.add("prod-api-01", "Production", "trung@10.20.1.14");
    a.add("home-nas", "Personal", "admin@nas.local");
    let open = |a: &mut App| {
        a.h.get_by_role(Role::Button, "New terminal").click();
        a.h.settle();
        let field = a.h.get_by_role(Role::TextInput, "Search hosts, or type user@host…").node();
        assert_eq!(a.h.tree.focused(), Some(field), "the focus is in the field");
    };
    open(&mut a);
    assert!(
        a.h.query(|n| n.semantics.role == Role::MenuItem && n.semantics.name == "Local shell").len() == 1
    );
    a.h.type_text("NAS");
    a.h.settle();
    let items = a.h.query(|n| n.semantics.role == Role::MenuItem);
    let nas = a.h.query(|n| n.semantics.role == Role::MenuItem && n.semantics.name == "home-nas");
    assert_eq!((items.len(), nas.len()), (2, 1), "{}", a.h.semantics());
    assert_eq!(items[0], nas[0], "the host found comes before connecting to the word");
    a.h.key(Key::Escape, Mods::NONE);
    a.h.settle();

    open(&mut a);
    a.h.type_text("ops@10.9.8.7:2200");
    a.h.settle();
    a.h.key(Key::Enter, Mods::NONE);
    a.h.settle();
    let launch = a.made.borrow().last().cloned().unwrap();
    assert_eq!(launch.ssh.map(|it| it.target.label()), Some("ops@10.9.8.7:2200".into()));
}

#[test]
fn a_host_is_edited_in_its_dialog_and_a_new_name_takes_its_place() {
    let mut a = app();
    a.add("db", "Staging", "trung@10.0.0.9");
    let node = a.h.get_by_role(Role::Button, "db").node();
    let at = a.h.tree.window_rect(node).center();
    for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
        a.h.tree.pointer(input, quadra::MouseButton::Right, at, Mods::NONE, a.h.now());
    }
    a.h.settle();
    a.h.get_by_role(Role::MenuItem, "Edit\u{2026}").click();
    a.h.settle();
    assert!(a.h.has_text("Edit SSH host"));
    let tree = a.h.semantics();
    assert!(
        tree.contains("= \"10.0.0.9\" #host-target") && tree.contains("= \"trung\" #host-user"),
        "{tree}"
    );
    a.h.get_by_test_id("host-name").set_value("db-main");
    a.h.get_by_test_id("host-target").set_value("trung@10.0.0.10");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Save").click();
    a.h.settle();
    assert_eq!(a.rows(), [["db-main", "Staging", "trung@10.0.0.10", "Auto", "", "", ""]]);
    assert!(!a.h.has_text("Edit SSH host"), "the dialog closed");
}

#[test]
fn the_filter_keeps_the_hosts_that_match() {
    let mut a = app();
    a.add("prod-api-01", "Production", "trung@10.20.1.14");
    a.add("home-nas", "Personal", "admin@nas.local");
    a.h.get_by_test_id("host-filter").set_value("NAS");
    a.h.settle();
    assert!(a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "prod-api-01").is_empty());
    assert_eq!(a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "home-nas").len(), 1);
}

#[test]
fn quick_connect_can_save_the_host() {
    let mut a = app();
    a.h.key(Key::K, Mods::CTRL | Mods::SHIFT);
    a.h.settle();
    a.h.get_by_role(Role::Checkbox, "Save as host").click();
    a.h.get_by_test_id("connect-target").set_value("ops@10.20.0.2");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Connect").click();
    a.h.settle();
    assert_eq!(a.rows()[0][..3], ["10.20.0.2", "", "ops@10.20.0.2"]);
}

#[test]
fn a_host_has_a_colour_tag_its_group_s_until_one_is_chosen_and_kept() {
    let mut a = app();
    a.add("prod-api-01", "Production", "trung@10.20.1.14");
    let tags = |a: &App| {
        a.h.query(|n| n.semantics.role == Role::Image && n.semantics.name.starts_with("Colour tag"))
    };
    assert_eq!(tags(&a).len(), 1, "a tag by its group");
    a.h.get_by_role(Role::Button, "Add SSH host").click();
    a.h.settle();
    for (id, value) in
        [("host-name", "prod-lb"), ("host-group", "Production"), ("host-target", "ops@10.20.0.2")]
    {
        a.h.get_by_test_id(id).set_value(value);
    }
    // The group typed, the picker is made again with its colour for the default.
    a.h.settle();
    a.h.get_by_test_id("host-tag").click();
    a.h.settle();
    a.h.get_by_role(Role::Button, "Purple").click();
    a.h.settle();
    a.h.get_by_role(Role::Button, "Add host").click();
    a.h.settle();
    assert_eq!(a.rows()[1][7], "purple");
    assert!(a.h.has_text("Colour tag: purple"));
    // Edited, the host keeps its tag.
    let node = a.h.get_by_role(Role::Button, "prod-lb").node();
    let at = a.h.tree.window_rect(node).center();
    for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
        a.h.tree.pointer(input, quadra::MouseButton::Right, at, Mods::NONE, a.h.now());
    }
    a.h.settle();
    a.h.get_by_role(Role::MenuItem, "Edit\u{2026}").click();
    a.h.settle();
    a.h.get_by_test_id("host-target").set_value("ops@10.20.0.3");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Save").click();
    a.h.settle();
    assert_eq!(a.rows()[1][2..], ["ops@10.20.0.3", "Auto", "", "", "", "purple"]);
}

#[test]
fn a_colour_tag_is_any_colour_and_a_tag_saved_by_name_still_reads() {
    let mut a = app();
    let row = |name: &str, target: &str, tag: &str| {
        [name, "", target, "Auto", "", "", "", tag].map(String::from).to_vec()
    };
    let rows = vec![row("old", "u@10.0.0.1", "yellow"), row("odd", "u@10.0.0.2", "mauve")];
    a.settings.value("ssh.hosts", Vec::<Vec<String>>::new()).set(rows);
    a.h.settle();
    assert!(a.h.has_text("Colour tag: yellow"));
    assert!(!a.h.has_text("Colour tag: mauve"), "a word that is no colour is the group's");
    a.h.get_by_role(Role::Button, "Add SSH host").click();
    a.h.settle();
    a.h.get_by_test_id("host-target").set_value("ops@10.20.0.2");
    a.h.get_by_test_id("host-tag").click();
    a.h.settle();
    a.h.get_by_role(Role::TextInput, "Hex").type_text("#336699");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Add host").click();
    a.h.settle();
    assert_eq!(a.rows()[2][7], "#336699");
    assert!(a.h.has_text("Colour tag: #336699"));
}
