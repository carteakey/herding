//! Bounded read paths for bulletin delivery status: batched recipient
//! statuses, one statement per posts page, and a recipient-scoped pending
//! count. These tests pin the number of statements a read issues, which is
//! what the serial team worker pays for per refresh.

#![allow(clippy::unwrap_used)]

use super::*;

fn member(id: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
        model: None,
        account: None,
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

fn store_in(dir: &tempfile::TempDir) -> TeamStore {
    TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap()
}

fn human() -> Author {
    Author::Human {
        user_id: "human".into(),
    }
}

fn add_posts(store: &mut TeamStore, team_id: &str, recipients: &Recipients, count: usize) {
    for index in 0..count {
        store
            .add_post_with_recipients(
                &format!("{team_id}-p{index:04}"),
                team_id,
                &human(),
                None,
                "body",
                recipients,
            )
            .unwrap();
    }
}

fn post_ids(prefix: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("{prefix}-p{index:04}"))
        .collect()
}

/// The batched status read is one statement per `STATUS_BATCH_CHUNK` ids, so
/// the worker's per-refresh cost does not grow with the number of posts.
#[test]
fn batched_statuses_issue_one_statement_per_chunk() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_in(&dir);
    store.save_team(&team("chunk", &["w1", "w2"])).unwrap();
    add_posts(
        &mut store,
        "chunk",
        &Recipients::Explicit(vec!["w1".into(), "w2".into()]),
        600,
    );
    let ids = post_ids("chunk", 600);

    store.reset_statement_count();
    let one_page = store.recipient_statuses_for_posts(&ids[..200]).unwrap();
    assert_eq!(
        store.statement_count(),
        1,
        "a snapshot-sized request stays one statement"
    );
    assert_eq!(one_page.len(), 200);

    // 600 ids spans two STATUS_BATCH_CHUNKs, never one statement per post.
    store.reset_statement_count();
    let all = store.recipient_statuses_for_posts(&ids).unwrap();
    assert_eq!(
        store.statement_count(),
        ids.chunks(STATUS_BATCH_CHUNK).count(),
        "one statement per id chunk"
    );
    assert_eq!(all.len(), 600);

    store.reset_statement_count();
    assert!(store.recipient_statuses_for_posts(&[]).unwrap().is_empty());
    assert_eq!(store.statement_count(), 0, "no ids, no query");
}

/// `posts()` carries each page's mentions in the page query, so a page costs
/// one statement no matter how many mentions its posts carry.
#[test]
fn a_posts_page_costs_one_statement_whatever_the_mention_count() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("team.sqlite3");

    let mut sparse = TeamStore::open(&dir.path().join("sparse").join("team.sqlite3")).unwrap();
    sparse.save_team(&team("sparse", &["w1"])).unwrap();
    add_posts(&mut sparse, "sparse", &Recipients::Explicit(vec![]), 100);

    let mut dense = TeamStore::open(&path).unwrap();
    dense
        .save_team(&team("dense", &["w1", "w2", "w3", "w4"]))
        .unwrap();
    add_posts(
        &mut dense,
        "dense",
        &Recipients::Explicit(vec!["w1".into(), "w2".into(), "w3".into(), "w4".into()]),
        100,
    );

    sparse.reset_statement_count();
    let sparse_page = sparse.posts("sparse", 0, MAX_PAGE_SIZE).unwrap();
    let sparse_statements = sparse.statement_count();
    dense.reset_statement_count();
    let dense_page = dense.posts("dense", 0, MAX_PAGE_SIZE).unwrap();
    let dense_statements = dense.statement_count();

    assert_eq!(sparse_page.len(), 100);
    assert_eq!(dense_page.len(), 100, "limit counts posts, not join rows");
    assert!(
        sparse_page.iter().all(|(_, post)| post.mentions.is_empty()),
        "the sparse page mentions nobody"
    );
    assert!(
        dense_page.iter().all(|(_, post)| post.mentions.len() == 4),
        "the dense page carries four mentions per post"
    );
    assert_eq!(sparse_statements, 1, "one statement per page");
    assert_eq!(dense_statements, 1, "the same page costs the same");
}

/// The pending count is the recipient's own backlog in one team, not every
/// team that happens to carry the same member id.
#[test]
fn pending_mention_count_is_scoped_to_one_team() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_in(&dir);
    store.save_team(&team("home", &["w1"])).unwrap();
    store.save_team(&team("other", &["w1"])).unwrap();
    add_posts(
        &mut store,
        "home",
        &Recipients::Explicit(vec!["w1".into()]),
        4,
    );
    add_posts(
        &mut store,
        "other",
        &Recipients::Explicit(vec!["w1".into()]),
        7,
    );
    store.acknowledge("home-p0000", "w1").unwrap();

    assert_eq!(store.pending_mention_count("home", "w1").unwrap(), 3);
    assert_eq!(store.pending_mention_count("other", "w1").unwrap(), 7);
    assert_eq!(store.pending_mention_count("home", "nobody").unwrap(), 0);
}

/// The count seeks the recipient's mentions through the index instead of
/// scanning the mention table, which is what makes it cheaper than the post
/// scan it replaced.
#[test]
fn pending_mention_count_uses_the_recipient_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_in(&dir);
    store.save_team(&team("plan", &["w1"])).unwrap();
    add_posts(
        &mut store,
        "plan",
        &Recipients::Explicit(vec!["w1".into()]),
        10,
    );

    let mut stmt = store
        .db
        .prepare(&format!("EXPLAIN QUERY PLAN {PENDING_MENTIONS_SQL}"))
        .unwrap();
    let plan: Vec<String> = stmt
        .query_map(params!["plan", "w1"], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    let plan = plan.join("\n");
    assert!(
        plan.contains("mentions_recipient_idx"),
        "expected the recipient index in the plan: {plan}"
    );
    assert!(
        !plan.contains("SCAN mentions"),
        "the count must not scan every mention: {plan}"
    );
    assert_eq!(store.pending_mention_count("plan", "w1").unwrap(), 10);
}
