use sheprd_chat_protocol::{ActivityKind, ActivityState};

use super::*;

fn apply(projector: &mut Projector, transcript: &mut Transcript, value: Value) {
    for change in projector.update(transcript, update(value), 1_000) {
        transcript.apply(&change).unwrap();
    }
}

fn tool<'a>(transcript: &'a Transcript, call: &str) -> &'a Tool {
    transcript
        .entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Tool(tool) if tool.call_id == call => Some(tool),
            _ => None,
        })
        .unwrap()
}

/// Copilot's sub-agent call, as a real session sent it.
fn copilot(id: &str) -> Value {
    json!({
        "sessionUpdate": "tool_call", "toolCallId": id, "title": "Reply with OK",
        "kind": "other", "status": "pending",
        "rawInput": {
            "agent_type": "general-purpose", "name": "simple-ok-response",
            "description": "Reply with OK", "prompt": "Reply with OK.",
        },
    })
}

fn child(id: &str, title: &str) -> Value {
    json!({
        "sessionUpdate": "tool_call", "toolCallId": format!("ses_c:{id}"), "title": title,
        "kind": "read", "status": "in_progress",
        "_meta": {"opencode/child-session": {"id": "ses_c", "parentID": "ses_p", "depth": 1}},
    })
}

#[test]
fn copilots_sub_agent_runs_until_its_call_completes() {
    let (mut projector, mut transcript) = (Projector::default(), Transcript::default());
    apply(&mut projector, &mut transcript, copilot("t1"));
    let activity = transcript.activity("t1").unwrap();
    assert_eq!(activity.kind, ActivityKind::Subagent);
    assert_eq!(activity.label, "Reply with OK");
    assert!(activity.is_running());
    assert!(!activity.stoppable, "ACP has no request to stop one");
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed"}),
    );
    assert_eq!(
        transcript.activity("t1").unwrap().state,
        ActivityState::Finished
    );
}

#[test]
fn opencodes_input_can_arrive_after_its_call() {
    let (mut projector, mut transcript) = (Projector::default(), Transcript::default());
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "task",
               "kind": "think", "status": "pending", "rawInput": {}}),
    );
    assert!(transcript.activities.is_empty());
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "in_progress",
               "title": "Look around",
               "rawInput": {"subagent_type": "general", "prompt": "Look"}}),
    );
    assert!(transcript.activity("t1").unwrap().is_running());
}

#[test]
fn a_child_sessions_step_goes_under_the_one_sub_agent_running() {
    let (mut projector, mut transcript) = (Projector::default(), Transcript::default());
    apply(&mut projector, &mut transcript, copilot("t1"));
    apply(&mut projector, &mut transcript, child("r1", "Read a.rs"));
    assert_eq!(tool(&transcript, "ses_c:r1").parent.as_deref(), Some("t1"));
    assert_eq!(
        transcript.activity("t1").unwrap().detail.as_deref(),
        Some("Read a.rs")
    );
}

#[test]
fn with_several_running_a_step_is_not_guessed_under_one() {
    let (mut projector, mut transcript) = (Projector::default(), Transcript::default());
    apply(&mut projector, &mut transcript, copilot("t1"));
    apply(&mut projector, &mut transcript, copilot("t2"));
    apply(&mut projector, &mut transcript, child("r1", "Read a.rs"));
    assert_eq!(tool(&transcript, "ses_c:r1").parent, None);
}

#[test]
fn a_child_sessions_words_stay_out_of_the_conversation() {
    let (mut projector, mut transcript) = (Projector::default(), Transcript::default());
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "agent_message_chunk",
               "content": {"type": "text", "text": "Inside."},
               "_meta": {"opencode/child-session": {"id": "ses_c"}}}),
    );
    assert!(transcript.entries.is_empty());
}

#[test]
fn a_sub_agent_never_reported_ended_ends_with_its_turn() {
    let (mut projector, mut transcript) = (Projector::default(), Transcript::default());
    apply(&mut projector, &mut transcript, copilot("t1"));
    for change in projector.direct_turn_ended(&transcript, 2_000) {
        transcript.apply(&change).unwrap();
    }
    let activity = transcript.activity("t1").unwrap();
    assert_eq!(activity.state, ActivityState::Killed);
    assert_eq!(activity.label, "Reply with OK", "it keeps its label");
    assert!(projector.direct_turn_ended(&transcript, 3_000).is_empty());
}

#[test]
fn a_step_keeps_its_first_place_when_a_sub_agent_ends() {
    let (mut projector, mut transcript) = (Projector::default(), Transcript::default());
    apply(&mut projector, &mut transcript, copilot("t1"));
    apply(&mut projector, &mut transcript, copilot("t2"));
    apply(&mut projector, &mut transcript, child("r1", "Read a.rs"));
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "t2", "status": "completed"}),
    );
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "ses_c:r1",
               "status": "completed", "title": "Read b.rs",
               "_meta": {"opencode/child-session": {"id": "ses_c", "parentID": "ses_p"}}}),
    );
    assert_eq!(tool(&transcript, "ses_c:r1").parent, None);
    assert_eq!(transcript.activity("t1").unwrap().detail, None);
}
