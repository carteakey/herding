use sheprd_chat_protocol::{ActivityKind, ActivityState};

use super::*;

#[test]
fn what_still_runs_when_the_agent_exits_is_marked_stopped() {
    let mut harness = ready();
    harness.update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "bash", "title": "`npm run dev`",
        "kind": "execute", "status": "completed",
        "_meta": {"herdr": {"activity": {"kind": "shell", "state": "running"}}},
    }));
    let activity = harness.transcript().activity("bash").unwrap();
    assert_eq!(activity.kind, ActivityKind::Shell);
    assert!(activity.is_running());
    let receiver = harness.subscribe(1, false, None);
    harness.input(Input::AgentExited {
        code: Some(0),
        stderr: Vec::new(),
    });
    let activity = harness.transcript().activity("bash").unwrap();
    assert_eq!(activity.state, ActivityState::Killed);
    assert!(activity.finished_ms.is_some());
    assert!(
        drain(&receiver).iter().any(|message| matches!(
            message,
            ServerMessage::Change {
                change: Change::Activity { .. },
                ..
            }
        )),
        "clients hear of it"
    );
}

fn shell(harness: &mut Harness, stoppable: bool) {
    harness.update(json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "bash",
        "title": "npm run dev",
        "_meta": {"herdr": {"activity": {
            "kind": "shell", "state": "running", "stoppable": stoppable,
        }}},
    }));
}

#[test]
fn stopping_reaches_the_bridge_and_changes_nothing_until_it_reports() {
    let mut harness = ready();
    shell(&mut harness, true);
    harness.command(
        0,
        Command::StopActivity {
            call_id: "bash".to_owned(),
        },
    );
    assert_eq!(
        harness.last_request(crate::project::STOP_METHOD)["params"]["callId"],
        "bash"
    );
    assert_eq!(harness.session.handled.back().unwrap().1, None);
    assert!(harness.transcript().activity("bash").unwrap().is_running());
}

#[test]
fn only_running_stoppable_work_can_be_stopped() {
    let mut harness = ready();
    shell(&mut harness, false);
    let stop = Command::StopActivity {
        call_id: "bash".to_owned(),
    };
    harness.command(0, stop.clone());
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::UnknownActivity)
    );
    harness.command(
        0,
        Command::StopActivity {
            call_id: "nothing".to_owned(),
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::UnknownActivity)
    );
    assert!(
        harness
            .sent()
            .iter()
            .all(|message| message["method"] != crate::project::STOP_METHOD)
    );
}

#[test]
fn a_later_report_keeps_work_stoppable_and_an_end_clears_it() {
    let mut harness = ready();
    shell(&mut harness, true);
    harness.update(json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "bash",
        "_meta": {"herdr": {"activity": {
            "kind": "shell", "state": "running", "detail": "Compiling",
        }}},
    }));
    assert!(harness.transcript().activity("bash").unwrap().stoppable);
    harness.update(json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "bash",
        "_meta": {"herdr": {"activity": {"kind": "shell", "state": "finished"}}},
    }));
    assert!(!harness.transcript().activity("bash").unwrap().stoppable);
}
