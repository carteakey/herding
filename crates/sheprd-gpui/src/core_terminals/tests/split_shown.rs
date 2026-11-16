//! A split asked for in a herdr-core tab the active group does not show,
//! as an editor opened from a review is, brings that tab forward.

use crate::{
    browser::{Location, Store, TabId},
    core_terminals::{
        Host, WORKSPACE,
        panes::{Axis, Panes},
    },
    sidebar::layout_tests::{fixture_window, full_draw},
};
use gpui::TestAppContext;
use sheprd_backend::TerminalId;

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

#[gpui::test]
fn a_split_in_a_hidden_tab_shows_that_tab(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = None;
        view.live.missing_installation = true;
        view
    });
    let tabs: Vec<TabId> = view.update(cx, |view, cx| {
        let scope = crate::browser::scope(&view.endpoints[0]);
        ["t1", "t2"]
            .into_iter()
            .map(|name| {
                let location = Location::core_terminal(Panes::single(id(name)), false);
                let tab = Store::update(cx, |store| {
                    store.open(scope.clone(), WORKSPACE, Some(location), None)
                })
                .unwrap();
                view.core_terminals
                    .show_for_test(tab, Panes::single(id(name)), false, Vec::new());
                tab
            })
            .collect()
    });
    let shown = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| view.read(cx).shown_core_pane(cx).map(|(tab, _)| tab))
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.show_browser_tab(tabs[0], window, cx));
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(shown(cx), Some(tabs[0]));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.place_split(
                tabs[1],
                &id("t2"),
                Axis::Row,
                (Host::Local, id("t3")),
                window,
                cx,
            )
        });
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(shown(cx), Some(tabs[1]));
}
