//! Team goals, member self-registration, session-to-team discovery, and `@mention` resolution.

use super::{
    Author, Error, MAX_ID_BYTES, MAX_MEMBERS, MAX_MENTIONS, MAX_TEXT_BYTES, Member, Recipients,
    Result, SessionTarget, Team, TeamStore, insert_member, read_member, validate, validate_member,
};
use rusqlite::{OptionalExtension, params};
use std::collections::HashSet;

impl TeamStore {
    pub fn set_team_goal(&mut self, team_id: &str, goal: &str) -> Result<()> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        if goal.len() > MAX_TEXT_BYTES {
            return Err(Error::Invalid {
                field: "goal",
                max: MAX_TEXT_BYTES,
            });
        }
        let tx = self.db.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM teams WHERE team_id = ?1)",
            [team_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(Error::TeamNotFound(team_id.to_owned()));
        }
        if goal.trim().is_empty() {
            tx.execute("DELETE FROM team_goals WHERE team_id = ?1", [team_id])?;
        } else {
            tx.execute(
                "INSERT INTO team_goals(team_id, goal) VALUES (?1, ?2)
                 ON CONFLICT(team_id) DO UPDATE SET goal = excluded.goal",
                params![team_id, goal.trim()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn team_goal(&self, team_id: &str) -> Result<Option<String>> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        Ok(self
            .db
            .query_row(
                "SELECT goal FROM team_goals WHERE team_id = ?1",
                [team_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Registers a live session target in `team_id` or updates its display name if already present.
    pub fn register_member(
        &mut self,
        team_id: &str,
        member_id: &str,
        display_name: &str,
        target: &SessionTarget,
    ) -> Result<Member> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        let candidate = Member {
            member_id: member_id.to_owned(),
            display_name: display_name.trim().to_owned(),
            target: target.clone(),
        };
        validate_member(&candidate)?;
        let tx = self.db.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM teams WHERE team_id = ?1)",
            [team_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(Error::TeamNotFound(team_id.to_owned()));
        }
        let mut stmt = tx.prepare(
            "SELECT member_id, display_name, host, session, workspace, pane
             FROM members WHERE team_id = ?1 ORDER BY member_id",
        )?;
        let members = stmt
            .query_map([team_id], read_member)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        if let Some(existing) = members.iter().find(|m| m.target == *target) {
            if members.iter().any(|m| {
                m.member_id != existing.member_id
                    && m.display_name.eq_ignore_ascii_case(&candidate.display_name)
            }) {
                return Err(Error::DuplicateDisplayName(candidate.display_name));
            }
            let updated = Member {
                member_id: existing.member_id.clone(),
                display_name: candidate.display_name,
                target: target.clone(),
            };
            insert_member(&tx, team_id, &updated)?;
            tx.commit()?;
            return Ok(updated);
        }

        if members.iter().any(|m| m.member_id == candidate.member_id) {
            return Err(Error::MemberTargetChanged {
                team_id: team_id.to_owned(),
                member_id: candidate.member_id,
            });
        }
        if members
            .iter()
            .any(|m| m.display_name.eq_ignore_ascii_case(&candidate.display_name))
        {
            return Err(Error::DuplicateDisplayName(candidate.display_name));
        }
        if members.len() >= MAX_MEMBERS {
            return Err(Error::TooManyMembers { max: MAX_MEMBERS });
        }
        insert_member(&tx, team_id, &candidate)?;
        tx.commit()?;
        Ok(candidate)
    }

    /// Finds the team and member record matching a verified `SessionTarget`.
    pub fn find_member_team(
        &self,
        team_id: Option<&str>,
        target: &SessionTarget,
    ) -> Result<Option<(Team, Member)>> {
        let team_id = match team_id {
            Some(id) => Some(id.to_owned()),
            None => self
                .db
                .query_row(
                    "SELECT team_id FROM members
                     WHERE host = ?1 AND session = ?2
                       AND (?3 IS NULL OR workspace = ?3)
                       AND (?4 IS NULL OR pane = ?4)
                     ORDER BY team_id LIMIT 1",
                    params![target.host, target.session, target.workspace, target.pane],
                    |row| row.get(0),
                )
                .optional()?,
        };
        let Some(team_id) = team_id else {
            return Ok(None);
        };
        let Some(team) = self.team(&team_id)? else {
            return Ok(None);
        };
        let member = std::iter::once(&team.supervisor)
            .chain(team.members.iter())
            .find(|m| m.target == *target)
            .cloned();
        Ok(member.map(|m| (team, m)))
    }
}

/// Resolves explicit mention tokens and/or `@tokens` embedded in `body` against `team`.
///
/// Supports `@team` (broadcast), `@supervisor` (team supervisor), exact `member_id`,
/// or unique case-insensitive `display_name`. Unknown or ambiguous tokens fail
/// explicitly rather than silently falling back to a broadcast.
pub fn resolve_recipients(
    team: &Team,
    author: &Author,
    explicit: &[String],
    body: &str,
) -> Result<Recipients> {
    let raw_tokens = if !explicit.is_empty() {
        explicit.to_vec()
    } else {
        extract_at_tokens(body)
    };
    if raw_tokens.is_empty() {
        return Ok(Recipients::All);
    }
    if raw_tokens.len() > MAX_MENTIONS {
        return Err(Error::Invalid {
            field: "mentions",
            max: MAX_MENTIONS,
        });
    }
    let author_member_id = match author {
        Author::Member { member_id } => Some(member_id.as_str()),
        Author::Human { .. } => None,
    };
    let all_members: Vec<&Member> = std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .collect();
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();

    for raw in &raw_tokens {
        let token = raw.trim().trim_start_matches('@');
        if token.is_empty() {
            return Err(Error::Invalid {
                field: "mention",
                max: MAX_ID_BYTES,
            });
        }
        if token.eq_ignore_ascii_case("team") || token.eq_ignore_ascii_case("all") {
            return Ok(Recipients::All);
        }
        let target_id = if token.eq_ignore_ascii_case("supervisor") {
            team.supervisor.member_id.clone()
        } else if let Some(exact) = all_members.iter().find(|m| m.member_id == token) {
            exact.member_id.clone()
        } else {
            let matches: Vec<&Member> = all_members
                .iter()
                .copied()
                .filter(|m| m.display_name.eq_ignore_ascii_case(token))
                .collect();
            match matches.as_slice() {
                [single] => single.member_id.clone(),
                [] => return Err(Error::UnknownMention(token.to_owned())),
                _ => return Err(Error::AmbiguousMention(token.to_owned())),
            }
        };
        if !explicit.is_empty() && !seen.insert(target_id.clone()) {
            return Err(Error::DuplicateRecipient(target_id));
        }
        if author_member_id != Some(target_id.as_str()) && !resolved.contains(&target_id) {
            resolved.push(target_id);
        }
    }
    Ok(Recipients::Explicit(resolved))
}

/// Resolves a single member reference (`supervisor`, `member_id`, or unique `display_name`).
pub fn resolve_member_ref(team: &Team, token: &str) -> Result<String> {
    let clean = token.trim().trim_start_matches('@');
    if clean.is_empty() {
        return Err(Error::Invalid {
            field: "member",
            max: MAX_ID_BYTES,
        });
    }
    if clean.eq_ignore_ascii_case("supervisor") {
        return Ok(team.supervisor.member_id.clone());
    }
    let all_members: Vec<&Member> = std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .collect();
    if let Some(exact) = all_members.iter().find(|m| m.member_id == clean) {
        return Ok(exact.member_id.clone());
    }
    let matches: Vec<&Member> = all_members
        .iter()
        .copied()
        .filter(|m| m.display_name.eq_ignore_ascii_case(clean))
        .collect();
    match matches.as_slice() {
        [single] => Ok(single.member_id.clone()),
        [] => Err(Error::UnknownMention(clean.to_owned())),
        _ => Err(Error::AmbiguousMention(clean.to_owned())),
    }
}

/// Extracts `@token` mentions from `body`, ignoring email-like `user@domain` substrings.
pub fn extract_at_tokens(body: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<(usize, char)> = body.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (_, ch) = chars[i];
        let preceded_by_word = i > 0 && {
            let prev = chars[i - 1].1;
            prev.is_alphanumeric() || prev == '_' || prev == '@'
        };
        if ch == '@' && !preceded_by_word {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() {
                let c = chars[end].1;
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                let byte_start = chars[start].0;
                let byte_end = if end < chars.len() {
                    chars[end].0
                } else {
                    body.len()
                };
                let token = &body[byte_start..byte_end];
                if !tokens
                    .iter()
                    .any(|t: &String| t.eq_ignore_ascii_case(token))
                {
                    tokens.push(token.to_owned());
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    tokens
}
