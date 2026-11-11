//! Team wakes route by the member's recorded host (#40): pane ids are
//! per-daemon and short, so two connected hosts can both show the same pane
//! id, and a bare pane id names no machine. These pin that a wake resolves
//! its endpoint by the recorded host first and the pane inside it, that a
//! disconnected host is unreachable rather than falling back to whatever the
//! selected endpoint shows, and that local members keep their existing path.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use crate::SheprdWindow;
use crate::{
    endpoint::Endpoint,
    sidebar::layout_tests::fixture_window,
    state::{ConnectionStatus, LiveState},
};
use sheprd_client::{ConnectTarget, protocol::ClientShellSnapshot};
use std::sync::Arc;

/// A snapshot with one pane and one agent in the given status, so tests can
/// drive the busy and idle sides of the hold gate.
fn test_snapshot_with_status(
    boot_id: &str,
    workspace: &str,
    pane: &str,
    agent: &str,
    agent_status: &str,
) -> ClientShellSnapshot {
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
            "tokens": [], "focused": true, "agent_status": agent_status
        }],
        "tabs": [{
            "tab_id": "t0", "workspace_id": workspace, "number": 1, "label": "tab",
            "custom_label": false, "zoomed": false, "focused": true,
            "agent_status": agent_status
        }],
        "panes": [{
            "pane_id": pane, "workspace_id": workspace, "tab_id": "t0",
            "focused": true, "right_click_passthrough": false
        }],
        "agents": [{
            "pane_id": pane, "workspace_id": workspace, "tab_id": "t0",
            "display_agent": agent, "agent": agent, "agent_status": agent_status,
            "state_change_seq": 1, "state_labels": [], "tokens": [], "focused": true
        }],
        "commands": []
    }))
    .expect("test snapshot is valid")
}

fn add_remote_endpoint(view: &mut SheprdWindow, id: &str, label: &str, live: LiveState) {
    let mut remote = Endpoint::new(
        id.into(),
        label.into(),
        ConnectTarget::Socket(format!("/unused-{id}.sock").into()),
        true,
    );
    remote.never_connect();
    remote.live = live;
    view.endpoints.push(remote);
}

/// The collision shape of #40: the local daemon and the remote daemon both
/// allocated the pane id `w0:p1`. The local agent is mid-turn, the remote
/// agent idle, so a wake routed by pane id alone would be held - or pasted -
/// by whichever endpoint comes first.
fn two_hosts_same_pane_id(view: &mut SheprdWindow) {
    view.live.status = ConnectionStatus::Connected;
    view.live.snapshot = Some(Arc::new(test_snapshot_with_status(
        "boot-local",
        "w0",
        "w0:p1",
        "codex",
        "working",
    )));
    view.endpoints[0].live = view.live.clone();

    let mut remote_live = LiveState::default();
    remote_live.status = ConnectionStatus::Connected;
    remote_live.snapshot = Some(Arc::new(test_snapshot_with_status(
        "boot-m1", "w0", "w0:p1", "claude", "idle",
    )));
    add_remote_endpoint(view, "m1", "m1", remote_live);
}

#[gpui::test]
fn the_same_pane_id_on_two_hosts_routes_to_the_recorded_host(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            two_hosts_same_pane_id(view);
            assert_eq!(view.selected_endpoint, 0, "the local host is first");

            // The recorded host picks the endpoint, not the first endpoint
            // that happens to show the pane id.
            assert_eq!(
                view.target_endpoint_for_member("boot-local", "w0:p1"),
                Some(0)
            );
            assert_eq!(view.target_endpoint_for_member("boot-m1", "w0:p1"), Some(1));

            // The hold gate is scoped to the member's own host: the local
            // pane being mid-turn must not hold the m1 member's wake.
            assert!(view.team_pane_busy("boot-local", "w0:p1"));
            assert!(!view.team_pane_busy("boot-m1", "w0:p1"));

            // The m1 member's wake gets past host resolution to m1's own
            // endpoint. What matters is that it is not `Unreachable`, which
            // is what a failed host lookup would report. The endpoint's
            // daemon input is not connected in this fixture, so the wake is
            // kept for retry rather than claimed delivered - a wake that
            // never reached a program must not be recorded as sent (#62).
            assert_eq!(
                view.deliver_team_prompt("boot-m1", "w0:p1", "claim the task".into()),
                crate::agent_notes::Delivered::Kept
            );
        })
    });
}

#[gpui::test]
fn a_disconnected_host_is_unreachable_and_never_falls_back(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            two_hosts_same_pane_id(view);
            // The selected endpoint shows w0:p1; a member recorded on a host
            // that is not connected must not wake it.
            assert_eq!(
                view.target_endpoint_for_member("boot-gone", "w0:p1"),
                None,
                "an unknown host resolves to nothing"
            );
            assert_eq!(
                view.deliver_team_prompt("boot-gone", "w0:p1", "claim the task".into()),
                crate::agent_notes::Delivered::Unreachable,
                "no connected host, no delivery - and nothing pasted into the selected endpoint's agent"
            );

            // An endpoint that dropped its connection is the same, even
            // though its last snapshot still shows the pane.
            view.endpoints[1].live.status = ConnectionStatus::Disconnected;
            assert_eq!(view.target_endpoint_for_member("boot-m1", "w0:p1"), None);
            assert_eq!(
                view.deliver_team_prompt("boot-m1", "w0:p1", "claim the task".into()),
                crate::agent_notes::Delivered::Unreachable
            );

            // A record with no host at all cannot name a machine, so it
            // delivers nowhere rather than guessing.
            assert_eq!(view.target_endpoint_for_member("", "w0:p1"), None);
        })
    });
}

#[gpui::test]
fn a_local_member_keeps_its_existing_path(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            two_hosts_same_pane_id(view);

            // The recorded host is the local daemon's boot id: resolution
            // lands on the selected endpoint, busy and dead checks see the
            // local agent, and delivery reaches the local pane exactly as
            // before host routing existed.
            assert_eq!(
                view.target_endpoint_for_member("boot-local", "w0:p1"),
                Some(0)
            );
            assert!(view.team_pane_busy("boot-local", "w0:p1"));
            assert!(!view.team_session_dead("boot-local", "w0:p1"));
            assert_eq!(
                view.deliver_team_prompt("boot-local", "w0:p1", "claim the task".into()),
                crate::agent_notes::Delivered::Kept,
                "the local agent is mid-turn, so the wake is kept for it to finish"
            );
        })
    });
}
