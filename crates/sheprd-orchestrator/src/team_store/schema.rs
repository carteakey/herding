//! The store's tables and the migrations an existing database needs.
use super::Result;
use rusqlite::Connection;
use rusqlite::params;

/// Creates every table on a fresh database and brings an existing one up
/// to the current shape: the issue-sync columns postdate the first
/// schema, so every database gains them in the migration below.
pub(super) fn run(db: &Connection) -> Result<()> {
    db.execute_batch(
            "PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS teams (
                team_id TEXT PRIMARY KEY, name TEXT NOT NULL,
                supervisor_id TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS team_goals (
                team_id TEXT PRIMARY KEY REFERENCES teams(team_id) ON DELETE CASCADE,
                goal TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS members (
                team_id TEXT NOT NULL REFERENCES teams(team_id) ON DELETE CASCADE,
                member_id TEXT NOT NULL, display_name TEXT NOT NULL,
                host TEXT NOT NULL, session TEXT NOT NULL,
                workspace TEXT, pane TEXT,
                PRIMARY KEY (team_id, member_id)
            );
            CREATE TABLE IF NOT EXISTS posts (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                post_id TEXT NOT NULL UNIQUE,
                team_id TEXT NOT NULL REFERENCES teams(team_id) ON DELETE CASCADE,
                author_kind TEXT NOT NULL CHECK(author_kind IN ('human','member')),
                author_id TEXT NOT NULL, reply_to TEXT REFERENCES posts(post_id),
                body TEXT NOT NULL, created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS posts_team_idx ON posts(team_id, sequence);
            CREATE TABLE IF NOT EXISTS mentions (
                post_id TEXT NOT NULL REFERENCES posts(post_id) ON DELETE CASCADE,
                recipient_id TEXT NOT NULL, PRIMARY KEY(post_id, recipient_id)
            );
            CREATE INDEX IF NOT EXISTS mentions_recipient_idx ON mentions(recipient_id);
            CREATE TABLE IF NOT EXISTS acknowledgements (
                post_id TEXT NOT NULL REFERENCES posts(post_id) ON DELETE CASCADE,
                recipient_id TEXT NOT NULL, acknowledged_at TEXT NOT NULL,
                PRIMARY KEY(post_id, recipient_id)
            );
            CREATE TABLE IF NOT EXISTS tasks (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL UNIQUE,
                team_id TEXT NOT NULL REFERENCES teams(team_id) ON DELETE CASCADE,
                creator_kind TEXT NOT NULL CHECK(creator_kind IN ('human','member')),
                creator_id TEXT NOT NULL,
                title TEXT NOT NULL,
                brief TEXT NOT NULL,
                assignee_id TEXT,
                status TEXT NOT NULL CHECK(status IN ('open','in_progress','blocked','done','cancelled')),
                blocker_reason TEXT,
                result TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS tasks_team_idx ON tasks(team_id, sequence);
            CREATE TABLE IF NOT EXISTS findings (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                finding_id TEXT NOT NULL UNIQUE,
                team_id TEXT NOT NULL REFERENCES teams(team_id) ON DELETE CASCADE,
                author_kind TEXT NOT NULL CHECK(author_kind IN ('human','member')),
                author_id TEXT NOT NULL,
                task_id TEXT REFERENCES tasks(task_id),
                description TEXT NOT NULL,
                evidence TEXT NOT NULL,
                confidence TEXT NOT NULL CHECK(confidence IN ('low','medium','high')),
                supersedes TEXT REFERENCES findings(finding_id),
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS findings_team_idx ON findings(team_id, sequence);
            CREATE TABLE IF NOT EXISTS team_issue_watch (
                team_id TEXT NOT NULL REFERENCES teams(team_id) ON DELETE CASCADE,
                repo TEXT NOT NULL,
                number INTEGER NOT NULL CHECK(number >= 0),
                added_by_kind TEXT NOT NULL CHECK(added_by_kind IN ('human','member')),
                added_by_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (team_id, repo, number)
            );
            CREATE TABLE IF NOT EXISTS audit (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                team_id TEXT NOT NULL,
                actor TEXT NOT NULL DEFAULT 'unknown',
                terminal TEXT,
                op TEXT NOT NULL,
                ref_id TEXT,
                at TEXT NOT NULL,
                display_name TEXT, host TEXT, session TEXT, workspace TEXT, pane TEXT
            );
            CREATE INDEX IF NOT EXISTS audit_team_idx ON audit(team_id, seq);
            CREATE INDEX IF NOT EXISTS audit_ref_idx ON audit(team_id, ref_id, seq);",
        )?;
    // The issue-sync columns postdate the first schema: the CREATE TABLE
    // above keeps the original shape, so every database gains the columns
    // here, new or old.
    for (column, declaration) in [
        ("issue_repo", "TEXT"),
        ("issue_number", "INTEGER"),
        ("issue_url", "TEXT"),
        ("issue_state", "TEXT"),
    ] {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name = ?1",
            params![column],
            |row| row.get(0),
        )?;
        if present == 0 {
            db.execute_batch(&format!(
                "ALTER TABLE tasks ADD COLUMN {column} {declaration};"
            ))?;
        }
    }
    // One task per issue per team: a re-import updates, it never
    // duplicates.
    db.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS tasks_issue_idx
                ON tasks(team_id, issue_repo, issue_number)
                WHERE issue_repo IS NOT NULL;",
    )?;
    Ok(())
}
