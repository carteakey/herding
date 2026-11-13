use super::*;
use sheprd_orchestrator::team_store::{Author, Member, SessionTarget, Task, TaskStatus};

#[test]
fn failed_submission_does_not_block_future_refreshes() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        ..Bridge::default()
    };
    bridge.refresh("team");
    assert_eq!(bridge.snapshot_in_flight, None);
}

#[test]
fn stale_snapshot_does_not_clear_or_replace_newer_request() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        snapshot_in_flight: Some(2),
        ..Bridge::default()
    };
    bridge.pending.insert(
        1,
        Pending::Snapshot {
            team_id: "team".into(),
        },
    );
    bridge.complete(Completion {
        request_id: 1,
        result: Ok(Reply::Snapshot {
            team: None,
            goal: None,
            posts: Vec::new(),
            tasks: Vec::new(),
            findings: Vec::new(),
            task_older: None,
            finding_older: None,
        }),
    });
    assert_eq!(bridge.snapshot_in_flight, Some(2));
    assert!(!bridge.dirty);
}

#[test]
fn add_worker_rejects_duplicate_pane_or_session() {
    let mut bridge = Bridge {
        selected: Some("team1".into()),
        team: Some(Team {
            team_id: "team1".into(),
            name: "team1".into(),
            supervisor: Member {
                member_id: "sup".into(),
                display_name: "supervisor".into(),
                model: None,
                account: None,
                target: SessionTarget {
                    host: "local".into(),
                    session: "sess-1".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p1".into()),
                },
            },
            members: Vec::new(),
        }),
        ..Bridge::default()
    };

    let dup_pane = PaneCandidate {
        identity: PaneIdentity {
            daemon_id: "local".into(),
            session_id: Some("sess-2".into()),
            workspace_id: "ws".into(),
            pane_id: "p1".into(),
        },
        label: "dup-pane".into(),
        agent: "opencode".into(),
        status: "idle".into(),
    };

    let candidates = vec![dup_pane.clone()];
    bridge.act(
        TeamAction::AddWorker {
            team_id: "team1".into(),
            name: "worker2".into(),
            pane: dup_pane.identity,
        },
        &candidates,
    );

    assert!(bridge.warning.is_some());
    assert!(bridge.team.as_ref().is_some_and(|t| t.members.is_empty()));

    let dup_session = PaneCandidate {
        identity: PaneIdentity {
            daemon_id: "local".into(),
            session_id: Some("sess-1".into()),
            workspace_id: "ws".into(),
            pane_id: "p2".into(),
        },
        label: "dup-session".into(),
        agent: "opencode".into(),
        status: "idle".into(),
    };
    let candidates2 = vec![dup_session.clone()];
    bridge.act(
        TeamAction::AddWorker {
            team_id: "team1".into(),
            name: "worker3".into(),
            pane: dup_session.identity,
        },
        &candidates2,
    );
    assert!(bridge.warning.is_some());
    assert!(bridge.team.as_ref().is_some_and(|t| t.members.is_empty()));
}

#[test]
fn pause_queue_is_bounded_and_scoped_per_team() {
    let mut bridge = Bridge::default();
    for i in 0..100 {
        bridge.enqueue_prompt(
            "team1".into(),
            "boot".into(),
            format!("pane-{i}"),
            format!("prompt-{i}"),
            format!("summary-{i}"),
            Vec::new(),
        );
    }
    assert_eq!(bridge.queued_prompts.len(), PAUSE_QUEUE_CAPACITY);
    assert_eq!(bridge.queued_prompts[0].pane_id, "pane-36");
    assert_eq!(
        bridge.queued_prompts[PAUSE_QUEUE_CAPACITY - 1].pane_id,
        "pane-99"
    );
}

#[test]
fn drain_unpaused_prompts_retains_prompts_when_team_is_none() {
    let mut bridge = Bridge::default();
    bridge.enqueue_prompt(
        "team1".into(),
        "boot".into(),
        "p1".into(),
        "prompt1".into(),
        "summary1".into(),
        Vec::new(),
    );
    assert_eq!(bridge.team, None);
    let drained = bridge.drain_unpaused_prompts();
    assert!(drained.is_empty());
    assert_eq!(bridge.queued_prompts.len(), 1);
}

