//! A phone's split or zoom of a Herdr 2 tab, made by the window holding it.

use crate::{
    browser::TabId,
    core_terminals::{
        CoreTerminals, Host, Purpose, TabPanes,
        panes::{Axis, Panes},
        phone::Change,
    },
};
use sheprd_backend::{Spawn, TerminalId};
use sheprd_remote_control::control::{
    changes::AskId,
    layout::{Answer, Ask, Failure, Op},
};
use std::time::Instant;

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

/// Tab 1 drives t1 and t2; tab 2 mirrors t3.
fn terminals() -> CoreTerminals {
    let mut terminals = CoreTerminals::default();
    let mut panes = Panes::single(id("t1"));
    panes.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    terminals
        .tabs
        .insert(TabId::test(1), TabPanes::new(Host::Local, panes, false));
    terminals.tabs.insert(
        TabId::test(2),
        TabPanes::new(Host::Local, Panes::single(id("t3")), true),
    );
    terminals
}

fn ask(tab: u64, terminal: &str, op: Op) -> Ask {
    Ask {
        id: AskId(1),
        tab,
        terminal: terminal.to_owned(),
        op,
    }
}

#[test]
fn a_split_starts_a_shell_beside_the_phones_pane() {
    let mut terminals = terminals();
    let Some(Change::Split(Purpose::Split { tab, at, axis })) =
        terminals.phone_change(&ask(1, "t2", Op::SplitDown))
    else {
        panic!("a split");
    };
    assert_eq!((tab, at, axis), (TabId::test(1), id("t2"), Axis::Column));
}

#[test]
fn a_zoom_fills_the_tab_with_the_phones_pane_and_a_second_shows_the_split() {
    let mut terminals = terminals();
    assert!(matches!(
        terminals.phone_change(&ask(1, "t2", Op::Zoom)),
        Some(Change::Zoomed(tab)) if tab == TabId::test(1)
    ));
    let shown = &terminals.tabs[&TabId::test(1)];
    assert!(shown.zoom.is_open());
    assert_eq!(shown.active, id("t2"), "the pane zoomed is the one in use");
    terminals.phone_change(&ask(1, "t2", Op::Zoom));
    assert!(!terminals.tabs[&TabId::test(1)].zoom.is_open());
}

#[test]
fn closing_names_the_tab_and_the_terminals_left_in_it() {
    let mut terminals = terminals();
    let left = terminals.tabs[&TabId::test(1)]
        .panes
        .terminals()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    assert!(!left.is_empty());
    // A split that landed after the daemon ended the tab's terminals is
    // still in the tab: it ends with it rather than outliving it.
    assert!(matches!(
        terminals.phone_change(&ask(1, "t1", Op::Close)),
        Some(Change::Close(tab, ending)) if tab == TabId::test(1) && ending == left
    ));
}

#[test]
fn a_split_is_answered_once_its_pane_is_in_the_tab() {
    let mut terminals = terminals();
    let now = Instant::now();
    assert!(matches!(
        terminals.phone_change(&ask(1, "t1", Op::SplitRight)),
        Some(Change::Split(_))
    ));
    // The window starts the shell: until it lands, nothing is answered,
    // and a second split waits its turn.
    terminals.wanted = std::collections::VecDeque::from([(
        Purpose::Tab {
            scope: crate::browser::Scope::endpoint("mac"),
            workspace: "w".into(),
        },
        Host::Local,
        Spawn::default(),
        None,
    )]);
    assert!(terminals.settled_phone_split(now).is_none());
    assert!(matches!(
        terminals.phone_change(&ask(1, "t1", Op::SplitDown)),
        Some(Change::Answered(Answer::Failed(Failure::Busy)))
    ));
    terminals
        .tabs
        .get_mut(&TabId::test(1))
        .unwrap()
        .panes
        .split(&id("t1"), Axis::Row, id("t9"))
        .unwrap();
    // The phone is told the new shell, to show it next.
    assert_eq!(
        terminals.settled_phone_split(now),
        Some((AskId(1), Answer::Split("t9".into())))
    );
    assert!(terminals.settled_phone_split(now).is_none(), "once");
}

