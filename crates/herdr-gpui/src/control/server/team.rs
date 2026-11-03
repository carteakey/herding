//! Agent-facing team operations. Identity is proved from an attached ACP
//! transcript, a verified CLI binding, or an active agent session in the
//! current selected daemon snapshot on the UI thread; the worker repeats exact
//! target membership matching before accessing the store.
use crate::HerdrWindow;
use crate::control::server::team_worker::{
    Error as WorkerError, Operation, Reply, ScopedAction, Worker,
};
use crate::control::{
    protocol::{
        Call, ErrorCode, Response,
        team::{
            self, BulletinEntry, ConfidenceLevel, DeliveryState, FindingInfo, MemberInfo, TaskInfo,
            TaskState,
        },
    },
    server::{Watches, in_window, socket},
};
use gpui::App;
use herdr_client::protocol::AgentStatus;
use herdr_orchestrator::team_store::{
    self, Author, Confidence, Finding, Member, Post, RecipientStatus, SessionTarget, Task,
    TaskStatus, Team,
};
use std::{collections::HashMap, sync::mpsc, time::Duration};

const WATCH_SECONDS: u64 = 20;

pub(super) fn serve(
    call: Call<team::TeamOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    worker: Option<&Worker>,
    cx: &mut App,
) {
    let Some(worker) = worker else {
        incoming.respond(Response::error(
            ErrorCode::Unsupported,
            "Team storage is unavailable",
        ));
        return;
    };
    let Call { caller, op } = call;
    let mut incoming = Some(incoming);
    let response = in_window(&caller, cx, |view, _, window_cx| {
        let target = match current_identity(view, &caller) {
            Ok(target) => target,
            Err(response) => return response,
        };
        let pane_agents = pane_agents(view);
        let (team_id, action) = match op {
            team::TeamOp::Status { team_id } => {
                if let Some(id) = team_id.as_deref()
                    && !valid_id(id, team::MAX_TEAM_ID_BYTES)
                {
                    return invalid("team ID must be nonempty and at most 128 UTF-8 bytes");
                }
                (
                    team_id.clone().unwrap_or_default(),
                    ScopedAction::Status { team_id },
                )
            }
            team::TeamOp::Register { team_id, name } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&name, team::MAX_MEMBER_NAME_BYTES)
                {
                    return invalid(
                        "team ID and member name must be nonempty and within size limits",
                    );
                }
                (team_id, ScopedAction::Register { name })
            }
            team::TeamOp::Roster { team_id } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES) {
                    return invalid("team ID must be nonempty and at most 128 UTF-8 bytes");
                }
                (team_id, ScopedAction::Roster)
            }
            team::TeamOp::GoalSet { team_id, goal } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&goal, team::MAX_BULLETIN_TEXT_BYTES)
                {
                    return invalid("team ID and goal must be nonempty and within size limits");
                }
                (team_id, ScopedAction::GoalSet { goal })
            }
            team::TeamOp::BulletinList { team_id, after } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES) {
                    return invalid("team ID must be nonempty and at most 128 UTF-8 bytes");
                }
                let after = match after.map(i64::try_from).transpose() {
                    Ok(after) => after.unwrap_or(0),
                    Err(_) => return invalid("cursor exceeds the supported sequence range"),
                };
                (
                    team_id,
                    ScopedAction::BulletinList {
                        after,
                        limit: team::MAX_PAGE_SIZE,
                    },
                )
            }
            team::TeamOp::BulletinPost {
                team_id,
                reply_to,
                mentions,
                text,
            } => {
                if let Some(response) =
                    validate_post(&team_id, reply_to.as_deref(), &mentions, &text)
                {
                    return response;
                }
                (
                    team_id,
                    ScopedAction::BulletinPost {
                        post_id: uuid::Uuid::new_v4().to_string(),
                        reply_to,
                        text,
                        mentions,
                    },
                )
            }
            team::TeamOp::BulletinAck {
                team_id,
                message_id,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&message_id, team::MAX_MESSAGE_ID_BYTES)
                {
                    return invalid(
                        "team and message IDs must be nonempty and at most 128 UTF-8 bytes",
                    );
                }
                (team_id, ScopedAction::BulletinAck { message_id })
            }
            team::TeamOp::TaskList { team_id } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES) {
                    return invalid("team ID must be nonempty and at most 128 UTF-8 bytes");
                }
                (team_id, ScopedAction::TaskList)
            }
            team::TeamOp::TaskCreate {
                team_id,
                title,
                brief,
                assignee,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&title, team::MAX_TASK_TITLE_BYTES)
                    || brief.len() > team::MAX_BULLETIN_TEXT_BYTES
                    || assignee
                        .as_deref()
                        .is_some_and(|a| !valid_id(a, team::MAX_MEMBER_ID_BYTES))
                {
                    return invalid("Invalid task create field or size limit");
                }
                (
                    team_id,
                    ScopedAction::TaskCreate {
                        task_id: uuid::Uuid::new_v4().to_string(),
                        title,
                        brief,
                        assignee,
                    },
                )
            }
            team::TeamOp::TaskAssign {
                team_id,
                task_id,
                assignee,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&task_id, team::MAX_TASK_ID_BYTES)
                    || !valid_id(&assignee, team::MAX_MEMBER_ID_BYTES)
                {
                    return invalid("Invalid task assign field or size limit");
                }
                (team_id, ScopedAction::TaskAssign { task_id, assignee })
            }
            team::TeamOp::TaskClaim { team_id, task_id } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&task_id, team::MAX_TASK_ID_BYTES)
                {
                    return invalid("Invalid task claim field or size limit");
                }
                (team_id, ScopedAction::TaskClaim { task_id })
            }
            team::TeamOp::TaskBlock {
                team_id,
                task_id,
                reason,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&task_id, team::MAX_TASK_ID_BYTES)
                    || !valid_id(&reason, team::MAX_BULLETIN_TEXT_BYTES)
                {
                    return invalid("Invalid task block field or size limit");
                }
                (team_id, ScopedAction::TaskBlock { task_id, reason })
            }
            team::TeamOp::TaskComplete {
                team_id,
                task_id,
                result,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&task_id, team::MAX_TASK_ID_BYTES)
                    || !valid_id(&result, team::MAX_BULLETIN_TEXT_BYTES)
                {
                    return invalid("Invalid task complete field or size limit");
                }
                (team_id, ScopedAction::TaskComplete { task_id, result })
            }
            team::TeamOp::TaskCancel { team_id, task_id } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || !valid_id(&task_id, team::MAX_TASK_ID_BYTES)
                {
                    return invalid("Invalid task cancel field or size limit");
                }
                (team_id, ScopedAction::TaskCancel { task_id })
            }
            team::TeamOp::FindingList { team_id } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES) {
                    return invalid("team ID must be nonempty and at most 128 UTF-8 bytes");
                }
                (team_id, ScopedAction::FindingList)
            }
            team::TeamOp::FindingAdd {
                team_id,
                task_id,
                description,
                evidence,
                confidence,
                supersedes,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || task_id
                        .as_deref()
                        .is_some_and(|id| !valid_id(id, team::MAX_TASK_ID_BYTES))
                    || !valid_id(&description, team::MAX_BULLETIN_TEXT_BYTES)
                    || !valid_id(&evidence, team::MAX_BULLETIN_TEXT_BYTES)
                    || supersedes
                        .as_deref()
                        .is_some_and(|id| !valid_id(id, team::MAX_FINDING_ID_BYTES))
                {
                    return invalid("Invalid finding field or size limit");
                }
                let confidence = match confidence {
                    ConfidenceLevel::Low => Confidence::Low,
                    ConfidenceLevel::Medium => Confidence::Medium,
                    ConfidenceLevel::High => Confidence::High,
                };
                (
                    team_id,
                    ScopedAction::FindingAdd {
                        finding_id: uuid::Uuid::new_v4().to_string(),
                        task_id,
                        description,
                        evidence,
                        confidence,
                        supersedes,
                    },
                )
            }
        };
        let is_mutation = matches!(
            &action,
            ScopedAction::Register { .. }
                | ScopedAction::GoalSet { .. }
                | ScopedAction::BulletinPost { .. }
                | ScopedAction::BulletinAck { .. }
                | ScopedAction::TaskCreate { .. }
                | ScopedAction::TaskAssign { .. }
                | ScopedAction::TaskClaim { .. }
                | ScopedAction::TaskBlock { .. }
                | ScopedAction::TaskComplete { .. }
                | ScopedAction::TaskCancel { .. }
                | ScopedAction::FindingAdd { .. }
        );
        let op = Operation::Scoped {
            team_id,
            caller: target,
            action,
        };
        // Saturation and a stopped worker reach the caller through the watch
        // registered inside, so the result carries nothing more to report.
        let _ = worker.submit(op, |receiver| {
            let Some(incoming) = incoming.take() else {
                return false;
            };
            watches.add(
                incoming,
                Duration::from_secs(WATCH_SECONDS),
                move |app_cx| match receiver.try_recv() {
                    Ok(Ok(reply)) => {
                        if is_mutation {
                            notify_team_mutation(&reply, app_cx);
                        }
                        Some(reply_response(reply, &pane_agents))
                    }
                    Ok(Err(error)) => Some(worker_error(error)),
                    Err(mpsc::TryRecvError::Disconnected) => Some(Response::error(
                        ErrorCode::Failed,
                        "Team worker stopped before replying",
                    )),
                    Err(mpsc::TryRecvError::Empty) => None,
                },
                Response::error(
                    ErrorCode::Timeout,
                    "Team operation timed out; a posted result may be uncertain",
                ),
                window_cx,
            );
            true
        });
        // The response is now parked (or answered by Watches on capacity).
        Response::Done {
            message: "Team request queued".into(),
        }
    });
    // `in_window`'s return is only used for immediate refusals. A queued
    // request has already consumed `incoming`; no second response is sent.
    if let Some(incoming) = incoming {
        incoming.respond(response);
    }
}

