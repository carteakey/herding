//! The one operation that reaches outside the store: importing a GitHub
//! issue shells out to `gh`, so it runs on the worker thread like every
//! other store operation, never on the UI thread.
use super::*;

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
    let heading = if number > 0 {
        format!("#{number}: {title}")
    } else {
        title.to_owned()
    };
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
    if details.len() > 8000 {
        details.truncate(details.floor_char_boundary(8000));
    }
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
