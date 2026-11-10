//! Agents in plain terminals push on their settled status.

use super::*;
use crate::push::terminals::SETTLE_MS;
use sheprd_client::protocol::AgentStatus;

const CHAT: Chat<'static> = Chat {
    agent: "Claude",
    pane: "w1-p3",
    title: "server",
    workspace: "herdr-gpui",
};

fn seen(status: AgentStatus) -> SeenAgent<'static> {
    SeenAgent {
        chat: CHAT,
        status,
        task: Some("Fix the flaky test"),
    }
}

/// Steps `agents` through `statuses`, one second apart from `start_ms`.
fn walk(agents: &mut TerminalAgents, start_ms: u64, statuses: &[AgentStatus]) -> Vec<Moved> {
    let mut moved = Vec::new();
    for (step, status) in statuses.iter().enumerate() {
        let at = start_ms + 1000 * step as u64;
        moved.extend(agents.step([seen(*status)], at));
    }
    moved
}

#[test]
fn what_a_pane_showed_when_first_seen_is_never_announced() {
    let mut agents = TerminalAgents::default();
    assert!(walk(&mut agents, 0, &[AgentStatus::Blocked; 5]).is_empty());
}

#[test]
fn a_status_counts_once_it_held() {
    use AgentStatus::{Blocked, Working};
    let mut agents = TerminalAgents::default();
    let moved = walk(&mut agents, 0, &[Working, Blocked, Blocked]);
    assert!(moved.is_empty(), "held one second, not {SETTLE_MS} ms");
    let moved = agents.step([seen(Blocked)], 3000);
    assert_eq!(
        moved,
        [Moved {
            pane: CHAT.pane.to_owned(),
            from: Working,
            to: Some(Blocked),
            started_ms: Some(0),
        }]
    );
    assert!(agents.step([seen(Blocked)], 9000).is_empty(), "once");
}

#[test]
fn flicker_never_settles() {
    use AgentStatus::{Blocked, Working};
    let mut agents = TerminalAgents::default();
    let moved = walk(
        &mut agents,
        0,
        &[Working, Blocked, Working, Blocked, Working],
    );
    assert!(moved.is_empty());
}

#[test]
fn a_run_keeps_its_start_while_it_asks_and_works_again() {
    use AgentStatus::{Blocked, Done, Idle, Working};
    let mut agents = TerminalAgents::default();
    let moved = walk(
        &mut agents,
        0,
        &[
            Idle, Working, Working, Working, Blocked, Blocked, Blocked, Working, Working, Working,
        ],
    );
    let starts: Vec<_> = moved
        .iter()
        .map(|moved| (moved.to, moved.started_ms))
        .collect();
    assert_eq!(
        starts,
        [
            (Some(Working), Some(1000)),
            (Some(Blocked), Some(1000)),
            (Some(Working), Some(1000)),
        ]
    );
    // The run's end keeps when it began, for the finished activity's timer.
    let moved = walk(&mut agents, 20_000, &[Done, Done, Done]);
    assert_eq!(moved[0].started_ms, Some(1000));
    // The next run starts its own clock.
    let moved = walk(&mut agents, 40_000, &[Working, Working, Working]);
    assert_eq!(moved[0].started_ms, Some(40_000));
}

#[test]
fn a_pane_that_leaves_mid_run_keeps_its_start() {
    use AgentStatus::Working;
    let mut agents = TerminalAgents::default();
    let moved = walk(&mut agents, 0, &[Working]);
    assert!(moved.is_empty());
    let moved = agents.step([], 5_000);
    assert_eq!(moved[0].to, None);
    assert_eq!(moved[0].started_ms, Some(0));
}

#[test]
fn a_pane_that_left_is_told_once() {
    let mut agents = TerminalAgents::default();
    agents.step([seen(AgentStatus::Working)], 0);
    let moved = agents.step([], 1000);
    assert_eq!(moved.len(), 1);
    assert_eq!((moved[0].from, moved[0].to), (AgentStatus::Working, None));
    assert!(agents.step([], 2000).is_empty());
}

fn moved(from: AgentStatus, to: Option<AgentStatus>) -> Moved {
    Moved {
        pane: CHAT.pane.to_owned(),
        from,
        to,
        started_ms: None,
    }
}

