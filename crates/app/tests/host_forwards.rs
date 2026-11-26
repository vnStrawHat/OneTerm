//! A host's forwards and its way through another host (specs/oneterm, "M8 in detail"): the rules
//! of the editor's Forwards tab, each said in a sentence, saved in ssh's words and carried by
//! the launch. The sessions are `echo()`: no host is reached.

use quadra::Role;

#[path = "support/hosts_app.rs"]
mod support;
use support::{App, app};

/// The fields of a name, top to bottom.
fn fields(a: &App, name: &str) -> Vec<quadra::NodeId> {
    a.h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == name)
}

#[test]
fn a_host_reached_through_another_carries_its_way_and_forwards() {
    let mut a = app();
    a.add("bastion", "", "ops@203.0.113.5");
    a.h.get_by_role(Role::Button, "Add SSH host").click();
    a.h.settle();
    for (id, value) in [("host-name", "db"), ("host-target", "trung@10.0.0.9")] {
        a.h.get_by_test_id(id).set_value(value);
    }
    a.h.get_by_test_id("host-jump").click();
    a.h.settle();
    a.h.get_by_role(Role::MenuItem, "bastion").click();
    a.h.settle();
    // A forward is a row of its tab.
    a.h.get_by_role(Role::Tab, "Forwards").click();
    a.h.settle();
    a.h.get_by_role(Role::Button, "Add forward").click();
    a.h.settle();
    a.h.get_by_role(Role::TextInput, "Listen port").set_value("5432");
    a.h.get_by_role(Role::TextInput, "Destination").set_value("localhost:5432");
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
fn a_rule_says_what_it_does_and_one_that_is_no_forward_is_not_saved() {
    let mut a = app();
    a.h.get_by_role(Role::Button, "Add SSH host").click();
    a.h.settle();
    a.h.get_by_test_id("host-target").set_value("ops@10.0.0.9");
    a.h.get_by_role(Role::Tab, "Forwards").click();
    a.h.settle();
    assert!(a.h.has_text("No forwards."));
    for _ in 0..3 {
        a.h.get_by_role(Role::Button, "Add forward").click();
        a.h.settle();
    }
    assert!(a.h.has_text("Give the port to listen on, 1 to 65535."));
    let set = |a: &mut App, name: &str, at: usize, value: &str| {
        let field = fields(a, name)[at];
        assert!(a.h.tree.act(field, &quadra_tree::SemanticAction::SetValue(value.into())));
        a.h.settle();
    };
    // Local, the type a new row has; its sentence follows what is typed.
    set(&mut a, "Listen port", 0, "8080");
    assert!(a.h.has_text("Give the destination as host:port, such as localhost:80."));
    set(&mut a, "Destination", 0, "localhost:80");
    assert!(a.h.has_text("Port 8080 on this computer opens localhost:80 as seen from the host."));
    // The type is a select of the three.
    let types = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Type");
    assert_eq!(types.len(), 3);
    for (row, kind) in [(1, "Remote"), (2, "Dynamic")] {
        assert!(a.h.tree.act(types[row], &quadra_tree::SemanticAction::Click));
        a.h.settle();
        a.h.get_by_role(Role::MenuItem, kind).click();
        a.h.settle();
    }
    set(&mut a, "Listen port", 1, "9000");
    set(&mut a, "Destination", 1, "localhost:3000");
    assert!(a.h.has_text("Port 9000 on the host opens localhost:3000 as seen from this computer."));
    set(&mut a, "Listen port", 2, "1080");
    assert!(a.h.has_text("Port 1080 on this computer is a SOCKS proxy through the host."));
    // A row that is no forward keeps the dialog open, on Forwards.
    a.h.get_by_role(Role::Button, "Add forward").click();
    a.h.settle();
    set(&mut a, "Listen port", 3, "70000");
    a.h.get_by_role(Role::Button, "Add host").click();
    a.h.settle();
    assert!(a.rows().is_empty() && a.h.has_text("Not a forward: 70000:"), "{}", a.h.semantics());
    // Removed, the rest is saved in ssh's words.
    let removes = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Remove forward");
    assert!(a.h.tree.act(removes[3], &quadra_tree::SemanticAction::Click));
    a.h.settle();
    a.h.get_by_role(Role::Button, "Add host").click();
    a.h.settle();
    assert_eq!(a.rows()[0][6], "8080:localhost:80 R9000:localhost:3000 D1080");
    // Edited, the rules are the rows again.
    a.edit("10.0.0.9");
    a.h.get_by_role(Role::Tab, "Forwards").click();
    a.h.settle();
    assert_eq!(fields(&a, "Listen port").len(), 3);
    assert!(a.h.has_text("Port 1080 on this computer is a SOCKS proxy through the host."));
}
