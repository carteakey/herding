#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::*;

#[test]
fn a_task_notification_envelope_reads_only_its_display_fields() {
    let text = "<task-notification>\n<task-id>ba2w31ujt</task-id>\n\
        <tool-use-id>toolu_1</tool-use-id>\n<output-file>/tmp/x.output</output-file>\n\
        <status>killed</status>\n<summary>Background command stopped</summary>\n\
        </task-notification>";
    assert_eq!(
        task_notification(text),
        Some((
            "ba2w31ujt",
            Some("toolu_1"),
            "killed",
            Some("Background command stopped")
        ))
    );
    for malformed in [
        "<task-notification><status>completed</status></task-notification>",
        "<task-notification><task-id>a</task-id><status>done",
        "please read <task-notification><task-id>a</task-id><status>x</status></task-notification>",
        "<task-notification><task-id><b>a</b></task-id><status>completed</status></task-notification>",
    ] {
        assert_eq!(task_notification(malformed), None, "{malformed}");
    }
}

#[test]
fn a_background_bash_runs_until_its_notification() {
    let mut background = Background::default();
    let tag = background.started(
        "toolu_1",
        "Bash",
        &json!({"command": "npm run dev\n--port 1", "run_in_background": true}),
        None,
    );
    let report = tag.activity.unwrap();
    assert_eq!(report.kind, ActivityKind::Shell);
    assert_eq!(report.label.as_deref(), Some("npm run dev\n--port 1"));
    let (own, other) = background.result(
        "toolu_1",
        "Bash",
        false,
        &json!("Command running in background with ID: ba2w31ujt. Output is being written to: /tmp/t.output"),
        None,
    );
    assert_eq!(
        own,
        Some(Report::stoppable(ActivityKind::Shell)),
        "it already runs"
    );
    assert!(other.is_none());
    assert!(background.turn_ended().is_empty(), "it outlives the turn");
    let (call, report) = background
        .notified("ba2w31ujt", None, "completed", Some("done"))
        .unwrap();
    assert_eq!(call, "toolu_1");
    assert_eq!(report.state, ActivityState::Finished);
    assert_eq!(report.detail.as_deref(), Some("done"));
    assert!(
        background
            .notified("ba2w31ujt", None, "completed", None)
            .is_none(),
        "a second notification says nothing new"
    );
}

#[test]
fn a_command_moved_to_the_background_starts_a_shell() {
    let mut background = Background::default();
    let tag = background.started("toolu_1", "Bash", &json!({"command": "cargo build"}), None);
    assert!(tag.activity.is_none());
    let (own, _) = background.result(
        "toolu_1",
        "Bash",
        false,
        &json!("…"),
        Some(&json!({"backgroundTaskId": "b1", "timedOutAfterMs": 120000})),
    );
    assert_eq!(own.unwrap().state, ActivityState::Running);
    let (call, report) = background.notified("b1", None, "failed", None).unwrap();
    assert_eq!(call, "toolu_1");
    assert_eq!(report.state, ActivityState::Failed);
}

#[test]
fn task_stop_kills_the_shell_it_names() {
    let mut background = Background::default();
    background.started(
        "toolu_1",
        "Bash",
        &json!({"command": "sleep 100", "run_in_background": true}),
        None,
    );
    background.result(
        "toolu_1",
        "Bash",
        false,
        &json!(""),
        Some(&json!({"backgroundTaskId": "b1"})),
    );
    background.started("toolu_2", "TaskStop", &json!({"task_id": "b1"}), None);
    let (own, other) = background.result("toolu_2", "TaskStop", false, &json!("stopped"), None);
    assert!(own.is_none());
    let (call, report) = other.unwrap();
    assert_eq!(call, "toolu_1");
    assert_eq!(report.state, ActivityState::Killed);
    assert_eq!(report.kind, ActivityKind::Shell);
}

