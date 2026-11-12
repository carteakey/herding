//! Harness-agnostic stall detection (issue #56 point 2): a member holding
//! assigned work whose pane has produced no output and whose team CLI has
//! been silent for the configured timeout is stalled, and the supervisor is
//! woken once per stall state.
//!
//! The output signal is the daemon's terminal content revision, the counter
//! `herdr pane list` reports: it climbs with every redraw, spinner and clock
//! included, and freezes when the pane stops drawing - so a harness that
//! reports `idle` mid-turn (#61) cannot create a false stall, and a turn
//! stuck `working` cannot hide from it either. The daemon's `AgentStatus` is
//! only a window hint: a `Working` pane is judged on the same silence but
//! over a tripled window, so a slow compile is not a stall while a turn that
//! died working still trips it (#56). Panes the surface frame does not carry
//! (a background tab, a daemon without the surface capability) have no
//! observable output and are never claimed stalled. Terminal output is never
//! read (issue #55); only the counter.
//!
//! Activity is window-local: a restart forgets calls made before it, so a
//! member that went quiet around a restart is detected by output silence
//! alone. Detection is per member and freezes the prompt and its dedupe
//! signal at the moment the stall was found, so a wake re-offered while the
//! supervisor is busy stays one wake per stall state.
use super::{PromptDispatch, PromptOutcome, team_cli_prefix};
use crate::SheprdWindow;
use crate::i18n::fl;
use crate::team::bridge::views::roster;
use sheprd_client::ConnectTarget;
use sheprd_client::protocol::AgentStatus;
use sheprd_orchestrator::team_store::{Member, Task, TaskStatus};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use pane_list::PaneListTarget;

/// The default stall timeout: three minutes of silence on both signals.
pub(crate) const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// How often each connected endpoint's daemon is asked for every pane's
/// content revision: the surface frame covers only the panes the window
/// renders, and this poll covers the rest, background tabs included
/// (finding 736a1b22).
pub(crate) const DEFAULT_PANE_LIST_POLL: Duration = Duration::from_secs(10);

/// How a connected endpoint's daemon is asked for its pane revisions, when
/// the CLI can address it at all: a custom client socket, WSL, and cloud
/// hosts have no CLI route here, so their panes stay surface-frame-only.
fn pane_list_target(target: &ConnectTarget) -> Option<PaneListTarget> {
    match target {
        ConnectTarget::Local => Some(PaneListTarget::Local { session: None }),
        ConnectTarget::Session { name, .. } => Some(PaneListTarget::Local {
            session: Some(name.clone()),
        }),
        ConnectTarget::Ssh { target, session } => Some(PaneListTarget::Ssh {
            target: target.clone(),
            session: Some(session.clone()),
        }),
        ConnectTarget::Socket(_) | ConnectTarget::Wsl { .. } => None,
        #[cfg(feature = "cloud")]
        ConnectTarget::Cloud { .. } => None,
    }
}

/// How much longer a pane the daemon reports `Working` may sit silent before
/// it is stalled: a compile or a long tool run redraws nothing, so the plain
/// timeout would false-positive, but a turn that died working must still
/// trip eventually (#56).
const WORKING_WINDOW_FACTOR: u32 = 3;

/// The daemon's status hint for a member's pane, from the snapshot's agents.
pub(crate) type PaneStatusHint = Option<AgentStatus>;

/// A roster member with their pane and the status hint about it.
pub(crate) type ObservedMember<'a> = (&'a Member, String, PaneStatusHint);

/// One detected stall, frozen until the member is active again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StalledMember {
    pub(crate) member_id: String,
    pub(crate) display_name: String,
    pub(crate) pane_id: String,
    pub(crate) task_id: String,
    pub(crate) task_title: String,
    /// Whole minutes of silence when the stall was detected, frozen with the
    /// rest of the record so the dedupe signal cannot drift.
    pub(crate) minutes: u64,
    /// Which stall episode this is for the member, so a stall that returns
    /// after a recovery re-wakes even at the same minute count.
    episode: u64,
}

impl StalledMember {
    /// The dedupe key: one wake per member, task, and episode. A recovery
    /// clears the detection, so a later stall re-wakes even when it froze at
    /// the same minute count.
    pub(crate) fn signal(&self, team_id: &str) -> String {
        format!(
            "stall|{}|{}|{}|{}",
            team_id, self.task_id, self.member_id, self.episode
        )
    }

    fn prompt(&self, team_id: &str, cli: &str) -> String {
        format!(
            "[Herdr Team] Worker {} looks stalled on task {} (\"{}\"): no team CLI calls \
             and no pane output for {} minutes. Check their pane ({}) and re-prompt once; \
             if they are gone, reassign the task with `{cli} task assign {team_id} {} --to MEMBER`, \
             or take the next clear action from `{cli} task list {team_id}`.",
            self.display_name,
            self.task_id,
            self.task_title,
            self.minutes,
            self.pane_id,
            self.task_id,
        )
    }

    fn summary(&self) -> String {
        format!(
            "Woke supervisor: {} stalled on {}",
            self.display_name, self.task_id
        )
    }
}

