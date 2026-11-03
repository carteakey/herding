//! ACP-only agent-facing team operations. Identity is proved from the attached
//! transcript and the current selected daemon snapshot on the UI thread; the
//! worker repeats exact target membership matching before accessing the store.
use crate::HerdrWindow;
use crate::control::server::team_worker::{
    Error as WorkerError, Operation, Reply, ScopedAction, Worker,
};
use crate::control::{
    protocol::{
        Call, ErrorCode, Response,
        team::{self, BulletinEntry, DeliveryState, MemberInfo},
    },
    server::{Watches, in_window, socket},
};
use gpui::App;
use herdr_orchestrator::team_store::{Member, Post, RecipientStatus, SessionTarget, Team};
use std::{sync::mpsc, time::Duration};

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
        let (team_id, action) = match op {
            team::TeamOp::Register { .. } => {
                return Response::error(
                    ErrorCode::Unsupported,
                    "Members are added in the Team view, not by self-registration",
                );
            }
            team::TeamOp::Roster { team_id } => (team_id, ScopedAction::Roster),
            team::TeamOp::BulletinList { team_id, after } => {
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
        };
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
                move |_| match receiver.try_recv() {
                    Ok(Ok(reply)) => Some(reply_response(reply)),
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
    let chat = view.chats.pane(pane).ok_or_else(|| {
        Response::error(
            ErrorCode::Unsupported,
            "No attached ACP transcript proves this caller's session",
        )
    })?;
    if chat.target.existing.is_some() || !chat.attached() {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "Only an attached native ACP chat has a verifiable session",
        ));
    }
    let transcript = chat.session.transcript();
    let session = transcript
        .session
        .as_ref()
        .map(|session| session.id.as_str())
        .filter(|session| !session.is_empty())
        .ok_or_else(|| {
            Response::error(
                ErrorCode::Unsupported,
                "The ACP transcript has no session ID",
            )
        })?;
    if chat.target.pane_id != pane
        || !found.snapshot.panes.iter().any(|candidate| {
            candidate.pane_id == pane && candidate.workspace_id == found.workspace.workspace_id
        })
        || found.snapshot.boot_id.is_empty()
    {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "The attached ACP chat does not match the current pane snapshot",
        ));
    }
    Ok(SessionTarget {
        host: found.snapshot.boot_id.clone(),
        session: session.to_owned(),
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
        WorkerError::Store(source) => Response::error(ErrorCode::Failed, source.to_string()),
    }
}

fn member_info(member: &Member, team: &Team) -> MemberInfo {
    MemberInfo {
        id: member.member_id.clone(),
        name: member.display_name.clone(),
        supervisor: member.member_id == team.supervisor.member_id,
        agent: String::new(),
    }
}

fn entry(
    sequence: i64,
    post: Post,
    team: &Team,
    member_id: &str,
    status: Option<RecipientStatus>,
) -> BulletinEntry {
    let author_id = match post.author {
        herdr_orchestrator::team_store::Author::Member { member_id } => member_id,
        herdr_orchestrator::team_store::Author::Human { user_id } => user_id,
    };
    let author = std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .find(|member| member.member_id == author_id)
        .map_or(author_id.clone(), |member| member.display_name.clone());
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

fn reply_response(reply: Reply) -> Response {
    use team::Reply as WireReply;
    match reply {
        Reply::ScopedRoster { team } => Response::Team {
            reply: WireReply::Roster {
                team: team::TeamInfo {
                    id: team.team_id.clone(),
                    name: team.name.clone(),
                },
                members: std::iter::once(&team.supervisor)
                    .chain(team.members.iter())
                    .map(|member| member_info(member, &team))
                    .collect(),
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
            let sequence = 0;
            let author = std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .find(|member| match &post.author {
                    herdr_orchestrator::team_store::Author::Member { member_id } => {
                        member.member_id == *member_id
                    }
                    _ => false,
                })
                .map_or_else(String::new, |member| member.display_name.clone());
            Response::Team {
                reply: WireReply::Posted {
                    entry: Box::new(BulletinEntry {
                        sequence,
                        id: post.post_id,
                        member_id: match post.author {
                            herdr_orchestrator::team_store::Author::Member { member_id } => {
                                member_id
                            }
                            herdr_orchestrator::team_store::Author::Human { user_id } => user_id,
                        },
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
        _ => Response::error(ErrorCode::Failed, "Unexpected team worker reply"),
    }
}
