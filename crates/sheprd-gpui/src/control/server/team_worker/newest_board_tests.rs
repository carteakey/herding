//! The board reads its newest work: a team with more tasks or findings than
//! one page still reports the tasks people are arguing about now, and its
//! status counts stay exact whatever the history holds.

#![allow(clippy::unwrap_used)]
use super::*;
use sheprd_orchestrator::team_store::{Author, Confidence, Member, SessionTarget, TaskStatus};

fn target(n: u8) -> SessionTarget {
    SessionTarget {
        host: "boot".into(),
        session: format!("session-{n}"),
        workspace: Some("w".into()),
        pane: Some(format!("p{n}")),
    }
}

/// A team with `tasks` tasks and `findings` findings numbered upward from zero.
/// Every third task goes to `worker` and the first ten are cancelled, so the
/// status counts are only right if they are counted across pages.
fn seeded(dir: &std::path::Path, tasks: usize, findings: usize) -> TeamStore {
    let mut store = TeamStore::open(&dir.join("team").join("t.sqlite3")).unwrap();
    let member = |id: &str, n| Member {
        member_id: id.into(),
        display_name: id.into(),
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
    let human = Author::Human {
        user_id: "human".into(),
    };
    for index in 0..tasks {
        store
            .create_task(
                &format!("task-{index:04}"),
                "t",
                &human,
                &format!("task-{index:04}"),
                "brief",
                if index % 3 == 0 { Some("worker") } else { None },
            )
            .unwrap();
    }
    for index in 0..tasks.min(10) {
        store
            .cancel_task("t", &format!("task-{index:04}"), &human)
            .unwrap();
    }
    for index in 0..findings {
        store
            .add_finding(
                &format!("finding-{index:04}"),
                "t",
                &human,
                None,
                &format!("finding-{index:04}"),
                "evidence",
                Confidence::Medium,
                None,
            )
            .unwrap();
    }
    store
}

fn titles(tasks: &[Task]) -> Vec<String> {
    tasks.iter().map(|task| task.title.clone()).collect()
}

#[test]
fn a_snapshot_shows_the_newest_tasks_and_findings() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = seeded(dir.path(), 130, 130);

    let Reply::Snapshot {
        tasks, findings, ..
    } = snapshot(&mut store, "t", 200).unwrap()
    else {
        panic!("snapshot reply expected");
    };

    assert_eq!(tasks.len(), team_store::MAX_PAGE_SIZE);
    assert_eq!(titles(&tasks).first().unwrap(), "task-0030");
    assert_eq!(titles(&tasks).last().unwrap(), "task-0129");
    assert!(
        matches!(
            tasks.last().unwrap().status,
            TaskStatus::Open | TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::Cancelled
        ),
        "the newest task is still a real task"
    );
    assert_eq!(findings.len(), team_store::MAX_PAGE_SIZE);
    assert_eq!(findings.first().unwrap().description, "finding-0030");
    assert_eq!(findings.last().unwrap().description, "finding-0129");
}

/// The count is an aggregate over the team, not a scan of whatever page the
/// board happens to hold: 130 tasks with ten cancelled and every third one
/// assigned would have read as the oldest 100's cancellations.
#[test]
fn a_status_report_counts_tasks_past_one_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = seeded(dir.path(), 130, 1);
    let caller = target(2);

    let Reply::ScopedStatus {
        open_tasks,
        assigned_tasks,
        ..
    } = scoped(
        &mut store,
        "t",
        &caller,
        &caller.host,
        &[],
        &[],
        ScopedAction::Status { team_id: None },
        &[],
    )
    .unwrap()
    else {
        panic!("status reply expected");
    };

    assert_eq!(open_tasks, 120);
    assert_eq!(assigned_tasks, 40);
}

#[test]
fn a_status_report_issues_the_same_queries_whatever_the_task_count() {
    let few_dir = tempfile::tempdir().unwrap();
    let many_dir = tempfile::tempdir().unwrap();
    let mut few = seeded(few_dir.path(), 5, 1);
    let mut many = seeded(many_dir.path(), 400, 1);
    let caller = target(2);

    let count = |store: &mut TeamStore| {
        store.reset_statement_count();
        let reply = scoped(
            store,
            "t",
            &caller,
            &caller.host,
            &[],
            &[],
            ScopedAction::Status { team_id: None },
            &[],
        )
        .unwrap();
        let Reply::ScopedStatus { open_tasks, .. } = reply else {
            panic!("status reply expected");
        };
        (store.statement_count(), open_tasks)
    };

    let (few_queries, few_open) = count(&mut few);
    let (many_queries, many_open) = count(&mut many);

    assert_eq!(few_open, 0, "5 tasks, all of them cancelled");
    assert_eq!(many_open, 390);
    assert_eq!(
        many_queries, few_queries,
        "counting tasks does not scale with them: {few_queries} vs {many_queries}"
    );
}

