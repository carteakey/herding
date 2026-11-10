//! Closing a herdr-core tab ends every terminal its live layout holds, not
//! only those its last saved layout listed.

use super::commands::{id, window_with_split};
use crate::{
    browser::Store,
    core_terminals::{Host, panes::Axis},
};

#[gpui::test]
fn a_tab_closed_before_its_split_is_saved_ends_the_new_pane(cx: &mut gpui::TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["a"]);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let shown = view.core_terminals.tabs.get_mut(&tab).unwrap();
            shown.panes.split(&id("a"), Axis::Row, id("b")).unwrap();
            shown.unsaved = true;
        });
        Store::update(cx, |store| store.close(tab));
        view.update(cx, |view, cx| view.sync_core_views(cx));
        assert!(!view.read(cx).core_terminals.tabs.contains_key(&tab));
        let ending = Store::update(cx, Store::take_ending);
        assert_eq!(ending, vec![(Host::Local, id("a")), (Host::Local, id("b"))]);
    });
}
