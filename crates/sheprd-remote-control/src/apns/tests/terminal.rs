//! A terminal agent's Live Activity follows its settled status.

use super::*;
use crate::apns::decide::{DEFAULT_STEP, LINGER_S};
use crate::push::{Moved, SeenAgent};
use sheprd_client::protocol::AgentStatus::{self, Blocked, Done, Idle, Working};

const NOW_MS: u64 = 1_800_000_000_000;
const FINGERPRINT: &str = "AB:01";

fn agent(task: Option<&'static str>) -> SeenAgent<'static> {
    SeenAgent {
        chat: CHAT,
        status: Working,
        task,
    }
}

fn moved(from: AgentStatus, to: Option<AgentStatus>) -> Moved {
    Moved {
        pane: CHAT.pane.to_owned(),
        from,
        to,
        started_ms: Some(NOW_MS - 30_000),
    }
}

fn decide(from: AgentStatus, to: Option<AgentStatus>) -> Option<Live> {
    let agent = agent(Some("Fix the flaky test"));
    let agent = to.is_some().then_some(&agent);
    decide_terminal(&moved(from, to), agent, "studio", FINGERPRINT, NOW_MS)
}

#[test]
fn a_run_starts_an_activity_named_for_the_tab() {
    let live = decide(Idle, Some(Working)).unwrap();
    let Event::Start { alert, attributes } = &live.event else {
        panic!("{:?}", live.event);
    };
    assert_eq!(alert.title, "Claude is working");
    assert_eq!(attributes.chat_title, "remote plan");
    assert_eq!(attributes.computer_id, "AB01");
    assert_eq!(live.state.phase, Phase::Working);
    assert_eq!(live.state.step, "Fix the flaky test");
    assert_eq!(live.state.started_at, (NOW_MS - 30_000) / 1000);
}

#[test]
fn waiting_alerts_on_the_running_activity_or_starts_one() {
    let live = decide(Working, Some(Blocked)).unwrap();
    assert!(
        matches!(&live.event, Event::Update { alert: Some(alert) } if alert.title == "Claude needs an answer")
    );
    assert_eq!(live.state.phase, Phase::Question);
    assert!(live.urgent());
    let live = decide(Idle, Some(Blocked)).unwrap();
    assert!(matches!(live.event, Event::Start { .. }));
    let live = decide(Blocked, Some(Working)).unwrap();
    assert_eq!(live.event, Event::Update { alert: None });
    assert_eq!(live.state.phase, Phase::Working);
}

#[test]
fn finishing_lingers_and_anything_else_ends_at_once() {
    let now_s = NOW_MS / 1000;
    let live = decide(Working, Some(Done)).unwrap();
    assert_eq!(
        live.event,
        Event::End {
            dismissal_s: now_s + LINGER_S
        }
    );
    assert_eq!(live.state.phase, Phase::Finished);
    assert_eq!(
        live.state.started_at,
        (NOW_MS - 30_000) / 1000,
        "the finished timer keeps the run's start"
    );
    for to in [Some(Idle), None] {
        let live = decide(Blocked, to).unwrap();
        assert_eq!(live.event, Event::End { dismissal_s: now_s });
    }
}

#[test]
fn a_finished_run_stepped_through_keeps_its_elapsed_time() {
    let mut agents = crate::push::TerminalAgents::default();
    let mut moved = Vec::new();
    for (at, status) in [(0, Idle), (1, Working), (2, Working), (3, Working)]
        .into_iter()
        .chain([(60, Done), (61, Done), (62, Done)])
    {
        let seen = SeenAgent {
            chat: CHAT,
            status,
            task: None,
        };
        moved.extend(agents.step([seen], NOW_MS + at * 1000));
    }
    let done = moved.last().unwrap();
    let agent = agent(None);
    let live = decide_terminal(done, Some(&agent), "studio", FINGERPRINT, NOW_MS + 62_000);
    let live = live.unwrap();
    assert_eq!(live.state.phase, Phase::Finished);
    assert_eq!(live.state.started_at, NOW_MS / 1000 + 1);
}

#[test]
fn nothing_runs_nothing_to_end() {
    for (from, to) in [(Idle, Some(Done)), (Done, Some(Idle)), (Idle, None)] {
        assert!(decide(from, to).is_none(), "{from:?} to {to:?}");
    }
}

#[test]
fn a_new_task_moves_the_step_without_an_alert() {
    let live = decide(Working, Some(Working)).unwrap();
    assert_eq!(live.event, Event::Update { alert: None });
    assert_eq!(live.state.step, "Fix the flaky test");
    let live = decide(Blocked, Some(Blocked)).unwrap();
    assert_eq!(live.event, Event::Update { alert: None });
    assert_eq!(live.state.phase, Phase::Question);
}

#[test]
fn without_a_title_the_step_is_the_default() {
    let live = decide_terminal(
        &moved(Idle, Some(Working)),
        Some(&agent(None)),
        "studio",
        FINGERPRINT,
        NOW_MS,
    )
    .unwrap();
    assert_eq!(live.state.step, DEFAULT_STEP);
}

#[test]
fn a_terminal_activity_says_so_and_a_chat_leaves_it_out() {
    let live = decide(Idle, Some(Working)).unwrap();
    let Event::Start { attributes, .. } = &live.event else {
        panic!("{:?}", live.event);
    };
    assert!(attributes.terminal);
    let json = serde_json::to_value(attributes).unwrap();
    assert_eq!(json["terminal"], true);
    let chat = Attributes {
        terminal: false,
        ..attributes.clone()
    };
    assert!(
        serde_json::to_value(chat)
            .unwrap()
            .get("terminal")
            .is_none()
    );
}
