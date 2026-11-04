#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::dispatch::prompts_for_scoped_reply;
use super::identity::{AgentProcess, MemberBinding, PeerBindings, check_peer, proc_stat};
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
/// the peer-credential binding recorded at registration, checked by
/// `check_peer` on top of the proof. The old baseline
/// (`a_caller_naming_a_live_pane_is_treated_as_that_pane`) pinned the
/// pre-hardening behavior; this pins the new one, per side.
#[gpui::test]
fn a_caller_naming_a_live_pane_needs_its_process_identity(cx: &mut gpui::TestAppContext) {
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
        let bystander = Caller {
            pane_id: Some("w0:p1".into()),
            workspace_id: Some("w0".into()),
            ..Caller::default()
        };
        let (target, endpoint) = current_identity(view, &bystander)
            .expect("the proof relies on the asserted pane id alone, by design");
        assert_eq!(endpoint, 0);
        assert_eq!(target.pane.as_deref(), Some("w0:p1"));

        let uid = nix::unistd::geteuid().as_raw();
        let gid = nix::unistd::getgid().as_raw();
        let agent = PeerCredentials {
            uid,
            gid,
            pid: Some(424242),
        };
        let bystander_cred = PeerCredentials {
            uid,
            gid,
            pid: Some(999999),
        };
        let bindings = [(
            target.clone(),
            MemberBinding {
                credentials: agent,
                agent: None,
            },
        )]
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();

        // A same-user bystander without the agent's process identity is
        // refused where it used to be treated as the pane's member.
        assert!(check_peer(&bindings, &target, Some(bystander_cred), true, Some(uid)).is_err());
        // No binding at all (a UI restart clears them) is refused with
        // register-again guidance.
        let empty = PeerBindings::default();
        let refused = check_peer(&empty, &target, Some(agent), true, Some(uid)).unwrap_err();
        let Response::Error { message, .. } = &refused else {
            panic!("an error response is expected");
        };
        assert!(message.contains("register"), "{message}");
        // The registered agent process is allowed.
        assert!(check_peer(&bindings, &target, Some(agent), true, Some(uid)).is_ok());
        // The tunnel's sshd peer (root) is the documented downgrade.
        let tunnel = PeerCredentials {
            uid: 0,
            gid: 0,
            pid: Some(1),
        };
        assert!(check_peer(&empty, &target, Some(tunnel), true, Some(uid)).is_ok());
        // Another local user is refused, and the Windows transport, which
        // cannot prove a process identity, degrades to its pipe DACL.
        let other = PeerCredentials {
            uid: uid + 1,
            gid,
            pid: Some(2),
        };
        assert!(check_peer(&empty, &target, Some(other), true, Some(uid)).is_err());
        assert!(check_peer(&empty, &target, None, false, None).is_ok());
    });
}

/// The agent-lineage contract (issue #3): registration anchors the pane's
/// agent process, and a caller is accepted when it descends from that exact
/// process, start time included. Linux only: peer credentials carry no pid on
/// macOS, which keeps the user check, and `/proc` does not exist there.
#[cfg(target_os = "linux")]
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

    fn binding_for(agent: AgentProcess) -> MemberBinding {
        MemberBinding {
            credentials: PeerCredentials {
                uid: nix::unistd::geteuid().as_raw(),
                gid: nix::unistd::getgid().as_raw(),
                pid: None,
            },
            agent: Some(agent),
        }
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
                let Some((parent, _)) = proc_stat(current) else {
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
        let started = proc_stat(pid)
            .expect("this test's own process is readable")
            .1;
        let bindings = [(target_fixture(), binding_for(AgentProcess { pid, started }))]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        assert!(check_peer(&bindings, &target_fixture(), caller(pid), true, local_uid()).is_ok());
    }

    #[test]
    fn a_descendant_of_the_agent_is_accepted() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("a child process spawns");
        let pid = std::process::id();
        let (_, started) = proc_stat(pid).expect("this test's own process is readable");
        let bindings = [(target_fixture(), binding_for(AgentProcess { pid, started }))]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        assert!(
            check_peer(
                &bindings,
                &target_fixture(),
                caller(child.id()),
                true,
                local_uid()
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
        let (_, started) = proc_stat(pid_self).expect("this test's own process is readable");
        let bindings = [(
            target_fixture(),
            binding_for(AgentProcess {
                pid: pid_self,
                started,
            }),
        )]
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
        let refused = check_peer(&bindings, &target_fixture(), caller(pid), true, local_uid())
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
        let bindings = [(
            target_fixture(),
            binding_for(AgentProcess {
                pid: child.id(),
                started: 1,
            }),
        )]
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
        assert!(
            check_peer(
                &bindings,
                &target_fixture(),
                caller(child.id()),
                true,
                local_uid()
            )
            .is_err()
        );
        let _ = child.kill();
        let _ = child.wait();
    }
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
