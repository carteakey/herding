use super::*;

/// A bridge running a background shell Claude Code knows as `ba2w31ujt`.
fn with_shell() -> Bridge {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "serve");
    bridge.on_harness(&json!({
        "type": "assistant",
        "message": {"id": "msg_1", "role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_bg", "name": "Bash",
             "input": {"command": "npm run dev", "run_in_background": true}},
        ]},
        "parent_tool_use_id": null,
        "session_id": SESSION,
    }));
    bridge.on_harness(&json!({
        "type": "system", "subtype": "task_started",
        "task_id": "ba2w31ujt", "tool_use_id": "toolu_bg",
    }));
    bridge
}

fn stop(bridge: &mut Bridge, call: &str) -> Vec<Out> {
    bridge.on_session(Inbound::Notification {
        method: STOP_METHOD.to_owned(),
        params: json!({"callId": call}),
    })
}

#[test]
fn a_task_claude_named_can_be_stopped() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "serve");
    bridge.on_harness(&json!({
        "type": "assistant",
        "message": {"id": "msg_1", "role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_a", "name": "Agent",
             "input": {"description": "Explore", "prompt": "Look"}},
        ]},
        "parent_tool_use_id": null,
        "session_id": SESSION,
    }));
    let outs = bridge.on_harness(&json!({
        "type": "system", "subtype": "task_started",
        "task_id": "t1", "tool_use_id": "toolu_a",
    }));
    let report = &update(&outs[0])["_meta"]["herdr"]["activity"];
    assert_eq!(report["kind"], "subagent");
    assert_eq!(report["stoppable"], true);
}

#[test]
fn stopping_asks_claude_by_task_id_and_leaves_the_end_to_its_notification() {
    let mut bridge = with_shell();
    let outs = stop(&mut bridge, "toolu_bg");
    assert_eq!(outs.len(), 1, "no update claims it ended yet");
    let request = claude(&outs[0]);
    assert_eq!(request["type"], "control_request");
    assert_eq!(request["request"]["subtype"], "stop_task");
    assert_eq!(request["request"]["task_id"], "ba2w31ujt");
}

#[test]
fn a_call_with_no_task_id_sends_nothing() {
    let mut bridge = with_shell();
    assert!(stop(&mut bridge, "toolu_other").is_empty());
}

#[test]
fn a_refused_stop_says_so() {
    let mut bridge = with_shell();
    let outs = stop(&mut bridge, "toolu_bg");
    let id = claude(&outs[0])["request_id"].clone();
    let outs = bridge.on_harness(&json!({
        "type": "control_response",
        "response": {"subtype": "error", "request_id": id, "error": "no such task"},
    }));
    let notice = update(&outs[0]);
    assert_eq!(notice["severity"], "warning");
    assert!(notice["title"].as_str().unwrap().contains("no such task"));
}
