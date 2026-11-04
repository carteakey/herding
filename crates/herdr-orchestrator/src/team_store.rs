//! Small durable store for explicit team coordination records.
//!
//! This module stores teams, goals, members, posts, tasks, findings, and
//! targeting metadata. It does not deliver, dispatch, or acknowledge anything
//! on behalf of a recipient.

mod findings;
mod mentions;
mod tasks;

pub use findings::{Confidence, Finding};
pub use mentions::{extract_at_tokens, resolve_member_ref, resolve_recipients};
pub use tasks::{Task, TaskStatus};

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{CachedStatement, Connection, OptionalExtension, params};
use std::{collections::HashSet, path::Path};

pub const MAX_ID_BYTES: usize = 128;
pub const MAX_NAME_BYTES: usize = 256;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_MENTIONS: usize = 64;
pub const MAX_MEMBERS: usize = 64;
pub const MAX_PAGE_SIZE: usize = 100;
/// SQL of [`TeamStore::pending_mention_count`], named so a test can check the
/// query plan the recipient index is meant to serve.
const PENDING_MENTIONS_SQL: &str = "SELECT COUNT(*) FROM mentions m
     JOIN posts p ON p.post_id=m.post_id
     WHERE p.team_id=?1 AND m.recipient_id=?2
       AND NOT EXISTS (
           SELECT 1 FROM acknowledgements a
           WHERE a.post_id=m.post_id AND a.recipient_id=m.recipient_id
       )";
