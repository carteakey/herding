//! A snapshot reads its page from the newest end of a team's history, so a
//! long-running team shows its latest conversation and pays the same queries
//! whatever it has accumulated.

#![allow(clippy::unwrap_used)]
use super::*;
use sheprd_orchestrator::team_store::{Author, Member, Recipients, SessionTarget};

fn target(n: u8) -> SessionTarget {
    SessionTarget {
        host: "boot".into(),
        session: format!("session-{n}"),
        workspace: Some("w".into()),
        pane: Some(format!("p{n}")),
    }
}

/// A team with `posts` bulletin entries body-named `body-0000` upward, each
/// mentioning both members so every returned post carries delivery statuses.
fn seeded(dir: &std::path::Path, posts: usize) -> TeamStore {
    let mut store = TeamStore::open(&dir.join("team").join("t.sqlite3")).unwrap();
    let member = |id: &str, n| Member {
        member_id: id.into(),
        display_name: id.into(),
        model: None,
        account: None,
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
    for index in 0..posts {
        store
            .add_post_with_recipients(
                &format!("p{index:04}"),
                "t",
                &human,
                None,
                &format!("body-{index:04}"),
                &Recipients::Explicit(vec!["lead".into(), "worker".into()]),
            )
            .unwrap();
    }
    store
}

fn snapshot_posts(store: &mut TeamStore) -> Vec<Post> {
    let Reply::Snapshot { posts, .. } = snapshot(store, "t", 200).unwrap() else {
        panic!("snapshot reply expected");
    };
    posts.into_iter().map(|(post, _)| post).collect()
}

#[test]
fn a_snapshot_shows_the_newest_posts_of_a_long_team() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = seeded(dir.path(), 1000);

    let posts = snapshot_posts(&mut store);

    assert_eq!(posts.len(), 200);
    assert_eq!(posts.first().unwrap().body, "body-0800");
    assert_eq!(posts.last().unwrap().body, "body-0999");
    let bodies: Vec<&str> = posts.iter().map(|post| post.body.as_str()).collect();
    assert_eq!(
        bodies.windows(2).filter(|pair| pair[0] > pair[1]).count(),
        0
    );
}

#[test]
fn a_longer_team_costs_the_same_refresh_as_a_short_one() {
    let long_dir = tempfile::tempdir().unwrap();
    let short_dir = tempfile::tempdir().unwrap();
    let mut long = seeded(long_dir.path(), 1000);
    let mut short = seeded(short_dir.path(), 200);

    long.reset_statement_count();
    snapshot(&mut long, "t", 200).unwrap();
    let long_queries = long.statement_count();

    short.reset_statement_count();
    snapshot(&mut short, "t", 200).unwrap();
    let short_queries = short.statement_count();

    assert_eq!(
        long_queries, short_queries,
        "five times the history, the same queries: {long_queries} vs {short_queries}"
    );
    assert!(
        long_queries <= 8,
        "team, goal, two newest pages, statuses, tasks, findings: {long_queries}"
    );
}

#[test]
fn the_newest_page_still_carries_delivery_statuses() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = seeded(dir.path(), 300);
    // The two newest entries and one that the snapshot will not reach.
    store.acknowledge("p0299", "worker").unwrap();
    store.acknowledge("p0298", "worker").unwrap();
    store.acknowledge("p0000", "worker").unwrap();

    let Reply::Snapshot { posts, .. } = snapshot(&mut store, "t", 200).unwrap() else {
        panic!("snapshot reply expected");
    };

    assert_eq!(posts.len(), 200);
    assert!(
        posts.iter().all(|(_, statuses)| statuses.len() == 2),
        "every shown post reports both members"
    );
    let acknowledged: Vec<String> = posts
        .iter()
        .filter(|(_, statuses)| {
            statuses.iter().any(|(id, status)| {
                id == "worker" && matches!(status, RecipientStatus::Acknowledged(_))
            })
        })
        .map(|(post, _)| post.body.clone())
        .collect();
    assert_eq!(
        acknowledged,
        vec!["body-0298".to_owned(), "body-0299".to_owned()],
        "acknowledgements ride along on the newest page"
    );
}
