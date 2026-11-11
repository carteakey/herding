#![allow(clippy::unwrap_used)]

use super::*;

fn member(id: &str, name: &str, session: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: name.into(),
        target: SessionTarget {
            host: "local".into(),
            session: session.into(),
            workspace: Some("ws-1".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

fn setup_team(store: &mut TeamStore) -> Team {
    let team = Team {
        team_id: "team-1".into(),
        name: "Binary Auth".into(),
        supervisor: member("sup-1", "Shepherd", "sess-sup"),
        members: vec![
            member("w-1", "Protocol", "sess-w1"),
            member("w-2", "Tracer", "sess-w2"),
        ],
    };
    store.save_team(&team).unwrap();
    team
}

#[test]
fn task_lifecycle_atomic_claim_assignment_and_completion() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    let _team = setup_team(&mut store);
    let sup = Author::Member {
        member_id: "sup-1".into(),
    };
    let w1 = Author::Member {
        member_id: "w-1".into(),
    };
    let w2 = Author::Member {
        member_id: "w-2".into(),
    };

    // Unassigned open task can be claimed by only one worker.
    let t1 = store
        .create_task(
            "t1",
            "team-1",
            &sup,
            "Map ELF sections",
            "Inspect binary headers",
            None,
        )
        .unwrap();
    assert_eq!(t1.status, TaskStatus::Open);
    assert_eq!(t1.assignee, None);

    let claimed = store.claim_task("team-1", "t1", "w-1").unwrap();
    assert_eq!(claimed.status, TaskStatus::InProgress);
    assert_eq!(claimed.assignee.as_deref(), Some("w-1"));

    // Second worker cannot claim an already-claimed task.
    assert!(matches!(
        store.claim_task("team-1", "t1", "w-2"),
        Err(Error::TaskUnavailable(_))
    ));

    // Assigned open task can only be claimed by the assigned worker.
    let t2 = store
        .create_task(
            "t2",
            "team-1",
            &sup,
            "Trace auth handshake",
            "",
            Some("w-2"),
        )
        .unwrap();
    assert_eq!(t2.status, TaskStatus::Open);
    assert_eq!(t2.assignee.as_deref(), Some("w-2"));
    assert!(matches!(
        store.claim_task("team-1", "t2", "w-1"),
        Err(Error::TaskUnavailable(_))
    ));
    let claimed_t2 = store.claim_task("team-1", "t2", "w-2").unwrap();
    assert_eq!(claimed_t2.status, TaskStatus::InProgress);

    // Block and reassign t2.
    let blocked = store
        .block_task("team-1", "t2", &w2, "Needs packet structure from Protocol")
        .unwrap();
    assert_eq!(blocked.status, TaskStatus::Blocked);
    assert_eq!(
        blocked.blocker_reason.as_deref(),
        Some("Needs packet structure from Protocol")
    );

    let reassigned = store.assign_task("team-1", "t2", &sup, "w-1").unwrap();
    assert_eq!(reassigned.status, TaskStatus::Open);
    assert_eq!(reassigned.assignee.as_deref(), Some("w-1"));
    assert_eq!(reassigned.blocker_reason, None);

    // Unassigned worker cannot complete w-1's in-progress task t1, but w-1 can.
    assert!(matches!(
        store.complete_task("team-1", "t1", &w2, "forged"),
        Err(Error::TaskNotPermitted { .. })
    ));
    let done = store
        .complete_task("team-1", "t1", &w1, "ELF entrypoint at 0x401000")
        .unwrap();
    assert_eq!(done.status, TaskStatus::Done);
    assert_eq!(done.result.as_deref(), Some("ELF entrypoint at 0x401000"));

    // Completed task cannot transition again.
    assert!(matches!(
        store.cancel_task("team-1", "t2", &sup).map(|t| t.status),
        Ok(TaskStatus::Cancelled)
    ));
    assert!(matches!(
        store.block_task("team-1", "t1", &sup, "late"),
        Err(Error::InvalidTaskTransition { .. })
    ));

    let tasks = store.tasks("team-1", 10).unwrap();
    assert_eq!(tasks.len(), 2);
}

#[test]
fn findings_record_confidence_task_links_and_non_destructive_supersession() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    let _team = setup_team(&mut store);
    let w1 = Author::Member {
        member_id: "w-1".into(),
    };
    let w2 = Author::Member {
        member_id: "w-2".into(),
    };
    store
        .create_task("t1", "team-1", &w1, "Parse packet", "", Some("w-1"))
        .unwrap();

    let f1 = store
        .add_finding(
            "f1",
            "team-1",
            &w1,
            Some("t1"),
            "Packet header uses 4-byte magic 0x48524452",
            "bin/auth:0x1040",
            Confidence::Medium,
            None,
        )
        .unwrap();
    let f2 = store
        .add_finding(
            "f2",
            "team-1",
            &w2,
            Some("t1"),
            "Packet header uses 4-byte magic followed by 2-byte version",
            "bin/auth:0x1040..0x1046",
            Confidence::High,
            Some(&f1.finding_id),
        )
        .unwrap();
    assert_eq!(f2.supersedes.as_deref(), Some("f1"));

    let list = store.findings("team-1", 10).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].finding_id, "f1");
    assert_eq!(list[1].finding_id, "f2");
    assert_eq!(list[1].confidence, Confidence::High);

    assert!(matches!(
        store.add_finding(
            "f3",
            "team-1",
            &w1,
            Some("missing-task"),
            "desc",
            "ev",
            Confidence::Low,
            None,
        ),
        Err(Error::TaskNotFound(_))
    ));
    assert!(matches!(
        store.add_finding(
            "f4",
            "team-1",
            &w1,
            None,
            "desc",
            "ev",
            Confidence::Low,
            Some("missing-finding"),
        ),
        Err(Error::FindingNotFound(_))
    ));
}

