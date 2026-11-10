//! A caller in a herdr-core terminal, as the CLI describes it to the app.

use super::*;

fn vars<'a>(set: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
    |name| {
        set.iter()
            .find(|(known, _)| *known == name)
            .map(|(_, value)| (*value).to_owned())
    }
}

#[test]
fn a_core_terminal_names_itself_and_drops_an_inherited_herdr_pane() {
    let set = [
        ("HC_TERMINAL_ID", "t7"),
        ("HERDR_PANE_ID", "w1:p2"),
        ("HERDR_WORKSPACE_ID", "w1"),
    ];
    let caller = caller_from(None, vars(&set), None);
    assert_eq!(caller.terminal_id.as_deref(), Some("t7"));
    assert_eq!(caller.pane_id, None);
    // A terminal the app opened in a Herdr workspace names that workspace.
    assert_eq!(caller.workspace_id.as_deref(), Some("w1"));
    assert_eq!(
        crate::control::origin(caller.pane_id.as_deref(), caller.terminal_id.as_deref()).as_deref(),
        Some("core:t7")
    );
}

#[test]
fn a_herdr_pane_is_described_as_before() {
    let set = [("HERDR_PANE_ID", "w1:p2"), ("HERDR_WORKSPACE_ID", "w1")];
    let caller = caller_from(Some("w9".into()), vars(&set), Some("/s.sock".into()));
    assert_eq!(caller.pane_id.as_deref(), Some("w1:p2"));
    assert_eq!(caller.workspace_id.as_deref(), Some("w9"));
    assert_eq!(caller.terminal_id, None);
    // An older app reads the request: the absent field is not sent.
    let json = serde_json::to_value(&caller).unwrap();
    assert!(json.get("terminal_id").is_none());
}

#[test]
fn an_invalid_terminal_is_no_origin_and_no_valid_caller() {
    assert_eq!(crate::control::origin(None, Some("a\nb")), None);
    assert_eq!(crate::control::origin(None, None), None);
    let caller = Caller {
        terminal_id: Some("x\u{7}".into()),
        ..Caller::default()
    };
    assert!(!crate::control::valid_caller_for_test(&caller));
}
