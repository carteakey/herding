//! Agents in plain terminals: Herdr detects them from the terminal (Herdr
//! 1's snapshot, herdr-core's terminal status), with no transcript, so a
//! push follows their status instead. Pure, with explicit times.
//!
//! A status counts once it held for [`SETTLE_MS`]: detection can flicker
//! between working and waiting, and an agent the user answers at the
//! computer at once needs no push. What was there when the daemon first
//! saw a pane is never announced. While a run goes on, a new task (what the
//! terminal's title says it works on) is a move to the same status, which
//! updates a Live Activity's step and alerts nobody.

use super::{Chat, Kind, Notification, Privacy, triggers};
use sheprd_client::protocol::AgentStatus;
use std::collections::HashMap;

/// How long a status holds before it counts.
pub(crate) const SETTLE_MS: u64 = 2_000;

/// One terminal agent as the layout shows it now.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Seen<'a> {
    pub chat: Chat<'a>,
    pub status: AgentStatus,
    /// What it works on, as its terminal title says.
    pub task: Option<&'a str>,
}

/// A settled status change of one terminal agent, or a new task in a run
/// (`from` and `to` the same).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Moved {
    pub pane: String,
    pub from: AgentStatus,
    /// `None` when the pane left the layout.
    pub to: Option<AgentStatus>,
    /// When the run in progress began, while it is working or asking, and
    /// when the run that just ended began, so a finished Live Activity
    /// keeps its elapsed time.
    pub started_ms: Option<u64>,
}

#[derive(Debug)]
struct Pane {
    /// The status last seen, and since when.
    raw: (AgentStatus, u64),
    settled: AgentStatus,
    started_ms: Option<u64>,
    task: Option<String>,
}

/// Every terminal agent's settled status.
#[derive(Debug, Default)]
pub(crate) struct Agents {
    panes: HashMap<String, Pane>,
}

impl Agents {
    /// Takes the agents the layout shows at `now_ms`, and says whose
    /// status settled on another one since the last step.
    pub(crate) fn step<'a>(
        &mut self,
        seen: impl IntoIterator<Item = Seen<'a>>,
        now_ms: u64,
    ) -> Vec<Moved> {
        let mut moved = Vec::new();
        let mut present = Vec::new();
        for agent in seen {
            let pane = agent.chat.pane;
            present.push(pane.to_owned());
            let Some(tracked) = self.panes.get_mut(pane) else {
                let running = is_running(agent.status);
                self.panes.insert(
                    pane.to_owned(),
                    Pane {
                        raw: (agent.status, now_ms),
                        settled: agent.status,
                        started_ms: running.then_some(now_ms),
                        task: agent.task.map(str::to_owned),
                    },
                );
                continue;
            };
            if tracked.raw.0 != agent.status {
                tracked.raw = (agent.status, now_ms);
            }
            let (status, since) = tracked.raw;
            let task_moved = tracked.task.as_deref() != agent.task;
            if task_moved {
                tracked.task = agent.task.map(str::to_owned);
            }
            if status == tracked.settled {
                if task_moved && is_running(status) {
                    moved.push(Moved {
                        pane: pane.to_owned(),
                        from: status,
                        to: Some(status),
                        started_ms: tracked.started_ms,
                    });
                }
                continue;
            }
            if now_ms.saturating_sub(since) < SETTLE_MS {
                continue;
            }
            let from = std::mem::replace(&mut tracked.settled, status);
            let run = tracked.started_ms;
            tracked.started_ms = match (is_running(from), is_running(status)) {
                (_, false) => None,
                (false, true) => Some(since),
                (true, true) => run,
            };
            moved.push(Moved {
                pane: pane.to_owned(),
                from,
                to: Some(status),
                // A run that ended reports when it began.
                started_ms: tracked.started_ms.or(run),
            });
        }
        self.panes.retain(|pane, tracked| {
            let kept = present.contains(pane);
            if !kept {
                moved.push(Moved {
                    pane: pane.clone(),
                    from: tracked.settled,
                    to: None,
                    started_ms: tracked.started_ms,
                });
            }
            kept
        });
        moved
    }
}

/// Whether an agent in `status` is in the middle of a run.
pub(crate) fn is_running(status: AgentStatus) -> bool {
    matches!(status, AgentStatus::Working | AgentStatus::Blocked)
}

/// The push a terminal agent's settled change calls for: it started
/// waiting for the user, or it finished work it was doing. Clearing an
/// answered one is the badge's business, as for a chat's question.
pub(crate) fn decide(agent: &Seen<'_>, moved: &Moved, privacy: Privacy) -> Option<Notification> {
    if moved.to == Some(moved.from) {
        return None;
    }
    let excerpt = agent
        .task
        .filter(|_| privacy == Privacy::Excerpt)
        .map(triggers::excerpt);
    let (kind, title) = match moved.to? {
        AgentStatus::Blocked => (
            Kind::TerminalAsking,
            format!("{} needs an answer", agent.chat.agent),
        ),
        AgentStatus::Done if is_running(moved.from) => {
            (Kind::TerminalDone, format!("{} finished", agent.chat.agent))
        }
        _ => return None,
    };
    let body = triggers::body(&agent.chat, excerpt, None);
    Some(Notification::new(&agent.chat, kind, title, body))
}
