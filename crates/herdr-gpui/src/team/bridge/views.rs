use std::collections::HashSet;

use gpui::SharedString;
use herdr_client::protocol::AgentStatus;
use herdr_orchestrator::team_store::{Author, Member, RecipientStatus, SessionTarget, Task, Team};

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
                target_pane: pane_of(member),
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
    posts: &[(
        herdr_orchestrator::team_store::Post,
        Vec<(String, RecipientStatus)>,
    )],
) -> Vec<TeamPost> {
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
    posts
        .iter()
        .map(|(post, statuses)| TeamPost {
            id: post.post_id.clone(),
            author: author(&post.author),
            body: post.body.clone().into(),
            reply_to_author: post.reply_to.as_ref().and_then(|id| {
                posts
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

pub(crate) fn view_tasks(team: &Team, tasks: &[Task]) -> Vec<TeamTask> {
    let name = |id: &str| -> SharedString {
        roster(team)
            .find(|member| member.member_id == id)
            .map_or_else(|| id.to_owned(), |member| member.display_name.clone())
            .into()
    };
    tasks
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

pub(crate) fn view_findings(
    team: &Team,
    findings: &[herdr_orchestrator::team_store::Finding],
) -> Vec<TeamFinding> {
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
    findings
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

pub(crate) fn status_label(status: AgentStatus) -> SharedString {
    match status {
        AgentStatus::Idle => fl!("team-status-idle").into(),
        AgentStatus::Working => fl!("team-status-working").into(),
        AgentStatus::Blocked => fl!("team-status-blocked").into(),
        AgentStatus::Done => fl!("team-status-done").into(),
        AgentStatus::Unknown => fl!("team-status-live").into(),
    }
}
