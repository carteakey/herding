//! Window-side owner of the Team service: turns view intents into bounded
//! worker operations and worker replies into prepared snapshots. Nothing here
//! blocks; storage runs on the worker threads behind [`Service`].
use super::{
    TeamAction,
    model::{PaneCandidate, PaneIdentity, TeamChoice, TeamLoadState, TeamSnapshot},
};
use crate::{
    control::server::{
        team_service::{Completion, Service},
        team_worker::{Operation, PostDelivery, Reply},
    },
    i18n::fl,
};
use gpui::SharedString;
use sheprd_orchestrator::team_store::{self, Author, Finding, MAX_MEMBERS, Task, TaskStatus, Team};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

mod actions;
pub(crate) mod prompts;
mod queue;
pub(crate) use queue::PromptDispatch;
use queue::{HeldPrompts, QueuedPrompt, SupervisorWake};
// The pause-queue bounds are only named outside the queue module by tests.
#[cfg(test)]
use queue::{PAUSE_QUEUE_CAPACITY, PROMPT_HOLD_CAPACITY};
mod views;
pub(super) use views::*;

const REFRESH: Duration = Duration::from_secs(2);
const TEAM_PAGE: usize = 50;
const POST_LIMIT: usize = 200;
/// How long a wake the drain could not deliver waits before it is retried.
/// Long enough that a reconnecting pane is not hammered every tick, short
/// enough that a brief gap does not strand an assignment. A pane that is
/// still unreachable after this long is marked undeliverable instead of
/// being re-offered forever.
pub(crate) const WAKE_RETRY: Duration = Duration::from_secs(5);
/// How often the periodic supervisor catch-up may attempt a wake. It only
/// fires on stored results no wake has covered, so a quiet team is silent.
pub(crate) const SUPERVISOR_FALLBACK: Duration = Duration::from_secs(600);
/// Oldest wake signals fall off once the ledger outgrows this.
const WAKE_LEDGER_CAPACITY: usize = 512;
/// Results listed in one catch-up prompt before "(and N more)".
const WAKE_LIST_LIMIT: usize = 8;

enum Pending {
    Teams {
        append: bool,
    },
    Snapshot {
        team_id: String,
    },
    /// An older task page in flight; `before` restores the cursor on failure.
    EarlierTasks {
        team_id: String,
        before: i64,
    },
    EarlierFindings {
        team_id: String,
        before: i64,
    },
    /// A write; the team is read again once it completes.
    Write {
        team_id: String,
    },
}

#[derive(Default)]
pub(crate) struct Bridge {
    service: Option<Service>,
    started: bool,
    pending: HashMap<u64, Pending>,
    /// The first operation the action that just ran queued, if any. Cleared
    /// when `act` starts, so held prompts can never name an older action.
    action_request: Option<u64>,
    /// Whether the action that just ran was refused - a full worker queue, or no
    /// worker at all. Its prompts are withdrawn rather than dispatched.
    action_refused: bool,
    selected: Option<String>,
    team: Option<Team>,
    goal: Option<String>,
    posts: Vec<PostDelivery>,
    tasks: Vec<Task>,
    findings: Vec<Finding>,
    /// Show-earlier cursors from the last full page; `None` when fully loaded.
    tasks_older: Option<i64>,
    findings_older: Option<i64>,
    teams: Vec<TeamChoice>,
    teams_after: Option<String>,
    failure: Option<SharedString>,
    warning: Option<SharedString>,
    last_refresh: Option<Instant>,
    snapshot_in_flight: Option<u64>,
    candidates: Vec<PaneIdentity>,
    bulletin_closed: bool,
    paused_teams: HashSet<String>,
    paused_members: HashSet<String>,
    queued_prompts: Vec<QueuedPrompt>,
    /// Wakes the drain gave up on: their pane left the roster and no
    /// connected snapshot shows it, so no later tick could deliver them.
    /// Surfaced in the team status so a stranded assignment is visible;
    /// reset when another team is shown.
    undeliverable: usize,
    /// Prompts held against the request id of the operation that produced them,
    /// so an agent is told about a row only once the store has it.
    awaiting_write: HeldPrompts,
    /// Prompts whose operation the worker confirmed, ready for this tick.
    released_prompts: Vec<PromptDispatch>,
    /// Stored results each team's supervisor has been woken about, keyed by
    /// team id. Window-local: after a restart the fallback may repeat one
    /// catch-up, which is bounded and rarely wrong to want.
    supervisor_wakes: HashMap<String, VecDeque<String>>,
    /// Members whose host endpoint is connected but whose recorded pane is
    /// gone - a daemon restart or a closed pane - keyed by member id with
    /// the boot that lost them. Window-local: the supervisor wake re-fires
    /// once per episode, and the roster shows the mark until the member is
    /// proved again or re-registers.
    needs_reregister: HashMap<String, String>,
    last_fallback: Option<Instant>,
    last_activity: Option<SharedString>,
    dirty: bool,
}