#[test]
fn waiting_for_the_user_asks_and_opens_the_terminal() {
    let agent = seen(AgentStatus::Blocked);
    let alert = decide_terminal(
        &agent,
        &moved(AgentStatus::Working, Some(AgentStatus::Blocked)),
        Privacy::Excerpt,
    )
    .unwrap();
    assert_eq!(alert.kind, Kind::TerminalAsking);
    assert_eq!(alert.title, "Claude needs an answer");
    assert_eq!(alert.body, "Fix the flaky test\nherdr-gpui \u{b7} server");
    assert_eq!(alert.urgency, Urgency::High);
    let message = alert.message(1);
    assert_eq!(message.url, "/term/w1-p3");
    assert_eq!(message.tag, "chat-w1-p3", "the badge's clear closes it");
}

#[test]
fn finishing_a_run_tells_and_privacy_hides_the_task() {
    let agent = seen(AgentStatus::Done);
    let alert = decide_terminal(
        &agent,
        &moved(AgentStatus::Working, Some(AgentStatus::Done)),
        Privacy::Generic,
    )
    .unwrap();
    assert_eq!(alert.kind, Kind::TerminalDone);
    assert_eq!(alert.title, "Claude finished");
    assert_eq!(alert.body, "herdr-gpui \u{b7} server");
    assert_eq!(alert.urgency, Urgency::Normal);
}

#[test]
fn nothing_else_alerts() {
    use AgentStatus::{Blocked, Done, Idle, Unknown, Working};
    for (from, to) in [
        (Idle, Some(Done)),
        (Unknown, Some(Done)),
        (Blocked, Some(Working)),
        (Idle, Some(Working)),
        (Working, Some(Idle)),
        (Working, None),
    ] {
        let agent = seen(to.unwrap_or(Idle));
        assert!(
            decide_terminal(&agent, &moved(from, to), Privacy::Excerpt).is_none(),
            "{from:?} to {to:?}"
        );
    }
}

#[test]
fn a_terminal_question_skips_the_wait_and_lasts_a_day() {
    let agent = seen(AgentStatus::Blocked);
    let asking = moved(AgentStatus::Working, Some(AgentStatus::Blocked));
    let alert = decide_terminal(&agent, &asking, Privacy::Excerpt).unwrap();
    let mut throttle = Throttle::default();
    assert!(throttle.offer(alert.clone(), 0).is_some());
    assert!(throttle.offer(alert.clone(), 10).is_some());
    assert_eq!(Delivery::for_notification(&alert).ttl_s, 24 * 60 * 60);
}

fn titled(status: AgentStatus, task: &'static str) -> SeenAgent<'static> {
    SeenAgent {
        task: Some(task),
        ..seen(status)
    }
}

#[test]
fn a_new_task_in_a_run_moves_at_once_and_alerts_nobody() {
    use AgentStatus::{Idle, Working};
    let mut agents = TerminalAgents::default();
    agents.step([titled(Working, "one")], 0);
    let moved = agents.step([titled(Working, "two")], 100);
    assert_eq!(moved.len(), 1);
    assert_eq!((moved[0].from, moved[0].to), (Working, Some(Working)));
    assert!(decide_terminal(&titled(Working, "two"), &moved[0], Privacy::Excerpt).is_none());
    assert!(
        agents.step([titled(Working, "two")], 200).is_empty(),
        "once"
    );
    // Out of a run, a title is nobody's business.
    let mut agents = TerminalAgents::default();
    agents.step([titled(Idle, "one")], 0);
    assert!(agents.step([titled(Idle, "two")], 100).is_empty());
}

#[test]
fn a_new_task_on_the_way_to_another_status_rides_with_it() {
    use AgentStatus::{Blocked, Working};
    let mut agents = TerminalAgents::default();
    agents.step([titled(Working, "one")], 0);
    // Asking under a new title: one move once the status held, whose
    // activity update shows the new title.
    assert!(agents.step([titled(Blocked, "two")], 100).is_empty());
    let moved = agents.step([titled(Blocked, "two")], 100 + SETTLE_MS);
    assert_eq!(moved.len(), 1);
    assert_eq!((moved[0].from, moved[0].to), (Working, Some(Blocked)));
}
