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
use herdr_orchestrator::team_store::{self, Author, Finding, MAX_MEMBERS, Task, TaskStatus, Team};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

pub(crate) mod prompts;
mod views;
pub(super) use views::*;

const REFRESH: Duration = Duration::from_secs(2);
const TEAM_PAGE: usize = 50;
const POST_LIMIT: usize = 200;
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
    selected: Option<String>,
    team: Option<Team>,
    goal: Option<String>,
    posts: Vec<PostDelivery>,
    tasks: Vec<Task>,
    findings: Vec<Finding>,
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
    /// Stored results each team's supervisor has been woken about, keyed by
    /// team id. Window-local: after a restart the fallback may repeat one
    /// catch-up, which is bounded and rarely wrong to want.
    supervisor_wakes: HashMap<String, VecDeque<String>>,
    last_fallback: Option<Instant>,
    last_activity: Option<SharedString>,
    dirty: bool,
}

/// A pending catch-up dispatch for the selected team's supervisor.
pub(super) struct SupervisorWake {
    pub(super) team_id: String,
    pub(super) pane_id: String,
    pub(super) prompt: String,
    pub(super) summary: String,
    pub(super) signals: Vec<String>,
}

pub(crate) const PAUSE_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueuedPrompt {
    pub(super) team_id: String,
    pub(super) pane_id: String,
    pub(super) prompt: String,
    pub(super) summary: String,
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

    pub(super) fn enqueue_prompt(
        &mut self,
        team_id: String,
        pane_id: String,
        prompt: String,
        summary: String,
    ) {
        if self.queued_prompts.len() >= PAUSE_QUEUE_CAPACITY {
            self.queued_prompts.remove(0);
        }
        self.queued_prompts.push(QueuedPrompt {
            team_id,
            pane_id,
            prompt,
            summary,
        });
    }

    pub(super) fn queued_count(&self) -> usize {
        match self.current_team_id() {
            Some(team_id) => self
                .queued_prompts
                .iter()
                .filter(|p| p.team_id == team_id)
                .count(),
            None => self.queued_prompts.len(),
        }
    }

    pub(super) fn drain_unpaused_prompts(&mut self) -> Vec<(String, String, String)> {
        let Some(team) = self.team.as_ref() else {
            return Vec::new();
        };
        if self.is_team_paused() {
            return Vec::new();
        }
        let team_id = team.team_id.as_str();
        let paused_panes: HashSet<String> = roster(team)
            .filter(|m| self.paused_members.contains(&m.member_id))
            .filter_map(|m| m.target.pane.clone())
            .collect();
        let mut ready = Vec::new();
        let mut held = Vec::new();
        for item in self.queued_prompts.drain(..) {
            if item.team_id == team_id {
                if paused_panes.contains(&item.pane_id) {
                    held.push(item);
                } else {
                    ready.push((item.pane_id, item.prompt, item.summary));
                }
            } else {
                held.push(item);
            }
        }
        self.queued_prompts = held;
        ready
    }

    /// Whether this stored result has already been woken for the team's
    /// supervisor (by an event or the fallback).
    pub(crate) fn wake_seen(&self, team_id: &str, signal: &str) -> bool {
        self.supervisor_wakes
            .get(team_id)
            .is_some_and(|ledger| ledger.iter().any(|seen| seen == signal))
    }

    /// Records that a wake reached the supervisor (or is held for it), so
    /// neither the event path nor the fallback repeats it. Unavailable or
    /// failed dispatches are not recorded: the work stays pending.
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
        let service = self.service.as_mut()?;
        match service.submit(operation) {
            Ok(id) => {
                self.pending.insert(id, pending);
                Some(id)
            }
            Err(error) => {
                tracing::warn!(%error, "Team operation was not queued");
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
        let Some(pending) = self.pending.remove(&completion.request_id) else {
            return;
        };
        if matches!(pending, Pending::Snapshot { .. })
            && self.snapshot_in_flight != Some(completion.request_id)
        {
            return;
        }
        self.dirty = true;
        let reply = match completion.result {
            Ok(reply) => reply,
            Err(error)
                if matches!(
                    error.as_ref(),
                    crate::control::server::team_worker::Error::External(_)
                ) =>
            {
                self.warning = Some(error.to_string().into());
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "Team operation failed");
                self.failure = Some(fl!("team-error-not-connected").into());
                if matches!(pending, Pending::Snapshot { .. })
                    && self.snapshot_in_flight == Some(completion.request_id)
                {
                    self.snapshot_in_flight = None;
                }
                return;
            }
        };
        self.failure = None;
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
                },
            ) => {
                if self.snapshot_in_flight == Some(completion.request_id) {
                    self.snapshot_in_flight = None;
                }
                // A reply for a team that is no longer selected is stale.
                if self.selected.as_deref() == Some(team_id.as_str()) {
                    self.team = team;
                    self.goal = goal;
                    self.posts = posts;
                    self.tasks = tasks;
                    self.findings = findings;
                }
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

    fn act(&mut self, action: TeamAction, candidates: &[PaneCandidate]) {
        // A pane is only a member if the current daemon snapshot still proves it.
        let live = |pane: &PaneIdentity| {
            pane.verified()
                && candidates
                    .iter()
                    .any(|candidate| candidate.identity == *pane)
        };
        match action {
            TeamAction::SwitchTeamView => {
                self.selected = None;
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.snapshot_in_flight = None;
                self.warning = None;
                self.dirty = true;
                self.submit(
                    Operation::ListTeams {
                        after: None,
                        limit: TEAM_PAGE,
                    },
                    Pending::Teams { append: false },
                );
            }
            TeamAction::SelectTeam { team_id } => {
                self.selected = Some(team_id.clone());
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.snapshot_in_flight = None;
                self.warning = None;
                self.refresh(&team_id);
            }
            TeamAction::LoadMoreTeams { after } => {
                self.submit(
                    Operation::ListTeams {
                        after: Some(after),
                        limit: TEAM_PAGE,
                    },
                    Pending::Teams { append: true },
                );
            }
            TeamAction::CreateTeam { name, supervisor } => {
                let Some(target) = live(&supervisor).then(|| target(&supervisor)).flatten() else {
                    return;
                };
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                let team = Team {
                    team_id: uuid::Uuid::new_v4().to_string(),
                    name: name.to_owned(),
                    supervisor: member("Supervisor", target),
                    members: Vec::new(),
                };
                self.selected = Some(team.team_id.clone());
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.warning = None;
                self.save(team);
            }
            TeamAction::SetGoal { team_id, goal } => {
                let goal = goal.trim().to_owned();
                if goal.is_empty() {
                    return;
                }
                self.warning = None;
                self.submit(
                    Operation::SetGoal {
                        team_id: team_id.clone(),
                        goal,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::RenameTeam { team_id, name } => {
                let Some(mut team) = self.current(&team_id) else {
                    return;
                };
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                self.warning = None;
                team.name = name.to_owned();
                self.last_activity = Some(format!("Renamed team to \"{name}\"").into());
                self.save(team);
            }
            TeamAction::DeleteTeam { team_id } => {
                self.selected = None;
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.snapshot_in_flight = None;
                self.supervisor_wakes.remove(&team_id);
                self.warning = None;
                self.dirty = true;
                self.submit(
                    Operation::DeleteTeam { team_id },
                    Pending::Teams { append: false },
                );
                self.submit(
                    Operation::ListTeams {
                        after: None,
                        limit: TEAM_PAGE,
                    },
                    Pending::Teams { append: false },
                );
            }
            TeamAction::RenameMember {
                team_id,
                member_id,
                name,
            } => {
                let Some(mut team) = self.current(&team_id) else {
                    return;
                };
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                if roster(&team)
                    .any(|m| m.member_id != member_id && m.display_name.eq_ignore_ascii_case(name))
                {
                    self.warning = Some(fl!("team-warning-duplicate-name", name = name).into());
                    self.dirty = true;
                    return;
                }
                if team.supervisor.member_id == member_id {
                    team.supervisor.display_name = name.to_owned();
                } else if let Some(worker) =
                    team.members.iter_mut().find(|m| m.member_id == member_id)
                {
                    worker.display_name = name.to_owned();
                } else {
                    return;
                }
                self.warning = None;
                self.last_activity = Some(format!("Renamed agent to \"{name}\"").into());
                self.save(team);
            }
            TeamAction::RemoveMember { team_id, member_id } => {
                self.warning = None;
                self.last_activity = Some("Removed worker from team".into());
                self.submit(
                    Operation::RemoveMember {
                        team_id: team_id.clone(),
                        member_id,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::AddWorker {
                team_id,
                name,
                pane,
            } => {
                let (Some(mut team), Some(target)) = (self.current(&team_id), target(&pane)) else {
                    return;
                };
                let name = name.trim();
                if !live(&pane) || name.is_empty() || team.members.len() + 1 >= MAX_MEMBERS {
                    return;
                }
                if roster(&team).any(|m| m.display_name.eq_ignore_ascii_case(name)) {
                    self.warning = Some(fl!("team-warning-duplicate-name", name = name).into());
                    self.dirty = true;
                    return;
                }
                if roster(&team).any(|m| {
                    (target.pane.is_some() && m.target.pane == target.pane)
                        || (!target.session.is_empty() && m.target.session == target.session)
                }) {
                    self.warning = Some(fl!("team-warning-duplicate-target").into());
                    self.dirty = true;
                    return;
                }
                self.warning = None;
                team.members.push(member(name, target));
                self.save(team);
            }
            TeamAction::SelectSupervisor { team_id, member_id } => {
                let Some(mut team) = self.current(&team_id) else {
                    return;
                };
                let Some(index) = team.members.iter().position(|m| m.member_id == member_id) else {
                    return;
                };
                self.warning = None;
                std::mem::swap(&mut team.supervisor, &mut team.members[index]);
                self.save(team);
            }
            TeamAction::Post {
                team_id,
                author,
                body,
            } => self.post(&team_id, author, None, body),
            TeamAction::Reply {
                team_id,
                author,
                reply_to,
                body,
            } => self.post(&team_id, author, Some(reply_to), body),
            TeamAction::CreateTask {
                team_id,
                title,
                brief,
                assignee,
            } => {
                let Some(team) = self.current(&team_id) else {
                    return;
                };
                let title = title.trim().to_owned();
                let brief = brief.trim().to_owned();
                if title.is_empty() {
                    return;
                }
                let assignee_id = match assignee.as_deref().map(str::trim).filter(|s| !s.is_empty())
                {
                    Some(raw) => match team_store::resolve_member_ref(&team, raw) {
                        Ok(id) => Some(id),
                        Err(error) => {
                            self.warning = Some(error.to_string().into());
                            self.dirty = true;
                            return;
                        }
                    },
                    None => None,
                };
                self.warning = None;
                self.submit(
                    Operation::CreateTask {
                        task_id: uuid::Uuid::new_v4().to_string(),
                        team_id: team_id.clone(),
                        creator: human(),
                        title,
                        brief,
                        assignee: assignee_id,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::UpdateTask {
                team_id,
                task_id,
                title,
                brief,
                assignee,
            } => {
                let Some(team) = self.current(&team_id) else {
                    return;
                };
                let title = title.trim().to_owned();
                let brief = brief.trim().to_owned();
                if title.is_empty() {
                    return;
                }
                let assignee_id = match assignee.as_deref().map(str::trim).filter(|s| !s.is_empty())
                {
                    Some(raw) => match team_store::resolve_member_ref(&team, raw) {
                        Ok(id) => Some(id),
                        Err(error) => {
                            self.warning = Some(error.to_string().into());
                            self.dirty = true;
                            return;
                        }
                    },
                    None => None,
                };
                self.warning = None;
                self.last_activity = Some(format!("Updated task \"{title}\"").into());
                self.submit(
                    Operation::UpdateTask {
                        task_id,
                        team_id: team_id.clone(),
                        actor: human(),
                        title,
                        brief,
                        assignee: assignee_id,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::CompleteTask {
                team_id,
                task_id,
                result,
            } => {
                let result = result.trim().to_owned();
                let result = if result.is_empty() {
                    fl!("team-task-completed-default")
                } else {
                    result
                };
                self.warning = None;
                self.submit(
                    Operation::CompleteTask {
                        task_id,
                        team_id: team_id.clone(),
                        actor: human(),
                        result,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::CancelTask { team_id, task_id } => {
                self.warning = None;
                self.submit(
                    Operation::CancelTask {
                        task_id,
                        team_id: team_id.clone(),
                        actor: human(),
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::AddFinding {
                team_id,
                author,
                description,
                evidence,
                confidence,
            } => {
                let description = description.trim().to_owned();
                let evidence = evidence.trim().to_owned();
                if description.is_empty() || evidence.is_empty() {
                    return;
                }
                self.warning = None;
                self.submit(
                    Operation::AddFinding {
                        finding_id: uuid::Uuid::new_v4().to_string(),
                        team_id: team_id.clone(),
                        author,
                        task_id: None,
                        description,
                        evidence,
                        confidence,
                        supersedes_finding_id: None,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::ToggleBulletinSidebar => {
                self.bulletin_closed = !self.bulletin_closed;
                self.dirty = true;
            }
            TeamAction::TogglePauseTeam => {
                if let Some(team_id) = self
                    .team
                    .as_ref()
                    .map(|t| t.team_id.clone())
                    .or_else(|| self.selected.clone())
                {
                    if self.selected.is_none() {
                        self.selected = Some(team_id.clone());
                    }
                    let paused = if self.paused_teams.contains(&team_id) {
                        self.paused_teams.remove(&team_id);
                        false
                    } else {
                        self.paused_teams.insert(team_id);
                        true
                    };
                    self.last_activity = Some(
                        if paused {
                            "Paused team prompt delivery"
                        } else {
                            "Resumed team prompt delivery"
                        }
                        .into(),
                    );
                    self.dirty = true;
                }
            }
            TeamAction::TogglePauseMember { member_id } => {
                let paused = if self.paused_members.remove(&member_id) {
                    false
                } else {
                    self.paused_members.insert(member_id.clone());
                    true
                };
                self.last_activity = Some(
                    if paused {
                        "Paused agent prompt delivery"
                    } else {
                        "Resumed agent prompt delivery"
                    }
                    .into(),
                );
                self.dirty = true;
            }
            TeamAction::ImportGithubIssue {
                team_id,
                issue_ref,
                as_goal,
            } => {
                let issue_ref = issue_ref.trim().to_owned();
                if issue_ref.is_empty() {
                    return;
                }
                self.warning = None;
                self.last_activity = Some(format!("Imported GitHub issue {issue_ref}").into());
                self.submit(
                    Operation::ImportGithubIssue {
                        task_id: uuid::Uuid::new_v4().to_string(),
                        team_id: team_id.clone(),
                        creator: human(),
                        issue_ref,
                        as_goal,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::DispatchTask { .. }
            | TeamAction::NudgeMember { .. }
            | TeamAction::SpawnAgentSession { .. }
            | TeamAction::SplitNewPane
            | TeamAction::FocusWorker { .. } => {}
        }
    }

    fn current(&self, team_id: &str) -> Option<Team> {
        self.team.clone().filter(|team| team.team_id == team_id)
    }

    fn post(&mut self, team_id: &str, author: Author, reply_to: Option<String>, body: String) {
        let Some(team) = self.current(team_id) else {
            return;
        };
        let body = body.trim().to_owned();
        if body.is_empty() {
            return;
        }
        let recipients = match team_store::resolve_recipients(&team, &author, &[], &body) {
            Ok(recipients) => recipients,
            Err(error) => {
                self.warning = Some(error.to_string().into());
                self.dirty = true;
                return;
            }
        };
        self.warning = None;
        self.submit(
            Operation::AddPost {
                post_id: uuid::Uuid::new_v4().to_string(),
                team_id: team_id.to_owned(),
                author,
                reply_to,
                body,
                recipients,
            },
            Pending::Write {
                team_id: team_id.to_owned(),
            },
        );
    }

    pub(crate) fn snapshot(&self, candidates: &[PaneCandidate]) -> TeamSnapshot {
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
            post_author: self
                .team
                .as_ref()
                .map(|_| (human(), fl!("team-author-human").into())),
            bulletin_open: !self.bulletin_closed,
            paused: self.is_team_paused(),
            queued_count: self.queued_count(),
            last_activity: self.last_activity.clone(),
        }
    }
}

#[cfg(test)]
mod perf_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod view_tests;
