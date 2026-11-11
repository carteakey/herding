#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::SheprdWindow;
use sheprd_orchestrator::team_store::{Author, Member, SessionTarget, Task, TaskStatus};

fn test_team() -> Team {
    Team {
        team_id: "t1".into(),
        name: "team1".into(),
        supervisor: Member {
            member_id: "m_sup".into(),
            display_name: "supervisor".into(),
            target: SessionTarget {
                host: "local".into(),
                session: "default".into(),
                workspace: Some("ws".into()),
                pane: Some("p_sup".into()),
            },
        },
        members: vec![
            Member {
                member_id: "m_w1".into(),
                display_name: "worker1".into(),
                target: SessionTarget {
                    host: "local".into(),
                    session: "default".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p_w1".into()),
                },
            },
            Member {
                member_id: "m_w2".into(),
                display_name: "worker2".into(),
                target: SessionTarget {
                    host: "local".into(),
                    session: "default".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p_w2".into()),
                },
            },
        ],
    }
}

#[test]
fn bulletin_post_targets_only_mentioned_recipient() {
    let team = test_team();
    let action = TeamAction::Post {
        team_id: "t1".into(),
        author: Author::Human {
            user_id: "human".into(),
        },
        body: "@worker1 please check this".into(),
    };
    let prompts = prompts_for_team_action(&action, Some(&team), &[]);
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].pane_id, "p_w1");
    assert!(prompts[0].prompt.contains("@worker1 please check this"));
}

#[test]
fn bulletin_prompts_instruct_explicit_ack() {
    let team = test_team();
    let post_action = TeamAction::Post {
        team_id: "t1".into(),
        author: Author::Human {
            user_id: "human".into(),
        },
        body: "@worker1 please check this".into(),
    };
    let prompts = prompts_for_team_action(&post_action, Some(&team), &[]);
    assert!(prompts[0].prompt.contains("bulletin ack t1"));

    let worker_action = TeamAction::AddWorker {
        team_id: "t1".into(),
        name: "worker3".into(),
        pane: PaneIdentity {
            daemon_id: "d1".into(),
            session_id: Some("s1".into()),
            workspace_id: "ws".into(),
            pane_id: "p_w3".into(),
        },
    };
    let worker_prompts = prompts_for_team_action(&worker_action, Some(&team), &[]);
    assert!(worker_prompts[0].prompt.contains("bulletin ack t1"));
}

#[test]
fn bulletin_post_broadcasts_to_all_when_no_mention() {
    let team = test_team();
    let action = TeamAction::Post {
        team_id: "t1".into(),
        author: Author::Human {
            user_id: "human".into(),
        },
        body: "Team sync in 5m".into(),
    };
    let prompts = prompts_for_team_action(&action, Some(&team), &[]);
    assert_eq!(prompts.len(), 3);
    let panes: Vec<_> = prompts
        .iter()
        .map(|dispatch| dispatch.pane_id.as_str())
        .collect();
    assert!(panes.contains(&"p_sup"));
    assert!(panes.contains(&"p_w1"));
    assert!(panes.contains(&"p_w2"));
}

#[test]
fn bulletin_post_excludes_member_author_from_all_broadcast() {
    let team = test_team();
    let action = TeamAction::Post {
        team_id: "t1".into(),
        author: Author::Member {
            member_id: "m_w1".into(),
        },
        body: "General update from w1".into(),
    };
    let prompts = prompts_for_team_action(&action, Some(&team), &[]);
    assert_eq!(prompts.len(), 2);
    let panes: Vec<_> = prompts
        .iter()
        .map(|dispatch| dispatch.pane_id.as_str())
        .collect();
    assert!(panes.contains(&"p_sup"));
    assert!(panes.contains(&"p_w2"));
    assert!(!panes.contains(&"p_w1"));
}

#[test]
fn bulletin_post_with_invalid_mention_dispatches_to_none() {
    let team = test_team();
    let action = TeamAction::Post {
        team_id: "t1".into(),
        author: Author::Human {
            user_id: "human".into(),
        },
        body: "@nonexistent_person check this".into(),
    };
    let prompts = prompts_for_team_action(&action, Some(&team), &[]);
    assert_eq!(prompts.len(), 0);
}

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

fn signal_task(status: TaskStatus) -> Task {
    Task {
        task_id: "task1".into(),
        team_id: "t1".into(),
        creator: Author::Human {
            user_id: "human".into(),
        },
        title: "Write report".into(),
        brief: String::new(),
        assignee: None,
        status,
        blocker_reason: None,
        result: None,
        created_at: at(0),
        updated_at: at(10),
    }
}

#[test]
fn excerpt_caps_long_results() {
    let long = "x".repeat(500);
    assert_eq!(excerpt(&long).chars().count(), 243);
    assert!(excerpt(&long).ends_with("..."));
    assert_eq!(excerpt("  short  "), "short");
}

#[test]
fn wake_signal_tracks_status_and_revision() {
    let mut task = signal_task(TaskStatus::Done);
    let first = wake_signal(&task);
    task.updated_at = at(11);
    assert_ne!(wake_signal(&task), first, "a re-done task wakes again");
    task.status = TaskStatus::Open;
    assert_ne!(wake_signal(&task), first, "a reopened task wakes again");
}

use crate::{
    endpoint::Endpoint,
    sidebar::layout_tests::fixture_window,
    state::{ConnectionStatus, LiveState},
};
use sheprd_client::{ConnectTarget, protocol::ClientShellSnapshot};
use std::sync::Arc;

