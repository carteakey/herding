#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::*;

#[test]
fn copilots_call_as_it_sends_it_runs_a_sub_agent() {
    assert!(waits_for_one(&json!({
        "agent_type": "general-purpose",
        "name": "simple-ok-response",
        "description": "Reply with OK",
        "prompt": "Reply with the single word OK, using no tools.",
    })));
}

#[test]
fn opencodes_task_runs_a_sub_agent_unless_it_is_sent_away() {
    let input = json!({"subagent_type": "general", "prompt": "Look", "description": "Look"});
    assert!(waits_for_one(&input));
    let away = json!({"subagent_type": "general", "prompt": "Look", "background": true});
    assert!(!waits_for_one(&away));
}

#[test]
fn a_call_that_only_looks_like_one_is_left_alone() {
    assert!(!waits_for_one(&json!({"prompt": "Look"})));
    assert!(!waits_for_one(&json!({"agent": "general"})));
    assert!(!waits_for_one(&json!({"agent": 1, "prompt": "Look"})));
    assert!(!waits_for_one(&json!({})));
}