/// Post ids per `IN (...)` chunk in `recipient_statuses_for_posts`; well
/// under SQLite's variable limit, and one chunk covers a full snapshot page.
pub const STATUS_BATCH_CHUNK: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Team database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Private(#[from] crate::private_file::Error),
    #[error("Invalid {field}: must contain 1..={max} bytes")]
    Invalid { field: &'static str, max: usize },
    #[error("Team {0:?} was not found")]
    TeamNotFound(String),
    #[error("Post {0:?} was not found")]
    PostNotFound(String),
    #[error("Task {0:?} was not found")]
    TaskNotFound(String),
    #[error("Task {0:?} is not available for this member or transition")]
    TaskUnavailable(String),
    #[error("Task {task_id:?} cannot transition from status {status:?}")]
    InvalidTaskTransition { task_id: String, status: String },
    #[error("Finding {0:?} was not found")]
    FindingNotFound(String),
    #[error("Member {member_id:?} is not in team {team_id:?}")]
    MemberNotInTeam { team_id: String, member_id: String },
    #[error("Reply target {0:?} is absent or belongs to another team")]
    InvalidReply(String),
    #[error("Team has more than {max} members")]
    TooManyMembers { max: usize },
    #[error("Team repeats member id {0:?}")]
    DuplicateMember(String),
    #[error("Team already has a member named {0:?}")]
    DuplicateDisplayName(String),
    #[error(
        "Member {member_id:?} in team {team_id:?} cannot be reassigned to a different session target"
    )]
    MemberTargetChanged { team_id: String, member_id: String },
    #[error("Post repeats recipient id {0:?}")]
    DuplicateRecipient(String),
    #[error("Mention {0:?} does not match any member in this team")]
    UnknownMention(String),
    #[error("Mention {0:?} matches more than one member in this team")]
    AmbiguousMention(String),
    #[error("Recipient {recipient_id:?} was not mentioned in post {post_id:?}")]
    NotMentioned {
        post_id: String,
        recipient_id: String,
    },
    #[error("Stored timestamp is invalid: {value:?}")]
    Time {
        value: String,
        #[source]
        source: chrono::ParseError,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTarget {
    pub host: String,
    pub session: String,
    pub workspace: Option<String>,
    pub pane: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub member_id: String,
    pub display_name: String,
    pub target: SessionTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Team {
    pub team_id: String,
    pub name: String,
    pub supervisor: Member,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub post_id: String,
    pub team_id: String,
    pub author: Author,
    pub reply_to: Option<String>,
    pub body: String,
    pub mentions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Selection at creation time. `All` is resolved to a persisted snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recipients {
    All,
    Explicit(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Author {
    Human { user_id: String },
    Member { member_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acknowledgement {
    pub post_id: String,
    pub recipient_id: String,
    pub acknowledged_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecipientStatus {
    Pending,
    Acknowledged(DateTime<Utc>),
}

pub struct TeamStore {
    pub(super) db: Connection,
    /// Statements prepared through [`TeamStore::prep`], counted only when this
    /// crate's test fixtures are on: a test can then pin how many queries a
    /// read path issues instead of trusting a comment about N+1.
    #[cfg(any(test, feature = "test-support"))]
    statements: std::cell::Cell<usize>,
}

impl TeamStore {
    pub fn open(path: &Path) -> Result<Self> {
        crate::private_file::prepare(path)?;
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
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
            CREATE INDEX IF NOT EXISTS findings_team_idx ON findings(team_id, sequence);",
        )?;
        Ok(Self {
            db,
            #[cfg(any(test, feature = "test-support"))]
            statements: std::cell::Cell::new(0),
        })
    }

    /// Prepare through the store's statement cache, counting the prepare when
    /// test fixtures are on so a read path's query count is testable.
    fn prep(&self, sql: &str) -> Result<CachedStatement<'_>> {
        #[cfg(any(test, feature = "test-support"))]
        self.statements.set(self.statements.get() + 1);
        Ok(self.db.prepare_cached(sql)?)
    }

    /// Statements prepared since the last reset. Test fixtures only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn statement_count(&self) -> usize {
        self.statements.get()
    }

    /// Zero the [`Self::statement_count`] baseline. Test fixtures only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn reset_statement_count(&self) {
        self.statements.set(0);
    }

    pub fn save_team(&mut self, team: &Team) -> Result<()> {
        validate("team_id", &team.team_id, MAX_ID_BYTES)?;
        validate("team name", &team.name, MAX_NAME_BYTES)?;
        validate_member(&team.supervisor)?;
        let mut submitted = HashSet::with_capacity(team.members.len() + 1);
        submitted.insert(team.supervisor.member_id.clone());
        for member in &team.members {
            validate_member(member)?;
            if !submitted.insert(member.member_id.clone()) {
                return Err(Error::DuplicateMember(member.member_id.clone()));
            }
        }
        let tx = self.db.transaction()?;
        let mut existing = tx.prepare("SELECT member_id FROM members WHERE team_id=?1")?;
        let existing_ids = existing
            .query_map([&team.team_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<HashSet<_>, _>>()?;
        drop(existing);
        let total = existing_ids.union(&submitted).count();
        if total > MAX_MEMBERS {
            return Err(Error::TooManyMembers { max: MAX_MEMBERS });
        }
        for member in std::iter::once(&team.supervisor).chain(team.members.iter()) {
            let old_target: Option<(String, String, Option<String>, Option<String>)> = tx
                .query_row(
                    "SELECT host,session,workspace,pane FROM members WHERE team_id=?1 AND member_id=?2",
                    params![team.team_id, member.member_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            if old_target.is_some_and(|target| {
                target
                    != (
                        member.target.host.clone(),
                        member.target.session.clone(),
                        member.target.workspace.clone(),
                        member.target.pane.clone(),
                    )
            }) {
                return Err(Error::MemberTargetChanged {
                    team_id: team.team_id.clone(),
                    member_id: member.member_id.clone(),
                });
            }
        }
        tx.execute("INSERT INTO teams(team_id,name,supervisor_id) VALUES (?1,?2,?3)
            ON CONFLICT(team_id) DO UPDATE SET name=excluded.name, supervisor_id=excluded.supervisor_id",
            params![team.team_id, team.name, team.supervisor.member_id])?;
        insert_member(&tx, &team.team_id, &team.supervisor)?;
        for member in &team.members {
            validate_member(member)?;
            insert_member(&tx, &team.team_id, member)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn team(&self, team_id: &str) -> Result<Option<Team>> {
        let row: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT name,supervisor_id FROM teams WHERE team_id=?1",
                [team_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((name, supervisor_id)) = row else {
            return Ok(None);
        };
        let mut stmt = self.prep("SELECT member_id,display_name,host,session,workspace,pane FROM members WHERE team_id=?1 ORDER BY member_id")?;
        let members = stmt
            .query_map([team_id], read_member)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let supervisor = members
            .iter()
            .find(|m| m.member_id == supervisor_id)
            .cloned()
            .ok_or_else(|| Error::TeamNotFound(team_id.to_owned()))?;
        Ok(Some(Team {
            team_id: team_id.to_owned(),
            name,
            supervisor,
            members: members
                .into_iter()
                .filter(|m| m.member_id != supervisor_id)
                .collect(),
        }))
    }

    /// Teams in lexical ID order, after an optional exclusive cursor.
    pub fn list_teams(&self, after: Option<&str>, limit: usize) -> Result<Vec<Team>> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE,
            });
        }
        let mut stmt = self.prep(
            "SELECT team_id FROM teams WHERE (?1 IS NULL OR team_id>?1) ORDER BY team_id LIMIT ?2",
        )?;
        let ids = stmt
            .query_map(params![after, limit as i64], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter()
            .map(|id| {
                self.team(id)?
                    .ok_or_else(|| Error::TeamNotFound(id.clone()))
            })
            .collect()
    }

    pub fn add_post(
        &mut self,
        post_id: &str,
        team_id: &str,
        author: &Author,
        reply_to: Option<&str>,
        body: &str,
        mentions: &[String],
    ) -> Result<Post> {
        self.add_post_with_recipients(
            post_id,
            team_id,
            author,
            reply_to,
            body,
            &Recipients::Explicit(mentions.to_vec()),
        )
    }

    /// Resolves recipients and writes the post plus recipient snapshot atomically.
    pub fn add_post_with_recipients(
        &mut self,
        post_id: &str,
        team_id: &str,
        author: &Author,
        reply_to: Option<&str>,
        body: &str,
        recipients: &Recipients,
    ) -> Result<Post> {
        validate("post_id", post_id, MAX_ID_BYTES)?;
        validate("team_id", team_id, MAX_ID_BYTES)?;
        let (author_kind, author_id) = match author {
            Author::Human { user_id } => ("human", user_id.as_str()),
            Author::Member { member_id } => ("member", member_id.as_str()),
        };
        validate("author_id", author_id, MAX_ID_BYTES)?;
        validate("body", body, MAX_TEXT_BYTES)?;
        if let Recipients::Explicit(mentions) = recipients {
            if mentions.len() > MAX_MENTIONS {
                return Err(Error::Invalid {
                    field: "mentions",
                    max: MAX_MENTIONS,
                });
            }
            let mut seen = HashSet::with_capacity(mentions.len());
            for mention in mentions {
                validate("mention", mention, MAX_ID_BYTES)?;
                if !seen.insert(mention) {
                    return Err(Error::DuplicateRecipient(mention.clone()));
                }
            }
        }
        let now = Utc::now();
        let stamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);
        let tx = self.db.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM teams WHERE team_id=?1)",
            [team_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::TeamNotFound(team_id.to_owned()));
        }
        let is_member = |id: &str| -> rusqlite::Result<bool> {
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM members WHERE team_id=?1 AND member_id=?2)",
                params![team_id, id],
                |r| r.get(0),
            )
        };
        if author_kind == "member" && !is_member(author_id)? {
            return Err(Error::MemberNotInTeam {
                team_id: team_id.to_owned(),
                member_id: author_id.to_owned(),
            });
        }
        let mentions = match recipients {
            Recipients::Explicit(mentions) => mentions.clone(),
            Recipients::All => {
                let mut stmt = tx
                    .prepare("SELECT member_id FROM members WHERE team_id=?1 ORDER BY member_id")?;
                let ids = stmt
                    .query_map([team_id], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                ids.into_iter()
                    .filter(|id| !(author_kind == "member" && id == author_id))
                    .collect()
            }
        };
        for id in &mentions {
            if !is_member(id)? {
                return Err(Error::MemberNotInTeam {
                    team_id: team_id.to_owned(),
                    member_id: id.clone(),
                });
            }
        }
        if let Some(parent) = reply_to {
            let parent_team: Option<String> = tx
                .query_row(
                    "SELECT team_id FROM posts WHERE post_id=?1",
                    [parent],
                    |r| r.get(0),
                )
                .optional()?;
            if parent_team.as_deref() != Some(team_id) {
                return Err(Error::InvalidReply(parent.to_owned()));
            }
        }
        tx.execute("INSERT INTO posts(post_id,team_id,author_kind,author_id,reply_to,body,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![post_id, team_id, author_kind, author_id, reply_to, body, stamp])?;
        for recipient in &mentions {
            tx.execute(
                "INSERT INTO mentions(post_id,recipient_id) VALUES (?1,?2)",
                params![post_id, recipient],
            )?;
        }
        tx.commit()?;
        Ok(Post {
            post_id: post_id.to_owned(),
            team_id: team_id.to_owned(),
            author: author.clone(),
            reply_to: reply_to.map(str::to_owned),
            body: body.to_owned(),
            mentions,
            created_at: parse_time(stamp)?,
        })
    }

    /// Stable forward pagination: the returned sequence is the next cursor.
    ///
    /// One statement: the page of posts is selected first (so `limit` counts
    /// posts, not join rows) and each page's mentions ride along in the same
    /// query instead of one query per post.
    pub fn posts(&self, team_id: &str, cursor: i64, limit: usize) -> Result<Vec<(i64, Post)>> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE,
            });
        }
        let mut stmt = self.prep(
            "WITH page AS (
                SELECT sequence,post_id,team_id,author_kind,author_id,reply_to,body,created_at
                FROM posts WHERE team_id=?1 AND sequence>?2
                ORDER BY sequence LIMIT ?3
            )
            SELECT page.sequence,page.post_id,page.team_id,page.author_kind,page.author_id,
                   page.reply_to,page.body,page.created_at,m.recipient_id
            FROM page LEFT JOIN mentions m ON m.post_id=page.post_id
            ORDER BY page.sequence,m.recipient_id",
        )?;
        let rows = stmt.query_map(params![team_id, cursor, limit as i64], |r| {
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
            "SELECT EXISTS(SELECT 1 FROM members WHERE team_id=?1 AND member_id=?2)",
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
            let sql = format!(
                "SELECT m.post_id,m.recipient_id,a.acknowledged_at
                 FROM mentions m
                 LEFT JOIN acknowledgements a
                        ON a.post_id=m.post_id AND a.recipient_id=m.recipient_id
                 WHERE m.post_id IN ({list})
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

pub(super) fn validate(field: &'static str, value: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.len() > max {
        Err(Error::Invalid { field, max })
    } else {
        Ok(())
    }
}
pub(super) fn validate_member(m: &Member) -> Result<()> {
    validate("member_id", &m.member_id, MAX_ID_BYTES)?;
    validate("display_name", &m.display_name, MAX_NAME_BYTES)?;
    validate("host", &m.target.host, MAX_ID_BYTES)?;
    validate("session", &m.target.session, MAX_ID_BYTES)?;
    for (field, v) in [
        ("workspace", m.target.workspace.as_deref()),
        ("pane", m.target.pane.as_deref()),
    ] {
        if let Some(v) = v {
            validate(field, v, MAX_ID_BYTES)?;
        }
    }
    Ok(())
}
pub(super) fn insert_member(
    tx: &rusqlite::Transaction<'_>,
    team_id: &str,
    m: &Member,
) -> Result<()> {
    tx.execute("INSERT INTO members(team_id,member_id,display_name,host,session,workspace,pane) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(team_id,member_id) DO UPDATE SET display_name=excluded.display_name,host=excluded.host,session=excluded.session,workspace=excluded.workspace,pane=excluded.pane",params![team_id,m.member_id,m.display_name,m.target.host,m.target.session,m.target.workspace,m.target.pane])?;
    Ok(())
}
pub(super) fn read_member(r: &rusqlite::Row<'_>) -> rusqlite::Result<Member> {
    Ok(Member {
        member_id: r.get(0)?,
        display_name: r.get(1)?,
        target: SessionTarget {
            host: r.get(2)?,
            session: r.get(3)?,
            workspace: r.get(4)?,
            pane: r.get(5)?,
        },
    })
}
pub(super) fn parse_time(value: String) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|v| v.to_utc())
        .map_err(|source| Error::Time { value, source })
}
pub(super) fn now_stamp() -> Result<(DateTime<Utc>, String)> {
    let at =
        DateTime::from_timestamp_millis(Utc::now().timestamp_millis()).ok_or(Error::Invalid {
            field: "current timestamp",
            max: MAX_ID_BYTES,
        })?;
    let stamp = at.to_rfc3339_opts(SecondsFormat::Millis, true);
    Ok((at, stamp))
}
pub(super) fn is_member(
    tx: &rusqlite::Transaction<'_>,
    team_id: &str,
    member_id: &str,
) -> rusqlite::Result<bool> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM members WHERE team_id = ?1 AND member_id = ?2)",
        params![team_id, member_id],
        |row| row.get(0),
    )
}
pub(super) fn ensure_author_in_team<'a>(
    tx: &rusqlite::Transaction<'_>,
    team_id: &str,
    author: &'a Author,
) -> Result<(&'static str, &'a str)> {
    let (kind, id) = match author {
        Author::Human { user_id } => ("human", user_id.as_str()),
        Author::Member { member_id } => ("member", member_id.as_str()),
    };
    validate("author_id", id, MAX_ID_BYTES)?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM teams WHERE team_id = ?1)",
        [team_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(Error::TeamNotFound(team_id.to_owned()));
    }
    if kind == "member" && !is_member(tx, team_id, id)? {
        return Err(Error::MemberNotInTeam {
            team_id: team_id.to_owned(),
            member_id: id.to_owned(),
        });
    }
    Ok((kind, id))
}

#[cfg(test)]
#[path = "team_store/departure_tests.rs"]
mod departure_tests;

#[cfg(test)]
#[path = "team_store/status_tests.rs"]
mod status_tests;

#[cfg(test)]
#[path = "team_store/task_lifecycle_tests.rs"]
mod task_lifecycle_tests;

#[cfg(test)]
#[path = "team_store/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "team_store/workflow_tests.rs"]
mod workflow_tests;
