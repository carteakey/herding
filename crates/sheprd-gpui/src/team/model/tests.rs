use super::*;

#[test]
fn pane_actions_require_a_known_session() {
    let mut identity = PaneIdentity {
        daemon_id: "local".into(),
        session_id: None,
        workspace_id: "workspace-1".into(),
        pane_id: "pane-1".into(),
    };
    assert!(!identity.verified());
    identity.session_id = Some("session-1".into());
    assert!(identity.verified());
    identity.session_id = Some(String::new());
    assert!(!identity.verified());
}
