//! The shown herdr-core pane's branch is read with the kept workspaces'
//! checkouts, in one batch, for the Git and pull request chips.

use crate::{
    browser::{Location, Store, TabId},
    core_terminals::{WORKSPACE, panes::Panes},
    sidebar::layout_tests::fixture_window,
};
use gpui::TestAppContext;
use sheprd_backend::{RunState, TerminalId, TerminalInfo};
use sheprd_client::protocol::AgentStatus;
use sheprd_vcs::git::Identity;
use std::path::PathBuf;

#[cfg(not(windows))]
const FOLDER: &str = "/src/app-feature";
#[cfg(windows)]
const FOLDER: &str = r"C:\src\app-feature";

fn info(name: &str) -> TerminalInfo {
    TerminalInfo {
        agent: AgentStatus::Unknown,
        cwd: Some(PathBuf::from(FOLDER)),
        foreground: None,
        id: TerminalId::new(name).unwrap(),
        labels: Default::default(),
        name: None,
        reporter: None,
        state: RunState::Running,
        title: None,
    }
}

#[gpui::test]
fn the_shown_panes_branch_comes_from_the_shared_checkout_reader(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = None;
        view.live.missing_installation = true;
        view
    });
    let tab: TabId = view.update(cx, |view, cx| {
        let scope = crate::browser::scope(&view.endpoints[0]);
        let id = TerminalId::new("t1").unwrap();
        let location = Location::core_terminal(Panes::single(id.clone()), false);
        let tab = Store::update(cx, |store| {
            store.open(scope, WORKSPACE, Some(location), None)
        })
        .unwrap();
        view.core_terminals
            .show_for_test(tab, Panes::single(id), false, vec![info("t1")]);
        view.core_terminals.checkouts.seed(
            FOLDER,
            Some(Identity {
                branch: Some("feature".into()),
                key: "/src/app/.git".into(),
                label: "app".into(),
                linked: true,
            }),
        );
        tab
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_browser_tab(tab, window, cx);
            view.core_terminals.shown = view.shown_core_pane(cx);
            // Its folder joins the batch, so the answer is kept.
            view.refresh_core_session(cx);
            let input = view.core_git_input().unwrap();
            assert_eq!(input.branch, "feature");
            assert_eq!(input.checkout.as_deref(), Some(FOLDER));
            // Once no pane is shown, the folder leaves the batch.
            view.core_terminals.shown = None;
            view.refresh_core_session(cx);
            assert!(view.core_terminals.checkouts.get(FOLDER).is_none());
        });
    });
}
