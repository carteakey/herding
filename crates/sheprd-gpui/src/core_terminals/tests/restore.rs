//! A herdr-core tab comes back as it was left: the pane in use and the
//! zoomed pane are saved with its layout.

use crate::{
    browser::Location,
    core_terminals::{
        Host, TabPanes,
        panes::{Axis, Panes},
    },
};
use sheprd_backend::TerminalId;

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

fn split() -> Panes {
    let mut panes = Panes::single(id("t1"));
    panes.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    panes
}

#[test]
fn the_pane_in_use_and_the_zoom_save_and_come_back() {
    let mut shown = TabPanes::new(Host::Local, split(), false);
    shown.active = id("t2");
    shown.set_zoom(Some(id("t2")));
    let location = shown.location();
    let saved = serde_json::to_string(&location).unwrap();
    let read: Location = serde_json::from_str(&saved).unwrap();
    assert_eq!(read, location);
    let restored = TabPanes::restore(Host::Local, &read).unwrap();
    assert_eq!(restored.active, id("t2"));
    assert_eq!(restored.zoomed, Some(id("t2")));
    // Already zoomed as the window opens, not growing.
    let now = crate::motion::now();
    assert!(restored.zoom.is_open() && !restored.zoom.moving(now));
    assert_eq!(restored.zoom_at(now).map(|(pane, _)| pane), Some(&id("t2")));
}

#[test]
fn an_unzoomed_tab_saves_no_zoom() {
    let mut shown = TabPanes::new(Host::Local, split(), false);
    shown.set_zoom(Some(id("t1")));
    shown.set_zoom(None);
    let Location::CoreTerminal { zoomed, .. } = shown.location() else {
        panic!("a terminal tab");
    };
    assert_eq!(zoomed, None);
}

fn saved(panes: Panes, active: Option<&str>, zoomed: Option<&str>) -> Location {
    let Location::CoreTerminal {
        panes,
        observe,
        name,
        names,
        ..
    } = Location::core_terminal(panes, false)
    else {
        unreachable!();
    };
    Location::CoreTerminal {
        panes,
        observe,
        active: active.map(id),
        zoomed: zoomed.map(id),
        name,
        names,
    }
}

#[test]
fn panes_the_layout_no_longer_holds_are_ignored() {
    let restored = TabPanes::restore(Host::Local, &saved(split(), Some("t9"), Some("t9"))).unwrap();
    assert_eq!(restored.active, id("t1"));
    assert!(!restored.zoom.is_open());
    // One pane has nothing to zoom over.
    let single = TabPanes::restore(
        Host::Local,
        &saved(Panes::single(id("t1")), None, Some("t1")),
    )
    .unwrap();
    assert!(!single.zoom.is_open());
}

#[test]
fn names_save_and_come_back_for_panes_still_there() {
    let mut shown = TabPanes::new(Host::Local, split(), false);
    shown.name = Some("build".into());
    shown.names.insert(id("t2"), "tests".into());
    shown.names.insert(id("t9"), "gone".into());
    let location = shown.location();
    let read: Location = serde_json::from_str(&serde_json::to_string(&location).unwrap()).unwrap();
    let restored = TabPanes::restore(Host::Local, &read).unwrap();
    assert_eq!(restored.name.as_deref(), Some("build"));
    assert_eq!(restored.names.len(), 1);
    assert_eq!(restored.names[&id("t2")], "tests");
}

#[test]
fn a_tab_saved_without_them_starts_on_its_first_pane() {
    let saved = r#"{"kind":"core_terminal","panes":{"axis":"row","first":"t1","second":"t2"}}"#;
    let restored = TabPanes::restore(Host::Local, &serde_json::from_str(saved).unwrap()).unwrap();
    assert!(restored.name.is_none() && restored.names.is_empty());
    assert_eq!(restored.active, id("t1"));
    assert!(!restored.zoom.is_open());
}
