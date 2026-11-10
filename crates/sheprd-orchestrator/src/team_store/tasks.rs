//! Durable task board records and atomic task lifecycle transitions.

use super::{
    Author, Error, MAX_ID_BYTES, MAX_NAME_BYTES, MAX_PAGE_SIZE, MAX_TEXT_BYTES, Result, TeamStore,
    Window, ensure_author_in_team, is_member, now_stamp, parse_time, validate,
};
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

const TASK_COUNTS_SQL: &str = "SELECT SUM(CASE WHEN status IN ('open','in_progress','blocked')
                    THEN 1 ELSE 0 END),
                    SUM(CASE WHEN status IN ('open','in_progress','blocked')
                             AND assignee_id = ?2 THEN 1 ELSE 0 END)
             FROM tasks WHERE team_id = ?1";
const TASKS_LATEST_PAGE_SQL: &str =
    "SELECT task_id, team_id, creator_kind, creator_id, title, brief,
                    assignee_id, status, blocker_reason, result, created_at, updated_at, sequence
             FROM tasks WHERE team_id = ?1 AND sequence < ?2 ORDER BY sequence DESC LIMIT ?3";
const TASKS_PAGE_SQL: &str = "SELECT task_id, team_id, creator_kind, creator_id, title, brief,
                    assignee_id, status, blocker_reason, result, created_at, updated_at, sequence
             FROM tasks WHERE team_id = ?1 AND sequence > ?2 ORDER BY sequence LIMIT ?3";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Open,
    InProgress,
    Blocked,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "in_progress" => Some(Self::InProgress),
            "blocked" => Some(Self::Blocked),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub task_id: String,
    pub team_id: String,
    pub creator: Author,
    pub title: String,
    pub brief: String,
    pub assignee: Option<String>,
    pub status: TaskStatus,
    pub blocker_reason: Option<String>,
    pub result: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskCounts {
    pub open: usize,
    pub assigned: usize,
}