fn notify_team_mutation(reply: &Reply, cx: &mut App) {
    let dispatches = prompts_for_scoped_reply(reply);
    for handle in crate::control::main_windows(cx) {
        let _ = handle.update(cx, |view, _, cx| {
            view.team_bridge.mark_dirty();
            if view.live.status.is_connected() {
                for (pane_id, prompt, summary) in &dispatches {
                    view.enqueue_or_dispatch_team_prompt(
                        pane_id,
                        prompt.clone(),
                        summary.clone(),
                        cx,
                    );
                }
            }
            cx.notify();
        });
    }
}

fn prompts_for_scoped_reply(reply: &Reply) -> Vec<(String, String, String)> {
    match reply {
        Reply::ScopedPosted { team, post } => {
            let team_id = &team.team_id;
            let (_, author_name) = author_parts(team, &post.author);
            std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .filter(|m| post.mentions.contains(&m.member_id))
                .filter_map(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = crate::team::bridge::prompts::team_cli_prefix(
                        &pane_id,
                        m.target.workspace.as_deref(),
                    );
                    Some((
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] @{}: new bulletin from {author_name} in team {team_id} (msg {}): \"{}\". Run `{cli} bulletin ack {team_id} {}` to acknowledge and `{cli} bulletin post {team_id} --reply-to {} \"...\"` to reply.",
                            m.display_name, post.post_id, post.body, post.post_id, post.post_id
                        ),
                        format!("{author_name} mentioned @{} ({pane_id})", m.display_name),
                    ))
                })
                .collect()
        }
        Reply::ScopedTask { team, task } if task.status == TaskStatus::Open => {
            let team_id = &team.team_id;
            let Some(assignee_id) = task.assignee.as_deref() else {
                return Vec::new();
            };
            std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .find(|m| m.member_id == assignee_id)
                .and_then(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = crate::team::bridge::prompts::team_cli_prefix(
                        &pane_id,
                        m.target.workspace.as_deref(),
                    );
                    Some((
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] Task assigned to {} in team {team_id}: {} (\"{}\"). Claim it with `{cli} task claim {team_id} {}` and complete it with `{cli} task complete {team_id} {} --result \"...\"`.",
                            m.display_name, task.task_id, task.title, task.task_id, task.task_id
                        ),
                        format!("Assigned task \"{}\" to {} ({pane_id})", task.title, m.display_name),
                    ))
                })
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Resolves the proven session identity and agent kind for `pane_id` in `view`.
pub(crate) fn prove_pane_session(view: &HerdrWindow, pane_id: &str) -> Option<(String, String)> {
    let snapshot = view.live.snapshot.as_deref()?;
    if !view.live.status.is_connected() || snapshot.boot_id.is_empty() {
        return None;
    }
    let pane = snapshot.panes.iter().find(|p| p.pane_id == pane_id)?;
    let agent_entry = snapshot.agents.iter().find(|a| a.pane_id == pane_id);
    let agent_label = agent_entry
        .and_then(|a| a.display_agent.as_deref().or(a.agent.as_deref()))
        .unwrap_or_default()
        .trim()
        .to_owned();

    // 1. Native attached ACP chat transcript
    if let Some(chat) = view.chats.pane(pane_id)
        && chat.target.existing.is_none()
        && chat.attached()
        && chat.target.pane_id == pane_id
        && let Some(session) = chat
            .session
            .transcript()
            .session
            .as_ref()
            .map(|s| s.id.as_str())
            .filter(|id| !id.is_empty())
    {
        let kind = if agent_label.is_empty() {
            "acp".to_owned()
        } else {
            agent_label
        };
        return Some((session.to_owned(), kind));
    }

    // 2. Verified existing CLI conversation binding (Codex / Claude process + session ID)
    if let Some(binding) = view
        .chats
        .existing
        .bindings()
        .iter()
        .find(|b| b.pane_id == pane_id)
    {
        let kind = if agent_label.is_empty() {
            binding.agent.name().to_owned()
        } else {
            agent_label
        };
        return Some((
            format!(
                "cli:{}:{}:{}",
                binding.session_id, binding.pid, binding.started_ticks
            ),
            kind,
        ));
    }

    // 3. Active terminal agent session reported by the daemon snapshot (ZCode, OpenCode, Codex, Claude, etc.)
    if let Some(agent) = agent_entry {
        let kind = agent
            .agent
            .as_deref()
            .or(agent.display_agent.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(kind) = kind
            && agent.agent_status != AgentStatus::Unknown
        {
            let label = if agent_label.is_empty() {
                kind.to_owned()
            } else {
                agent_label
            };
            return Some((
                format!(
                    "agent:{}:{}:{}",
                    kind.to_ascii_lowercase(),
                    pane.workspace_id,
                    pane.pane_id
                ),
                label,
            ));
        }
    }

    None
}

fn pane_agents(view: &HerdrWindow) -> HashMap<String, String> {
    let Some(snapshot) = view.live.snapshot.as_deref() else {
        return HashMap::new();
    };
    snapshot
        .panes
        .iter()
        .filter_map(|pane| {
            let (_, kind) = prove_pane_session(view, &pane.pane_id)?;
            Some((pane.pane_id.clone(), kind))
        })
        .collect()
}

fn current_identity(
    view: &HerdrWindow,
    caller: &crate::control::protocol::Caller,
) -> Result<SessionTarget, Response> {
    let pane = caller
        .pane_id
        .as_deref()
        .ok_or_else(|| invalid("Team operations require --pane"))?;
    let found = super::work::find(view, caller)?;
    if found.index != view.selected_endpoint || !view.live.status.is_connected() {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "Team calls require the caller's connected selected daemon",
        ));
    }
    if caller.workspace_id.as_deref() != Some(found.workspace.workspace_id.as_str()) {
        return Err(invalid("Team calls require an exact caller workspace"));
    }
    if !found.snapshot.panes.iter().any(|candidate| {
        candidate.pane_id == pane && candidate.workspace_id == found.workspace.workspace_id
    }) || found.snapshot.boot_id.is_empty()
    {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "The caller pane does not match the current pane snapshot",
        ));
    }
    let (session, _) = prove_pane_session(view, pane).ok_or_else(|| {
        Response::error(
            ErrorCode::Unsupported,
            "No active agent session or attached ACP transcript proves this caller's pane",
        )
    })?;
    Ok(SessionTarget {
        host: found.snapshot.boot_id.clone(),
        session,
        workspace: Some(found.workspace.workspace_id.clone()),
        pane: Some(pane.to_owned()),
    })
}

