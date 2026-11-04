use std::collections::{HashMap, HashSet};

use gpui::SharedString;
use herdr_client::protocol::AgentStatus;
use herdr_orchestrator::team_store::{
    self, Author, Member, RecipientStatus, SessionTarget, Task, Team,
};

use super::Pending;
use crate::control::server::team_worker::{Operation, Reply};
use crate::i18n::fl;
use crate::team::model::{
    DeliveryState, PaneCandidate, PaneIdentity, TeamFinding, TeamMember, TeamPost, TeamTask,
};

pub(crate) const HUMAN_ID: &str = "human";

pub(crate) fn human() -> Author {
    Author::Human {
        user_id: HUMAN_ID.to_owned(),
    }
}

/// Member id → display name, built once per projection pass so author and
/// recipient lookups do not rescan the roster per item.
fn name_map(team: &Team) -> HashMap<String, SharedString> {
    roster(team)
        .map(|member| {
            (
                member.member_id.clone(),
                SharedString::from(member.display_name.clone()),
            )
        })
        .collect()
}

/// Resolves a member id to a display name, falling back to the raw id for
/// members who are no longer on the roster.
fn display_name(names: &HashMap<String, SharedString>, id: &str) -> SharedString {
    names
        .get(id)
        .cloned()
        .unwrap_or_else(|| SharedString::from(id.to_owned()))
}

pub(crate) fn roster(team: &Team) -> impl Iterator<Item = &Member> {
    std::iter::once(&team.supervisor).chain(team.members.iter())
}

pub(crate) fn member(name: &str, target: SessionTarget) -> Member {
    Member {
        member_id: uuid::Uuid::new_v4().to_string(),
        display_name: name.to_owned(),
        target,
    }
}

pub(crate) fn target(pane: &PaneIdentity) -> Option<SessionTarget> {
    Some(SessionTarget {
        host: pane.daemon_id.clone(),
        session: pane.session_id.clone().filter(|id| !id.trim().is_empty())?,
        workspace: Some(pane.workspace_id.clone()),
        pane: Some(pane.pane_id.clone()),
    })
}

pub(crate) fn pane_of(member: &Member) -> Option<PaneIdentity> {
    Some(PaneIdentity {
        daemon_id: member.target.host.clone(),
        session_id: Some(member.target.session.clone()),
        workspace_id: member.target.workspace.clone()?,
        pane_id: member.target.pane.clone()?,
    })
}

pub(crate) fn members(
    team: &Team,
    candidates: &[PaneCandidate],
    team_paused: bool,
    paused_members: &HashSet<String>,
) -> Vec<TeamMember> {
    roster(team)
        .map(|member| {
            let pane = pane_of(member);
            let matched = pane.as_ref().and_then(|pane| {
                candidates.iter().find(|c| c.identity == *pane).or_else(|| {
                    candidates
                        .iter()
                        .find(|c| c.identity.pane_id == pane.pane_id)
                })
            });
            let live = matched.is_some();
            let supervisor = member.member_id == team.supervisor.member_id;
            // `pane` is consumed by the `pane` field below, so capture the
            // configured target once instead of recomputing it.
            let target_pane = pane.clone();
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
                pane: matched
                    .map(|c| c.identity.clone())
                    .or_else(|| pane.filter(|_| live)),
                target_pane,
                status: matched
                    .map(|c| c.status.clone())
                    .unwrap_or_else(|| fl!("team-status-unavailable").into()),
                supervisor,
                paused: team_paused || paused_members.contains(&member.member_id),
            }
        })
        .collect()
}

