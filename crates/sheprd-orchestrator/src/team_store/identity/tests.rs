//! The identity audit trail: every member mutation leaves a durable,
//! atomic witness - the evidence the Nov 12 roster-cleanup investigation
//! lacked.

use super::super::{HostContext, Team};
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

/// The caller sits on the `local` endpoint, whose stable id is known, and
/// `live_panes` says which panes hold a live agent session.
fn hosts(live_panes: &[&str]) -> HostContext {
    HostContext {
        caller_boot: "local".into(),
        live_hosts: vec!["local".into()],
        endpoint_ids: vec!["local".into()],
        live_panes: live_panes.iter().map(|s| (*s).to_owned()).collect(),
    }
}

/// The returned guard owns the temporary directory and must outlive the
/// store's connection, so the pair stays alive together in the test.
fn open() -> (TeamStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team").join("team.sqlite3");
    (TeamStore::open(&path).unwrap(), dir)
}

#[test]
fn save_register_rebind_and_remove_are_all_recorded() {
    let (mut store, _dir) = open();
    store.save_team(&team("t", &["w1"])).unwrap();
    let events = store.identity_history("t", None).unwrap();
    assert_eq!(
        events.len(),
        2,
        "the supervisor and the new worker register"
    );
    assert!(events.iter().all(|event| event.op == "registered"));
    // The store's ops do not know their caller, so the actor records the
    // schema default phase 2 will replace.
    assert!(events.iter().all(|event| event.actor == "unknown"));
    assert!(events.iter().all(|event| event.terminal.is_none()));

    // A re-proof of the same target registers again, unchanged.
    store
        .register_member(
            "t",
            "w1",
            "w1",
            &member("w1").target,
            &hosts(&["pane-lead", "pane-w1"]),
        )
        .unwrap();
    let events = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| event.op == "registered"));

    // A resumed session on the same pane rebinds: the identity moved. The
    // resumed agent proves the same name and pane under a fresh member id;
    // the rebind keeps the row's own id.
    let mut moved = member("w1").target;
    moved.session = "session-w1-resumed".into();
    store
        .register_member(
            "t",
            "w1-resumed",
            "w1",
            &moved,
            &hosts(&["pane-lead", "pane-w1"]),
        )
        .unwrap();
    let rebound = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(rebound.last().unwrap().op, "rebound");
    assert_eq!(rebound.last().unwrap().member_id, "w1");
    assert_eq!(rebound.last().unwrap().session, "session-w1-resumed");

    // Removal records the identity that is about to disappear.
    store.remove_member("t", "w1").unwrap();
    let events = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(events.last().unwrap().op, "removed");
    assert_eq!(events.last().unwrap().session, "session-w1-resumed");
    assert_eq!(events.last().unwrap().pane.as_deref(), Some("pane-w1"));
}

#[test]
fn a_host_migration_is_recorded_as_a_rebind() {
    let (mut store, _dir) = open();
    // A pre-stable-key row registered under the boot id `boot-old`.
    let mut stale = team("t", &["w1"]);
    stale.members[0].target.host = "boot-old".into();
    store.save_team(&stale).unwrap();

    // The same session, workspace, and pane now prove themselves from the
    // stable endpoint `local`; the row's boot is neither the caller's nor a
    // live one, so the host migrates and the log gains the rebind.
    let mut proved = member("w1").target;
    proved.host = "local".into();
    let found = store
        .find_member_team(Some("t"), &proved, &hosts(&["pane-lead", "pane-w1"]))
        .unwrap()
        .expect("the stale row is adopted");
    assert_eq!(found.1.member_id, "w1");
    assert_eq!(found.1.target.host, "local");

    let history = store.identity_history("t", Some("w1")).unwrap();
    assert_eq!(history.len(), 2, "registered by the save, then rebound");
    assert_eq!(history.last().unwrap().op, "rebound");
    assert_eq!(history.last().unwrap().host, "local");
    assert_eq!(history.last().unwrap().session, "session-w1");
    // The roster and the log agree on the migrated host.
    let roster = store.team("t").unwrap().unwrap();
    assert_eq!(roster.members[0].target.host, "local");
}

#[test]
fn a_removed_member_is_not_in_the_roster_but_is_in_the_log() {
    let (mut store, _dir) = open();
    store.save_team(&team("t", &["w1", "w2"])).unwrap();
    store.remove_member("t", "w1").unwrap();
    let team = store.team("t").unwrap().unwrap();
    assert!(!team.members.iter().any(|m| m.member_id == "w1"));
    let history = store.identity_history("t", None).unwrap();
    let removed: Vec<&IdentityEvent> = history
        .iter()
        .filter(|event| event.op == "removed")
        .collect();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].member_id, "w1");
}

#[test]
fn a_failed_removal_records_nothing() {
    let (mut store, _dir) = open();
    store.save_team(&team("t", &["w1"])).unwrap();
    assert!(store.remove_member("t", "gone").is_err());
    assert!(
        !store
            .identity_history("t", None)
            .unwrap()
            .iter()
            .any(|event| event.op == "removed")
    );
    // Removing the supervisor is refused, and nothing is logged.
    assert!(store.remove_member("t", "lead").is_err());
    assert!(
        !store
            .identity_history("t", None)
            .unwrap()
            .iter()
            .any(|event| event.op == "removed")
    );
}

#[test]
fn member_history_scopes_to_one_member() {
    let (mut store, _dir) = open();
    store.save_team(&team("t", &["w1", "w2"])).unwrap();
    store.remove_member("t", "w1").unwrap();
    let w1 = store.identity_history("t", Some("w1")).unwrap();
    let w2 = store.identity_history("t", Some("w2")).unwrap();
    assert_eq!(w1.len(), 2, "registered by the save, then removed");
    assert_eq!(w1.last().unwrap().op, "removed");
    assert!(w2.iter().all(|event| event.op == "registered"));
}

#[test]
fn unknown_team_has_an_empty_history() {
    let (store, _dir) = open();
    assert!(store.identity_history("nope", None).unwrap().is_empty());
}
