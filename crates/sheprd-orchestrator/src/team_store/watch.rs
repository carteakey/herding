//! The per-team watch set the GitHub issue sync follows: whole repositories
//! (number [`REPO_WATCH`]) or explicit issues. Rows are bounded so a team
//! cannot promise unbounded syncing, and editing them is a member action
//! like any other board write.

use super::{
    Author, Error, MAX_ID_BYTES, Result, TeamStore, ensure_author_in_team, now_stamp, validate,
};
use rusqlite::params;

/// One watched GitHub issue syncs only itself; the [`REPO_WATCH`] number
/// watches every issue a repository has.
pub const REPO_WATCH: i64 = 0;

/// Watch rows a single team may hold: enough for the fleet's trackers and a
/// handful of repos, few enough that one sync pass stays one small query per
/// repository.
pub const MAX_WATCHES: usize = 8;

/// A watched repository, or one watched issue inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueWatch {
    /// `owner/repo` as GitHub addresses it.
    pub repo: String,
    /// The watched issue, or [`REPO_WATCH`] for the whole repository.
    pub number: i64,
}

/// The `owner/repo` slug a watch or sync addresses, validated here so a
/// typo becomes a refusal at watch time instead of a GraphQL error at sync
/// time.
fn validate_repo(repo: &str) -> Result<()> {
    validate("repo", repo, MAX_ID_BYTES)?;
    let slug = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
    };
    match repo.split_once('/') {
        Some((owner, name)) if slug(owner) && slug(name) => Ok(()),
        _ => Err(Error::Invalid {
            field: "repo",
            max: MAX_ID_BYTES,
        }),
    }
}

impl TeamStore {
    /// Adds watches: an empty `numbers` watches the whole repository, any
    /// numbers watch exactly those issues. Returns the team's full watch
    /// set, sorted, so the caller prints what holds now. Idempotent.
    pub fn watch_issues(
        &mut self,
        team_id: &str,
        actor: &Author,
        repo: &str,
        numbers: &[i64],
    ) -> Result<Vec<IssueWatch>> {
        validate_repo(repo)?;
        for number in numbers {
            if *number < 1 {
                return Err(Error::Invalid {
                    field: "issue number",
                    max: i64::MAX as usize,
                });
            }
        }
        let (_, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        let (kind, id) = ensure_author_in_team(&tx, team_id, actor)?;
        let watched: i64 = tx
            .prepare_cached("SELECT COUNT(*) FROM team_issue_watch WHERE team_id = ?1")?
            .query_row(params![team_id], |row| row.get(0))?;
        // Only rows that do not hold yet count against the bound: re-watching
        // what the team already watches is idempotent, cap or no cap.
        let rows: Vec<i64> = if numbers.is_empty() {
            vec![REPO_WATCH]
        } else {
            numbers.to_vec()
        };
        let mut present = tx.prepare_cached(
            "SELECT COUNT(*) FROM team_issue_watch WHERE team_id = ?1 AND repo = ?2 AND number = ?3",
        )?;
        let mut additions = 0i64;
        for number in &rows {
            let held: i64 = present.query_row(params![team_id, repo, number], |row| row.get(0))?;
            if held == 0 {
                additions += 1;
            }
        }
        drop(present);
        if watched + additions > MAX_WATCHES as i64 {
            return Err(Error::Invalid {
                field: "watch set",
                max: MAX_WATCHES,
            });
        }
        let mut insert = tx.prepare_cached(
            "INSERT OR IGNORE INTO team_issue_watch
                (team_id, repo, number, added_by_kind, added_by_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for number in rows {
            insert.execute(params![team_id, repo, number, kind, id, stamp])?;
        }
        drop(insert);
        tx.commit()?;
        self.issue_watches(team_id)
    }

    /// Removes watches: an empty `numbers` forgets the repository and every
    /// issue watched inside it, any numbers forget exactly those rows.
    /// Removing an absent watch succeeds; returns the watch set that holds
    /// now.
    pub fn unwatch_issues(
        &mut self,
        team_id: &str,
        repo: &str,
        numbers: &[i64],
    ) -> Result<Vec<IssueWatch>> {
        validate_repo(repo)?;
        let tx = self.db.transaction()?;
        if numbers.is_empty() {
            tx.execute(
                "DELETE FROM team_issue_watch WHERE team_id = ?1 AND repo = ?2",
                params![team_id, repo],
            )?;
        } else {
            let mut delete = tx.prepare_cached(
                "DELETE FROM team_issue_watch WHERE team_id = ?1 AND repo = ?2 AND number = ?3",
            )?;
            for number in numbers {
                delete.execute(params![team_id, repo, number])?;
            }
        }
        tx.commit()?;
        self.issue_watches(team_id)
    }

    /// The team's watch set, repositories first then ascending issues, so a
    /// sync pass reads repositories in a stable order.
    pub fn issue_watches(&self, team_id: &str) -> Result<Vec<IssueWatch>> {
        let mut statement = self.prep(
            "SELECT repo, number FROM team_issue_watch WHERE team_id = ?1
             ORDER BY number = 0 DESC, repo, number",
        )?;
        let rows = statement
            .query_map(params![team_id], |row| {
                Ok(IssueWatch {
                    repo: row.get(0)?,
                    number: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
