#![allow(clippy::unwrap_used)]

use super::*;

fn member(id: &str, session: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
        model: None,
        account: None,
        target: SessionTarget {
            host: "local".into(),
            session: session.into(),
            workspace: Some("ws-1".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

#[test]
fn team_posts_replies_mentions_and_acknowledgements_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("team.sqlite3");
    let team = Team {
        team_id: "team-a".into(),
        name: "A team".into(),
        supervisor: member("lead", "session-lead"),
        members: vec![member("worker", "session-worker")],
    };
    {
        let mut store = TeamStore::open(&path).unwrap();
        store.save_team(&team).unwrap();
        assert_eq!(store.team("team-a").unwrap().unwrap(), team);
        let post = store
            .add_post(
                "p1",
                "team-a",
                &Author::Member {
                    member_id: "lead".into(),
                },
                None,
                "Please inspect",
                &["worker".into()],
            )
            .unwrap();
        let reply = store
            .add_post(
                "p2",
                "team-a",
                &Author::Member {
                    member_id: "worker".into(),
                },
                Some("p1"),
                "On it",
                &[],
            )
            .unwrap();
        assert_eq!(reply.reply_to.as_deref(), Some("p1"));
        let ack = store.acknowledge("p1", "worker").unwrap();
        assert_eq!(ack.post_id, post.post_id);
        assert_eq!(
            store.posts("team-a", 0, 10).unwrap()[0].1.created_at,
            post.created_at
        );
    }
    let store = TeamStore::open(&path).unwrap();
    let posts = store.posts("team-a", 0, 10).unwrap();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].1.mentions, ["worker"]);
    assert_eq!(posts[1].1.reply_to.as_deref(), Some("p1"));
    assert_eq!(
        store
            .acknowledgements("p1")
            .unwrap()
            .iter()
            .map(|a| a.recipient_id.as_str())
            .collect::<Vec<_>>(),
        ["worker"]
    );
    assert!(matches!(
        store.recipient_statuses("p1").unwrap()[0].1,
        RecipientStatus::Acknowledged(_)
    ));
}

#[test]
fn bounds_and_missing_references_are_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    assert!(matches!(
        store.add_post(
            "p",
            "absent",
            &Author::Human {
                user_id: "u".into()
            },
            None,
            "hi",
            &[]
        ),
        Err(Error::TeamNotFound(_))
    ));
    let too_long = "x".repeat(MAX_TEXT_BYTES + 1);
    let team = Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor: member("lead", "s"),
        members: vec![],
    };
    store.save_team(&team).unwrap();
    store
        .add_post(
            "p",
            "t",
            &Author::Member {
                member_id: "lead".into(),
            },
            None,
            "valid",
            &[],
        )
        .unwrap();
    assert!(matches!(
        store.add_post(
            "p",
            "t",
            &Author::Member {
                member_id: "lead".into()
            },
            None,
            &too_long,
            &[]
        ),
        Err(Error::Invalid { field: "body", .. })
    ));
    assert!(matches!(
        store.add_post(
            "p",
            "t",
            &Author::Member {
                member_id: "outsider".into()
            },
            None,
            "hi",
            &[]
        ),
        Err(Error::MemberNotInTeam { .. })
    ));
    assert!(matches!(
        store.add_post(
            "p",
            "t",
            &Author::Member {
                member_id: "lead".into()
            },
            Some("elsewhere"),
            "hi",
            &[]
        ),
        Err(Error::InvalidReply(_))
    ));
    assert!(matches!(
        store.acknowledge("missing", "lead"),
        Err(Error::PostNotFound(_))
    ));
    assert!(matches!(
        store.acknowledge("p", "lead"),
        Err(Error::NotMentioned { .. })
    ));
    assert!(matches!(
        store.posts("t", 0, MAX_PAGE_SIZE + 1),
        Err(Error::Invalid {
            field: "page size",
            ..
        })
    ));
}

#[test]
fn mentioned_recipient_starts_pending_and_ack_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "Team".into(),
            supervisor: member("lead", "s1"),
            members: vec![member("m", "s2")],
        })
        .unwrap();
    store
        .add_post(
            "p",
            "t",
            &Author::Member {
                member_id: "lead".into(),
            },
            None,
            "hello",
            &["m".into()],
        )
        .unwrap();
    assert_eq!(
        store.recipient_statuses("p").unwrap()[0].1,
        RecipientStatus::Pending
    );
    let first = store.acknowledge("p", "m").unwrap();
    let second = store.acknowledge("p", "m").unwrap();
    assert_eq!(first, second);
    let page = store.posts("t", 0, 1).unwrap();
    assert_eq!(page[0].0, 1);
    assert!(store.posts("t", page[0].0, 1).unwrap().is_empty());
}

