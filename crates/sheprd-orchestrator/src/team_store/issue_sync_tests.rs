//! The GitHub issue sync's store contract: watch sets are bounded and
//! member-gated, one issue owns one task per team, and the issue's state
//! maps onto the task lifecycle without ever overwriting human intent.

#![allow(clippy::unwrap_used)]

use super::*;
use rusqlite::Connection;

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
            members: vec![member("w1", "s1")],
        })
        .unwrap();
    store
}

fn issue(number: i64, state: &str, title: &str) -> IssueSnapshot {
    IssueSnapshot {
        repo: "carteakey/herding".into(),
        number,
        url: format!("https://github.com/carteakey/herding/issues/{number}"),
        title: title.into(),
        body: format!("The body of #{number}."),
        state: state.into(),
    }
}

fn upsert(store: &mut TeamStore, snapshot: &IssueSnapshot) -> (Task, IssueSyncKind) {
    let heading = format!("#{}: {}", snapshot.number, snapshot.title);
    let details = format!("{}\n\n{}", snapshot.url, snapshot.body);
    store
        .upsert_issue_task(
            "t",
            &lead(),
            &format!("task-{}", snapshot.number),
            snapshot,
            &heading,
            &details,
        )
        .unwrap()
}

#[test]
fn watch_sets_are_member_gated_bounded_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);

    let outsider = Author::Member {
        member_id: "ghost".into(),
    };
    assert!(matches!(
        store.watch_issues("t", &outsider, "carteakey/herding", &[]),
        Err(Error::MemberNotInTeam { .. })
    ));

    let watched = store
        .watch_issues("t", &lead(), "carteakey/herding", &[])
        .unwrap();
    assert_eq!(
        watched,
        vec![IssueWatch {
            repo: "carteakey/herding".into(),
            number: REPO_WATCH
        }]
    );

    // Watching the same repository again does not duplicate a row.
    let watched = store
        .watch_issues("t", &lead(), "carteakey/herding", &[])
        .unwrap();
    assert_eq!(watched.len(), 1);

    // Issue numbers watch exactly those issues beside other repositories.
    let watched = store
        .watch_issues("t", &lead(), "carteakey/other", &[7, 9])
        .unwrap();
    assert_eq!(
        watched,
        vec![
            IssueWatch {
                repo: "carteakey/herding".into(),
                number: REPO_WATCH
            },
            IssueWatch {
                repo: "carteakey/other".into(),
                number: 7
            },
            IssueWatch {
                repo: "carteakey/other".into(),
                number: 9
            },
        ]
    );

    assert!(matches!(
        store.watch_issues("t", &lead(), "not-a-slug", &[]),
        Err(Error::Invalid { field: "repo", .. })
    ));
    assert!(matches!(
        store.watch_issues("t", &lead(), "carteakey/other", &[0]),
        Err(Error::Invalid {
            field: "issue number",
            ..
        })
    ));

    // The bound keeps one quiet team from promising unbounded syncing.
    for repo in 0..5 {
        store
            .watch_issues("t", &lead(), &format!("carteakey/r{repo}"), &[])
            .unwrap();
    }
    assert!(matches!(
        store.watch_issues("t", &lead(), "carteakey/one-too-many", &[]),
        Err(Error::Invalid {
            field: "watch set",
            ..
        })
    ));

    // Unwatching the repository forgets the issues watched inside it.
    let watched = store.unwatch_issues("t", "carteakey/other", &[]).unwrap();
    assert!(!watched.iter().any(|watch| watch.repo == "carteakey/other"));
    let watched = store.unwatch_issues("t", "carteakey/other", &[7]).unwrap();
    assert_eq!(watched.len(), 6, "removing an absent watch is not an error");
}

#[test]
fn one_issue_maps_to_one_task_that_follows_its_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);

    let (task, kind) = upsert(&mut store, &issue(1, "OPEN", "Wake reliability"));
    assert_eq!(kind, IssueSyncKind::Created);
    assert_eq!(task.status, TaskStatus::Open);
    assert_eq!(task.creator, lead());

    // The same issue again is stored once and changes nothing.
    let (again, kind) = upsert(&mut store, &issue(1, "OPEN", "Wake reliability"));
    assert_eq!(kind, IssueSyncKind::Unchanged);
    assert_eq!(again.task_id, task.task_id);
    assert_eq!(store.tasks("t", 10).unwrap().len(), 1);

    // A title edit refreshes the heading; a body edit refreshes the details.
    let (task, kind) = upsert(&mut store, &issue(1, "OPEN", "Wake reliability on LAN"));
    assert_eq!(kind, IssueSyncKind::Refreshed);
    assert!(task.title.contains("on LAN"));
    let snapshot = IssueSnapshot {
        body: "Now with more detail.".into(),
        ..issue(1, "OPEN", "Wake reliability on LAN")
    };
    let (task, kind) = upsert(&mut store, &snapshot);
    assert_eq!(kind, IssueSyncKind::Refreshed);
    assert!(task.brief.ends_with("Now with more detail."));
    assert_eq!(store.tasks("t", 10).unwrap().len(), 1, "still one task");
    assert_eq!(
        store
            .task_for_issue("t", "carteakey/herding", 1)
            .unwrap()
            .unwrap()
            .task_id,
        task.task_id
    );
}

