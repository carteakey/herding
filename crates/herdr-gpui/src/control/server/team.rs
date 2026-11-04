//! Agent-facing team operations. Identity is proved from an attached ACP
//! transcript, a verified CLI binding, or an active agent session in the
//! caller's daemon snapshot on the UI thread; the worker repeats exact target
//! membership matching before accessing the store. The window may be showing
//! another host, so the snapshot comes from the caller's endpoint, not the
//! selected one.
use crate::control::server::team_worker::{
    Error as WorkerError, Operation, Reply, ScopedAction, Worker,
};
use crate::control::{
    protocol::{
        Call, ErrorCode, Response,
        team::{self, ConfidenceLevel},
    },
    server::{Watches, in_window, socket},
};
use crate::{HerdrWindow, state::LiveState};
use gpui::App;
use herdr_client::protocol::AgentStatus;
use herdr_orchestrator::team_store::{
    self, Author, Confidence, Member, SessionTarget, Task, TaskStatus, Team,
};
use std::{collections::HashMap, sync::mpsc, time::Duration};

const WATCH_SECONDS: u64 = 20;

mod heartbeat;
mod replies;
use replies::{author_parts, reply_response};

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
        let (target, endpoint) = match current_identity(view, &caller) {
            Ok(identity) => identity,
            Err(response) => return response,
        };
        let pane_agents = pane_agents(view, endpoint);
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
            team::TeamOp::TaskList {
                team_id,
                before,
                limit,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES) {
                    return invalid("team ID must be nonempty and at most 128 UTF-8 bytes");
                }
                let Some(limit) = page_limit(limit) else {
                    return invalid("task list limit must be between 1 and 100");
                };
                (team_id, ScopedAction::TaskList { before, limit })
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
            team::TeamOp::FindingList {
                team_id,
                before,
                limit,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES) {
                    return invalid("team ID must be nonempty and at most 128 UTF-8 bytes");
                }
                let Some(limit) = page_limit(limit) else {
                    return invalid("finding list limit must be between 1 and 100");
                };
                (team_id, ScopedAction::FindingList { before, limit })
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
            team::TeamOp::Heartbeat {
                team_id,
                member_id,
                ping: _,
            } => {
                if !valid_id(&team_id, team::MAX_TEAM_ID_BYTES)
                    || member_id
                        .as_deref()
                        .is_some_and(|id| !valid_id(id, team::MAX_MEMBER_ID_BYTES))
                {
                    return invalid("Invalid team or member ID limit");
                }
                (team_id, ScopedAction::Heartbeat { member_id })
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

/// One prompt prepared from a durable team mutation. `signal` marks a
/// supervisor result wake: the window bridge dedupes on it so an event and
/// the periodic fallback deliver each result once.
pub(crate) struct TeamDispatch {
    pub(crate) team_id: String,
    pub(crate) pane_id: String,
    pub(crate) prompt: String,
    pub(crate) summary: String,
    pub(crate) signal: Option<String>,
}

fn notify_team_mutation(reply: &Reply, cx: &mut App) {
    let dispatches = prompts_for_scoped_reply(reply);
    for handle in crate::control::main_windows(cx) {
        let _ = handle.update(cx, |view, _, cx| {
            view.team_bridge.mark_dirty();
            if view.live.status.is_connected() {
                for dispatch in &dispatches {
                    if let Some(signal) = &dispatch.signal
                        && view.team_bridge.wake_seen(&dispatch.team_id, signal)
                    {
                        continue;
                    }
                    let outcome = view.enqueue_or_dispatch_team_prompt(
                        Some(dispatch.team_id.as_str()),
                        &dispatch.pane_id,
                        dispatch.prompt.clone(),
                        dispatch.summary.clone(),
                        cx,
                    );
                    if let Some(signal) = &dispatch.signal
                        && matches!(
                            outcome,
                            crate::team::bridge::prompts::PromptOutcome::Sent
                                | crate::team::bridge::prompts::PromptOutcome::Held
                        )
                    {
                        view.team_bridge
                            .mark_supervisor_woken(&dispatch.team_id, [signal]);
                    }
                }
            }
            cx.notify();
        });
    }
}

fn prompts_for_scoped_reply(reply: &Reply) -> Vec<TeamDispatch> {
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
                    Some(TeamDispatch {
                        team_id: team_id.clone(),
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] @{}: new bulletin from {author_name} in team {team_id} (msg {}): \"{}\". Run `{cli} bulletin ack {team_id} {}` to acknowledge and `{cli} bulletin post {team_id} --reply-to {} \"...\"` to reply.",
                            m.display_name, post.post_id, post.body, post.post_id, post.post_id
                        ),
                        summary: format!("{author_name} mentioned @{} ({pane_id})", m.display_name),
                        signal: None,
                    })
                })
                .collect()
        }
        Reply::ScopedTask { team, task, .. } if task.status == TaskStatus::Open => {
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
                    Some(TeamDispatch {
                        team_id: team_id.clone(),
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] Task assigned to {} in team {team_id}: {} (\"{}\"). Claim it with `{cli} task claim {team_id} {}` and complete it with `{cli} task complete {team_id} {} --result \"...\"`.",
                            m.display_name, task.task_id, task.title, task.task_id, task.task_id
                        ),
                        summary: format!("Assigned task \"{}\" to {} ({pane_id})", task.title, m.display_name),
                        signal: None,
                    })
                })
                .into_iter()
                .collect()
        }
        // A finished or blocked task wakes the supervisor once per stored
        // result, so coordination continues without polling. The supervisor's
        // own writes do not wake it.
        Reply::ScopedTask { team, task, actor }
            if matches!(task.status, TaskStatus::Done | TaskStatus::Blocked)
                && !matches!(actor, Author::Member { member_id }
                    if member_id == &team.supervisor.member_id) =>
        {
            supervisor_wake(team, task)
        }
        _ => Vec::new(),
    }
}

