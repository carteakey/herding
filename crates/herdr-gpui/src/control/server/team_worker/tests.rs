#![allow(clippy::unwrap_used)]
use super::*;
use herdr_orchestrator::team_store::{Author, Member, Recipients, SessionTarget};
use std::{sync::mpsc, time::Duration};

fn team() -> Team {
    let member = Member {
        member_id: "member-1".into(),
        display_name: "lead".into(),
        target: SessionTarget {
            host: "local".into(),
            session: "session-1".into(),
            workspace: Some("workspace-1".into()),
            pane: Some("pane-1".into()),
        },
    };
    let second = Member {
        member_id: "member-2".into(),
        display_name: "reviewer".into(),
        target: SessionTarget {
            host: "local".into(),
            session: "session-2".into(),
            workspace: Some("workspace-2".into()),
            pane: Some("pane-2".into()),
        },
    };
    Team {
        team_id: "team-1".into(),
        name: "review".into(),
        supervisor: member,
        members: vec![second],
    }
}

fn paused(path: PathBuf) -> (Worker, SyncSender<()>, Receiver<()>) {
    let (release, gate) = mpsc::sync_channel(1);
    let (stopped, done) = mpsc::sync_channel(1);
    let worker = Worker::spawn(path, Some(gate), Some(stopped)).unwrap();
    (worker, release, done)
}

fn watch(
    receivers: &mut Vec<Receiver<Result<Reply, Error>>>,
) -> impl FnOnce(Receiver<Result<Reply, Error>>) -> bool + '_ {
    |receiver| {
        receivers.push(receiver);
        true
    }
}

#[test]
fn queue_is_bounded_and_full_is_delivered_to_registered_watch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let (worker, release, _) = paused(path);
    let mut answers = Vec::new();
    for _ in 0..QUEUE_CAPACITY {
        worker
            .submit(
                Operation::Snapshot {
                    team_id: "team-1".into(),
                    limit: 1,
                },
                watch(&mut answers),
            )
            .unwrap();
    }
    let error = worker
        .submit(
            Operation::Snapshot {
                team_id: "team-1".into(),
                limit: 1,
            },
            watch(&mut answers),
        )
        .unwrap_err();
    assert!(matches!(error, Error::Busy));
    assert!(matches!(
        answers.last().unwrap().recv().unwrap(),
        Err(Error::Busy)
    ));
    release.send(()).unwrap();
    for answer in answers.into_iter().take(QUEUE_CAPACITY) {
        let _ = answer.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    worker.finish();
}

#[test]
fn missing_response_does_not_cancel_an_accepted_mutation_and_drop_drains_queue() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let (worker, release, stopped) = paused(path.clone());
    let mut answer = None;
    worker
        .submit(Operation::SaveTeam(team()), |receiver| {
            answer = Some(receiver);
            true
        })
        .unwrap();
    drop(answer.take());
    drop(worker);
    release.send(()).unwrap();
    stopped.recv_timeout(Duration::from_secs(3)).unwrap();

    let reopened = TeamStore::open(&path).unwrap();
    assert_eq!(reopened.team("team-1").unwrap().unwrap().name, "review");
}

#[test]
fn rejected_watch_never_queues_a_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let (worker, release, stopped) = paused(path.clone());
    assert!(matches!(
        worker.submit(Operation::SaveTeam(team()), |_| false),
        Err(Error::WatchUnavailable)
    ));
    drop(worker);
    release.send(()).unwrap();
    stopped.recv_timeout(Duration::from_secs(3)).unwrap();
    let store = TeamStore::open(&path).unwrap();
    assert!(store.team("team-1").unwrap().is_none());
}

#[test]
fn broadcast_is_resolved_by_the_store_and_excludes_member_author() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let worker = Worker::start(path).unwrap();
    let mut saved = None;
    worker
        .submit(Operation::SaveTeam(team()), |receiver| {
            saved = Some(receiver);
            true
        })
        .unwrap();
    assert!(matches!(
        saved
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        Ok(Reply::Saved)
    ));

    let mut posted = None;
    worker
        .submit(
            Operation::AddPost {
                post_id: "post-1".into(),
                team_id: "team-1".into(),
                author: Author::Member {
                    member_id: "member-1".into(),
                },
                reply_to: None,
                body: "status".into(),
                recipients: Recipients::All,
            },
            |receiver| {
                posted = Some(receiver);
                true
            },
        )
        .unwrap();
    let Ok(Reply::Saved) = posted
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    else {
        panic!("expected posted entry");
    };
    worker.finish();
}