#[test]
fn select_team_does_not_flush_paused_prompts() {
    let mut bridge = Bridge {
        selected: Some("team1".into()),
        team: Some(Team {
            team_id: "team1".into(),
            name: "team1".into(),
            supervisor: Member {
                member_id: "sup".into(),
                display_name: "supervisor".into(),
                model: None,
                account: None,
                target: SessionTarget {
                    host: "local".into(),
                    session: "sess-1".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p1".into()),
                },
            },
            members: Vec::new(),
        }),
        ..Bridge::default()
    };
    bridge.paused_members.insert("sup".into());
    bridge.enqueue_prompt(
        "team1".into(),
        "boot".into(),
        "p1".into(),
        "prompt1".into(),
        "summary1".into(),
        Vec::new(),
    );

    bridge.act(
        TeamAction::SelectTeam {
            team_id: "team2".into(),
        },
        &[],
    );
    assert_eq!(bridge.team, None);

    let drained = bridge.drain_unpaused_prompts();
    assert!(drained.is_empty());
    assert_eq!(bridge.queued_prompts.len(), 1);
}

#[test]
fn toggle_pause_team_resolves_team_when_selected_is_none() {
    let mut bridge = Bridge {
        selected: None,
        team: Some(Team {
            team_id: "team-a".into(),
            name: "Team A".into(),
            supervisor: Member {
                member_id: "sup".into(),
                display_name: "supervisor".into(),
                model: None,
                account: None,
                target: SessionTarget {
                    host: "local".into(),
                    session: "sess-1".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p1".into()),
                },
            },
            members: Vec::new(),
        }),
        ..Bridge::default()
    };

    assert!(!bridge.is_team_paused());
    assert!(!bridge.snapshot(&[]).paused);

    bridge.act(TeamAction::TogglePauseTeam, &[]);

    assert_eq!(bridge.selected, Some("team-a".into()));
    assert!(bridge.paused_teams.contains("team-a"));
    assert!(bridge.is_team_paused());
    assert!(bridge.dirty);
    assert!(bridge.snapshot(&[]).paused);

    bridge.act(TeamAction::TogglePauseTeam, &[]);
    assert!(!bridge.paused_teams.contains("team-a"));
    assert!(!bridge.is_team_paused());
    assert!(!bridge.snapshot(&[]).paused);
}

#[test]
fn is_pane_paused_for_team_retains_paused_status_when_switching_teams() {
    let mut bridge = Bridge {
        selected: Some("team1".into()),
        team: Some(Team {
            team_id: "team1".into(),
            name: "team1".into(),
            supervisor: Member {
                member_id: "sup".into(),
                display_name: "supervisor".into(),
                model: None,
                account: None,
                target: SessionTarget {
                    host: "local".into(),
                    session: "sess-1".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p1".into()),
                },
            },
            members: Vec::new(),
        }),
        ..Bridge::default()
    };

    bridge.act(TeamAction::TogglePauseTeam, &[]);
    assert!(bridge.is_team_id_paused("team1"));
    assert!(bridge.is_pane_paused_for_team("team1", "p1"));

    // Switch to team list view
    bridge.act(TeamAction::SwitchTeamView, &[]);
    assert_eq!(bridge.selected, None);
    assert_eq!(bridge.team, None);

    // Verify team1 is still treated as paused for incoming mutations
    assert!(bridge.is_team_id_paused("team1"));
    assert!(bridge.is_pane_paused_for_team("team1", "p1"));

    // Select team2
    bridge.act(
        TeamAction::SelectTeam {
            team_id: "team2".into(),
        },
        &[],
    );
    assert_eq!(bridge.selected, Some("team2".into()));

    // team1 still paused, team2 is not paused
    assert!(bridge.is_pane_paused_for_team("team1", "p1"));
    assert!(!bridge.is_pane_paused_for_team("team2", "p1"));
}

fn fallback_team() -> Team {
    Team {
        team_id: "t1".into(),
        name: "team1".into(),
        supervisor: Member {
            member_id: "sup".into(),
            display_name: "supervisor".into(),
            model: None,
            account: None,
            target: SessionTarget {
                host: "local".into(),
                session: "sess-1".into(),
                workspace: Some("ws".into()),
                pane: Some("p_sup".into()),
            },
        },
        members: Vec::new(),
    }
}

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

