#![allow(clippy::unwrap_used)]

use super::*;
use herdr_orchestrator::team_store::{Author, Member, Recipients, SessionTarget};

/// A team with `posts` messages, sequence 1 upward, no delivery recipients.
fn seeded(posts: usize) -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("t.sqlite3")).unwrap();
    let member = |id: &str, n| Member {
        member_id: id.into(),
        display_name: id.into(),
        target: SessionTarget {
            host: "boot".into(),
            session: format!("session-{n}"),
            workspace: Some("w".into()),
            pane: Some(format!("p{n}")),
        },
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
    let recipients = Recipients::Explicit(Vec::new());
    for index in 0..posts {
        store
            .add_post_with_recipients(
                &format!("post-{index:04}"),
                "t",
                &human,
                None,
                &format!("body-{index:04}"),
                &recipients,
            )
            .unwrap();
    }
    (dir, store)
}

fn list(store: &mut TeamStore, after: Option<i64>) -> (Vec<i64>, Option<i64>) {
    let Reply::ScopedBulletin { entries, next, .. } = scoped(
        store,
        "t",
        &SessionTarget {
            host: "boot".into(),
            session: "session-2".into(),
            workspace: Some("w".into()),
            pane: Some("p2".into()),
        },
        ScopedAction::BulletinList { after, limit: 100 },
        &[],
    )
    .unwrap() else {
        panic!("expected a bulletin");
    };
    let sequences = entries.iter().map(|(sequence, _, _)| *sequence).collect();
    (sequences, next)
}

#[test]
fn the_default_list_is_the_newest_page_oldest_first() {
    let (_dir, mut store) = seeded(105);
    let (sequences, next) = list(&mut store, None);
    let expected: Vec<i64> = (6..=105).collect();
    assert_eq!(sequences, expected);
    assert_eq!(next, Some(105));
}

#[test]
fn after_still_pages_forward_and_a_short_page_ends_the_walk() {
    let (_dir, mut store) = seeded(105);
    let (sequences, next) = list(&mut store, Some(100));
    assert_eq!(sequences, (101..=105).collect::<Vec<i64>>());
    assert_eq!(next, None);
    let (sequences, next) = list(&mut store, Some(105));
    assert!(sequences.is_empty());
    assert_eq!(next, None);
}

#[test]
fn from_zero_reads_history_from_the_beginning() {
    let (_dir, mut store) = seeded(105);
    let (sequences, next) = list(&mut store, Some(0));
    assert_eq!(sequences, (1..=100).collect::<Vec<i64>>());
    assert_eq!(next, Some(100));
}

#[test]
fn the_newest_page_cursor_is_honest_when_history_is_short() {
    let (_dir, mut store) = seeded(3);
    let (sequences, next) = list(&mut store, None);
    assert_eq!(sequences, vec![1, 2, 3]);
    assert_eq!(next, None);
}
