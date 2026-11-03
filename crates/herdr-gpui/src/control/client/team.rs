//! Agent-facing team and bulletin commands.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::TeamCommand,
    control::protocol::{
        Request, Response,
        team::{Reply, TeamOp},
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

fn delivery_state_label(state: crate::control::protocol::team::DeliveryState) -> &'static str {
    use crate::control::protocol::team::DeliveryState;
    match state {
        DeliveryState::NotMentioned => "not_mentioned",
        DeliveryState::Pending => "pending",
        DeliveryState::Acknowledged => "acknowledged",
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

pub(super) fn team(command: TeamCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        TeamCommand::Register { team_id, name } => TeamOp::Register { team_id, name },
        TeamCommand::Roster { team_id } => TeamOp::Roster { team_id },
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
    };
    match context.call(Request::Team, op)? {
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
        response => Err(unexpected(response)),
    }
}
