#![allow(clippy::unwrap_used)]

use super::{
    Author, Confidence, Error, MAX_PAGE_SIZE, Member, SessionTarget, Task, TaskCounts, Team,
    TeamStore,
};
use tempfile::TempDir;

const TASKS: usize = 130;
const FINDINGS: usize = 130;

fn target(n: u8) -> SessionTarget {
    SessionTarget {
        host: "boot".into(),
        session: format!("session-{n}"),
        workspace: Some("w".into()),
        pane: Some(format!("p{n}")),
    }
}

fn member(id: &str, n: u8) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
        model: None,
        account: None,
        target: target(n),
    }
}

fn human() -> Author {
    Author::Human {
        user_id: "human".into(),
    }
}

fn title(index: usize) -> String {
    format!("task-{index:04}")
}

fn finding_name(index: usize) -> String {
    format!("finding-{index:04}")
}

/// A team with [`TASKS`] tasks and [`FINDINGS`] findings numbered upward from
/// zero, so a page can be named by the titles it must hold. Every third task
/// goes to `worker` and the first ten are cancelled, which is what makes the
/// count test cross a page boundary.
fn seeded() -> (TeamStore, TempDir) {
    let dir = TempDir::new().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "team".into(),
            name: "team".into(),
            supervisor: member("lead", 1),
            members: vec![member("worker", 2)],
        })
        .unwrap();
    for index in 0..TASKS {
        store
            .create_task(
                &title(index),
                "team",
                &human(),
                &title(index),
                "brief",
                if index % 3 == 0 { Some("worker") } else { None },
            )
            .unwrap();
    }
    for index in 0..10 {
        store.cancel_task("team", &title(index), &human()).unwrap();
    }
    for index in 0..FINDINGS {
        store
            .add_finding(
                &finding_name(index),
                "team",
                &human(),
                None,
                &finding_name(index),
                "evidence",
                Confidence::Medium,
                None,
            )
            .unwrap();
    }
    (store, dir)
}

fn task_titles(page: &[(i64, Task)]) -> Vec<String> {
    page.iter().map(|(_, task)| task.title.clone()).collect()
}

fn finding_names(page: &[(i64, super::Finding)]) -> Vec<String> {
    page.iter()
        .map(|(_, finding)| finding.description.clone())
        .collect()
}

#[test]
fn a_board_read_starts_at_the_newest_task() {
    let (store, _dir) = seeded();

    let page = store.latest_tasks("team", None, MAX_PAGE_SIZE).unwrap();
    let titles = task_titles(&page);
    assert_eq!(titles.len(), MAX_PAGE_SIZE);
    assert_eq!(titles.first().unwrap(), &title(TASKS - MAX_PAGE_SIZE));
    assert_eq!(titles.last().unwrap(), &title(TASKS - 1));
    // Sequences ascend with the titles, so a sequence means the same thing to
    // a forward cursor and a backward one.
    assert!(page.windows(2).all(|pair| pair[0].0 < pair[1].0));
}

#[test]
fn task_cursors_cover_the_history_once() {
    let (store, _dir) = seeded();

    let mut pages: Vec<Vec<String>> = Vec::new();
    let mut before = None;
    loop {
        let page = store.latest_tasks("team", before, MAX_PAGE_SIZE).unwrap();
        if page.is_empty() {
            break;
        }
        before = Some(page.first().unwrap().0);
        pages.push(task_titles(&page));
    }

    // Each page is ascending; the pages themselves come newest first.
    let seen: Vec<String> = pages.into_iter().rev().flatten().collect();
    let expected: Vec<String> = (0..TASKS).map(title).collect();
    assert_eq!(seen, expected);
}

#[test]
fn tasks_still_read_from_the_oldest_end() {
    let (store, _dir) = seeded();

    let page = store.tasks("team", MAX_PAGE_SIZE).unwrap();
    let expected: Vec<String> = (0..MAX_PAGE_SIZE).map(title).collect();
    let got: Vec<String> = page.iter().map(|task| task.title.clone()).collect();
    assert_eq!(got, expected);
}

#[test]
fn task_pages_keep_the_size_bound_in_both_directions() {
    let (store, _dir) = seeded();

    for limit in [0, MAX_PAGE_SIZE + 1] {
        assert!(
            matches!(
                store.latest_tasks("team", None, limit),
                Err(Error::Invalid {
                    field: "page size",
                    max: MAX_PAGE_SIZE
                })
            ),
            "newest page of {limit}"
        );
        assert!(
            matches!(
                store.tasks("team", limit),
                Err(Error::Invalid {
                    field: "page size",
                    max: MAX_PAGE_SIZE
                })
            ),
            "forward page of {limit}"
        );
    }
}

#[test]
fn task_counts_span_more_than_one_page() {
    let (store, _dir) = seeded();

    // Of 130 tasks ten are cancelled and every third one belongs to `worker`,
    // so four of the cancelled ones were assigned. Reading the oldest 100 and
    // counting in Rust, as the status reply used to, would report the first
    // page's cancellations as the team's whole workload.
    assert_eq!(
        store.task_counts("team", "worker").unwrap(),
        TaskCounts {
            open: TASKS - 10,
            assigned: TASKS.div_ceil(3) - 4,
        }
    );
    assert_eq!(
        store.task_counts("absent-team", "worker").unwrap(),
        TaskCounts::default()
    );
}

#[test]
fn finding_pages_walk_from_the_newest_end() {
    let (store, _dir) = seeded();

    let newest = store.latest_findings("team", None, MAX_PAGE_SIZE).unwrap();
    let newest_names = finding_names(&newest);
    assert_eq!(
        newest_names.first().unwrap(),
        &finding_name(FINDINGS - MAX_PAGE_SIZE)
    );
    assert_eq!(newest_names.last().unwrap(), &finding_name(FINDINGS - 1));

    let older = store
        .latest_findings("team", Some(newest.first().unwrap().0), MAX_PAGE_SIZE)
        .unwrap();
    let older_names = finding_names(&older);
    assert_eq!(older_names.len(), FINDINGS - MAX_PAGE_SIZE);
    assert_eq!(older_names.first().unwrap(), &finding_name(0));
    assert_eq!(
        older_names.last().unwrap(),
        &finding_name(FINDINGS - MAX_PAGE_SIZE - 1)
    );
    assert!(
        older_names.iter().all(|name| !newest_names.contains(name)),
        "the two pages overlap"
    );

    let oldest_first = store.findings("team", MAX_PAGE_SIZE).unwrap();
    assert_eq!(oldest_first.first().unwrap().description, finding_name(0));
    assert!(
        matches!(
            store.latest_findings("team", None, MAX_PAGE_SIZE + 1),
            Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE
            })
        ),
        "an over-long newest page"
    );
}
