//! Harness-agnostic stall detection (issue #56 point 2): a member holding
//! assigned work whose pane has produced no output and whose team CLI has
//! been silent for the configured timeout is stalled, and the supervisor is
//! woken once per stall state.
//!
//! The signals are deliberately status-independent (bulletin 289, issue #61):
//! a harness that reports `idle` mid-turn cannot create a false stall here,
//! because the judgment needs output silence (no agent `state_change_seq`
//! change) *and* no team CLI call, both over the same timeout window. The
//! daemon's `AgentStatus` is only a hint in the other direction: a `Working`
//! pane is never stalled, but `idle` alone never proves one. Terminal output
//! is never read (issue #55); only the snapshot's counters.
//!
//! Activity is window-local: a restart forgets calls made before it, so a
//! member that went quiet around a restart is detected by output silence
//! alone. Detection is per member and freezes the prompt and its dedupe
//! signal at the moment the stall was found, so a wake re-offered while the
//! supervisor is busy stays one wake per stall state.
use super::{PromptOutcome, team_cli_prefix};
use crate::SheprdWindow;
use crate::i18n::fl;
use crate::team::bridge::views::roster;
use gpui::Context;
use sheprd_client::protocol::AgentStatus;
use sheprd_orchestrator::team_store::{Member, Task, TaskStatus};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The default stall timeout: three minutes of silence on both signals.
pub(crate) const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// One member's pane signal as the snapshots report it: the daemon's status
/// hint and the agent state counter the output clock reads.
pub(crate) type PaneSignal = (AgentStatus, u64);

/// A roster member with their pane and current signal.
pub(crate) type ObservedMember<'a> = (&'a Member, String, Option<PaneSignal>);

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
    /// Pane id to the last seen `state_change_seq` and when it changed.
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

    /// Advances the output clock from the connected snapshots' agents. A pane
    /// with a new `state_change_seq` is active now; a pane that left the
    /// snapshots is dropped, so a gone member is not judged on stale silence.
    pub(crate) fn observe_panes<'a>(
        &mut self,
        agents: impl IntoIterator<Item = (&'a str, u64)>,
        now: Instant,
    ) {
        let seen: HashMap<String, u64> = agents
            .into_iter()
            .map(|(pane, seq)| (pane.to_owned(), seq))
            .collect();
        self.pane_output.retain(|pane, _| seen.contains_key(pane));
        for (pane, seq) in seen {
            match self.pane_output.get(&pane) {
                // Unchanged output keeps its original change time.
                Some((last, _)) if *last == seq => {}
                _ => {
                    self.pane_output.insert(pane, (seq, now));
                }
            }
        }
    }

    /// The member is stalled when their pane has not changed its agent state
    /// for the timeout, they have not called the team CLI within it, and the
    /// daemon does not report them working.
    fn is_stalled(
        &self,
        session: &str,
        pane_id: &str,
        agent_status: Option<AgentStatus>,
        timeout: Duration,
        now: Instant,
    ) -> bool {
        if matches!(agent_status, Some(AgentStatus::Working)) {
            return false;
        }
        let silent_output = match self.pane_output.get(pane_id) {
            Some((_, changed_at)) => now.duration_since(*changed_at) >= timeout,
            // No agent entry: output is unobservable, so no stall is claimed.
            None => return false,
        };
        let silent_calls = self
            .member_calls
            .get(session)
            .is_none_or(|called| now.duration_since(*called) >= timeout);
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
        for (member, pane_id, agent) in members {
            let Some(task) = held.get(member.member_id.as_str()) else {
                continue;
            };
            if !self.is_stalled(
                &member.target.session,
                pane_id,
                agent.map(|(status, _)| status),
                timeout,
                now,
            ) {
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
    pub(crate) fn track_team_stalls(&mut self, now: Instant, cx: &mut Context<Self>) {
        self.observe_team_pane_output(now);
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
                let agent = self
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
                    .map(|agent| (agent.agent_status, agent.state_change_seq));
                Some((member, pane_id, agent))
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
                &supervisor,
                record.prompt(&team.team_id, &cli),
                record.summary(),
                vec![signal.clone()],
                cx,
            );
            if matches!(outcome, PromptOutcome::Sent) {
                self.team_bridge
                    .mark_supervisor_woken(&team.team_id, vec![signal]);
            }
        }
    }

    fn observe_team_pane_output(&mut self, now: Instant) {
        let agents: Vec<_> = self
            .endpoints
            .iter()
            .enumerate()
            .filter_map(|(index, endpoint)| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                live.snapshot.as_ref().map(|snapshot| &snapshot.agents)
            })
            .flatten()
            .map(|agent| (agent.pane_id.as_str(), agent.state_change_seq))
            .collect();
        self.team_stalls.observe_panes(agents, now);
    }
}

/// The roster label for a member whose pane sits in a detected stall.
pub(crate) fn stalled_label() -> String {
    fl!("team-status-stalled")
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod window_tests;
