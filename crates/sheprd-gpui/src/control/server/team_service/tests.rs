#![allow(clippy::unwrap_used)]

use super::super::team_worker::Reply;
use super::*;
use sheprd_orchestrator::team_store::{Member, SessionTarget, Team};
use std::time::{Duration, Instant};

fn sample_team() -> Team {
    Team {
        team_id: "team-a".into(),
        name: "Review".into(),
        supervisor: Member {
            member_id: "supervisor".into(),
            display_name: "Supervisor".into(),
            target: SessionTarget {
                host: "local".into(),
                session: "session-a".into(),
                workspace: Some("workspace-a".into()),
                pane: Some("pane-a".into()),
            },
        },
        members: vec![],
    }
}

fn wait_snapshot(service: &Service, request_id: u64) -> Completion {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        for completion in service.poll() {
            if completion.request_id == request_id {
                return completion;
            }
        }
        assert!(Instant::now() < deadline, "team snapshot did not arrive");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn accepted_operations_publish_coalesced_snapshots_without_ui_io() {
    let directory = tempfile::tempdir().unwrap();
    let mut service =
        Service::start_at(directory.path().join("team").join("state.sqlite3")).unwrap();
    let save_id = service.submit(Operation::SaveTeam(sample_team())).unwrap();
    assert!(matches!(
        wait_snapshot(&service, save_id).result,
        Ok(Reply::Saved)
    ));

    let list_id = service
        .submit(Operation::ListTeams {
            after: None,
            limit: 20,
        })
        .unwrap();
    let snapshot = wait_snapshot(&service, list_id);
    let Ok(Reply::Teams(teams)) = snapshot.result else {
        panic!("expected team list snapshot");
    };
    assert_eq!(teams, vec![sample_team()]);
    service.finish();
}

#[test]
fn snapshot_poll_preserves_ordered_operation_completions() {
    let directory = tempfile::tempdir().unwrap();
    let mut service =
        Service::start_at(directory.path().join("team").join("state.sqlite3")).unwrap();
    let first = service.submit(Operation::SaveTeam(sample_team())).unwrap();
    let second = service
        .submit(Operation::ListTeams {
            after: None,
            limit: 10,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut completions = Vec::new();
    while completions.len() < 2 {
        completions.extend(service.poll());
        assert!(Instant::now() < deadline, "team completions did not arrive");
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        completions
            .iter()
            .map(|completion| completion.request_id)
            .collect::<Vec<_>>(),
        [first, second]
    );
    assert!(matches!(completions.remove(0).result, Ok(Reply::Saved)));
    assert!(matches!(completions.remove(0).result, Ok(Reply::Teams(_))));
    service.finish();
}
