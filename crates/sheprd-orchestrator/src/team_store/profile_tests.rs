//! A member's self-reported model and account: recorded at registration,
//! kept across a flagless re-registration, and added to older databases.
#![allow(clippy::unwrap_used)]

use super::*;

fn hosts() -> HostContext {
    HostContext {
        caller_boot: "m1".into(),
        live_hosts: vec!["m1".into()],
        endpoint_ids: vec!["m1".into()],
        live_panes: Vec::new(),
    }
}

fn target(session: &str) -> SessionTarget {
    SessionTarget {
        host: "m1".into(),
        session: session.into(),
        workspace: Some("w".into()),
        pane: Some("p1".into()),
    }
}

fn profile(model: &str, account: &str) -> MemberProfile {
    MemberProfile {
        model: Some(model.into()),
        account: Some(account.into()),
    }
}

fn store_with_team() -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    let supervisor = Member {
        member_id: "sup".into(),
        display_name: "supervisor".into(),
        model: None,
        account: None,
        target: SessionTarget {
            host: "m1".into(),
            session: "session-sup".into(),
            workspace: Some("w".into()),
            pane: Some("p-sup".into()),
        },
    };
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "T".into(),
            supervisor,
            members: Vec::new(),
        })
        .unwrap();
    (dir, store)
}

fn stored(store: &TeamStore, member_id: &str) -> Member {
    store
        .team("t")
        .unwrap()
        .unwrap()
        .members
        .into_iter()
        .find(|m| m.member_id == member_id)
        .unwrap()
}

#[test]
fn a_registration_records_the_reported_profile() {
    let (_dir, mut store) = store_with_team();
    let registered = store
        .register_member(
            "t",
            "w1",
            "zcode1",
            &profile("claude-sonnet-4-5", "team-plan"),
            &target("session-1"),
            &hosts(),
        )
        .unwrap();
    assert_eq!(registered.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(registered.account.as_deref(), Some("team-plan"));

    let persisted = stored(&store, "w1");
    assert_eq!(persisted.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(persisted.account.as_deref(), Some("team-plan"));
}

#[test]
fn a_flagless_reregistration_keeps_the_recorded_profile() {
    let (_dir, mut store) = store_with_team();
    store
        .register_member(
            "t",
            "w1",
            "zcode1",
            &profile("claude-sonnet-4-5", "team-plan"),
            &target("session-1"),
            &hosts(),
        )
        .unwrap();
    // A re-registration from the same pane names no profile: what the row
    // already records stays.
    let again = store
        .register_member(
            "t",
            "w2",
            "zcode1",
            &MemberProfile::default(),
            &target("session-1"),
            &hosts(),
        )
        .unwrap();
    assert_eq!(again.member_id, "w1");
    assert_eq!(again.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(again.account.as_deref(), Some("team-plan"));
    assert_eq!(stored(&store, "w1").account.as_deref(), Some("team-plan"));
}

#[test]
fn a_new_profile_replaces_the_recorded_one() {
    let (_dir, mut store) = store_with_team();
    store
        .register_member(
            "t",
            "w1",
            "zcode1",
            &profile("claude-sonnet-4-5", "team-plan"),
            &target("session-1"),
            &hosts(),
        )
        .unwrap();
    let moved = store
        .register_member(
            "t",
            "w2",
            "zcode1",
            &profile("glm-5.3", "zai-coding-plan"),
            &target("session-1"),
            &hosts(),
        )
        .unwrap();
    assert_eq!(moved.member_id, "w1");
    assert_eq!(moved.model.as_deref(), Some("glm-5.3"));
    assert_eq!(moved.account.as_deref(), Some("zai-coding-plan"));
}

#[test]
fn an_oversized_profile_is_refused() {
    let (_dir, mut store) = store_with_team();
    let too_long = "m".repeat(MAX_ID_BYTES + 1);
    assert!(matches!(
        store.register_member(
            "t",
            "w1",
            "zcode1",
            &profile(&too_long, "team-plan"),
            &target("session-1"),
            &hosts(),
        ),
        Err(Error::Invalid { field: "model", .. })
    ));
}

#[test]
fn an_older_database_gains_the_profile_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("db.sqlite3");
    {
        // A database written before the profile columns existed: create it
        // through the store, then take the two columns away again.
        let mut store = TeamStore::open(&path).unwrap();
        store
            .save_team(&Team {
                team_id: "t".into(),
                name: "T".into(),
                supervisor: Member {
                    member_id: "sup".into(),
                    display_name: "supervisor".into(),
                    model: None,
                    account: None,
                    target: SessionTarget {
                        host: "m1".into(),
                        session: "session-sup".into(),
                        workspace: Some("w".into()),
                        pane: Some("p-sup".into()),
                    },
                },
                members: Vec::new(),
            })
            .unwrap();
        store
            .register_member(
                "t",
                "w1",
                "zcode1",
                &MemberProfile::default(),
                &target("session-1"),
                &hosts(),
            )
            .unwrap();
    }
    {
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "ALTER TABLE members DROP COLUMN model;
             ALTER TABLE members DROP COLUMN account;",
        )
        .unwrap();
    }
    let mut store = TeamStore::open(&path).unwrap();
    let old = stored(&store, "w1");
    assert_eq!(old.model, None);
    assert_eq!(old.account, None);

    // The migrated table takes a profile like a fresh one.
    store
        .register_member(
            "t",
            "w2",
            "zcode1",
            &profile("glm-5.3", "zai-coding-plan"),
            &target("session-1"),
            &hosts(),
        )
        .unwrap();
    let updated = stored(&store, "w1");
    assert_eq!(updated.model.as_deref(), Some("glm-5.3"));
    assert_eq!(updated.account.as_deref(), Some("zai-coding-plan"));
}
