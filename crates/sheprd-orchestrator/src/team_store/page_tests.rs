#![allow(clippy::unwrap_used)]

use super::{
    Author, Error, MAX_PAGE_SIZE, Member, Post, Recipients, SessionTarget, Team, TeamStore,
};
use tempfile::TempDir;

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
        target: target(n),
    }
}

/// A team with `posts` messages whose bodies are numbered `body-0000` upward.
/// Every `mentioned`-th post mentions `worker`, so a page can be checked for
/// the mentions it must carry. The `TempDir` is kept next to the store so the
/// database outlives no one's cleanup.
fn seeded(posts: usize, mentioned: usize) -> (TeamStore, TempDir) {
    let dir = TempDir::new().unwrap();
    // The store insists its database's directory is private, so it lives in a
    // nested path like every other team store test.
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "team".into(),
            name: "team".into(),
            supervisor: member("lead", 1),
            members: vec![member("worker", 2)],
        })
        .unwrap();
    let human = Author::Human {
        user_id: "human".into(),
    };
    for index in 0..posts {
        let recipients = if index % mentioned.max(1) == 0 {
            Recipients::Explicit(vec!["worker".into()])
        } else {
            Recipients::Explicit(Vec::new())
        };
        store
            .add_post_with_recipients(
                &format!("post-{index:04}"),
                "team",
                &human,
                None,
                &format!("body-{index:04}"),
                &recipients,
            )
            .unwrap();
    }
    (store, dir)
}

fn bodies(page: &[(i64, Post)]) -> Vec<String> {
    page.iter().map(|(_, post)| post.body.clone()).collect()
}

fn sequences(page: &[(i64, Post)]) -> Vec<i64> {
    page.iter().map(|(sequence, _)| *sequence).collect()
}

fn range(from: usize, to: usize) -> Vec<String> {
    (from..to).map(|index| format!("body-{index:04}")).collect()
}

#[test]
fn latest_posts_returns_the_newest_page_oldest_first() {
    let (store, _dir) = seeded(250, 7);

    let page = store.latest_posts("team", None, 100).unwrap();

    assert_eq!(page.len(), 100);
    assert_eq!(bodies(&page), range(150, 250));
    assert_eq!(
        sequences(&page),
        (151..=250).collect::<Vec<i64>>(),
        "no gap and no repeat inside a page"
    );
}

#[test]
fn latest_posts_pages_older_from_a_cursor_without_gaps_or_overlap() {
    let (store, _dir) = seeded(250, 7);

    let newest = store.latest_posts("team", None, 100).unwrap();
    let middle = store
        .latest_posts("team", Some(sequences(&newest)[0]), 100)
        .unwrap();
    let oldest = store
        .latest_posts("team", Some(sequences(&middle)[0]), 100)
        .unwrap();
    let past_the_start = store
        .latest_posts("team", Some(sequences(&oldest)[0]), 100)
        .unwrap();

    assert_eq!(bodies(&newest), range(150, 250));
    assert_eq!(bodies(&middle), range(50, 150));
    assert_eq!(bodies(&oldest), range(0, 50));
    assert!(
        past_the_start.is_empty(),
        "the oldest end of the team is reached"
    );

    let all: Vec<String> = [&oldest, &middle, &newest]
        .into_iter()
        .flat_map(|page| bodies(page))
        .collect();
    assert_eq!(all, range(0, 250), "every post exactly once");
}

#[test]
fn latest_posts_carries_the_mentions_of_its_own_page() {
    let (store, _dir) = seeded(250, 7);

    let page = store.latest_posts("team", None, 100).unwrap();

    let mentioned: Vec<String> = page
        .iter()
        .filter(|(_, post)| !post.mentions.is_empty())
        .map(|(_, post)| post.body.clone())
        .collect();
    // Every seventh post, counting from the oldest, is addressed to `worker`.
    assert_eq!(
        mentioned,
        (154..250)
            .step_by(7)
            .map(|i| format!("body-{i:04}"))
            .collect::<Vec<String>>()
    );
}

#[test]
fn latest_posts_honours_the_same_page_bound_as_posts() {
    let (store, _dir) = seeded(10, 3);

    assert!(
        matches!(
            store.latest_posts("team", None, 0),
            Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE
            })
        ),
        "an empty page is rejected, not read"
    );
    assert!(
        matches!(
            store.latest_posts("team", None, MAX_PAGE_SIZE + 1),
            Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE
            })
        ),
        "the newest read is bounded like forward paging"
    );
    assert!(
        matches!(
            store.posts("team", 0, MAX_PAGE_SIZE + 1),
            Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE
            })
        ),
        "forward paging keeps its bound"
    );
}

#[test]
fn posts_still_pages_forward_from_a_cursor_after_the_newest_read() {
    let (store, _dir) = seeded(250, 7);

    let first = store.posts("team", 0, 100).unwrap();
    let next = store
        .posts("team", *sequences(&first).last().unwrap(), 100)
        .unwrap();

    assert_eq!(bodies(&first), range(0, 100));
    assert_eq!(bodies(&next), range(100, 200));
    let backwards = store.latest_posts("team", Some(101), 100).unwrap();
    assert_eq!(bodies(&backwards), range(0, 100));
    assert_eq!(
        sequences(&backwards).last().map(|sequence| sequence + 1),
        sequences(&next).first().copied(),
        "a newest-first page ends exactly where forward paging continues"
    );
}
