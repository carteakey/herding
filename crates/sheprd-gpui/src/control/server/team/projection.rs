//! Shaping worker replies into wire responses: validation, error mapping,
//! and the member/task/finding/bulletin projections the CLI renders.
use crate::control::protocol::{
    ErrorCode, Response,
    team::{
        self, BulletinEntry, ConfidenceLevel, DeliveryState, FindingInfo, MemberInfo, TaskInfo,
        TaskState,
    },
};
use crate::control::server::team_worker::{Error as WorkerError, Reply};
use sheprd_orchestrator::team_store::{
    self, Author, Confidence, Finding, Member, Post, RecipientStatus, Task, TaskStatus, Team,
};
use std::collections::HashMap;

pub(super) fn validate_post(
    team_id: &str,
    reply_to: Option<&str>,
    mentions: &[String],
    text: &str,
) -> Option<Response> {
    if !valid_id(team_id, team::MAX_TEAM_ID_BYTES)
        || reply_to.is_some_and(|id| !valid_id(id, team::MAX_MESSAGE_ID_BYTES))
        || mentions.len() > team::MAX_MENTIONS
        || mentions
            .iter()
            .any(|id| !valid_id(id, team::MAX_MEMBER_ID_BYTES))
        || text.is_empty()
        || text.len() > team::MAX_BULLETIN_TEXT_BYTES
    {
        return Some(invalid("Invalid team post field or size limit"));
    }
    None
}

pub(super) fn valid_id(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max
}

pub(super) fn invalid(message: &str) -> Response {
    Response::error(ErrorCode::InvalidRequest, message)
}

pub(super) fn worker_error(error: WorkerError) -> Response {
    match error {
        WorkerError::Busy => Response::error(ErrorCode::Busy, "Team worker queue is full"),
        WorkerError::NotMember => Response::error(
            ErrorCode::Unsupported,
            "The proven caller session is not a registered member",
        ),
        WorkerError::Stopped | WorkerError::WatchUnavailable => {
            Response::error(ErrorCode::Failed, "Team worker is unavailable")
        }
        WorkerError::External(msg) => Response::error(ErrorCode::Failed, msg),
        WorkerError::Store(source) => {
            let code = match source.as_ref() {
                team_store::Error::TeamNotFound(_)
                | team_store::Error::PostNotFound(_)
                | team_store::Error::TaskNotFound(_)
                | team_store::Error::FindingNotFound(_) => ErrorCode::NotFound,
                team_store::Error::Invalid { .. }
                | team_store::Error::TooManyMembers { .. }
                | team_store::Error::DuplicateMember(_)
                | team_store::Error::DuplicateDisplayName(_)
                | team_store::Error::MemberTargetChanged { .. }
                | team_store::Error::MemberNotInTeam { .. }
                | team_store::Error::InvalidReply(_)
                | team_store::Error::DuplicateRecipient(_)
                | team_store::Error::UnknownMention(_)
                | team_store::Error::AmbiguousMention(_)
                | team_store::Error::AmbiguousId { .. }
                | team_store::Error::NotMentioned { .. }
                | team_store::Error::TaskUnavailable(_)
                | team_store::Error::TaskNotPermitted { .. }
                | team_store::Error::InvalidTaskTransition { .. } => ErrorCode::InvalidRequest,
                _ => ErrorCode::Failed,
            };
            Response::error(code, source.to_string())
        }
    }
}

pub(super) fn member_info(
    member: &Member,
    team: &Team,
    pane_agents: &HashMap<String, String>,
) -> MemberInfo {
    let agent = member
        .target
        .pane
        .as_deref()
        .and_then(|pane| pane_agents.get(pane))
        .cloned()
        .unwrap_or_default();
    MemberInfo {
        id: member.member_id.clone(),
        name: member.display_name.clone(),
        supervisor: member.member_id == team.supervisor.member_id,
        agent,
    }
}

pub(super) fn member_display_name(team: &Team, id: &str) -> String {
    std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .find(|m| m.member_id == id)
        .map_or_else(|| id.to_owned(), |m| m.display_name.clone())
}

pub(super) fn author_parts(team: &Team, author: &Author) -> (String, String) {
    match author {
        Author::Member { member_id } => (member_id.clone(), member_display_name(team, member_id)),
        Author::Human { user_id } => (user_id.clone(), user_id.clone()),
    }
}

/// The wire's optional page size as a store limit; absent means a full page.
pub(super) fn page_limit(limit: Option<usize>) -> Option<usize> {
    let size = limit.unwrap_or(team_store::MAX_PAGE_SIZE);
    (1..=team_store::MAX_PAGE_SIZE)
        .contains(&size)
        .then_some(size)
}

pub(super) fn task_info(task: Task, team: &Team) -> TaskInfo {
    let (_, creator) = author_parts(team, &task.creator);
    let assignee = task
        .assignee
        .as_deref()
        .map(|id| member_display_name(team, id));
    let status = match task.status {
        TaskStatus::Open => TaskState::Open,
        TaskStatus::InProgress => TaskState::InProgress,
        TaskStatus::Blocked => TaskState::Blocked,
        TaskStatus::Done => TaskState::Done,
        TaskStatus::Cancelled => TaskState::Cancelled,
    };
    TaskInfo {
        id: task.task_id,
        title: task.title,
        brief: task.brief,
        creator,
        assignee_id: task.assignee,
        assignee,
        status,
        blocker_reason: task.blocker_reason,
        result: task.result,
        created_at: task
            .created_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        updated_at: task
            .updated_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    }
}