/// A snapshot's reads are pages, so a 400-task team and a 5-task team cost the
/// same statements: one task page, one finding page, and the fixed team reads.
#[test]
fn a_snapshot_issues_the_same_queries_whatever_the_board_holds() {
    let few_dir = tempfile::tempdir().unwrap();
    let many_dir = tempfile::tempdir().unwrap();
    let mut few = seeded(few_dir.path(), 5, 2);
    let mut many = seeded(many_dir.path(), 400, 400);

    let count = |store: &mut TeamStore| {
        store.reset_statement_count();
        let reply = snapshot(store, "t", 200).unwrap();
        let Reply::Snapshot {
            tasks, findings, ..
        } = reply
        else {
            panic!("snapshot reply expected");
        };
        (store.statement_count(), tasks.len(), findings.len())
    };

    let (few_queries, few_tasks, few_findings) = count(&mut few);
    let (many_queries, many_tasks, many_findings) = count(&mut many);

    assert_eq!((few_tasks, few_findings), (5, 2));
    assert_eq!(
        (many_tasks, many_findings),
        (team_store::MAX_PAGE_SIZE, team_store::MAX_PAGE_SIZE)
    );
    assert_eq!(
        many_queries, few_queries,
        "the board pays for a page, not for the team's history: {few_queries} vs {many_queries}"
    );
}

/// A scoped task list is a cursor page: the newest page reports the sequence
/// to page back from, the page behind it holds only older work, and an
/// exhausted board reports no further cursor.
#[test]
fn scoped_task_and_finding_lists_page_behind_a_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = seeded(dir.path(), 130, 130);
    let caller = target(2);
    let page = team_store::MAX_PAGE_SIZE;

    let Reply::ScopedTasks { tasks, older, .. } = scoped(
        &mut store,
        "t",
        &caller,
        &caller.host,
        &[],
        &[],
        ScopedAction::TaskList {
            before: None,
            limit: page,
        },
        &[],
    )
    .unwrap() else {
        panic!("tasks reply expected");
    };
    assert_eq!(tasks.len(), page);
    let Some(before) = older else {
        panic!("a full page has an older cursor");
    };
    assert_eq!(before, tasks.first().unwrap().0);

    let Reply::ScopedTasks { tasks, older, .. } = scoped(
        &mut store,
        "t",
        &caller,
        &caller.host,
        &[],
        &[],
        ScopedAction::TaskList {
            before: Some(before),
            limit: page,
        },
        &[],
    )
    .unwrap() else {
        panic!("tasks reply expected");
    };
    assert_eq!(tasks.len(), 30, "130 tasks minus the newest page");
    assert_eq!(older, None, "the board is exhausted");
    assert_eq!(
        tasks.last().unwrap().0,
        before - 1,
        "the cursor is exclusive, so the pages tile the board without a gap"
    );

    let Reply::ScopedFindings {
        findings, older, ..
    } = scoped(
        &mut store,
        "t",
        &caller,
        &caller.host,
        &[],
        &[],
        ScopedAction::FindingList {
            before: None,
            limit: page,
        },
        &[],
    )
    .unwrap()
    else {
        panic!("findings reply expected");
    };
    assert_eq!(findings.len(), page);
    assert_eq!(older, Some(findings.first().unwrap().0));
}

/// The snapshot tells the board whether older pages exist behind the
/// show-earlier controls: a full page carries the cursor, a short board
/// reports none.
#[test]
fn a_snapshot_carries_the_board_paging_cursors() {
    let full_dir = tempfile::tempdir().unwrap();
    let mut full = seeded(full_dir.path(), 130, 5);
    let Reply::Snapshot {
        task_older,
        finding_older,
        ..
    } = snapshot(&mut full, "t", 200).unwrap()
    else {
        panic!("snapshot reply expected");
    };
    assert!(task_older.is_some(), "130 tasks fill the page");
    assert_eq!(finding_older, None, "5 findings leave the page short");

    let short_dir = tempfile::tempdir().unwrap();
    let mut short = seeded(short_dir.path(), 12, 3);
    let Reply::Snapshot {
        task_older,
        finding_older,
        ..
    } = snapshot(&mut short, "t", 200).unwrap()
    else {
        panic!("snapshot reply expected");
    };
    assert_eq!(task_older, None);
    assert_eq!(finding_older, None);
}
