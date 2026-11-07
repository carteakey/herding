#![allow(clippy::unwrap_used)]

use super::*;

fn member(id: &str, session: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: format!("bee-{session}"),
        target: SessionTarget {
            host: "local".into(),
            session: session.into(),
            workspace: Some("ws-1".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

fn store_with_hex_members() -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "T".into(),
            supervisor: member("aabb00cc-1111-4222-8333-444444444444", "s0"),
            members: vec![
                member("aabb00cc-5555-4666-8777-888888888888", "s1"),
                member("aabb00ff-9999-4aaa-8bbb-cccccccccccc", "s2"),
            ],
        })
        .unwrap();
    (dir, store)
}

#[test]
fn task_post_and_finding_references_resolve_by_unique_hex_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "T".into(),
            supervisor: member("lead", "s0"),
            members: vec![],
        })
        .unwrap();
    let author = Author::Member {
        member_id: "lead".into(),
    };
    // Two tasks share an 8-char head so both the unique and ambiguous paths
    // are exercised against real stored rows.
    let task_a = "cafe0000-1111-4222-8333-444444444444";
    let task_b = "cafe0000-5555-4666-8777-888888888888";
    let task_c = "deadbeef-9999-4aaa-8bbb-cccccccccccc";
    for (id, title) in [(task_a, "A"), (task_b, "B"), (task_c, "C")] {
        store
            .create_task(id, "t", &author, title, "brief", None)
            .unwrap();
    }
    let post_a = store
        .add_post(
            "cafe1111-1111-4222-8333-444444444444",
            "t",
            &author,
            None,
            "a",
            &[],
        )
        .unwrap();
    let finding_a = store
        .add_finding(
            "cafe2222-1111-4222-8333-444444444444",
            "t",
            &author,
            Some(task_a),
            "found",
            "evidence",
            Confidence::Medium,
            None,
        )
        .unwrap();

    // Full IDs are returned as-is.
    assert_eq!(store.resolve_task_ref("t", task_a).unwrap(), task_a);
    assert_eq!(
        store.resolve_post_ref("t", &post_a.post_id).unwrap(),
        post_a.post_id
    );
    assert_eq!(
        store
            .resolve_finding_ref("t", &finding_a.finding_id)
            .unwrap(),
        finding_a.finding_id
    );
    // A unique prefix resolves to the full ID, dashed or not, any case.
    assert_eq!(
        store.resolve_task_ref("t", "cafe0000-1111").unwrap(),
        task_a
    );
    assert_eq!(store.resolve_task_ref("t", "CAFE00005555").unwrap(), task_b);
    assert_eq!(store.resolve_task_ref("t", "deadb").unwrap(), task_c);
    assert_eq!(
        store.resolve_post_ref("t", "cafe1111").unwrap(),
        post_a.post_id
    );
    assert_eq!(
        store.resolve_finding_ref("t", "cafe2222").unwrap(),
        finding_a.finding_id
    );
    // Ambiguous prefixes are refused with the candidates listed.
    let ambiguous = store.resolve_task_ref("t", "cafe").unwrap_err();
    assert!(matches!(ambiguous, Error::AmbiguousId { kind: "task", .. }));
    let message = ambiguous.to_string();
    assert!(
        message.contains(task_a) && message.contains(task_b),
        "candidates listed: {message}"
    );
    // Shorter than four hex chars, non-hex, and unmatched tokens pass through.
    assert_eq!(store.resolve_task_ref("t", "caf").unwrap(), "caf");
    assert_eq!(store.resolve_task_ref("t", "zzzz").unwrap(), "zzzz");
    assert_eq!(store.resolve_task_ref("t", "cafe9").unwrap(), "cafe9");
    // Tokens of another team's prefix space do not leak across teams.
    store
        .save_team(&Team {
            team_id: "u".into(),
            name: "U".into(),
            supervisor: member("lead-u", "u0"),
            members: vec![],
        })
        .unwrap();
    assert_eq!(store.resolve_task_ref("u", "cafe0").unwrap(), "cafe0");
}

#[test]
fn member_references_resolve_by_unique_prefix_after_names_and_exact_ids() {
    let (_dir, mut store) = store_with_hex_members();
    let team = store.team("t").unwrap().unwrap();
    let supervisor_id = team.supervisor.member_id.clone();
    let worker_a = team.members[0].member_id.clone();
    let worker_b = team.members[1].member_id.clone();

    // Exact IDs and the supervisor alias keep their precedence.
    assert_eq!(resolve_member_ref(&team, &worker_a).unwrap(), worker_a);
    assert_eq!(
        resolve_member_ref(&team, "supervisor").unwrap(),
        supervisor_id
    );
    // A unique dashed prefix resolves.
    assert_eq!(
        resolve_member_ref(&team, "aabb00cc-5555").unwrap(),
        worker_a
    );
    // A shared head is ambiguous, and both candidates are named.
    let ambiguous = resolve_member_ref(&team, "aabb00cc").unwrap_err();
    let message = ambiguous.to_string();
    assert!(
        message.contains(&worker_a) && message.contains(&supervisor_id),
        "{message}"
    );
    // Display names keep their precedence and still fail loudly when unknown.
    assert_eq!(resolve_member_ref(&team, "bee-s1").unwrap(), worker_a);
    assert!(matches!(
        resolve_member_ref(&team, "nobody"),
        Err(Error::UnknownMention(_))
    ));

    // Mentions follow the same rules: a unique prefix targets one member.
    store
        .add_post_with_recipients(
            "p",
            "t",
            &Author::Member {
                member_id: supervisor_id.clone(),
            },
            None,
            "ping",
            &Recipients::Explicit(vec![worker_b.clone()]),
        )
        .unwrap();
    assert_eq!(
        store.posts("t", 0, 10).unwrap()[0].1.mentions,
        [worker_b.clone()]
    );
    let recipients = resolve_recipients(
        &team,
        &Author::Member {
            member_id: supervisor_id.clone(),
        },
        &["aabb00ff".to_owned()],
        "",
    )
    .unwrap();
    assert_eq!(recipients, Recipients::Explicit(vec![worker_b]));
}