/// Unwraps a fallback decision with a plain panic; this file keeps
/// clippy::expect_used denied.
fn must_wake(bridge: &mut Bridge, now: Instant) -> SupervisorWake {
    match bridge.supervisor_fallback_due(now) {
        Some(wake) => wake,
        None => panic!("expected a supervisor wake"),
    }
}

fn stored_task(status: TaskStatus, updated_secs: i64) -> Task {
    Task {
        task_id: "task1".into(),
        team_id: "t1".into(),
        creator: Author::Human {
            user_id: "human".into(),
        },
        title: "Write report".into(),
        brief: String::new(),
        assignee: Some("sup".into()),
        status,
        blocker_reason: None,
        result: None,
        created_at: at(0),
        updated_at: at(updated_secs),
    }
}

#[test]
fn fallback_wakes_each_unseen_result_once() {
    let mut bridge = Bridge {
        selected: Some("t1".into()),
        team: Some(fallback_team()),
        tasks: vec![stored_task(TaskStatus::Done, 10)],
        ..Bridge::default()
    };
    let now = Instant::now();

    let wake = must_wake(&mut bridge, now);
    assert_eq!(wake.team_id, "t1");
    assert_eq!(wake.pane_id, "p_sup");
    assert_eq!(wake.signals.len(), 1);
    assert!(wake.prompt.contains("Write report"));
    bridge.mark_supervisor_woken("t1", wake.signals);

    // Still within the interval: bounded to one attempt.
    assert!(bridge.supervisor_fallback_due(now).is_none());
    // After the interval the reviewed board is silent: a finished goal
    // stops unattended activity.
    assert!(
        bridge
            .supervisor_fallback_due(now + SUPERVISOR_FALLBACK)
            .is_none()
    );
}

#[test]
fn fallback_wakes_a_new_result_after_the_interval() {
    let mut bridge = Bridge {
        selected: Some("t1".into()),
        team: Some(fallback_team()),
        tasks: vec![stored_task(TaskStatus::Done, 10)],
        ..Bridge::default()
    };
    let now = Instant::now();
    let wake = must_wake(&mut bridge, now);
    bridge.mark_supervisor_woken("t1", wake.signals);

    bridge.tasks.push(stored_task(TaskStatus::Blocked, 20));
    let wake = must_wake(&mut bridge, now + SUPERVISOR_FALLBACK);
    assert_eq!(wake.signals.len(), 1);
    assert!(wake.prompt.contains("blocked"));
}

#[test]
fn fallback_wakes_a_reopened_result_again() {
    let mut bridge = Bridge {
        selected: Some("t1".into()),
        team: Some(fallback_team()),
        tasks: vec![stored_task(TaskStatus::Done, 10)],
        ..Bridge::default()
    };
    let now = Instant::now();
    let wake = must_wake(&mut bridge, now);
    bridge.mark_supervisor_woken("t1", wake.signals);

    // The same task was reopened and finished again: a new stored state.
    bridge.tasks[0].updated_at = at(20);
    assert!(
        bridge
            .supervisor_fallback_due(now + SUPERVISOR_FALLBACK)
            .is_some()
    );
}

#[test]
fn fallback_is_silent_while_the_team_is_paused() {
    let mut bridge = Bridge {
        selected: Some("t1".into()),
        team: Some(fallback_team()),
        tasks: vec![stored_task(TaskStatus::Done, 10)],
        ..Bridge::default()
    };
    bridge.paused_teams.insert("t1".into());
    let now = Instant::now();

    assert!(bridge.supervisor_fallback_due(now).is_none());
    // Pausing did not even consume an attempt slot.
    assert_eq!(bridge.last_fallback, None);
    // The wake is not lost: unpausing and waiting lets it fire.
    bridge.paused_teams.clear();
    assert!(
        bridge
            .supervisor_fallback_due(now + SUPERVISOR_FALLBACK)
            .is_some()
    );
}

