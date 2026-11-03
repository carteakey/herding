#![allow(clippy::unwrap_used)]

use super::*;

fn member(id: &str, session: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
        target: SessionTarget {
            host: "local".into(),
            session: session.into(),
            workspace: Some("ws-1".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

fn lead() -> Author {
    Author::Member {
        member_id: "lead".into(),
    }
}

fn store(dir: &tempfile::TempDir) -> TeamStore {
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "Team".into(),
            supervisor: member("lead", "s0"),
            members: vec![member("w1", "s1"), member("w2", "s2")],
        })
        .unwrap();
    store
}

fn task(store: &TeamStore, task_id: &str) -> Task {
    store
        .tasks("t", 10)
        .unwrap()
        .into_iter()
        .find(|task| task.task_id == task_id)
        .unwrap()
}

#[test]
fn removing_a_member_reopens_in_progress_and_blocked_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);
    store
        .create_task("progress", "t", &lead(), "Progress", "", Some("w1"))
        .unwrap();
    store
        .create_task("blocked", "t", &lead(), "Blocked", "", Some("w1"))
        .unwrap();
    store
        .create_task("finished", "t", &lead(), "Finished", "", Some("w1"))
        .unwrap();
    store.claim_task("t", "progress", "w1").unwrap();
    store.claim_task("t", "blocked", "w1").unwrap();
    store
        .block_task("t", "blocked", &lead(), "waiting on credentials")
        .unwrap();
    store.claim_task("t", "finished", "w1").unwrap();
    store
        .complete_task("t", "finished", &lead(), "shipped")
        .unwrap();

    store.remove_member("t", "w1").unwrap();

    let progress = task(&store, "progress");
    assert_eq!(progress.status, TaskStatus::Open);
    assert_eq!(progress.assignee, None);
    assert_eq!(progress.blocker_reason, None);

    let blocked = task(&store, "blocked");
    assert_eq!(blocked.status, TaskStatus::Open);
    assert_eq!(blocked.assignee, None);
    assert_eq!(blocked.blocker_reason, None);

    let finished = task(&store, "finished");
    assert_eq!(finished.status, TaskStatus::Done);
    assert_eq!(finished.result.as_deref(), Some("shipped"));

    // Reopened work stays claimable by another worker.
    store.claim_task("t", "progress", "w2").unwrap();
    store.claim_task("t", "blocked", "w2").unwrap();
}

#[test]
fn changing_the_assignee_reopens_progress_and_blocked_claims() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);
    store
        .create_task("progress", "t", &lead(), "Progress", "", Some("w1"))
        .unwrap();
    store
        .create_task("blocked", "t", &lead(), "Blocked", "", Some("w1"))
        .unwrap();
    store
        .create_task("finished", "t", &lead(), "Finished", "", Some("w1"))
        .unwrap();
    store.claim_task("t", "progress", "w1").unwrap();
    store.claim_task("t", "blocked", "w1").unwrap();
    store
        .block_task("t", "blocked", &lead(), "waiting on credentials")
        .unwrap();
    store.claim_task("t", "finished", "w1").unwrap();
    store
        .complete_task("t", "finished", &lead(), "shipped")
        .unwrap();

    store
        .update_task("t", "progress", &lead(), "Progress", "", Some("w2"))
        .unwrap();
    let progress = task(&store, "progress");
    assert_eq!(progress.status, TaskStatus::Open);
    assert_eq!(progress.assignee.as_deref(), Some("w2"));
    assert_eq!(progress.blocker_reason, None);
    store.claim_task("t", "progress", "w2").unwrap();

    store
        .update_task("t", "blocked", &lead(), "Blocked", "", None)
        .unwrap();
    let blocked = task(&store, "blocked");
    assert_eq!(blocked.status, TaskStatus::Open);
    assert_eq!(blocked.assignee, None);
    assert_eq!(blocked.blocker_reason, None);

    // Editing the same assignee's in-progress task keeps its claim.
    store
        .update_task(
            "t",
            "progress",
            &lead(),
            "Progress edited",
            "brief",
            Some("w2"),
        )
        .unwrap();
    assert_eq!(task(&store, "progress").status, TaskStatus::InProgress);

    // Finished work keeps its outcome when reassigned.
    store
        .update_task("t", "finished", &lead(), "Finished", "", Some("w2"))
        .unwrap();
    let finished = task(&store, "finished");
    assert_eq!(finished.status, TaskStatus::Done);
    assert_eq!(finished.result.as_deref(), Some("shipped"));
}
