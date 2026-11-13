//! Team goals, member self-registration, session-to-team discovery, and `@mention` resolution.

use super::{
    Acknowledgement, Author, Error, MAX_ID_BYTES, MAX_MEMBERS, MAX_MENTIONS, MAX_PAGE_SIZE,
    MAX_TEXT_BYTES, Member, PENDING_MENTIONS_SQL, Post, RecipientStatus, Recipients, Result,
    STATUS_BATCH_CHUNK, SessionTarget, Team, TeamStore, identity, insert_member, now_stamp,
    parse_time, read_member, validate, validate_member,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{OptionalExtension, params};
use std::collections::HashSet;

/// How long a tombstoned member row waits for its own pane to come back
/// before a re-registration mints a new identity instead (task 79a12d7b).
const RESTORE_WINDOW_DAYS: i64 = 7;

/// SQL of [`TeamStore::posts`]: the page is selected before the mentions join
/// so `limit` counts posts rather than join rows.
const POSTS_PAGE_SQL: &str = "WITH page AS (
    SELECT sequence,post_id,team_id,author_kind,author_id,reply_to,body,created_at
    FROM posts WHERE team_id=?1 AND sequence>?2
    ORDER BY sequence LIMIT ?3
)
SELECT page.sequence,page.post_id,page.team_id,page.author_kind,page.author_id,
       page.reply_to,page.body,page.created_at,m.recipient_id
FROM page LEFT JOIN mentions m ON m.post_id=page.post_id
WHERE m.recipient_id IS NULL OR m.recipient_id IN (
    SELECT member_id FROM members
    WHERE team_id=page.team_id AND removed_at IS NULL
)
ORDER BY page.sequence,m.recipient_id";
/// SQL of [`TeamStore::latest_posts`], the same page read from its newest end
/// so a team's older history is never touched.
const POSTS_LATEST_PAGE_SQL: &str = "WITH page AS (
    SELECT sequence,post_id,team_id,author_kind,author_id,reply_to,body,created_at
    FROM posts WHERE team_id=?1 AND sequence<?2
    ORDER BY sequence DESC LIMIT ?3
)
SELECT page.sequence,page.post_id,page.team_id,page.author_kind,page.author_id,
       page.reply_to,page.body,page.created_at,m.recipient_id
FROM page LEFT JOIN mentions m ON m.post_id=page.post_id
WHERE m.recipient_id IS NULL OR m.recipient_id IN (
    SELECT member_id FROM members
    WHERE team_id=page.team_id AND removed_at IS NULL
)
ORDER BY page.sequence DESC,m.recipient_id";

/// Which side of a sequence [`TeamStore::page`] takes its `limit` posts from.
pub(crate) enum Window {
    After(i64),
    Before(i64),
}

