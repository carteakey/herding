//! The one operation that reaches outside the store: importing a GitHub
//! issue shells out to `gh`, so it runs on the worker thread like every
//! other store operation, never on the UI thread.
use super::*;
use sheprd_orchestrator::team_store::{
    ISSUE_DETAILS_LIMIT, IssueSnapshot, IssueSyncKind, IssueWatch, REPO_WATCH,
};

pub(super) fn import_github_issue(
    store: &mut TeamStore,
    team_id: &str,
    task_id: &str,
    creator: &Author,
    issue_ref: &str,
    as_goal: bool,
) -> Result<(), Error> {
    let raw = issue_ref.trim();
    if raw.is_empty() {
        return Err(Error::External("Issue reference must not be empty".into()));
    }
    let mut cmd = sheprd_client::new_command("gh");
    cmd.arg("issue").arg("view");
    if let Some((repo, num)) = raw.split_once('#')
        && !repo.trim().is_empty()
    {
        cmd.arg(num.trim()).arg("--repo").arg(repo.trim());
    } else {
        cmd.arg(raw.trim_start_matches('#'));
    }
    cmd.args(["--json", "number,title,body,url"]);
    let output = cmd
        .output()
        .map_err(|e| Error::External(format!("Failed to run `gh issue view`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(Error::External(if stderr.is_empty() {
            "gh issue view failed".into()
        } else {
            stderr
        }));
    }
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| Error::External(format!("Invalid JSON from `gh issue view`: {e}")))?;
    let number = parsed["number"].as_u64().unwrap_or(0);
    let title = parsed["title"].as_str().unwrap_or("Issue").trim();
    let body = parsed["body"].as_str().unwrap_or("").trim();
    let url = parsed["url"].as_str().unwrap_or("").trim();
    let heading = issue_heading(number, title);
    let details = issue_details(url, body);
    if as_goal {
        let goal = if details.is_empty() {
            heading
        } else {
            format!("{heading} — {details}")
        };
        store.set_team_goal(team_id, &goal)?;
    } else {
        store.create_task(task_id, team_id, creator, &heading, &details, None)?;
    }
    Ok(())
}

/// The task heading one issue renders as: its number and title, or the bare
/// title when the API could not say the number.
fn issue_heading(number: u64, title: &str) -> String {
    if number > 0 {
        format!("#{number}: {title}")
    } else {
        title.to_owned()
    }
}

/// The task details one issue renders as: its URL, then the body, truncated
/// at the store's issue limit so a long issue never exceeds the board.
fn issue_details(url: &str, body: &str) -> String {
    let mut details = String::new();
    if !url.is_empty() {
        details.push_str(url);
    }
    if !body.is_empty() {
        if !details.is_empty() {
            details.push_str("\n\n");
        }
        details.push_str(body);
    }
    let limit = ISSUE_DETAILS_LIMIT;
    if details.len() > limit {
        details.truncate(details.floor_char_boundary(limit));
    }
    details
}

/// How long one sync's GraphQL request may take before the pass fails and
/// the next one retries.
const SYNC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The issues one sync pass fetched, ready for the store's upsert.
struct FetchedIssue {
    snapshot: IssueSnapshot,
    heading: String,
    details: String,
}

/// Fetches one repository's watched issues over the GraphQL API: the whole
/// repository when `numbers` is empty, exactly those issues otherwise. A
/// repository that cannot be read at all is an error; an explicitly watched
/// issue that cannot be fetched is named in the returned failures and left
/// as the last pass left it.
fn fetch_issues(
    repo: &str,
    numbers: &[i64],
    tokens: &crate::github::Tokens,
) -> Result<(Vec<FetchedIssue>, Vec<String>), Error> {
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(|| Error::External(format!("Invalid repository reference {repo:?}")))?;
    let token = tokens.for_owner(owner);
    let mut cooldown: Option<std::time::Duration> = None;
    let fields = "number title body state url";
    if numbers.is_empty() {
        let query = format!(
            "query($owner: String!, $repo: String!) {{
                repository(owner: $owner, name: $repo) {{
                    issues(first: 100, states: [OPEN, CLOSED],
                           orderBy: {{field: UPDATED_AT, direction: DESC}}) {{
                        nodes {{ {fields} }}
                    }}
                }}
            }}"
        );
        let response = sheprd_github::graphql(
            "team-issue-sync",
            token,
            &query,
            serde_json::json!({"owner": owner, "repo": name}),
            SYNC_TIMEOUT,
            || false,
            &mut cooldown,
        )
        .map_err(|error| Error::External(format!("GitHub sync for {repo} failed: {error}")))?;
        let nodes = response["data"]["repository"]["issues"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let fetched = nodes
            .iter()
            .filter_map(|node| issue_from_node(repo, node))
            .collect();
        Ok((fetched, Vec::new()))
    } else {
        // One query per watched issue: a deleted or inaccessible issue fails
        // alone, so one stale reference cannot stall the repository's
        // remaining watches.
        let mut fetched = Vec::new();
        let mut failed = Vec::new();
        for number in numbers {
            let query = format!(
                "query($owner: String!, $repo: String!) {{
                    repository(owner: $owner, name: $repo) {{
                        issue(number: {number}) {{ {fields} }}
                    }}
                }}"
            );
            let response = sheprd_github::graphql(
                "team-issue-sync",
                token,
                &query,
                serde_json::json!({"owner": owner, "repo": name}),
                SYNC_TIMEOUT,
                || false,
                &mut cooldown,
            );
            match response {
                Ok(response) => {
                    let node = &response["data"]["repository"]["issue"];
                    match node
                        .is_object()
                        .then(|| issue_from_node(repo, node))
                        .flatten()
                    {
                        Some(issue) => fetched.push(issue),
                        None => {
                            tracing::warn!(repo = %repo, number,
                                "GitHub issue sync read no usable issue for the watched reference");
                            failed.push(format!("{repo}#{number}"));
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, repo = %repo, number, "GitHub issue sync could not fetch the watched issue");
                    failed.push(format!("{repo}#{number}"));
                }
            }
        }
        Ok((fetched, failed))
    }
}