#[test]
fn human_posts_have_explicit_human_attribution() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "Team".into(),
            supervisor: member("lead", "s1"),
            members: vec![],
        })
        .unwrap();
    let post = store
        .add_post(
            "human-post",
            "t",
            &Author::Human {
                user_id: "person-1".into(),
            },
            None,
            "Hello team",
            &[],
        )
        .unwrap();
    assert_eq!(
        post.author,
        Author::Human {
            user_id: "person-1".into()
        }
    );
    assert_eq!(store.posts("t", 0, 10).unwrap()[0].1.author, post.author);
}

#[test]
fn membership_cap_counts_existing_members_and_rejects_duplicate_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    let base = Team {
        team_id: "t".into(),
        name: "Team".into(),
        supervisor: member("lead", "s0"),
        members: (0..(MAX_MEMBERS - 1))
            .map(|n| member(&format!("m{n}"), "s"))
            .collect(),
    };
    store.save_team(&base).unwrap();
    let oversized = Team {
        members: vec![member("new", "s")],
        ..base.clone()
    };
    assert!(matches!(
        store.save_team(&oversized),
        Err(Error::TooManyMembers { .. })
    ));
    let duplicate = Team {
        team_id: "other".into(),
        name: "Other".into(),
        supervisor: member("same", "s0"),
        members: vec![member("same", "s1")],
    };
    assert!(matches!(
        store.save_team(&duplicate),
        Err(Error::DuplicateMember(_))
    ));
}

#[test]
fn existing_member_identity_cannot_move_to_another_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    let original = Team {
        team_id: "t".into(),
        name: "Team".into(),
        supervisor: member("lead", "session-a"),
        members: vec![],
    };
    store.save_team(&original).unwrap();
    let moved = Team {
        supervisor: member("lead", "session-b"),
        ..original
    };
    assert!(matches!(
        store.save_team(&moved),
        Err(Error::MemberTargetChanged { .. })
    ));
}

#[test]
fn broadcast_resolves_member_snapshot_and_excludes_member_author() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    let team = Team {
        team_id: "t".into(),
        name: "Team".into(),
        supervisor: member("lead", "s0"),
        members: vec![member("m1", "s1")],
    };
    store.save_team(&team).unwrap();
    let post = store
        .add_post_with_recipients(
            "broadcast",
            "t",
            &Author::Member {
                member_id: "lead".into(),
            },
            None,
            "hello",
            &Recipients::All,
        )
        .unwrap();
    assert_eq!(post.mentions, ["m1"]);
    store
        .save_team(&Team {
            members: vec![member("m1", "s1"), member("m2", "s2")],
            ..team
        })
        .unwrap();
    assert_eq!(store.posts("t", 0, 10).unwrap()[0].1.mentions, ["m1"]);
}

#[test]
fn explicit_duplicate_recipients_fail_and_team_list_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    let team = Team {
        team_id: "t".into(),
        name: "Team".into(),
        supervisor: member("lead", "s0"),
        members: vec![member("m", "s1")],
    };
    store.save_team(&team).unwrap();
    assert!(matches!(
        store.add_post_with_recipients(
            "p",
            "t",
            &Author::Human {
                user_id: "human".into()
            },
            None,
            "hello",
            &Recipients::Explicit(vec!["m".into(), "m".into()])
        ),
        Err(Error::DuplicateRecipient(_))
    ));
    store
        .save_team(&Team {
            team_id: "a".into(),
            ..team
        })
        .unwrap();
    assert_eq!(store.list_teams(None, 1).unwrap().len(), 1);
    assert_eq!(store.list_teams(Some("a"), 10).unwrap()[0].team_id, "t");
    assert!(matches!(
        store.list_teams(None, MAX_PAGE_SIZE + 1),
        Err(Error::Invalid {
            field: "page size",
            ..
        })
    ));
}

fn perf_member(id: &str) -> Member {
    member(id, &format!("session-{id}"))
}

