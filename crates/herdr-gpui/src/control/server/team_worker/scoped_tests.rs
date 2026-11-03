#![allow(clippy::unwrap_used)]
use super::*;
use herdr_orchestrator::team_store::{Member, SessionTarget};

fn target(n: u8) -> SessionTarget {
    SessionTarget {
        host: "boot".into(),
        session: format!("session-{n}"),
        workspace: Some("w".into()),
        pane: Some(format!("p{n}")),
    }
}

fn store() -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("t.sqlite3")).unwrap();
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
    (dir, store)
}

fn post(
    store: &mut TeamStore,
    caller: &SessionTarget,
    mentions: Vec<String>,
) -> Result<Reply, Error> {
    scoped(
        store,
        "t",
        caller,
        ScopedAction::BulletinPost {
            post_id: uuid::Uuid::new_v4().to_string(),
            reply_to: None,
            text: "hello".into(),
            mentions,
        },
    )
}

#[test]
fn a_session_that_is_not_a_member_is_refused_for_every_action() {
    let (_dir, mut store) = store();
    let stranger = target(9);
    // Same pane and workspace, different session: a reused pane is no member.
    let reused = SessionTarget {
        session: "other".into(),
        ..target(2)
    };
    for caller in [&stranger, &reused] {
        assert!(matches!(
            scoped(&mut store, "t", caller, ScopedAction::Roster),
            Err(Error::NotMember)
        ));
        assert!(matches!(
            post(&mut store, caller, vec![]),
            Err(Error::NotMember)
        ));
    }
    assert!(matches!(
        scoped(&mut store, "missing", &target(1), ScopedAction::Roster),
        Err(Error::NotMember)
    ));
}

#[test]
fn a_member_post_is_pending_for_the_recipient_until_it_acknowledges() {
    let (_dir, mut store) = store();
    let Reply::ScopedPosted { post, .. } = post(&mut store, &target(1), vec![]).unwrap() else {
        panic!("expected a posted reply");
    };
    assert_eq!(post.mentions, vec!["worker"]);
    let list = |store: &mut TeamStore| {
        let Reply::ScopedBulletin { entries, .. } = scoped(
            store,
            "t",
            &target(2),
            ScopedAction::BulletinList {
                after: 0,
                limit: 10,
            },
        )
        .unwrap() else {
            panic!("expected a bulletin");
        };
        entries
    };
    assert!(matches!(
        list(&mut store)[0].2,
        Some(RecipientStatus::Pending)
    ));
    scoped(
        &mut store,
        "t",
        &target(2),
        ScopedAction::BulletinAck {
            message_id: post.post_id.clone(),
        },
    )
    .unwrap();
    assert!(matches!(
        list(&mut store)[0].2,
        Some(RecipientStatus::Acknowledged(_))
    ));
}

#[test]
fn only_a_recipient_can_acknowledge_a_directed_post() {
    let (_dir, mut store) = store();
    let Reply::ScopedPosted { post, .. } =
        post(&mut store, &target(2), vec!["lead".into()]).unwrap()
    else {
        panic!("expected a posted reply");
    };
    // The author was not a recipient of its own post.
    assert!(
        scoped(
            &mut store,
            "t",
            &target(2),
            ScopedAction::BulletinAck {
                message_id: post.post_id,
            },
        )
        .is_err()
    );
}