#[test]
fn fallback_reports_a_supervisor_without_pane_instead_of_waking() {
    let mut team = fallback_team();
    team.supervisor.target.pane = None;
    let mut bridge = Bridge {
        selected: Some("t1".into()),
        team: Some(team),
        tasks: vec![stored_task(TaskStatus::Done, 10)],
        ..Bridge::default()
    };
    let now = Instant::now();

    assert!(bridge.supervisor_fallback_due(now).is_none());
    let activity = bridge.last_activity.clone().unwrap_or_default();
    assert!(activity.contains("pending"), "activity: {activity}");
    // The result stays unseen, and the attempt was still rate-limited.
    assert!(!bridge.wake_seen("t1", "task1|done|10000"));
    assert!(
        bridge
            .supervisor_fallback_due(now + SUPERVISOR_FALLBACK)
            .is_none()
    );
}

#[test]
fn deleting_a_team_drops_its_wake_ledger() {
    let mut bridge = Bridge::default();
    bridge.mark_supervisor_woken("t1", ["task1|done|0"]);
    bridge.mark_supervisor_woken("t2", ["task1|done|0"]);

    bridge.act(
        TeamAction::DeleteTeam {
            team_id: "t1".into(),
        },
        &[],
    );

    assert!(!bridge.supervisor_wakes.contains_key("t1"));
    assert!(bridge.supervisor_wakes.contains_key("t2"));
}

#[test]
fn wake_ledger_is_bounded() {
    let mut bridge = Bridge::default();
    for i in 0..(WAKE_LEDGER_CAPACITY + 16) {
        bridge.mark_supervisor_woken("t1", [format!("task{i}|done|0")]);
    }
    let ledger = &bridge.supervisor_wakes["t1"];
    assert_eq!(ledger.len(), WAKE_LEDGER_CAPACITY);
    assert_eq!(ledger.front().map(String::as_str), Some("task16|done|0"));
}

fn store_task(id: &str) -> Task {
    Task {
        task_id: id.into(),
        team_id: "team".into(),
        creator: Author::Human {
            user_id: "human".into(),
        },
        title: id.into(),
        brief: String::new(),
        assignee: None,
        status: TaskStatus::Open,
        blocker_reason: None,
        result: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn an_older_task_page_prepends_and_advances_the_cursor() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        tasks_older: Some(31),
        ..Bridge::default()
    };
    bridge.pending.insert(
        7,
        Pending::EarlierTasks {
            team_id: "team".into(),
            before: 31,
        },
    );
    bridge.complete(Completion {
        request_id: 7,
        result: Ok(Reply::TaskPage {
            tasks: vec![(1, store_task("task-0000")), (2, store_task("task-0001"))],
            older: Some(3),
        }),
    });
    assert_eq!(bridge.tasks.len(), 2);
    assert_eq!(
        bridge.tasks[0].task_id, "task-0000",
        "older rows land first"
    );
    assert_eq!(bridge.tasks_older, Some(3));
}

#[test]
fn an_older_page_for_another_team_is_dropped() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        tasks_older: Some(31),
        ..Bridge::default()
    };
    bridge.pending.insert(
        7,
        Pending::EarlierTasks {
            team_id: "other".into(),
            before: 31,
        },
    );
    bridge.complete(Completion {
        request_id: 7,
        result: Ok(Reply::TaskPage {
            tasks: vec![(1, store_task("task-0000"))],
            older: Some(3),
        }),
    });
    assert!(bridge.tasks.is_empty(), "a stale page never merges");
    assert_eq!(bridge.tasks_older, Some(31));
}

#[test]
fn a_failed_older_page_restores_the_cursor() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        ..Bridge::default()
    };
    bridge.pending.insert(
        7,
        Pending::EarlierTasks {
            team_id: "team".into(),
            before: 31,
        },
    );
    bridge.complete(Completion {
        request_id: 7,
        result: Err(std::sync::Arc::new(
            crate::control::server::team_worker::Error::External("store went away".into()),
        )),
    });
    assert_eq!(
        bridge.tasks_older,
        Some(31),
        "the show-earlier control survives a failed fetch"
    );
}

#[test]
fn load_earlier_without_a_service_keeps_the_cursor_available() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        tasks_older: Some(31),
        ..Bridge::default()
    };
    bridge.act(TeamAction::LoadEarlierTasks, &[]);
    assert_eq!(bridge.tasks_older, Some(31));
}
