//! Panel actions must not tell an agent about a row before the store has it.
//! The bridge holds an action's prompts against the operation that action
//! queued, releases them when the worker confirms it, and withdraws them when
//! the write is refused - so the first CLI call a woken agent makes finds the
//! post or task it was told about.

#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::server::team_worker::Error as WorkerError;
use std::sync::Arc;

fn prompt(pane: &str, text: &str) -> PromptDispatch {
    PromptDispatch {
        host: "boot".into(),
        pane_id: pane.to_owned(),
        prompt: format!("[Herdr Team] {text}"),
        summary: text.to_owned(),
    }
}

/// A bridge whose action has queued a write that the worker has not answered.
fn queued(request: u64) -> Bridge {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        action_request: Some(request),
        ..Bridge::default()
    };
    bridge.pending.insert(
        request,
        Pending::Write {
            team_id: "team".into(),
        },
    );
    bridge
}

/// The whole point of the ordering: a woken agent's `task claim` or
/// `bulletin ack` runs against the store, so the row must be there first.
#[test]
fn a_prompt_waits_for_the_write_that_produced_it() {
    let mut bridge = queued(7);

    let immediate =
        bridge.hold_prompts_for_action(vec![prompt("p1", "Task t-1 is yours; claim it")]);
    assert!(immediate.is_empty(), "the prompt waits for the store");
    assert_eq!(bridge.held_prompt_count(), 1);
    assert!(
        bridge.take_released_prompts().is_empty(),
        "nothing dispatches before the confirmation"
    );

    bridge.complete(Completion {
        request_id: 7,
        result: Ok(Reply::Saved),
    });

    let released = bridge.take_released_prompts();
    assert_eq!(released.len(), 1);
    assert_eq!(released[0].pane_id, "p1");
    assert!(released[0].prompt.contains("Task t-1 is yours"));
    assert_eq!(bridge.held_prompt_count(), 0);
    assert!(
        bridge.take_released_prompts().is_empty(),
        "a released prompt dispatches once"
    );
    assert!(bridge.failure.is_none());
}

#[test]
fn a_failed_write_withdraws_the_prompts_waiting_on_it() {
    let mut bridge = queued(7);
    bridge.hold_prompts_for_action(vec![prompt("p1", "Post p-1 is on the board")]);

    bridge.complete(Completion {
        request_id: 7,
        result: Err(Arc::new(WorkerError::Busy)),
    });

    assert_eq!(
        bridge.held_prompt_count(),
        0,
        "nothing waits for a write that will not answer"
    );
    assert!(
        bridge.take_released_prompts().is_empty(),
        "nothing is announced for a row that never landed"
    );
    assert!(bridge.failure.is_some());
}

/// A store-side refusal (the caller's session is not a member) reports a
/// warning rather than a connection failure, and withdraws prompts the same way.
#[test]
fn a_refused_write_withdraws_the_prompts_too() {
    let mut bridge = queued(7);
    bridge.hold_prompts_for_action(vec![prompt("p1", "Post p-1 is on the board")]);

    bridge.complete(Completion {
        request_id: 7,
        result: Err(Arc::new(WorkerError::External(
            "The proven caller session is not a member of this team".into(),
        ))),
    });

    assert_eq!(bridge.held_prompt_count(), 0);
    assert!(bridge.take_released_prompts().is_empty());
    assert!(bridge.warning.is_some());
}

/// A nudge never reaches the store, so waiting for a confirmation would mean
/// waiting forever: its prompt dispatches in the tick it was asked for.
#[test]
fn an_action_that_queued_nothing_dispatches_its_prompts_now() {
    let mut bridge = Bridge::default();

    let immediate = bridge.hold_prompts_for_action(vec![prompt("p1", "Status check for worker")]);

    assert_eq!(immediate.len(), 1);
    assert_eq!(bridge.held_prompt_count(), 0);
}

/// With no worker to queue the write at all - storage unavailable - an action's
/// prompts are withdrawn rather than announcing work that will never persist.
#[test]
fn a_refused_action_announces_nothing() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        ..Bridge::default()
    };

    bridge.act(
        TeamAction::SetGoal {
            team_id: "team".into(),
            goal: "ship it".into(),
        },
        &[],
    );
    assert!(bridge.action_refused, "the goal never reached a worker");
    let immediate = bridge.hold_prompts_for_action(vec![prompt("p1", "Team goal set")]);

    assert!(immediate.is_empty());
    assert_eq!(bridge.held_prompt_count(), 0);
}

/// A reply the bridge ignores (an older snapshot superseded by a newer request)
/// must not leave prompts parked: nothing else will ever confirm them.
#[test]
fn a_reply_the_bridge_ignores_withdraws_the_prompts_waiting_on_it() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        snapshot_in_flight: Some(11),
        action_request: Some(7),
        ..Bridge::default()
    };
    bridge.pending.insert(
        7,
        Pending::Snapshot {
            team_id: "team".into(),
        },
    );
    bridge.hold_prompts_for_action(vec![prompt("p1", "Team goal set")]);

    bridge.complete(Completion {
        request_id: 7,
        result: Ok(Reply::Snapshot {
            team: None,
            goal: None,
            posts: Vec::new(),
            tasks: Vec::new(),
            findings: Vec::new(),
            task_older: None,
            finding_older: None,
        }),
    });

    assert_eq!(bridge.held_prompt_count(), 0);
    assert!(bridge.take_released_prompts().is_empty());
}

/// A worker that never answers must not turn the bridge into a prompt backlog.
#[test]
fn held_prompts_stay_bounded_when_writes_never_answer() {
    let mut bridge = Bridge::default();
    let rounds = PROMPT_HOLD_CAPACITY * 2;
    for request in 0..rounds {
        bridge.action_request = Some(request as u64);
        bridge.hold_prompts_for_action(vec![prompt("p1", &format!("post {request}")); 2]);
    }

    assert!(
        bridge.held_prompt_count() <= PROMPT_HOLD_CAPACITY,
        "held prompts are bounded: {}",
        bridge.held_prompt_count()
    );

    let newest = rounds - 1;
    bridge.pending.insert(
        newest as u64,
        Pending::Write {
            team_id: "team".into(),
        },
    );
    bridge.complete(Completion {
        request_id: newest as u64,
        result: Ok(Reply::Saved),
    });

    let released = bridge.take_released_prompts();
    assert_eq!(released.len(), 2, "the newest prompts survived");
    assert!(released[0].prompt.contains(&format!("post {newest}")));
}
