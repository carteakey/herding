//! Agent-facing team registration, status, goal, bulletin, task board, and finding commands.
use super::{Args, CliError, unknown};
use sheprd_control_protocol::protocol::team::{
    ConfidenceLevel, MAX_BULLETIN_TEXT_BYTES, MAX_FINDING_ID_BYTES, MAX_MEMBER_ID_BYTES,
    MAX_MEMBER_NAME_BYTES, MAX_MENTIONS, MAX_MESSAGE_ID_BYTES, MAX_PAGE_SIZE, MAX_REPO_BYTES,
    MAX_TASK_ID_BYTES, MAX_TASK_TITLE_BYTES, MAX_TEAM_ID_BYTES,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamCommand {
    Status {
        team_id: Option<String>,
    },
    Register {
        team_id: String,
        name: String,
    },
    Roster {
        team_id: String,
    },
    GoalSet {
        team_id: String,
        goal: String,
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
    TaskList {
        team_id: String,
        before: Option<i64>,
        limit: Option<usize>,
    },
    TaskCreate {
        team_id: String,
        title: String,
        brief: String,
        assignee: Option<String>,
    },
    TaskAssign {
        team_id: String,
        task_id: String,
        assignee: String,
    },
    TaskClaim {
        team_id: String,
        task_id: String,
    },
    TaskBlock {
        team_id: String,
        task_id: String,
        reason: String,
    },
    TaskComplete {
        team_id: String,
        task_id: String,
        result: String,
    },
    TaskCancel {
        team_id: String,
        task_id: String,
    },
    FindingList {
        team_id: String,
        before: Option<i64>,
        limit: Option<usize>,
    },
    FindingAdd {
        team_id: String,
        task_id: Option<String>,
        confidence: ConfidenceLevel,
        evidence: String,
        supersedes: Option<String>,
        description: String,
    },
    Heartbeat {
        team_id: String,
        member_id: Option<String>,
        ping: bool,
    },
    IssueWatch {
        team_id: String,
        repo: String,
        numbers: Vec<i64>,
    },
    IssueUnwatch {
        team_id: String,
        repo: String,
        numbers: Vec<i64>,
    },
    IssueWatches {
        team_id: String,
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

fn task_id(value: String) -> Result<String, CliError> {
    bounded(value, "TASK_ID", MAX_TASK_ID_BYTES)
}

fn finding_id(value: String) -> Result<String, CliError> {
    bounded(value, "FINDING_ID", MAX_FINDING_ID_BYTES)
}

fn confidence(value: String) -> Result<ConfidenceLevel, CliError> {
    match value.as_str() {
        "low" => Ok(ConfidenceLevel::Low),
        "medium" => Ok(ConfidenceLevel::Medium),
        "high" => Ok(ConfidenceLevel::High),
        _ => Err(CliError::InvalidValue("--confidence")),
    }
}

pub(super) fn team(args: &mut Args) -> Result<TeamCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "team",
        what: "status|register|roster|goal|bulletin|task|finding|watch",
    })?;
    match verb.to_str() {
        Some("status") => {
            let team_id = args
                .optional_text()?
                .map(|id| bounded(id, "TEAM_ID", MAX_TEAM_ID_BYTES))
                .transpose()?;
            Ok(TeamCommand::Status { team_id })
        }
        Some("register") => {
            let name = bounded(
                args.text_option("--name")?
                    .ok_or(CliError::MissingValue("--name"))?,
                "--name",
                MAX_MEMBER_NAME_BYTES,
            )?;
            let team_id = team_id(args)?;
            Ok(TeamCommand::Register { team_id, name })
        }
        Some("roster") => Ok(TeamCommand::Roster {
            team_id: team_id(args)?,
        }),
        Some("goal") => {
            let team_id = team_id(args)?;
            let goal = bounded(
                args.words()?.ok_or(CliError::MissingArgument {
                    command: "team goal",
                    what: "TEXT",
                })?,
                "TEXT",
                MAX_BULLETIN_TEXT_BYTES,
            )?;
            Ok(TeamCommand::GoalSet { team_id, goal })
        }
        Some("bulletin") => parse_bulletin(args),
        Some("task") => parse_task(args),
        Some("finding") => parse_finding(args),
        Some("watch") | Some("unwatch") | Some("watches") => {
            parse_issue_watch(args, verb.to_str().unwrap_or_default())
        }
        Some("heartbeat") => {
            let ping = args.flag("--ping");
            let member_id = args.text_option("--member")?.map(member_id).transpose()?;
            let team_id = team_id(args)?;
            Ok(TeamCommand::Heartbeat {
                team_id,
                member_id,
                ping,
            })
        }
        _ => Err(unknown("team", verb)),
    }
}

fn parse_bulletin(args: &mut Args) -> Result<TeamCommand, CliError> {
    let action = args.verb().ok_or(CliError::MissingArgument {
        command: "team bulletin",
        what: "list|post|ack",
    })?;
    match action.to_str() {
        Some("list") => {
            let after = args.number("--after", 0..=u64::MAX)?;
            let from = args.number("--from", 0..=u64::MAX)?;
            if after.is_some() && from.is_some() {
                return Err(CliError::InvalidValue("--after and --from"));
            }
            // `--from 0` asks for the beginning of history, which is the
            // same forward walk `--after` already does.
            let after = after.or(from);
            let team_id = team_id(args)?;
            Ok(TeamCommand::BulletinList { team_id, after })
        }
        Some("post") => {
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
            let team_id = team_id(args)?;
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

fn parse_task(args: &mut Args) -> Result<TeamCommand, CliError> {
    let action = args.verb().ok_or(CliError::MissingArgument {
        command: "team task",
        what: "list|create|assign|claim|block|complete|cancel",
    })?;
    match action.to_str() {
        Some("list") => {
            let before = args.number("--before", 1..=i64::MAX)?;
            let limit = args
                .number("--limit", 1..=(MAX_PAGE_SIZE as i64))?
                .map(|limit| limit as usize);
            let team_id = team_id(args)?;
            Ok(TeamCommand::TaskList {
                team_id,
                before,
                limit,
            })
        }
        Some("create") => {
            let title = bounded(
                args.text_option("--title")?
                    .ok_or(CliError::MissingValue("--title"))?,
                "--title",
                MAX_TASK_TITLE_BYTES,
            )?;
            let brief = match args.text_option("--brief")? {
                Some(brief) if brief.len() > MAX_BULLETIN_TEXT_BYTES => {
                    return Err(CliError::InvalidValue("--brief"));
                }
                Some(brief) => brief,
                None => String::new(),
            };
            let assignee = args.text_option("--assign")?.map(member_id).transpose()?;
            let team_id = team_id(args)?;
            Ok(TeamCommand::TaskCreate {
                team_id,
                title,
                brief,
                assignee,
            })
        }
        Some("assign") => {
            let assignee = member_id(
                args.text_option("--to")?
                    .ok_or(CliError::MissingValue("--to"))?,
            )?;
            let team_id = team_id(args)?;
            let task_id = task_id(args.required_text("TASK_ID")?)?;
            Ok(TeamCommand::TaskAssign {
                team_id,
                task_id,
                assignee,
            })
        }
        Some("claim") => Ok(TeamCommand::TaskClaim {
            team_id: team_id(args)?,
            task_id: task_id(args.required_text("TASK_ID")?)?,
        }),
        Some("block") => {
            let reason = bounded(
                args.text_option("--reason")?
                    .ok_or(CliError::MissingValue("--reason"))?,
                "--reason",
                MAX_BULLETIN_TEXT_BYTES,
            )?;
            let team_id = team_id(args)?;
            let task_id = task_id(args.required_text("TASK_ID")?)?;
            Ok(TeamCommand::TaskBlock {
                team_id,
                task_id,
                reason,
            })
        }
        Some("complete" | "done") => {
            let result = bounded(
                args.text_option("--result")?
                    .ok_or(CliError::MissingValue("--result"))?,
                "--result",
                MAX_BULLETIN_TEXT_BYTES,
            )?;
            let team_id = team_id(args)?;
            let task_id = task_id(args.required_text("TASK_ID")?)?;
            Ok(TeamCommand::TaskComplete {
                team_id,
                task_id,
                result,
            })
        }
        Some("cancel") => Ok(TeamCommand::TaskCancel {
            team_id: team_id(args)?,
            task_id: task_id(args.required_text("TASK_ID")?)?,
        }),
        _ => Err(unknown("team task", action)),
    }
}

fn parse_finding(args: &mut Args) -> Result<TeamCommand, CliError> {
    let action = args.verb().ok_or(CliError::MissingArgument {
        command: "team finding",
        what: "list|add",
    })?;
    match action.to_str() {
        Some("list") => {
            let before = args.number("--before", 1..=i64::MAX)?;
            let limit = args
                .number("--limit", 1..=(MAX_PAGE_SIZE as i64))?
                .map(|limit| limit as usize);
            let team_id = team_id(args)?;
            Ok(TeamCommand::FindingList {
                team_id,
                before,
                limit,
            })
        }
        Some("add") => {
            let task_id = args.text_option("--task")?.map(task_id).transpose()?;
            let confidence = match args.text_option("--confidence")? {
                Some(value) => confidence(value)?,
                None => ConfidenceLevel::Medium,
            };
            let evidence = bounded(
                args.text_option("--evidence")?
                    .ok_or(CliError::MissingValue("--evidence"))?,
                "--evidence",
                MAX_BULLETIN_TEXT_BYTES,
            )?;
            let supersedes = args
                .text_option("--supersedes")?
                .map(finding_id)
                .transpose()?;
            let displayed = args.text_option("--description")?;
            let team_id = team_id(args)?;
            let trailing = args.words()?;
            let description = match (displayed, trailing) {
                (Some(description), None) | (None, Some(description)) => description,
                (Some(_), Some(_)) => {
                    return Err(CliError::DuplicateOption("--description"));
                }
                (None, None) => {
                    return Err(CliError::MissingArgument {
                        command: "team finding add",
                        what: "DESCRIPTION",
                    });
                }
            };
            let description = bounded(description, "DESCRIPTION", MAX_BULLETIN_TEXT_BYTES)?;
            Ok(TeamCommand::FindingAdd {
                team_id,
                task_id,
                confidence,
                evidence,
                supersedes,
                description,
            })
        }
        _ => Err(unknown("team finding", action)),
    }
}

#[cfg(test)]
mod tests;

/// `team watch`, `team unwatch`, and `team watches`: the watch set the
/// GitHub issue sync follows. Watching a repository needs only `--repo`;
/// `--issue` may repeat, and an empty `--issue` list on `watch` watches the
/// whole repository.
fn parse_issue_watch(args: &mut Args, verb: &str) -> Result<TeamCommand, CliError> {
    let repo = bounded(
        args.text_option("--repo")?
            .ok_or(CliError::MissingValue("--repo"))?,
        "--repo",
        MAX_REPO_BYTES,
    )?;
    let mut numbers = Vec::new();
    for value in args.options("--issue")? {
        for number in crate::cli::args::utf8(value)?.split(',') {
            let number = number
                .trim()
                .parse::<i64>()
                .map_err(|_| CliError::InvalidValue("--issue"))?;
            if number < 1 {
                return Err(CliError::InvalidValue("--issue"));
            }
            numbers.push(number);
        }
    }
    let team_id = team_id(args)?;
    match verb {
        "watch" => Ok(TeamCommand::IssueWatch {
            team_id,
            repo,
            numbers,
        }),
        "unwatch" => Ok(TeamCommand::IssueUnwatch {
            team_id,
            repo,
            numbers,
        }),
        _ => {
            if !numbers.is_empty() {
                return Err(CliError::InvalidValue("--issue"));
            }
            Ok(TeamCommand::IssueWatches { team_id })
        }
    }
}
