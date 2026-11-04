//! Wire serialization and reply conversion for team worker responses.
use super::heartbeat::heartbeat_response;
use super::*;
use crate::control::protocol::team::Reply as WireReply;

pub(super) fn reply_response(reply: Reply, pane_agents: &HashMap<String, String>) -> Response {
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
        } => {
            let entries = entries
                .into_iter()
                .map(|(sequence, post, status)| entry(sequence, post, &team, &member_id, status))
                .collect::<Vec<_>>();
            let next = entries.last().map(|entry| entry.sequence);
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
        } => heartbeat_response(&team, target_member_id.as_deref(), pane_agents),
        _ => Response::error(ErrorCode::Failed, "Unexpected team worker reply"),
    }
}

fn member_info(member: &Member, team: &Team, pane_agents: &HashMap<String, String>) -> MemberInfo {
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

fn task_info(task: Task, team: &Team) -> TaskInfo {
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
    }
}

fn finding_info(finding: Finding, team: &Team) -> FindingInfo {
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

fn entry(
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
