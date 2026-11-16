//! Names given to a Herdr 2 tab and its panes are saved with the tab, so
//! every window showing it takes a rename made in another.

use super::commands::{id, window_with_split};
use crate::{
    browser::{Location, Store},
    core_terminals::menu::{Target, clean_name},
};
use gpui::TestAppContext;

#[gpui::test]
fn a_rename_saved_by_another_window_is_taken_and_kept(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["t1", "t2"]);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_core_name(&Target::Tab(tab), clean_name("build"), cx);
            view.save_core_panes(cx);
        })
    });
    // Another window renames the tab and a pane, and saves it.
    cx.update(|_, cx| {
        let mut location = cx.global::<Store>().get(tab).unwrap().location.clone();
        let Some(Location::CoreTerminal { name, names, .. }) = &mut location else {
            panic!("a Herdr 2 tab");
        };
        *name = Some("deploy".into());
        names.insert(id("t2"), "logs".into());
        Store::update(cx, |store| store.visited(tab, location, None));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.sync_core_views(cx);
            let shown = &view.core_terminals.tabs[&tab];
            assert_eq!(shown.name.as_deref(), Some("deploy"));
            assert_eq!(view.core_pane_label(tab, &id("t2"), cx).0, "logs");
            // This window's next save keeps them rather than undoing them.
            view.core_terminals.tabs.get_mut(&tab).unwrap().unsaved = true;
            view.save_core_panes(cx);
        })
    });
    let saved = cx.update(|_, cx| cx.global::<Store>().get(tab).unwrap().location.clone());
    let Some(Location::CoreTerminal { name, names, .. }) = saved else {
        panic!("a Herdr 2 tab");
    };
    assert_eq!(name.as_deref(), Some("deploy"));
    assert_eq!(names.get(&id("t2")).map(String::as_str), Some("logs"));
}
