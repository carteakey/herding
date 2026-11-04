#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};

#[test]
fn team_requests_have_stable_wire_forms() {
    let requests = [
        Request::Team(Call {
            caller: Caller::default(),
            op: TeamOp::Register {
                team_id: "team-1".into(),
                name: "builder".into(),
            },
        }),
        Request::Team(Call {
            caller: Caller::default(),
            op: TeamOp::Roster {
                team_id: "team-1".into(),
            },
        }),
        Request::Team(Call {
            caller: Caller::default(),
            op: TeamOp::BulletinList {
                team_id: "team-1".into(),
                after: Some(4),
            },
        }),
        Request::Team(Call {
            caller: Caller::default(),
            op: TeamOp::BulletinPost {
                team_id: "team-1".into(),
                reply_to: Some("msg-1".into()),
                mentions: vec!["member-2".into()],
                text: "ready".into(),
            },
        }),
        Request::Team(Call {
            caller: Caller::default(),
            op: TeamOp::BulletinAck {
                team_id: "team-1".into(),
                message_id: "msg-1".into(),
            },
        }),
    ];
    for request in &requests {
        let text = serde_json::to_string(request).unwrap();
        assert_eq!(&serde_json::from_str::<Request>(&text).unwrap(), request);
    }
    let text = serde_json::to_string(&requests[3]).unwrap();
    assert!(text.ends_with(r#""op":{"op":"bulletin_post","team_id":"team-1","reply_to":"msg-1","mentions":["member-2"],"text":"ready"}}"#));
}

#[test]
fn team_requests_reject_unknown_operations_and_fields() {
    for text in [
        r#"{"method":"team","caller":{},"op":{"op":"create","name":"x"}}"#,
        r#"{"method":"team","caller":{},"op":{"op":"delete","team_id":"x"}}"#,
        r#"{"method":"team","caller":{},"op":{"op":"roster","team_id":"x","all":true}}"#,
        r#"{"method":"team","caller":{},"op":{"op":"bulletin_ack","team_id":"x"}}"#,
    ] {
        assert!(serde_json::from_str::<Request>(text).is_err(), "{text}");
    }
}

#[test]
fn team_replies_round_trip_and_delivery_states_are_enum_values() {
    let team = TeamInfo {
        id: "team-1".into(),
        name: "review".into(),
    };
    let member = MemberInfo {
        id: "member-1".into(),
        name: "builder".into(),
        supervisor: false,
        agent: "codex".into(),
    };
    let entry = BulletinEntry {
        sequence: 7,
        id: "msg-1".into(),
        member_id: member.id.clone(),
        author: member.name.clone(),
        reply_to: Some("msg-0".into()),
        mentions: vec![member.id.clone()],
        text: "ready".into(),
        delivery_state: DeliveryState::Pending,
    };
    for reply in [
        Reply::Registered {
            member: Box::new(member.clone()),
        },
        Reply::Roster {
            team: team.clone(),
            members: vec![member.clone()],
        },
        Reply::Bulletin {
            entries: vec![entry.clone()],
            next: Some(7),
        },
        Reply::Posted {
            entry: Box::new(entry),
        },
        Reply::Acknowledged {
            message_id: "msg-1".into(),
            delivery_state: DeliveryState::Acknowledged,
        },
    ] {
        let response = Response::Team { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
    for (state, wire) in [
        (DeliveryState::NotMentioned, "not_mentioned"),
        (DeliveryState::Pending, "pending"),
        (DeliveryState::Acknowledged, "acknowledged"),
    ] {
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            format!("\"{wire}\"")
        );
        assert_eq!(
            serde_json::from_str::<DeliveryState>(&format!("\"{wire}\"")).unwrap(),
            state
        );
    }
}

#[test]
fn task_finding_and_status_wire_forms_round_trip() {
    let ops = [
        TeamOp::Status {
            team_id: Some("team-1".into()),
        },
        TeamOp::GoalSet {
            team_id: "team-1".into(),
            goal: "Investigate auth protocol".into(),
        },
        TeamOp::TaskList {
            team_id: "team-1".into(),
            before: None,
            limit: None,
        },
        TeamOp::TaskCreate {
            team_id: "team-1".into(),
            title: "Map sections".into(),
            brief: "Check ELF headers".into(),
            assignee: Some("worker-1".into()),
        },
        TeamOp::TaskAssign {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
            assignee: "worker-2".into(),
        },
        TeamOp::TaskClaim {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
        },
        TeamOp::TaskBlock {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
            reason: "Needs header format".into(),
        },
        TeamOp::TaskComplete {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
            result: "Header magic 0x4852".into(),
        },
        TeamOp::TaskCancel {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
        },
        TeamOp::FindingList {
            team_id: "team-1".into(),
            before: Some(12),
            limit: Some(50),
        },
        TeamOp::FindingAdd {
            team_id: "team-1".into(),
            task_id: Some("task-1".into()),
            confidence: ConfidenceLevel::High,
            evidence: "bin/auth:0x1040".into(),
            supersedes: None,
            description: "Uses HMAC-SHA256".into(),
        },
    ];
    for op in ops {
        let req = Request::Team(Call {
            caller: Caller::default(),
            op,
        });
        let text = serde_json::to_string(&req).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), req);
    }

    let task = TaskInfo {
        id: "task-1".into(),
        title: "Map sections".into(),
        brief: "Check ELF".into(),
        creator: "lead".into(),
        assignee_id: Some("w1".into()),
        assignee: Some("worker".into()),
        status: TaskState::InProgress,
        blocker_reason: None,
        result: None,
    };
    let finding = FindingInfo {
        id: "find-1".into(),
        author_id: "w1".into(),
        author: "worker".into(),
        task_id: Some("task-1".into()),
        description: "Uses HMAC".into(),
        evidence: "0x1040".into(),
        confidence: ConfidenceLevel::High,
        supersedes: None,
    };
    for reply in [
        Reply::Status {
            team: TeamInfo {
                id: "team-1".into(),
                name: "Auth".into(),
            },
            goal: Some("Reverse engineer".into()),
            member: Box::new(MemberInfo {
                id: "w1".into(),
                name: "worker".into(),
                supervisor: false,
                agent: "zcode".into(),
            }),
            pending_mentions: 2,
            open_tasks: 1,
            assigned_tasks: 1,
        },
        Reply::GoalUpdated {
            team_id: "team-1".into(),
            goal: Some("Reverse engineer".into()),
        },
        Reply::Tasks {
            tasks: vec![task.clone()],
            next_before: Some(12),
        },
        Reply::Task {
            task: Box::new(task),
        },
        Reply::Findings {
            findings: vec![finding.clone()],
            next_before: None,
        },
        Reply::Finding {
            finding: Box::new(finding),
        },
    ] {
        let res = Response::Team { reply };
        let text = serde_json::to_string(&res).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), res);
    }
}