pub(super) fn read_member_with_removal(
    r: &rusqlite::Row<'_>,
) -> rusqlite::Result<(Member, Option<String>)> {
    let removed_at: Option<String> = r.get(6)?;
    Ok((read_member(r)?, removed_at))
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
        // outcome and status. The reset task ids come back in the audit
        // trail, so a restore within the grace window can re-link them.
        let reset: Vec<String> = {
            let mut statement = tx.prepare(
                "SELECT task_id FROM tasks
                 WHERE team_id = ?1 AND assignee_id = ?2
                   AND status IN ('in_progress', 'blocked')",
            )?;
            statement
                .query_map(params![team_id, member_id], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
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
        let departing = identity::member_before_removal(&tx, team_id, member_id)?;
        // Tombstone, not delete: the row stays with removed_at set, so a
        // re-registration within the grace window restores the same member
        // identity instead of minting a new one (task 79a12d7b).
        let changed = tx.execute(
            "UPDATE members SET removed_at = ?3 WHERE team_id = ?1 AND member_id = ?2
             AND removed_at IS NULL",
            params![team_id, member_id, stamp],
        )?;
        if changed == 0 {
            return Err(Error::MemberNotInTeam {
                team_id: team_id.to_owned(),
                member_id: member_id.to_owned(),
            });
        }
        if let Some(member) = departing {
            let details = (!reset.is_empty()).then(|| reset.join(","));
            identity::record(&tx, "removed", team_id, &member, details.as_deref())?;
        }
        // Delivery receipts stay with the tombstoned row: a restore within
        // the grace window brings the member's mention state back with them.
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
    /// `live_hosts` names the boot ids the caller's endpoints currently report; a name holder whose
    /// recorded boot is absent from the set was left behind by a daemon restart and re-binds.
    pub fn register_member(
        &mut self,
        team_id: &str,
        member_id: &str,
        display_name: &str,
        target: &SessionTarget,
        live_hosts: &[String],
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
        // Live rows drive the matching and vetoes; tombstoned rows from a
        // recent removal wait to be restored by their own pane coming back.
        let mut stmt = tx.prepare(
            "SELECT member_id, display_name, host, session, workspace, pane, removed_at
             FROM members WHERE team_id = ?1 ORDER BY member_id",
        )?;
        let rows = stmt
            .query_map([team_id], read_member_with_removal)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        let mut members = Vec::new();
        let mut tombstoned = Vec::new();
        for (member, removed_at) in rows {
            match removed_at {
                Some(_) => tombstoned.push(member),
                None => members.push(member),
            }
        }

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
            identity::record(&tx, "registered", team_id, &updated, None)?;
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
        // A resumed agent session re-proves the same pane under a new
        // session id, so the exact-target match above misses it and the
        // member's own name would otherwise be squatted by its dead
        // predecessor forever. One pane hosts one live agent session, and
        // the candidate target was proved against that pane's snapshot, so
        // re-bind the name holder in the same workspace and pane:
        // member_id preserved, so supervisor status, assignments, and
        // mention history stay intact. A daemon restart rotates the boot
        // id as well, so a holder whose recorded boot is not among the
        // caller's live hosts was left behind by that restart and also
        // re-binds. A holder whose boot IS still live keeps the name - a
        // claim from another live boot is identity theft across the fleet,
        // and a claim from a different pane is a different agent; both
        // stay refused below.
        let resumed = members.iter().find(|m| {
            m.display_name.eq_ignore_ascii_case(&candidate.display_name)
                && m.target.workspace == candidate.target.workspace
                && m.target.pane == candidate.target.pane
                && (m.target.host == candidate.target.host || !live_hosts.contains(&m.target.host))
        });
        if let Some(existing) = resumed {
            let updated = Member {
                member_id: existing.member_id.clone(),
                display_name: candidate.display_name,
                target: target.clone(),
            };
            identity::record(&tx, "rebound", team_id, &updated, None)?;
            insert_member(&tx, team_id, &updated)?;
            tx.commit()?;
            return Ok(updated);
        }

        // A tombstoned row from a recent removal restores with its own id
        // and its reset work re-linked, so a mistaken cleanup heals without
        // minting a new identity (task 79a12d7b).
        if let Some((tombstoned_row, task_ids)) =
            tombstone_to_restore(&tx, team_id, &candidate, &tombstoned)?
        {
            let restored = Member {
                member_id: tombstoned_row.member_id.clone(),
                display_name: candidate.display_name,
                target: target.clone(),
            };
            tx.execute(
                "UPDATE members SET removed_at = NULL, display_name = ?3, host = ?4,
                 session = ?5, workspace = ?6, pane = ?7
                 WHERE team_id = ?1 AND member_id = ?2",
                params![
                    team_id,
                    restored.member_id,
                    restored.display_name,
                    restored.target.host,
                    restored.target.session,
                    restored.target.workspace,
                    restored.target.pane,
                ],
            )?;
            for task_id in &task_ids {
                tx.execute(
                    "UPDATE tasks SET assignee_id = ?3
                     WHERE team_id = ?1 AND task_id = ?2 AND assignee_id IS NULL",
                    params![team_id, task_id, restored.member_id],
                )?;
            }
            let details = (!task_ids.is_empty()).then(|| task_ids.join(","));
            identity::record(&tx, "restored", team_id, &restored, details.as_deref())?;
            tx.commit()?;
            return Ok(restored);
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
        identity::record(&tx, "registered", team_id, &candidate, None)?;
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
                       AND removed_at IS NULL
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

impl TeamStore {
    pub fn posts(&self, team_id: &str, cursor: i64, limit: usize) -> Result<Vec<(i64, Post)>> {
        self.page(team_id, Window::After(cursor), limit)
    }

    /// The newest `limit` posts, in the same ascending order [`posts`] returns.
    ///
    /// A snapshot uses this instead of reading from the oldest post and keeping
    /// only the last page, which costs a page per page-sized slice of the
    /// team's whole history. Pass the first sequence already shown as `before`
    /// to page further back; that page's first sequence is the next cursor.
    pub fn latest_posts(
        &self,
        team_id: &str,
        before: Option<i64>,
        limit: usize,
    ) -> Result<Vec<(i64, Post)>> {
        self.page(team_id, Window::Before(before.unwrap_or(i64::MAX)), limit)
    }

    fn page(&self, team_id: &str, window: Window, limit: usize) -> Result<Vec<(i64, Post)>> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE,
            });
        }
        let (sql, bound, newest_first) = match window {
            Window::After(cursor) => (POSTS_PAGE_SQL, cursor, false),
            Window::Before(bound) => (POSTS_LATEST_PAGE_SQL, bound, true),
        };
        let mut stmt = self.prep(sql)?;
        let rows = stmt.query_map(params![team_id, bound, limit as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })?;
        let mut out: Vec<(i64, Post)> = Vec::new();
        for row in rows {
            let (seq, post_id, team_id, author_kind, author_id, reply_to, body, at, recipient) =
                row?;
            match out.last_mut() {
                Some((last_seq, post)) if *last_seq == seq && post.post_id == post_id => {
                    if let Some(recipient) = recipient {
                        post.mentions.push(recipient);
                    }
                }
                _ => out.push((
                    seq,
                    Post {
                        mentions: recipient
                            .map(|recipient| vec![recipient])
                            .unwrap_or_default(),
                        post_id,
                        team_id,
                        author: match author_kind.as_str() {
                            "human" => Author::Human { user_id: author_id },
                            _ => Author::Member {
                                member_id: author_id,
                            },
                        },
                        reply_to,
                        body,
                        created_at: parse_time(at)?,
                    },
                )),
            }
        }
        // A newest-first page is read from the team's newest end; callers get
        // every page oldest-first so `posts` cursors and `before` cursors keep
        // meaning the same sequence order.
        if newest_first {
            out.reverse();
        }
        Ok(out)
    }

    pub fn acknowledge(&self, post_id: &str, recipient_id: &str) -> Result<Acknowledgement> {
        validate("recipient_id", recipient_id, MAX_ID_BYTES)?;
        let tx = self.db.unchecked_transaction()?;
        let team_id: Option<String> = tx
            .query_row(
                "SELECT team_id FROM posts WHERE post_id=?1",
                [post_id],
                |r| r.get(0),
            )
            .optional()?;
        let team_id = team_id.ok_or_else(|| Error::PostNotFound(post_id.to_owned()))?;
        let in_team: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE team_id=?1 AND member_id=?2 AND removed_at IS NULL)",
            params![team_id, recipient_id],
            |r| r.get(0),
        )?;
        if !in_team {
            return Err(Error::MemberNotInTeam {
                team_id,
                member_id: recipient_id.to_owned(),
            });
        }
        let mentioned: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM mentions WHERE post_id=?1 AND recipient_id=?2)",
            params![post_id, recipient_id],
            |r| r.get(0),
        )?;
        if !mentioned {
            return Err(Error::NotMentioned {
                post_id: post_id.to_owned(),
                recipient_id: recipient_id.to_owned(),
            });
        }
        let at =
            DateTime::from_timestamp_millis(Utc::now().timestamp_millis()).ok_or_else(|| {
                Error::Invalid {
                    field: "current timestamp",
                    max: MAX_ID_BYTES,
                }
            })?;
        let stamp = at.to_rfc3339_opts(SecondsFormat::Millis, true);
        tx.execute("INSERT INTO acknowledgements(post_id,recipient_id,acknowledged_at) VALUES (?1,?2,?3) ON CONFLICT(post_id,recipient_id) DO NOTHING", params![post_id,recipient_id,stamp])?;
        let stored: String = tx.query_row(
            "SELECT acknowledged_at FROM acknowledgements WHERE post_id=?1 AND recipient_id=?2",
            params![post_id, recipient_id],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(Acknowledgement {
            post_id: post_id.to_owned(),
            recipient_id: recipient_id.to_owned(),
            acknowledged_at: parse_time(stored)?,
        })
    }

    pub fn acknowledgements(&self, post_id: &str) -> Result<Vec<Acknowledgement>> {
        let mut stmt=self.prep("SELECT recipient_id,acknowledged_at FROM acknowledgements WHERE post_id=?1 ORDER BY recipient_id")?;
        let rows = stmt.query_map([post_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (recipient_id, at) = row?;
            Ok(Acknowledgement {
                post_id: post_id.to_owned(),
                recipient_id,
                acknowledged_at: parse_time(at)?,
            })
        })
        .collect()
    }

    /// One explicit status for each mentioned recipient; absent acknowledgements remain pending.
    pub fn recipient_statuses(&self, post_id: &str) -> Result<Vec<(String, RecipientStatus)>> {
        let mut stmt = self.prep("SELECT m.recipient_id,a.acknowledged_at FROM mentions m LEFT JOIN acknowledgements a ON a.post_id=m.post_id AND a.recipient_id=m.recipient_id WHERE m.post_id=?1 ORDER BY m.recipient_id")?;
        let rows = stmt.query_map([post_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.map(|row| {
            let (id, at) = row?;
            Ok((
                id,
                match at {
                    Some(value) => RecipientStatus::Acknowledged(parse_time(value)?),
                    None => RecipientStatus::Pending,
                },
            ))
        })
        .collect()
    }

    /// [`recipient_statuses`] for many posts, one query per 500-id chunk,
    /// keyed by post id. Posts without mentions map to an empty vector;
    /// per-post vectors are ordered by recipient id, matching the
    /// single-post form.
    pub fn recipient_statuses_for_posts(
        &self,
        post_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<(String, RecipientStatus)>>> {
        let mut out: std::collections::HashMap<String, Vec<(String, RecipientStatus)>> =
            std::collections::HashMap::with_capacity(post_ids.len());
        for post_id in post_ids {
            out.entry(post_id.clone()).or_default();
        }
        for chunk in post_ids.chunks(STATUS_BATCH_CHUNK) {
            let list = vec!["?"; chunk.len()].join(",");
            // Receipts of a tombstoned member wait for a restore instead of
            // showing on the board next to a roster that no longer lists
            // them (task 79a12d7b).
            let sql = format!(
                "SELECT m.post_id,m.recipient_id,a.acknowledged_at
                 FROM mentions m
                 LEFT JOIN acknowledgements a
                        ON a.post_id=m.post_id AND a.recipient_id=m.recipient_id
                 WHERE m.post_id IN ({list})
                   AND m.recipient_id IN (
                       SELECT member_id FROM members
                       WHERE team_id IN (SELECT team_id FROM posts WHERE post_id = m.post_id)
                         AND removed_at IS NULL
                   )
                 ORDER BY m.post_id,m.recipient_id"
            );
            let mut stmt = self.prep(&sql)?;
            let params: Vec<&str> = chunk.iter().map(String::as_str).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?;
            for row in rows {
                let (post_id, id, at) = row?;
                let status = match at {
                    Some(value) => RecipientStatus::Acknowledged(parse_time(value)?),
                    None => RecipientStatus::Pending,
                };
                if let Some(entries) = out.get_mut(&post_id) {
                    entries.push((id, status));
                }
            }
        }
        Ok(out)
    }

    /// Mentioned posts a recipient has not acknowledged, in one query. This
    /// is the whole history the `team status` command needs, where it used
    /// to scan every post with a statuses query per post.
    pub fn pending_mention_count(&self, team_id: &str, recipient_id: &str) -> Result<usize> {
        let mut stmt = self.prep(PENDING_MENTIONS_SQL)?;
        Ok(stmt.query_row(params![team_id, recipient_id], |r| r.get::<_, i64>(0))? as usize)
    }
}

/// The tombstoned row a re-registration can restore: the same member name
/// and pane, removed within the grace window. Returns the row and the task
/// ids its removal reset, from the audit trail's details.
fn tombstone_to_restore(
    tx: &rusqlite::Transaction<'_>,
    team_id: &str,
    candidate: &Member,
    tombstoned: &[Member],
) -> Result<Option<(Member, Vec<String>)>> {
    let cutoff = (Utc::now() - Duration::days(RESTORE_WINDOW_DAYS))
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let row = tombstoned.iter().find(|member| {
        member
            .display_name
            .eq_ignore_ascii_case(&candidate.display_name)
            && member.target.workspace == candidate.target.workspace
            && member.target.pane == candidate.target.pane
    });
    let Some(row) = row else {
        return Ok(None);
    };
    let removed_at: Option<String> = tx
        .query_row(
            "SELECT removed_at FROM members WHERE team_id = ?1 AND member_id = ?2",
            params![team_id, row.member_id],
            |r| r.get(0),
        )
        .optional()?;
    if !removed_at.is_some_and(|at| at >= cutoff) {
        return Ok(None);
    }
    let details: Option<String> = tx
        .query_row(
            "SELECT details FROM identity_log
             WHERE team_id = ?1 AND member_id = ?2 AND action = 'removed'
             ORDER BY id DESC LIMIT 1",
            params![team_id, row.member_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let task_ids = details
        .map(|details| {
            details
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    Ok(Some((row.clone(), task_ids)))
}
