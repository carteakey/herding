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
fn removing_a_member_deletes_their_mentions_and_acknowledgements() {
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

    assert_eq!(store.posts("t", 0, 10).unwrap()[0].1.mentions, ["staying"]);
    let statuses = store.recipient_statuses("p").unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].0, "staying");
    assert!(matches!(statuses[0].1, RecipientStatus::Acknowledged(_)));
    assert_eq!(
        store
            .acknowledgements("p")
            .unwrap()
            .iter()
            .map(|ack| ack.recipient_id.as_str())
            .collect::<Vec<_>>(),
        ["staying"]
    );
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

    assert!(store.recipient_statuses("p-a").unwrap().is_empty());
    assert_eq!(store.recipient_statuses("p-b").unwrap().len(), 1);
}