#[test]
fn closing_completes_reopening_reopens_and_humans_win() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);

    let (task, kind) = upsert(&mut store, &issue(2, "OPEN", "Fleet pipeline tracker"));
    assert_eq!(kind, IssueSyncKind::Created);
    let close_url = issue(2, "CLOSED", "Fleet pipeline tracker").url;

    // A human completion result survives the issue closing.
    store
        .complete_task("t", &task.task_id, &lead(), "tracked in the release doc")
        .unwrap();
    let (task, kind) = upsert(&mut store, &issue(2, "CLOSED", "Fleet pipeline tracker"));
    assert_eq!(kind, IssueSyncKind::Closed);
    assert_eq!(task.status, TaskStatus::Done);
    assert_eq!(task.result.as_deref(), Some("tracked in the release doc"));

    // Reopening sends the completed task back to open, dropping the result.
    let (task, kind) = upsert(&mut store, &issue(2, "OPEN", "Fleet pipeline tracker"));
    assert_eq!(kind, IssueSyncKind::Reopened);
    assert_eq!(task.status, TaskStatus::Open);
    assert_eq!(task.result, None);

    // An open task takes the issue URL as its close link.
    let (task, kind) = upsert(&mut store, &issue(2, "CLOSED", "Fleet pipeline tracker"));
    assert_eq!(kind, IssueSyncKind::Closed);
    assert_eq!(task.status, TaskStatus::Done);
    assert_eq!(task.result.as_deref(), Some(close_url.as_str()));
    let (task, kind) = upsert(&mut store, &issue(2, "CLOSED", "Fleet pipeline tracker"));
    assert_eq!(
        kind,
        IssueSyncKind::Unchanged,
        "a quiet issue writes nothing"
    );
    assert_eq!(task.status, TaskStatus::Done);

    // A cancelled task stays cancelled: that was human intent, and a sync
    // moves it in neither direction. The transitions themselves are still
    // recorded, so the next sync compares against the state it saw. The
    // task is cancelled while open: the board refuses cancelling a done
    // task, and the sync must not need a refusal the board would not make.
    let (task, _) = upsert(&mut store, &issue(4, "OPEN", "Cancel intent"));
    store.cancel_task("t", &task.task_id, &lead()).unwrap();
    let (task, kind) = upsert(&mut store, &issue(4, "CLOSED", "Cancel intent"));
    assert_eq!(kind, IssueSyncKind::Closed);
    assert_eq!(task.status, TaskStatus::Cancelled);
    let (task, kind) = upsert(&mut store, &issue(4, "OPEN", "Cancel intent"));
    assert_eq!(task.status, TaskStatus::Cancelled);
    assert_eq!(kind, IssueSyncKind::Reopened);
}

#[test]
fn a_first_sync_of_a_closed_issue_lands_done_with_the_close_link() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);
    let (task, kind) = upsert(&mut store, &issue(3, "CLOSED", "Already fixed upstream"));
    assert_eq!(kind, IssueSyncKind::Created);
    assert_eq!(task.status, TaskStatus::Done);
    assert_eq!(
        task.result.as_deref(),
        Some("https://github.com/carteakey/herding/issues/3")
    );
}

#[test]
fn a_store_from_before_the_issue_columns_gains_them_and_keeps_its_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("db.sqlite3");
    let mut store = store(&dir);
    store
        .create_task(
            "task-human",
            "t",
            &lead(),
            "#9: A human task",
            "Written by hand before the sync existed.",
            None,
        )
        .unwrap();
    drop(store);
    // As a build from before the issue sync left it: no refs on tasks and
    // no watch set.
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "DROP INDEX IF EXISTS tasks_issue_idx;
             ALTER TABLE tasks DROP COLUMN issue_repo;
             ALTER TABLE tasks DROP COLUMN issue_number;
             ALTER TABLE tasks DROP COLUMN issue_url;
             ALTER TABLE tasks DROP COLUMN issue_state;
             DROP TABLE IF EXISTS team_issue_watch;",
        )
        .unwrap();

    let mut store = TeamStore::open(&path).unwrap();
    let tasks = store.tasks("t", 10).unwrap();
    assert_eq!(tasks.len(), 1, "the human task survives the migration");
    assert_eq!(tasks[0].task_id, "task-human");

    // The reopened store syncs issues beside the task it kept: the human
    // task carries no ref, so the issue owns a task of its own.
    let (task, kind) = upsert(&mut store, &issue(5, "OPEN", "Post-migration sync"));
    assert_eq!(kind, IssueSyncKind::Created);
    assert_eq!(store.tasks("t", 10).unwrap().len(), 2);
    assert_eq!(
        store
            .task_for_issue("t", "carteakey/herding", 5)
            .unwrap()
            .unwrap()
            .task_id,
        task.task_id
    );
    store
        .watch_issues("t", &lead(), "carteakey/herding", &[])
        .unwrap();
}
