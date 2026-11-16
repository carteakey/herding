use super::*;
use crate::core_terminals::panes::{Axis, Panes};
use sheprd_backend::TerminalId;

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

/// A workspace's Herdr 2 tabs move with it; a mirror, a page, and another
/// workspace's tab stay.
#[gpui::test]
fn the_workspaces_herdr_2_tabs_move_with_it(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let scope = Scope::endpoint("laptop");
        let mut split = Panes::single(id("t1"));
        split.split(&id("t1"), Axis::Row, id("t2")).unwrap();
        let core = |panes: &Panes, observe| Location::core_terminal(panes.clone(), observe);
        let (moving, _) = Store::update(cx, |store| {
            let moving = store
                .open(scope.clone(), "w4", Some(core(&split, false)), None)
                .unwrap();
            store.open(scope.clone(), "w4", Some(core(&split, true)), None);
            store.open(scope.clone(), "w4", None, None);
            let elsewhere = Panes::single(id("t3"));
            let other = store.open(scope.clone(), "w5", Some(core(&elsewhere, false)), None);
            (moving, other)
        });
        assert_eq!(core_tabs(cx, &scope, "w4"), [(moving, split)]);
        assert!(core_tabs(cx, &Scope::endpoint("other"), "w4").is_empty());
    });
}