#[test]
fn team_goal_registration_discovery_and_mention_validation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    let team = setup_team(&mut store);

    assert_eq!(store.team_goal("team-1").unwrap(), None);
    store
        .set_team_goal("team-1", "Reverse engineer the binary auth handshake")
        .unwrap();
    assert_eq!(
        store.team_goal("team-1").unwrap().as_deref(),
        Some("Reverse engineer the binary auth handshake")
    );

    let new_target = SessionTarget {
        host: "local".into(),
        session: "sess-w3".into(),
        workspace: Some("ws-1".into()),
        pane: Some("pane-w-3".into()),
    };
    let registered = store
        .register_member("team-1", "w-3", "ZCode-1", &new_target, &[])
        .unwrap();
    assert_eq!(registered.display_name, "ZCode-1");

    // A resumed session in the same pane re-binds the name holder: the
    // member id survives, the target moves to the new session, and no
    // second member appears.
    let resumed_target = SessionTarget {
        session: "sess-w4".into(),
        ..new_target.clone()
    };
    let resumed = store
        .register_member("team-1", "w-4", "zcode-1", &resumed_target, &[])
        .unwrap();
    assert_eq!(resumed.member_id, "w-3");
    assert_eq!(resumed.display_name, "zcode-1");
    let rebound = store.team("team-1").unwrap().unwrap();
    assert_eq!(
        rebound
            .members
            .iter()
            .filter(|m| m.display_name.eq_ignore_ascii_case("zcode-1"))
            .count(),
        1
    );

    // The same name from a DIFFERENT pane is a different agent: rejected.
    let stolen_target = SessionTarget {
        session: "sess-w5".into(),
        pane: Some("pane-w-5".into()),
        ..new_target.clone()
    };
    assert!(matches!(
        store.register_member("team-1", "w-5", "zcode-1", &stolen_target, &[]),
        Err(Error::DuplicateDisplayName(_))
    ));

    // Session discovery finds the team without knowing team_id.
    let (found_team, found_member) = store
        .find_member_team(None, &resumed_target)
        .unwrap()
        .unwrap();
    assert_eq!(found_team.team_id, "team-1");
    assert_eq!(found_member.member_id, "w-3");

    // Mention resolution supports @supervisor, @team, display names, and rejects unknowns.
    let w1_author = Author::Member {
        member_id: "w-1".into(),
    };
    assert_eq!(
        resolve_recipients(&team, &w1_author, &[], "Hey @supervisor and @Tracer").unwrap(),
        Recipients::Explicit(vec!["sup-1".into(), "w-2".into()])
    );
    assert_eq!(
        resolve_recipients(&team, &w1_author, &[], "Broadcast to @team now").unwrap(),
        Recipients::All
    );
    assert!(matches!(
        resolve_recipients(&team, &w1_author, &[], "Ask @nobody about this"),
        Err(Error::UnknownMention(name)) if name == "nobody"
    ));
    // Email addresses in body are not treated as @mentions.
    assert_eq!(
        resolve_recipients(&team, &w1_author, &[], "Contact dev@example.com").unwrap(),
        Recipients::All
    );
}

#[test]
fn a_stale_boot_rebinds_a_pane_while_a_live_other_boot_stays_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    setup_team(&mut store);

    let target = SessionTarget {
        host: "boot-old".into(),
        session: "sess-w3".into(),
        workspace: Some("ws-1".into()),
        pane: Some("pane-w-3".into()),
    };
    store
        .register_member("team-1", "w-3", "ZCode-1", &target, &[])
        .unwrap();

    // The daemon rebooted: the caller proves the same workspace and pane
    // under a new boot id, and no live endpoint reports boot-old any more,
    // so the name holder re-binds with its member id intact.
    let restarted = SessionTarget {
        host: "boot-new".into(),
        session: "sess-w4".into(),
        ..target.clone()
    };
    let live = vec!["boot-new".to_owned()];
    let rebound = store
        .register_member("team-1", "w-9", "zcode-1", &restarted, &live)
        .unwrap();
    assert_eq!(rebound.member_id, "w-3");
    assert_eq!(rebound.display_name, "zcode-1");
    let roster = store.team("team-1").unwrap().unwrap();
    assert_eq!(
        roster
            .members
            .iter()
            .filter(|m| m.display_name.eq_ignore_ascii_case("zcode-1"))
            .count(),
        1,
        "the re-bind keeps one member under the name"
    );

    // A holder whose boot IS live keeps the name: the same pane id claimed
    // from another live boot is identity theft across the fleet, not drift.
    let stolen = SessionTarget {
        host: "boot-other".into(),
        session: "sess-w5".into(),
        ..target.clone()
    };
    let live = vec!["boot-new".to_owned(), "boot-other".to_owned()];
    assert!(matches!(
        store.register_member("team-1", "w-10", "zcode-1", &stolen, &live),
        Err(Error::DuplicateDisplayName(_))
    ));

    // A different pane is a different agent even when the holder's boot
    // has drifted away.
    let other_pane = SessionTarget {
        host: "boot-new".into(),
        session: "sess-w6".into(),
        pane: Some("pane-w-6".into()),
        ..target.clone()
    };
    let live = vec!["boot-new".to_owned()];
    assert!(matches!(
        store.register_member("team-1", "w-11", "zcode-1", &other_pane, &live),
        Err(Error::DuplicateDisplayName(_))
    ));
}
