#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::dispatch::prompts_for_scoped_reply;
use super::identity::{PeerBindings, ProcessIdentity, check_peer};
use super::*;
use crate::{
    HerdrWindow,
    control::protocol::Caller,
    control::server::team_worker::Reply,
    endpoint::Endpoint,
    sidebar::layout_tests::fixture_window,
    state::{ConnectionStatus, LiveState},
};
use herdr_client::{ConnectTarget, protocol::ClientShellSnapshot};
use herdr_control_protocol::socket::PeerCredentials;
use herdr_orchestrator::team_store::{Author, Member, SessionTarget, Task, TaskStatus, Team};
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

/// A done task completed by its worker on a team whose supervisor pane is
/// `p_sup`, which the fixture window's daemon does not show.
fn supervisor_wake_reply() -> Reply {
    Reply::ScopedTask {
        team: Team {
            team_id: "t-x".into(),
            name: "wake-test".into(),
            supervisor: Member {
                member_id: "m_sup".into(),
                display_name: "supervisor".into(),
                target: SessionTarget {
                    host: "local".into(),
                    session: "default".into(),
                    workspace: Some("w1X".into()),
                    pane: Some("p_sup".into()),
                },
            },
            members: vec![Member {
                member_id: "m_w1".into(),
                display_name: "worker1".into(),
                target: SessionTarget {
                    host: "local".into(),
                    session: "default".into(),
                    workspace: Some("w1X".into()),
                    pane: Some("w1X:p9".into()),
                },
            }],
        },
        task: Task {
            task_id: "task9".into(),
            team_id: "t-x".into(),
            creator: Author::Human {
                user_id: "human".into(),
            },
            title: "Write report".into(),
            brief: String::new(),
            assignee: Some("m_w1".into()),
            status: TaskStatus::Done,
            blocker_reason: None,
            result: Some("Report drafted".into()),
            created_at: at(0),
            updated_at: at(10),
        },
        actor: Author::Member {
            member_id: "m_w1".into(),
        },
    }
}

#[gpui::test]
fn supervisor_wake_on_unreachable_pane_stays_pending_and_honest(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    window
        .update(cx, |view, _, _| {
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(snapshot(
                "boot-local",
                "w1X",
                "w1X:p9",
                Some("opencode"),
            )));
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
        })
        .expect("window open");

    cx.update(|app| notify_team_mutation(&supervisor_wake_reply(), app));

    let signal = "task9|done|10000";
    window
        .update(cx, |view, _, _| {
            let activity = view
                .team_bridge
                .snapshot(&[])
                .last_activity
                .expect("the wake attempt is recorded");
            assert!(
                activity.contains("not delivered"),
                "unreachable pane is reported honestly: {activity}"
            );
            // Fail closed: nothing was marked woken, so the fallback or a
            // later event can still deliver the result.
            assert!(!view.team_bridge.wake_seen("t-x", signal));

            // The event is not repeated once a wake is known to have landed.
            view.team_bridge.mark_supervisor_woken("t-x", [signal]);
        })
        .expect("window open");

    let before = window
        .update(cx, |view, _, _| {
            view.team_bridge.snapshot(&[]).last_activity
        })
        .expect("window open");
    cx.update(|app| notify_team_mutation(&supervisor_wake_reply(), app));
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.snapshot(&[]).last_activity,
                before,
                "a seen signal dispatches nothing"
            );
        })
        .expect("window open");
}

fn worker_actor() -> Author {
    Author::Member {
        member_id: "m_w1".into(),
    }
}

fn wake_task(status: TaskStatus) -> Task {
    Task {
        task_id: "task1".into(),
        team_id: "t1".into(),
        creator: Author::Human {
            user_id: "human".into(),
        },
        title: "Write report".into(),
        brief: String::new(),
        assignee: Some("m_w1".into()),
        status,
        blocker_reason: None,
        result: None,
        created_at: at(0),
        updated_at: at(10),
    }
}

fn wake_team() -> Team {
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
        members: vec![Member {
            member_id: "m_w1".into(),
            display_name: "worker1".into(),
            target: SessionTarget {
                host: "local".into(),
                session: "default".into(),
                workspace: Some("ws".into()),
                pane: Some("p_w1".into()),
            },
        }],
    }
}

#[test]
fn completed_task_wakes_supervisor_with_result_and_signal() {
    let mut task = wake_task(TaskStatus::Done);
    task.result = Some("Report drafted and reviewed".into());
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task,
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    let dispatch = &dispatches[0];
    assert_eq!(dispatch.team_id, "t1");
    assert_eq!(dispatch.pane_id, "p_sup");
    assert!(dispatch.prompt.contains("completed by worker1"));
    assert!(dispatch.prompt.contains("Report drafted and reviewed"));
    assert!(dispatch.prompt.contains("task list t1"));
    assert!(dispatch.prompt.contains("go idle"));
    assert_eq!(dispatch.signal.as_deref(), Some("task1|done|10000"));
}

#[test]
fn blocked_task_wakes_supervisor_with_reason() {
    let mut task = wake_task(TaskStatus::Blocked);
    task.blocker_reason = Some("waiting on credentials".into());
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task,
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    assert!(dispatches[0].prompt.contains("waiting on credentials"));
    assert!(dispatches[0].prompt.contains("blocked"));
}

#[test]
fn supervisors_own_completion_does_not_wake_itself() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Done),
        actor: Author::Member {
            member_id: "m_sup".into(),
        },
    });
    assert!(dispatches.is_empty());
}

