//! The roster projects what each member reported about its model and account.
#![allow(clippy::unwrap_used)]

use super::*;

fn target(pane: &str) -> SessionTarget {
    SessionTarget {
        host: "local".into(),
        session: format!("agent:zcode:w:{pane}"),
        workspace: Some("w".into()),
        pane: Some(pane.into()),
    }
}

fn member(id: &str, name: &str, pane: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: name.into(),
        model: None,
        account: None,
        target: target(pane),
    }
}

#[test]
fn the_roster_carries_the_reported_model_and_account() {
    let supervisor = member("sup", "supervisor", "p-sup");
    let mut worker = member("w1", "zcode1", "p1");
    worker.model = Some("glm-5.3".into());
    worker.account = Some("zai-coding-plan".into());
    let team = Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor,
        members: vec![worker.clone()],
    };

    let info = projection::member_info(&worker, &team, &Default::default());
    assert_eq!(info.model.as_deref(), Some("glm-5.3"));
    assert_eq!(info.account.as_deref(), Some("zai-coding-plan"));
}

#[test]
fn a_member_that_reported_neither_projects_neither() {
    let supervisor = member("sup", "supervisor", "p-sup");
    let worker = member("w1", "zcode1", "p1");
    let team = Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor,
        members: vec![worker.clone()],
    };

    let info = projection::member_info(&worker, &team, &Default::default());
    assert_eq!(info.model, None);
    assert_eq!(info.account, None);
}
