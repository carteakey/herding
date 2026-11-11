#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::endpoint::Endpoint;
use crate::sidebar::layout_tests::fixture_window;
use crate::state::{ConnectionStatus, LiveState};
use gpui::TestAppContext;
use sheprd_client::{ConnectTarget, protocol::ClientShellSnapshot};
use sheprd_orchestrator::team_store::{Author, Member, SessionTarget, Task, TaskStatus, Team};
use std::sync::Arc;

fn member(id: &str, pane: &str) -> Member {
    Member {
        member_id: id.to_owned(),
        display_name: id.to_owned(),
        target: SessionTarget {
            host: "boot-local".into(),
            session: format!("agent:zcode:ws:{id}"),
            workspace: Some("ws".into()),
            pane: Some(pane.into()),
        },
    }
}

fn team() -> Team {
    Team {
        team_id: "t".into(),
        name: "Stall".into(),
        supervisor: member("sup", "ws:p1"),
        members: vec![member("w1", "ws:p2")],
    }
}

fn held_task() -> Task {
    Task {
        task_id: "t-1".into(),
        team_id: "t".into(),
        creator: Author::Member {
            member_id: "sup".into(),
        },
        title: "Watched work".into(),
        brief: String::new(),
        assignee: Some("w1".into()),
        status: TaskStatus::InProgress,
        blocker_reason: None,
        result: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn snapshot(pane: &str, seq: u64) -> ClientShellSnapshot {
    let agents = vec![serde_json::json!({
        "pane_id": pane, "workspace_id": "ws", "tab_id": "t0",
        "display_agent": "zcode", "agent": "zcode", "agent_status": "idle",
        "state_change_seq": seq, "state_labels": [], "tokens": [], "focused": true
    })];
    serde_json::from_value(serde_json::json!({
        "boot_id": "boot-local", "revision": seq,
        "update_install_command": "",
        "latest_release_notes_available": false,
        "integration_updates_available": false,
        "worktree_directory": "",
        "tab_bar_right": [],
        "tab_bar_right_separator": "",
        "agent_order": [],
        "workspaces": [{
            "workspace_id": "ws", "active_tab_id": "t0", "new_workspace_cwd": "",
            "number": 1, "label": "ws", "custom_label": false,
            "tokens": [], "focused": true, "agent_status": "idle"
        }],
        "tabs": [{
            "tab_id": "t0", "workspace_id": "ws", "number": 1, "label": "tab",
            "custom_label": false, "zoomed": false, "focused": true,
            "agent_status": "idle"
        }],
        "panes": [{
            "pane_id": pane, "workspace_id": "ws", "tab_id": "t0",
            "focused": true, "right_click_passthrough": false
        }],
        "agents": agents,
        "commands": []
    }))
    .expect("test snapshot is valid")
}

/// The stall clock is the caller's `now`, so the tests step it directly
/// instead of waiting out the timeout.
const TIMEOUT: Duration = Duration::from_secs(180);

fn show_local(view: &mut SheprdWindow, pane: &str, seq: u64) {
    let mut local = Endpoint::new(
        "Local".into(),
        "Local".into(),
        ConnectTarget::Socket("/unused-stall-test.sock".into()),
        true,
    );
    local.never_connect();
    view.endpoints.push(local);
    view.selected_endpoint = 0;
    view.live = LiveState::default();
    view.live.status = ConnectionStatus::Connected;
    view.live.snapshot = Some(Arc::new(snapshot(pane, seq)));
}

#[gpui::test]
fn a_silent_assigned_worker_queues_one_wake_without_the_panel(cx: &mut TestAppContext) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, cx| {
            show_local(view, "ws:p2", 1);
            view.team_bridge.set_team_for_test(team());
            view.team_bridge.tasks = vec![held_task()];
            // The panel was never shown: tracking still runs.
            assert!(!view.team_panel_shown());
            assert_eq!(view.team_bridge.queued_count_for_test(), 0);

            let start = Instant::now();
            view.track_team_stalls(start, cx);
            assert!(view.team_stall_for_pane("ws:p2").is_none());

            // Both signals silent past the timeout: detected, and one wake
            // is queued for the supervisor.
            view.track_team_stalls(start + TIMEOUT, cx);
            let record = view.team_stall_for_pane("ws:p2").expect("w1 is stalled");
            assert_eq!(record.task_id, "t-1");
            assert_eq!(record.minutes, 3);
            assert_eq!(view.team_bridge.queued_count_for_test(), 1);

            // The same stall re-offered keeps one wake: the frozen signal
            // dedupes at the queue.
            view.track_team_stalls(start + TIMEOUT + Duration::from_secs(10), cx);
            assert_eq!(view.team_bridge.queued_count_for_test(), 1);
        })
        .expect("the window stays open");
}

#[gpui::test]
fn a_recovery_clears_the_stall_and_a_restall_rewakes(cx: &mut TestAppContext) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, cx| {
            show_local(view, "ws:p2", 1);
            view.team_bridge.set_team_for_test(team());
            view.team_bridge.tasks = vec![held_task()];
            let start = Instant::now();
            view.track_team_stalls(start, cx);
            view.track_team_stalls(start + TIMEOUT, cx);
            assert!(view.team_stall_for_pane("ws:p2").is_some());

            // Recovery: fresh output and a fresh CLI call end the stall.
            view.live.snapshot = Some(Arc::new(snapshot("ws:p2", 2)));
            view.note_team_member_call("agent:zcode:ws:w1");
            let recovered_at = start + TIMEOUT + Duration::from_secs(10);
            view.track_team_stalls(recovered_at, cx);
            assert!(view.team_stall_for_pane("ws:p2").is_none());
            assert_eq!(view.team_bridge.queued_count_for_test(), 1);

            // A new stall re-wakes: the episode changes the signal.
            let restalled_at = recovered_at + TIMEOUT;
            view.track_team_stalls(restalled_at, cx);
            let record = view.team_stall_for_pane("ws:p2").expect("stalled again");
            assert_ne!(record.signal("t"), "stall|t|t-1|w1|1");
            assert_eq!(view.team_bridge.queued_count_for_test(), 2);
        })
        .expect("the window stays open");
}