/// The supervisor wake prompt for a task the supervisor must react to.
fn supervisor_wake(team: &Team, task: &Task) -> Vec<TeamDispatch> {
    let team_id = &team.team_id;
    let Some(pane_id) = team.supervisor.target.pane.clone() else {
        return Vec::new();
    };
    let cli = crate::team::bridge::prompts::team_cli_prefix(
        &pane_id,
        team.supervisor.target.workspace.as_deref(),
    );
    let who = task
        .assignee
        .as_deref()
        .and_then(|id| {
            std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .find(|m| m.member_id == id)
        })
        .map(|m| format!(" by {}", m.display_name))
        .unwrap_or_default();
    let (situation, summary) = match task.status {
        TaskStatus::Done => (
            format!(
                "was completed{} with result: \"{}\"",
                who,
                crate::team::bridge::prompts::excerpt(task.result.as_deref().unwrap_or("none"))
            ),
            format!("Woke supervisor: task \"{}\" done", task.title),
        ),
        TaskStatus::Blocked => (
            format!(
                "is blocked{}: \"{}\"",
                who,
                task.blocker_reason
                    .as_deref()
                    .unwrap_or("no reason recorded")
            ),
            format!("Woke supervisor: task \"{}\" blocked", task.title),
        ),
        _ => return Vec::new(),
    };
    vec![TeamDispatch {
        team_id: team_id.clone(),
        pane_id,
        prompt: format!(
            "[Herdr Team] Task {} (\"{}\") {situation} in team {team_id}. Run `{cli} task list {team_id}` to review the board, take the next clear action (assign follow-up work, record findings, or ask the human only if a decision genuinely needs them), then go idle. Do not poll or re-check for updates: further results will wake you, and if the goal is complete or nothing clear remains, stop.",
            task.task_id, task.title
        ),
        summary,
        signal: Some(crate::team::bridge::prompts::wake_signal(task)),
    }]
}

/// Resolves the proven session identity and agent kind for `pane_id` in the
/// window's selected daemon.
pub(crate) fn prove_pane_session(view: &HerdrWindow, pane_id: &str) -> Option<(String, String)> {
    prove_pane_session_in(view, view.selected_endpoint, &view.live, pane_id)
}

/// [`prove_pane_session`] against the endpoint at `index`. ACP transcripts and
/// CLI bindings are projected from the selected daemon alone, so they only
/// prove a pane on that daemon; every other host is proved by the agent
/// session its own snapshot reports.
fn prove_pane_session_at(
    view: &HerdrWindow,
    index: usize,
    pane_id: &str,
) -> Option<(String, String)> {
    let live = if index == view.selected_endpoint {
        &view.live
    } else {
        &view.endpoints.get(index)?.live
    };
    prove_pane_session_in(view, index, live, pane_id)
}

fn prove_pane_session_in(
    view: &HerdrWindow,
    index: usize,
    live: &LiveState,
    pane_id: &str,
) -> Option<(String, String)> {
    let snapshot = live.snapshot.as_deref()?;
    if !live.status.is_connected() || snapshot.boot_id.is_empty() {
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
    if index == view.selected_endpoint
        && let Some(chat) = view.chats.pane(pane_id)
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
    if index == view.selected_endpoint
        && let Some(binding) = view
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

fn pane_agents(view: &HerdrWindow, endpoint: usize) -> HashMap<String, String> {
    let Some(snapshot) = live_at(view, endpoint).and_then(|live| live.snapshot.as_deref()) else {
        return HashMap::new();
    };
    snapshot
        .panes
        .iter()
        .filter_map(|pane| {
            let (_, kind) = prove_pane_session_at(view, endpoint, &pane.pane_id)?;
            Some((pane.pane_id.clone(), kind))
        })
        .collect()
}

/// The live projection of the endpoint at `index`, which the selected endpoint
/// keeps as `view.live`.
fn live_at(view: &HerdrWindow, index: usize) -> Option<&LiveState> {
    if index == view.selected_endpoint {
        Some(&view.live)
    } else {
        view.endpoints.get(index).map(|endpoint| &endpoint.live)
    }
}

fn current_identity(
    view: &HerdrWindow,
    caller: &crate::control::protocol::Caller,
) -> Result<(SessionTarget, usize), Response> {
    let pane = caller
        .pane_id
        .as_deref()
        .ok_or_else(|| invalid("Team operations require --pane"))?;
    let found = super::work::find(view, caller)?;
    let live = live_at(view, found.index).ok_or_else(|| {
        Response::error(
            ErrorCode::Unsupported,
            "The caller's host is no longer shown by this window",
        )
    })?;
    if !live.status.is_connected() {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "Team calls require the caller's connected daemon",
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
    let (session, _) = prove_pane_session_at(view, found.index, pane).ok_or_else(|| {
        Response::error(
            ErrorCode::Unsupported,
            "No active agent session or attached ACP transcript proves this caller's pane",
        )
    })?;
    Ok((
        SessionTarget {
            host: found.snapshot.boot_id.clone(),
            session,
            workspace: Some(found.workspace.workspace_id.clone()),
            pane: Some(pane.to_owned()),
        },
        found.index,
    ))
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

/// The wire's optional page size as a store limit; absent means a full page.
fn page_limit(limit: Option<usize>) -> Option<usize> {
    let size = limit.unwrap_or(team_store::MAX_PAGE_SIZE);
    (1..=team_store::MAX_PAGE_SIZE)
        .contains(&size)
        .then_some(size)
}

#[cfg(test)]
mod tests;
