//! Agent-facing team operations. Identity is proved from an attached ACP
//! transcript, a verified CLI binding, or an active agent session in the
//! caller's daemon snapshot on the UI thread; the worker repeats exact target
//! membership matching before accessing the store. The window may be showing
//! another host, so the snapshot comes from the caller's endpoint, not the
//! selected one.
use crate::control::{
    protocol::{
        Call, ErrorCode, Response,
        team::{self, ConfidenceLevel},
    },
    server::{
        Watches, in_window, socket,
        team_worker::{Operation, ScopedAction, Worker},
    },
};
use gpui::App;
use herdr_orchestrator::team_store::Confidence;
use std::{sync::mpsc, time::Duration};

mod dispatch;
mod heartbeat;
mod identity;
mod projection;

use dispatch::notify_team_mutation;
use identity::{
    PeerBindings, agent_lineage, check_peer, current_identity, pane_agents, process_info,
};
pub(crate) use identity::{ProcessIdentity, prove_pane_session_at};
use projection::{invalid, page_limit, reply_response, valid_id, validate_post, worker_error};

const WATCH_SECONDS: u64 = 20;

/// The UI process's own user, so a peer credential can be classified as a
/// same-user caller or the tunnel's sshd; platforms without a user id
/// never classify.
#[cfg(unix)]
fn local_uid() -> Option<u32> {
    Some(nix::unistd::geteuid().as_raw())
}

#[cfg(not(unix))]
fn local_uid() -> Option<u32> {
    None
}

pub(super) fn serve(
    call: Call<team::TeamOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    worker: Option<&Worker>,
    peers: &mut PeerBindings,
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
    let peer = incoming.peer;
    let peer_auth = incoming.peer_auth;
    let mut incoming = Some(incoming);
    let registering = matches!(op, team::TeamOp::Register { .. });
    let response = in_window(&caller, cx, |view, _, window_cx| {
        let (target, endpoint) = match current_identity(view, &caller) {
            Ok(identity) => identity,
            Err(response) => return response,
        };
        // Registration records the pane's agent process for every later
        // request: the CLI call itself is a fresh process each time, so the
        // binding anchors on the agent that spawns those calls, which the
        // pane's discovered process bindings already name. Every other call
        // must come from that process tree, on top of the proof above.
        if registering {
            if peer_auth && peer.is_none() {
                return Response::error(
                    ErrorCode::Unsupported,
                    "The connecting process's credentials could not be read; team registration needs a Unix socket",
                );
            }
            let discovered = view
                .chats
                .existing
                .bindings()
                .iter()
                .find(|binding| binding.pane_id == caller.pane_id.as_deref().unwrap_or_default())
                .map(|binding| ProcessIdentity {
                    pid: binding.pid,
                    started_ticks: Some(binding.started_ticks),
                });
            // Without a discovered binding, an agent-session proof still
            // names the agent kind, and the caller's own ancestry carries
            // that agent: anchor it from /proc directly.
            let agent = discovered.or_else(|| {
                let kind = target.session.strip_prefix("agent:")?.split(':').next()?;
                (!kind.is_empty()).then_some(()).and_then(|()| {
                    let pid = peer.as_ref()?.pid?;
                    agent_lineage(pid, kind)
                })
            });
            peers.insert(target.clone(), agent);
        } else if let Err(response) =
            check_peer(peers, &target, peer, peer_auth, local_uid(), &process_info)
        {
            return response;
        }
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
                    // `None` means the newest page; `Some(cursor)` pages forward.
                    Ok(after) => after,
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

#[cfg(test)]
mod tests;
