//! The poll's script and its answer parse, against the shape `herdr pane
//! list` answers with (verified against the daemon's own output; a pane
//! without a revision is skipped, unknown fields ignored).

use super::*;

#[test]
fn the_script_prefers_named_candidates_then_the_path() {
    let script = pane_list_script(None);
    assert!(script.contains("exec \"$c\" pane list"), "{script}");
    assert!(script.contains("exec herdr pane list"), "{script}");
    assert!(
        script.contains("/home/linuxbrew/.linuxbrew/bin/herdr"),
        "{script}"
    );
    assert!(script.contains("$HOME/.cargo/bin/herdr"), "{script}");
    let session = pane_list_script(Some("agent:zcode:ws:w1"));
    assert!(
        session.contains("exec \"$c\" --session 'agent:zcode:ws:w1' pane list"),
        "a non-default session is named: {session}"
    );
    assert!(!pane_list_script(Some("default")).contains("--session"));
}

#[test]
fn the_answer_parses_panes_and_skips_missing_revisions() {
    let answer: PaneListAnswer = serde_json::from_str(
        r#"{
            "id": "cli:pane:list",
            "result": {
                "panes": [
                    {"pane_id": "w1:p1", "revision": 107108, "cwd": "/repo",
                     "scroll": {"offset_from_bottom": 0}, "terminal_id": "term_1"},
                    {"pane_id": "w1:p2", "cwd": "/repo"},
                    {"pane_id": "w1:p3", "revision": 4}
                ]
            },
            "type": "pane_list"
        }"#,
    )
    .expect("the daemon's answer shape parses");
    let panes: Vec<(String, u64)> = answer
        .result
        .panes
        .into_iter()
        .filter_map(|pane| pane.revision.map(|revision| (pane.pane_id, revision)))
        .collect();
    assert_eq!(
        panes,
        vec![("w1:p1".to_owned(), 107108), ("w1:p3".to_owned(), 4)],
        "a pane without a revision stays unobservable, the rest carry theirs"
    );
}

#[test]
fn an_unparseable_answer_is_a_failure_not_a_panic() {
    let parsed: Result<PaneListAnswer, _> = serde_json::from_str("not json");
    assert!(parsed.is_err(), "garbage is a poll failure");
}
