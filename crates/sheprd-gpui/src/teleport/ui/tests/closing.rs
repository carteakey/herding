use super::*;
use crate::browser::TabId;
use core::prelude::v1::test;
use sheprd_backend::TerminalId;
use sheprd_session::panes::{Axis, Panes};

fn panes(id: &str) -> Panes {
    Panes::single(TerminalId::new(id).unwrap())
}

/// The workspace's Herdr 2 tabs close once the move stopped their
/// terminals, and only then: a move that failed earlier leaves them working.
#[test]
fn herdr_2_tabs_close_once_their_terminals_stopped() {
    let tabs = vec![(TabId::test(1), panes("t1")), (TabId::test(2), panes("t2"))];
    let unchanged = |tab: TabId| {
        tabs.iter()
            .find(|(id, _)| *id == tab)
            .map(|(_, p)| p.clone())
    };
    let ids = [TabId::test(1), TabId::test(2)];
    let mut early = Teleport::start(source(), "feature".into(), Vec::new()).closing(tabs.clone());
    assert!(early.closed_tabs(false, unchanged).is_empty());
    assert_eq!(early.closed_tabs(true, unchanged), ids);
    // Taken once.
    assert!(early.closed_tabs(true, unchanged).is_empty());

    let mut late = Teleport::start(source(), "feature".into(), Vec::new()).closing(tabs.clone());
    late.retired = true;
    assert_eq!(late.closed_tabs(false, unchanged), ids);
}

/// A tab split during the move holds a terminal that did not move, so it
/// stays open with it, and one already closed is left alone. A tab the
/// window shrank as its moved panes ended still closes.
#[test]
fn only_tabs_holding_moved_terminals_alone_close() {
    let id = |name: &str| TerminalId::new(name).unwrap();
    let mut both = panes("t1");
    both.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    let tabs = vec![
        (TabId::test(1), panes("t1")),
        (TabId::test(2), both),
        (TabId::test(3), panes("t3")),
    ];
    let mut teleport = Teleport::start(source(), "feature".into(), Vec::new()).closing(tabs);
    let now = |tab: TabId| match tab {
        // Split during the move.
        t if t == TabId::test(1) => {
            let mut split = panes("t1");
            split.split(&id("t1"), Axis::Row, id("t5")).unwrap();
            Some(split)
        }
        // `t1` ended and the window dropped its pane.
        t if t == TabId::test(2) => Some(panes("t2")),
        // Closed by the user.
        _ => None,
    };
    assert_eq!(teleport.closed_tabs(true, now), [TabId::test(2)]);
}
