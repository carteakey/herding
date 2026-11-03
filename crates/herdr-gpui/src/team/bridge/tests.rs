use super::*;

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
    assert!(bridge.team.as_ref().unwrap().members.is_empty());

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
    assert!(bridge.team.as_ref().unwrap().members.is_empty());
}
