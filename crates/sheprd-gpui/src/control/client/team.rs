//! Agent-facing team, status, goal, bulletin, task, and finding commands.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::TeamCommand,
    control::protocol::{
        Request, Response,
        team::{ConfidenceLevel, DeliveryState, FindingInfo, Reply, TaskInfo, TaskState, TeamOp},
    },
};
use std::fmt::Write as _;

fn member_line(member: &crate::control::protocol::team::MemberInfo) -> String {
    format!(
        "{}  {}  {}{}",
        member.id,
        member.name,
        member.agent,
        if member.supervisor {
            "  supervisor"
        } else {
            ""
        }
    )
}

fn delivery_state_label(state: DeliveryState) -> &'static str {
    match state {
        DeliveryState::NotMentioned => "not_mentioned",
        DeliveryState::Pending => "pending",
        DeliveryState::Acknowledged => "acknowledged",
    }
}

fn task_state_label(state: TaskState) -> &'static str {
    match state {
        TaskState::Open => "open",
        TaskState::InProgress => "in_progress",
        TaskState::Blocked => "blocked",
        TaskState::Done => "done",
        TaskState::Cancelled => "cancelled",
    }
}

fn confidence_label(level: ConfidenceLevel) -> &'static str {
    match level {
        ConfidenceLevel::Low => "low",
        ConfidenceLevel::Medium => "medium",
        ConfidenceLevel::High => "high",
    }
}

fn entry_line(entry: &crate::control::protocol::team::BulletinEntry) -> String {
    let mut line = format!(
        "{}  {}  [{}] {}",
        entry.sequence,
        entry.author,
        delivery_state_label(entry.delivery_state),
        entry.text
    );
    if let Some(parent) = &entry.reply_to {
        let _ = write!(line, "  reply to {parent}");
    }
    if !entry.mentions.is_empty() {
        let _ = write!(line, "  to {}", entry.mentions.join(", "));
    }
    line
}

fn task_line(task: &TaskInfo) -> String {
    let mut line = format!(
        "{}  [{}]  {}",
        task.id,
        task_state_label(task.status),
        task.title
    );
    if let Some(assignee) = &task.assignee {
        let _ = write!(line, "  -> {assignee}");
    }
    if let Some(reason) = &task.blocker_reason {
        let _ = write!(line, "  (blocked: {reason})");
    }
    if let Some(result) = &task.result {
        let _ = write!(line, "  result: {result}");
    }
    let _ = write!(
        line,
        "  created {}  updated {}",
        task.created_at, task.updated_at
    );
    line
}

fn finding_line(finding: &FindingInfo) -> String {
    let mut line = format!(
        "{}  [{}]  {}  {} ({})",
        finding.id,
        confidence_label(finding.confidence),
        finding.author,
        finding.description,
        finding.evidence
    );
    if let Some(task_id) = &finding.task_id {
        let _ = write!(line, "  task {task_id}");
    }
    if let Some(prior) = &finding.supersedes {
        let _ = write!(line, "  supersedes {prior}");
    }
    line
}

