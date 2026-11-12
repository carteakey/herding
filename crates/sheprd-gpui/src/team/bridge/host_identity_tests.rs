//! Stable host identity at the bridge: routing matches the endpoint's own
//! id before its boot, and an expired wake is never silent while the
//! member's host is connected - the supervisor is woken once per episode
//! and the roster marks the member until their pane is proved again.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use crate::SheprdWindow;
use crate::team::bridge::Bridge;
use crate::{endpoint::Endpoint, sidebar::layout_tests::fixture_window, state::ConnectionStatus};
use sheprd_client::{ConnectTarget, protocol::ClientShellSnapshot};
use sheprd_orchestrator::team_store::{Member, SessionTarget, Task, TaskStatus, Team};
use std::sync::Arc;

fn test_snapshot(boot_id: &str, pane: &str, idle: bool) -> ClientShellSnapshot {
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
            "workspace_id": "w0", "active_tab_id": "t0", "new_workspace_cwd": "",
            "number": 1, "label": "w0", "custom_label": false,
            "tokens": [], "focused": true, "agent_status": "idle"
        }],
        "tabs": [{
            "tab_id": "t0", "workspace_id": "w0", "number": 1, "label": "tab",
            "custom_label": false, "zoomed": false, "focused": true,
            "agent_status": "idle"
        }],
        "panes": [{
            "pane_id": pane, "workspace_id": "w0", "tab_id": "t0",
            "focused": true, "right_click_passthrough": false
        }],
        "agents": [{
            "pane_id": pane, "workspace_id": "w0", "tab_id": "t0",
            "display_agent": "zcode", "agent": "zcode",
            "agent_status": if idle { "idle" } else { "working" },
            "state_change_seq": 1, "state_labels": [], "tokens": [], "focused": true
        }],
        "commands": []
    }))
    .expect("test snapshot is valid")
}

fn member(id: &str, host: &str, pane: &str, name: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: name.into(),
        target: SessionTarget {
            host: host.into(),
            session: format!("agent:zcode:w0:{pane}"),
            workspace: Some("w0".into()),
            pane: Some(pane.into()),
        },
    }
}

fn team() -> Team {
    Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor: member("sup", "m1", "w0:p1", "sup"),
        members: vec![member("w1", "m1", "w0:p2", "w1")],
    }
}

fn held_task() -> Task {
    Task {
        task_id: "task-1".into(),
        team_id: "t".into(),
        creator: sheprd_orchestrator::team_store::Author::Member {
            member_id: "sup".into(),
        },
        title: "Watched".into(),
        brief: String::new(),
        assignee: Some("w1".into()),
        status: TaskStatus::InProgress,
        blocker_reason: None,
        result: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Endpoint `m1` connected under the given boot, showing only `pane`.
fn show_m1(view: &mut SheprdWindow, boot: &str, pane: &str) {
    let index = match view
        .endpoints
        .iter()
        .position(|endpoint| endpoint.id == "m1")
    {
        Some(index) => index,
        None => {
            let mut remote = Endpoint::new(
                "m1".into(),
                "m1".into(),
                ConnectTarget::Socket("/unused-host-identity.sock".into()),
                true,
            );
            remote.never_connect();
            view.endpoints.push(remote);
            view.endpoints.len() - 1
        }
    };
    view.endpoints[index].live.status = ConnectionStatus::Connected;
    view.endpoints[index].live.snapshot = Some(Arc::new(test_snapshot(boot, pane, true)));
    view.selected_endpoint = index;
    view.live = view.endpoints[index].live.clone();
}

fn bridge_with_team(view: &mut SheprdWindow) {
    view.team_bridge.set_team_for_test(team());
    view.team_bridge.tasks = vec![held_task()];
}

#[gpui::test]
fn a_wake_whose_pane_vanished_from_a_connected_host_wakes_the_supervisor_once(
    cx: &mut gpui::TestAppContext,
) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, _| {
            show_m1(view, "boot-m1", "w0:p1");
            bridge_with_team(view);
            // The member's wake, due now, recorded on m1 under the stable
            // host id; the snapshot no longer shows their pane.
            view.team_bridge.hold_prompt(
                "t".into(),
                "m1".into(),
                "w0:p2".into(),
                "claim the task".into(),
                "assigned".into(),
                Vec::new(),
            );
            view.team_bridge.force_retry_due_for_test();

            view.dispatch_drained_team_prompts();
            assert!(
                view.team_bridge.needs_reregister.contains_key("w1"),
                "the member is marked"
            );
            // No connection handle in the fixture: the supervisor wake is
            // held for retry, honestly not marked delivered, alongside the
            // member's own re-offered wake.
            assert_eq!(view.team_bridge.queued_count_for_test(), 2);
            assert!(
                !view.team_bridge.wake_seen("t", "reregister|t|w1|boot-m1"),
                "a held wake is not recorded as delivered"
            );

            // The same episode re-offered does not queue a second copy: the
            // held queue dedupes on the episode's signal.
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts();
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                2,
                "one member wake plus one supervisor wake, deduped"
            );
        })
        .expect("the window stays open");
}

