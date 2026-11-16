//! Live Activities for agents in plain terminals, from their settled
//! status ([`crate::push::TerminalAgents`]): a run starts one, waiting for
//! the user updates it with an alert, and the run's end ends it. With no
//! transcript there are no steps, plans, or answer buttons: the step is
//! what the terminal's title says it works on.
//!
//! | Settled change | Activity |
//! | --- | --- |
//! | to working, from not running | start, working |
//! | to waiting, from not running | start, question |
//! | to waiting, while working | update, question, alert |
//! | to working, from waiting | update, working |
//! | a new task while running | update with the step, no alert |
//! | to done, while running | end, finished, lingers |
//! | to idle, or gone, while running | end, finished, dismissed at once |

use super::{
    Alert, Attributes, ContentState, Event, Live, Phase,
    decide::{DEFAULT_STEP, LINGER_S, computer_id},
};
use crate::push::{Moved, SeenAgent, excerpt, is_running};
use sheprd_client::protocol::AgentStatus;

/// The activity push `moved` calls for. `agent` is the agent as the layout
/// shows it now, `None` once its pane left.
pub(crate) fn decide(
    moved: &Moved,
    agent: Option<&SeenAgent<'_>>,
    computer: &str,
    fingerprint: &str,
    now_ms: u64,
) -> Option<Live> {
    let now_s = now_ms / 1000;
    let running = is_running(moved.from);
    let step = agent
        .and_then(|agent| agent.task)
        .filter(|task| !task.trim().is_empty())
        .map_or_else(|| DEFAULT_STEP.to_owned(), excerpt);
    let started_at = moved.started_ms.map_or(now_s, |started| started / 1000);
    let state = |phase| state(phase, step.clone(), started_at);
    let asking = |agent: &SeenAgent<'_>| Alert {
        body: place(agent),
        title: format!("{} needs an answer", agent.chat.agent),
    };
    let live = |event, state| Live {
        event,
        pane: moved.pane.clone(),
        state,
    };
    match moved.to {
        Some(AgentStatus::Working) if moved.from == AgentStatus::Blocked => {
            Some(live(Event::Update { alert: None }, state(Phase::Working)))
        }
        Some(AgentStatus::Working) if !running => {
            let agent = agent?;
            let alert = Alert {
                body: place(agent),
                title: format!("{} is working", agent.chat.agent),
            };
            Some(live(
                start(agent, alert, computer, fingerprint),
                state(Phase::Working),
            ))
        }
        Some(AgentStatus::Working) if moved.from == AgentStatus::Working => {
            Some(live(Event::Update { alert: None }, state(Phase::Working)))
        }
        Some(AgentStatus::Blocked) if moved.from == AgentStatus::Blocked => {
            Some(live(Event::Update { alert: None }, state(Phase::Question)))
        }
        Some(AgentStatus::Blocked) => {
            let agent = agent?;
            let event = if running {
                Event::Update {
                    alert: Some(asking(agent)),
                }
            } else {
                start(agent, asking(agent), computer, fingerprint)
            };
            Some(live(event, state(Phase::Question)))
        }
        Some(AgentStatus::Working) => None,
        to if running => {
            let dismissal_s = if to == Some(AgentStatus::Done) {
                now_s + LINGER_S
            } else {
                now_s
            };
            Some(live(Event::End { dismissal_s }, state(Phase::Finished)))
        }
        _ => None,
    }
}

fn start(agent: &SeenAgent<'_>, alert: Alert, computer: &str, fingerprint: &str) -> Event {
    let chat = &agent.chat;
    Event::Start {
        alert,
        attributes: Attributes {
            agent: excerpt(chat.agent),
            chat_title: excerpt(chat.title),
            computer: excerpt(computer),
            computer_id: computer_id(fingerprint),
            pane: chat.pane.to_owned(),
            terminal: true,
            workspace: excerpt(chat.workspace),
        },
    }
}

fn state(phase: Phase, step: String, started_at: u64) -> ContentState {
    ContentState {
        options: Vec::new(),
        permission: None,
        phase,
        plan_done: None,
        plan_total: None,
        previous: Vec::new(),
        question: None,
        started_at,
        step,
        summary: None,
        waiting: 0,
        working: 0,
    }
}

fn place(agent: &SeenAgent<'_>) -> String {
    let chat = &agent.chat;
    if chat.title.is_empty() {
        excerpt(chat.workspace)
    } else {
        excerpt(&format!("{} \u{b7} {}", chat.workspace, chat.title))
    }
}