impl Bridge {
    pub(crate) fn has_selected_team(&self) -> bool {
        self.selected.is_some() && self.team.is_some()
    }

    pub(crate) fn bulletin_open(&self) -> bool {
        !self.bulletin_closed
    }

    pub(super) fn current_team_id(&self) -> Option<&str> {
        self.team
            .as_ref()
            .map(|t| t.team_id.as_str())
            .or(self.selected.as_deref())
    }

    pub(super) fn is_team_paused(&self) -> bool {
        self.current_team_id()
            .is_some_and(|id| self.paused_teams.contains(id))
    }

    pub(super) fn is_team_id_paused(&self, team_id: &str) -> bool {
        self.paused_teams.contains(team_id)
    }

    pub(super) fn is_pane_paused_for_team(&self, team_id: &str, pane_id: &str) -> bool {
        if self.is_team_id_paused(team_id) {
            return true;
        }
        let Some(team) = self.team.as_ref() else {
            return false;
        };
        if team.team_id == team_id {
            roster(team).any(|m| {
                m.target.pane.as_deref() == Some(pane_id)
                    && self.paused_members.contains(&m.member_id)
            })
        } else {
            false
        }
    }

    /// Whether this stored result has already been woken for the team's
    /// supervisor (by an event or the fallback).
    pub(crate) fn wake_seen(&self, team_id: &str, signal: &str) -> bool {
        self.supervisor_wakes
            .get(team_id)
            .is_some_and(|ledger| ledger.iter().any(|seen| seen == signal))
    }

    /// Records that a wake actually reached the supervisor, so neither the
    /// event path nor the fallback repeats it. Unavailable, failed, or merely
    /// held dispatches are not recorded: the work stays pending and a held
    /// prompt carries its signals until the drain delivers it.
    pub(crate) fn mark_supervisor_woken(
        &mut self,
        team_id: &str,
        signals: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        let ledger = self.supervisor_wakes.entry(team_id.to_owned()).or_default();
        for signal in signals {
            let signal = signal.as_ref();
            if ledger.iter().any(|seen| seen == signal) {
                continue;
            }
            if ledger.len() >= WAKE_LEDGER_CAPACITY {
                ledger.pop_front();
            }
            ledger.push_back(signal.to_owned());
        }
    }

    /// Test-only: show `team` as the selected team in this bridge.
    #[cfg(test)]
    pub(crate) fn set_team_for_test(&mut self, team: Team) {
        self.selected = Some(team.team_id.clone());
        self.team = Some(team);
    }

    /// Test-only: pause a member by id, as the roster's pause action does.
    #[cfg(test)]
    pub(crate) fn pause_member_for_test(&mut self, member_id: &str) {
        self.paused_members.insert(member_id.to_owned());
    }

    /// Test-only: clear every team and member pause.
    #[cfg(test)]
    pub(crate) fn unpause_all_for_test(&mut self) {
        self.paused_members.clear();
        self.paused_teams.clear();
    }

    /// Test-only: how many prompts wait for an unpaused pane.
    #[cfg(test)]
    pub(crate) fn queued_count_for_test(&self) -> usize {
        self.queued_count()
    }

    /// Test-only: make every queued retry due now, as another five seconds of
    /// ticks would.
    #[cfg(test)]
    pub(crate) fn force_retry_due_for_test(&mut self) {
        let past = Instant::now() - Duration::from_secs(1);
        for prompt in &mut self.queued_prompts {
            prompt.retry_at = past;
        }
    }

