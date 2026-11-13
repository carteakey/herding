//! The identity audit trail: every member mutation leaves a durable,
//! atomic witness - the evidence the Nov 12 roster-cleanup investigation
//! lacked.

use super::super::{Author, TaskStatus, Team};
use super::*;

fn member(id: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
        target: SessionTarget {
            host: "local".into(),
            session: format!("session-{id}"),
            workspace: Some("ws".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

fn team(team_id: &str, workers: &[&str]) -> Team {
    Team {
        team_id: team_id.into(),
        name: team_id.into(),
        supervisor: member("lead"),
        members: workers.iter().map(|id| member(id)).collect(),
    }
}

fn open() -> TeamStore {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("team.sqlite3");
    // The store owns its connection for the life of the test; the directory
    // must outlive it, so leak the guard on purpose.
    std::mem::forget(dir);
    TeamStore::open(&path).unwrap()
}

#[test]
fn save_register_rebind_and_remove_are_all_recorded() {
    let mut store = open();
    store.save_team(&team("t", &["w1"])).unwrap();
    let events = store.identity_history("t", None).unwrap();
    assert_eq!(
        events.len(),
        2,
        "the supervisor and the new worker register"
    );
    assert!(events.iter().all(|event| event.action == "registered"));

    // A re-proof of the same target registers again, unchanged.
    store
        .register_member("t", "w1", "w1", &member("w1").target, &["local".into()])
        .unwrap();
    let events = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| event.action == "registered"));

    // A resumed session on the same pane rebinds: the identity moved. The
    // resumed agent proves the same name and pane under a fresh member id;
    // the rebind keeps the row's own id.
    let mut moved = member("w1").target;
    moved.session = "session-w1-resumed".into();
    store
        .register_member("t", "w1-resumed", "w1", &moved, &["local".into()])
        .unwrap();
    let rebound = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(rebound.last().unwrap().action, "rebound");
    assert_eq!(rebound.last().unwrap().member_id, "w1");
    assert_eq!(rebound.last().unwrap().session, "session-w1-resumed");

    // Removal records the identity that is about to disappear.
    store.remove_member("t", "w1").unwrap();
    let events = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(events.last().unwrap().action, "removed");
    assert_eq!(events.last().unwrap().session, "session-w1-resumed");
    assert_eq!(events.last().unwrap().pane.as_deref(), Some("pane-w1"));
}

#[test]
fn a_removed_member_is_not_in_the_roster_but_is_in_the_log() {
    let mut store = open();
    store.save_team(&team("t", &["w1", "w2"])).unwrap();
    store.remove_member("t", "w1").unwrap();
    let team = store.team("t").unwrap().unwrap();
    assert!(!team.members.iter().any(|m| m.member_id == "w1"));
    let history = store.identity_history("t", None).unwrap();
    let removed: Vec<&IdentityEvent> = history
        .iter()
        .filter(|event| event.action == "removed")
        .collect();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].member_id, "w1");
}

#[test]
fn a_failed_removal_records_nothing() {
    let mut store = open();
    store.save_team(&team("t", &["w1"])).unwrap();
    assert!(store.remove_member("t", "gone").is_err());
    assert!(
        !store
            .identity_history("t", None)
            .unwrap()
            .iter()
            .any(|event| event.action == "removed")
    );
    // Removing the supervisor is refused, and nothing is logged.
    assert!(store.remove_member("t", "lead").is_err());
    assert!(
        !store
            .identity_history("t", None)
            .unwrap()
            .iter()
            .any(|event| event.action == "removed")
    );
}

#[test]
fn member_history_scopes_to_one_member() {
    let mut store = open();
    store.save_team(&team("t", &["w1", "w2"])).unwrap();
    store.remove_member("t", "w1").unwrap();
    let w1 = store.identity_history("t", Some("w1")).unwrap();
    let w2 = store.identity_history("t", Some("w2")).unwrap();
    assert_eq!(w1.len(), 2, "registered by the save, then removed");
    assert_eq!(w1.last().unwrap().action, "removed");
    assert!(w2.iter().all(|event| event.action == "registered"));
}

#[test]
fn unknown_team_has_an_empty_history() {
    let store = open();
    assert!(store.identity_history("nope", None).unwrap().is_empty());
}

#[test]
fn a_mistaken_removal_heals_on_re_registration_within_the_window() {
    let mut store = open();
    store.save_team(&team("t", &["w1"])).unwrap();
    store
        .create_task(
            "task1",
            "t",
            &Author::Human {
                user_id: "human".into(),
            },
            "Write report",
            "",
            Some("w1"),
        )
        .unwrap();
    store.claim_task("t", "task1", "w1").unwrap();

    // The cleanup removes the live worker: the row tombstones, the task
    // returns to the board.
    store.remove_member("t", "w1").unwrap();
    let roster = store.team("t").unwrap().unwrap();
    assert!(roster.members.iter().all(|m| m.member_id != "w1"));
    let task = store
        .tasks("t", 10)
        .unwrap()
        .into_iter()
        .find(|task| task.task_id == "task1")
        .expect("the reset task");
    assert_eq!(task.status, TaskStatus::Open);
    assert!(task.assignee.is_none());

    // The worker's pane comes back and registers under a fresh member id:
    // the tombstoned row restores with its own id and its reset work
    // re-linked, so the identity and the assignment survive the mistake.
    let mut resumed = member("w1").target;
    resumed.session = "session-w1-resumed".into();
    let restored = store
        .register_member("t", "w1-resumed", "w1", &resumed, &["local".into()])
        .unwrap();
    assert_eq!(
        restored.member_id, "w1",
        "the row's own id comes back, not the CLI-minted one"
    );
    let roster = store.team("t").unwrap().unwrap();
    assert!(roster.members.iter().any(|m| m.member_id == "w1"));
    let task = store
        .tasks("t", 10)
        .unwrap()
        .into_iter()
        .find(|task| task.task_id == "task1")
        .expect("the task");
    assert_eq!(
        task.assignee.as_deref(),
        Some("w1"),
        "the assignment is re-linked"
    );
    assert_eq!(
        task.status,
        TaskStatus::Open,
        "the worker re-claims to resume"
    );

    let events = store.identity_history("t", Some("w1")).unwrap();
    let actions: Vec<&str> = events.iter().map(|event| event.action.as_str()).collect();
    assert_eq!(
        actions,
        vec!["registered", "removed", "restored"],
        "the audit trail tells the whole story"
    );
}

#[test]
fn a_tombstone_beyond_the_grace_window_mints_a_new_identity() {
    let mut store = open();
    store.save_team(&team("t", &["w1"])).unwrap();
    store.remove_member("t", "w1").unwrap();
    // Age the tombstone past the grace window.
    store
        .db
        .execute(
            "UPDATE members SET removed_at = '2020-01-01T00:00:00.000Z'
             WHERE team_id = 't' AND member_id = 'w1'",
            [],
        )
        .unwrap();

    let mut moved = member("w1").target;
    moved.session = "session-w1-resumed".into();
    let registered = store
        .register_member("t", "w1-resumed", "w1", &moved, &["local".into()])
        .unwrap();
    assert_eq!(
        registered.member_id, "w1-resumed",
        "a long-gone member's name is re-registered, not restored"
    );
    // The old tombstone stays for the audit trail.
    let events = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(events.last().unwrap().action, "removed");
}