/// Shapes one GraphQL issue node into the store's snapshot plus the task
/// heading and details it renders as. A node the API half-answered is
/// skipped rather than stored half-true.
fn issue_from_node(repo: &str, node: &serde_json::Value) -> Option<FetchedIssue> {
    let number = node["number"].as_i64()?;
    let title = node["title"].as_str()?.trim();
    let body = node["body"].as_str().unwrap_or("").trim();
    let url = node["url"].as_str()?.trim();
    let state = node["state"].as_str()?.trim();
    if state != "OPEN" && state != "CLOSED" {
        return None;
    }
    let snapshot = IssueSnapshot {
        repo: repo.to_owned(),
        number,
        url: url.to_owned(),
        title: title.to_owned(),
        body: body.to_owned(),
        state: state.to_owned(),
    };
    let heading = issue_heading(number.max(0) as u64, title);
    let details = issue_details(url, body);
    Some(FetchedIssue {
        snapshot,
        heading,
        details,
    })
}

/// One sync pass over a team's watch set: fetches every watched repository
/// and issue with the caller's GitHub token, then lets the store apply the
/// lifecycle each issue's state maps to. The supervisor owns the tasks the
/// sync creates; a deleted issue or unreadable repository is counted as a
/// failure and left exactly as the last pass left it.
pub(super) fn sync_team_issues(
    store: &mut TeamStore,
    team_id: &str,
    tokens: &crate::github::Tokens,
) -> Result<Reply, Error> {
    let watches = store.issue_watches(team_id)?;
    let supervisor = store
        .team(team_id)?
        .ok_or(Error::NotMember)?
        .supervisor
        .member_id;
    let creator = Author::Member {
        member_id: supervisor,
    };
    let mut created = 0usize;
    let mut updated = 0usize;
    let mut unchanged = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for repo in watches
        .iter()
        .map(|IssueWatch { repo, .. }| repo.clone())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let numbers: Vec<i64> = watches
            .iter()
            .filter(|watch| watch.repo == repo && watch.number != REPO_WATCH)
            .map(|watch| watch.number)
            .collect();
        let (fetched, missed) = match fetch_issues(&repo, &numbers, tokens) {
            Ok((fetched, missed)) => (fetched, missed),
            Err(error) => {
                failed.push(repo.clone());
                tracing::warn!(%error, repo = %repo, "GitHub issue sync could not read the repository");
                continue;
            }
        };
        failed.extend(missed);
        for issue in fetched {
            let FetchedIssue {
                snapshot,
                heading,
                details,
            } = issue;
            let task_id = uuid::Uuid::new_v4().to_string();
            match store
                .upsert_issue_task(team_id, &creator, &task_id, &snapshot, &heading, &details)
            {
                Ok((_, IssueSyncKind::Created)) => created += 1,
                Ok((_, IssueSyncKind::Unchanged)) => unchanged += 1,
                Ok(_) => updated += 1,
                Err(error) => {
                    failed.push(format!("{}#{}", snapshot.repo, snapshot.number));
                    tracing::warn!(%error, repo = %snapshot.repo, number = snapshot.number,
                        "GitHub issue sync could not update the task");
                }
            }
        }
    }
    Ok(Reply::IssueSync {
        created,
        updated,
        unchanged,
        failed,
    })
}