#[test]
fn a_poll_that_reports_an_exit_ends_the_shell() {
    let mut background = Background::default();
    background.started(
        "toolu_1",
        "Bash",
        &json!({"command": "make", "run_in_background": true}),
        None,
    );
    background.result(
        "toolu_1",
        "Bash",
        false,
        &json!("Command running in background with ID: b1."),
        None,
    );
    background.started("toolu_2", "TaskOutput", &json!({"task_id": "b1"}), None);
    let (_, other) = background.result(
        "toolu_2",
        "TaskOutput",
        false,
        &json!("<status>running</status>"),
        None,
    );
    assert!(other.is_none(), "still running");
    background.started("toolu_3", "BashOutput", &json!({"bash_id": "b1"}), None);
    let (_, other) = background.result(
        "toolu_3",
        "BashOutput",
        false,
        &json!([{"type": "text", "text": "<status>completed</status><exit_code>2</exit_code>"}]),
        None,
    );
    let (call, report) = other.unwrap();
    assert_eq!(call, "toolu_1");
    assert_eq!(report.state, ActivityState::Finished);
    assert_eq!(report.detail.as_deref(), Some("Exited 2"));
}

#[test]
fn a_foreground_subagent_ends_with_its_result_or_its_turn() {
    let mut background = Background::default();
    let tag = background.started("toolu_a", "Agent", &json!({"description": "Explore"}), None);
    assert_eq!(tag.activity.unwrap().kind, ActivityKind::Subagent);
    let (own, _) = background.result("toolu_a", "Agent", true, &json!("boom"), None);
    assert_eq!(own.unwrap().state, ActivityState::Failed);

    background.started("toolu_b", "Task", &json!({"description": "Review"}), None);
    let stopped = background.turn_ended();
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0].0, "toolu_b");
    assert_eq!(stopped[0].1.state, ActivityState::Killed);
}

#[test]
fn a_background_subagent_ends_with_its_agent_ids_notification() {
    let mut background = Background::default();
    background.started(
        "toolu_a",
        "Agent",
        &json!({"description": "Explore", "run_in_background": true}),
        None,
    );
    let (own, _) = background.result(
        "toolu_a",
        "Agent",
        false,
        &json!([{"type": "text", "text": "Async agent launched successfully.\nagentId: a0a3df77f66a8e3f4 (internal ID)"}]),
        None,
    );
    assert_eq!(
        own,
        Some(Report::stoppable(ActivityKind::Subagent)),
        "it runs on"
    );
    assert!(background.turn_ended().is_empty());
    let (call, report) = background
        .notified(
            "a0a3df77f66a8e3f4",
            None,
            "completed",
            Some("Agent finished"),
        )
        .unwrap();
    assert_eq!(call, "toolu_a");
    assert_eq!(report.kind, ActivityKind::Subagent);
}

#[test]
fn followed_work_is_bounded() {
    let mut background = Background::default();
    for index in 0..MAX_TRACKED + 3 {
        background.started(
            &format!("toolu_{index}"),
            "Agent",
            &json!({"description": "x"}),
            None,
        );
    }
    assert_eq!(background.running.len(), MAX_TRACKED);
    let tag = background.started("toolu_more", "Agent", &json!({}), Some("toolu_0"));
    assert!(tag.activity.is_none(), "past the cap it stays a card");
    assert_eq!(tag.parent.as_deref(), Some("toolu_0"));
}

#[test]
fn a_monitor_runs_past_its_result_until_its_notification() {
    let mut background = Background::default();
    let tag = background.started(
        "toolu_m",
        "Monitor",
        &json!({"description": "errors in the deploy log", "command": "tail -f deploy.log"}),
        None,
    );
    let report = tag.activity.unwrap();
    assert_eq!(report.kind, ActivityKind::Monitor);
    assert_eq!(report.label.as_deref(), Some("errors in the deploy log"));
    let (own, _) = background.result(
        "toolu_m",
        "Monitor",
        false,
        &json!(
            "Monitor started (task b7kq2, timeout 300000ms). You will be notified on each event."
        ),
        None,
    );
    assert_eq!(own, Some(Report::stoppable(ActivityKind::Monitor)));
    assert_eq!(background.task_of("toolu_m"), Some("b7kq2"));
    assert!(background.turn_ended().is_empty(), "it outlives its turn");
    let (call, report) = background
        .notified("b7kq2", None, "completed", None)
        .unwrap();
    assert_eq!(call, "toolu_m");
    assert_eq!(report.state, ActivityState::Finished);
    assert_eq!(background.task_of("toolu_m"), None);
}

#[test]
fn a_monitor_that_never_started_ends_with_its_result() {
    let mut background = Background::default();
    background.started("toolu_m", "Monitor", &json!({"description": "x"}), None);
    let (own, _) = background.result("toolu_m", "Monitor", true, &json!("refused"), None);
    assert_eq!(own.unwrap().state, ActivityState::Failed);
}
