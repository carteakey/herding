//! The team store's tables and the additive column migrations they need.
//!
//! Every table is created with `IF NOT EXISTS`, so a database written by an
//! older build keeps its rows. A column added later is applied here one
//! `ALTER TABLE` at a time, guarded by a column check, and is nullable so an
//! older build that does not know it still reads and writes the row it
//! always did.

use rusqlite::Connection;

use super::Result;

/// Tables, indexes, and the connection's pragmas. Keep the statement order:
/// `journal_mode` cannot change inside a transaction, and the foreign keys
/// are referenced in the order the tables are created.
const SCHEMA: &str = "PRAGMA journal_mode = WAL;
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
        model TEXT, account TEXT,
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
    CREATE INDEX IF NOT EXISTS findings_team_idx ON findings(team_id, sequence);";

/// Columns added to `members` after the first release. They are nullable, so
/// a row written before them reads back as unknown rather than failing.
const MEMBER_COLUMNS: [&str; 2] = ["model", "account"];

pub(super) fn prepare(db: &Connection) -> Result<()> {
    db.execute_batch(SCHEMA)?;
    for column in MEMBER_COLUMNS {
        if !has_column(db, "members", column)? {
            db.execute(&format!("ALTER TABLE members ADD COLUMN {column} TEXT"), [])?;
        }
    }
    Ok(())
}

fn has_column(db: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = db.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement.query_map([], |row| row.get::<_, String>("name"))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