#[test]
fn posts_page_batches_mentions_with_stable_pagination() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("team.sqlite3");
    let mut store = TeamStore::open(&path).unwrap();
    store
        .save_team(&Team {
            team_id: "team-a".into(),
            name: "A team".into(),
            supervisor: perf_member("lead"),
            members: vec![perf_member("w1"), perf_member("w2")],
        })
        .unwrap();
    let human = Author::Human {
        user_id: "human".into(),
    };
    // Deterministic mention counts: no mentions, one, two, repeating.
    for index in 0..250 {
        let recipients = match index % 3 {
            0 => Recipients::Explicit(vec![]),
            1 => Recipients::Explicit(vec!["w1".into()]),
            _ => Recipients::Explicit(vec!["w2".into(), "w1".into()]),
        };
        store
            .add_post_with_recipients(
                &format!("p{index:04}"),
                "team-a",
                &human,
                None,
                "body",
                &recipients,
            )
            .unwrap();
    }

    // Pagination still counts posts, not join rows.
    let mut cursor = 0i64;
    let mut seen = Vec::new();
    loop {
        let page = store.posts("team-a", cursor, 100).unwrap();
        let Some((last, _)) = page.last() else { break };
        cursor = *last;
        let full = page.len() == 100;
        seen.extend(page.into_iter().map(|(_, post)| post));
        if !full {
            break;
        }
    }
    assert_eq!(seen.len(), 250, "every post appears exactly once");
    for (index, post) in seen.iter().enumerate() {
        assert_eq!(post.post_id, format!("p{index:04}"));
        let expected: Vec<String> = match index % 3 {
            0 => vec![],
            1 => vec!["w1".into()],
            _ => vec!["w1".into(), "w2".into()],
        };
        assert_eq!(post.mentions, expected, "mentions sorted per post");
    }

    // The batched shape survives a reopen from disk.
    drop(store);
    let store = TeamStore::open(&path).unwrap();
    let page = store.posts("team-a", 0, 2).unwrap();
    assert_eq!(page.len(), 2);
    assert!(page[0].1.mentions.is_empty());
    assert_eq!(page[1].1.mentions, vec!["w1".to_owned()]);
}

#[test]
fn batched_statuses_match_single_post_statuses() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("team.sqlite3");
    let mut store = TeamStore::open(&path).unwrap();
    store
        .save_team(&Team {
            team_id: "team-b".into(),
            name: "B team".into(),
            supervisor: perf_member("lead"),
            members: vec![perf_member("w1"), perf_member("w2")],
        })
        .unwrap();
    let human = Author::Human {
        user_id: "human".into(),
    };
    for index in 0..600 {
        store
            .add_post_with_recipients(
                &format!("p{index:04}"),
                "team-b",
                &human,
                None,
                "body",
                &match index % 3 {
                    0 => Recipients::Explicit(vec![]),
                    1 => Recipients::Explicit(vec!["w1".into()]),
                    _ => Recipients::Explicit(vec!["w1".into(), "w2".into()]),
                },
            )
            .unwrap();
    }
    // Acknowledge every third mentionable post for w1.
    for index in (0..600).step_by(3) {
        let _ = store.acknowledge(&format!("p{index:04}"), "w1");
    }

    // 600 ids spans two STATUS_BATCH_CHUNKs.
    let ids: Vec<String> = (0..600).map(|index| format!("p{index:04}")).collect();
    let batched = store.recipient_statuses_for_posts(&ids).unwrap();
    assert_eq!(batched.len(), 600, "every requested post has an entry");
    for id in &ids {
        let per_post = store.recipient_statuses(id).unwrap();
        assert_eq!(&batched[id], &per_post, "parity for {id}");
    }
    // An empty request is an empty map, not an error.
    assert!(store.recipient_statuses_for_posts(&[]).unwrap().is_empty());
}

#[test]
fn pending_mention_count_matches_the_old_scan_and_departures() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("team.sqlite3");
    let mut store = TeamStore::open(&path).unwrap();
    store
        .save_team(&Team {
            team_id: "team-c".into(),
            name: "C team".into(),
            supervisor: perf_member("lead"),
            members: vec![perf_member("w1"), perf_member("w2")],
        })
        .unwrap();
    let human = Author::Human {
        user_id: "human".into(),
    };
    for index in 0..5 {
        store
            .add_post_with_recipients(
                &format!("a{index}"),
                "team-c",
                &human,
                None,
                "for w1",
                &Recipients::Explicit(vec!["w1".into()]),
            )
            .unwrap();
    }
    for index in 0..3 {
        store
            .add_post_with_recipients(
                &format!("b{index}"),
                "team-c",
                &human,
                None,
                "for w2",
                &Recipients::Explicit(vec!["w2".into()]),
            )
            .unwrap();
    }
    store
        .add_post_with_recipients(
            "c0",
            "team-c",
            &human,
            None,
            "no mentions",
            &Recipients::Explicit(vec![]),
        )
        .unwrap();
    store.acknowledge("a0", "w1").unwrap();
    store.acknowledge("a1", "w1").unwrap();

    assert_eq!(store.pending_mention_count("team-c", "w1").unwrap(), 3);
    assert_eq!(store.pending_mention_count("team-c", "w2").unwrap(), 3);
    assert_eq!(store.pending_mention_count("team-c", "lead").unwrap(), 0);
    // Departure removes the mention rows, so the count follows abb045ee.
    store.remove_member("team-c", "w2").unwrap();
    assert_eq!(store.pending_mention_count("team-c", "w2").unwrap(), 0);
}