pub(super) fn finding_info(finding: Finding, team: &Team) -> FindingInfo {
    let (author_id, author) = author_parts(team, &finding.author);
    let confidence = match finding.confidence {
        Confidence::Low => ConfidenceLevel::Low,
        Confidence::Medium => ConfidenceLevel::Medium,
        Confidence::High => ConfidenceLevel::High,
    };
    FindingInfo {
        id: finding.finding_id,
        author_id,
        author,
        task_id: finding.task_id,
        description: finding.description,
        evidence: finding.evidence,
        confidence,
        supersedes: finding.supersedes,
    }
}

pub(super) fn entry(
    sequence: i64,
    post: Post,
    team: &Team,
    member_id: &str,
    status: Option<RecipientStatus>,
) -> BulletinEntry {
    let (author_id, author) = author_parts(team, &post.author);
    let delivery_state = if !post.mentions.contains(&member_id.to_owned()) {
        DeliveryState::NotMentioned
    } else if matches!(status, Some(RecipientStatus::Acknowledged(_))) {
        DeliveryState::Acknowledged
    } else {
        DeliveryState::Pending
    };
    BulletinEntry {
        sequence: u64::try_from(sequence).unwrap_or(0),
        id: post.post_id,
        member_id: author_id,
        author,
        reply_to: post.reply_to,
        mentions: post.mentions,
        text: post.body,
        delivery_state,
    }
}

pub(super) fn reply_response(reply: Reply, pane_agents: &HashMap<String, String>) -> Response {
    use team::Reply as WireReply;
    match reply {
        Reply::ScopedStatus {
            team,
            member,
            goal,
            pending_mentions,
            open_tasks,
            assigned_tasks,
        } => Response::Team {
            reply: WireReply::Status {
                team: team::TeamInfo {
                    id: team.team_id.clone(),
                    name: team.name.clone(),
                },
                member: Box::new(member_info(&member, &team, pane_agents)),
                goal,
                pending_mentions,
                open_tasks,
                assigned_tasks,
            },
        },
        Reply::ScopedRegistered { team, member } => Response::Team {
            reply: WireReply::Registered {
                member: Box::new(member_info(&member, &team, pane_agents)),
            },
        },
        Reply::ScopedRoster { team } => Response::Team {
            reply: WireReply::Roster {
                team: team::TeamInfo {
                    id: team.team_id.clone(),
                    name: team.name.clone(),
                },
                members: std::iter::once(&team.supervisor)
                    .chain(team.members.iter())
                    .map(|member| member_info(member, &team, pane_agents))
                    .collect(),
            },
        },
        Reply::ScopedGoalUpdated { team, goal } => Response::Team {
            reply: WireReply::GoalUpdated {
                team_id: team.team_id,
                goal: (!goal.trim().is_empty()).then_some(goal),
            },
        },
        Reply::ScopedBulletin {
            team,
            member_id,
            entries,
            next,
        } => {
            let entries = entries
                .into_iter()
                .map(|(sequence, post, status)| entry(sequence, post, &team, &member_id, status))
                .collect::<Vec<_>>();
            let next = next.and_then(|sequence| u64::try_from(sequence).ok());
            Response::Team {
                reply: WireReply::Bulletin { entries, next },
            }
        }
        Reply::ScopedPosted { team, post } => {
            let (author_id, author) = author_parts(&team, &post.author);
            Response::Team {
                reply: WireReply::Posted {
                    entry: Box::new(BulletinEntry {
                        sequence: 0,
                        id: post.post_id,
                        member_id: author_id,
                        author,
                        reply_to: post.reply_to,
                        mentions: post.mentions,
                        text: post.body,
                        delivery_state: DeliveryState::NotMentioned,
                    }),
                },
            }
        }
        Reply::ScopedAcknowledged { acknowledgement } => Response::Team {
            reply: WireReply::Acknowledged {
                message_id: acknowledgement.post_id,
                delivery_state: DeliveryState::Acknowledged,
            },
        },
        Reply::ScopedTasks { team, tasks, older } => Response::Team {
            reply: WireReply::Tasks {
                tasks: tasks
                    .into_iter()
                    .map(|(_, task)| task_info(task, &team))
                    .collect(),
                next_before: older,
            },
        },
        Reply::ScopedTask { team, task, .. } => Response::Team {
            reply: WireReply::Task {
                task: Box::new(task_info(task, &team)),
            },
        },
        Reply::ScopedFindings {
            team,
            findings,
            older,
        } => Response::Team {
            reply: WireReply::Findings {
                findings: findings
                    .into_iter()
                    .map(|(_, finding)| finding_info(finding, &team))
                    .collect(),
                next_before: older,
            },
        },
        Reply::ScopedFinding { team, finding } => Response::Team {
            reply: WireReply::Finding {
                finding: Box::new(finding_info(finding, &team)),
            },
        },
        Reply::ScopedHeartbeat {
            team,
            target_member_id,
        } => super::heartbeat::heartbeat_response(&team, target_member_id.as_deref(), pane_agents),
        _ => Response::error(ErrorCode::Failed, "Unexpected team worker reply"),
    }
}
