//! Window-side owner of the Team service: turns view intents into bounded
//! worker operations and worker replies into prepared snapshots. Nothing here
//! blocks; storage runs on the worker threads behind [`Service`].
use super::{
    TeamAction,
    model::{
        DeliveryState, PaneCandidate, PaneIdentity, TeamChoice, TeamFinding, TeamLoadState,
        TeamMember, TeamPost, TeamSnapshot, TeamTask,
    },
};
use crate::{
    HerdrWindow,
    control::server::{
        team_service::{Completion, Service},
        team_worker::{Operation, PostDelivery, Reply},
    },
    i18n::fl,
    navigation::NavigationTarget,
};
use gpui::{Context, SharedString};
use herdr_client::protocol::AgentStatus;
use herdr_orchestrator::team_store::{
    self, Author, Finding, MAX_MEMBERS, Member, RecipientStatus, SessionTarget, Task, Team,
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const REFRESH: Duration = Duration::from_secs(2);
const TEAM_PAGE: usize = 50;
const POST_LIMIT: usize = 200;
/// The one human author. Agents are authors only through the control socket.
const HUMAN_ID: &str = "human";

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
    dirty: bool,
}

impl Bridge {
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
            TeamAction::FocusWorker { .. } => {}
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

    fn snapshot(&self, candidates: &[PaneCandidate]) -> TeamSnapshot {
        let state = match (&self.failure, self.started) {
            (Some(message), _) => TeamLoadState::Failed(message.clone()),
            (None, false) => TeamLoadState::Loading,
            (None, true) => TeamLoadState::Ready,
        };
        let (members, posts, tasks, findings) = match &self.team {
            Some(team) => (
                members(team, candidates),
                self.view_posts(team),
                self.view_tasks(team),
                self.view_findings(team),
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
        }
    }

    fn view_posts(&self, team: &Team) -> Vec<TeamPost> {
        let name = |id: &str| -> SharedString {
            roster(team)
                .find(|member| member.member_id == id)
                .map_or_else(|| id.to_owned(), |member| member.display_name.clone())
                .into()
        };
        let author = |author: &Author| match author {
            Author::Human { .. } => SharedString::from(fl!("team-author-human")),
            Author::Member { member_id } => name(member_id),
        };
        self.posts
            .iter()
            .map(|(post, statuses)| TeamPost {
                id: post.post_id.clone(),
                author: author(&post.author),
                body: post.body.clone().into(),
                reply_to: post.reply_to.clone(),
                reply_to_author: post.reply_to.as_ref().and_then(|id| {
                    self.posts
                        .iter()
                        .find(|(other, _)| other.post_id == *id)
                        .map(|(other, _)| author(&other.author))
                }),
                deliveries: statuses
                    .iter()
                    .map(|(recipient, status)| {
                        let state = match status {
                            RecipientStatus::Pending => DeliveryState::Pending,
                            RecipientStatus::Acknowledged(_) => DeliveryState::Acknowledged,
                        };
                        (recipient.clone(), name(recipient), state)
                    })
                    .collect(),
            })
            .collect()
    }

    fn view_tasks(&self, team: &Team) -> Vec<TeamTask> {
        let name = |id: &str| -> SharedString {
            roster(team)
                .find(|member| member.member_id == id)
                .map_or_else(|| id.to_owned(), |member| member.display_name.clone())
                .into()
        };
        self.tasks
            .iter()
            .map(|task| TeamTask {
                id: task.task_id.clone(),
                title: task.title.clone().into(),
                brief: task.brief.clone().into(),
                assignee: task.assignee.as_deref().map(name),
                status: task.status,
                blocker_reason: task.blocker_reason.clone().map(Into::into),
                result: task.result.clone().map(Into::into),
            })
            .collect()
    }

    fn view_findings(&self, team: &Team) -> Vec<TeamFinding> {
        let name = |id: &str| -> SharedString {
            roster(team)
                .find(|member| member.member_id == id)
                .map_or_else(|| id.to_owned(), |member| member.display_name.clone())
                .into()
        };
        let author = |author: &Author| match author {
            Author::Human { .. } => SharedString::from(fl!("team-author-human")),
            Author::Member { member_id } => name(member_id),
        };
        self.findings
            .iter()
            .map(|finding| TeamFinding {
                id: finding.finding_id.clone(),
                author: author(&finding.author),
                description: finding.description.clone().into(),
                evidence: finding.evidence.clone().into(),
                confidence: finding.confidence,
                supersedes: finding.supersedes.clone(),
            })
            .collect()
    }
}

fn human() -> Author {
    Author::Human {
        user_id: HUMAN_ID.to_owned(),
    }
}

fn roster(team: &Team) -> impl Iterator<Item = &Member> {
    std::iter::once(&team.supervisor).chain(team.members.iter())
}

fn member(name: &str, target: SessionTarget) -> Member {
    Member {
        member_id: uuid::Uuid::new_v4().to_string(),
        display_name: name.to_owned(),
        target,
    }
}

fn target(pane: &PaneIdentity) -> Option<SessionTarget> {
    Some(SessionTarget {
        host: pane.daemon_id.clone(),
        session: pane.session_id.clone().filter(|id| !id.trim().is_empty())?,
        workspace: Some(pane.workspace_id.clone()),
        pane: Some(pane.pane_id.clone()),
    })
}

fn pane_of(member: &Member) -> Option<PaneIdentity> {
    Some(PaneIdentity {
        daemon_id: member.target.host.clone(),
        session_id: Some(member.target.session.clone()),
        workspace_id: member.target.workspace.clone()?,
        pane_id: member.target.pane.clone()?,
    })
}

fn members(team: &Team, candidates: &[PaneCandidate]) -> Vec<TeamMember> {
    roster(team)
        .map(|member| {
            let pane = pane_of(member);
            let matched = pane
                .as_ref()
                .and_then(|pane| candidates.iter().find(|c| c.identity == *pane));
            let live = matched.is_some();
            let supervisor = member.member_id == team.supervisor.member_id;
            TeamMember {
                member_id: member.member_id.clone(),
                name: member.display_name.clone().into(),
                role: if supervisor {
                    fl!("team-role-supervisor")
                } else {
                    fl!("team-role-worker")
                }
                .into(),
                agent: matched.map(|c| c.agent.clone()).unwrap_or_default(),
                pane: pane.filter(|_| live),
                status: matched
                    .map(|c| c.status.clone())
                    .unwrap_or_else(|| fl!("team-status-unavailable").into()),
                supervisor,
            }
        })
        .collect()
}

fn status_label(status: AgentStatus) -> SharedString {
    match status {
        AgentStatus::Idle => fl!("team-status-idle").into(),
        AgentStatus::Working => fl!("team-status-working").into(),
        AgentStatus::Blocked => fl!("team-status-blocked").into(),
        AgentStatus::Done => fl!("team-status-done").into(),
        AgentStatus::Unknown => fl!("team-status-live").into(),
    }
}

impl HerdrWindow {
    /// Sessions this window can prove: native ACP chats, verified CLI bindings,
    /// and active terminal agent sessions in the selected daemon's snapshot.
    fn team_candidates(&self) -> Vec<PaneCandidate> {
        let Some(snapshot) = self.live.snapshot.as_deref() else {
            return Vec::new();
        };
        if !self.live.status.is_connected() || snapshot.boot_id.is_empty() {
            return Vec::new();
        }
        snapshot
            .panes
            .iter()
            .filter_map(|pane| {
                let (session, agent_kind) =
                    crate::control::server::team::prove_pane_session(self, &pane.pane_id)?;
                let agent_entry = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == pane.pane_id);
                let base_label = pane
                    .label
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| agent_entry.and_then(|a| a.title.as_deref()))
                    .unwrap_or(pane.pane_id.as_str());
                let label = if agent_kind.is_empty() {
                    base_label.to_owned()
                } else {
                    format!("{base_label} ({agent_kind})")
                };
                let status = agent_entry
                    .map(|a| status_label(a.agent_status))
                    .unwrap_or_else(|| fl!("team-status-live").into());
                Some(PaneCandidate {
                    identity: PaneIdentity {
                        daemon_id: snapshot.boot_id.clone(),
                        session_id: Some(session),
                        workspace_id: pane.workspace_id.clone(),
                        pane_id: pane.pane_id.clone(),
                    },
                    label: label.into(),
                    agent: agent_kind.into(),
                    status,
                })
            })
            .collect()
    }

    /// Runs on each window tick. Idle until the Team panel is first shown.
    pub(crate) fn poll_team(&mut self, cx: &mut Context<Self>) {
        let shown = self.activity_section() == Some(herdr_control_protocol::ActivityPanel::Team);
        if !shown && !self.team_bridge.started {
            return;
        }
        let candidates = self.team_candidates();
        let actions = self.take_team_actions(cx);
        let bridge = &mut self.team_bridge;
        bridge.start();
        if let Some(service) = bridge.service.as_ref() {
            for completion in service.poll() {
                bridge.complete(completion);
            }
        }
        let mut focus = None;
        for action in actions {
            match action {
                // Panes belong to the window, so focusing is not the bridge's.
                TeamAction::FocusWorker { pane } => focus = Some(pane),
                action => bridge.act(action, &candidates),
            }
        }
        if shown
            && let Some(team_id) = bridge.selected.clone()
            && bridge.last_refresh.is_none_or(|at| at.elapsed() >= REFRESH)
        {
            bridge.refresh(&team_id);
        }
        let identities: Vec<_> = candidates.iter().map(|c| c.identity.clone()).collect();
        let focus = focus.filter(|pane| identities.contains(pane));
        let bridge = &mut self.team_bridge;
        if bridge.dirty || bridge.candidates != identities {
            bridge.dirty = false;
            bridge.candidates = identities;
            let warning = bridge.warning.clone();
            let snapshot = bridge.snapshot(&candidates);
            self.set_team_snapshot(snapshot, cx);
            self.team_view
                .update(cx, |view, cx| view.set_action_warning(warning, cx));
        }
        if let Some(pane) = focus {
            self.navigate(NavigationTarget::Pane(pane.pane_id.as_str()), cx);
        }
    }
}

#[cfg(test)]
mod tests;
