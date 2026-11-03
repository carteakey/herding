#![allow(clippy::unwrap_used)]
use super::*;
use std::ffi::OsString;

fn parse(words: &[&str]) -> TeamCommand {
    let mut args = Args::new(words.iter().map(OsString::from), "team");
    let result = team(&mut args).unwrap();
    args.finish().unwrap();
    result
}

#[test]
fn parses_roster_list_post_and_ack() {
    assert_eq!(
        parse(&["roster", "team-1"]),
        TeamCommand::Roster {
            team_id: "team-1".into()
        }
    );
    assert_eq!(
        parse(&["bulletin", "list", "team-1", "--after", "8"]),
        TeamCommand::BulletinList {
            team_id: "team-1".into(),
            after: Some(8)
        }
    );
    assert_eq!(
        parse(&[
            "bulletin",
            "post",
            "team-1",
            "--reply-to",
            "msg-2",
            "--mention",
            "member-1",
            "please",
            "review"
        ]),
        TeamCommand::BulletinPost {
            team_id: "team-1".into(),
            reply_to: Some("msg-2".into()),
            mentions: vec!["member-1".into()],
            text: "please review".into(),
        }
    );
    assert_eq!(
        parse(&["bulletin", "ack", "team-1", "msg-4"]),
        TeamCommand::BulletinAck {
            team_id: "team-1".into(),
            message_id: "msg-4".into()
        }
    );
    assert_eq!(
        parse(&["register", "team-1", "--name", "builder"]),
        TeamCommand::Register {
            team_id: "team-1".into(),
            name: "builder".into(),
        }
    );
}

#[test]
fn parses_status_goal_tasks_and_findings() {
    assert_eq!(parse(&["status"]), TeamCommand::Status { team_id: None });
    assert_eq!(
        parse(&["status", "team-1"]),
        TeamCommand::Status {
            team_id: Some("team-1".into())
        }
    );
    assert_eq!(
        parse(&["goal", "team-1", "Reverse", "engineer", "auth"]),
        TeamCommand::GoalSet {
            team_id: "team-1".into(),
            goal: "Reverse engineer auth".into(),
        }
    );
    assert_eq!(
        parse(&["task", "list", "team-1"]),
        TeamCommand::TaskList {
            team_id: "team-1".into()
        }
    );
    assert_eq!(
        parse(&[
            "task",
            "create",
            "team-1",
            "--title",
            "Map ELF",
            "--brief",
            "Inspect sections",
            "--assign",
            "worker-1"
        ]),
        TeamCommand::TaskCreate {
            team_id: "team-1".into(),
            title: "Map ELF".into(),
            brief: "Inspect sections".into(),
            assignee: Some("worker-1".into()),
        }
    );
    assert_eq!(
        parse(&["task", "assign", "team-1", "task-1", "--to", "worker-2"]),
        TeamCommand::TaskAssign {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
            assignee: "worker-2".into(),
        }
    );
    assert_eq!(
        parse(&["task", "claim", "team-1", "task-1"]),
        TeamCommand::TaskClaim {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
        }
    );
    assert_eq!(
        parse(&[
            "task",
            "block",
            "team-1",
            "task-1",
            "--reason",
            "Needs packet header"
        ]),
        TeamCommand::TaskBlock {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
            reason: "Needs packet header".into(),
        }
    );
    assert_eq!(
        parse(&[
            "task",
            "complete",
            "team-1",
            "task-1",
            "--result",
            "Found parser at 0x4010"
        ]),
        TeamCommand::TaskComplete {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
            result: "Found parser at 0x4010".into(),
        }
    );
    assert_eq!(
        parse(&["task", "cancel", "team-1", "task-1"]),
        TeamCommand::TaskCancel {
            team_id: "team-1".into(),
            task_id: "task-1".into(),
        }
    );
    assert_eq!(
        parse(&["finding", "list", "team-1"]),
        TeamCommand::FindingList {
            team_id: "team-1".into()
        }
    );
    assert_eq!(
        parse(&[
            "finding",
            "add",
            "team-1",
            "--task",
            "task-1",
            "--confidence",
            "high",
            "--evidence",
            "bin/auth:0x1040",
            "--supersedes",
            "find-0",
            "Packet",
            "uses",
            "HMAC"
        ]),
        TeamCommand::FindingAdd {
            team_id: "team-1".into(),
            task_id: Some("task-1".into()),
            confidence: ConfidenceLevel::High,
            evidence: "bin/auth:0x1040".into(),
            supersedes: Some("find-0".into()),
            description: "Packet uses HMAC".into(),
        }
    );
}

#[test]
fn finding_add_accepts_description_option_and_defaults_confidence() {
    assert_eq!(
        parse(&[
            "finding",
            "add",
            "team-1",
            "--description",
            "Packet uses HMAC",
            "--evidence",
            "bin/auth:0x1040",
        ]),
        TeamCommand::FindingAdd {
            team_id: "team-1".into(),
            task_id: None,
            confidence: ConfidenceLevel::Medium,
            evidence: "bin/auth:0x1040".into(),
            supersedes: None,
            description: "Packet uses HMAC".into(),
        }
    );
    assert_eq!(
        parse(&[
            "finding",
            "add",
            "team-1",
            "--evidence",
            "bin/auth:0x1040",
            "Trailing",
            "description",
        ]),
        TeamCommand::FindingAdd {
            team_id: "team-1".into(),
            task_id: None,
            confidence: ConfidenceLevel::Medium,
            evidence: "bin/auth:0x1040".into(),
            supersedes: None,
            description: "Trailing description".into(),
        }
    );
    let mut both = Args::new(
        [
            "finding",
            "add",
            "team-1",
            "--description",
            "one",
            "--evidence",
            "here",
            "two",
        ]
        .map(OsString::from),
        "team",
    );
    assert_eq!(
        team(&mut both).unwrap_err(),
        CliError::DuplicateOption("--description")
    );
    let mut neither = Args::new(
        ["finding", "add", "team-1", "--evidence", "here"].map(OsString::from),
        "team",
    );
    assert_eq!(
        team(&mut neither).unwrap_err(),
        CliError::MissingArgument {
            command: "team finding add",
            what: "DESCRIPTION",
        }
    );
}

#[test]
fn rejects_invalid_bounds_and_missing_text() {
    for words in [
        vec!["roster", ""],
        vec!["bulletin", "list", "team", "--after", "-1"],
        vec!["bulletin", "post", "team", "--mention", "", "body"],
        vec!["bulletin", "post", "team"],
        vec!["register", "team"],
    ] {
        let mut args = Args::new(words.into_iter().map(OsString::from), "team");
        assert!(team(&mut args).is_err());
    }
    let overlong = "x".repeat(MAX_BULLETIN_TEXT_BYTES + 1);
    let mut args = Args::new(
        ["bulletin", "post", "team", &overlong].map(OsString::from),
        "team",
    );
    assert!(team(&mut args).is_err());
}
