#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::{
    control::protocol::Caller, endpoint::Endpoint, sidebar::layout_tests::fixture_window,
    state::ConnectionStatus,
};
use herdr_client::{ConnectTarget, protocol::ClientShellSnapshot};
use std::sync::Arc;

fn caller(workspace: &str, pane: &str) -> Caller {
    Caller {
        pane_id: Some(pane.to_owned()),
        workspace_id: Some(workspace.to_owned()),
        ..Caller::default()
    }
}

fn snapshot(
    boot_id: &str,
    workspace: &str,
    pane: &str,
    agent: Option<&str>,
) -> ClientShellSnapshot {
    let agents = match agent {
        Some(agent) => vec![serde_json::json!({
            "pane_id": pane, "workspace_id": workspace, "tab_id": "t0",
            "display_agent": agent, "agent": agent, "agent_status": "working",
            "state_change_seq": 1, "state_labels": [], "tokens": [], "focused": true
        })],
        None => Vec::new(),
    };
    serde_json::from_value(serde_json::json!({
        "boot_id": boot_id,
        "revision": 1,
        "update_install_command": "",
        "latest_release_notes_available": false,
        "integration_updates_available": false,
        "worktree_directory": "",
        "tab_bar_right": [],
        "tab_bar_right_separator": "",
        "agent_order": [],
        "workspaces": [{
            "workspace_id": workspace, "active_tab_id": "t0", "new_workspace_cwd": "",
            "number": 1, "label": workspace, "custom_label": false,
            "tokens": [], "focused": true, "agent_status": "working"
        }],
        "tabs": [{
            "tab_id": "t0", "workspace_id": workspace, "number": 1, "label": "tab",
            "custom_label": false, "zoomed": false, "focused": true,
            "agent_status": "working"
        }],
        "panes": [{
            "pane_id": pane, "workspace_id": workspace, "tab_id": "t0",
            "focused": true, "right_click_passthrough": false
        }],
        "agents": agents,
        "commands": []
    }))
    .expect("test snapshot is valid")
}

/// Adds a remote host and selects it, leaving the fixture's local daemon at
/// endpoint 0. `selected` is the projection the window shows for the remote.
fn show_remote_host(view: &mut HerdrWindow, selected: LiveState) {
    let mut remote = Endpoint::new(
        "m2".into(),
        "m2".into(),
        ConnectTarget::Socket("/unused-team-test.sock".into()),
        true,
    );
    remote.never_connect();
    view.endpoints.push(remote);
    view.selected_endpoint = 1;
    view.live = selected;
}

/// Runs `act` on a fixture window and fails the test if the window closes.
fn with_window(cx: &mut gpui::TestAppContext, act: impl FnOnce(&mut HerdrWindow)) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, _| act(view))
        .expect("the test window stays open");
}

#[gpui::test]
fn proves_a_caller_on_a_host_the_window_is_not_showing(cx: &mut gpui::TestAppContext) {
    with_window(cx, |view| {
        // The window is looking at m2; its own projection need not even be
        // connected for a caller that lives on the local daemon.
        show_remote_host(view, LiveState::default());
        view.endpoints[0].live.status = ConnectionStatus::Connected;
        view.endpoints[0].live.snapshot = Some(Arc::new(snapshot(
            "boot-local",
            "w1X",
            "w1X:p9",
            Some("opencode"),
        )));

        let (target, endpoint) = current_identity(view, &caller("w1X", "w1X:p9"))
            .expect("the caller's own daemon proves the session");

        assert_eq!(endpoint, 0);
        assert_eq!(target.host, "boot-local");
        assert_eq!(target.session, "agent:opencode:w1X:w1X:p9");
        assert_eq!(target.workspace.as_deref(), Some("w1X"));
        assert_eq!(target.pane.as_deref(), Some("w1X:p9"));
    });
}

#[gpui::test]
fn proves_a_caller_on_the_selected_host(cx: &mut gpui::TestAppContext) {
    with_window(cx, |view| {
        view.live.status = ConnectionStatus::Connected;
        view.live.snapshot = Some(Arc::new(snapshot(
            "boot-selected",
            "w0",
            "w0:p1",
            Some("codex"),
        )));
        view.endpoints[0].live = view.live.clone();

        let (target, endpoint) = current_identity(view, &caller("w0", "w0:p1"))
            .expect("the selected daemon proves the session");

        assert_eq!(endpoint, 0);
        assert_eq!(target.host, "boot-selected");
        assert_eq!(target.session, "agent:codex:w0:w0:p1");
        assert_eq!(target.pane.as_deref(), Some("w0:p1"));
    });
}

#[gpui::test]
fn refuses_a_caller_without_a_session_on_another_host(cx: &mut gpui::TestAppContext) {
    with_window(cx, |view| {
        show_remote_host(view, LiveState::default());
        view.endpoints[0].live.status = ConnectionStatus::Connected;
        view.endpoints[0].live.snapshot =
            Some(Arc::new(snapshot("boot-local", "w1X", "w1X:p9", None)));

        let response = current_identity(view, &caller("w1X", "w1X:p9"))
            .expect_err("a pane without an agent session cannot prove a caller");

        assert!(matches!(
            response,
            Response::Error {
                code: ErrorCode::Unsupported,
                ..
            }
        ));
    });
}

#[gpui::test]
fn refuses_a_disconnected_caller_host_even_when_the_window_is_connected(
    cx: &mut gpui::TestAppContext,
) {
    with_window(cx, |view| {
        let mut selected = LiveState::default();
        selected.status = ConnectionStatus::Connected;
        show_remote_host(view, selected);
        view.endpoints[0].live.status = ConnectionStatus::Disconnected;
        view.endpoints[0].live.snapshot = Some(Arc::new(snapshot(
            "boot-local",
            "w1X",
            "w1X:p9",
            Some("opencode"),
        )));

        let response = current_identity(view, &caller("w1X", "w1X:p9"))
            .expect_err("the caller's own daemon is not connected");

        assert!(matches!(
            response,
            Response::Error {
                code: ErrorCode::Unsupported,
                ..
            }
        ));
    });
}
