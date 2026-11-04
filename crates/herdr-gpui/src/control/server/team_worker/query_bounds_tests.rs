//! The team worker's read paths are bounded by pages and chunks, never by the
//! number of posts a team has. `statement_count` is the store's own counter of
//! prepared statements (test fixtures only), so these tests catch an N+1
//! regression in the refresh path instead of leaving it to a comment.

#![allow(clippy::unwrap_used)]
use super::*;
use herdr_orchestrator::team_store::{Author, Member, Recipients, SessionTarget};

fn target(n: u8) -> SessionTarget {
    SessionTarget {
        host: "boot".into(),
        session: format!("session-{n}"),
        workspace: Some("w".into()),
        pane: Some(format!("p{n}")),
    }
}

/// A team whose posts all mention both members, so every returned post carries
/// delivery statuses to fetch.
fn seeded(dir: &std::path::Path, posts: usize) -> TeamStore {
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
    for index in 0..posts {
        store
            .add_post_with_recipients(
                &format!("p{index:04}"),
                "t",
                &human,
                None,
                "body",
                &Recipients::Explicit(vec!["lead".into(), "worker".into()]),
            )
            .unwrap();
    }
    store
}

fn snapshot_queries(store: &mut TeamStore) -> (usize, usize) {
    store.reset_statement_count();
    let reply = snapshot(store, "t", 200).unwrap();
    let Reply::Snapshot { posts, .. } = reply else {
        panic!("snapshot reply expected");
    };
    (store.statement_count(), posts.len())
}

/// 60 posts fit one page, 250 need three: the refresh pays for the extra page
/// and nothing else. Before batching, the same pair differed by one status
/// query per returned post.
#[test]
fn a_snapshot_issues_the_same_queries_whatever_the_post_count() {
    let few_dir = tempfile::tempdir().unwrap();
    let many_dir = tempfile::tempdir().unwrap();
    let mut few = seeded(few_dir.path(), 60);
    let mut many = seeded(many_dir.path(), 250);

    let (few_queries, few_posts) = snapshot_queries(&mut few);
    let (many_queries, many_posts) = snapshot_queries(&mut many);

    assert_eq!(few_posts, 60);
    assert_eq!(many_posts, 200, "the snapshot returns its limit");
    assert_eq!(
        many_queries,
        few_queries + 2,
        "two extra pages, no extra per-post queries: {few_queries} vs {many_queries}"
    );
    assert!(
        many_queries <= 10,
        "a 250-post refresh stays a handful of queries: {many_queries}"
    );
}

/// The status command counts the caller's pending mentions in one aggregate,
/// so its cost is the same for a team with 60 posts and one with 250.
#[test]
fn a_status_report_issues_the_same_queries_whatever_the_post_count() {
    let few_dir = tempfile::tempdir().unwrap();
    let many_dir = tempfile::tempdir().unwrap();
    let mut few = seeded(few_dir.path(), 60);
    let mut many = seeded(many_dir.path(), 250);
    let caller = target(2);

    let count = |store: &mut TeamStore| {
        store.reset_statement_count();
        let reply = scoped(store, "t", &caller, ScopedAction::Status { team_id: None }).unwrap();
        let Reply::ScopedStatus {
            pending_mentions, ..
        } = reply
        else {
            panic!("status reply expected");
        };
        (store.statement_count(), pending_mentions)
    };

    let (few_queries, few_pending) = count(&mut few);
    let (many_queries, many_pending) = count(&mut many);

    assert_eq!(few_pending, 60, "60 posts mention the caller");
    assert_eq!(many_pending, 250);
    assert_eq!(
        many_queries, few_queries,
        "the pending count does not scale with posts"
    );
}