#[test]
fn a_split_that_never_lands_is_refused() {
    let mut terminals = terminals();
    let now = Instant::now();
    terminals.phone_change(&ask(1, "t1", Op::SplitRight));
    // Nothing starting and no new pane: the start failed.
    assert_eq!(
        terminals.settled_phone_split(now),
        Some((AskId(1), Answer::Failed(Failure::Refused)))
    );
    terminals.phone_change(&ask(1, "t1", Op::SplitRight));
    terminals.wanted = std::collections::VecDeque::from([(
        Purpose::Tab {
            scope: crate::browser::Scope::endpoint("mac"),
            workspace: "w".into(),
        },
        Host::Local,
        Spawn::default(),
        None,
    )]);
    assert!(terminals.settled_phone_split(now).is_none());
    assert_eq!(
        terminals.settled_phone_split(now + std::time::Duration::from_secs(60)),
        Some((AskId(1), Answer::Failed(Failure::Refused))),
        "a start that hangs gives up"
    );
}

#[test]
fn a_mirror_never_splits_and_a_pane_elsewhere_is_gone() {
    let mut terminals = terminals();
    assert!(matches!(
        terminals.phone_change(&ask(2, "t3", Op::SplitRight)),
        Some(Change::Answered(Answer::Failed(Failure::Watching)))
    ));
    assert!(matches!(
        terminals.phone_change(&ask(1, "t3", Op::SplitRight)),
        Some(Change::Answered(Answer::Failed(Failure::Gone)))
    ));
    // Another window may hold a tab this one does not.
    assert!(terminals.phone_change(&ask(9, "t1", Op::Zoom)).is_none());
}

#[test]
fn a_tab_closes_even_after_its_panes_ended() {
    let mut terminals = terminals();
    // The daemon ended the terminals first; the pane it names may be gone.
    assert!(matches!(
        terminals.phone_change(&ask(1, "t404", Op::Close)),
        Some(Change::Close(tab, _)) if tab == TabId::test(1)
    ));
}

#[test]
fn a_tab_on_another_host_is_not_the_phones() {
    let mut terminals = terminals();
    let remote = Host::Remote(crate::browser::Scope::endpoint("box"));
    terminals.tabs.insert(
        TabId::test(3),
        TabPanes::new(remote, Panes::single(id("t1")), false),
    );
    assert!(terminals.phone_change(&ask(3, "t1", Op::Close)).is_none());
}

#[test]
fn a_rename_names_the_tab_or_the_pane_as_its_menu_does() {
    use crate::core_terminals::menu::Target;
    let mut terminals = terminals();
    let tab = TabId::test(1);
    assert!(matches!(
        terminals.phone_change(&ask(1, "t2", Op::RenameTab(Some("  build\n".into())))),
        Some(Change::Renamed(Target::Tab(named), Some(name))) if named == tab && name == "build"
    ));
    assert!(matches!(
        terminals.phone_change(&ask(1, "t2", Op::RenamePane(None))),
        Some(Change::Renamed(Target::Pane { tab: named, terminal }, None))
            if named == tab && terminal == id("t2")
    ));
    // A blank name gives the title back, as clearing it in the menu does.
    assert!(matches!(
        terminals.phone_change(&ask(1, "t1", Op::RenameTab(Some("   ".into())))),
        Some(Change::Renamed(Target::Tab(_), None))
    ));
    // A pane the tab no longer has is gone.
    assert!(matches!(
        terminals.phone_change(&ask(1, "t9", Op::RenamePane(Some("x".into())))),
        Some(Change::Answered(Answer::Failed(Failure::Gone)))
    ));
}
