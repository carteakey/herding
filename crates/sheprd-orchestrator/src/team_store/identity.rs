//! The member-identity audit trail: who was registered, rebound, or
//! removed, when, with which identity. Written inside the mutation's own
//! transaction, so the durable witness can never drift from the member
//! table - the Nov 12 roster cleanup took an hours-long reconstruction from
//! task-reset timestamps because nothing recorded who removed what when
//! (task 79a12d7b, finding 7ad9beb0).

use super::{Error, Member, Result, SessionTarget, TeamStore, now_stamp};
use rusqlite::{OptionalExtension, params};

/// The identity table, created beside the rest of the schema.
pub(crate) const IDENTITY_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS identity_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at TEXT NOT NULL,
        action TEXT NOT NULL CHECK(action IN ('registered','rebound','removed')),
        team_id TEXT NOT NULL,
        member_id TEXT NOT NULL,
        display_name TEXT NOT NULL,
        host TEXT, session TEXT, workspace TEXT, pane TEXT
    );
    CREATE INDEX IF NOT EXISTS identity_log_member_idx
        ON identity_log(team_id, member_id, id);
";

/// One recorded identity event, oldest first in [`TeamStore::identity_history`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityEvent {
    pub at: String,
    /// `registered` (new member or re-proof of the same target), `rebound`
    /// (the identity moved: a resumed session or a pane move), or `removed`.
    pub action: String,
    pub team_id: String,
    pub member_id: String,
    pub display_name: String,
    pub host: String,
    pub session: String,
    pub workspace: Option<String>,
    pub pane: Option<String>,
}

/// Records one identity event inside the caller's transaction, so the log
/// commits atomically with the mutation it witnesses.
pub(super) fn record(
    connection: &rusqlite::Connection,
    action: &str,
    team_id: &str,
    member: &Member,
) -> Result<()> {
    let (_, stamp) = now_stamp()?;
    connection.execute(
        "INSERT INTO identity_log(at, action, team_id, member_id, display_name, host, session, workspace, pane)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            stamp,
            action,
            team_id,
            member.member_id,
            member.display_name,
            member.target.host,
            member.target.session,
            member.target.workspace,
            member.target.pane,
        ],
    )?;
    tracing::info!(
        team = %team_id,
        member = %member.display_name,
        member_id = %member.member_id,
        host = %member.target.host,
        pane = ?member.target.pane,
        action,
        "team member identity changed"
    );
    Ok(())
}

impl TeamStore {
    /// A member's identity events, oldest first; all members of the team
    /// when `member_id` is `None`.
    pub fn identity_history(
        &self,
        team_id: &str,
        member_id: Option<&str>,
    ) -> Result<Vec<IdentityEvent>> {
        let mut statement = self.db.prepare_cached(
            "SELECT at, action, member_id, display_name, host, session, workspace, pane
             FROM identity_log WHERE team_id = ?1 AND (?2 IS NULL OR member_id = ?2)
             ORDER BY id",
        )?;
        let rows = statement
            .query_map(params![team_id, member_id], |row| {
                Ok(IdentityEvent {
                    at: row.get(0)?,
                    action: row.get(1)?,
                    team_id: team_id.to_owned(),
                    member_id: row.get(2)?,
                    display_name: row.get(3)?,
                    host: row.get(4)?,
                    session: row.get(5)?,
                    workspace: row.get(6)?,
                    pane: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Records `registered` for every member of the batch the members table does
/// not know yet, so a save that only edits existing rows stays silent and a
/// genuinely new worker lands in the log.
pub(super) fn record_new_members<'a>(
    connection: &rusqlite::Connection,
    team_id: &str,
    members: impl IntoIterator<Item = &'a Member>,
) -> Result<()> {
    for member in members {
        let known = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM members WHERE team_id=?1 AND member_id=?2)",
                params![team_id, member.member_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists == 1)
            .unwrap_or(false);
        if !known {
            record(connection, "registered", team_id, member)?;
        }
    }
    Ok(())
}

/// The member row as it stood just before a removal, so the log keeps the
/// identity that is about to disappear. Runs on the caller's transaction.
pub(super) fn member_before_removal(
    connection: &rusqlite::Connection,
    team_id: &str,
    member_id: &str,
) -> Result<Option<Member>> {
    connection
        .query_row(
            "SELECT member_id, display_name, host, session, workspace, pane
             FROM members WHERE team_id = ?1 AND member_id = ?2",
            params![team_id, member_id],
            |row| {
                Ok(Member {
                    member_id: row.get(0)?,
                    display_name: row.get(1)?,
                    target: SessionTarget {
                        host: row.get(2)?,
                        session: row.get(3)?,
                        workspace: row.get(4)?,
                        pane: row.get(5)?,
                    },
                })
            },
        )
        .optional()
        .map_err(Error::from)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