pub(crate) fn view_posts(
    team: &Team,
    posts: &[(team_store::Post, Vec<(String, RecipientStatus)>)],
) -> Vec<TeamPost> {
    let names = name_map(team);
    let name = |id: &str| display_name(&names, id);
    let author = |author: &Author| match author {
        Author::Human { .. } => SharedString::from(fl!("team-author-human")),
        Author::Member { member_id } => name(member_id),
    };
    // One pass over the posts so each reply resolves its parent's author in
    // O(1) instead of rescanning every post.
    let authors: HashMap<&str, SharedString> = posts
        .iter()
        .map(|(post, _)| (post.post_id.as_str(), author(&post.author)))
        .collect();
    let roster_ids: HashSet<&str> = roster(team)
        .map(|member| member.member_id.as_str())
        .collect();
    posts
        .iter()
        .map(|(post, statuses)| TeamPost {
            id: post.post_id.clone(),
            author: author(&post.author),
            body: post.body.clone().into(),
            reply_to_author: post
                .reply_to
                .as_ref()
                .and_then(|id| authors.get(id.as_str()).cloned()),
            // A removed member keeps no delivery pill and cannot keep a post
            // counted as unacknowledged, even if their mention row predates
            // the cleanup in `remove_member`.
            deliveries: statuses
                .iter()
                .filter(|(recipient, _)| roster_ids.contains(recipient.as_str()))
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

pub(crate) fn view_tasks(team: &Team, tasks: &[Task]) -> Vec<TeamTask> {
    let names = name_map(team);
    tasks
        .iter()
        .map(|task| TeamTask {
            id: task.task_id.clone(),
            title: task.title.clone().into(),
            brief: task.brief.clone().into(),
            assignee: task.assignee.as_deref().map(|id| display_name(&names, id)),
            status: task.status,
            blocker_reason: task.blocker_reason.clone().map(Into::into),
            result: task.result.clone().map(Into::into),
        })
        .collect()
}

pub(crate) fn view_findings(team: &Team, findings: &[team_store::Finding]) -> Vec<TeamFinding> {
    let names = name_map(team);
    findings
        .iter()
        .map(|finding| TeamFinding {
            id: finding.finding_id.clone(),
            author: match &finding.author {
                Author::Human { .. } => SharedString::from(fl!("team-author-human")),
                Author::Member { member_id } => display_name(&names, member_id),
            },
            description: finding.description.clone().into(),
            evidence: finding.evidence.clone().into(),
            confidence: finding.confidence,
            supersedes: finding.supersedes.clone(),
        })
        .collect()
}

pub(crate) fn status_label(status: AgentStatus) -> SharedString {
    match status {
        AgentStatus::Idle => fl!("team-status-idle").into(),
        AgentStatus::Working => fl!("team-status-working").into(),
        AgentStatus::Blocked => fl!("team-status-blocked").into(),
        AgentStatus::Done => fl!("team-status-done").into(),
        AgentStatus::Unknown => fl!("team-status-live").into(),
    }
}

impl super::Bridge {
    /// Drops the show-earlier cursors when the selected team changes.
    pub(super) fn clear_board_cursors(&mut self) {
        self.tasks_older = None;
        self.findings_older = None;
    }

    /// Asks for the next older task page behind the board's show-earlier
    /// control, consuming the cursor while the fetch is in flight so a second
    /// click cannot request the same page twice.
    pub(super) fn load_earlier_tasks(&mut self) {
        let (Some(team_id), Some(before)) = (self.selected.clone(), self.tasks_older.take()) else {
            return;
        };
        let pending = Pending::EarlierTasks {
            team_id: team_id.clone(),
            before,
        };
        if self
            .submit(
                Operation::TaskPage {
                    team_id,
                    before,
                    limit: team_store::MAX_PAGE_SIZE,
                },
                pending,
            )
            .is_none()
        {
            self.tasks_older = Some(before);
        }
    }

    /// Asks for the next older finding page, same shape as
    /// [`super::Bridge::load_earlier_tasks`].
    pub(super) fn load_earlier_findings(&mut self) {
        let (Some(team_id), Some(before)) = (self.selected.clone(), self.findings_older.take())
        else {
            return;
        };
        let pending = Pending::EarlierFindings {
            team_id: team_id.clone(),
            before,
        };
        if self
            .submit(
                Operation::FindingPage {
                    team_id,
                    before,
                    limit: team_store::MAX_PAGE_SIZE,
                },
                pending,
            )
            .is_none()
        {
            self.findings_older = Some(before);
        }
    }

    /// Merges an older task or finding page into the board; other pending
    /// and reply pairs never meet this helper.
    pub(super) fn merge_page_reply(&mut self, pending: Pending, reply: Reply) {
        match (pending, reply) {
            (Pending::EarlierTasks { team_id, .. }, Reply::TaskPage { tasks, older }) => {
                self.merge_task_page(&team_id, tasks, older);
            }
            (
                Pending::EarlierFindings { team_id, .. },
                Reply::FindingPage {
                    findings, older, ..
                },
            ) => self.merge_finding_page(&team_id, findings, older),
            _ => {}
        }
    }

    /// Prepends an older task page under the same stale guard as snapshots:
    /// a reply for a team that is no longer selected is dropped.
    pub(super) fn merge_task_page(
        &mut self,
        team_id: &str,
        tasks: Vec<(i64, Task)>,
        older: Option<i64>,
    ) {
        if self.selected.as_deref() != Some(team_id) {
            return;
        }
        self.tasks
            .splice(..0, tasks.into_iter().map(|(_, task)| task));
        self.tasks_older = older;
    }

    /// Prepends an older finding page, same shape as
    /// [`super::Bridge::merge_task_page`].
    pub(super) fn merge_finding_page(
        &mut self,
        team_id: &str,
        findings: Vec<(i64, team_store::Finding)>,
        older: Option<i64>,
    ) {
        if self.selected.as_deref() != Some(team_id) {
            return;
        }
        self.findings
            .splice(..0, findings.into_iter().map(|(_, finding)| finding));
        self.findings_older = older;
    }
}
