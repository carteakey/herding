#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::dispatch::prompts_for_scoped_reply;
use super::identity::{PeerBindings, ProcessIdentity, check_peer};
use super::*;
use crate::{
    SheprdWindow,
    control::protocol::Caller,
    control::server::team_worker::Reply,
    endpoint::Endpoint,
    sidebar::layout_tests::fixture_window,
    state::{ConnectionStatus, LiveState},
};
use sheprd_client::{ConnectTarget, protocol::ClientShellSnapshot};
use sheprd_control_protocol::socket::PeerCredentials;
use sheprd_orchestrator::team_store::{Author, Member, SessionTarget, Task, TaskStatus, Team};
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
    snapshot_with_status(boot_id, workspace, pane, agent, "working")
}

fn snapshot_with_status(
    boot_id: &str,
    workspace: &str,
    pane: &str,
    agent: Option<&str>,
    status: &str,
) -> ClientShellSnapshot {
    let agents = match agent {
        Some(agent) => vec![serde_json::json!({
            "pane_id": pane, "workspace_id": workspace, "tab_id": "t0",
            "display_agent": agent, "agent": agent, "agent_status": status,
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
fn show_remote_host(view: &mut SheprdWindow, selected: LiveState) {
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
fn with_window(cx: &mut gpui::TestAppContext, act: impl FnOnce(&mut SheprdWindow)) {
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
fn live_boot_ids_collect_the_boots_connected_endpoints_report(cx: &mut gpui::TestAppContext) {
    with_window(cx, |view| {
        view.endpoints[0].live.status = ConnectionStatus::Connected;
        view.endpoints[0].live.snapshot = Some(Arc::new(snapshot(
            "boot-local",
            "w1X",
            "w1X:p9",
            Some("opencode"),
        )));
        let mut selected = LiveState::default();
        selected.status = ConnectionStatus::Connected;
        show_remote_host(view, selected);
        view.live.snapshot = Some(Arc::new(snapshot(
            "boot-remote",
            "w2",
            "w2:p1",
            Some("zcode"),
        )));

        let boots = live_boot_ids(view);
        assert_eq!(
            boots,
            vec!["boot-local".to_owned(), "boot-remote".to_owned()]
        );

        // A disconnected endpoint's snapshot may show the boot that just
        // died, so it does not count as live; neither does an empty boot.
        view.endpoints[0].live.status = ConnectionStatus::Disconnected;
        view.live.snapshot = Some(Arc::new(snapshot("", "w2", "w2:p1", Some("zcode"))));
        assert!(
            live_boot_ids(view).is_empty(),
            "a disconnected host and an empty boot id name no live boot"
        );
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

/// The hardening of finding c2686543: the snapshot proof still relies on
/// the asserted pane id, so a bystander process that names a live pane
/// passes `current_identity`; what pins it to the agent's own process is
/// the process binding recorded at registration - the pane's agent process,
/// not the transient CLI process - checked by `check_peer` as
/// descendant-or-self over the caller's process tree. The pre-hardening
/// baseline (a_caller_naming_a_live_pane_is_treated_as_that_pane) pinned
/// user-level trust; the first binding pinned the transient CLI pid and
/// locked every fresh invocation out; this pins the descendant contract.
#[gpui::test]
fn a_caller_naming_a_live_pane_needs_the_agents_process_tree(cx: &mut gpui::TestAppContext) {
    with_window(cx, |view| {
        view.live.status = ConnectionStatus::Connected;
        view.live.snapshot = Some(Arc::new(snapshot(
            "boot-selected",
            "w0",
            "w0:p1",
            Some("codex"),
        )));
        view.endpoints[0].live = view.live.clone();

        // The proof itself is unchanged: the asserted pane id is all the
        // snapshot sees.
        let bystander_caller = Caller {
            pane_id: Some("w0:p1".into()),
            workspace_id: Some("w0".into()),
            ..Caller::default()
        };
        let (target, endpoint) = current_identity(view, &bystander_caller)
            .expect("the proof relies on the asserted pane id alone, by design");
        assert_eq!(endpoint, 0);
        assert_eq!(target.pane.as_deref(), Some("w0:p1"));

        let uid = nix::unistd::geteuid().as_raw();
        let gid = nix::unistd::getgid().as_raw();
        // The registered agent process runs as pid 500 since ticks 777; the
        // caller's CLI process 900 descends from it through 800.
        let agent = ProcessIdentity {
            pid: 500,
            started_ticks: Some(777),
        };
        let bindings = [(target.clone(), Some(agent))]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        let table = [
            (900u32, (800u32, 10u64)),
            (800, (500, 10)),
            (500, (1, 777)),
            (901, (801, 10)),
            (801, (300, 10)),
            (300, (1, 42)),
        ];
        let process_info = move |pid: u32| table.iter().find(|(p, _)| *p == pid).map(|(_, v)| *v);

        // The registered agent process itself passes.
        let cred = PeerCredentials {
            uid,
            gid,
            pid: Some(500),
        };
        assert!(
            check_peer(
                &bindings,
                &target,
                Some(cred),
                true,
                Some(uid),
                &process_info
            )
            .is_ok()
        );
        // A descendant of it - a fresh CLI invocation spawned by the agent -
        // passes; that is the workflow the first binding locked out.
        let desc = PeerCredentials {
            uid,
            gid,
            pid: Some(900),
        };
        assert!(
            check_peer(
                &bindings,
                &target,
                Some(desc),
                true,
                Some(uid),
                &process_info
            )
            .is_ok()
        );
        // A bystander's tree never reaches the agent process.
        let bystander = PeerCredentials {
            uid,
            gid,
            pid: Some(901),
        };
        assert!(
            check_peer(
                &bindings,
                &target,
                Some(bystander),
                true,
                Some(uid),
                &process_info
            )
            .is_err()
        );
        // A recycled pid does not pass: the start time no longer matches.
        let reused = ProcessIdentity {
            pid: 500,
            started_ticks: Some(999),
        };
        let recycled = [(target.clone(), Some(reused))]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        assert!(
            check_peer(
                &recycled,
                &target,
                Some(cred),
                true,
                Some(uid),
                &process_info
            )
            .is_err()
        );
        // A pane registered without a provable agent process binds to the
        // user identity alone (ACP panes, macOS).
        let unpinned = [(target.clone(), None)]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        assert!(
            check_peer(
                &unpinned,
                &target,
                Some(desc),
                true,
                Some(uid),
                &process_info
            )
            .is_ok()
        );
        // The tunnel's sshd peer (root) is the documented downgrade; another
        // local user is refused; the Windows transport degrades to its DACL.
        let tunnel = PeerCredentials {
            uid: 0,
            gid: 0,
            pid: Some(1),
        };
        let empty = PeerBindings::default();
        assert!(
            check_peer(
                &empty,
                &target,
                Some(tunnel),
                true,
                Some(uid),
                &process_info
            )
            .is_ok()
        );
        let other = PeerCredentials {
            uid: uid + 1,
            gid,
            pid: Some(2),
        };
        assert!(check_peer(&empty, &target, Some(other), true, Some(uid), &process_info).is_err());
        assert!(check_peer(&empty, &target, None, false, None, &process_info).is_ok());
    });
}

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

mod dispatch_tests;
mod lineage;
mod wake_delivery_tests;