fn test_snapshot(
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

/// A snapshot whose single agent carries the given state labels, so tests
/// can drive the crash and rate-limit vocabulary.
fn test_snapshot_with_labels(
    boot_id: &str,
    workspace: &str,
    pane: &str,
    agent: &str,
    state_labels: &[(&str, &str)],
) -> ClientShellSnapshot {
    let mut snapshot = test_snapshot(boot_id, workspace, pane, Some(agent));
    let labels = state_labels
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect::<Vec<_>>();
    for entry in snapshot.agents.iter_mut() {
        entry.state_labels = labels.clone();
    }
    snapshot
}

#[gpui::test]
fn a_dead_session_is_never_dispatched_to(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // The crash vocabulary the heartbeat inspection reports.
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(test_snapshot_with_labels(
                "boot",
                "w1X",
                "w1X:p1",
                "codex",
                &[("working", "process exited with status 1")],
            )));

            assert!(
                view.team_session_dead("boot", "w1X:p1"),
                "a crashed session is dead and skipped"
            );
            assert!(
                !view.team_session_dead("boot", "w1X:other"),
                "a pane without an agent is not reported dead; delivery simply finds no agent"
            );
            assert!(
                !view.team_session_dead("other-boot", "w1X:p1"),
                "an unknown host resolves to no endpoint, so nothing is reported dead"
            );

            // The rate-limit vocabulary is the PTO state, not death.
            view.live.snapshot = Some(Arc::new(test_snapshot_with_labels(
                "boot",
                "w1X",
                "w1X:p1",
                "codex",
                &[("working", "rate limit 429, retry later")],
            )));
            assert!(!view.team_session_dead("boot", "w1X:p1"));

            // A healthy agent is alive.
            view.live.snapshot = Some(Arc::new(test_snapshot(
                "boot",
                "w1X",
                "w1X:p1",
                Some("codex"),
            )));
            assert!(!view.team_session_dead("boot", "w1X:p1"));
        })
    });
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

#[gpui::test]
fn team_candidates_discovers_agents_across_all_endpoints(cx: &mut gpui::TestAppContext) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, _| {
            // Setup local endpoint (index 0)
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(test_snapshot(
                "boot-local",
                "w0",
                "w0:p1",
                Some("codex"),
            )));
            view.endpoints[0].live = view.live.clone();

            // Setup remote endpoint (index 1)
            let mut remote_live = LiveState::default();
            remote_live.status = ConnectionStatus::Connected;
            remote_live.snapshot = Some(Arc::new(test_snapshot(
                "boot-m2",
                "w2X",
                "w2X:p1",
                Some("claude"),
            )));
            add_remote_endpoint(view, "m2", "m2", remote_live);

            let candidates = view.team_candidates();
            assert_eq!(candidates.len(), 2);

            // Local candidate has no prefix
            let local = candidates
                .iter()
                .find(|c| c.identity.pane_id == "w0:p1")
                .unwrap();
            assert_eq!(local.label.as_ref(), "w0:p1 (codex)");
            assert_eq!(local.identity.daemon_id, "boot-local");

            // Remote candidate has host prefix
            let remote = candidates
                .iter()
                .find(|c| c.identity.pane_id == "w2X:p1")
                .unwrap();
            assert_eq!(remote.label.as_ref(), "m2: w2X:p1 (claude)");
            assert_eq!(remote.identity.daemon_id, "boot-m2");
        })
        .expect("window stays open");
}

#[gpui::test]
fn target_endpoint_for_member_routes_by_recorded_host(cx: &mut gpui::TestAppContext) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, _| {
            // Local is index 0
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(test_snapshot(
                "boot-local",
                "w0",
                "w0:p1",
                Some("codex"),
            )));
            view.endpoints[0].live = view.live.clone();

            // Remote m2 is index 1
            let mut remote_live = LiveState::default();
            remote_live.status = ConnectionStatus::Connected;
            remote_live.snapshot = Some(Arc::new(test_snapshot(
                "boot-m2",
                "w2X",
                "w2X:p1",
                Some("claude"),
            )));
            add_remote_endpoint(view, "m2", "m2", remote_live);

            // While viewing local (selected_endpoint = 0):
            assert_eq!(view.selected_endpoint, 0);

            // A member recorded on the local boot routes to index 0
            assert_eq!(
                view.target_endpoint_for_member("boot-local", "w0:p1"),
                Some(0)
            );

            // A member recorded on m2 routes to index 1
            assert_eq!(
                view.target_endpoint_for_member("boot-m2", "w2X:p1"),
                Some(1)
            );

            // Scope resolution resolves to remote endpoint
            let target_index = view
                .target_endpoint_for_member("boot-m2", "w2X:p1")
                .expect("the remote member resolves");
            let scope = crate::browser::scope(&view.endpoints[target_index]);
            assert_eq!(scope, crate::browser::scope(&view.endpoints[1]));

            // A recorded host that is not connected is a refusal, never a
            // fall back to the selected endpoint: the same pane id can name
            // an unrelated agent there (#40).
            assert_eq!(view.target_endpoint_for_member("boot-gone", "w0:p1"), None);
            assert_eq!(
                view.target_endpoint_for_member("boot-gone", "unknown:pane"),
                None
            );

            // A connected host whose snapshot no longer shows the pane is a
            // refusal too.
            assert_eq!(
                view.target_endpoint_for_member("boot-m2", "unknown:pane"),
                None
            );
            assert_eq!(
                view.target_endpoint_for_member("boot-local", "w2X:p1"),
                None
            );

            // If viewing remote (selected_endpoint = 1), the recorded host
            // still wins over the selection.
            view.selected_endpoint = 1;
            assert_eq!(
                view.target_endpoint_for_member("boot-local", "w0:p1"),
                Some(0)
            );
            assert_eq!(view.target_endpoint_for_member("boot-gone", "w2X:p1"), None);
        })
        .expect("window stays open");
}
