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

fn w1() -> Author {
    Author::Member {
        member_id: "w1".into(),
    }
}

fn w2() -> Author {
    Author::Member {
        member_id: "w2".into(),
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

#[test]
fn only_the_assignee_supervisor_or_open_creator_transitions_a_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);
    store
        .create_task("claimed", "t", &lead(), "Claimed", "", Some("w1"))
        .unwrap();
    store
        .create_task("open", "t", &w1(), "Open", "", None)
        .unwrap();
    store.claim_task("t", "claimed", "w1").unwrap();

    // The assignee may block and cancel their own claim.
    store
        .block_task("t", "claimed", &w1(), "waiting on credentials")
        .unwrap();
    store.cancel_task("t", "claimed", &w1()).unwrap();
    assert_eq!(task(&store, "claimed").status, TaskStatus::Cancelled);

    // Another worker may not block, cancel, or edit someone else's claim.
    store.claim_task("t", "open", "w1").unwrap();
    assert!(matches!(
        store.block_task("t", "open", &w2(), "hostile"),
        Err(Error::TaskUnavailable(_))
    ));
    assert!(matches!(
        store.cancel_task("t", "open", &w2()),
        Err(Error::TaskUnavailable(_))
    ));
    assert!(matches!(
        store.update_task("t", "open", &w2(), "Stolen", "", Some("w2")),
        Err(Error::TaskUnavailable(_))
    ));
    assert_eq!(task(&store, "open").assignee.as_deref(), Some("w1"));

    // The supervisor may still block, cancel, and re-scope any task.
    store
        .block_task("t", "open", &lead(), "waiting on the human")
        .unwrap();
    assert_eq!(task(&store, "open").status, TaskStatus::Blocked);
    store
        .update_task("t", "open", &lead(), "Open", "", Some("w2"))
        .unwrap();

    // The creator of an open, unassigned task may cancel it.
    store
        .update_task("t", "open", &lead(), "Open", "", None)
        .unwrap();
    store.cancel_task("t", "open", &w1()).unwrap();
    assert_eq!(task(&store, "open").status, TaskStatus::Cancelled);
    assert!(matches!(
        store.cancel_task("t", "open", &w1()),
        Err(Error::InvalidTaskTransition { .. })
    ));
}

#[test]
fn a_worker_may_hand_off_their_own_claim_but_not_steal_another() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);
    store
        .create_task("mine", "t", &lead(), "Mine", "", Some("w1"))
        .unwrap();
    store
        .create_task("theirs", "t", &lead(), "Theirs", "", Some("w2"))
        .unwrap();
    store.claim_task("t", "mine", "w1").unwrap();
    store.claim_task("t", "theirs", "w2").unwrap();

    // Handing off your own claim reopens the task for the next worker.
    store
        .update_task("t", "mine", &w1(), "Mine", "", Some("w2"))
        .unwrap();
    let mine = task(&store, "mine");
    assert_eq!(mine.status, TaskStatus::Open);
    assert_eq!(mine.assignee.as_deref(), Some("w2"));

    // Taking another worker's claim through an edit is refused.
    assert!(matches!(
        store.update_task("t", "mine", &w1(), "Mine", "", Some("w1")),
        Err(Error::TaskUnavailable(_))
    ));
}