fn to_op(command: TeamCommand) -> TeamOp {
    match command {
        TeamCommand::Status { team_id } => TeamOp::Status { team_id },
        TeamCommand::Register { team_id, name } => TeamOp::Register { team_id, name },
        TeamCommand::Roster { team_id } => TeamOp::Roster { team_id },
        TeamCommand::GoalSet { team_id, goal } => TeamOp::GoalSet { team_id, goal },
        TeamCommand::BulletinList { team_id, after } => TeamOp::BulletinList { team_id, after },
        TeamCommand::BulletinPost {
            team_id,
            reply_to,
            mentions,
            text,
        } => TeamOp::BulletinPost {
            team_id,
            reply_to,
            mentions,
            text,
        },
        TeamCommand::BulletinAck {
            team_id,
            message_id,
        } => TeamOp::BulletinAck {
            team_id,
            message_id,
        },
        TeamCommand::TaskList {
            team_id,
            before,
            limit,
        } => TeamOp::TaskList {
            team_id,
            before,
            limit,
        },
        TeamCommand::TaskCreate {
            team_id,
            title,
            brief,
            assignee,
        } => TeamOp::TaskCreate {
            team_id,
            title,
            brief,
            assignee,
        },
        TeamCommand::TaskAssign {
            team_id,
            task_id,
            assignee,
        } => TeamOp::TaskAssign {
            team_id,
            task_id,
            assignee,
        },
        TeamCommand::TaskClaim { team_id, task_id } => TeamOp::TaskClaim { team_id, task_id },
        TeamCommand::TaskBlock {
            team_id,
            task_id,
            reason,
        } => TeamOp::TaskBlock {
            team_id,
            task_id,
            reason,
        },
        TeamCommand::TaskComplete {
            team_id,
            task_id,
            result,
        } => TeamOp::TaskComplete {
            team_id,
            task_id,
            result,
        },
        TeamCommand::TaskCancel { team_id, task_id } => TeamOp::TaskCancel { team_id, task_id },
        TeamCommand::FindingList {
            team_id,
            before,
            limit,
        } => TeamOp::FindingList {
            team_id,
            before,
            limit,
        },
        TeamCommand::FindingAdd {
            team_id,
            task_id,
            confidence,
            evidence,
            supersedes,
            description,
        } => TeamOp::FindingAdd {
            team_id,
            task_id,
            confidence,
            evidence,
            supersedes,
            description,
        },
        TeamCommand::Heartbeat {
            team_id,
            member_id,
            ping,
        } => TeamOp::Heartbeat {
            team_id,
            member_id,
            ping,
        },
        TeamCommand::IssueWatch {
            team_id,
            repo,
            numbers,
        } => TeamOp::IssueWatch {
            team_id,
            repo,
            numbers,
        },
        TeamCommand::IssueUnwatch {
            team_id,
            repo,
            numbers,
        } => TeamOp::IssueUnwatch {
            team_id,
            repo,
            numbers,
        },
        TeamCommand::IssueWatches { team_id } => TeamOp::IssueWatches { team_id },
    }
}

