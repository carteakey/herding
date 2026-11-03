//! Window-side owner of the Team service: turns view intents into bounded
//! worker operations and worker replies into prepared snapshots. Nothing here
//! blocks; storage runs on the worker threads behind [`Service`].
use super::{
    TeamAction,
    model::{
        DeliveryState, PaneCandidate, PaneIdentity, TeamChoice, TeamLoadState, TeamMember,
        TeamPost, TeamSnapshot,
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
use herdr_orchestrator::team_store::{
    Author, MAX_MEMBERS, Member, RecipientStatus, Recipients, SessionTarget, Team,
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
    posts: Vec<PostDelivery>,
    teams: Vec<TeamChoice>,
    teams_after: Option<String>,
    failure: Option<SharedString>,
    last_refresh: Option<Instant>,
    snapshot_in_flight: Option<u64>,
    candidates: Vec<PaneIdentity>,
    dirty: bool,
}

impl Bridge {
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
            (Pending::Snapshot { team_id }, Reply::Snapshot { team, posts }) => {
                if self.snapshot_in_flight == Some(completion.request_id) {
                    self.snapshot_in_flight = None;
                }
                // A reply for a team that is no longer selected is stale.
                if self.selected.as_deref() == Some(team_id.as_str()) {
                    self.team = team;
                    self.posts = posts;
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
            TeamAction::SelectTeam { team_id } => {
                self.selected = Some(team_id.clone());
                self.team = None;
                self.posts.clear();
                self.snapshot_in_flight = None;
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
                self.posts.clear();
                self.save(team);
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
        let mentioned = mentions(&team, &body);
        let recipients = if mentioned.is_empty() {
            Recipients::All
        } else {
            Recipients::Explicit(mentioned)
        };
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
        let (members, posts) = match &self.team {
            Some(team) => (members(team, candidates), self.view_posts(team)),
            None => (Vec::new(), Vec::new()),
        };
        TeamSnapshot {
            state,
            team_id: self.team.as_ref().map(|team| team.team_id.clone()),
            team_name: self.team.as_ref().map(|team| team.name.clone().into()),
            teams: self.teams.clone(),
            teams_after: self.teams_after.clone(),
            members,
            pane_candidates: candidates.to_vec(),
            posts,
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

/// `@name` mentions resolved to member IDs; an unknown name mentions no one.
fn mentions(team: &Team, body: &str) -> Vec<String> {
    let lowered = body.to_lowercase();
    roster(team)
        .filter(|member| {
            let tag = format!("@{}", member.display_name.to_lowercase());
            lowered.match_indices(&tag).any(|(at, _)| {
                lowered[at + tag.len()..]
                    .chars()
                    .next()
                    .is_none_or(|next| !next.is_alphanumeric())
            })
        })
        .map(|member| member.member_id.clone())
        .collect()
}

fn members(team: &Team, candidates: &[PaneCandidate]) -> Vec<TeamMember> {
    roster(team)
        .map(|member| {
            let pane = pane_of(member);
            let live = pane
                .as_ref()
                .is_some_and(|pane| candidates.iter().any(|c| c.identity == *pane));
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
                pane: pane.filter(|_| live),
                status: if live {
                    fl!("team-status-live")
                } else {
                    fl!("team-status-unavailable")
                }
                .into(),
                supervisor,
            }
        })
        .collect()
}

impl HerdrWindow {
    /// Sessions this window can prove: native ACP chats that are attached,
    /// have a transcript session ID, and sit in the selected daemon's snapshot.
    fn team_candidates(&self) -> Vec<PaneCandidate> {
        let Some(snapshot) = self.live.snapshot.as_deref() else {
            return Vec::new();
        };
        if !self.live.status.is_connected() || snapshot.boot_id.is_empty() {
            return Vec::new();
        }
        self.chats
            .panes()
            .filter(|chat| chat.target.existing.is_none() && chat.attached())
            .filter_map(|chat| {
                let session = chat
                    .session
                    .transcript()
                    .session
                    .as_ref()
                    .map(|session| session.id.clone())
                    .filter(|id| !id.is_empty())?;
                let pane = snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == chat.target.pane_id)?;
                Some(PaneCandidate {
                    identity: PaneIdentity {
                        daemon_id: snapshot.boot_id.clone(),
                        session_id: Some(session),
                        workspace_id: pane.workspace_id.clone(),
                        pane_id: pane.pane_id.clone(),
                    },
                    label: pane
                        .label
                        .clone()
                        .unwrap_or_else(|| pane.pane_id.clone())
                        .into(),
                    status: fl!("team-status-live").into(),
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
            let snapshot = bridge.snapshot(&candidates);
            self.set_team_snapshot(snapshot, cx);
        }
        if let Some(pane) = focus {
            self.navigate(NavigationTarget::Pane(pane.pane_id.as_str()), cx);
        }
    }
}

#[cfg(test)]
mod tests;
