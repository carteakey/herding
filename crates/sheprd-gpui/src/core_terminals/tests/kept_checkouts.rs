//! A kept workspace whose folder is a Git checkout carries it as Herdr 1's
//! workspaces do, so the readers on `Session` treat it alike.

use crate::{
    SheprdWindow,
    browser::Location,
    core_terminals::panes::Panes,
    sidebar::layout_tests::{SidebarFixture, fixture_window},
};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};
use sheprd_backend::TerminalId;
use sheprd_vcs::git::Identity;
use std::path::Path;

// Pull requests are only looked up for an absolute checkout, which a path
// without a drive is not on Windows.
#[cfg(not(windows))]
const FOLDER: &str = "/src/app-feature";
#[cfg(not(windows))]
const KEY: &str = "/src/app/.git";
#[cfg(windows)]
const FOLDER: &str = r"C:\src\app-feature";
#[cfg(windows)]
const KEY: &str = r"C:\src\app\.git";

fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext, String) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.snapshot = None;
            view.live.missing_installation = true;
            view.core_terminals.checkouts.seed(
                FOLDER,
                Some(Identity {
                    branch: Some("feature".into()),
                    key: KEY.into(),
                    label: "app".into(),
                    linked: true,
                }),
            );
            let location =
                Location::core_terminal(Panes::single(TerminalId::new("t1").unwrap()), false);
            view.open_kept_workspace(None, Path::new(FOLDER), location, window, cx)
        })
    });
    (view, cx, id.unwrap())
}

#[gpui::test]
fn a_kept_checkout_reads_like_herdr_1s(cx: &mut TestAppContext) {
    let (view, cx, id) = window(cx);
    view.read_with(cx, |view, _| {
        let sessions = view.sessions(0);
        let workspace = sessions.workspace(&id).unwrap();
        assert_eq!(workspace.branch.as_deref(), Some("feature"));
        let tree = workspace.worktree.as_ref().unwrap();
        assert_eq!((tree.key.as_str(), tree.linked), (KEY, true));
        assert!(!workspace.custom_label, "named after its folder");
        assert!(crate::worktree_identity::front_is_linked(
            &sessions,
            Some(&id)
        ));
        let inputs = view.kept_checkout_inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].1.repo_key.as_deref(), Some(KEY));
        assert_eq!(inputs[0].1.branch, "feature");
        assert_eq!(
            view.git_input(),
            Some(inputs[0].1.clone()),
            "it is in front"
        );
    });
}

#[gpui::test]
fn renaming_a_kept_workspace_marks_its_name_as_chosen(cx: &mut TestAppContext) {
    let (view, cx, id) = window(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.rename_kept_workspace(&id, "api", cx)));
    view.read_with(cx, |view, _| {
        let sessions = view.sessions(0);
        let workspace = sessions.workspace(&id).unwrap();
        assert!(workspace.custom_label);
        assert_eq!(crate::sidebar::workspace_label(workspace, true), "api");
    });
}

#[gpui::test]
fn pull_requests_of_kept_checkouts_are_read_without_herdr_1(cx: &mut TestAppContext) {
    let (view, cx, _) = window(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.pr_boot().as_deref(), Some(""), "no daemon to answer");
    });
}

#[gpui::test]
fn a_name_given_at_creation_is_kept_as_chosen(cx: &mut TestAppContext) {
    let (view, cx, _) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.new_kept_workspace(Some("tools"), Some("/src/app-tools".into()), cx);
            view.new_kept_workspace(Some("app-web"), Some("/src/app-web".into()), cx);
        })
    });
    view.read_with(cx, |view, _| {
        let sessions = view.sessions(0);
        let custom = |folder: &str| {
            let id = view.kept_workspace_at(Path::new(folder)).unwrap();
            sessions.workspace(&id).unwrap().custom_label
        };
        assert!(custom("/src/app-tools"), "the user's own name");
        assert!(!custom("/src/app-web"), "only its folder's name");
    });
}

#[gpui::test]
fn mods_follow_the_tab_shown_in_a_kept_workspace(cx: &mut TestAppContext) {
    let (view, cx, id) = window(cx);
    // As each window tick does once the tab is open.
    cx.update(|_, cx| view.update(cx, |view, cx| view.refresh_core_session(cx)));
    view.read_with(cx, |view, cx| {
        let scope = crate::browser::scope(&view.endpoints[0]);
        let tab = cx
            .global::<crate::browser::Store>()
            .in_workspace(&scope, &id)
            .next()
            .unwrap()
            .id
            .number()
            .to_string();
        let state = view.mods_state();
        assert_eq!(state.focus.unwrap().tab.as_deref(), Some(tab.as_str()));
        let workspace = state.workspaces.iter().find(|w| w.id == id).unwrap();
        assert!(workspace.focused);
        let focused: Vec<_> = workspace
            .tabs
            .iter()
            .filter(|t| t.focused)
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(focused, [tab.as_str()]);
    });
}
