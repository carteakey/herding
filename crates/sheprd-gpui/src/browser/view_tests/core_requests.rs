use super::*;
use crate::{
    control::{Opening, Placed, Placement, Target},
    core_terminals::panes::Panes,
};
use sheprd_backend::TerminalId;

/// Opens a request's tab from herdr-core `terminal`, with no Herdr pane or
/// workspace named, without switching to it.
fn request(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    terminal: &str,
) -> Option<Placed> {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let target = Target {
                daemon: None,
                workspace: None,
                pane: None,
                terminal: Some(terminal),
            };
            view.open_requested_browser_tab(
                &target,
                false,
                &Opening {
                    location: &url("http://localhost:3000/"),
                    focus: false,
                    placement: Placement::Tab,
                },
                window,
                cx,
            )
        })
    })
}

#[gpui::test]
fn a_core_terminal_caller_opens_in_the_workspace_of_its_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        let scope = scope(&view.endpoints[0]);
        let t1 = TerminalId::new("t1").unwrap();
        let location = Location::core_terminal(Panes::single(t1.clone()), false);
        let tab = Store::update(cx, |store| {
            store.open(scope.clone(), "w1", Some(location), None)
        })
        .unwrap();
        view.core_terminals
            .show_for_test(tab, Panes::single(t1), false, Vec::new());
        view.core_terminals.place_for_test(tab, scope, "w1");
    });
    assert!(matches!(
        request(&view, cx, "t1"),
        Some(Placed::Opened { workspace_id, .. }) if workspace_id == "w1"
    ));
    // The page remembers the terminal, so its notes go back there.
    let origin = view.read_with(cx, |_, cx| {
        let store = cx.global::<Store>();
        let opened = store
            .tabs()
            .find(|tab| matches!(&tab.location, Some(Location::Web { .. })))
            .unwrap();
        opened.origin.clone()
    });
    assert_eq!(origin.as_deref(), Some("core:t1"));
    // A terminal no tab of this window holds is another window's, or none.
    assert!(request(&view, cx, "t9").is_none());
    assert!(request(&view, cx, "bad\nid").is_none());
}
