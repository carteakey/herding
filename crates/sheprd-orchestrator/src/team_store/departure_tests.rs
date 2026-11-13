//! Removing a member also removes their bulletin delivery receipts.

#![allow(clippy::unwrap_used)]

use super::*;

fn member(id: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
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

#[test]
fn removing_a_member_tombstones_the_row_and_keeps_their_delivery_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    store
        .save_team(&team("t", &["departing", "staying"]))
        .unwrap();
    store
        .add_post(
            "p",
            "t",
            &Author::Member {
                member_id: "lead".into(),
            },
            None,
            "Please inspect",
            &["departing".into(), "staying".into()],
        )
        .unwrap();
    store.acknowledge("p", "departing").unwrap();
    store.acknowledge("p", "staying").unwrap();

    store.remove_member("t", "departing").unwrap();

    // The row is tombstoned, not deleted: the roster no longer lists them,
    // and the board's deliveries hide their receipts while they are gone.
    let roster = store.team("t").unwrap().unwrap();
    assert!(
        !roster
            .members
            .iter()
            .any(|member| member.member_id == "departing")
    );
    assert_eq!(
        store.posts("t", 0, 10).unwrap()[0].1.mentions,
        ["staying"],
        "the tombstoned member's receipt waits for a restore"
    );

    // Re-registering the same member and pane restores their identity and
    // their delivery state instead of minting a new member.
    let returning = Member {
        member_id: "departing-resumed".into(),
        display_name: "departing".into(),
        target: SessionTarget {
            host: "local".into(),
            session: "session-departing-resumed".into(),
            workspace: Some("ws".into()),
            pane: Some("pane-departing".into()),
        },
    };
    let restored = store
        .register_member(
            "t",
            "departing-resumed",
            "departing",
            &returning.target,
            &["local".into()],
        )
        .unwrap();
    assert_eq!(
        restored.member_id, "departing",
        "the row's own id comes back"
    );
    assert_eq!(
        store.posts("t", 0, 10).unwrap()[0].1.mentions,
        ["departing", "staying"],
        "the restored member's delivery state came back with them"
    );
    let statuses = store.recipient_statuses("p").unwrap();
    let departing = statuses
        .iter()
        .find(|(recipient, _)| recipient == "departing")
        .unwrap();
    assert!(matches!(departing.1, RecipientStatus::Acknowledged(_)));
}

#[test]
fn removing_a_member_leaves_another_teams_delivery_rows_intact() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("team.sqlite3")).unwrap();
    store.save_team(&team("a", &["worker"])).unwrap();
    store.save_team(&team("b", &["worker"])).unwrap();
    for (team_id, post_id) in [("a", "p-a"), ("b", "p-b")] {
        store
            .add_post(
                post_id,
                team_id,
                &Author::Member {
                    member_id: "lead".into(),
                },
                None,
                "Please inspect",
                &["worker".into()],
            )
            .unwrap();
    }

    store.remove_member("a", "worker").unwrap();

    // The tombstone deletes nothing: both teams keep their delivery rows,
    // and team a's roster no longer lists the removed worker.
    assert_eq!(store.recipient_statuses("p-a").unwrap().len(), 1);
    assert_eq!(store.recipient_statuses("p-b").unwrap().len(), 1);
    let roster_a = store.team("a").unwrap().unwrap();
    assert!(
        !roster_a
            .members
            .iter()
            .any(|member| member.member_id == "worker")
    );
}