impl TeamStore {
    pub fn create_task(
        &mut self,
        task_id: &str,
        team_id: &str,
        creator: &Author,
        title: &str,
        brief: &str,
        assignee: Option<&str>,
    ) -> Result<Task> {
        validate("task_id", task_id, MAX_ID_BYTES)?;
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("title", title, MAX_NAME_BYTES)?;
        if brief.len() > MAX_TEXT_BYTES {
            return Err(Error::Invalid {
                field: "brief",
                max: MAX_TEXT_BYTES,
            });
        }
        if let Some(assignee) = assignee {
            validate("assignee", assignee, MAX_ID_BYTES)?;
        }
        let (now, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        let (creator_kind, creator_id) = ensure_author_in_team(&tx, team_id, creator)?;
        if let Some(assignee) = assignee
            && !is_member(&tx, team_id, assignee)?
        {
            return Err(Error::MemberNotInTeam {
                team_id: team_id.to_owned(),
                member_id: assignee.to_owned(),
            });
        }
        tx.execute(
            "INSERT INTO tasks(
                task_id, team_id, creator_kind, creator_id, title, brief,
                assignee_id, status, blocker_reason, result, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', NULL, NULL, ?8, ?8)",
            params![
                task_id,
                team_id,
                creator_kind,
                creator_id,
                title,
                brief,
                assignee,
                stamp
            ],
        )?;
        tx.commit()?;
        Ok(Task {
            task_id: task_id.to_owned(),
            team_id: team_id.to_owned(),
            creator: creator.clone(),
            title: title.to_owned(),
            brief: brief.to_owned(),
            assignee: assignee.map(str::to_owned),
            status: TaskStatus::Open,
            blocker_reason: None,
            result: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Assigns or reassigns a task without claiming it as in-progress.
    pub fn assign_task(
        &mut self,
        team_id: &str,
        task_id: &str,
        actor: &Author,
        assignee: &str,
    ) -> Result<Task> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("task_id", task_id, MAX_ID_BYTES)?;
        validate("assignee", assignee, MAX_ID_BYTES)?;
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        ensure_author_in_team(&tx, team_id, actor)?;
        if !is_member(&tx, team_id, assignee)? {
            return Err(Error::MemberNotInTeam {
                team_id: team_id.to_owned(),
                member_id: assignee.to_owned(),
            });
        }
        let current = read_task_in_tx(&tx, team_id, task_id)?;
        if matches!(current.status, TaskStatus::Done | TaskStatus::Cancelled) {
            return Err(Error::InvalidTaskTransition {
                task_id: task_id.to_owned(),
                status: current.status.as_str().to_owned(),
            });
        }
        tx.execute(
            "UPDATE tasks SET assignee_id = ?1, status = 'open', blocker_reason = NULL, updated_at = ?2
             WHERE team_id = ?3 AND task_id = ?4",
            params![assignee, stamp, team_id, task_id],
        )?;
        let updated = read_task_in_tx(&tx, team_id, task_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Atomically claims an open task that is either unassigned or already assigned to `member_id`.
    pub fn claim_task(&mut self, team_id: &str, task_id: &str, member_id: &str) -> Result<Task> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("task_id", task_id, MAX_ID_BYTES)?;
        validate("member_id", member_id, MAX_ID_BYTES)?;
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        if !is_member(&tx, team_id, member_id)? {
            return Err(Error::MemberNotInTeam {
                team_id: team_id.to_owned(),
                member_id: member_id.to_owned(),
            });
        }
        let _ = read_task_in_tx(&tx, team_id, task_id)?;
        let changed = tx.execute(
            "UPDATE tasks
             SET assignee_id = ?1, status = 'in_progress', blocker_reason = NULL, updated_at = ?2
             WHERE team_id = ?3 AND task_id = ?4 AND status = 'open'
               AND (assignee_id IS NULL OR assignee_id = ?1)",
            params![member_id, stamp, team_id, task_id],
        )?;
        if changed != 1 {
            return Err(Error::TaskUnavailable(task_id.to_owned()));
        }
        let updated = read_task_in_tx(&tx, team_id, task_id)?;
        tx.commit()?;
        Ok(updated)
    }

    pub fn block_task(
        &mut self,
        team_id: &str,
        task_id: &str,
        actor: &Author,
        reason: &str,
    ) -> Result<Task> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("task_id", task_id, MAX_ID_BYTES)?;
        validate("blocker_reason", reason, MAX_TEXT_BYTES)?;
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        ensure_author_in_team(&tx, team_id, actor)?;
        let current = read_task_in_tx(&tx, team_id, task_id)?;
        if matches!(current.status, TaskStatus::Done | TaskStatus::Cancelled) {
            return Err(Error::InvalidTaskTransition {
                task_id: task_id.to_owned(),
                status: current.status.as_str().to_owned(),
            });
        }
        tx.execute(
            "UPDATE tasks SET status = 'blocked', blocker_reason = ?1, updated_at = ?2
             WHERE team_id = ?3 AND task_id = ?4",
            params![reason, stamp, team_id, task_id],
        )?;
        let updated = read_task_in_tx(&tx, team_id, task_id)?;
        tx.commit()?;
        Ok(updated)
    }

    pub fn complete_task(
        &mut self,
        team_id: &str,
        task_id: &str,
        actor: &Author,
        result: &str,
    ) -> Result<Task> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("task_id", task_id, MAX_ID_BYTES)?;
        validate("result", result, MAX_TEXT_BYTES)?;
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        ensure_author_in_team(&tx, team_id, actor)?;
        let current = read_task_in_tx(&tx, team_id, task_id)?;
        if matches!(current.status, TaskStatus::Done | TaskStatus::Cancelled) {
            return Err(Error::InvalidTaskTransition {
                task_id: task_id.to_owned(),
                status: current.status.as_str().to_owned(),
            });
        }
        if let Author::Member { member_id } = actor {
            let supervisor_id: String = tx.query_row(
                "SELECT supervisor_id FROM teams WHERE team_id = ?1",
                [team_id],
                |row| row.get(0),
            )?;
            let is_supervisor = *member_id == supervisor_id;
            let is_assigned_worker = current.status == TaskStatus::InProgress
                && current.assignee.as_deref() == Some(member_id.as_str());
            if !is_supervisor && !is_assigned_worker {
                return Err(Error::TaskUnavailable(task_id.to_owned()));
            }
        }
        tx.execute(
            "UPDATE tasks SET status = 'done', blocker_reason = NULL, result = ?1, updated_at = ?2
             WHERE team_id = ?3 AND task_id = ?4",
            params![result, stamp, team_id, task_id],
        )?;
        let updated = read_task_in_tx(&tx, team_id, task_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Updates a task's title, brief, and optional assignee.
    pub fn update_task(
        &mut self,
        team_id: &str,
        task_id: &str,
        actor: &Author,
        title: &str,
        brief: &str,
        assignee: Option<&str>,
    ) -> Result<Task> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("task_id", task_id, MAX_ID_BYTES)?;
        validate("title", title, MAX_NAME_BYTES)?;
        if brief.len() > MAX_TEXT_BYTES {
            return Err(Error::Invalid {
                field: "brief",
                max: MAX_TEXT_BYTES,
            });
        }
        if let Some(assignee) = assignee {
            validate("assignee", assignee, MAX_ID_BYTES)?;
        }
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        ensure_author_in_team(&tx, team_id, actor)?;
        if let Some(assignee) = assignee
            && !is_member(&tx, team_id, assignee)?
        {
            return Err(Error::MemberNotInTeam {
                team_id: team_id.to_owned(),
                member_id: assignee.to_owned(),
            });
        }
        let current = read_task_in_tx(&tx, team_id, task_id)?;
        // A different assignee cannot continue another worker's in-progress or
        // blocked claim, so the task returns to the board as claimable work.
        let reset = current.assignee.as_deref() != assignee
            && matches!(current.status, TaskStatus::InProgress | TaskStatus::Blocked);
        tx.execute(
            "UPDATE tasks SET title = ?1, brief = ?2, assignee_id = ?3, updated_at = ?4,
                 status = CASE WHEN ?7 THEN 'open' ELSE status END,
                 blocker_reason = CASE WHEN ?7 THEN NULL ELSE blocker_reason END
             WHERE team_id = ?5 AND task_id = ?6",
            params![title, brief, assignee, stamp, team_id, task_id, reset],
        )?;
        let updated = read_task_in_tx(&tx, team_id, task_id)?;
        tx.commit()?;
        Ok(updated)
    }

    pub fn cancel_task(&mut self, team_id: &str, task_id: &str, actor: &Author) -> Result<Task> {
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("task_id", task_id, MAX_ID_BYTES)?;
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        ensure_author_in_team(&tx, team_id, actor)?;
        let _ = read_task_in_tx(&tx, team_id, task_id)?;
        tx.execute(
            "UPDATE tasks SET status = 'cancelled', updated_at = ?1
             WHERE team_id = ?2 AND task_id = ?3",
            params![stamp, team_id, task_id],
        )?;
        let updated = read_task_in_tx(&tx, team_id, task_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// How many tasks a team has open, and how many of those are assigned to
    /// `member_id`.
    ///
    /// A status line counts, it does not page: a team with more tasks than one
    /// page would report the first 100 and call that its open workload.
    pub fn task_counts(&self, team_id: &str, member_id: &str) -> Result<TaskCounts> {
        let mut stmt = self.prep(TASK_COUNTS_SQL)?;
        let counts = stmt.query_row(params![team_id, member_id], |row| {
            // SUM is NULL when a team has no tasks.
            let open: Option<i64> = row.get(0)?;
            let assigned: Option<i64> = row.get(1)?;
            Ok(TaskCounts {
                open: open.unwrap_or(0).max(0) as usize,
                assigned: assigned.unwrap_or(0).max(0) as usize,
            })
        })?;
        Ok(counts)
    }

    /// The board from its oldest task: the returned list is a page, and
    /// [`latest_tasks`] is the shape that reaches the newest work.
    pub fn tasks(&self, team_id: &str, limit: usize) -> Result<Vec<Task>> {
        Ok(self
            .task_page(Window::After(0), team_id, limit)?
            .into_iter()
            .map(|(_, task)| task)
            .collect())
    }

    /// The newest `limit` tasks, oldest first within the page, or the page
    /// older than `before`.
    ///
    /// A board shows its current work without reading the team's whole task
    /// history, which a forward read from sequence 0 does. Pass the first
    /// sequence already shown as `before` to page further back; that page's
    /// first sequence is the next cursor.
    pub fn latest_tasks(
        &self,
        team_id: &str,
        before: Option<i64>,
        limit: usize,
    ) -> Result<Vec<(i64, Task)>> {
        self.task_page(Window::Before(before.unwrap_or(i64::MAX)), team_id, limit)
    }

    fn task_page(&self, window: Window, team_id: &str, limit: usize) -> Result<Vec<(i64, Task)>> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE,
            });
        }
        let (sql, bound, newest_first) = match window {
            Window::After(cursor) => (TASKS_PAGE_SQL, cursor, false),
            Window::Before(bound) => (TASKS_LATEST_PAGE_SQL, bound, true),
        };
        let mut stmt = self.prep(sql)?;
        let rows = stmt.query_map(params![team_id, bound, limit as i64], |row| {
            Ok((row.get::<_, i64>(12)?, read_task_row(row)?))
        })?;
        let mut out: Vec<(i64, Task)> = Vec::new();
        for row in rows {
            let (sequence, task) = row?;
            out.push((sequence, task?));
        }
        // A DESC page is newest first; cursors and callers both speak ascending
        // sequence, the same convention `posts` and `latest_posts` use.
        if newest_first {
            out.reverse();
        }
        Ok(out)
    }
}

fn read_task_in_tx(tx: &rusqlite::Transaction<'_>, team_id: &str, task_id: &str) -> Result<Task> {
    let row = tx
        .query_row(
            "SELECT task_id, team_id, creator_kind, creator_id, title, brief,
                    assignee_id, status, blocker_reason, result, created_at, updated_at
             FROM tasks WHERE team_id = ?1 AND task_id = ?2",
            params![team_id, task_id],
            read_task_row,
        )
        .optional()?;
    match row {
        Some(task) => task,
        None => Err(Error::TaskNotFound(task_id.to_owned())),
    }
}

fn read_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Task>> {
    let task_id: String = row.get(0)?;
    let team_id: String = row.get(1)?;
    let creator_kind: String = row.get(2)?;
    let creator_id: String = row.get(3)?;
    let title: String = row.get(4)?;
    let brief: String = row.get(5)?;
    let assignee: Option<String> = row.get(6)?;
    let status_raw: String = row.get(7)?;
    let blocker_reason: Option<String> = row.get(8)?;
    let result: Option<String> = row.get(9)?;
    let created_at: String = row.get(10)?;
    let updated_at: String = row.get(11)?;

    let status = TaskStatus::parse(&status_raw).ok_or_else(|| Error::Invalid {
        field: "task status",
        max: MAX_ID_BYTES,
    });
    Ok(status.and_then(|status| {
        Ok(Task {
            task_id,
            team_id,
            creator: match creator_kind.as_str() {
                "human" => Author::Human {
                    user_id: creator_id,
                },
                _ => Author::Member {
                    member_id: creator_id,
                },
            },
            title,
            brief,
            assignee,
            status,
            blocker_reason,
            result,
            created_at: parse_time(created_at)?,
            updated_at: parse_time(updated_at)?,
        })
    }))
}
