//! Agent-facing team registration and bulletin commands.
use super::{Args, CliError, unknown};
use herdr_control_protocol::protocol::team::{
    MAX_BULLETIN_TEXT_BYTES, MAX_MEMBER_ID_BYTES, MAX_MEMBER_NAME_BYTES, MAX_MENTIONS,
    MAX_MESSAGE_ID_BYTES, MAX_TEAM_ID_BYTES,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamCommand {
    Register {
        team_id: String,
        name: String,
    },
    Roster {
        team_id: String,
    },
    BulletinList {
        team_id: String,
        after: Option<u64>,
    },
    BulletinPost {
        team_id: String,
        reply_to: Option<String>,
        mentions: Vec<String>,
        text: String,
    },
    BulletinAck {
        team_id: String,
        message_id: String,
    },
}

fn bounded(value: String, field: &'static str, limit: usize) -> Result<String, CliError> {
    if value.is_empty() || value.len() > limit {
        return Err(CliError::InvalidValue(field));
    }
    Ok(value)
}

fn team_id(args: &mut Args) -> Result<String, CliError> {
    bounded(args.required_text("TEAM_ID")?, "TEAM_ID", MAX_TEAM_ID_BYTES)
}

fn member_id(value: String) -> Result<String, CliError> {
    bounded(value, "MEMBER_ID", MAX_MEMBER_ID_BYTES)
}

fn message_id(value: String) -> Result<String, CliError> {
    bounded(value, "MESSAGE_ID", MAX_MESSAGE_ID_BYTES)
}

pub(super) fn team(args: &mut Args) -> Result<TeamCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "team",
        what: "register|roster|bulletin",
    })?;
    match verb.to_str() {
        Some("register") => {
            let team_id = team_id(args)?;
            let name = bounded(
                args.text_option("--name")?
                    .ok_or(CliError::MissingValue("--name"))?,
                "--name",
                MAX_MEMBER_NAME_BYTES,
            )?;
            Ok(TeamCommand::Register { team_id, name })
        }
        Some("roster") => Ok(TeamCommand::Roster {
            team_id: team_id(args)?,
        }),
        Some("bulletin") => {
            let action = args.verb().ok_or(CliError::MissingArgument {
                command: "team bulletin",
                what: "list|post|ack",
            })?;
            match action.to_str() {
                Some("list") => {
                    let team_id = team_id(args)?;
                    let after = args.number("--after", 0..=u64::MAX)?;
                    Ok(TeamCommand::BulletinList { team_id, after })
                }
                Some("post") => {
                    let team_id = team_id(args)?;
                    let reply_to = args
                        .text_option("--reply-to")?
                        .map(message_id)
                        .transpose()?;
                    let mentions = args
                        .options("--mention")?
                        .into_iter()
                        .map(crate::cli::args::utf8)
                        .map(|mention| mention.and_then(member_id))
                        .collect::<Result<Vec<_>, _>>()?;
                    if mentions.len() > MAX_MENTIONS {
                        return Err(CliError::InvalidValue("--mention count"));
                    }
                    let text = bounded(
                        args.words()?.ok_or(CliError::MissingArgument {
                            command: "team bulletin post",
                            what: "TEXT",
                        })?,
                        "TEXT",
                        MAX_BULLETIN_TEXT_BYTES,
                    )?;
                    Ok(TeamCommand::BulletinPost {
                        team_id,
                        reply_to,
                        mentions,
                        text,
                    })
                }
                Some("ack") => Ok(TeamCommand::BulletinAck {
                    team_id: team_id(args)?,
                    message_id: message_id(args.required_text("MESSAGE_ID")?)?,
                }),
                _ => Err(unknown("team bulletin", action)),
            }
        }
        _ => Err(unknown("team", verb)),
    }
}

#[cfg(test)]
mod tests;
