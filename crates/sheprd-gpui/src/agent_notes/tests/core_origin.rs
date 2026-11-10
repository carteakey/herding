#![allow(clippy::unwrap_used)]
//! Notes from a page a herdr-core terminal opened, in a window with no
//! Herdr session to vouch for its origin.

use crate::{
    agent_notes::Delivered,
    browser::{FeedbackKey, Location, Store},
    core_terminals::{WORKSPACE, panes::Panes},
    sidebar::layout_tests::fixture_window,
};
use gpui::TestAppContext;
use sheprd_backend::TerminalId;

#[gpui::test]
fn notes_for_a_core_terminal_wait_for_it_without_herdr(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = None;
        view.live.missing_installation = true;
        view
    });
    let (delivered, gone, key) = view.update(cx, |view, cx| {
        let scope = crate::browser::scope(&view.endpoints[0]);
        let t1 = TerminalId::new("t1").unwrap();
        let location = Location::core_terminal(Panes::single(t1.clone()), false);
        let tab = Store::update(cx, |store| {
            store.open(scope.clone(), WORKSPACE, Some(location), None)
        })
        .unwrap();
        view.core_terminals
            .show_for_test(tab, Panes::single(t1), false, Vec::new());
        let delivered = view.deliver_from(None, Some("core:t1".into()), "Note".into(), None, cx);
        // A terminal no tab holds any more is not vouched for.
        let gone = view.deliver_from(None, Some("core:t9".into()), "Note".into(), None, cx);
        let key = FeedbackKey {
            scope,
            pane_id: "core:t1".into(),
        };
        (delivered, gone, key)
    });
    assert!(matches!(delivered, Delivered::Kept | Delivered::Agent));
    assert!(view.read_with(cx, |_, cx| {
        cx.global::<crate::browser::Feedback>().has(&key)
    }));
    assert!(matches!(gone, Delivered::Copy(_)));
}

#[gpui::test]
fn notes_for_a_core_terminal_are_kept_under_its_tabs_daemon(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let key = view.update(cx, |view, cx| {
        // The terminal's tab is in a workspace of another local daemon than
        // the one the window shows.
        let other = crate::browser::Scope::local(std::path::Path::new("/run/b/herdr-client.sock"));
        let t1 = TerminalId::new("t1").unwrap();
        let location = Location::core_terminal(Panes::single(t1.clone()), false);
        let tab = Store::update(cx, |store| {
            store.open(other.clone(), "kept-1", Some(location), None)
        })
        .unwrap();
        view.core_terminals
            .show_for_test(tab, Panes::single(t1), false, Vec::new());
        view.core_terminals
            .place_for_test(tab, other.clone(), "kept-1");
        let delivered = view.deliver_from(None, Some("core:t1".into()), "Note".into(), None, cx);
        assert!(!matches!(delivered, Delivered::Copy(_)));
        FeedbackKey {
            scope: other,
            pane_id: "core:t1".into(),
        }
    });
    // Where `feedback --wait` from that terminal looks for them.
    assert!(view.read_with(cx, |_, cx| {
        cx.global::<crate::browser::Feedback>().has(&key)
    }));
}
