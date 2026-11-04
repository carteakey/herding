//! Durable team findings, evidence references, confidence levels, and non-destructive supersession.

use super::{
    Author, Error, MAX_ID_BYTES, MAX_PAGE_SIZE, MAX_TEXT_BYTES, Result, TeamStore,
    ensure_author_in_team, now_stamp, parse_time, validate,
};
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub finding_id: String,
    pub team_id: String,
    pub author: Author,
    pub task_id: Option<String>,
    pub description: String,
    pub evidence: String,
    pub confidence: Confidence,
    pub supersedes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl TeamStore {
    #[allow(clippy::too_many_arguments)]
    pub fn add_finding(
        &mut self,
        finding_id: &str,
        team_id: &str,
        author: &Author,
        task_id: Option<&str>,
        description: &str,
        evidence: &str,
        confidence: Confidence,
        supersedes: Option<&str>,
    ) -> Result<Finding> {
        validate("finding_id", finding_id, MAX_ID_BYTES)?;
        validate("team_id", team_id, MAX_ID_BYTES)?;
        validate("description", description, MAX_TEXT_BYTES)?;
        validate("evidence", evidence, MAX_TEXT_BYTES)?;
        if let Some(task_id) = task_id {
            validate("task_id", task_id, MAX_ID_BYTES)?;
        }
        if let Some(supersedes) = supersedes {
            validate("supersedes", supersedes, MAX_ID_BYTES)?;
        }
        let (now, stamp) = now_stamp()?;
        let tx = self.db.transaction()?;
        let (author_kind, author_id) = ensure_author_in_team(&tx, team_id, author)?;
        if let Some(task_id) = task_id {
            let task_team: Option<String> = tx
                .query_row(
                    "SELECT team_id FROM tasks WHERE task_id = ?1",
                    [task_id],
                    |row| row.get(0),
                )
                .optional()?;
            if task_team.as_deref() != Some(team_id) {
                return Err(Error::TaskNotFound(task_id.to_owned()));
            }
        }
        if let Some(prior) = supersedes {
            let prior_team: Option<String> = tx
                .query_row(
                    "SELECT team_id FROM findings WHERE finding_id = ?1",
                    [prior],
                    |row| row.get(0),
                )
                .optional()?;
            if prior_team.as_deref() != Some(team_id) {
                return Err(Error::FindingNotFound(prior.to_owned()));
            }
        }
        tx.execute(
            "INSERT INTO findings(
                finding_id, team_id, author_kind, author_id, task_id,
                description, evidence, confidence, supersedes, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                finding_id,
                team_id,
                author_kind,
                author_id,
                task_id,
                description,
                evidence,
                confidence.as_str(),
                supersedes,
                stamp,
            ],
        )?;
        tx.commit()?;
        Ok(Finding {
            finding_id: finding_id.to_owned(),
            team_id: team_id.to_owned(),
            author: author.clone(),
            task_id: task_id.map(str::to_owned),
            description: description.to_owned(),
            evidence: evidence.to_owned(),
            confidence,
            supersedes: supersedes.map(str::to_owned),
            created_at: now,
        })
    }

    pub fn findings(&self, team_id: &str, limit: usize) -> Result<Vec<Finding>> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(Error::Invalid {
                field: "page size",
                max: MAX_PAGE_SIZE,
            });
        }
        let mut stmt = self.prep(
            "SELECT finding_id, team_id, author_kind, author_id, task_id,
                    description, evidence, confidence, supersedes, created_at
             FROM findings WHERE team_id = ?1 ORDER BY sequence LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![team_id, limit as i64], read_finding_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }
}

fn read_finding_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Finding>> {
    let finding_id: String = row.get(0)?;
    let team_id: String = row.get(1)?;
    let author_kind: String = row.get(2)?;
    let author_id: String = row.get(3)?;
    let task_id: Option<String> = row.get(4)?;
    let description: String = row.get(5)?;
    let evidence: String = row.get(6)?;
    let confidence_raw: String = row.get(7)?;
    let supersedes: Option<String> = row.get(8)?;
    let created_at: String = row.get(9)?;

    let confidence = Confidence::parse(&confidence_raw).ok_or_else(|| Error::Invalid {
        field: "confidence",
        max: MAX_ID_BYTES,
    });
    Ok(confidence.and_then(|confidence| {
        Ok(Finding {
            finding_id,
            team_id,
            author: match author_kind.as_str() {
                "human" => Author::Human { user_id: author_id },
                _ => Author::Member {
                    member_id: author_id,
                },
            },
            task_id,
            description,
            evidence,
            confidence,
            supersedes,
            created_at: parse_time(created_at)?,
        })
    }))
}