/// Per-pane output recency and per-member CLI recency, in wall time. The
/// window seeds member activity at construction: an unstamped member counts
/// as silent only after the first full timeout passes, so a restart cannot
/// manufacture a stall for a member that called before it.
#[derive(Default)]
pub(crate) struct StallTracker {
    /// Pane id to the last seen terminal content revision and when it moved.
    pane_output: HashMap<String, (u64, Instant)>,
    /// The caller's proven session to its last team CLI call.
    member_calls: HashMap<String, Instant>,
    /// The current stalled members, keyed by member id. An entry survives
    /// until the member is active again, keeping one wake per stall state.
    stalled: HashMap<String, StalledMember>,
    /// How many stall episodes each member has been detected in, so the
    /// dedupe signal changes when a stall returns after a recovery.
    episodes: HashMap<String, u64>,
}

impl StallTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records a team CLI call by the proven caller's session.
    pub(crate) fn note_member_call(&mut self, session: &str, now: Instant) {
        self.member_calls.insert(session.to_owned(), now);
    }

    /// Advances the output clock from the connected endpoints' surface
    /// frames. A pane whose content revision moved is active now; a pane
    /// that left the frames is dropped, so a gone member is not judged on
    /// stale silence.
    pub(crate) fn observe_panes<'a>(
        &mut self,
        panes: impl IntoIterator<Item = (&'a str, u64)>,
        now: Instant,
    ) {
        let seen: HashMap<String, u64> = panes
            .into_iter()
            .map(|(pane, revision)| (pane.to_owned(), revision))
            .collect();
        self.pane_output.retain(|pane, _| seen.contains_key(pane));
        for (pane, revision) in seen {
            match self.pane_output.get(&pane) {
                // Unchanged output keeps its original change time.
                Some((last, _)) if *last == revision => {}
                _ => {
                    self.pane_output.insert(pane, (revision, now));
                }
            }
        }
    }

    /// The member is stalled when their pane's content revision has not
    /// moved and they have not called the team CLI for the window. A pane
    /// the daemon reports `Working` gets a tripled window: drawing stops
    /// during a long tool run, but a turn that died working still trips it.
    fn is_stalled(
        &self,
        session: &str,
        pane_id: &str,
        agent_status: PaneStatusHint,
        timeout: Duration,
        now: Instant,
    ) -> bool {
        let window = match agent_status {
            Some(AgentStatus::Working) => timeout * WORKING_WINDOW_FACTOR,
            _ => timeout,
        };
        let silent_output = match self.pane_output.get(pane_id) {
            Some((_, changed_at)) => now.duration_since(*changed_at) >= window,
            // Absent from the surface frame: output is unobservable, so no
            // stall is claimed.
            None => return false,
        };
        let silent_calls = self
            .member_calls
            .get(session)
            .is_none_or(|called| now.duration_since(*called) >= window);
        silent_output && silent_calls
    }

    /// Re-derives the stalled set from this tick's members and their held
    /// tasks. An existing detection is kept frozen; a new one is recorded at
    /// `now`; a recovered member's detection is dropped, so a later stall
    /// re-wakes. Returns the current stalls as owned records.
    pub(crate) fn detect(
        &mut self,
        members: &[ObservedMember<'_>],
        held: &HashMap<&str, &Task>,
        timeout: Duration,
        now: Instant,
    ) -> Vec<StalledMember> {
        let mut still_stalled = HashMap::new();
        for (member, pane_id, status) in members {
            let Some(task) = held.get(member.member_id.as_str()) else {
                continue;
            };
            if !self.is_stalled(&member.target.session, pane_id, *status, timeout, now) {
                continue;
            }
            let record = match self.stalled.get(&member.member_id) {
                // Keep the frozen record: the wake and its signal must not
                // drift while the same stall lasts.
                Some(existing) => existing.clone(),
                None => {
                    let episode = self.episodes.entry(member.member_id.clone()).or_default();
                    *episode += 1;
                    StalledMember {
                        member_id: member.member_id.clone(),
                        display_name: member.display_name.clone(),
                        pane_id: pane_id.clone(),
                        task_id: task.task_id.clone(),
                        task_title: task.title.clone(),
                        minutes: self
                            .pane_output
                            .get(pane_id)
                            .map(|(_, changed)| now.duration_since(*changed).as_secs() / 60)
                            .unwrap_or(0),
                        episode: *episode,
                    }
                }
            };
            still_stalled.insert(member.member_id.clone(), record);
        }
        self.stalled = still_stalled;
        self.stalled.values().cloned().collect()
    }
}

impl SheprdWindow {
    /// The stalled member a roster row's pane belongs to, if any.
    pub(crate) fn team_stall_for_pane(&self, pane_id: &str) -> Option<&StalledMember> {
        self.team_stalls
            .stalled
            .values()
            .find(|record| record.pane_id == pane_id)
    }

    /// Called on every proven team CLI request, so activity covers reads as
    /// much as mutations: a worker polling its board is not stalled.
    pub(crate) fn note_team_member_call(&mut self, session: &str) {
        self.team_stalls.note_member_call(session, Instant::now());
    }

