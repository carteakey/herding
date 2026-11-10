use super::*;
use crate::core_terminals::{
    Host,
    panes::{Axis, Panes},
};
use sheprd_backend::TerminalId;

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

/// `name` in the core of the host the tests' tabs are on.
fn queued(name: &str) -> Ending {
    (Host::of(&Scope::endpoint("local")), id(name))
}

fn open(store: &mut Store, panes: Panes, observe: bool) -> TabId {
    store
        .open(
            Scope::endpoint("local"),
            "w",
            Some(Location::core_terminal(panes, observe)),
            None,
        )
        .unwrap()
}

#[test]
fn closing_a_terminal_tab_ends_its_terminals_and_cannot_be_reopened() {
    let mut store = Store::default();
    let mut panes = Panes::single(id("t1"));
    panes.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    let tab = open(&mut store, panes, false);
    assert!(!store.has_ending());
    store.close(tab).unwrap();
    assert_eq!(store.take_ending(), [queued("t1"), queued("t2")]);
    assert!(!store.has_ending(), "taken once");
    // An ended terminal has nothing to come back to.
    assert!(store.peek_closed(&Scope::endpoint("local"), "w").is_none());
}

#[test]
fn closing_a_mirror_only_stops_watching() {
    let mut store = Store::default();
    let typing = open(&mut store, Panes::single(id("t1")), false);
    let mirror = open(&mut store, Panes::single(id("t1")), true);
    store.close(mirror).unwrap();
    assert!(!store.has_ending());
    assert!(store.get(typing).is_some());
}

#[test]
fn a_terminal_another_tab_types_into_is_not_ended() {
    let mut store = Store::default();
    let first = open(&mut store, Panes::single(id("t1")), false);
    open(&mut store, Panes::single(id("t1")), false);
    store.close(first).unwrap();
    assert!(store.has_ending());
    assert!(store.take_ending().is_empty());
}

#[test]
fn a_closed_workspace_ends_its_terminal_tabs() {
    let mut store = Store::default();
    open(&mut store, Panes::single(id("t1")), false);
    open(&mut store, Panes::single(id("t1")), true);
    let other = store
        .open(
            Scope::endpoint("local"),
            "w2",
            Some(Location::core_terminal(Panes::single(id("t2")), false)),
            None,
        )
        .unwrap();
    assert!(store.forget_workspaces(&Scope::endpoint("local"), &["w".to_owned()]));
    assert_eq!(store.take_ending(), [queued("t1")]);
    assert!(store.get(other).is_some());
}

#[test]
fn a_terminal_of_the_same_name_on_another_host_does_not_keep_it() {
    let mut store = Store::default();
    let first = open(&mut store, Panes::single(id("t1")), false);
    // Terminal IDs are only unique within one core: this `t1` is another.
    store
        .open(
            Scope::endpoint("other"),
            "w",
            Some(Location::core_terminal(Panes::single(id("t1")), false)),
            None,
        )
        .unwrap();
    store.close(first).unwrap();
    store.end_terminals([queued("t1")]);
    assert_eq!(store.take_ending(), [queued("t1")], "each once");
}