    /// The periodic check behind supervisor follow-through: at most one
    /// attempt per [`SUPERVISOR_FALLBACK`], only for the selected team, and
    /// only while stored task results exist that no wake has covered. A
    /// paused team, an empty backlog, and a fully reviewed board all return
    /// `None`, so completed and blocked goals stop unattended activity. A
    /// supervisor without a pane is reported honestly instead of woken.
    pub(super) fn supervisor_fallback_due(&mut self, now: Instant) -> Option<SupervisorWake> {
        if self.is_team_paused()
            || self
                .last_fallback
                .is_some_and(|at| now.duration_since(at) < SUPERVISOR_FALLBACK)
        {
            return None;
        }
        self.last_fallback = Some(now);
        let team = self.team.as_ref()?;
        let team_id = team.team_id.clone();
        let unseen: Vec<(String, String)> = self
            .tasks
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Done | TaskStatus::Blocked))
            .filter(|task| !self.wake_seen(&team_id, &prompts::wake_signal(task)))
            .map(|task| {
                let mut label = format!("\"{}\" ({})", task.title, task.status.as_str());
                if let Some(reason) = task.blocker_reason.as_deref() {
                    label.push_str(&format!(": {}", prompts::excerpt(reason)));
                }
                (prompts::wake_signal(task), label)
            })
            .collect();
        if unseen.is_empty() {
            return None;
        }
        let Some(pane_id) = team.supervisor.target.pane.clone() else {
            self.last_activity = Some(
                format!(
                    "Supervisor has no pane; {} stored result(s) pending on the board",
                    unseen.len()
                )
                .into(),
            );
            self.dirty = true;
            return None;
        };
        let cli = prompts::team_cli_prefix(&pane_id, team.supervisor.target.workspace.as_deref());
        let mut list = String::new();
        for (_, label) in unseen.iter().take(WAKE_LIST_LIMIT) {
            list.push_str(&format!(" {label};"));
        }
        if unseen.len() > WAKE_LIST_LIMIT {
            list.push_str(&format!(" (and {} more);", unseen.len() - WAKE_LIST_LIMIT));
        }
        Some(SupervisorWake {
            team_id: team_id.clone(),
            host: team.supervisor.target.host.clone(),
            pane_id,
            prompt: format!(
                "[Herdr Team] Supervisor catch-up for team {team_id}: {} stored task result(s) not yet reviewed:{list} Run `{cli} task list {team_id}` to review the board, take the next clear actions, then go idle. Do not poll or re-check for updates: further results will wake you, and if the goal is complete or nothing clear remains, stop.",
                unseen.len()
            ),
            summary: format!("Sent supervisor catch-up ({} unseen)", unseen.len()),
            signals: unseen.into_iter().map(|(signal, _)| signal).collect(),
        })
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_refresh = None;
        if let Some(team_id) = self.selected.clone() {
            self.refresh(&team_id);
        }
    }

    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        match Service::start() {
            Ok(service) => self.service = Some(service),
            Err(error) => {
                tracing::warn!(%error, "Team storage unavailable");
                self.failure = Some(fl!("team-error-not-connected").into());
            }
        }
        self.dirty = true;
        self.submit(
            Operation::ListTeams {
                after: None,
                limit: TEAM_PAGE,
            },
            Pending::Teams { append: false },
        );
    }

    fn submit(&mut self, operation: Operation, pending: Pending) -> Option<u64> {
        let Some(service) = self.service.as_mut() else {
            self.action_refused = true;
            return None;
        };
        match service.submit(operation) {
            Ok(id) => {
                self.pending.insert(id, pending);
                self.action_request = self.action_request.or(Some(id));
                Some(id)
            }
            Err(error) => {
                tracing::warn!(%error, "Team operation was not queued");
                self.action_refused = true;
                self.failure = Some(fl!("team-action-queue-full").into());
                self.dirty = true;
                None
            }
        }
    }

    fn refresh(&mut self, team_id: &str) {
        if self.snapshot_in_flight.is_some() || self.selected.as_deref() != Some(team_id) {
            return;
        }
        self.last_refresh = Some(Instant::now());
        self.snapshot_in_flight = self.submit(
            Operation::Snapshot {
                team_id: team_id.to_owned(),
                limit: POST_LIMIT,
            },
            Pending::Snapshot {
                team_id: team_id.to_owned(),
            },
        );
    }

    fn save(&mut self, team: Team) {
        let team_id = team.team_id.clone();
        self.submit(Operation::SaveTeam(team), Pending::Write { team_id });
    }

    fn complete(&mut self, completion: Completion) {
        let request_id = completion.request_id;
        let Some(pending) = self.pending.remove(&request_id) else {
            return;
        };
        if matches!(pending, Pending::Snapshot { .. })
            && self.snapshot_in_flight != Some(request_id)
        {
            self.drop_held_prompts(request_id);
            return;
        }
        self.dirty = true;
        // Hand the show-earlier cursor back; a successful merge replaces it.
        match &pending {
            Pending::EarlierTasks { before, .. } => self.tasks_older = Some(*before),
            Pending::EarlierFindings { before, .. } => self.findings_older = Some(*before),
            _ => {}
        }
        let reply = match completion.result {
            Ok(reply) => reply,
            Err(error)
                if matches!(
                    error.as_ref(),
                    crate::control::server::team_worker::Error::External(_)
                ) =>
            {
                self.drop_held_prompts(request_id);
                self.warning = Some(error.to_string().into());
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "Team operation failed");
                self.drop_held_prompts(request_id);
                self.failure = Some(fl!("team-error-not-connected").into());
                if matches!(pending, Pending::Snapshot { .. })
                    && self.snapshot_in_flight == Some(request_id)
                {
                    self.snapshot_in_flight = None;
                }
                return;
            }
        };
        self.failure = None;
        self.release_held_prompts(request_id);
        match (pending, reply) {
            (Pending::Teams { append }, Reply::Teams(teams)) => {
                self.teams_after = (teams.len() == TEAM_PAGE)
                    .then(|| teams.last().map(|team| team.team_id.clone()))
                    .flatten();
                let choices = teams.into_iter().map(|team| TeamChoice {
                    team_id: team.team_id,
                    name: team.name.into(),
                });
                if append {
                    self.teams.extend(choices);
                } else {
                    self.teams = choices.collect();
                }
            }
            (
                Pending::Snapshot { team_id },
                Reply::Snapshot {
                    team,
                    goal,
                    posts,
                    tasks,
                    findings,
                    task_older,
                    finding_older,
                },
            ) => {
                if self.snapshot_in_flight == Some(request_id) {
                    self.snapshot_in_flight = None;
                }
                // A reply for a team that is no longer selected is stale.
                if self.selected.as_deref() == Some(team_id.as_str()) {
                    self.team = team;
                    self.goal = goal;
                    self.posts = posts;
                    self.tasks = tasks;
                    self.findings = findings;
                    self.tasks_older = task_older;
                    self.findings_older = finding_older;
                }
            }
            (pending @ (Pending::EarlierTasks { .. } | Pending::EarlierFindings { .. }), reply) => {
                self.merge_page_reply(pending, reply)
            }
            (Pending::Write { team_id }, _) => {
                self.refresh(&team_id);
                self.submit(
                    Operation::ListTeams {
                        after: None,
                        limit: TEAM_PAGE,
                    },
                    Pending::Teams { append: false },
                );
            }
            _ => {}
        }
    }

    pub(crate) fn snapshot(&self, candidates: &[PaneCandidate]) -> TeamSnapshot {
        // Retained board sizes, so the memory sampler names a growing team
        // view instead of only the process total.
        crate::memory::gauge("team.posts").set(self.posts.len() as u64);
        crate::memory::gauge("team.tasks").set(self.tasks.len() as u64);
        crate::memory::gauge("team.findings").set(self.findings.len() as u64);
        let state = match (&self.failure, self.started) {
            (Some(message), _) => TeamLoadState::Failed(message.clone()),
            (None, false) => TeamLoadState::Loading,
            (None, true) => TeamLoadState::Ready,
        };
        let (members, posts, tasks, findings) = match &self.team {
            Some(team) => (
                members(
                    team,
                    candidates,
                    self.is_team_paused(),
                    &self.paused_members,
                    &self.needs_reregister,
                ),
                view_posts(team, &self.posts),
                view_tasks(team, &self.tasks),
                view_findings(team, &self.findings),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        TeamSnapshot {
            state,
            team_id: self.team.as_ref().map(|team| team.team_id.clone()),
            team_name: self.team.as_ref().map(|team| team.name.clone().into()),
            team_goal: self.goal.as_ref().map(|goal| goal.clone().into()),
            teams: self.teams.clone(),
            teams_after: self.teams_after.clone(),
            members,
            pane_candidates: candidates.to_vec(),
            posts,
            tasks,
            findings,
            tasks_older: self.team.is_some() && self.tasks_older.is_some(),
            findings_older: self.team.is_some() && self.findings_older.is_some(),
            post_author: self
                .team
                .as_ref()
                .map(|_| (human(), fl!("team-author-human").into())),
            bulletin_open: !self.bulletin_closed,
            paused: self.is_team_paused(),
            queued_count: self.queued_count(),
            undeliverable_count: self.undeliverable,
            last_activity: self.last_activity.clone(),
        }
    }
}

#[cfg(test)]
mod perf_tests;

#[cfg(test)]
mod persist_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod view_tests;

#[cfg(test)]
mod wake_host_tests;

#[cfg(test)]
mod wake_tests;
