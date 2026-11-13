//! Team goals, member self-registration, session-to-team discovery, and `@mention` resolution.

use super::{
    Author, Error, MAX_ID_BYTES, MAX_MEMBERS, MAX_MENTIONS, MAX_TEXT_BYTES, Member, Recipients,
    Result, SessionTarget, Team, TeamStore, insert_member, now_stamp, read_member, validate,
    validate_member,
};
use rusqlite::{OptionalExtension, params};
use std::collections::HashSet;

/// The host context a caller is proved against: the boot their endpoint
/// currently reports, the stable ids and boots of the connected endpoints,
/// the stable ids of every known endpoint whether connected or not, and
/// the pane ids that currently hold a live agent session on the connected
/// endpoints.
pub struct HostContext {
    pub caller_boot: String,
    pub live_hosts: Vec<String>,
    pub endpoint_ids: Vec<String>,
    pub live_panes: Vec<String>,
}

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

    pub fn delete_team(&mut self, team_id: &str) -> Result<()> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        let changed = self
            .db
            .execute("DELETE FROM teams WHERE team_id = ?1", [team_id])?;
        if changed == 0 {
            return Err(Error::TeamNotFound(team_id.to_owned()));
        }
        Ok(())
    }

    pub fn remove_member(&mut self, team_id: &str, member_id: &str) -> Result<()> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("member_id", member_id, MAX_ID_BYTES)?;
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        let supervisor_id: Option<String> = tx
            .query_row(
                "SELECT supervisor_id FROM teams WHERE team_id = ?1",
                [team_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(supervisor_id) = supervisor_id else {
            return Err(Error::TeamNotFound(team_id.to_owned()));
        };
        if supervisor_id == member_id {
            return Err(Error::Invalid {
                field: "supervisor cannot be removed",
                max: 1,
            });
        }
        // The member's in-progress or blocked work returns to the board so
        // another worker can claim it; done and cancelled work keeps its
        // outcome and status.
        tx.execute(
            "UPDATE tasks
             SET assignee_id = NULL,
                 status = CASE WHEN status IN ('in_progress', 'blocked')
                               THEN 'open' ELSE status END,
                 blocker_reason = CASE WHEN status IN ('in_progress', 'blocked')
                                       THEN NULL ELSE blocker_reason END,
                 updated_at = ?3
             WHERE team_id = ?1 AND assignee_id = ?2",
            params![team_id, member_id, stamp],
        )?;
        let changed = tx.execute(
            "DELETE FROM members WHERE team_id = ?1 AND member_id = ?2",
            params![team_id, member_id],
        )?;
        if changed == 0 {
            return Err(Error::MemberNotInTeam {
                team_id: team_id.to_owned(),
                member_id: member_id.to_owned(),
            });
        }
        // Delivery receipts belong to the member, not the post history, and
        // the row must go even though posts remain. Scope by team because
        // member IDs are only unique inside a team.
        tx.execute(
            "DELETE FROM acknowledgements
             WHERE recipient_id = ?1
               AND post_id IN (SELECT post_id FROM posts WHERE team_id = ?2)",
            params![member_id, team_id],
        )?;
        tx.execute(
            "DELETE FROM mentions
             WHERE recipient_id = ?1
               AND post_id IN (SELECT post_id FROM posts WHERE team_id = ?2)",
            params![member_id, team_id],
        )?;
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
    /// `caller_boot` is the boot the caller's endpoint currently reports, `live_hosts` names the
    /// connected endpoints' stable ids plus their boot ids, and `endpoint_ids` names every known
    /// endpoint's stable id whether connected or not: a name holder whose recorded host is the
    /// caller's own boot (a pre-stable-key row), or is neither a known endpoint's stable id nor
    /// live (a boot a restart retired), re-binds. A holder on a known endpoint's stable id keeps
    /// the name against a claim from another machine, connected or not.
    pub fn register_member(
        &mut self,
        team_id: &str,
        member_id: &str,
        display_name: &str,
        target: &SessionTarget,
        hosts: &HostContext,
    ) -> Result<Member> {
        let HostContext {
            caller_boot,
            live_hosts,
            endpoint_ids,
            live_panes,
        } = &hosts;
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
        // A resumed agent session re-proves the same workspace under a new
        // session id, and a reboot hands out NEW pane ids, so the holder's
        // pane is not an identity anchor: the anchors are the name, the
        // workspace, and the machine. The holder's host must be the
        // caller's own endpoint (the same machine across a restart, by
        // The holder's machine decides what a re-bind may cross. The
        // caller's own endpoint, or the caller's current boot (a
        // pre-stable-key row registered on this very boot): the member is
        // back on the same machine, so the re-bind crosses panes only when
        // the holder's pane is gone from the live snapshots - no pane, or
        // no agent session in it. A holder whose pane still runs an agent
        // keeps the name: any agent on this machine could otherwise take
        // the supervisor row while the real supervisor is alive in another
        // pane. A dead holder (a boot no endpoint reports, no known
        // endpoint's stable id) re-binds pane-free: the whole machine went
        // away, and every pane died with it. Any other live host stays
        // refused.
        let same_machine = &candidate.target.host;
        let holder_gone = |m: &Member| {
            m.target
                .pane
                .as_deref()
                .is_none_or(|pane| !live_panes.iter().any(|live| live == pane))
        };
        let resumed = members.iter().find(|m| {
            m.display_name.eq_ignore_ascii_case(&candidate.display_name)
                && m.target.workspace == candidate.target.workspace
                && (((&m.target.host == same_machine || &m.target.host == caller_boot)
                    && (m.target.pane == candidate.target.pane || holder_gone(m)))
                    || (!endpoint_ids.contains(&m.target.host)
                        && !live_hosts.contains(&m.target.host)))
        });
        if let Some(existing) = resumed {
            let updated = Member {
                member_id: existing.member_id.clone(),
                display_name: candidate.display_name,
                target: target.clone(),
            };
            insert_member(&tx, team_id, &updated)?;
            tx.commit()?;
            return Ok(updated);
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
    ///
    /// The recorded host is the member's endpoint's stable id, but rows from
    /// before that change name the boot id they registered under, and a
    /// daemon restart rotates boots. A caller whose session, workspace, and
    /// pane all match a row whose recorded host is their endpoint's current
    /// boot (`caller_boot`) is that member's session on a pre-stable-key
    /// row, and so is a row whose recorded host is not the stable id of ANY
    /// known endpoint (`endpoint_ids`, connected or not) and not live - a
    /// boot a restart retired. Such a row's host migrates to the caller's
    /// stable id and the member keeps their id, assignments, and mentions.
    /// A row naming a known endpoint's stable id only ever matches that
    /// endpoint exactly: a stable id survives restarts, so a merely
    /// disconnected machine's row must not be adopted by the same pane id
    /// from another machine (#40). Exact matches win over migration, the
    /// candidates are deterministic, and at most one row is written - only
    /// the one this call returns.
    pub fn find_member_team(
        &mut self,
        team_id: Option<&str>,
        target: &SessionTarget,
        hosts: &HostContext,
    ) -> Result<Option<(Team, Member)>> {
        let HostContext {
            caller_boot,
            live_hosts,
            endpoint_ids,
            ..
        } = &hosts;
        let adoptable = |host: &String| -> bool {
            host == caller_boot || (!endpoint_ids.contains(host) && !live_hosts.contains(host))
        };
        let candidates: Vec<String> = match team_id {
            Some(id) => vec![id.to_owned()],
            None => {
                // The host is matched in Rust below, where the live-key rule
                // can see the caller's boot; session, workspace, and pane
                // stay the selective anchors. Deterministic order so a
                // multi-team member always resolves the same way (#48).
                let mut stmt = self.db.prepare(
                    "SELECT DISTINCT team_id FROM members
                     WHERE session = ?1
                       AND (?2 IS NULL OR workspace = ?2)
                       AND (?3 IS NULL OR pane = ?3)
                     ORDER BY team_id",
                )?;
                let rows = stmt
                    .query_map(
                        params![target.session, target.workspace, target.pane],
                        |row| row.get(0),
                    )?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                drop(stmt);
                rows
            }
        };
        let mut teams = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            let Some(team) = self.team(candidate)? else {
                continue;
            };
            // Exact matches decide first, across every candidate, so no
            // migration is written for a row this call will not return.
            let exact = std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .find(|m| m.target == *target)
                .cloned();
            if let Some(member) = exact {
                return Ok(Some((team, member)));
            }
            let stale = std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .filter(|m| {
                    m.target.session == target.session
                        && m.target.workspace == target.workspace
                        && m.target.pane == target.pane
                })
                .find(|m| m.target.host != target.host && adoptable(&m.target.host))
                .map(|record| (team.team_id.clone(), record.clone()));
            if let Some(found) = stale {
                teams.push(found);
            }
        }
        let Some((team_id, record)) = teams.first() else {
            return Ok(None);
        };
        let team = self
            .team(team_id)?
            .ok_or(Error::TeamNotFound(team_id.to_owned()))?;
        self.migrate_member_host(team_id, &record.member_id, &target.host)?;
        Ok(Some((
            team,
            Member {
                target: target.clone(),
                ..record.clone()
            },
        )))
    }

    /// One host column's migration to the stable endpoint id; everything
    /// else about the member is untouched.
    fn migrate_member_host(&mut self, team_id: &str, member_id: &str, host: &str) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE members SET host = ?1 WHERE team_id = ?2 AND member_id = ?3",
            params![host, team_id, member_id],
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Resolves explicit mention tokens and/or `@tokens` embedded in `body` against `team`.
///
/// Supports `@team` (broadcast), `@supervisor` (team supervisor), exact `member_id`,
/// unique case-insensitive `display_name`, or a unique hexadecimal `member_id`
/// prefix as the last resort. Unknown or ambiguous tokens fail explicitly
/// rather than silently falling back to a broadcast.
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
                [] => resolve_member_prefix(&all_members, token)?,
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

/// Resolves a single member reference (`supervisor`, `member_id`, unique
/// `display_name`, or a unique hexadecimal `member_id` prefix as the last resort).
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
        [] => resolve_member_prefix(&all_members, clean),
        _ => Err(Error::AmbiguousMention(clean.to_owned())),
    }
}

/// Last-resort member resolution by unique hexadecimal ID prefix; a token that
/// does not prefix-match anyone comes back unchanged, so the caller reports
/// its usual unknown-mention error.
fn resolve_member_prefix(all_members: &[&Member], token: &str) -> Result<String> {
    let ids: Vec<String> = all_members.iter().map(|m| m.member_id.clone()).collect();
    match super::resolve_id_prefix("member", &ids, token)? {
        resolved if resolved == token => Err(Error::UnknownMention(token.to_owned())),
        resolved => Ok(resolved),
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