#[test]
fn human_completion_wakes_supervisor() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Done),
        actor: Author::Human {
            user_id: "human".into(),
        },
    });
    assert_eq!(dispatches.len(), 1);
}

#[test]
fn open_task_wakes_the_assignee_with_a_dedupe_signal() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Open),
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].pane_id, "p_w1");
    assert_eq!(
        dispatches[0].signal.as_deref(),
        Some("assignee|task1|m_w1|10000"),
        "the wake dedupes on one stored assignment state"
    );
}

#[test]
fn a_reassigned_task_wakes_once_per_assignment_state() {
    // The same stored state redelivered - identical signal, so the wake
    // ledger dedupes it and the assignee is never woken twice for it.
    let first = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Open),
        actor: worker_actor(),
    });
    let redelivered = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Open),
        actor: worker_actor(),
    });
    assert_eq!(first[0].signal, redelivered[0].signal);

    // A genuine re-assign is a new state: a new signal, one further wake.
    let mut reassigned = wake_task(TaskStatus::Open);
    reassigned.updated_at = at(11);
    let again = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: reassigned,
        actor: worker_actor(),
    });
    assert_ne!(first[0].signal, again[0].signal);
}

#[test]
fn claimed_task_does_not_wake_anyone() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::InProgress),
        actor: worker_actor(),
    });
    assert!(dispatches.is_empty());
}

/// The lineage contract against the real /proc reader, with live processes:
/// these complement the synthetic-table contract test above by proving the
/// walk against the kernel, including the re-parented-orphan case.
mod lineage {
    use super::*;
    use std::time::Duration;

    fn target_fixture() -> SessionTarget {
        SessionTarget {
            host: "boot".into(),
            session: "agent:codex:w0:w0:p1".into(),
            workspace: Some("w0".into()),
            pane: Some("w0:p1".into()),
        }
    }

    fn bindings_for(agent: ProcessIdentity) -> PeerBindings {
        [(target_fixture(), Some(agent))]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>()
    }

    fn caller(pid: u32) -> Option<PeerCredentials> {
        Some(PeerCredentials {
            uid: nix::unistd::geteuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
            pid: Some(pid),
        })
    }

    fn local_uid() -> Option<u32> {
        Some(nix::unistd::geteuid().as_raw())
    }

    /// Waits until `pid` no longer descends from this test process: a
    /// `sh -c 'sleep 5 &'` orphan is still a child of the short-lived shell
    /// for a few ticks, which would make the bystander look like a
    /// descendant of the agent. Re-parenting lands on a systemd user
    /// subreaper rather than init, so the parent pid alone cannot be checked.
    fn wait_detached_from_test(pid: u32) {
        let self_pid = std::process::id();
        for _ in 0..40 {
            let mut current = pid;
            let mut reaches_test = false;
            for _ in 0..16 {
                if current == self_pid {
                    reaches_test = true;
                    break;
                }
                let Some((parent, _)) = process_info(current) else {
                    break;
                };
                if parent <= 1 {
                    break;
                }
                current = parent;
            }
            if !reaches_test {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the orphan process still descends from this test process");
    }

    #[test]
    fn the_agent_process_itself_is_accepted() {
        let pid = std::process::id();
        let (_, started) = process_info(pid).expect("this test's own process is readable");
        let bindings = bindings_for(ProcessIdentity {
            pid,
            started_ticks: Some(started),
        });
        assert!(
            check_peer(
                &bindings,
                &target_fixture(),
                caller(pid),
                true,
                local_uid(),
                &process_info
            )
            .is_ok()
        );
    }

    #[test]
    fn a_descendant_of_the_agent_is_accepted() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("a child process spawns");
        let pid = std::process::id();
        let (_, started) = process_info(pid).expect("this test's own process is readable");
        let bindings = bindings_for(ProcessIdentity {
            pid,
            started_ticks: Some(started),
        });
        assert!(
            check_peer(
                &bindings,
                &target_fixture(),
                caller(child.id()),
                true,
                local_uid(),
                &process_info
            )
            .is_ok()
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn a_bystander_outside_the_lineage_is_refused() {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 5 & echo $!")
            .output()
            .expect("an orphan process spawns");
        let pid: u32 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .expect("the orphan prints its pid");
        wait_detached_from_test(pid);

        let pid_self = std::process::id();
        let (_, started) = process_info(pid_self).expect("this test's own process is readable");
        let bindings = bindings_for(ProcessIdentity {
            pid: pid_self,
            started_ticks: Some(started),
        });
        let refused = check_peer(
            &bindings,
            &target_fixture(),
            caller(pid),
            true,
            local_uid(),
            &process_info,
        )
        .expect_err("a re-parented bystander is not a descendant of the agent");
        let Response::Error { message, .. } = &refused else {
            panic!("an error response is expected");
        };
        assert!(message.contains("descend"), "{message}");
    }

    #[test]
    fn a_reused_pid_with_a_different_start_time_is_refused() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("a child process spawns");
        // The child's pid with a start time that belongs to no real process:
        // exactly what a reused pid looks like to the check.
        let bindings = bindings_for(ProcessIdentity {
            pid: child.id(),
            started_ticks: Some(1),
        });
        assert!(
            check_peer(
                &bindings,
                &target_fixture(),
                caller(child.id()),
                true,
                local_uid(),
                &process_info
            )
            .is_err()
        );
        let _ = child.kill();
        let _ = child.wait();
    }
}