pub(super) fn team(command: TeamCommand, context: &Context) -> crate::Result<Output> {
    let op = to_op(command);
    match context.call(Request::Team, op)? {
        Response::Team {
            reply:
                Reply::Status {
                    team,
                    goal,
                    member,
                    pending_mentions,
                    open_tasks,
                    assigned_tasks,
                },
        } => {
            let role = if member.supervisor {
                "supervisor"
            } else {
                "worker"
            };
            let mut text = format!(
                "{} ({})  member: {} ({}, {})  pending mentions: {pending_mentions}  assigned tasks: {assigned_tasks}  open tasks: {open_tasks}",
                team.name, team.id, member.name, member.id, role
            );
            if let Some(goal) = &goal {
                let _ = write!(text, "\ngoal: {goal}");
            }
            Output::data(
                text,
                &serde_json::json!({
                    "team": team,
                    "goal": goal,
                    "member": member,
                    "pending_mentions": pending_mentions,
                    "open_tasks": open_tasks,
                    "assigned_tasks": assigned_tasks,
                }),
            )
        }
        Response::Team {
            reply: Reply::Registered { member },
        } => Output::data(
            format!("Registered {} as {}", member.name, member.id),
            &serde_json::json!({ "member": member }),
        ),
        Response::Team {
            reply: Reply::Roster { team, members },
        } => {
            let lines = members.iter().map(member_line).collect::<Vec<_>>();
            let text = if lines.is_empty() {
                format!("{}: no registered members", team.name)
            } else {
                lines.join("\n")
            };
            Output::data(
                text,
                &serde_json::json!({ "team": team, "members": members }),
            )
        }
        Response::Team {
            reply: Reply::GoalUpdated { team_id, goal },
        } => Output::data(
            format!(
                "Updated goal for {team_id}: {}",
                goal.as_deref().unwrap_or("(cleared)")
            ),
            &serde_json::json!({ "team_id": team_id, "goal": goal }),
        ),
        Response::Team {
            reply: Reply::Bulletin { entries, next },
        } => {
            let json = serde_json::json!({ "entries": entries, "next": next });
            if entries.is_empty() {
                return Ok(Output::data("No bulletin entries", &json)?.nothing());
            }
            Output::data(
                entries
                    .iter()
                    .map(entry_line)
                    .collect::<Vec<_>>()
                    .join("\n"),
                &json,
            )
        }
        Response::Team {
            reply: Reply::Posted { entry },
        } => Output::data(
            format!("Posted {}", entry.id),
            &serde_json::json!({ "entry": entry }),
        ),
        Response::Team {
            reply:
                Reply::Acknowledged {
                    message_id,
                    delivery_state,
                },
        } => Output::data(
            format!(
                "Acknowledged {message_id}: {}",
                delivery_state_label(delivery_state)
            ),
            &serde_json::json!({ "message_id": message_id, "delivery_state": delivery_state }),
        ),
        Response::Team {
            reply: Reply::Tasks { tasks, next_before },
        } => {
            let json = serde_json::json!({ "tasks": tasks, "next_before": next_before });
            if tasks.is_empty() {
                return Ok(Output::data("No tasks", &json)?.nothing());
            }
            Output::data(
                tasks.iter().map(task_line).collect::<Vec<_>>().join("\n"),
                &json,
            )
        }
        Response::Team {
            reply: Reply::Task { task },
        } => Output::data(task_line(&task), &serde_json::json!({ "task": task })),
        Response::Team {
            reply:
                Reply::Findings {
                    findings,
                    next_before,
                },
        } => {
            let json = serde_json::json!({ "findings": findings, "next_before": next_before });
            if findings.is_empty() {
                return Ok(Output::data("No findings", &json)?.nothing());
            }
            Output::data(
                findings
                    .iter()
                    .map(finding_line)
                    .collect::<Vec<_>>()
                    .join("\n"),
                &json,
            )
        }
        Response::Team {
            reply: Reply::Finding { finding },
        } => Output::data(
            finding_line(&finding),
            &serde_json::json!({ "finding": finding }),
        ),
        Response::Team {
            reply: Reply::Heartbeat { team_id, members },
        } => {
            let json = serde_json::json!({ "team_id": team_id, "members": members });
            let lines = members
                .iter()
                .map(|m| {
                    let rtt_str = m.rtt_ms.map(|ms| format!(" ({ms}ms)")).unwrap_or_default();
                    let detail_str = if m.details.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", m.details)
                    };
                    format!(
                        "{:<14} [{}] {:<8} {:<10}{}{}",
                        m.name, m.health, m.status, m.agent, rtt_str, detail_str
                    )
                })
                .collect::<Vec<_>>();
            let text = if lines.is_empty() {
                format!("{team_id}: no members to inspect")
            } else {
                lines.join("\n")
            };
            Output::data(text, &json)
        }
        Response::Team {
            reply: Reply::IssueWatches { watches },
        } => {
            let json = serde_json::json!({ "watches": watches });
            let text = if watches.is_empty() {
                "No repositories watched; the issue sync follows nothing".to_owned()
            } else {
                watches
                    .iter()
                    .map(|watch| {
                        if watch.number == 0 {
                            format!("{} (whole repository)", watch.repo)
                        } else {
                            format!("{}#{}", watch.repo, watch.number)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            Output::data(text, &json)
        }
        Response::Team {
            reply: Reply::IssueSync { summary },
        } => {
            let json = serde_json::json!({ "summary": summary });
            let mut text = format!(
                "Synced: {} created, {} updated, {} unchanged",
                summary.created, summary.updated, summary.unchanged
            );
            if !summary.failed.is_empty() {
                let _ = write!(text, "; failed: {}", summary.failed.join(", "));
            }
            Output::data(text, &json)
        }
        response => Err(unexpected(response)),
    }
}