#[gpui::test]
fn a_new_boot_after_another_restart_rewakes_the_supervisor(cx: &mut gpui::TestAppContext) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, _| {
            show_m1(view, "boot-m1", "w0:p1");
            bridge_with_team(view);
            view.team_bridge.hold_prompt(
                "t".into(),
                "m1".into(),
                "w0:p2".into(),
                "claim the task".into(),
                "assigned".into(),
                Vec::new(),
            );
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts();
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                2,
                "the member wake plus the first episode's supervisor wake"
            );

            // Another restart: m1 now reports a different boot and still
            // lacks the pane, so the episode changes and the supervisor is
            // woken again - a second, distinct supervisor wake. The member's
            // own wake is still queued from the first episode and retries.
            show_m1(view, "boot-m2", "w0:p1");
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts();
            assert_eq!(
                view.team_bridge
                    .needs_reregister
                    .get("w1")
                    .map(String::as_str),
                Some("boot-m2"),
                "the mark carries the boot that lost the member"
            );
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                3,
                "the member wake, the held first episode, and the new episode's wake"
            );
            assert!(
                !view.team_bridge.wake_seen("t", "reregister|t|w1|boot-m2"),
                "an undelivered episode is not recorded as delivered"
            );
        })
        .expect("the window stays open");
}

#[gpui::test]
fn the_mark_clears_when_the_members_pane_is_proved_again(cx: &mut gpui::TestAppContext) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, _| {
            show_m1(view, "boot-m1", "w0:p1");
            bridge_with_team(view);
            view.team_bridge.hold_prompt(
                "t".into(),
                "m1".into(),
                "w0:p2".into(),
                "claim the task".into(),
                "assigned".into(),
                Vec::new(),
            );
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts();
            assert!(view.team_bridge.needs_reregister.contains_key("w1"));

            // The member re-registers onto a live pane on the same host;
            // the refreshed roster shows it, so the mark clears.
            let mut rebound = team();
            rebound.members[0].target.pane = Some("w0:p3".into());
            view.team_bridge.set_team_for_test(rebound);
            show_m1(view, "boot-m1", "w0:p3");
            view.dispatch_drained_team_prompts();
            assert!(
                !view.team_bridge.needs_reregister.contains_key("w1"),
                "a proved pane clears the mark"
            );
        })
        .expect("the window stays open");
}

#[gpui::test]
fn routing_matches_the_endpoint_id_before_the_boot(cx: &mut gpui::TestAppContext) {
    cx.add_window(fixture_window)
        .update(cx, |view, _, _| {
            // Endpoint m1 reports boot-m1: a member recorded under either
            // key resolves to it. A restart keeps the id match alive.
            show_m1(view, "boot-m1", "w0:p1");
            assert_eq!(view.endpoint_for_host("m1"), Some(1));
            assert_eq!(view.endpoint_for_host("boot-m1"), Some(1));
            assert_eq!(view.endpoint_for_host("m2"), None);
            assert_eq!(view.endpoint_for_host(""), None);
        })
        .expect("the window stays open");
}

#[test]
fn the_roster_marks_a_member_whose_pane_is_gone() {
    let mut bridge = Bridge::default();
    bridge.set_team_for_test(team());
    bridge
        .needs_reregister
        .insert("w1".into(), "boot-m1".into());
    let snapshot = bridge.snapshot(&[]);
    let w1 = snapshot
        .members
        .iter()
        .find(|member| member.member_id == "w1")
        .expect("the member is on the roster");
    assert_eq!(w1.status, "Needs re-register");
    let sup = snapshot
        .members
        .iter()
        .find(|member| member.member_id == "sup")
        .expect("the supervisor is on the roster");
    assert_ne!(sup.status, "Needs re-register");
}
