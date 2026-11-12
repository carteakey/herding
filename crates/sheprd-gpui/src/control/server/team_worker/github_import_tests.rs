//! The sync pass against a loopback GitHub: one watched repository creates
//! one bound task, a quiet second pass changes nothing, and a watched
//! reference the fake does not serve counts as failed rather than blocking
//! the rest of the pass.

#![allow(clippy::unwrap_used)]

use super::*;
use sheprd_orchestrator::team_store::{Author, Member, SessionTarget, Team, TeamStore};
use std::sync::Arc;

#[path = "../../../orchestrator/tests/fake_github.rs"]
mod fake_github;

use fake_github::FakeGithub;

fn store(dir: &tempfile::TempDir) -> TeamStore {
    let mut store = TeamStore::open(&dir.path().join("team").join("db.sqlite3")).unwrap();
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "Team".into(),
            supervisor: Member {
                member_id: "lead".into(),
                display_name: "lead".into(),
                target: SessionTarget {
                    host: "local".into(),
                    session: "s0".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p0".into()),
                },
            },
            members: Vec::new(),
        })
        .unwrap();
    let _ = Author::Member {
        member_id: "lead".into(),
    };
    store
}

fn tokens(token: &str) -> crate::github::Tokens {
    let main = crate::github::Profile::fixture("penso", Arc::new(token.to_owned().into()), &[]);
    crate::github::Tokens::new(&main, std::iter::empty())
}

#[test]
fn a_sync_pass_creates_the_watched_issue_task_and_a_quiet_pass_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);
    store
        .watch_issues(
            "t",
            &Author::Member {
                member_id: "lead".into(),
            },
            "penso/e2e-app",
            &[],
        )
        .unwrap();

    let token = format!("issue-sync-{}", std::process::id());
    let _github = FakeGithub::start(&token);
    let tokens = tokens(&token);

    let reply = github_import::sync_team_issues(&mut store, "t", &tokens).unwrap();
    let Reply::IssueSync {
        created,
        updated,
        unchanged,
        failed,
    } = reply
    else {
        panic!("expected an issue sync reply");
    };
    assert_eq!((created, updated, unchanged, failed.len()), (1, 0, 0, 0));

    // The task is bound to the issue's ref, headed by its number and title,
    // and created under the supervisor's own identity.
    let task = store
        .task_for_issue("t", "penso/e2e-app", fake_github::ISSUE_NUMBER)
        .unwrap()
        .unwrap_or_else(|| panic!("the synced issue owns a task"));
    assert!(
        task.title.contains(fake_github::ISSUE_TITLE),
        "{}",
        task.title
    );
    assert!(
        task.brief
            .contains("https://github.com/penso/e2e-app/issues/7")
    );
    assert_eq!(
        task.creator,
        Author::Member {
            member_id: "lead".into()
        }
    );

    let reply = github_import::sync_team_issues(&mut store, "t", &tokens).unwrap();
    let Reply::IssueSync {
        created,
        updated,
        unchanged,
        ..
    } = reply
    else {
        panic!("expected an issue sync reply");
    };
    assert_eq!(
        (created, updated, unchanged),
        (0, 0, 1),
        "a quiet pass writes nothing"
    );
}

#[test]
fn an_unfetched_issue_is_counted_failed_without_blocking_the_repository() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store(&dir);
    // The fake serves exactly one open issue (number 7): the explicitly
    // watched 9 is missing, the watched 7 arrives.
    store
        .watch_issues(
            "t",
            &Author::Member {
                member_id: "lead".into(),
            },
            "penso/e2e-app",
            &[7, 9],
        )
        .unwrap();

    let token = format!("issue-sync-miss-{}", std::process::id());
    let _github = FakeGithub::start(&token);
    let tokens = tokens(&token);

    let reply = github_import::sync_team_issues(&mut store, "t", &tokens).unwrap();
    let Reply::IssueSync {
        created, failed, ..
    } = reply
    else {
        panic!("expected an issue sync reply");
    };
    assert_eq!(created, 1, "issue 7 is still imported");
    assert_eq!(failed, vec!["penso/e2e-app#9".to_string()]);
    assert!(
        store
            .task_for_issue("t", "penso/e2e-app", 9)
            .unwrap()
            .is_none(),
        "a missing issue owns no task"
    );
}