fn validate_post(
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

fn valid_id(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max
}
fn invalid(message: &str) -> Response {
    Response::error(ErrorCode::InvalidRequest, message)
}

fn worker_error(error: WorkerError) -> Response {
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
                | team_store::Error::NotMentioned { .. }
                | team_store::Error::TaskUnavailable(_)
                | team_store::Error::InvalidTaskTransition { .. } => ErrorCode::InvalidRequest,
                _ => ErrorCode::Failed,
            };
            Response::error(code, source.to_string())
        }
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

fn member_display_name(team: &Team, id: &str) -> String {
    std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .find(|m| m.member_id == id)
        .map_or_else(|| id.to_owned(), |m| m.display_name.clone())
}

fn author_parts(team: &Team, author: &Author) -> (String, String) {
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

fn reply_response(reply: Reply, pane_agents: &HashMap<String, String>) -> Response {
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
        Reply::ScopedTasks { team, tasks } => Response::Team {
            reply: WireReply::Tasks {
                tasks: tasks
                    .into_iter()
                    .map(|task| task_info(task, &team))
                    .collect(),
            },
        },
        Reply::ScopedTask { team, task } => Response::Team {
            reply: WireReply::Task {
                task: Box::new(task_info(task, &team)),
            },
        },
        Reply::ScopedFindings { team, findings } => Response::Team {
            reply: WireReply::Findings {
                findings: findings
                    .into_iter()
                    .map(|finding| finding_info(finding, &team))
                    .collect(),
            },
        },
        Reply::ScopedFinding { team, finding } => Response::Team {
            reply: WireReply::Finding {
                finding: Box::new(finding_info(finding, &team)),
            },
        },
        _ => Response::error(ErrorCode::Failed, "Unexpected team worker reply"),
    }
}
