//! Team deletion is the panel's confirmed action (`TeamAction::DeleteTeam`,
//! emitted only by the confirm banner). The agent-facing surface has no path to
//! it: every `ScopedAction` the team CLI can send is a read or a write that
//! keeps the team and its members.
#![allow(clippy::unwrap_used)]
use super::*;
use sheprd_orchestrator::team_store::{Confidence, Member, SessionTarget};

fn target(n: u8) -> SessionTarget {
    SessionTarget {
        host: "boot".into(),
        session: format!("session-{n}"),
        workspace: Some("w".into()),
        pane: Some(format!("p{n}")),
    }
}

fn store() -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("t.sqlite3")).unwrap();
    let member = |id: &str, n| Member {
        member_id: id.into(),
        display_name: id.into(),
        model: None,
        account: None,
        target: target(n),
    };
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "t".into(),
            supervisor: member("lead", 1),
            members: vec![member("worker", 2)],
        })
        .unwrap();
    (dir, store)
}

/// Name every action a caller can send from a shell. The match is exhaustive on
/// purpose: a new `ScopedAction` variant has to be classified here - is it a
/// read, or a write that keeps the team - before it can ship.
fn scoped_action_keeps_the_team(action: &ScopedAction) -> bool {
    match action {
        ScopedAction::Status { .. }
        | ScopedAction::Roster
        | ScopedAction::BulletinList { .. }
        | ScopedAction::TaskList { .. }
        | ScopedAction::FindingList { .. }
        | ScopedAction::Heartbeat { .. } => true,
        ScopedAction::Register { .. }
        | ScopedAction::GoalSet { .. }
        | ScopedAction::BulletinPost { .. }
        | ScopedAction::BulletinAck { .. }
        | ScopedAction::TaskCreate { .. }
        | ScopedAction::TaskAssign { .. }
        | ScopedAction::TaskClaim { .. }
        | ScopedAction::TaskBlock { .. }
        | ScopedAction::TaskComplete { .. }
        | ScopedAction::TaskCancel { .. }
        | ScopedAction::FindingAdd { .. } => true,
    }
}

/// Everything the CLI can ask for, sent by a real member of the team.
fn cli_actions() -> Vec<ScopedAction> {
    let post_id = uuid::Uuid::new_v4().to_string();
    let task_id = uuid::Uuid::new_v4().to_string();
    [
        ScopedAction::Status {
            team_id: Some("t".into()),
        },
        ScopedAction::Roster,
        ScopedAction::GoalSet {
            goal: "ship it".into(),
        },
        ScopedAction::BulletinList {
            after: Some(0),
            limit: 20,
        },
        ScopedAction::BulletinPost {
            post_id,
            reply_to: None,
            text: "working on it".into(),
            mentions: vec!["lead".into()],
        },
        ScopedAction::TaskList {
            before: None,
            limit: 20,
        },
        ScopedAction::TaskCreate {
            task_id: task_id.clone(),
            title: "Write report".into(),
            brief: String::new(),
            assignee: Some("worker".into()),
        },
        ScopedAction::TaskAssign {
            task_id: task_id.clone(),
            assignee: "worker".into(),
        },
        ScopedAction::TaskClaim {
            task_id: task_id.clone(),
        },
        ScopedAction::TaskBlock {
            task_id: task_id.clone(),
            reason: "waiting on a review".into(),
        },
        ScopedAction::TaskComplete {
            task_id,
            result: "report written".into(),
        },
        ScopedAction::FindingList {
            before: None,
            limit: 20,
        },
        ScopedAction::FindingAdd {
            finding_id: uuid::Uuid::new_v4().to_string(),
            task_id: None,
            description: "Deletion is not reachable from the CLI".into(),
            evidence: "Every ScopedAction keeps the team row.".into(),
            confidence: Confidence::High,
            supersedes: None,
        },
    ]
    .into_iter()
    .collect()
}

#[test]
fn every_command_line_action_keeps_the_team() {
    for action in cli_actions() {
        assert!(
            scoped_action_keeps_the_team(&action),
            "a team action may change rows, but none may remove the team"
        );
    }
}

#[test]
fn a_board_full_of_command_line_work_leaves_the_team_and_its_members() {
    let (_dir, mut store) = store();
    let worker = target(2);
    // Board rules may refuse one step of the walk (a completed task is not
    // claimable again); what must never happen is a team that stops existing.
    for action in cli_actions() {
        let _ = execute(
            &mut store,
            Operation::Scoped {
                team_id: "t".into(),
                caller: worker.clone(),
                caller_boot: worker.host.clone(),
                endpoint_ids: Vec::new(),
                live_panes: Vec::new(),
                action,
                live_hosts: Vec::new(),
            },
        );
    }

    let Reply::Snapshot {
        team,
        posts,
        tasks,
        findings,
        ..
    } = execute(
        &mut store,
        Operation::Snapshot {
            team_id: "t".into(),
            limit: 20,
        },
    )
    .unwrap()
    else {
        panic!("expected a snapshot");
    };
    let Some(team) = team else {
        panic!("the CLI never removes a team");
    };
    assert_eq!(team.supervisor.display_name, "lead");
    assert_eq!(
        team.members.len(),
        1,
        "no command line action unregisters a member"
    );
    assert_eq!(team.name, "t");
    // The rows the actions wrote are still there, so the run above really
    // exercised the board rather than refusing everything.
    assert!(!posts.is_empty());
    assert!(!tasks.is_empty());
    assert!(!findings.is_empty());
}

/// `Operation::DeleteTeam` exists for the panel's confirmed action and nothing
/// else: a caller that can only send `ScopedAction` cannot reach it.
#[test]
fn only_the_confirmed_panel_action_deletes_a_team() {
    let (_dir, mut store) = store();
    let mut scoped = |action: ScopedAction| {
        execute(
            &mut store,
            Operation::Scoped {
                team_id: "t".into(),
                caller: target(2),
                caller_boot: target(2).host,
                endpoint_ids: Vec::new(),
                live_panes: Vec::new(),
                action,
                live_hosts: Vec::new(),
            },
        )
    };
    assert!(
        scoped(ScopedAction::TaskCancel {
            task_id: "missing".into()
        })
        .is_err()
    );
    assert!(
        matches!(
            scoped(ScopedAction::GoalSet {
                goal: "still here".into()
            }),
            Ok(Reply::ScopedGoalUpdated { .. })
        ),
        "the caller can change the board"
    );
    let Reply::Snapshot { team, .. } = execute(
        &mut store,
        Operation::Snapshot {
            team_id: "t".into(),
            limit: 20,
        },
    )
    .unwrap() else {
        panic!("expected a snapshot");
    };
    assert!(team.is_some(), "no scoped action removed the team");

    execute(
        &mut store,
        Operation::DeleteTeam {
            team_id: "t".into(),
        },
    )
    .unwrap();
    let Reply::Snapshot { team, .. } = execute(
        &mut store,
        Operation::Snapshot {
            team_id: "t".into(),
            limit: 20,
        },
    )
    .unwrap() else {
        panic!("expected a snapshot");
    };
    assert!(
        team.is_none(),
        "the confirmed panel action is the one that deletes"
    );
}
