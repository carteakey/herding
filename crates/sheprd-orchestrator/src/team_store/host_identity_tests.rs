//! Stable host identity: a member's recorded host is their endpoint's id,
//! and identity matching migrates rows recorded under a retired boot.

#![allow(clippy::unwrap_used)]

use super::*;

/// The host context each scenario passes: the caller's endpoint's current
/// boot, the connected hosts, and every known endpoint's stable id.
fn hosts(caller_boot: &str, live: &[&str], known: &[&str]) -> HostContext {
    HostContext {
        caller_boot: caller_boot.to_owned(),
        live_hosts: live.iter().map(|s| (*s).to_owned()).collect(),
        endpoint_ids: known.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn member(id: &str, host: &str, pane: &str, name: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: name.into(),
        target: SessionTarget {
            host: host.into(),
            session: format!("agent:zcode:w:{pane}"),
            workspace: Some("w".into()),
            pane: Some(pane.into()),
        },
    }
}

fn team_with(members: Vec<Member>) -> Team {
    Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor: member("sup", "sup-host", "p-sup", "sup"),
        members,
    }
}

fn store_with(members: Vec<Member>) -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    store.save_team(&team_with(members)).unwrap();
    (dir, store)
}

#[test]
fn a_caller_on_the_recorded_endpoint_matches_exactly() {
    let (_dir, mut store) = store_with(vec![member("w1", "m1", "p1", "w1")]);
    let found = store
        .find_member_team(
            Some("t"),
            &member("w1", "m1", "p1", "w1").target,
            &hosts("", &["m1"], &["m1"]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(found.1.member_id, "w1");
}

#[test]
fn a_row_recorded_under_the_callers_current_boot_migrates_to_the_stable_id() {
    // Pre-stable-key rows name the boot they registered under; the same
    // daemon reporting that boot proves the same member.
    let (_dir, mut store) = store_with(vec![member("w1", "boot-1", "p1", "w1")]);
    let caller = member("w1", "m1", "p1", "w1").target;
    let found = store
        .find_member_team(
            Some("t"),
            &caller,
            &hosts("boot-1", &["m1", "boot-1"], &["m1"]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(found.1.member_id, "w1");
    assert_eq!(
        found.1.target.host, "m1",
        "the row migrated to the stable id"
    );
    // The migration is durable: the next call matches exactly.
    let again = store
        .find_member_team(
            Some("t"),
            &caller,
            &hosts("boot-1", &["m1", "boot-1"], &["m1"]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(again.1.target.host, "m1");
}

#[test]
fn a_row_whose_boot_a_restart_retired_migrates_to_the_caller() {
    // The daemon restarted: the recorded boot is gone, and the caller
    // proves the same session, workspace, and pane on the same endpoint.
    let (_dir, mut store) = store_with(vec![member("w1", "boot-1", "p1", "w1")]);
    let caller = member("w1", "m1", "p1", "w1").target;
    let found = store
        .find_member_team(
            Some("t"),
            &caller,
            &hosts("boot-2", &["m1", "boot-2"], &["m1"]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(found.1.member_id, "w1");
    assert_eq!(found.1.target.host, "m1");
}

#[test]
fn a_live_host_key_is_never_adopted_from_another_machine() {
    // Pane ids are per-daemon (#40): a caller on another live endpoint
    // cannot adopt a row that names a live host.
    let (_dir, mut store) = store_with(vec![member("w1", "m1", "p1", "w1")]);
    let impostor = member("w1", "m2", "p1", "w1").target;
    assert!(
        store
            .find_member_team(
                Some("t"),
                &impostor,
                &hosts("boot-2", &["m1", "m2", "boot-2"], &["m1", "m2"]),
            )
            .unwrap()
            .is_none(),
        "a live host key is not migrated"
    );
}

#[test]
fn a_disconnected_machines_stable_id_row_is_not_adopted_cross_host() {
    // m2 is a SAVED endpoint that is merely disconnected: its stable id is
    // known, so its member's row is never adopted by the same pane id from
    // another machine, however dead m2 looks right now (#40).
    let (_dir, mut store) = store_with(vec![member("w1", "m2", "p1", "w1")]);
    let impostor = member("w1", "m1", "p1", "w1").target;
    assert!(
        store
            .find_member_team(
                Some("t"),
                &impostor,
                &hosts("boot-1", &["m1", "boot-1"], &["m1", "m2"]),
            )
            .unwrap()
            .is_none(),
        "a known endpoint's stable id is never adopted"
    );
    // The row is untouched for its real member's return.
    let row = store
        .find_member_team(
            Some("t"),
            &member("w1", "m2", "p1", "w1").target,
            &hosts("", &["m1", "m2", "boot-1"], &["m1", "m2"]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(row.1.member_id, "w1");
}

#[test]
fn a_legacy_boot_row_from_the_callers_own_previous_boot_is_still_adopted() {
    // boot-old is not any endpoint's stable id and is no longer live: the
    // caller proving the same session, workspace, and pane is that member
    // back from a restart, so the row migrates.
    let (_dir, mut store) = store_with(vec![member("w1", "boot-old", "p1", "w1")]);
    let caller = member("w1", "m1", "p1", "w1").target;
    let found = store
        .find_member_team(
            Some("t"),
            &caller,
            &hosts("boot-old", &["m1", "boot-old"], &["m1"]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(found.1.member_id, "w1");
    assert_eq!(found.1.target.host, "m1");
}

#[test]
fn a_caller_whose_pane_or_session_differs_never_migrates_a_row() {
    let (_dir, mut store) = store_with(vec![member("w1", "boot-1", "p1", "w1")]);
    let other_pane = member("w1", "m1", "p2", "w1").target;
    assert!(
        store
            .find_member_team(
                Some("t"),
                &other_pane,
                &hosts("boot-2", &["m1", "boot-2"], &["m1"]),
            )
            .unwrap()
            .is_none()
    );
    let other_session = Member {
        target: SessionTarget {
            host: "m1".into(),
            session: "agent:codex:w:p1".into(),
            workspace: Some("w".into()),
            pane: Some("p1".into()),
        },
        ..member("w1", "m1", "p1", "w1")
    };
    assert!(
        store
            .find_member_team(
                Some("t"),
                &other_session.target,
                &hosts("boot-2", &["m1", "boot-2"], &["m1"]),
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_exact_match_in_a_later_team_wins_and_no_stale_row_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("t.sqlite3")).unwrap();
    // Two teams match the same session, workspace, and pane: team "a" holds
    // a stale row, team "b" an exact one. Team ids sort "a" before "b", so
    // the stale row is scanned first and must stay unwritten.
    store
        .save_team(&team_with(vec![member(
            "w-stale", "boot-old", "p1", "stale",
        )]))
        .unwrap();
    let mut renamed = store.team("t").unwrap().unwrap();
    renamed.team_id = "a".into();
    store.save_team(&renamed).unwrap();
    let exact_team = Team {
        team_id: "b".into(),
        name: "B".into(),
        supervisor: member("sup-b", "m1", "p-sup", "sup"),
        members: vec![member("w1", "m1", "p1", "w1")],
    };
    store.save_team(&exact_team).unwrap();

    let caller = member("w1", "m1", "p1", "w1").target;
    let found = store
        .find_member_team(
            None,
            &caller,
            &hosts("boot-old", &["m1", "boot-old"], &["m1"]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(found.0.team_id, "b", "the exact match decides");
    let stale_team = store.team("a").unwrap().unwrap();
    let stale_row = stale_team
        .members
        .iter()
        .find(|m| m.member_id == "w-stale")
        .unwrap();
    assert_eq!(
        stale_row.target.host, "boot-old",
        "no write for a row this call did not return"
    );
}

#[test]
fn a_name_holder_on_a_live_endpoint_keeps_the_name_against_another_machine() {
    let (_dir, mut store) = store_with(vec![member("w1", "m1", "p1", "w1")]);
    // Same name, workspace, and pane claimed from endpoint m2 while m1 is
    // still live: refused, the claim would be identity theft across hosts.
    let claim = member("new", "m2", "p1", "w1").target;
    assert!(matches!(
        store.register_member(
            "t",
            "new-id",
            "w1",
            &claim,
            &hosts("boot-2", &["m1", "m2", "boot-2"], &["m1", "m2"]),
        ),
        Err(Error::DuplicateDisplayName(_))
    ));
}

#[test]
fn a_disconnected_machines_name_holder_is_kept_against_a_new_machine() {
    let (_dir, mut store) = store_with(vec![member("w1", "m2", "p1", "w1")]);
    // Same situation as above, but m2 is disconnected: its stable id is
    // still known, so the holder keeps the name and the claim is refused.
    let claim = member("new", "m1", "p1", "w1").target;
    assert!(matches!(
        store.register_member(
            "t",
            "new-id",
            "w1",
            &claim,
            &hosts("boot-1", &["m1", "boot-1"], &["m1", "m2"]),
        ),
        Err(Error::DuplicateDisplayName(_))
    ));
}

#[test]
fn a_name_holder_recorded_under_the_callers_boot_rebinds() {
    let (_dir, mut store) = store_with(vec![member("w1", "boot-1", "p1", "w1")]);
    // The pre-stable-key row names the boot the caller's endpoint still
    // reports: the same pane reclaimed its name, migrating to the stable id.
    let claim = member("new-id", "m1", "p1", "w1").target;
    let rebound = store
        .register_member(
            "t",
            "new-id",
            "w1",
            &claim,
            &hosts("boot-1", &["m1", "boot-1"], &["m1"]),
        )
        .unwrap();
    assert_eq!(rebound.member_id, "w1", "the member id is preserved");
    assert_eq!(rebound.target.host, "m1");
}
