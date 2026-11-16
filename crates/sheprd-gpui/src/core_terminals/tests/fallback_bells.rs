//! The shell standing in for Herdr 1 rings like a tab: only while the
//! workspace it belongs to is the one shown.

use crate::{
    SheprdWindow,
    core_terminals::{Error, Hub, WORKSPACE},
    sidebar::layout_tests::{SidebarFixture, fixture_window},
};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};

/// A window on a computer with no Herdr 1 installed, whose core is never
/// found, so no test reaches or starts the user's own.
fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.core_terminals.hub = Hub::with_join(|_| Err(Error::NotBundled));
            view.live.snapshot = None;
            view.live.missing_installation = true;
            view.refresh_core_session(cx);
        })
    });
    (view, cx)
}

#[gpui::test]
fn the_stand_in_shell_is_in_sight_only_with_its_workspace(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser_key().unwrap().1, WORKSPACE);
        assert!(view.default_workspace_shown());
    });
    // Another workspace in front hides the shell, which stays attached.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.new_kept_workspace(Some("api"), None, cx)
        })
    });
    view.read_with(cx, |view, _| {
        assert_ne!(view.browser_key().unwrap().1, WORKSPACE);
        assert!(!view.default_workspace_shown());
    });
}
