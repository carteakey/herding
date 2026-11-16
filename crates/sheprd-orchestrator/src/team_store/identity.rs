//! The member-identity audit trail: who was registered, rebound, or
//! removed, when, with which identity. Written inside the mutation's own
//! transaction, so the durable witness can never drift from the member
//! table - the Nov 12 roster cleanup took an hours-long reconstruction from
//! task-reset timestamps because nothing recorded who removed what when
//! (task 79a12d7b, finding 7ad9beb0).
//!
//! The table is the plan's unified `audit` table (see
//! `docs/herding-teams-on-core.md`, Audit): these identity events are its
//! first writer, so rows written now are already in phase 2's final shape.
//! The store's ops do not know their caller yet, so `actor` records the
//! schema's `'unknown'` default and `terminal` stays NULL; phase 2's
//! `apply(actor, op)` fills both. Identity events store the member in
//! `ref_id`.

use super::{Error, Member, Result, SessionTarget, TeamStore, now_stamp};
use rusqlite::{OptionalExtension, params};

/// One recorded identity event, oldest first in [`TeamStore::identity_history`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityEvent {
    pub at: String,
    /// `registered` (new member or re-proof of the same target), `rebound`
    /// (the identity moved: a resumed session or a host migration), or
    /// `removed`.
    pub op: String,
    pub team_id: String,
    /// The member the event is about, recorded in `audit.ref_id`.
    pub member_id: String,
    /// Who acted, from `audit.actor`; `'unknown'` until phase 2 passes the
    /// caller through.
    pub actor: String,
    pub terminal: Option<String>,
    pub display_name: String,
    pub host: String,
    pub session: String,
    pub workspace: Option<String>,
    pub pane: Option<String>,
}

/// Records one identity event inside the caller's transaction, so the log
/// commits atomically with the mutation it witnesses. `actor` and `terminal`
/// are left to the schema defaults (`'unknown'`, NULL) because the store's
/// ops do not know their caller; phase 2's `apply` passes both.
pub(super) fn record(
    connection: &rusqlite::Connection,
    op: &str,
    team_id: &str,
    member: &Member,
) -> Result<()> {
    let (_, stamp) = now_stamp()?;
    connection.execute(
        "INSERT INTO audit(team_id, op, ref_id, at, display_name, host, session, workspace, pane)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            team_id,
            op,
            member.member_id,
            stamp,
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
        op,
        "team member identity changed"
    );
    Ok(())
}

impl TeamStore {
    /// A member's identity events, oldest first; all members of the team
    /// when `member_id` is `None`. Reads the shared `audit` table through
    /// the identity ops, so phase 2's other writers cannot leak into this
    /// view.
    pub fn identity_history(
        &self,
        team_id: &str,
        member_id: Option<&str>,
    ) -> Result<Vec<IdentityEvent>> {
        let mut statement = self.db.prepare_cached(
            "SELECT at, op, ref_id, actor, terminal, display_name, host, session, workspace, pane
             FROM audit
             WHERE team_id = ?1
               AND (?2 IS NULL OR ref_id = ?2)
               AND op IN ('registered','rebound','removed')
             ORDER BY seq",
        )?;
        let rows = statement
            .query_map(params![team_id, member_id], |row| {
                Ok(IdentityEvent {
                    at: row.get(0)?,
                    op: row.get(1)?,
                    team_id: team_id.to_owned(),
                    member_id: row.get(2)?,
                    actor: row.get(3)?,
                    terminal: row.get(4)?,
                    display_name: row.get(5)?,
                    host: row.get(6)?,
                    session: row.get(7)?,
                    workspace: row.get(8)?,
                    pane: row.get(9)?,
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
        let exists: i64 = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE team_id=?1 AND member_id=?2)",
            params![team_id, member.member_id],
            |row| row.get(0),
        )?;
        if exists != 1 {
            record(connection, "registered", team_id, member)?;
        }
    }
    Ok(())
}

/// The member row as it stands inside the caller's transaction, so a
/// mutation logs the identity it is about to change or remove. Runs on the
/// caller's transaction.
pub(super) fn member_row(
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
