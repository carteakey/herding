use super::*;
use crate::{
    browser::{Location, Store},
    core_terminals::panes::Panes,
};
use gpui::WindowHandle;
use sheprd_backend::TerminalId;

/// Gives `handle`'s window a herdr-core tab in w3 holding `terminal`.
fn hold(handle: WindowHandle<SheprdWindow>, terminal: &str, cx: &mut TestAppContext) {
    handle
        .update(cx, |view, _, cx| {
            let scope = crate::browser::scope(&view.endpoints[0]);
            let id = TerminalId::new(terminal).unwrap();
            let location = Location::core_terminal(Panes::single(id.clone()), false);
            let tab = Store::update(cx, |store| {
                store.open(scope.clone(), "w3", Some(location), None)
            })
            .unwrap();
            view.core_terminals
                .show_for_test(tab, Panes::single(id), false, Vec::new());
            view.core_terminals.place_for_test(tab, scope, "w3");
        })
        .unwrap();
}

#[gpui::test]
fn a_core_terminal_in_a_shared_workspace_reaches_the_window_holding_it(cx: &mut TestAppContext) {
    let first = cx.add_window(fixture_window);
    let second = cx.add_window(fixture_window);
    hold(first, "t1", cx);
    hold(second, "t2", cx);
    cx.update(|cx| {
        // Both windows show w3; only the frontmost would serve the
        // workspace alone, whichever holds the caller's terminal.
        for (terminal, holder) in [("t1", first), ("t2", second)] {
            let caller = Caller {
                terminal_id: Some(terminal.into()),
                ..caller(Some("w3"))
            };
            let handle = window_for(&caller, cx).unwrap();
            assert_eq!(handle.window_id(), holder.window_id(), "{terminal}");
        }
    });
}