    /// Runs on each window tick, ahead of the team snapshot: refreshes the
    /// output clock, re-derives the stalled set for the bridge's current
    /// team, and wakes that team's supervisor once per stall state. Wakes go
    /// through the ordinary dispatch, so busy holds queue and retry them,
    /// and the signal keeps the drain's ledger from repeating one.
    pub(crate) fn track_team_stalls(&mut self, now: Instant) {
        let pane_list_poll = self.config.teams.pane_list_poll();
        self.observe_team_pane_output(now, pane_list_poll);
        let Some(team) = self.team_bridge.team.clone() else {
            self.team_stalls.stalled.clear();
            return;
        };
        let timeout = self.config.teams.stall_timeout();
        // Each member's pane status and session, taken before any dispatch
        // borrows the window mutably again.
        let members: Vec<ObservedMember<'_>> = roster(&team)
            .filter_map(|member: &Member| {
                let pane_id = member.target.pane.clone()?;
                let status = self
                    .endpoints
                    .iter()
                    .enumerate()
                    .find_map(|(index, endpoint)| {
                        let live = if index == self.selected_endpoint {
                            &self.live
                        } else {
                            &endpoint.live
                        };
                        live.snapshot
                            .as_ref()?
                            .agents
                            .iter()
                            .find(|agent| agent.pane_id == pane_id)
                    })
                    .map(|agent| agent.agent_status);
                Some((member, pane_id, status))
            })
            .collect();
        let members: Vec<_> = members
            .into_iter()
            .filter(|(member, _, _)| member.member_id != team.supervisor.member_id)
            .collect();
        let held: HashMap<&str, &Task> = self
            .team_bridge
            .tasks
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Open | TaskStatus::InProgress))
            .filter_map(|task| task.assignee.as_deref().map(|assignee| (assignee, task)))
            .collect();
        let detected = self.team_stalls.detect(&members, &held, timeout, now);
        let Some(supervisor) = team.supervisor.target.pane.clone() else {
            return;
        };
        for record in &detected {
            let signal = record.signal(&team.team_id);
            if self.team_bridge.wake_seen(&team.team_id, &signal) {
                continue;
            }
            let cli = team_cli_prefix(&supervisor, team.supervisor.target.workspace.as_deref());
            let outcome = self.enqueue_or_dispatch_team_prompt(
                Some(&team.team_id),
                PromptDispatch {
                    host: team.supervisor.target.host.clone(),
                    pane_id: supervisor.clone(),
                    prompt: record.prompt(&team.team_id, &cli),
                    summary: record.summary(),
                },
                vec![signal.clone()],
            );
            if matches!(outcome, PromptOutcome::Sent) {
                self.team_bridge
                    .mark_supervisor_woken(&team.team_id, vec![signal]);
            }
        }
    }

    fn observe_team_pane_output(&mut self, now: Instant, pane_list_poll: Duration) {
        // The polls fire ahead of the union, so an answer that just landed
        // counts this tick; each endpoint is asked at most once per
        // PANE_LIST_POLL, on the worker thread, never on this tick.
        let requests = self
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.live.status.is_connected())
            .map(|endpoint| {
                (
                    endpoint.id.clone(),
                    pane_list_target(&endpoint.connection.target),
                )
            });
        let updates = self.stall_pane_lists.poll(requests, now, pane_list_poll);
        for update in updates {
            match &update.panes {
                Err(error) => tracing::warn!(
                    endpoint = %update.endpoint,
                    %error,
                    "team stall pane-list poll failed; the next window retries and the endpoint's panes stay unobservable"
                ),
                Ok(panes) if panes.is_empty() => tracing::info!(
                    endpoint = %update.endpoint,
                    outcome = "empty",
                    "team stall pane-list poll answered with no panes"
                ),
                Ok(panes) => tracing::info!(
                    endpoint = %update.endpoint,
                    panes = panes.len(),
                    "team stall pane-list poll observed every pane's revision"
                ),
            }
        }
        // Surface frames cover the panes this window renders; the pane-list
        // poll covers every pane of the daemons the CLI can address. Their
        // union is one observation, so a pane either source reports keeps
        // its output clock, and a pane neither reports is unobservable - no
        // stall is claimed on it.
        let rendered: Vec<(String, u64)> = self
            .endpoints
            .iter()
            .enumerate()
            .filter_map(|(index, endpoint)| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                live.surface.as_ref().map(|frame| &frame.panes)
            })
            .flatten()
            .map(|pane| (pane.pane_id.clone(), pane.content_revision))
            .collect();
        let polled: Vec<(&str, u64)> = self.stall_pane_lists.panes().collect();
        let merged = rendered
            .iter()
            .map(|(pane, revision)| (pane.as_str(), *revision))
            .chain(polled);
        self.team_stalls.observe_panes(merged, now);
    }
}

/// The roster label for a member whose pane sits in a detected stall.
pub(crate) fn stalled_label() -> String {
    fl!("team-status-stalled")
}

pub(crate) mod pane_list;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod window_tests;
