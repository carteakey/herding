//! Team deletion is a Herdr UI action behind a confirmation banner. Nothing on
//! the control wire removes a team, so an agent-facing call cannot skip that
//! confirmation, and no flag can smuggle one into an operation that does exist.
#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

/// Names a caller might guess for a destructive team operation. None of them is
/// an operation the protocol knows.
const DESTRUCTIVE_NAMES: &[&str] = &[
    "delete",
    "delete_team",
    "remove",
    "remove_team",
    "drop_team",
    "destroy",
];

/// Classifies the whole agent-facing surface. The match is exhaustive on
/// purpose: a new `TeamOp` variant has to be judged here - is it a read, or a
/// write that keeps the team - before it can ship.
fn team_op_keeps_the_team(op: &TeamOp) -> bool {
    match op {
        TeamOp::Status { .. }
        | TeamOp::Roster { .. }
        | TeamOp::TaskList { .. }
        | TeamOp::FindingList { .. }
        | TeamOp::BulletinList { .. } => true,
        TeamOp::Register { .. }
        | TeamOp::GoalSet { .. }
        | TeamOp::BulletinPost { .. }
        | TeamOp::BulletinAck { .. }
        | TeamOp::TaskCreate { .. }
        | TeamOp::TaskAssign { .. }
        | TeamOp::TaskClaim { .. }
        | TeamOp::TaskBlock { .. }
        | TeamOp::TaskComplete { .. }
        | TeamOp::TaskCancel { .. }
        | TeamOp::FindingAdd { .. } => true,
    }
}

#[test]
fn no_team_operation_deletes_a_team() {
    let ops = [
        TeamOp::Status {
            team_id: Some("team-1".into()),
        },
        TeamOp::Register {
            team_id: "team-1".into(),
            name: "builder".into(),
        },
        TeamOp::Roster {
            team_id: "team-1".into(),
        },
        TeamOp::GoalSet {
            team_id: "team-1".into(),
            goal: "ship it".into(),
        },
        TeamOp::BulletinList {
            team_id: "team-1".into(),
            after: Some(4),
        },
        TeamOp::BulletinPost {
            team_id: "team-1".into(),
            reply_to: None,
            mentions: Vec::new(),
            text: "hello".into(),
        },
        TeamOp::BulletinAck {
            team_id: "team-1".into(),
            message_id: "m-1".into(),
        },
        TeamOp::TaskList {
            team_id: "team-1".into(),
        },
        TeamOp::TaskCreate {
            team_id: "team-1".into(),
            title: "Write report".into(),
            brief: String::new(),
            assignee: None,
        },
        TeamOp::TaskAssign {
            team_id: "team-1".into(),
            task_id: "t-1".into(),
            assignee: "member-1".into(),
        },
        TeamOp::TaskClaim {
            team_id: "team-1".into(),
            task_id: "t-1".into(),
        },
        TeamOp::TaskBlock {
            team_id: "team-1".into(),
            task_id: "t-1".into(),
            reason: "waiting".into(),
        },
        TeamOp::TaskComplete {
            team_id: "team-1".into(),
            task_id: "t-1".into(),
            result: "done".into(),
        },
        TeamOp::TaskCancel {
            team_id: "team-1".into(),
            task_id: "t-1".into(),
        },
        TeamOp::FindingList {
            team_id: "team-1".into(),
        },
        TeamOp::FindingAdd {
            team_id: "team-1".into(),
            task_id: None,
            confidence: ConfidenceLevel::Medium,
            evidence: "A probe showed it.".into(),
            supersedes: None,
            description: "A gap".into(),
        },
    ];
    for op in &ops {
        assert!(
            team_op_keeps_the_team(op),
            "a team operation may change rows, but none may remove the team: {op:?}"
        );
    }
}

#[test]
fn a_caller_cannot_name_a_team_deletion_on_the_wire() {
    for name in DESTRUCTIVE_NAMES {
        let request = json!({ "op": name, "team_id": "team-1" });
        assert!(
            serde_json::from_value::<TeamOp>(request.clone()).is_err(),
            "{name} should not decode as a team operation"
        );
    }
}

/// Deleting a team is not a flag on an operation either: `TeamOp` declares its
/// fields and refuses the rest.
#[test]
fn a_team_operation_refuses_a_field_it_never_declared() {
    let smuggled = json!({
        "op": "status",
        "team_id": "team-1",
        "delete_team": true,
    });
    assert!(
        serde_json::from_value::<TeamOp>(smuggled).is_err(),
        "an undeclared field must not reach a team operation"
    );
}
