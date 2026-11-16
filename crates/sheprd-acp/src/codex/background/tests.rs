#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::*;

#[test]
fn followed_sub_agents_and_commands_are_bounded() {
    let mut background = Background::default();
    for index in 0..MAX_TRACKED + 2 {
        background.started(
            &json!({"type": "collabAgentToolCall", "id": format!("s{index}"),
            "tool": "spawnAgent"}),
        );
        background.started(
            &json!({"type": "commandExecution", "id": format!("c{index}"),
            "command": "sleep 1"}),
        );
    }
    assert_eq!(background.agents.len(), MAX_TRACKED);
    assert_eq!(background.commands.len(), MAX_TRACKED);
    let tag = background
        .started(&json!({"type": "collabAgentToolCall", "id": "late", "tool": "spawnAgent"}));
    assert!(tag.activity.is_none(), "past the cap it stays a card");
}

#[test]
fn a_sub_agent_can_be_stopped_once_its_turn_is_known_until_it_ends() {
    let mut background = Background::default();
    background.started(&json!({"type": "collabAgentToolCall", "id": "s", "tool": "spawnAgent"}));
    background.completed(
        &json!({"type": "collabAgentToolCall", "id": "s", "tool": "spawnAgent",
        "receiverThreadIds": ["child"]}),
    );
    assert_eq!(background.child_of("s"), None, "no turn to interrupt yet");

    let (call, report) = background.child_turn("child", "turn-1").unwrap();
    assert_eq!(call, "s");
    assert!(report.stoppable);
    assert_eq!(background.child_of("s"), Some(("child", "turn-1")));
    assert!(
        background.child_turn("child", "turn-1").is_none(),
        "said once"
    );

    background.child_turn_completed("child", Some("interrupted"));
    assert_eq!(background.child_of("s"), None);
    assert!(background.child_turn("child", "turn-2").is_none());
}

#[test]
fn a_thread_that_is_no_sub_agent_cannot_be_stopped() {
    let mut background = Background::default();
    assert!(background.child_turn("stranger", "turn-1").is_none());
    assert!(background.turns.is_empty());
}

#[test]
fn only_a_spawn_starts_a_sub_agent_and_running_states_end_nothing() {
    let mut background = Background::default();
    let wait =
        background.started(&json!({"type": "collabAgentToolCall", "id": "w", "tool": "wait"}));
    assert!(wait.activity.is_none());
    background.started(&json!({"type": "collabAgentToolCall", "id": "s", "tool": "spawnAgent"}));
    let (own, others) = background.completed(&json!({"type": "collabAgentToolCall", "id": "s",
        "tool": "spawnAgent", "status": "completed", "receiverThreadIds": ["t"],
        "agentsStates": {"t": {"status": "pendingInit"}, "u": {"status": "completed"}}}));
    assert!(own.is_none());
    assert!(others.is_empty(), "an unknown thread is not this session's");
    assert_eq!(background.parent("t"), Some("s"));
    // A malformed item is ignored.
    assert_eq!(background.completed(&json!({"type": 3})).1.len(), 0);
    assert!(background.started(&json!({"id": ["x"]})).activity.is_none());
}

#[test]
fn a_spawn_whose_child_already_finished_ends_with_it() {
    let mut background = Background::default();
    background.started(&json!({"type": "collabAgentToolCall", "id": "s", "tool": "spawnAgent"}));
    let (own, others) = background.completed(&json!({"type": "collabAgentToolCall", "id": "s",
        "tool": "spawnAgent", "status": "completed", "receiverThreadIds": ["t"],
        "agentsStates": {"t": {"status": "shutdown", "message": null}}}));
    assert!(others.is_empty());
    assert_eq!(own.unwrap().state, ActivityState::Killed);
    assert!(background.turn_ended().is_empty());
}
